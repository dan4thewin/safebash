//! Shared types for codegen.
//!
//! TRUST: ExternFuncs declares every runtime function available to compiled
//! binaries. A reviewer should verify this matches sash-rt/src/lib.rs exactly.

use cranelift_codegen::ir::types::I64;
use cranelift_codegen::ir::{InstBuilder, StackSlotData, StackSlotKind, Value};
use cranelift_frontend::{FunctionBuilder, Variable};
use cranelift_module::Module;
use cranelift_object::ObjectModule;

#[derive(Debug, Clone, Copy)]
pub(super) enum VarSlot {
    Str { ptr: Variable, len: Variable },
    Int(Variable),
}

impl VarSlot {
    // INVARIANT: str_ptr/str_len are called only on slots the caller has
    // already established are strings — positionals, arrays, read/capture
    // targets, and the `VarSlot::Str` arm of a `match` on this enum (values.rs,
    // setup.rs, dispatch.rs). The Int arm is unreachable from those sites; a hit
    // means a codegen path built a slot of the wrong shape (a compiler bug, not
    // user-reachable input). Int-valued reads dispatch via `match *sv` instead,
    // so no `is_int()`-then-panicking-accessor dance survives.
    pub(super) fn str_ptr(&self) -> Variable {
        match self {
            VarSlot::Str { ptr, .. } => *ptr,
            VarSlot::Int(_) => panic!("ICE: expected string var, got int"),
        }
    }
    pub(super) fn str_len(&self) -> Variable {
        match self {
            VarSlot::Str { ptr: _, len } => *len,
            VarSlot::Int(_) => panic!("ICE: expected string var, got int"),
        }
    }
}

#[allow(dead_code)]
pub(super) struct ExternFuncs {
    pub(super) write: cranelift_module::FuncId,
    pub(super) exit: cranelift_module::FuncId,
    pub(super) emsg: cranelift_module::FuncId,
    pub(super) emsg_status: cranelift_module::FuncId,
    pub(super) attest_fail: cranelift_module::FuncId,
    pub(super) arith_overflow: cranelift_module::FuncId,
    pub(super) arith_divzero: cranelift_module::FuncId,
    pub(super) arith_negshift: cranelift_module::FuncId,
    pub(super) run: cranelift_module::FuncId,
    pub(super) getenv: cranelift_module::FuncId,
    pub(super) capture_drain: cranelift_module::FuncId,
    pub(super) strlen: cranelift_module::FuncId,
    pub(super) default_value: cranelift_module::FuncId,
    // Composable process primitives (Design Decision #14)
    pub(super) fork: cranelift_module::FuncId,
    pub(super) exec: cranelift_module::FuncId,
    pub(super) wait: cranelift_module::FuncId,
    pub(super) pipe: cranelift_module::FuncId,
    pub(super) close: cranelift_module::FuncId,
    pub(super) dup2: cranelift_module::FuncId,
    pub(super) dup: cranelift_module::FuncId,
    pub(super) redirect: cranelift_module::FuncId,
    pub(super) redirect_s: cranelift_module::FuncId,
    pub(super) read: cranelift_module::FuncId,
    pub(super) alloc: cranelift_module::FuncId,
    pub(super) concat: cranelift_module::FuncId,
    pub(super) atoi: cranelift_module::FuncId,
    pub(super) is_int: cranelift_module::FuncId,
    pub(super) is_print: cranelift_module::FuncId,
    pub(super) is_graph: cranelift_module::FuncId,
    pub(super) atoi_or: cranelift_module::FuncId,
    pub(super) atoi_or_abort: cranelift_module::FuncId,
    pub(super) itoa: cranelift_module::FuncId,
    pub(super) join_args: cranelift_module::FuncId,
    pub(super) assoc_lookup: cranelift_module::FuncId,
    pub(super) strcmp: cranelift_module::FuncId,
    pub(super) regex_match: cranelift_module::FuncId,
    pub(super) regex_replace: cranelift_module::FuncId,
    pub(super) regex_match_str: cranelift_module::FuncId,
    pub(super) regex_match_into: cranelift_module::FuncId,
    pub(super) regex_matchall_into: cranelift_module::FuncId,
    pub(super) argv_guard: cranelift_module::FuncId,
    pub(super) table_new: cranelift_module::FuncId,
    pub(super) table_set: cranelift_module::FuncId,
    pub(super) table_get: cranelift_module::FuncId,
    pub(super) table_len: cranelift_module::FuncId,
    pub(super) table_value: cranelift_module::FuncId,
    pub(super) table_key: cranelift_module::FuncId,
    pub(super) table_delete: cranelift_module::FuncId,
    pub(super) read_line: cranelift_module::FuncId,
    pub(super) splice_argv: cranelift_module::FuncId,
    pub(super) file_test: cranelift_module::FuncId,
    pub(super) pow: cranelift_module::FuncId,
    pub(super) substring: cranelift_module::FuncId,
    pub(super) isatty: cranelift_module::FuncId,
    pub(super) file_newer: cranelift_module::FuncId,
    pub(super) file_same_dev: cranelift_module::FuncId,
    pub(super) chdir: cranelift_module::FuncId,
    pub(super) getcwd: cranelift_module::FuncId,
    pub(super) slurp_file: cranelift_module::FuncId,
    pub(super) toupper_all: cranelift_module::FuncId,
    pub(super) tolower_all: cranelift_module::FuncId,
    pub(super) getpid: cranelift_module::FuncId,
    pub(super) getppid: cranelift_module::FuncId,
    pub(super) random: cranelift_module::FuncId,
    pub(super) epochseconds: cranelift_module::FuncId,
    pub(super) trap: cranelift_module::FuncId,
    pub(super) glob: cranelift_module::FuncId,
    pub(super) capture_start: cranelift_module::FuncId,
    pub(super) capture_end: cranelift_module::FuncId,
    pub(super) capture_start_buf: cranelift_module::FuncId,
    pub(super) capture_end_buf: cranelift_module::FuncId,
    pub(super) split_into_array: cranelift_module::FuncId,
    pub(super) map_from_array: cranelift_module::FuncId,
    pub(super) read_all: cranelift_module::FuncId,
    pub(super) envp: cranelift_module::DataId,
    pub(super) argc: cranelift_module::DataId,
    pub(super) argv: cranelift_module::DataId,
    pub(super) argv_get: cranelift_module::FuncId,
    pub(super) argv_get_or: cranelift_module::FuncId,
    pub(super) argv_snapshot: cranelift_module::FuncId,
    pub(super) splat_disposition: cranelift_module::FuncId,
    pub(super) require_value: cranelift_module::FuncId,
    pub(super) signal_flag: cranelift_module::FuncId,
    pub(super) signum: cranelift_module::DataId,
    pub(super) trace_flag: cranelift_module::DataId,
}

/// Shared context for instruction code generation.
///
/// Groups the many parameters that every instruction handler needs,
/// reducing the call signature from 23 params to 4: (inst, builder, module, ctx).
pub(super) struct InstCtx<'a> {
    pub(super) vars: &'a mut std::collections::HashMap<String, VarSlot>,
    pub(super) literals: &'a std::collections::HashMap<String, cranelift_module::DataId>,
    pub(super) externs: &'a ExternFuncs,
    pub(super) space_id: cranelift_module::DataId,
    pub(super) nl_id: cranelift_module::DataId,
    pub(super) test_result_var: cranelift_frontend::Variable,
    pub(super) exit_status_var: cranelift_frontend::Variable,
    pub(super) pipe_read_fd_var: cranelift_frontend::Variable,
    /// One `(saved_stdout, capture_handle)` slot pair per capture site, indexed
    /// by `SsaInst::CaptureStart { site }`. A single shared pair let a nested
    /// capture clobber the enclosing one.
    pub(super) capture_slots: &'a [(cranelift_frontend::Variable, cranelift_frontend::Variable)],
    /// One saved-fd slot per redirect, per `SsaInst::RedirectPush { site }`
    /// scope. Indexed by site, then by position in that scope's redirect
    /// list; the paired `RedirectPop` restores from the same slots. Scopes
    /// nest, so — as with captures — a single shared set would let an inner
    /// `f > a` destroy the fds an enclosing one still has to put back.
    pub(super) redirect_slots: &'a [Vec<cranelift_frontend::Variable>],
    pub(super) end_cl_block: Option<cranelift_codegen::ir::Block>,
    pub(super) program: &'a crate::ssa::SsaProgram,
    pub(super) ssa_id: usize,
    pub(super) saved_end_status_var: cranelift_frontend::Variable,
    pub(super) cl_blocks: &'a std::collections::HashMap<usize, cranelift_codegen::ir::Block>,
    /// DD#55: true when this command is the condition of a catching
    /// construct (last Command/Pipeline of a `Branch`-terminated block).
    /// Distinct from `checked` (which `-` suppression also clears); used
    /// so a redirect open(2) setup-failure is caught by `if`/`||` but
    /// still aborts under bare/`-`.
    pub(super) in_condition: bool,
    /// DD#56: true when this instruction is part of the END handler —
    /// `end_block` or anything reachable from it. An abort from here would
    /// route *through* END, which from inside END is a jump to the block
    /// already executing, so a failure diagnoses and lets the rest of the
    /// cleanup run instead. See `SsaProgram::end_region`.
    pub(super) in_end_block: bool,
}

/// Per-function compilation state, built during variable allocation.
pub(super) struct FuncState {
    pub(super) cl_blocks: std::collections::HashMap<usize, cranelift_codegen::ir::Block>,
    pub(super) vars: std::collections::HashMap<String, VarSlot>,
    pub(super) test_result_var: cranelift_frontend::Variable,
    pub(super) exit_status_var: cranelift_frontend::Variable,
    pub(super) pipe_read_fd_var: cranelift_frontend::Variable,
    pub(super) capture_slots: Vec<(cranelift_frontend::Variable, cranelift_frontend::Variable)>,
    pub(super) redirect_slots: Vec<Vec<cranelift_frontend::Variable>>,
    pub(super) saved_end_status_var: cranelift_frontend::Variable,
    pub(super) end_cl_block: Option<cranelift_codegen::ir::Block>,
}

/// Look up an array/map slot by base name. Tables live in `_0` slots
/// (they're mutated in place, not re-versioned like scalars in SSA);
/// the fallback to any matching `<name>_*` key is defensive.
///
/// Panics with an ICE message including `context` (e.g. `"array"` or
/// `"array in test"`) when the array is undefined.
pub(super) fn find_array_slot<'a>(
    vars: &'a std::collections::HashMap<String, VarSlot>,
    name: &str,
    context: &str,
) -> &'a VarSlot {
    let primary = format!("{}_0", name);
    if let Some(sv) = vars.get(&primary) {
        return sv;
    }
    let prefix = format!("{}_", name);
    vars.keys()
        .find(|k| k.starts_with(&prefix))
        .and_then(|k| vars.get(k))
        .unwrap_or_else(|| panic!("ICE: undefined {}: {}", context, name))
}

/// Lower a 64-bit integer Value into a (ptr, len) string pair via `sash_itoa`.
///
/// Every site that needs to produce a string representation of an integer
/// (echo of `$?`, `$$`, `${#arr[@]}`, arithmetic-expression result, etc.)
/// allocates the same 8-byte stack slot for the out-length, calls `sash_itoa`,
/// and reads back the returned pointer plus the slot. This helper centralises
/// that boilerplate so the call sites stay readable.
pub(super) fn emit_int_to_str(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    int_val: Value,
) -> (Value, Value) {
    let out_slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
    let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
    let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
    let call = builder.ins().call(itoa_ref, &[int_val, out_addr]);
    let ptr = builder.inst_results(call)[0];
    let len = builder.ins().stack_load(I64, out_slot, 0);
    (ptr, len)
}
