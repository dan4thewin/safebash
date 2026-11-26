//! DD136: ERR owns recovery at a source call, never at an inner return.

use crate::diagnostic::Diagnostic;
use crate::ir::*;
use crate::ssa::*;
use crate::terminal::live_insts;
use std::collections::{HashMap, HashSet};

pub(super) fn lower_call(
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
    lower: impl FnOnce(&mut SsaProgram, &mut VersionState, BlockId) -> BlockId,
) -> BlockId {
    if !state.attempt_calls.contains(&c.name.span) {
        return lower(ssa, state, block);
    }
    let entry = ssa.new_block("attempt");
    let failed = ssa.new_block("attempt_failed");
    let join = ssa.new_block("attempt_done");
    let inputs: Vec<_> = ssa.globals.keys().map(|name| state.lookup(name)).collect();
    let before = state.snapshot();
    ssa.block_mut(block).terminator = Terminator::Goto(entry);
    ssa.block_mut(entry).insts.push(SsaInst::AttemptStart {
        failed,
        inputs: inputs.clone(),
    });
    let normal = lower(ssa, state, entry);
    let returned = state.snapshot();
    ssa.block_mut(normal)
        .insts
        .push(SsaInst::AttemptEnd { failed: false });
    ssa.block_mut(normal).terminator = Terminator::Goto(join);

    state.restore(&before);
    ssa.block_mut(failed)
        .insts
        .push(SsaInst::AttemptEnd { failed: true });
    for input in inputs {
        let target = state.define_input(&input.name);
        ssa.block_mut(failed).insts.push(SsaInst::RecoverGlobal {
            target,
            sources: Vec::new(),
        });
    }
    let aborted = state.snapshot();
    ssa.block_mut(failed).terminator = Terminator::Goto(join);
    state.restore(&returned);
    super::helpers::insert_phis_two_branch(state, ssa, join, normal, &returned, failed, &aborted);
    join
}

#[derive(Clone, Copy)]
enum Failure {
    Guard,
    Status,
    Explicit,
}

/// Only checks with an existing operational result belong here. Runtime-only
/// guards (including reads, NUL, indices and `:?`) still terminate.
/// Total over SsaInst on purpose — do not add a wildcard.
fn recoverable(inst: &SsaInst, in_condition: bool) -> Option<Failure> {
    match inst {
        SsaInst::Command {
            kind,
            checked,
            redirects,
            ..
        } => command_failure(kind, *checked, redirect_failure(redirects, in_condition)),
        SsaInst::Assign { value, .. } => unchecked_value_failure(value),
        SsaInst::Pipeline { checked, .. }
        | SsaInst::PipelineFinish { checked, .. }
        | SsaInst::RedirectPush { checked, .. } => checked.then_some(Failure::Guard),
        SsaInst::GlobFill { checked, .. } => checked.then_some(Failure::Status),
        SsaInst::Attest { .. } => Some(Failure::Guard),
        SsaInst::AttestAll { if_form, .. } => (!if_form).then_some(Failure::Guard),
        SsaInst::ReadLine { redirects, .. } | SsaInst::ReadArray { redirects, .. } => {
            redirect_failure(redirects, in_condition).then_some(Failure::Guard)
        }
        SsaInst::SourceOrigin(..)
        | SsaInst::AttemptStart { .. }
        | SsaInst::AttemptEnd { .. }
        | SsaInst::RecoverGlobal { .. }
        | SsaInst::LifetimeStart { .. }
        | SsaInst::CollectionPublish { .. }
        | SsaInst::Test(..)
        | SsaInst::SubscriptAssign { .. }
        | SsaInst::ArrayAppend { .. }
        | SsaInst::MapFromArray { .. }
        | SsaInst::SplitValue { .. }
        | SsaInst::MatchInto { .. }
        | SsaInst::ArgvFill { .. }
        | SsaInst::KeysFill { .. }
        | SsaInst::SliceFill { .. }
        | SsaInst::CaptureStart { .. }
        | SsaInst::CaptureEnd { .. }
        | SsaInst::RedirectPop { .. }
        | SsaInst::AttestRetag { .. } => None,
    }
}

fn unchecked_value_failure(value: &SsaValue) -> Option<Failure> {
    matches!(
        value,
        SsaValue::CmdSubst { argv, suppressed: false }
            if matches!(argv.first(), Some(SsaValue::Literal(name)) if name == "mkdtemp")
    )
    .then_some(Failure::Status)
}

fn redirect_failure(redirects: &[Redirect], in_condition: bool) -> bool {
    !redirects.is_empty() && !in_condition
}

fn command_failure(kind: &BuiltinCmd, checked: bool, redirect: bool) -> Option<Failure> {
    let checked_result = match kind {
        BuiltinCmd::Fail => return Some(Failure::Explicit),
        BuiltinCmd::Other | BuiltinCmd::Echo | BuiltinCmd::Cd | BuiltinCmd::Close => checked,
        BuiltinCmd::Write
        | BuiltinCmd::NegateTestResult
        | BuiltinCmd::SetTestFromStatus
        | BuiltinCmd::SetTestFromPipe
        | BuiltinCmd::LetExit
        | BuiltinCmd::TestValue
        | BuiltinCmd::StoreBgPid
        | BuiltinCmd::TrapSignal
        | BuiltinCmd::SignalFlag
        | BuiltinCmd::SignalCheckpoint
        | BuiltinCmd::TableDelete
        | BuiltinCmd::TableDeleteIdx
        | BuiltinCmd::Test
        | BuiltinCmd::Noop
        | BuiltinCmd::Exit
        | BuiltinCmd::Exec
        | BuiltinCmd::Wait
        | BuiltinCmd::Attest
        | BuiltinCmd::Read
        | BuiltinCmd::Mapfile
        | BuiltinCmd::Argv
        | BuiltinCmd::Glob
        | BuiltinCmd::Rglob
        | BuiltinCmd::Break
        | BuiltinCmd::Continue
        | BuiltinCmd::Return
        | BuiltinCmd::Signal
        | BuiltinCmd::Unset
        | BuiltinCmd::Len
        | BuiltinCmd::Strlen
        | BuiltinCmd::Keys
        | BuiltinCmd::Slice
        | BuiltinCmd::Join
        | BuiltinCmd::Sub
        | BuiltinCmd::Gsub
        | BuiltinCmd::Match
        | BuiltinCmd::MatchAll
        | BuiltinCmd::Toupper
        | BuiltinCmd::Tolower
        | BuiltinCmd::Basename
        | BuiltinCmd::Dirname => false,
    };
    (checked_result || redirect).then_some(Failure::Guard)
}

pub(super) fn install(ssa: &mut SsaProgram) {
    for (entry, failed, inputs) in attempt_starts(ssa) {
        install_attempt(ssa, entry, failed, inputs);
    }
}

fn attempt_starts(ssa: &SsaProgram) -> Vec<(BlockId, BlockId, Vec<VarRef>)> {
    ssa.blocks
        .values()
        .flat_map(|block| {
            block.insts.iter().filter_map(move |inst| {
                if let SsaInst::AttemptStart { failed, inputs } = inst {
                    Some((block.id, *failed, inputs.clone()))
                } else {
                    None
                }
            })
        })
        .collect()
}

fn install_attempt(ssa: &mut SsaProgram, entry: BlockId, failed: BlockId, inputs: Vec<VarRef>) {
    for block in region(ssa, entry, failed) {
        split_checks(ssa, block, failed);
    }
    let sources = prefix_states(ssa, entry, failed, inputs);
    set_recovery_sources(ssa.block_mut(failed), &sources);
}

fn set_recovery_sources(block: &mut BasicBlock, sources: &GlobalState) {
    for inst in &mut block.insts {
        if let SsaInst::RecoverGlobal {
            target,
            sources: reaching,
        } = inst
        {
            *reaching = sources
                .get(&target.name)
                .into_iter()
                .flatten()
                .cloned()
                .collect();
            reaching.sort_by_key(|v| v.version);
        }
    }
}

fn region(ssa: &SsaProgram, entry: BlockId, failed: BlockId) -> Vec<BlockId> {
    let mut todo = vec![entry];
    let mut seen = HashSet::new();
    while let Some(id) = todo.pop() {
        if id != failed
            && seen.insert(id)
            && !ssa.blocks[&id]
                .insts
                .iter()
                .any(|i| matches!(i, SsaInst::AttemptEnd { .. }))
        {
            todo.extend(ssa.blocks[&id].terminator.successors());
        }
    }
    let mut blocks: Vec<_> = seen.into_iter().collect();
    blocks.sort_unstable();
    blocks
}

fn split_checks(ssa: &mut SsaProgram, mut id: BlockId, failed: BlockId) {
    loop {
        let block = &ssa.blocks[&id];
        let caught = condition_inst_index(block);
        let next = live_insts(block)
            .iter()
            .enumerate()
            .find_map(|(i, inst)| recoverable(inst, caught == Some(i)).map(|kind| (i, kind)));
        let Some((index, kind)) = next else { return };
        let normal = split_after(ssa, id, index);
        replace_failure_check(&mut ssa.block_mut(id).insts[index], kind);
        ssa.block_mut(id).terminator = Terminator::Checked {
            normal: (!matches!(kind, Failure::Explicit)).then_some(normal),
            failed,
            check_status: matches!(kind, Failure::Status),
        };
        id = normal;
    }
}

fn replace_failure_check(inst: &mut SsaInst, kind: Failure) {
    match kind {
        Failure::Explicit => {
            *inst = SsaInst::Assign {
                target: VarRef::new_typed("?", 0, VarType::Int),
                value: fail_status(inst),
            };
        }
        Failure::Status => match inst {
            SsaInst::Assign {
                value: SsaValue::CmdSubst { suppressed, .. },
                ..
            } => *suppressed = true,
            SsaInst::GlobFill { checked, .. } => *checked = false,
            _ => unreachable!("attempt status check requires a lifted status operation"),
        },
        Failure::Guard => {}
    }
}

fn fail_status(inst: &SsaInst) -> SsaValue {
    let SsaInst::Command { args, .. } = inst else {
        unreachable!("explicit attempt failure is a fail command")
    };
    match args.as_slice() {
        [] => SsaValue::Literal("1".into()),
        [value] => value.clone(),
        _ => unreachable!("validated fail has at most one status operand"),
    }
}

fn split_after(ssa: &mut SsaProgram, id: BlockId, index: usize) -> BlockId {
    let next = ssa.new_block("checked_continuation");
    let block = ssa.block_mut(id);
    let mut origin = block.origin;
    for inst in &block.insts[..=index] {
        if let SsaInst::SourceOrigin(at, _) = inst {
            origin = *at;
        }
    }
    let tail = block.insts.split_off(index + 1);
    let deferred = block
        .signal_deferred
        .iter()
        .filter(|i| **i > index)
        .map(|i| i - index - 1)
        .collect();
    block.signal_deferred.retain(|i| *i <= index);
    let terminator = std::mem::replace(&mut block.terminator, Terminator::Goto(next));
    for successor in terminator.successors() {
        ssa.block_mut(successor).replace_phi_predecessor(id, next);
    }
    let continuation = ssa.block_mut(next);
    continuation.insts = tail;
    continuation.terminator = terminator;
    continuation.origin = origin;
    continuation.signal_deferred = deferred;
    next
}

type GlobalState = HashMap<String, HashSet<VarRef>>;

fn merge_state(into: &mut GlobalState, from: &GlobalState) -> bool {
    let mut changed = false;
    for (name, values) in from {
        let here = into.entry(name.clone()).or_default();
        let before = here.len();
        here.extend(values.iter().cloned());
        changed |= here.len() != before;
    }
    changed
}

fn prefix_states(
    ssa: &SsaProgram,
    entry: BlockId,
    failed: BlockId,
    inputs: Vec<VarRef>,
) -> GlobalState {
    let initial: GlobalState = inputs
        .into_iter()
        .map(|v| (v.name.clone(), HashSet::from([v])))
        .collect();
    let mut incoming = HashMap::from([(entry, initial)]);
    let mut todo = vec![entry];
    let mut aborted = GlobalState::new();
    while let Some(id) = todo.pop() {
        let block = &ssa.blocks[&id];
        let mut state = incoming[&id].clone();
        if prefix_block(block, &mut state, &mut aborted) {
            propagate_state(block, failed, &state, &mut incoming, &mut todo);
        }
    }
    aborted
}

fn prefix_block(block: &BasicBlock, state: &mut GlobalState, aborted: &mut GlobalState) -> bool {
    for phi in &block.phis {
        update_state(state, &phi.target);
    }
    for (index, inst) in block.insts.iter().enumerate() {
        if ends_attempt_path(inst) {
            return false;
        }
        prefix_instruction(block, index, inst, state, aborted);
    }
    true
}

fn prefix_instruction(
    block: &BasicBlock,
    index: usize,
    inst: &SsaInst,
    state: &mut GlobalState,
    aborted: &mut GlobalState,
) {
    if block.checked_failure_at(index).is_some() {
        merge_state(aborted, state);
    }
    if let Some(target) = instruction_target(inst) {
        update_state(state, target);
    }
}

fn propagate_state(
    block: &BasicBlock,
    failed: BlockId,
    state: &GlobalState,
    incoming: &mut HashMap<BlockId, GlobalState>,
    todo: &mut Vec<BlockId>,
) {
    for next in block
        .terminator
        .successors()
        .into_iter()
        .filter(|next| *next != failed)
    {
        enqueue_state(next, state, incoming, todo);
    }
}

fn enqueue_state(
    next: BlockId,
    state: &GlobalState,
    incoming: &mut HashMap<BlockId, GlobalState>,
    todo: &mut Vec<BlockId>,
) {
    let changed = match incoming.entry(next) {
        std::collections::hash_map::Entry::Vacant(entry) => {
            entry.insert(state.clone());
            true
        }
        std::collections::hash_map::Entry::Occupied(mut entry) => {
            merge_state(entry.get_mut(), state)
        }
    };
    if changed {
        todo.push(next);
    }
}

fn update_state(state: &mut GlobalState, target: &VarRef) {
    if let Some(current) = state.get_mut(&target.name) {
        *current = HashSet::from([target.clone()]);
    }
}

fn ends_attempt_path(inst: &SsaInst) -> bool {
    matches!(
        inst,
        SsaInst::AttemptEnd { .. }
            | SsaInst::Command {
                kind: BuiltinCmd::Exit | BuiltinCmd::Fail,
                ..
            }
    )
}

pub(crate) struct AttemptSites {
    pub(crate) calls: HashSet<Span>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    classes: HashMap<String, FnClass>,
}

impl AttemptSites {
    pub(crate) fn new(program: &Program) -> Self {
        let known = program.function_defs().map(|f| f.name.clone()).collect();
        let mut sites = Self {
            calls: HashSet::new(),
            diagnostics: Vec::new(),
            classes: super::functions::static_classes(program, &known),
        };
        sites.block(&program.statements, false, false);
        for (kind, body) in program.handlers() {
            sites.block(body, kind == HandlerKind::Err, false);
        }
        sites
    }

    fn block(&mut self, body: &[Statement], owner: bool, consumed: bool) {
        for (i, stmt) in body.iter().enumerate() {
            self.statement(stmt, owner, consumed && i + 1 == body.len());
        }
    }

    fn command(&mut self, c: &Command, owner: bool, consumed: bool) {
        let name = crate::type_check::expr_to_name(&c.name);
        let class = self.classes.get(&name).copied();
        match (c.best_effort, owner, consumed, class) {
            (true, true, false, Some(FnClass::Void))
            | (false, true, true, Some(FnClass::Status)) => {
                self.calls.insert(c.name.span);
            }
            (true, _, _, _) => {
                self.diagnostics.push(Diagnostic::error_span(
                    c.name.span,
                    "SB5108",
                    "`: FUNCTION ...` is a resultless ERR-owned void-function attempt; \
                     use `_=` to evaluate and discard a value"
                        .to_string(),
                ));
            }
            (false, _, _, _) => {}
        }
    }

    fn capture_condition(&mut self, a: &Assignment, owner: bool, consumed: bool) -> bool {
        if !consumed {
            return false;
        }
        let ExprKind::CommandSubstitution(body) = &a.value.kind else {
            return false;
        };
        if !Self::status_capture(body, |name| self.classes.get(name).copied()) {
            return false;
        }
        self.block(body, owner, true);
        true
    }

    fn list(&mut self, l: &List, owner: bool, consumed: bool) {
        if l.op == ListOp::Background {
            self.statement(&l.left, false, false);
        } else {
            self.statement(&l.left, owner, true);
        }
        self.statement(&l.right, owner, consumed);
    }

    fn pipeline(&mut self, p: &Pipeline, owner: bool) {
        if let [stmt] = p.commands.as_slice() {
            self.statement(stmt, owner, true);
            return;
        }
        for (i, stmt) in p.commands.iter().enumerate() {
            let parent = i + 1 == p.commands.len() && stmt.binds_a_pipe_reader();
            self.statement(stmt, owner && parent, false);
        }
    }

    pub(crate) fn contains(&self, stmt: &Statement) -> bool {
        match &stmt.kind {
            StmtKind::Command(c) => self.calls.contains(&c.name.span),
            StmtKind::Redirected(r) => self.contains(&r.body),
            StmtKind::Assignment(a) => {
                let ExprKind::CommandSubstitution(body) = &a.value.kind else {
                    return false;
                };
                matches!(body.as_slice(), [call] if self.contains(call))
            }
            _ => false,
        }
    }

    pub(super) fn status_capture(
        body: &[Statement],
        class: impl Fn(&str) -> Option<FnClass>,
    ) -> bool {
        let [stmt] = body else {
            return false;
        };
        let (stmt, suppressed, negated) = stmt.stage_marks();
        let stmt = match &stmt.kind {
            StmtKind::Redirected(r) => r.body.as_ref(),
            _ => stmt,
        };
        let StmtKind::Command(c) = &stmt.kind else {
            return false;
        };
        !suppressed
            && !negated
            && class(&crate::type_check::expr_to_name(&c.name)) == Some(FnClass::Status)
    }

    /// Total over statement syntax; do not add a wildcard.
    fn statement(&mut self, stmt: &Statement, owner: bool, consumed: bool) {
        match &stmt.kind {
            StmtKind::Command(c) => self.command(c, owner, consumed),
            StmtKind::Assignment(a) if self.capture_condition(a, owner, consumed) => return,
            StmtKind::Assignment(_) | StmtKind::Declaration(_) | StmtKind::ArithTest(_) => {}
            StmtKind::Function(f) => self.block(&f.body, false, false),
            StmtKind::Pipeline(p) => self.pipeline(p, owner),
            StmtKind::List(l) => self.list(l, owner, consumed),
            StmtKind::Redirected(r) => self.statement(&r.body, owner, consumed),
            StmtKind::Suppressed(s) => self.statement(s, false, true),
            StmtKind::Negated(s) | StmtKind::ReturnProducer(s) => self.statement(s, owner, true),
            StmtKind::Group(g) => self.block(&g.body, owner, consumed),
            StmtKind::If(i) => self.if_statement(i, owner),
            StmtKind::While(w) => {
                self.block(&w.condition, owner, true);
                self.block(&w.body, owner, false);
            }
            StmtKind::For(f) => self.block(&f.body, owner, false),
            StmtKind::Case(c) => self.case_statement(c, owner),
        }
        for body in stmt.cmdsub_bodies() {
            self.block(body, false, false);
        }
    }

    fn case_statement(&mut self, c: &Case, owner: bool) {
        for arm in &c.items {
            self.block(&arm.body, owner, false);
        }
    }

    fn if_statement(&mut self, i: &If, owner: bool) {
        self.block(&i.condition, owner, true);
        self.block(&i.then_body, owner, false);
        for arm in &i.elif_clauses {
            self.block(&arm.condition, owner, true);
            self.block(&arm.body, owner, false);
        }
        if let Some(body) = &i.else_body {
            self.block(body, owner, false);
        }
    }
}
