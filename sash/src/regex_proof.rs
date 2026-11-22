//! DD132: immutable regex witnesses and lexical caught-scratch read permissions.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::diagnostic::Diagnostic;
use crate::ir::HandlerKind;
use crate::ssa::*;
use crate::terminal::{live_insts, live_successors, origin_sites};

type Scopes = BTreeSet<usize>;
type Certificates = BTreeSet<Scopes>;

#[derive(Clone, Debug, PartialEq)]
enum Selection {
    Scalar,
    Cell {
        index: SsaValue,
        reads: BTreeSet<String>,
        integer: bool,
    },
    Values,
    Keys,
}

#[derive(Clone, Debug, PartialEq)]
struct Place {
    name: String,
    selection: Selection,
}

impl Place {
    fn scalar(name: &str) -> Self {
        Self {
            name: name.to_string(),
            selection: Selection::Scalar,
        }
    }

    fn cell(name: &str, index: &SsaValue, integer: bool) -> Self {
        let mut index = index.clone();
        let mut reads = BTreeSet::new();
        index.map_variables(&mut |var| {
            reads.insert(var.name.clone());
            var.version = 0;
        });
        Self {
            name: name.to_string(),
            selection: Selection::Cell {
                index,
                reads,
                integer,
            },
        }
    }

    fn collection(name: &str, keys: bool) -> Self {
        Self {
            name: name.to_string(),
            selection: if keys {
                Selection::Keys
            } else {
                Selection::Values
            },
        }
    }

    fn reads(&self, name: &str) -> bool {
        self.name == name
            || matches!(&self.selection, Selection::Cell { reads, .. } if reads.contains(name))
    }

    fn stable(&self) -> bool {
        self.name != "?"
            && match &self.selection {
                Selection::Cell { index, reads, .. } => !reads.contains("?") && stable_index(index),
                Selection::Scalar | Selection::Values | Selection::Keys => true,
            }
    }

    fn renamed(&self, name: &str) -> Self {
        Self {
            name: name.to_string(),
            selection: self.selection.clone(),
        }
    }
}

fn unconditional() -> Certificates {
    BTreeSet::from([Scopes::new()])
}

fn alternatives(mut proofs: Certificates) -> Certificates {
    let all = proofs.clone();
    proofs.retain(|p| !all.iter().any(|q| q != p && q.is_subset(p)));
    proofs
}

fn require_both(a: &Certificates, b: &Certificates) -> Certificates {
    alternatives(
        a.iter()
            .flat_map(|x| b.iter().map(move |y| x.union(y).copied().collect()))
            .collect(),
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Permission {
    Within(Scopes),
    Blocked,
}

impl Permission {
    fn expire(&mut self, program: &SsaProgram, origin: Option<usize>) {
        if let Self::Within(scopes) = self {
            if !scopes.iter().all(|scope| within(program, origin, *scope)) {
                *self = Self::Blocked;
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Facts {
    proofs: Vec<(Place, Certificates)>,
    equal: Vec<(Place, Place)>,
    present: Vec<Place>,
    sizes: BTreeMap<String, usize>,
    permissions: BTreeMap<String, Permission>,
}

impl Facts {
    fn raw(&self, place: &Place) -> Certificates {
        self.proofs
            .iter()
            .find(|(p, _)| p == place)
            .map(|(_, proofs)| proofs.clone())
            .unwrap_or_default()
    }

    fn get(&self, place: &Place) -> Certificates {
        self.limit(&place.name, self.raw(place))
    }

    fn limit(&self, name: &str, proofs: Certificates) -> Certificates {
        match self.permissions.get(name) {
            None => proofs,
            Some(Permission::Blocked) => Certificates::new(),
            Some(Permission::Within(scopes)) => {
                require_both(&proofs, &BTreeSet::from([scopes.clone()]))
            }
        }
    }

    fn add(&mut self, place: Place, proofs: Certificates) {
        if proofs.is_empty() || !place.stable() {
            return;
        }
        if let Some((_, old)) = self.proofs.iter_mut().find(|(p, _)| p == &place) {
            old.extend(proofs);
            *old = alternatives(std::mem::take(old));
        } else {
            self.proofs.push((place, alternatives(proofs)));
        }
    }

    fn equivalents(&self, place: &Place) -> Vec<Place> {
        let mut result = vec![place.clone()];
        for (a, b) in &self.equal {
            if a == place {
                result.push(b.clone());
            } else if b == place {
                result.push(a.clone());
            }
        }
        result
    }

    fn alias(&mut self, target: &Place, source: &Place) {
        if !target.stable() || !source.stable() {
            return;
        }
        let mut places = self.equivalents(source);
        places.retain(|place| !self.same(target, place));
        self.equal
            .extend(places.into_iter().map(|place| (target.clone(), place)));
    }

    fn same(&self, a: &Place, b: &Place) -> bool {
        a == b
            || self
                .equal
                .iter()
                .any(|(x, y)| (a == x && b == y) || (a == y && b == x))
    }

    fn kill(&mut self, name: &str) {
        self.proofs.retain(|(place, _)| !place.reads(name));
        self.equal.retain(|(a, b)| !a.reads(name) && !b.reads(name));
        self.present.retain(|place| !place.reads(name));
        self.sizes.remove(name);
        self.permissions.remove(name);
    }

    fn at_origin(&mut self, program: &SsaProgram, origin: Option<usize>) {
        for (_, proofs) in &mut self.proofs {
            proofs.retain(|scopes| scopes.iter().all(|scope| within(program, origin, *scope)));
        }
        self.proofs.retain(|(_, proofs)| !proofs.is_empty());
        for permission in self.permissions.values_mut() {
            permission.expire(program, origin);
        }
    }

    fn meet(&self, other: &Self) -> Self {
        let mut result = self.clone();
        result.proofs.clear();
        for (place, proofs) in &self.proofs {
            result.add(place.clone(), require_both(proofs, &other.raw(place)));
        }
        result.equal.retain(|(a, b)| other.same(a, b));
        result.present.retain(|p| other.present.contains(p));
        result
            .sizes
            .retain(|name, size| other.sizes.get(name) == Some(size));
        for (name, permission) in &other.permissions {
            result.merge_permission(name, permission);
        }
        result
    }

    fn merge_permission(&mut self, name: &str, incoming: &Permission) {
        let value = match (self.permissions.get(name), incoming) {
            (Some(Permission::Blocked), _) | (_, Permission::Blocked) => Permission::Blocked,
            (Some(Permission::Within(a)), Permission::Within(b)) => {
                Permission::Within(a.union(b).copied().collect())
            }
            (None, permission) => permission.clone(),
        };
        self.permissions.insert(name.to_string(), value);
    }
}

fn within(program: &SsaProgram, mut origin: Option<usize>, scope: usize) -> bool {
    while let Some(id) = origin {
        if id == scope {
            return true;
        }
        origin = program.origins[id].parent;
    }
    false
}

#[derive(Clone, Debug, PartialEq)]
struct Flow {
    facts: Facts,
    outcomes: [Option<Facts>; 2],
    signals: crate::signals::PendingSignals,
}

impl Flow {
    fn new(facts: Facts) -> Self {
        Self {
            outcomes: [Some(facts.clone()), Some(facts.clone())],
            facts,
            signals: crate::signals::PendingSignals::default(),
        }
    }

    fn reset(&mut self, truth: Option<bool>) {
        self.outcomes = [Some(self.facts.clone()), Some(self.facts.clone())];
        if let Some(truth) = truth {
            self.outcomes[usize::from(!truth)] = None;
        }
    }

    fn map(&mut self, mut f: impl FnMut(&mut Facts)) {
        f(&mut self.facts);
        for facts in self.outcomes.iter_mut().flatten() {
            f(facts);
        }
    }

    fn meet(&self, other: &Self) -> Self {
        let mut signals = self.signals.clone();
        signals.extend(&other.signals);
        Self {
            facts: self.facts.meet(&other.facts),
            outcomes: std::array::from_fn(|i| meet_optional(&self.outcomes[i], &other.outcomes[i])),
            signals,
        }
    }

    fn edge(&self, truth: Option<bool>) -> Option<Self> {
        let Some(truth) = truth else {
            return Some(self.clone());
        };
        let facts = self.outcomes[usize::from(truth)].clone()?;
        let mut result = Self::new(facts);
        result.signals = self.signals.clone();
        result.reset(Some(truth));
        Some(result)
    }
}

fn meet_optional(a: &Option<Facts>, b: &Option<Facts>) -> Option<Facts> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.meet(b)),
        (None, facts) | (facts, None) => facts.clone(),
    }
}

struct Checker<'a> {
    program: &'a SsaProgram,
    inputs: BTreeMap<BlockId, Flow>,
    pending: VecDeque<BlockId>,
    diagnostics: Vec<Diagnostic>,
    reporting: bool,
    check_reads: bool,
    main_blocks: BTreeSet<BlockId>,
    exit_blocks: BTreeSet<BlockId>,
}

pub(crate) fn check(program: &SsaProgram, check_reads: bool) -> Vec<Diagnostic> {
    let mut checker = Checker {
        program,
        inputs: BTreeMap::new(),
        pending: VecDeque::new(),
        diagnostics: Vec::new(),
        reporting: false,
        check_reads,
        main_blocks: program
            .process_blocks(&[program.entry])
            .into_iter()
            .collect(),
        exit_blocks: program
            .handlers
            .get(&HandlerKind::Exit)
            .map(|handler| {
                program
                    .process_blocks(&[handler.block])
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default(),
    };
    for root in program.roots() {
        checker.enqueue(root, Flow::new(Facts::default()));
    }
    while let Some(id) = checker.pending.pop_front() {
        checker.block(id, true);
    }
    checker.reporting = true;
    for id in checker.inputs.keys().copied().collect::<Vec<_>>() {
        checker.block(id, false);
    }
    checker.diagnostics
}

impl Checker<'_> {
    fn immutable(&self, name: &str) -> bool {
        self.program.immutable_bindings.contains(name)
            || !self.program.source_bindings.contains(name)
    }

    fn enqueue(&mut self, id: BlockId, flow: Flow) {
        let next = match self.inputs.get(&id) {
            Some(old) => old.meet(&flow),
            None => flow,
        };
        if self.inputs.get(&id) != Some(&next) {
            self.inputs.insert(id, next);
            self.pending.push_back(id);
        }
    }

    fn block(&mut self, id: BlockId, propagate: bool) {
        let block = self.program.block(id);
        let mut flow = self.inputs[&id].clone();
        let mut origin = block.origin;
        flow.map(|facts| facts.at_origin(self.program, origin));
        let mut failure = None;
        for (index, inst) in live_insts(block).iter().enumerate() {
            if block.checked_failure_at(index).is_some() {
                failure = Some(flow.clone());
            }
            self.instruction_at(block, index, inst, &mut origin, &mut flow, propagate);
        }
        if propagate {
            self.block_completion(block, &flow.facts);
            self.successors(block, &flow, failure);
        }
    }

    fn instruction_at(
        &mut self,
        block: &BasicBlock,
        index: usize,
        inst: &SsaInst,
        origin: &mut Option<usize>,
        flow: &mut Flow,
        propagate: bool,
    ) {
        if let SsaInst::SourceOrigin(next, _) = inst {
            *origin = *next;
            flow.map(|facts| facts.at_origin(self.program, *origin));
        }
        let deferred = block.signal_deferred.contains(&index);
        if propagate {
            self.poll(block.id, inst, deferred, flow, true);
        }
        self.pipeline_children(inst, flow, propagate);
        self.instruction(inst, *origin, flow);
        if propagate {
            self.poll(block.id, inst, deferred, flow, false);
            self.terminal_inputs(block.id, inst, &flow.facts);
        }
    }

    fn poll(&mut self, id: BlockId, inst: &SsaInst, deferred: bool, flow: &mut Flow, before: bool) {
        if !self.main_blocks.contains(&id) {
            return;
        }
        let dispatch = if before {
            flow.signals.before(inst, deferred)
        } else {
            flow.signals.after(inst, deferred)
        };
        if dispatch {
            self.handler_input(HandlerKind::Exit, &flow.facts);
        }
    }

    fn terminal_inputs(&mut self, id: BlockId, inst: &SsaInst, facts: &Facts) {
        let main = self.main_blocks.contains(&id);
        if main
            && matches!(
                inst,
                SsaInst::Command {
                    kind: BuiltinCmd::Exit,
                    ..
                }
            )
        {
            self.handler_input(HandlerKind::Exit, facts);
        }
        if main || self.exit_blocks.contains(&id) {
            self.handler_input(HandlerKind::Err, facts);
        }
    }

    fn block_completion(&mut self, block: &BasicBlock, facts: &Facts) {
        if self.main_blocks.contains(&block.id)
            && matches!(block.terminator, Terminator::Return)
            && !live_insts(block).iter().any(crate::terminal::is_exit)
        {
            self.handler_input(HandlerKind::Exit, facts);
        }
    }

    fn handler_input(&mut self, kind: HandlerKind, facts: &Facts) {
        let Some(handler) = self.program.handlers.get(&kind) else {
            return;
        };
        let mut incoming = Facts::default();
        for name in facts.permissions.keys() {
            if self.program.globals.contains_key(name) {
                incoming
                    .permissions
                    .insert(name.clone(), Permission::Blocked);
            }
        }
        self.enqueue(handler.block, Flow::new(incoming));
    }

    fn pipeline_children(&mut self, inst: &SsaInst, flow: &Flow, propagate: bool) {
        if !propagate {
            return;
        }
        if let SsaInst::Pipeline { stages, .. } = inst {
            for entry in stages
                .iter()
                .filter_map(|stage| self.program.compound_stages.entry(&stage.0))
            {
                self.enqueue(entry, Flow::new(flow.facts.clone()));
            }
        }
    }

    fn successors(&mut self, block: &BasicBlock, flow: &Flow, failure: Option<Flow>) {
        if self.checked_successors(block, flow, failure) {
            return;
        }
        let successors = live_successors(block);
        for next in successors.iter().copied().collect::<BTreeSet<_>>() {
            self.successor(block, next, flow);
        }
        if successors.is_empty() {
            return;
        }
        if let Some(child) = block.terminator.forked_child() {
            self.enqueue(child, Flow::new(flow.facts.clone()));
        }
    }

    fn checked_successors(
        &mut self,
        block: &BasicBlock,
        flow: &Flow,
        failure: Option<Flow>,
    ) -> bool {
        let Terminator::Checked { normal, failed, .. } = block.terminator else {
            return false;
        };
        self.enqueue(
            failed,
            failure.expect("checked block has a failure operation"),
        );
        if let Some(normal) = normal {
            self.enqueue(normal, flow.clone());
        }
        true
    }

    fn successor(&mut self, block: &BasicBlock, next: BlockId, flow: &Flow) {
        for truth in block.terminator.edge_outcomes(next) {
            if let Some(flow) = flow.edge(truth) {
                self.enqueue(next, flow);
            }
        }
    }

    fn instruction(&mut self, inst: &SsaInst, origin: Option<usize>, flow: &mut Flow) {
        if self.reporting && self.check_reads && !flow.facts.permissions.is_empty() {
            self.instruction_reads(inst, origin, &flow.facts);
        }
        self.instruction_patterns(inst, origin, &flow.facts);
        match inst {
            SsaInst::Test(test, test_origin) => {
                self.test_instruction(test, *test_origin, origin, flow);
            }
            SsaInst::Attest { source, test, .. } => {
                flow.facts.permissions.remove(&source.name);
                flow.facts = self.test(test, origin, &flow.facts)[1].clone();
                flow.reset(None);
            }
            SsaInst::AttestAll {
                source,
                binders,
                test,
                is_map,
                if_form,
            } => {
                self.attest_all(source, binders, test, *is_map, *if_form, origin, flow);
            }
            SsaInst::Command {
                kind: BuiltinCmd::NegateTestResult,
                ..
            } => {
                flow.outcomes.swap(0, 1);
            }
            SsaInst::Command {
                kind: BuiltinCmd::SetTestFromStatus,
                ..
            } => {}
            _ => self.ordinary_instruction(inst, flow),
        }
    }

    fn ordinary_instruction(&self, inst: &SsaInst, flow: &mut Flow) {
        flow.map(|facts| self.write(inst, facts));
        match inst {
            SsaInst::AttemptEnd { failed: true } => flow.reset(Some(false)),
            SsaInst::Assign { target, value } if target.name == "?" => flow.reset(
                value
                    .literal_text()
                    .and_then(|s| s.parse::<i64>().ok())
                    .map(|n| n == 0),
            ),
            SsaInst::Assign { value, .. } if !value.child_values().is_empty() => {
                flow.reset(None);
            }
            _ if sets_condition_status(inst) => flow.reset(None),
            _ => {}
        }
    }

    fn test_instruction(
        &mut self,
        test: &SsaTest,
        test_origin: TestOrigin,
        origin: Option<usize>,
        flow: &mut Flow,
    ) {
        let mut input = flow.facts.clone();
        let caught = self.caught(test_origin == TestOrigin::AttestPredicate, origin);
        if let Some((scope, name)) = &caught {
            input
                .permissions
                .insert(name.clone(), Permission::Within(BTreeSet::from([*scope])));
        }
        let mut outcomes = self.test(test, origin, &input);
        if let Some((_, name)) = caught {
            outcomes[0].permissions.insert(name, Permission::Blocked);
        }
        flow.facts = outcomes[0].meet(&outcomes[1]);
        flow.outcomes = outcomes.map(Some);
    }

    fn caught(&self, enabled: bool, origin: Option<usize>) -> Option<(usize, String)> {
        let scope = origin.filter(|_| enabled)?;
        self.program
            .caught_scratch
            .get(&scope)
            .map(|name| (scope, name.clone()))
    }

    fn test(&mut self, test: &SsaTest, origin: Option<usize>, facts: &Facts) -> [Facts; 2] {
        match test {
            SsaTest::Not(inner) => {
                let [no, yes] = self.test(inner, origin, facts);
                [yes, no]
            }
            SsaTest::And(a, b) => {
                let [no, yes] = self.test(a, origin, facts);
                let [right_no, right_yes] = self.test(b, origin, &yes);
                [no.meet(&right_no), right_yes]
            }
            SsaTest::Or(a, b) => {
                let [no, yes] = self.test(a, origin, facts);
                let [right_no, right_yes] = self.test(b, origin, &no);
                [right_no, yes.meet(&right_yes)]
            }
            SsaTest::Unary { op, operand } => self.unary_test(op, operand, origin, facts),
            SsaTest::Binary { left, op, right } => self.binary_test(left, op, right, origin, facts),
        }
    }

    fn unary_test(
        &mut self,
        op: &str,
        operand: &SsaValue,
        origin: Option<usize>,
        facts: &Facts,
    ) -> [Facts; 2] {
        self.value_patterns(operand, origin, facts);
        let mut yes = facts.clone();
        if op == "r?" {
            self.witness(operand, &mut yes);
        }
        [facts.clone(), yes]
    }

    fn binary_test(
        &mut self,
        left: &SsaValue,
        op: &str,
        right: &SsaTestRhs,
        origin: Option<usize>,
        facts: &Facts,
    ) -> [Facts; 2] {
        self.value_patterns(left, origin, facts);
        if let SsaTestRhs::Value(right) = right {
            self.value_patterns(right, origin, facts);
            if op == "=~" {
                self.pattern(right, origin, facts);
            }
        }
        [facts.clone(), facts.clone()]
    }

    fn witness(&self, value: &SsaValue, facts: &mut Facts) {
        let Some(place) = place_of(value, true).filter(|p| self.immutable(&p.name)) else {
            return;
        };
        let proofs = facts.limit(&place.name, unconditional());
        for alias in facts.equivalents(&place) {
            if self.immutable(&alias.name) {
                facts.add(alias, proofs.clone());
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn attest_all(
        &mut self,
        source: &VarRef,
        binders: &[VarRef],
        test: &SsaTest,
        is_map: bool,
        if_form: bool,
        origin: Option<usize>,
        flow: &mut Flow,
    ) {
        let checked = self.test(test, origin, &flow.facts)[1].clone();
        let mut yes = flow.facts.clone();
        yes.permissions.remove(&source.name);
        let caught = self.caught(if_form, origin);
        if let Some((scope, name)) = &caught {
            yes.permissions
                .insert(name.clone(), Permission::Within(BTreeSet::from([*scope])));
        }
        self.collection_witnesses(source, binders, is_map, &checked, &mut yes);
        if !if_form {
            flow.facts = yes;
            flow.reset(None);
            return;
        }
        let mut no = flow.facts.clone();
        if let Some((_, name)) = caught {
            no.permissions.insert(name, Permission::Blocked);
        }
        flow.facts = no.meet(&yes);
        flow.outcomes = [Some(no), Some(yes)];
    }

    fn collection_witnesses(
        &self,
        source: &VarRef,
        binders: &[VarRef],
        is_map: bool,
        checked: &Facts,
        result: &mut Facts,
    ) {
        if !self.immutable(&source.name) {
            return;
        }
        for (index, binder) in binders.iter().enumerate() {
            self.collection_witness(source, binder, is_map && index == 0, checked, result);
        }
    }

    fn collection_witness(
        &self,
        source: &VarRef,
        binder: &VarRef,
        keys: bool,
        checked: &Facts,
        result: &mut Facts,
    ) {
        let proofs = result.limit(&source.name, checked.get(&Place::scalar(&binder.name)));
        result.add(Place::collection(&source.name, keys), proofs);
    }

    fn certificate(&self, value: &SsaValue, facts: &Facts, present: bool) -> Certificates {
        let Some(value) = unquoted_value(value) else {
            return Certificates::new();
        };
        match value {
            SsaValue::RequireValue { source, .. } => self.certificate(source, facts, true),
            SsaValue::ElideEmpty {
                source,
                taint_off: false,
            } => self.element_certificate(source, true, facts),
            SsaValue::ArraySplat { array, disposition } => {
                self.splat_certificate(array, disposition.as_ref(), true, facts)
            }
            SsaValue::TableValue { array, .. } if self.immutable(&array.name) => {
                facts.get(&Place::collection(&array.name, false))
            }
            SsaValue::TableKey { array, .. } if self.immutable(&array.name) => {
                facts.get(&Place::collection(&array.name, true))
            }
            _ => place_of(value, false)
                .map(|place| self.place_certificate(&place, facts, present))
                .unwrap_or_default(),
        }
    }

    fn place_certificate(&self, place: &Place, facts: &Facts, present: bool) -> Certificates {
        if !self.immutable(&place.name) {
            return Certificates::new();
        }
        let mut proofs = facts.get(place);
        if matches!(place.selection, Selection::Cell { .. })
            && (present || facts.present.contains(place))
        {
            proofs.extend(facts.get(&Place::collection(&place.name, false)));
        }
        alternatives(proofs)
    }

    fn write(&self, inst: &SsaInst, facts: &mut Facts) {
        let Some(name) = instruction_write(inst) else {
            return;
        };
        let before = facts.clone();
        facts.kill(name);
        match inst {
            SsaInst::Assign { target, value } => self.assignment(target, value, &before, facts),
            SsaInst::CollectionPublish {
                source,
                append_to: None,
                ..
            } => {
                self.copy_collection(name, &source.name, &before, facts);
            }
            _ => {}
        }
    }

    fn assignment(&self, binding: &VarRef, value: &SsaValue, before: &Facts, after: &mut Facts) {
        let name = &binding.name;
        let target = Place::scalar(name);
        after.add(target.clone(), self.certificate(value, before, false));
        self.copy_value(&target, value, before, after);
        match value {
            SsaValue::IndexedArray(values) => self.array(name, values, before, after),
            SsaValue::TableKey { array, .. } => {
                let key = SsaValue::Var(binding.clone());
                after.present.push(Place::cell(&array.name, &key, false));
            }
            SsaValue::AssocArray(values) => {
                for (key, value) in values {
                    let cell = Place::cell(name, &SsaValue::Literal(key.clone()), false);
                    self.copy_cell(cell, value, false, before, after);
                }
            }
            _ => {}
        }
    }

    fn copy_value(&self, target: &Place, value: &SsaValue, before: &Facts, after: &mut Facts) {
        let Some(source) = place_of(value, false) else {
            return;
        };
        if !source.reads(&target.name) {
            after.alias(target, &source);
        }
        if matches!(source.selection, Selection::Scalar) {
            self.copy_collection(&target.name, &source.name, before, after);
        }
    }

    fn copy_collection(&self, name: &str, source: &str, before: &Facts, after: &mut Facts) {
        if !self.immutable(source) {
            return;
        }
        for (place, _) in before.proofs.iter().filter(|(p, _)| p.name == source) {
            after.add(place.renamed(name), before.get(place));
        }
        after.present.extend(
            before
                .present
                .iter()
                .filter(|p| p.name == source)
                .map(|p| p.renamed(name)),
        );
        if let Some(size) = before.sizes.get(source) {
            after.sizes.insert(name.to_string(), *size);
        }
    }

    fn array(&self, name: &str, values: &[SsaValue], before: &Facts, after: &mut Facts) {
        let literals = self.program.immutable_bindings.contains(name)
            && !self.program.source_bindings.contains(name);
        let mut index = Some(0usize);
        let mut all = unconditional();
        for value in values {
            let proofs = self.array_element(name, index, value, literals, before, after);
            all = require_both(&all, &proofs);
            index = index.and_then(|i| element_count(value, before).and_then(|n| i.checked_add(n)));
        }
        after.add(Place::collection(name, false), all);
        if let Some(size) = index {
            after.sizes.insert(name.to_string(), size);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn array_element(
        &self,
        name: &str,
        index: Option<usize>,
        value: &SsaValue,
        literals: bool,
        before: &Facts,
        after: &mut Facts,
    ) -> Certificates {
        if let SsaValue::ArraySplat { array, disposition } = value {
            return self.copy_splat(
                name,
                index,
                array,
                disposition.as_ref(),
                literals,
                before,
                after,
            );
        }
        if let Some(index) = index {
            let cell = Place::cell(name, &SsaValue::Literal(index.to_string()), true);
            self.copy_cell(cell, value, literals, before, after)
        } else {
            self.element_certificate(value, literals, before)
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn copy_splat(
        &self,
        name: &str,
        offset: Option<usize>,
        source: &VarRef,
        disposition: Option<&crate::ssa::ArraySplatDisposition>,
        literals: bool,
        before: &Facts,
        after: &mut Facts,
    ) -> Certificates {
        let trusted = disposition.is_some_and(|d| d.taint_off);
        let default =
            splat_default(disposition).filter(|_| before.sizes.get(&source.name) == Some(&0));
        if let (Some(index), Some(default)) = (offset, default) {
            let cell = Place::cell(name, &SsaValue::Literal(index.to_string()), true);
            self.copy_cell(cell, default, literals && !trusted, before, after);
        } else {
            self.copy_splat_cells(name, offset, source, trusted, before, after);
        }
        self.splat_certificate(source, disposition, literals, before)
    }

    fn copy_splat_cells(
        &self,
        name: &str,
        offset: Option<usize>,
        source: &VarRef,
        trusted: bool,
        before: &Facts,
        after: &mut Facts,
    ) {
        let (Some(offset), Some(size)) = (offset, before.sizes.get(&source.name)) else {
            return;
        };
        for index in 0..*size {
            let from = Place::cell(&source.name, &SsaValue::Literal(index.to_string()), true);
            let to = Place::cell(name, &SsaValue::Literal((offset + index).to_string()), true);
            after.present.push(to.clone());
            if !trusted {
                after.add(to.clone(), self.place_certificate(&from, before, true));
                after.alias(&to, &from);
            }
        }
    }

    fn splat_certificate(
        &self,
        source: &VarRef,
        disposition: Option<&crate::ssa::ArraySplatDisposition>,
        literals: bool,
        facts: &Facts,
    ) -> Certificates {
        if disposition.is_some_and(|d| d.taint_off) {
            return Certificates::new();
        }
        let size = facts.sizes.get(&source.name).copied();
        let proofs = if size == Some(0) {
            unconditional()
        } else {
            self.place_certificate(&Place::collection(&source.name, false), facts, false)
        };
        let Some(default) = splat_default(disposition).filter(|_| size.is_none_or(|n| n == 0))
        else {
            return proofs;
        };
        require_both(&proofs, &self.element_certificate(default, literals, facts))
    }

    fn copy_cell(
        &self,
        cell: Place,
        value: &SsaValue,
        literals: bool,
        before: &Facts,
        after: &mut Facts,
    ) -> Certificates {
        let proofs = self.element_certificate(value, literals, before);
        after.present.push(cell.clone());
        after.add(cell.clone(), proofs.clone());
        if let Some(source) = place_of(value, false) {
            if !source.reads(&cell.name) {
                after.alias(&cell, &source);
            }
        }
        proofs
    }

    fn element_certificate(&self, value: &SsaValue, literals: bool, facts: &Facts) -> Certificates {
        if literals && static_pattern(value) {
            unconditional()
        } else {
            self.certificate(value, facts, false)
        }
    }

    fn instruction_reads(&mut self, inst: &SsaInst, origin: Option<usize>, facts: &Facts) {
        let subject = match inst {
            SsaInst::Attest { source, .. } | SsaInst::AttestAll { source, .. } => {
                Some(source.name.as_str())
            }
            SsaInst::Test(_, TestOrigin::AttestPredicate) => origin
                .and_then(|id| self.program.caught_scratch.get(&id))
                .map(String::as_str),
            _ => None,
        };
        let mut names = BTreeSet::new();
        for (read, _) in inst.ordinary_reads() {
            names.extend(read_names(read));
        }
        for name in names {
            self.restricted_read(&name, subject, origin, facts);
        }
    }

    fn restricted_read(
        &mut self,
        name: &str,
        subject: Option<&str>,
        origin: Option<usize>,
        facts: &Facts,
    ) {
        if subject == Some(name)
            || !matches!(facts.permissions.get(name), Some(Permission::Blocked))
        {
            return;
        }
        let (site, call) = origin_sites(self.program, origin);
        let diagnostic = Diagnostic::error_span(
            call,
            "SB5107",
            format!(
                "scratch '{name}' read at {}:{} is outside its caught attest-success arm; \
                 use an explicit :! read for ordinary data",
                site.line, site.col
            ),
        );
        if !self.diagnostics.contains(&diagnostic) {
            self.diagnostics.push(diagnostic);
        }
    }

    fn instruction_patterns(&mut self, inst: &SsaInst, origin: Option<usize>, facts: &Facts) {
        match inst {
            SsaInst::Test(..) | SsaInst::Attest { .. } | SsaInst::AttestAll { .. } => return,
            SsaInst::Assign { value, .. } => self.value_patterns(value, origin, facts),
            SsaInst::MatchInto { pattern, .. } => self.pattern(pattern, origin, facts),
            SsaInst::GlobFill {
                directory: Some(_),
                patterns,
                ..
            } => self.patterns(patterns, origin, facts),
            SsaInst::Command {
                kind: BuiltinCmd::Match,
                args,
                ..
            } => {
                let [_, pattern] = BuiltinCmd::Match.expect_ssa_args(args);
                self.pattern(pattern, origin, facts);
            }
            _ => {}
        }
        for (read, _) in inst.taint_reads() {
            if let TaintRead::Value(value) = read {
                self.value_patterns(value, origin, facts);
            }
        }
    }

    fn patterns(&mut self, values: &[SsaValue], origin: Option<usize>, facts: &Facts) {
        for value in values {
            self.pattern(value, origin, facts);
        }
    }

    fn value_patterns(&mut self, value: &SsaValue, origin: Option<usize>, facts: &Facts) {
        if let SsaValue::RegexMatch { pattern, .. } | SsaValue::RegexReplace { pattern, .. } = value
        {
            self.pattern(pattern, origin, facts);
        }
        for child in value.child_values() {
            self.value_patterns(child, origin, facts);
        }
    }

    fn pattern(&mut self, value: &SsaValue, origin: Option<usize>, facts: &Facts) {
        if !self.reporting
            || static_pattern(value)
            || !self.certificate(value, facts, false).is_empty()
        {
            return;
        }
        let (site, call) = origin_sites(self.program, origin);
        let diagnostic = Diagnostic::error_span(
            call,
            "SB5106",
            format!(
                "runtime regex pattern at {}:{} needs an immutable value \
                 validated by r? on every reaching path",
                site.line, site.col
            ),
        );
        if !self.diagnostics.contains(&diagnostic) {
            self.diagnostics.push(diagnostic);
        }
    }
}

fn read_names(read: TaintRead<'_>) -> Vec<String> {
    let mut names = Vec::new();
    match read {
        TaintRead::Value(value) => {
            value.read_variables(&mut |var| names.push(var.name.clone()));
        }
        TaintRead::Binding(var) => names.push(var.name.clone()),
        TaintRead::RedirectTarget(expr) => {
            crate::ir::walk_expr_tree(expr, &mut |node| {
                crate::ssa_taint::collect_expr_var_name(node, &mut names);
            });
        }
    }
    names
}

fn place_of(value: &SsaValue, trusted_candidate: bool) -> Option<Place> {
    match unquoted_value(value)? {
        SsaValue::Var(var) => Some(Place::scalar(&var.name)),
        SsaValue::Subscript {
            array,
            index,
            is_integer_key,
            ..
        } => Some(Place::cell(&array.name, index, *is_integer_key)),
        SsaValue::Trusted { source } if trusted_candidate => place_of(source, true),
        SsaValue::RequireValue { source, .. } => place_of(source, trusted_candidate),
        _ => None,
    }
}

fn unquoted_value(value: &SsaValue) -> Option<&SsaValue> {
    match value {
        SsaValue::Concat(parts) => {
            let mut values = parts
                .iter()
                .filter(|v| v.literal_text().as_deref() != Some(""));
            let only = values.next()?;
            if values.next().is_none() {
                unquoted_value(only)
            } else {
                None
            }
        }
        _ => Some(value),
    }
}

fn splat_default(disposition: Option<&crate::ssa::ArraySplatDisposition>) -> Option<&SsaValue> {
    disposition
        .filter(|d| d.has_word && !d.abort)
        .map(|d| d.word.as_ref())
}

fn static_pattern(value: &SsaValue) -> bool {
    value
        .literal_text()
        .is_some_and(|s| crate::regex_validate::validate_regex(&s).is_ok())
}

/// Calls and external reads can change without a write to an indexed binding.
/// Total over `SsaValue`; do not add a wildcard.
fn stable_index(value: &SsaValue) -> bool {
    match value {
        SsaValue::CmdSubst { .. }
        | SsaValue::CmdSubstBody { .. }
        | SsaValue::FuncCall { .. }
        | SsaValue::IntFuncCall { .. }
        | SsaValue::SlurpFile(_) => false,
        SsaValue::Literal(_)
        | SsaValue::Var(_)
        | SsaValue::Expr(_)
        | SsaValue::ArithExpr { .. }
        | SsaValue::ParamExpansion { .. }
        | SsaValue::ArgvIndex(_)
        | SsaValue::TableValue { .. }
        | SsaValue::TableKey { .. } => true,
        SsaValue::Concat(_)
        | SsaValue::ArithBinOp { .. }
        | SsaValue::RegexReplace { .. }
        | SsaValue::RegexMatch { .. }
        | SsaValue::CaseConvert { .. }
        | SsaValue::StrLen { .. }
        | SsaValue::DefaultValue { .. }
        | SsaValue::RequireValue { .. }
        | SsaValue::ElideEmpty { .. }
        | SsaValue::Trusted { .. }
        | SsaValue::Substring { .. }
        | SsaValue::AssocArray(_)
        | SsaValue::Subscript { .. }
        | SsaValue::IndexedArray(_)
        | SsaValue::IntCell(_)
        | SsaValue::Ternary { .. }
        | SsaValue::ArraySplat { .. } => value.child_values().into_iter().all(stable_index),
    }
}

fn element_count(value: &SsaValue, facts: &Facts) -> Option<usize> {
    let (array, disposition) = match value {
        SsaValue::ArraySplat { array, disposition } => (array, disposition),
        SsaValue::ElideEmpty { .. } => return None,
        _ => return Some(1),
    };
    let size = *facts.sizes.get(&array.name)?;
    Some(size.max(usize::from(splat_default(disposition.as_ref()).is_some())))
}
