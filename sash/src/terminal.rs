//! DD#128 operational failures are checked where lowering actually executes them.

use crate::diagnostic::Diagnostic;
use crate::ir::{HandlerKind, Span, VarType};
use crate::ssa::*;
use std::collections::{BTreeSet, HashMap, HashSet};

pub(crate) fn check_err(program: &SsaProgram) -> Vec<Diagnostic> {
    let Some(handler) = program.handlers.get(&HandlerKind::Err) else {
        return Vec::new();
    };
    let facts = Facts::new(program);
    let mut errors = Vec::new();
    for id in live_blocks(program, handler.block) {
        block_failures(program, program.block(id), &facts, &mut errors);
    }
    let mut seen = HashSet::new();
    errors.retain(|d| seen.insert((d.line, d.col, d.message.clone())));
    errors
}

fn block_failures(
    program: &SsaProgram,
    block: &BasicBlock,
    facts: &Facts<'_>,
    errors: &mut Vec<Diagnostic>,
) {
    let mut origin = block.origin;
    for inst in live_insts(block) {
        if let SsaInst::SourceOrigin(next, _) = inst {
            origin = *next;
            continue;
        }
        errors.extend(
            instruction_failures(inst, facts)
                .iter()
                .map(|failure| failure_diagnostic(program, origin.or(block.origin), failure)),
        );
    }
}

fn failure_diagnostic(program: &SsaProgram, origin: Option<usize>, failure: &str) -> Diagnostic {
    let mut path = Vec::new();
    let mut next = origin;
    while let Some(id) = next {
        let source = program.origins[id];
        if path.last() != Some(&source.span) {
            path.push(source.span);
        }
        next = source.parent;
    }
    let site = path.first().copied().unwrap_or(Span::new(1, 1));
    let call = path.last().copied().unwrap_or(site);
    Diagnostic::error_span(
        call,
        "SB5101",
        format!(
            "ERR can be abandoned by {failure} at {}:{}; catch or suppress that failure \
             where it occurs. Handling an enclosing command or function result does not reach it",
            site.line, site.col
        ),
    )
}

pub(crate) fn live_insts(block: &BasicBlock) -> &[SsaInst] {
    let end = block
        .insts
        .iter()
        .position(is_exit)
        .map_or(block.insts.len(), |i| i + 1);
    &block.insts[..end]
}

fn is_exit(inst: &SsaInst) -> bool {
    matches!(
        inst,
        SsaInst::Command {
            kind: BuiltinCmd::Exit,
            ..
        }
    )
}

pub(crate) fn live_blocks(program: &SsaProgram, root: BlockId) -> BTreeSet<BlockId> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        if seen.insert(id) {
            pending.extend(live_successors(program.block(id)));
        }
    }
    seen
}

pub(crate) fn live_successors(block: &BasicBlock) -> Vec<BlockId> {
    if block.insts.iter().any(is_exit) {
        Vec::new()
    } else {
        block.terminator.successors()
    }
}

/// Byte properties are independent of taint: `:!` cannot prevent exec's NUL guard.
#[derive(Clone, Default, PartialEq)]
struct Proof {
    may_nul: bool,
    may_empty: bool,
    keys_may_nul: bool,
    text: Option<String>,
    count: Option<usize>,
}

impl Proof {
    fn text(text: &str) -> Self {
        Self {
            may_nul: text.contains('\0'),
            may_empty: text.is_empty(),
            text: Some(text.to_string()),
            ..Self::default()
        }
    }

    fn unknown() -> Self {
        Self {
            may_nul: true,
            may_empty: true,
            keys_may_nul: true,
            ..Self::default()
        }
    }

    fn c_string() -> Self {
        Self {
            may_empty: true,
            ..Self::default()
        }
    }

    fn join(&mut self, other: Self) {
        self.may_nul |= other.may_nul;
        self.may_empty |= other.may_empty;
        self.keys_may_nul |= other.keys_may_nul;
        if self.text != other.text {
            self.text = None;
        }
        if self.count != other.count {
            self.count = None;
        }
    }
}

enum Definition<'a> {
    Inst(&'a SsaInst),
    Phi(&'a PhiNode),
    Input,
}

struct Mutation<'a> {
    value: Option<&'a SsaValue>,
    key: Option<&'a SsaValue>,
    may_empty: bool,
}

struct Facts<'a> {
    definitions: HashMap<VarRef, Definition<'a>>,
    values: HashMap<VarRef, Proof>,
    incoming: HashMap<String, Vec<VarRef>>,
    mutations: HashMap<String, Vec<Mutation<'a>>>,
    collections: HashSet<String>,
    live: BTreeSet<BlockId>,
}

impl<'a> Facts<'a> {
    fn new(program: &'a SsaProgram) -> Self {
        let mut facts = Self {
            definitions: HashMap::new(),
            values: HashMap::new(),
            incoming: HashMap::new(),
            mutations: HashMap::new(),
            collections: program
                .globals
                .iter()
                .filter(|(_, kind)| **kind == GlobalType::Table)
                .map(|(name, _)| name.clone())
                .collect(),
            live: BTreeSet::new(),
        };
        facts.collect_program(program);
        facts.converge();
        facts
    }

    fn collect_program(&mut self, program: &'a SsaProgram) {
        let roots = std::iter::once((None, program.entry)).chain(
            program
                .handlers
                .iter()
                .map(|(kind, handler)| (Some(*kind), handler.block)),
        );
        for (kind, root) in roots {
            self.collect_root(program, kind, root);
        }
        for input in program.handlers.values().flat_map(|h| h.inputs.values()) {
            self.definitions.insert(input.clone(), Definition::Input);
        }
    }

    fn collect_root(&mut self, program: &'a SsaProgram, kind: Option<HandlerKind>, root: BlockId) {
        for id in live_blocks(program, root) {
            self.live.insert(id);
            self.collect_block(program.block(id), kind != Some(HandlerKind::Err), program);
        }
    }

    fn converge(&mut self) {
        // Unknown definitions start at meaningful zeros. External byte producers
        // introduce possible NULs; cycles alone cannot manufacture a byte.
        self.values.extend(
            self.definitions
                .keys()
                .map(|v| (v.clone(), Proof::c_string())),
        );
        loop {
            let next: HashMap<_, _> = self
                .definitions
                .iter()
                .map(|(target, def)| (target.clone(), self.definition(target, def)))
                .collect();
            if next == self.values {
                break;
            }
            self.values = next;
        }
    }

    fn collect_block(&mut self, block: &'a BasicBlock, incoming: bool, program: &SsaProgram) {
        for phi in &block.phis {
            self.definitions
                .insert(phi.target.clone(), Definition::Phi(phi));
        }
        for inst in live_insts(block) {
            self.collect_definition(inst, incoming, program);
            self.collect_mutation(inst);
        }
    }

    fn collect_definition(&mut self, inst: &'a SsaInst, incoming: bool, program: &SsaProgram) {
        let Some(target) = crate::ssa_taint::instruction_target(inst) else {
            return;
        };
        self.definitions
            .insert(target.clone(), Definition::Inst(inst));
        if incoming && program.globals.contains_key(&target.name) {
            self.incoming
                .entry(target.name.clone())
                .or_default()
                .push(target.clone());
        }
    }

    fn collect_mutation(&mut self, inst: &'a SsaInst) {
        match inst {
            SsaInst::SubscriptAssign {
                array,
                index,
                value,
                ..
            } => {
                self.mutations
                    .entry(array.clone())
                    .or_default()
                    .push(Mutation {
                        value: Some(value),
                        key: Some(index),
                        may_empty: false,
                    });
            }
            SsaInst::ArrayAppend { array, value } => {
                self.mutations
                    .entry(array.clone())
                    .or_default()
                    .push(Mutation {
                        value: Some(value),
                        key: None,
                        may_empty: false,
                    });
            }
            SsaInst::MatchInto { array, subject, .. } => {
                self.mutations
                    .entry(array.clone())
                    .or_default()
                    .push(Mutation {
                        value: Some(subject),
                        key: None,
                        may_empty: true,
                    });
            }
            SsaInst::Command {
                kind: kind @ (BuiltinCmd::TableDelete | BuiltinCmd::TableDeleteIdx),
                args,
                ..
            } => {
                let [table, _] = kind.expect_ssa_args(args);
                let array = kind.expect_var_arg(table);
                self.mutations
                    .entry(array.name.clone())
                    .or_default()
                    .push(Mutation {
                        value: None,
                        key: None,
                        may_empty: true,
                    });
            }
            SsaInst::Assign {
                target,
                value: SsaValue::IndexedArray(_) | SsaValue::AssocArray(_),
            }
            | SsaInst::ReadArray { target, .. }
            | SsaInst::SplitValue { target, .. }
            | SsaInst::ArgvFill { target, .. }
            | SsaInst::KeysFill { target, .. }
            | SsaInst::MapFromArray { target, .. } => {
                self.collections.insert(target.name.clone());
            }
            _ => {}
        }
    }

    fn definition(&self, target: &VarRef, def: &Definition<'_>) -> Proof {
        if target.var_type == VarType::Int && !self.collections.contains(&target.name) {
            return Proof::default();
        }
        let mut proof = match def {
            Definition::Input => self.input_proof(target),
            Definition::Phi(phi) => self.phi_proof(phi),
            Definition::Inst(inst) => self.instruction(inst),
        };
        if let Some(stores) = self.mutations.get(&target.name) {
            self.apply_mutations(&mut proof, stores);
        }
        proof
    }

    fn apply_mutations(&self, proof: &mut Proof, stores: &[Mutation<'_>]) {
        proof.count = None;
        proof.text = None;
        for store in stores {
            proof.may_nul |= store.value.is_some_and(|value| self.value(value).may_nul);
            proof.keys_may_nul |= store.key.is_some_and(|key| self.value(key).may_nul);
            proof.may_empty |= store.may_empty;
        }
    }

    fn input_proof(&self, target: &VarRef) -> Proof {
        let mut proof = Proof::c_string();
        for source in self.incoming.get(&target.name).into_iter().flatten() {
            proof.join(self.variable(source));
        }
        proof
    }

    fn phi_proof(&self, phi: &PhiNode) -> Proof {
        let mut inputs = phi.sources.iter().filter(|(id, _)| self.live.contains(id));
        let Some((_, first)) = inputs.next() else {
            return Proof::unknown();
        };
        let mut proof = self.variable(first);
        for (_, input) in inputs {
            proof.join(self.variable(input));
        }
        proof
    }

    fn instruction(&self, inst: &SsaInst) -> Proof {
        match inst {
            SsaInst::Assign { value, .. } => self.value(value),
            SsaInst::Attest { source, .. } | SsaInst::AttestRetag { source, .. } => {
                self.variable(source)
            }
            SsaInst::ReadArray { .. } | SsaInst::SplitValue { .. } | SsaInst::ArgvFill { .. } => {
                Proof::c_string()
            }
            SsaInst::KeysFill { source, .. } => {
                let mut proof = Proof::c_string();
                proof.may_nul = self
                    .values
                    .iter()
                    .filter(|(v, _)| &v.name == source)
                    .any(|(v, _)| self.variable(v).keys_may_nul);
                proof
            }
            SsaInst::MapFromArray { source, key, .. } => {
                let mut proof = self.variable(source);
                proof.keys_may_nul = proof.may_nul || self.value(key).may_nul;
                proof.may_empty = true;
                proof.count = None;
                proof
            }
            SsaInst::ReadLine { .. } | SsaInst::CaptureEnd { .. } | SsaInst::Pipeline { .. } => {
                Proof::unknown()
            }
            _ => unreachable!("only defining instructions enter the byte-property analysis"),
        }
    }

    fn variable(&self, var: &VarRef) -> Proof {
        self.values
            .get(var)
            .cloned()
            .unwrap_or_else(|| Proof::text(""))
    }

    fn value(&self, value: &SsaValue) -> Proof {
        match value {
            SsaValue::Literal(text) => Proof::text(text),
            SsaValue::Var(var) => self.variable(var),
            SsaValue::Trusted { source }
            | SsaValue::ElideEmpty { source, .. }
            | SsaValue::RequireValue { source, .. } => self.value(source),
            SsaValue::ArithExpr { .. }
            | SsaValue::ArithBinOp { .. }
            | SsaValue::StrLen { .. }
            | SsaValue::IntCell(_) => Proof::default(),
            SsaValue::CmdSubst { argv, .. } => intrinsic_proof(argv, self),
            SsaValue::Concat(parts) => {
                let mut proof = Proof::text("");
                for part in parts {
                    let next = self.value(part);
                    proof.may_nul |= next.may_nul;
                    proof.may_empty &= next.may_empty;
                    proof.text = proof.text.zip(next.text).map(|(mut left, right)| {
                        left.push_str(&right);
                        left
                    });
                }
                proof
            }
            SsaValue::IndexedArray(parts) => {
                let fixed = parts.iter().all(|p| {
                    !matches!(p, SsaValue::ArraySplat { .. } | SsaValue::ElideEmpty { .. })
                });
                Proof {
                    may_nul: parts.iter().any(|p| self.value(p).may_nul),
                    may_empty: !fixed || parts.is_empty(),
                    keys_may_nul: !parts.is_empty(),
                    count: fixed.then_some(parts.len()),
                    text: None,
                }
            }
            SsaValue::AssocArray(parts) => Proof {
                may_nul: parts.iter().any(|(_, p)| self.value(p).may_nul),
                may_empty: parts.is_empty(),
                keys_may_nul: parts.iter().any(|(key, _)| key.contains('\0')),
                count: Some(parts.len()),
                text: None,
            },
            SsaValue::Subscript {
                array,
                is_integer_value: true,
                ..
            } if !self.variable(array).may_nul => Proof::default(),
            SsaValue::Subscript { array, .. } | SsaValue::TableValue { array, .. } => Proof {
                may_nul: self.variable(array).may_nul,
                ..Proof::c_string()
            },
            SsaValue::TableKey { array, .. } => Proof {
                may_nul: self.variable(array).keys_may_nul,
                ..Proof::c_string()
            },
            SsaValue::ArraySplat { array, disposition } => {
                let mut proof = self.variable(array);
                if let Some(d) = disposition.as_ref().filter(|d| d.has_word && !d.abort) {
                    proof.join(self.value(&d.word));
                }
                proof
            }
            SsaValue::DefaultValue { source, default } => {
                let mut proof = self.value(source);
                if proof.may_empty {
                    proof.join(self.value(default));
                    proof.may_empty = self.value(default).may_empty;
                }
                proof
            }
            SsaValue::Ternary {
                true_val,
                false_val,
                ..
            } => {
                let mut proof = self.value(true_val);
                proof.join(self.value(false_val));
                proof
            }
            SsaValue::CaseConvert { subject, .. }
            | SsaValue::Substring {
                source: subject, ..
            }
            | SsaValue::RegexMatch { subject, .. } => Proof {
                may_nul: self.value(subject).may_nul,
                ..Proof::c_string()
            },
            SsaValue::RegexReplace {
                subject,
                replacement,
                ..
            } => Proof {
                may_nul: self.value(subject).may_nul || self.value(replacement).may_nul,
                ..Proof::c_string()
            },
            SsaValue::ParamExpansion {
                var,
                operator,
                operand,
            } => self.parameter(var, operator, operand),
            SsaValue::ArgvIndex(_) => Proof::c_string(),
            SsaValue::Expr(_)
            | SsaValue::FuncCall { .. }
            | SsaValue::IntFuncCall { .. }
            | SsaValue::CmdSubstBody { .. }
            | SsaValue::SlurpFile(_) => Proof::unknown(),
        }
    }

    fn parameter(&self, var: &VarRef, op: &str, operand: &str) -> Proof {
        if matches!(op, "strlen" | "arrlen") {
            return Proof::default();
        }
        if op == "glob" {
            return Proof {
                keys_may_nul: true,
                ..Proof::c_string()
            };
        }
        Proof {
            may_nul: self.variable(var).may_nul || operand.contains('\0'),
            ..Proof::c_string()
        }
    }

    fn regex_safe(&self, pattern: &SsaValue) -> bool {
        self.value(pattern)
            .text
            .is_some_and(|text| crate::regex_validate::validate_regex(&text).is_ok())
    }
}

fn intrinsic_proof(argv: &[SsaValue], facts: &Facts<'_>) -> Proof {
    let Some(SsaValue::Literal(name)) = argv.first() else {
        return Proof::unknown();
    };
    match name.as_str() {
        "arg" => {
            let mut proof = Proof::c_string();
            if let Some(default) = argv.get(2) {
                proof.join(facts.value(default));
            }
            proof
        }
        "getenv" | "getcwd" | "argv" => Proof::c_string(),
        "len" | "strlen" | "epochseconds" | "random" | "argc" | "getpid" | "getppid"
        | "waitstatus" | "getcpid" | "status" | "signum" | "wait" => Proof::default(),
        _ => Proof::unknown(),
    }
}

/// Total over executable SSA instructions; do not add a wildcard.
fn instruction_failures(inst: &SsaInst, facts: &Facts<'_>) -> Vec<String> {
    let mut errors = Vec::new();
    match inst {
        SsaInst::Command {
            name,
            kind,
            args,
            redirects,
            checked,
        } => {
            errors.extend(command_failures(name, *kind, args, *checked, facts));
            redirect_failure(redirects, &mut errors);
            values_failures(args, facts, &mut errors);
        }
        SsaInst::Pipeline {
            stages,
            checked,
            capture,
            ..
        } => pipeline_failures(stages, *checked, capture, facts, &mut errors),
        SsaInst::PipelineFinish { checked: true, .. } => {
            errors.push("uncaught producer failure".to_string())
        }
        SsaInst::PipelineFinish { checked: false, .. } => {}
        SsaInst::Attest { test, .. }
        | SsaInst::AttestAll {
            test,
            if_form: false,
            ..
        } => {
            errors.push("an uncaught attest failure".to_string());
            test_failures(test, facts, &mut errors);
        }
        SsaInst::AttestAll {
            test,
            if_form: true,
            ..
        }
        | SsaInst::Test(test, _) => test_failures(test, facts, &mut errors),
        SsaInst::ReadArray { redirects, .. } => {
            errors.push("a bulk read's uncatchable I/O failure".to_string());
            redirect_failure(redirects, &mut errors);
        }
        SsaInst::ReadLine { redirects, .. } | SsaInst::RedirectPush { redirects, .. } => {
            redirect_failure(redirects, &mut errors)
        }
        SsaInst::CaptureStart {
            buffered: false, ..
        }
        | SsaInst::CaptureEnd {
            buffered: false, ..
        } => {
            errors.push("a descriptor capture's uncatchable setup or read failure".to_string());
        }
        SsaInst::MapFromArray {
            source, key, mode, ..
        } => map_conversion_failures(source, key, *mode, facts, &mut errors),
        SsaInst::Assign { value, .. }
        | SsaInst::ArrayAppend { value, .. }
        | SsaInst::SplitValue { source: value, .. } => value_failures(value, facts, &mut errors),
        SsaInst::SubscriptAssign { index, value, .. } => {
            values_failures([index, value], facts, &mut errors)
        }
        SsaInst::MatchInto {
            subject, pattern, ..
        } => {
            regex_failure(pattern, facts, &mut errors);
            values_failures([subject, pattern], facts, &mut errors);
        }
        SsaInst::CaptureStart { buffered: true, .. }
        | SsaInst::CaptureEnd { buffered: true, .. }
        | SsaInst::AttestRetag { .. }
        | SsaInst::ArgvFill { .. }
        | SsaInst::KeysFill { .. }
        | SsaInst::RedirectPop { .. }
        | SsaInst::SourceOrigin(..) => {}
    }
    errors
}

fn pipeline_failures(
    stages: &[(String, Vec<SsaValue>, Vec<crate::ir::Redirect>)],
    checked: bool,
    capture: &PipelineCapture,
    facts: &Facts<'_>,
    errors: &mut Vec<String>,
) {
    if checked {
        errors.push("uncaught pipeline construction or process failure".to_string());
    }
    if let PipelineCapture::Value(_) = capture {
        errors.push("a capture drain's uncatchable read failure".to_string());
        for (name, args, _) in stages.iter().filter(|(name, _, _)| name.starts_with('/')) {
            argv_failure(name, args, facts, errors);
            values_failures(args, facts, errors);
        }
    }
}

fn map_conversion_failures(
    source: &VarRef,
    key: &SsaValue,
    mode: u8,
    facts: &Facts<'_>,
    errors: &mut Vec<String>,
) {
    if mode == 1
        && facts
            .variable(source)
            .count
            .is_none_or(|count| count % 2 != 0)
    {
        errors.push("an unhandled odd-length map conversion".to_string());
    }
    value_failures(key, facts, errors);
}

/// Total over command kinds; do not add a wildcard.
fn command_failures(
    name: &str,
    kind: BuiltinCmd,
    args: &[SsaValue],
    checked: bool,
    facts: &Facts<'_>,
) -> Vec<String> {
    let mut errors = Vec::new();
    match kind {
        BuiltinCmd::Other => {
            errors.extend(checked.then(|| format!("uncaught command failure (`{name}`)")));
            argv_failure(name, args, facts, &mut errors);
        }
        BuiltinCmd::Echo | BuiltinCmd::Cd | BuiltinCmd::Close | BuiltinCmd::StoreBgPid => {
            errors.extend(checked.then(|| format!("uncaught `{name}` failure")));
        }
        BuiltinCmd::Exec => {
            errors.extend(
                (!args.is_empty()).then(|| "an uncatchable replacing-exec failure".to_string()),
            );
        }
        BuiltinCmd::Match => {
            let [_, pattern] = BuiltinCmd::Match.expect_ssa_args(args);
            regex_failure(pattern, facts, &mut errors);
        }
        BuiltinCmd::Noop
        | BuiltinCmd::Exit
        | BuiltinCmd::NegateTestResult
        | BuiltinCmd::SetTestFromStatus
        | BuiltinCmd::SetTestFromPipe
        | BuiltinCmd::LetExit
        | BuiltinCmd::TrapSignal
        | BuiltinCmd::SignalFlag
        | BuiltinCmd::SignalCheckpoint
        | BuiltinCmd::TableDelete
        | BuiltinCmd::TableDeleteIdx
        | BuiltinCmd::Wait => {}
        BuiltinCmd::Write
        | BuiltinCmd::Test
        | BuiltinCmd::Attest
        | BuiltinCmd::Read
        | BuiltinCmd::Mapfile
        | BuiltinCmd::Argv
        | BuiltinCmd::Break
        | BuiltinCmd::Continue
        | BuiltinCmd::Return
        | BuiltinCmd::Signal
        | BuiltinCmd::Unset
        | BuiltinCmd::Len
        | BuiltinCmd::Strlen
        | BuiltinCmd::Keys
        | BuiltinCmd::Join
        | BuiltinCmd::Sub
        | BuiltinCmd::Gsub
        | BuiltinCmd::MatchAll
        | BuiltinCmd::Toupper
        | BuiltinCmd::Tolower
        | BuiltinCmd::Basename
        | BuiltinCmd::Dirname => ice_unreachable_command(&kind, name),
    }
    errors
}

fn argv_failure(name: &str, args: &[SsaValue], facts: &Facts<'_>, errors: &mut Vec<String>) {
    if args.iter().any(|arg| facts.value(arg).may_nul) {
        errors.push(format!("the uncatchable NUL-byte argv guard for `{name}`"));
    }
}

fn redirect_failure(redirects: &[crate::ir::Redirect], errors: &mut Vec<String>) {
    if !redirects.is_empty() {
        errors.push("an uncatchable descriptor backup/restore failure".to_string());
    }
}

fn regex_failure(pattern: &SsaValue, facts: &Facts<'_>, errors: &mut Vec<String>) {
    if !facts.regex_safe(pattern) {
        errors.push("an uncatchable dynamic-regex compilation failure".to_string());
    }
}

fn values_failures<'a>(
    values: impl IntoIterator<Item = &'a SsaValue>,
    facts: &Facts<'_>,
    errors: &mut Vec<String>,
) {
    for value in values {
        value_failures(value, facts, errors);
    }
}

fn value_failures(value: &SsaValue, facts: &Facts<'_>, errors: &mut Vec<String>) {
    match value {
        SsaValue::RequireValue { source, message } => {
            empty_branch_failures(source, message, true, facts, errors);
            return;
        }
        SsaValue::DefaultValue { source, default } => {
            empty_branch_failures(source, default, false, facts, errors);
            return;
        }
        SsaValue::SlurpFile(_) => {
            errors.push("a slurp's uncatchable open/read failure".to_string())
        }
        SsaValue::RegexMatch { pattern, .. } | SsaValue::RegexReplace { pattern, .. } => {
            regex_failure(pattern, facts, errors)
        }
        SsaValue::ArraySplat { array, disposition } => {
            errors.extend(
                (disposition.as_ref().is_some_and(|d| d.abort) && facts.variable(array).may_empty)
                    .then(|| "an unhandled empty-splat `:?` failure".to_string()),
            );
        }
        SsaValue::CmdSubst { argv, .. } => {
            errors.extend(argv_bounds_failure(argv));
        }
        SsaValue::ParamExpansion { operator, .. } if operator == "glob" => {
            errors.push("a glob's uncatchable getcwd failure".to_string());
        }
        _ => {}
    }
    values_failures(value.child_values(), facts, errors);
}

fn empty_branch_failures(
    source: &SsaValue,
    branch: &SsaValue,
    required: bool,
    facts: &Facts<'_>,
    errors: &mut Vec<String>,
) {
    value_failures(source, facts, errors);
    if facts.value(source).may_empty {
        errors.extend(required.then(|| "an unhandled `:?` failure".to_string()));
        value_failures(branch, facts, errors);
    }
}

fn argv_bounds_failure(argv: &[SsaValue]) -> Option<String> {
    (argv.len() == 2
        && matches!(argv.first(), Some(SsaValue::Literal(name)) if matches!(name.as_str(), "argv" | "arg")))
        .then(|| "an uncatchable argv-bounds failure".to_string())
}

fn test_failures(test: &SsaTest, facts: &Facts<'_>, errors: &mut Vec<String>) {
    match test {
        SsaTest::Unary { operand, .. } => value_failures(operand, facts, errors),
        SsaTest::Binary { left, right, .. } => {
            value_failures(left, facts, errors);
            if let SsaTestRhs::Value(right) = right {
                value_failures(right, facts, errors);
            }
        }
        SsaTest::Not(inner) => test_failures(inner, facts, errors),
        SsaTest::And(left, right) | SsaTest::Or(left, right) => {
            test_failures(left, facts, errors);
            test_failures(right, facts, errors);
        }
    }
}
