//! Shared helpers for the transform module.

use crate::ssa::*;

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
