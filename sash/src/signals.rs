//! DD#128: semantic adjacency supplies pairs; the executable CFG supplies paths.

use crate::ir::Span;
use crate::ssa::*;
use crate::ssa_taint::{classify_main, instruction_target, Classification};
use crate::terminal::{live_blocks, live_insts};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

type Point = (BlockId, usize);

pub(crate) fn prepare(program: &mut SsaProgram, pairs: &HashSet<(Span, Span)>) {
    if program.exiting_signals.is_empty() {
        return;
    }
    let graph = Graph::new(program);
    let classified = classify_main(program);
    let guards = collect_guards(program, &graph, pairs, &classified);
    let plan = protection(program, &graph, &guards, &classified);
    install(program, &graph, &plan);
}

struct Node {
    point: Point,
    origin: Option<usize>,
    next: Vec<usize>,
    previous: Vec<usize>,
}

struct Graph {
    nodes: Vec<Node>,
    entry: usize,
    points: HashMap<Point, usize>,
}

impl Graph {
    fn new(program: &SsaProgram) -> Self {
        let mut graph = Self {
            nodes: Vec::new(),
            entry: 0,
            points: HashMap::new(),
        };
        for block in live_blocks(program, program.entry) {
            graph.push_block(program.block(block));
        }
        graph.entry = graph.points[&(program.entry, 0)];
        graph.connect(program);
        graph
    }

    fn push_block(&mut self, body: &BasicBlock) {
        let mut origin = body.origin;
        for (index, inst) in live_insts(body).iter().enumerate() {
            if let SsaInst::SourceOrigin(next, _) = inst {
                origin = *next;
            }
            self.push((body.id, index), origin);
        }
        self.push((body.id, live_insts(body).len()), origin);
    }

    fn push(&mut self, point: Point, origin: Option<usize>) {
        self.points.insert(point, self.nodes.len());
        self.nodes.push(Node {
            point,
            origin,
            next: Vec::new(),
            previous: Vec::new(),
        });
    }

    fn connect(&mut self, program: &SsaProgram) {
        for id in 0..self.nodes.len() {
            let next = self.successors(program, id);
            for &child in &next {
                self.nodes[child].previous.push(id);
            }
            self.nodes[id].next = next;
        }
    }

    fn successors(&self, program: &SsaProgram, node: usize) -> Vec<usize> {
        let (block, index) = self.nodes[node].point;
        let body = program.block(block);
        match live_insts(body).get(index) {
            Some(SsaInst::Command {
                kind: BuiltinCmd::Exit | BuiltinCmd::Fail,
                ..
            }) => Vec::new(),
            Some(_) => vec![self.points[&(block, index + 1)]],
            None if matches!(
                live_insts(body).last(),
                Some(SsaInst::Command {
                    kind: BuiltinCmd::Exit | BuiltinCmd::Fail,
                    ..
                })
            ) =>
            {
                Vec::new()
            }
            None => body
                .terminator
                .successors()
                .into_iter()
                .map(|next| self.points[&(next, 0)])
                .collect(),
        }
    }

    fn inst<'a>(&self, program: &'a SsaProgram, node: usize) -> Option<&'a SsaInst> {
        let (block, index) = self.nodes[node].point;
        live_insts(program.block(block)).get(index)
    }
}

struct Guard {
    name: String,
    scope: usize,
    starts: HashSet<usize>,
    sources: HashSet<usize>,
    checks: HashSet<usize>,
    waiting: HashSet<usize>,
}

fn collect_guards(
    program: &SsaProgram,
    graph: &Graph,
    pairs: &HashSet<(Span, Span)>,
    classified: &Classification,
) -> Vec<Guard> {
    let mut guards = Vec::new();
    for node in 0..graph.nodes.len() {
        let Some(SsaInst::Attest { source, .. } | SsaInst::AttestRetag { source, .. }) =
            graph.inst(program, node)
        else {
            continue;
        };
        let Some(scope) = graph.nodes[node].origin else {
            continue;
        };
        let Some(attest) = program.origins[scope].attest else {
            continue;
        };
        let spans: HashSet<_> = pairs
            .iter()
            .filter_map(|(source, target)| (*target == attest).then_some(*source))
            .collect();
        if spans.is_empty() {
            continue;
        }
        let candidates = source_starts(program, graph, scope, &source.name, &spans, classified);
        let checks = check_nodes(program, graph, scope, node);
        let waiting = waiting_region(graph, &checks, &candidates);
        let starts: HashSet<_> = candidates
            .keys()
            .filter(|node| waiting.contains(node))
            .copied()
            .collect();
        let sources = starts.iter().map(|node| candidates[node]).collect();
        guards.push(Guard {
            name: source.name.clone(),
            scope,
            starts,
            sources,
            checks,
            waiting,
        });
    }
    guards
}

fn source_starts(
    program: &SsaProgram,
    graph: &Graph,
    scope: usize,
    name: &str,
    spans: &HashSet<Span>,
    classified: &Classification,
) -> HashMap<usize, usize> {
    let mut starts = HashMap::new();
    for node in 0..graph.nodes.len() {
        let Some(origin) = source_origin(program, graph, node, spans) else {
            continue;
        };
        if program.origins[origin].for_binding && within(program, Some(scope), origin) {
            loop_binding_starts(program, graph, origin, name, classified, &mut starts);
            continue;
        }
        starts.insert(node, origin);
    }
    starts
}

fn source_origin(
    program: &SsaProgram,
    graph: &Graph,
    node: usize,
    spans: &HashSet<Span>,
) -> Option<usize> {
    let SsaInst::SourceOrigin(Some(origin), true) = graph.inst(program, node)? else {
        return None;
    };
    spans
        .contains(&program.origins[*origin].span)
        .then_some(*origin)
}

fn loop_binding_starts(
    program: &SsaProgram,
    graph: &Graph,
    origin: usize,
    name: &str,
    classified: &Classification,
    starts: &mut HashMap<usize, usize>,
) {
    for node in 0..graph.nodes.len() {
        let Some(inst) = graph.inst(program, node) else {
            continue;
        };
        if within(program, graph.nodes[node].origin, origin)
            && classified
                .acquisition(inst)
                .is_some_and(|target| target.name == name)
        {
            starts.insert(node, origin);
        }
    }
}

fn check_nodes(program: &SsaProgram, graph: &Graph, scope: usize, retag: usize) -> HashSet<usize> {
    let mut checks: HashSet<_> = graph
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(node, point)| {
            (point.origin == Some(scope)
                && matches!(
                    graph.inst(program, node),
                    Some(
                        SsaInst::Test(_, TestOrigin::AttestPredicate)
                            | SsaInst::AttestAll { .. }
                            | SsaInst::Attest { .. }
                    )
                ))
            .then_some(node)
        })
        .collect();
    checks.insert(retag);
    checks
}

fn waiting_region(
    graph: &Graph,
    checks: &HashSet<usize>,
    starts: &HashMap<usize, usize>,
) -> HashSet<usize> {
    let mut pending: Vec<_> = checks.iter().copied().collect();
    let mut seen = HashSet::new();
    while let Some(node) = pending.pop() {
        if !seen.insert(node) || starts.contains_key(&node) {
            continue;
        }
        pending.extend(&graph.nodes[node].previous);
    }
    seen
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

#[derive(Clone, Default, PartialEq, Eq)]
struct Active {
    waiting: BTreeSet<usize>,
    resolving: BTreeSet<usize>,
}

impl Active {
    fn deferred(&self) -> bool {
        !self.waiting.is_empty() || !self.resolving.is_empty()
    }

    fn extend(&mut self, other: &Self) {
        self.waiting.extend(&other.waiting);
        self.resolving.extend(&other.resolving);
    }

    fn prune(&mut self, node: usize, graph: &Graph, guards: &[Guard], program: &SsaProgram) {
        let origin = graph.nodes[node].origin;
        self.waiting.retain(|id| {
            guards[*id].waiting.contains(&node)
                || guards[*id]
                    .sources
                    .iter()
                    .any(|source| within(program, origin, *source))
        });
        self.resolving
            .retain(|id| within(program, origin, guards[*id].scope));
    }

    fn resolve(&mut self, name: &str, guards: &[Guard]) {
        self.waiting.retain(|id| guards[*id].name != name);
        self.resolving.retain(|id| guards[*id].name != name);
    }

    fn begin_resolution(&mut self, node: usize, guards: &[Guard]) {
        for (id, guard) in guards
            .iter()
            .enumerate()
            .filter(|(_, g)| g.checks.contains(&node))
        {
            if self.waiting.remove(&id) {
                self.resolving.insert(id);
            }
            debug_assert!(guard.waiting.contains(&node));
        }
    }

    fn propagate(
        &self,
        next: &[usize],
        states: &mut [Option<Self>],
        pending: &mut VecDeque<usize>,
    ) {
        for &next in next {
            let previous = states[next].clone();
            states[next].get_or_insert_default().extend(self);
            if states[next] != previous {
                pending.push_back(next);
            }
        }
    }
}

#[derive(Clone, Default)]
struct PollPlan {
    before: Option<bool>,
    deferred: bool,
}

fn protection(
    program: &SsaProgram,
    graph: &Graph,
    guards: &[Guard],
    classified: &Classification,
) -> Vec<PollPlan> {
    let mut states = vec![None; graph.nodes.len()];
    states[graph.entry] = Some(Active::default());
    let mut pending = VecDeque::from([graph.entry]);
    let mut plan = vec![PollPlan::default(); graph.nodes.len()];
    while let Some(node) = pending.pop_front() {
        let mut active = states[node]
            .clone()
            .expect("a queued point has an input state");
        plan[node] = transfer(node, &mut active, program, graph, guards, classified);
        active.propagate(&graph.nodes[node].next, &mut states, &mut pending);
    }
    plan
}

fn transfer(
    node: usize,
    active: &mut Active,
    program: &SsaProgram,
    graph: &Graph,
    guards: &[Guard],
    classified: &Classification,
) -> PollPlan {
    let inst = graph.inst(program, node);
    let metadata = matches!(inst, Some(SsaInst::SourceOrigin(..)));
    let starts = starting_guards(node, inst, program, graph, guards, classified);
    if !metadata || !starts.is_empty() {
        active.prune(node, graph, guards, program);
    }
    let mut plan = PollPlan::default();
    if !starts.is_empty() {
        plan.before = Some(active.deferred());
        active.waiting.extend(starts);
    }
    active.begin_resolution(node, guards);
    plan.deferred = active.deferred();
    if let Some(name) = resolved_name(inst, program, classified) {
        active.resolve(name, guards);
    }
    plan
}

fn starting_guards(
    node: usize,
    inst: Option<&SsaInst>,
    program: &SsaProgram,
    graph: &Graph,
    guards: &[Guard],
    classified: &Classification,
) -> Vec<usize> {
    let acquired = inst.and_then(|inst| classified.acquisition(inst));
    guards
        .iter()
        .enumerate()
        .filter_map(|(id, guard)| {
            let reacquired = acquired.is_some_and(|target| {
                target.name == guard.name
                    && guard
                        .sources
                        .iter()
                        .any(|source| within(program, graph.nodes[node].origin, *source))
            });
            (guard.starts.contains(&node) || reacquired).then_some(id)
        })
        .collect()
}

fn resolved_name<'a>(
    inst: Option<&'a SsaInst>,
    program: &SsaProgram,
    classified: &Classification,
) -> Option<&'a str> {
    let inst = inst?;
    match inst {
        SsaInst::Attest { target, .. } | SsaInst::AttestRetag { target, .. } => Some(&target.name),
        _ => instruction_target(inst)
            .filter(|target| program.resolutions.contains(*target) && !classified.tainted(target))
            .map(|target| target.name.as_str()),
    }
}

fn install(program: &mut SsaProgram, graph: &Graph, plan: &[PollPlan]) {
    let blocks: BTreeSet<_> = graph.nodes.iter().map(|node| node.point.0).collect();
    for block in blocks {
        let body = program.block_mut(block);
        let original = std::mem::take(&mut body.insts);
        for (index, inst) in original.into_iter().enumerate() {
            let Some(&node) = graph.points.get(&(block, index)) else {
                body.insts.push(inst);
                continue;
            };
            install_instruction(body, inst, &plan[node]);
        }
    }
}

fn install_instruction(body: &mut BasicBlock, mut inst: SsaInst, plan: &PollPlan) {
    let enters = matches!(inst, SsaInst::SourceOrigin(_, true));
    if !enters {
        append_poll(&mut body.insts, plan.before);
    }
    if matches!(
        inst,
        SsaInst::Command {
            kind: BuiltinCmd::SignalCheckpoint,
            ..
        }
    ) {
        inst = SsaInst::signal_checkpoint(plan.deferred);
    }
    if plan.deferred {
        body.signal_deferred.insert(body.insts.len());
    }
    body.insts.push(inst);
    if enters {
        append_poll(&mut body.insts, plan.before);
    }
}

fn append_poll(insts: &mut Vec<SsaInst>, deferred: Option<bool>) {
    if let Some(deferred) = deferred {
        insts.push(SsaInst::signal_checkpoint(deferred));
    }
}

/// Logical positions of the direct backend polls; redirect guards retain ownership.
pub(crate) fn direct_polls(inst: &SsaInst) -> (bool, bool) {
    match inst {
        SsaInst::Command {
            kind: BuiltinCmd::Other,
            ..
        }
        | SsaInst::Pipeline { .. } => (true, true),
        SsaInst::Command {
            kind: BuiltinCmd::Exec,
            args,
            ..
        } => (!args.is_empty(), false),
        SsaInst::PipelineFinish { .. } => (false, true),
        _ => (false, false),
    }
}
