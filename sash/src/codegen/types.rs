//! Shared types for codegen.
//!
//! TRUST: ExternFuncs declares every runtime function available to compiled
//! binaries. A reviewer should verify this matches sash-rt/src/lib.rs exactly.

use cranelift_frontend::Variable;

#[derive(Debug, Clone, Copy)]
pub(super) enum VarSlot {
    Str { ptr: Variable, len: Variable },
    Int(Variable),
}

impl VarSlot {
    #[allow(dead_code)]
    pub(super) fn as_str(&self) -> (&Variable, &Variable) {
        match self {
            VarSlot::Str { ptr, len } => (ptr, len),
            VarSlot::Int(_) => panic!("ICE: expected string var, got int"),
        }
    }
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
    pub(super) fn as_int(&self) -> &Variable {
        match self {
            VarSlot::Int(v) => v,
            VarSlot::Str { .. } => panic!("ICE: expected int var, got string"),
        }
    }
    pub(super) fn is_int(&self) -> bool {
        matches!(self, VarSlot::Int(_))
    }
}

#[allow(dead_code)]
pub(super) struct ExternFuncs {
    pub(super) write: cranelift_module::FuncId,
    pub(super) exit: cranelift_module::FuncId,
    pub(super) run: cranelift_module::FuncId,
    pub(super) getenv: cranelift_module::FuncId,
    #[allow(dead_code)]
    pub(super) capture: cranelift_module::FuncId,
    pub(super) strlen: cranelift_module::FuncId,
    pub(super) strip_suffix: cranelift_module::FuncId,
    pub(super) strip_prefix: cranelift_module::FuncId,
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
    pub(super) itoa: cranelift_module::FuncId,
    pub(super) join_args: cranelift_module::FuncId,
    pub(super) assoc_lookup: cranelift_module::FuncId,
    pub(super) strcmp: cranelift_module::FuncId,
    pub(super) regex_match: cranelift_module::FuncId,
    pub(super) regex_replace: cranelift_module::FuncId,
    pub(super) table_new: cranelift_module::FuncId,
    pub(super) table_set: cranelift_module::FuncId,
    pub(super) table_get: cranelift_module::FuncId,
    pub(super) table_len: cranelift_module::FuncId,
    pub(super) table_value: cranelift_module::FuncId,
    pub(super) table_key: cranelift_module::FuncId,
    pub(super) table_delete: cranelift_module::FuncId,
    pub(super) read_line: cranelift_module::FuncId,
    pub(super) read_delim: cranelift_module::FuncId,
    pub(super) splice_argv: cranelift_module::FuncId,
    pub(super) file_test: cranelift_module::FuncId,
    pub(super) pow: cranelift_module::FuncId,
    pub(super) substring: cranelift_module::FuncId,
    pub(super) isatty: cranelift_module::FuncId,
    pub(super) file_newer: cranelift_module::FuncId,
    pub(super) file_same_dev: cranelift_module::FuncId,
    pub(super) chdir: cranelift_module::FuncId,
    pub(super) getcwd: cranelift_module::FuncId,
    pub(super) heredoc_redirect: cranelift_module::FuncId,
    pub(super) toupper_first: cranelift_module::FuncId,
    pub(super) tolower_first: cranelift_module::FuncId,
    pub(super) toupper_all: cranelift_module::FuncId,
    pub(super) tolower_all: cranelift_module::FuncId,
    pub(super) getpid: cranelift_module::FuncId,
    pub(super) getppid: cranelift_module::FuncId,
    pub(super) epochseconds: cranelift_module::FuncId,
    pub(super) trap: cranelift_module::FuncId,
    pub(super) glob: cranelift_module::FuncId,
    pub(super) glob_regex: cranelift_module::FuncId,
    pub(super) capture_start: cranelift_module::FuncId,
    pub(super) capture_end: cranelift_module::FuncId,
    pub(super) fnmatch: cranelift_module::FuncId,
    pub(super) split_first_word: cranelift_module::FuncId,
    pub(super) split_field: cranelift_module::FuncId,
    pub(super) split_into_array: cranelift_module::FuncId,
    pub(super) read_all: cranelift_module::FuncId,
    pub(super) envp: cranelift_module::DataId,
    pub(super) argc: cranelift_module::DataId,
    pub(super) argv: cranelift_module::DataId,
    pub(super) argv_get: cranelift_module::FuncId,
    pub(super) signal_flag: cranelift_module::FuncId,
    pub(super) dtoi: cranelift_module::FuncId,
    pub(super) itod: cranelift_module::FuncId,
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
    pub(super) capture_saved_var: cranelift_frontend::Variable,
    pub(super) capture_pipe_var: cranelift_frontend::Variable,
    pub(super) func_constants: &'a std::collections::HashMap<String, String>,
    pub(super) redirect_stack: &'a mut Vec<Vec<super::redirects::SavedFd>>,
    pub(super) end_cl_block: Option<cranelift_codegen::ir::Block>,
    pub(super) program: &'a crate::ssa::SsaProgram,
    pub(super) ssa_id: usize,
    pub(super) saved_end_status_var: cranelift_frontend::Variable,
    pub(super) cl_blocks: &'a std::collections::HashMap<usize, cranelift_codegen::ir::Block>,
    pub(super) trace: bool,
}
