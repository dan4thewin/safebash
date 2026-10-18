//! Close reader scopes on control-flow edges, including nonlocal transfers.

use crate::ssa::*;
use std::collections::HashSet;

pub(super) fn install(ssa: &mut SsaProgram, regions: &[ReaderRegion]) {
    let mut regions: Vec<_> = regions
        .iter()
        .map(|region| (region, region_blocks(ssa, region)))
        .collect();
    regions.sort_by_key(|(region, _)| std::cmp::Reverse(region.site));
    let blocks: Vec<_> = ssa.blocks.keys().copied().collect();
    for from in blocks {
        install_block(ssa, &regions, from);
    }
}

fn install_block(
    ssa: &mut SsaProgram,
    regions: &[(&ReaderRegion, HashSet<BlockId>)],
    from: BlockId,
) {
    let mut targets = ssa.block(from).terminator.successors();
    targets.sort_unstable();
    targets.dedup();
    if targets.is_empty() {
        let finish = finishes(regions, from, None);
        ssa.block_mut(from).insts.extend(finish);
    } else {
        for target in targets {
            let finish = finishes(regions, from, Some(target));
            install_edge(ssa, from, target, finish);
        }
    }
}

fn region_blocks(ssa: &SsaProgram, region: &ReaderRegion) -> HashSet<BlockId> {
    let mut blocks = HashSet::new();
    let mut pending = vec![region.entry];
    while let Some(id) = pending.pop() {
        if region.candidates.contains(&id) && blocks.insert(id) {
            pending.extend(ssa.block(id).terminator.successors());
        }
    }
    blocks
}

fn finishes(
    regions: &[(&ReaderRegion, HashSet<BlockId>)],
    from: BlockId,
    target: Option<BlockId>,
) -> Vec<SsaInst> {
    regions
        .iter()
        .filter(|(_, blocks)| {
            blocks.contains(&from) && !target.is_some_and(|id| blocks.contains(&id))
        })
        .map(|(region, _)| SsaInst::PipelineFinish {
            site: region.site,
            checked: region.checked,
            preserve_status: target != Some(region.done),
            loop_consumer: region.loop_consumer,
        })
        .collect()
}

fn install_edge(ssa: &mut SsaProgram, from: BlockId, target: BlockId, finish: Vec<SsaInst>) {
    if finish.is_empty() {
        return;
    }
    if matches!(ssa.block(from).terminator, Terminator::Goto(_)) {
        ssa.block_mut(from).insts.extend(finish);
        return;
    }
    let bridge = ssa.new_block("pipe.finish");
    ssa.block_mut(bridge).insts = finish;
    ssa.block_mut(bridge).terminator = Terminator::Goto(target);
    redirect_edge(&mut ssa.block_mut(from).terminator, target, bridge);
    for (pred, _) in ssa
        .block_mut(target)
        .phis
        .iter_mut()
        .flat_map(|phi| &mut phi.sources)
        .filter(|(pred, _)| *pred == from)
    {
        *pred = bridge;
    }
}

fn redirect_edge(term: &mut Terminator, old: BlockId, new: BlockId) {
    let targets: &mut [&mut BlockId] = match term {
        Terminator::Branch {
            true_block,
            false_block,
            ..
        } => &mut [true_block, false_block],
        Terminator::Loop {
            body_block,
            exit_block,
            ..
        } => &mut [body_block, exit_block],
        Terminator::Goto(target) => &mut [target],
        Terminator::Return => &mut [],
    };
    for target in targets {
        if **target == old {
            **target = new;
        }
    }
}

#[cfg(test)]
mod invariant_tests {
    use super::*;

    #[test]
    fn conditional_cleanup_splits_edges_without_losing_phi_predecessors() {
        for is_loop in [false, true] {
            let mut ssa = SsaProgram::new();
            let entry = ssa.new_block("entry");
            let stay = ssa.new_block("stay");
            let done = ssa.new_block("done");
            let outside = ssa.new_block("outside");
            ssa.entry = entry;
            ssa.block_mut(entry).terminator = if is_loop {
                Terminator::Loop {
                    condition: "condition".into(),
                    body_block: stay,
                    exit_block: outside,
                }
            } else {
                Terminator::Branch {
                    condition: "condition".into(),
                    true_block: stay,
                    false_block: outside,
                }
            };
            ssa.block_mut(stay).terminator = Terminator::Goto(done);
            ssa.block_mut(done).terminator = Terminator::Goto(outside);
            ssa.block_mut(outside).phis.push(PhiNode {
                target: VarRef::new("value", 3),
                sources: vec![
                    (entry, VarRef::new("value", 1)),
                    (done, VarRef::new("value", 2)),
                ],
            });
            let inner = ReaderRegion {
                site: 1,
                entry,
                done: stay,
                candidates: HashSet::from([entry]),
                checked: false,
                loop_consumer: false,
            };
            let outer = ReaderRegion {
                site: 0,
                entry,
                done,
                candidates: HashSet::from([entry, stay]),
                checked: true,
                loop_consumer: true,
            };
            install(&mut ssa, &[outer, inner]);

            let edges = ssa.block(entry).terminator.successors();
            let normal = ssa.block(edges[0]);
            let escape = ssa.block(edges[1]);
            assert!(matches!(normal.terminator, Terminator::Goto(id) if id == stay));
            assert!(matches!(
                normal.insts.as_slice(),
                [SsaInst::PipelineFinish {
                    site: 1,
                    checked: false,
                    preserve_status: false,
                    loop_consumer: false
                }]
            ));
            assert!(matches!(escape.terminator, Terminator::Goto(id) if id == outside));
            assert!(matches!(
                escape.insts.as_slice(),
                [
                    SsaInst::PipelineFinish {
                        site: 1,
                        checked: false,
                        preserve_status: true,
                        loop_consumer: false
                    },
                    SsaInst::PipelineFinish {
                        site: 0,
                        checked: true,
                        preserve_status: true,
                        loop_consumer: true
                    }
                ]
            ));
            assert_eq!(
                ssa.block(outside).phis[0].sources,
                vec![
                    (escape.id, VarRef::new("value", 1)),
                    (done, VarRef::new("value", 2)),
                ]
            );
            assert!(matches!(
                ssa.block(stay).insts.as_slice(),
                [SsaInst::PipelineFinish {
                    site: 0,
                    checked: true,
                    preserve_status: false,
                    loop_consumer: true
                }]
            ));
            assert!(ssa.block(outside).insts.is_empty());
        }
    }

    #[test]
    fn edge_rewriting_handles_shared_targets_and_leaf_terminators() {
        let mut shared = Terminator::Branch {
            condition: "condition".into(),
            true_block: 1,
            false_block: 1,
        };
        redirect_edge(&mut shared, 1, 2);
        assert_eq!(shared.successors(), vec![2, 2]);
        let mut jump = Terminator::Goto(1);
        redirect_edge(&mut jump, 1, 2);
        assert!(matches!(jump, Terminator::Goto(2)));
        let mut leaf = Terminator::Return;
        redirect_edge(&mut leaf, 1, 2);
        assert!(matches!(leaf, Terminator::Return));
    }
}
