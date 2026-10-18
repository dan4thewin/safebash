//! Shared helpers for the transform module.

use crate::ssa::*;

/// All header phi names survive pass two: restores start from the merged state.
pub(super) fn update_loop_phi_sources(
    ssa: &mut SsaProgram,
    header: BlockId,
    body: &std::collections::HashMap<String, u32>,
) {
    for phi in &mut ssa.block_mut(header).phis {
        let name = &phi.target.name;
        let Some(version) = body.get(name) else {
            unreachable!("the second loop pass must retain every header phi variable")
        };
        phi.sources[1].1 = VarRef::new(name, *version);
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
    let all_vars: std::collections::HashSet<&String> =
        state_a.keys().chain(state_b.keys()).collect();

    for var in all_vars {
        let ver_a = state_a.get(var).copied().unwrap_or(0);
        let ver_b = state_b.get(var).copied().unwrap_or(0);
        if ver_a != ver_b {
            let target = state.define(var);
            ssa.block_mut(merge).phis.push(PhiNode {
                target,
                sources: vec![
                    (block_a, VarRef::new(var, ver_a)),
                    (block_b, VarRef::new(var, ver_b)),
                ],
            });
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
        update_loop_phi_sources(&mut ssa, header, &std::collections::HashMap::new());
    }
}
