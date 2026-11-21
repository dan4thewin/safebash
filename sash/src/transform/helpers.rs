//! Shared helpers for the transform module.

use crate::ssa::*;

/// All header phi names survive pass two: restores start from the merged state.
pub(super) fn update_loop_phi_sources(
    ssa: &mut SsaProgram,
    header: BlockId,
    back_edge: BlockId,
    body: &std::collections::HashMap<String, u32>,
    jumps: &[LoopJump],
) {
    for phi in &mut ssa.block_mut(header).phis {
        let name = &phi.target.name;
        let Some(version) = body.get(name) else {
            unreachable!("the second loop pass must retain every header phi variable")
        };
        phi.sources[1] = (back_edge, VarRef::new(name, *version));
        for jump in jumps {
            let version = jump
                .versions
                .get(name)
                .expect("a loop jump must retain every header phi variable");
            phi.sources
                .push((jump.predecessor, VarRef::new(name, *version)));
        }
    }
}

pub(super) fn insert_phis_two_branch(
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    merge: BlockId,
    block_a: BlockId,
    state_a: &std::collections::HashMap<String, u32>,
    block_b: BlockId,
    state_b: &std::collections::HashMap<String, u32>,
) {
    insert_phis(state, ssa, merge, &[(block_a, state_a), (block_b, state_b)]);
}

pub(super) fn merge_loop_jumps(
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    merge: BlockId,
    normal: BlockId,
    versions: &std::collections::HashMap<String, u32>,
) {
    state.restore(versions);
    let Some(mut jumps) = state.loop_jumps.remove(&merge) else {
        return;
    };
    jumps.push(LoopJump {
        predecessor: normal,
        versions: versions.clone(),
    });
    let sources: Vec<_> = jumps
        .iter()
        .map(|jump| (jump.predecessor, &jump.versions))
        .collect();
    insert_phis(state, ssa, merge, &sources);
}

fn insert_phis(
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    merge: BlockId,
    incoming: &[(BlockId, &std::collections::HashMap<String, u32>)],
) {
    let all_vars: std::collections::HashSet<&String> =
        incoming.iter().flat_map(|(_, vars)| vars.keys()).collect();

    for var in all_vars {
        let sources: Vec<_> = incoming
            .iter()
            .map(|(block, vars)| {
                (
                    *block,
                    VarRef::new(var, vars.get(var).copied().unwrap_or(0)),
                )
            })
            .collect();
        if sources.iter().any(|(_, value)| value != &sources[0].1) {
            let target = state.define_merge(var);
            ssa.block_mut(merge).phis.push(PhiNode { target, sources });
        }
    }
}

#[cfg(test)]
mod invariant_tests {
    use super::*;

    #[test]
    #[should_panic(expected = "the second loop pass must retain every header phi variable")]
    fn a_missing_back_edge_version_cannot_keep_a_discarded_pass_one_value() {
        let mut ssa = SsaProgram::new();
        let entry = ssa.new_block("entry");
        let header = ssa.new_block("header");
        let latch = ssa.new_block("latch");
        ssa.block_mut(header).phis.push(PhiNode {
            target: VarRef::new("value", 3),
            sources: vec![
                (entry, VarRef::new("value", 1)),
                (latch, VarRef::new("value", 2)),
            ],
        });
        update_loop_phi_sources(
            &mut ssa,
            header,
            latch,
            &std::collections::HashMap::new(),
            &[],
        );
    }
}
