#![allow(clippy::doc_markdown, clippy::doc_lazy_continuation)]
#![allow(clippy::too_many_arguments)]
//! Builtin command code generation: echo, exec.
//!
//! TRUST: lower_exec is the path to fork/exec. Command names are SSA
//! literals resolved at compile time. No shell interpretation.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types::I64;
use cranelift_codegen::ir::MemFlags;
use cranelift_codegen::ir::{InstBuilder, StackSlotData, StackSlotKind};
use cranelift_frontend::{FunctionBuilder, Variable};
use cranelift_module::Module;
use cranelift_object::ObjectModule;
use std::collections::HashMap;

use super::types::{ExternFuncs, VarSlot};
use super::values::lower_value;
use crate::ssa::*;

pub(super) fn lower_echo(
    args: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    space_id: cranelift_module::DataId,
    nl_id: cranelift_module::DataId,
    #[allow(unused_variables)] exit_status_var: Option<Variable>,
) {
    let fd = builder.ins().iconst(I64, 1);
    let write_ref = module.declare_func_in_func(externs.write, builder.func);
    let empty_fc = std::collections::HashMap::new();

    // Check for -n flag (suppress trailing newline)
    let (print_args, suppress_newline) = if let Some(SsaValue::Literal(flag)) = args.first() {
        if flag == "-n" {
            (&args[1..], true)
        } else {
            (args, false)
        }
    } else {
        (args, false)
    };

    for (i, arg) in print_args.iter().enumerate() {
        if i > 0 {
            let sp_gv = module.declare_data_in_func(space_id, builder.func);
            let sp_ptr = builder.ins().global_value(I64, sp_gv);
            let sp_len = builder.ins().iconst(I64, 1);
            builder.ins().call(write_ref, &[fd, sp_ptr, sp_len]);
        }
        // For Concat values, write each part sequentially (no spaces)
        if let SsaValue::Concat(parts) = arg {
            for part in parts {
                let (ptr, len) = lower_value(
                    part,
                    builder,
                    module,
                    vars,
                    literals,
                    &empty_fc,
                    externs,
                    exit_status_var,
                );
                builder.ins().call(write_ref, &[fd, ptr, len]);
            }
        } else {
            let (ptr, len) = lower_value(
                arg,
                builder,
                module,
                vars,
                literals,
                &empty_fc,
                externs,
                exit_status_var,
            );
            builder.ins().call(write_ref, &[fd, ptr, len]);
        }
    }

    if !suppress_newline {
        let nl_gv = module.declare_data_in_func(nl_id, builder.func);
        let nl_ptr = builder.ins().global_value(I64, nl_gv);
        let nl_len = builder.ins().iconst(I64, 1);
        builder.ins().call(write_ref, &[fd, nl_ptr, nl_len]);
    }
}

/// Emit fork/exec for an external command.
///
/// TRUST: The command name comes from SSA, which was:
/// 1. Validated by command_resolution (SB5011) — must be absolute path or resolved
/// 2. Resolved at compile time by resolve_pipeline_command (DD#23)
/// No runtime PATH lookup occurs. The binary literally contains "/usr/bin/grep".
///
/// TRUST: Arguments are SSA values — literals, variable reads, or
/// concatenations thereof. No shell interpretation (no word splitting,
/// no globbing, no metacharacter expansion).
#[allow(clippy::too_many_arguments)]
pub(super) fn lower_exec(
    name: &str,
    args: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    test_result_var: Variable,
    exit_status_var: Variable,
) {
    let argc = 1 + args.len();
    let argv_size = ((argc + 1) * 8) as u32;
    let argv_slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        argv_size,
        3,
    ));

    let cmd_val = SsaValue::Literal(name.to_string());
    let (cmd_ptr, _) = lower_value(
        &cmd_val,
        builder,
        module,
        vars,
        literals,
        &std::collections::HashMap::new(),
        externs,
        Some(exit_status_var),
    );
    builder.ins().stack_store(cmd_ptr, argv_slot, 0);

    for (i, arg) in args.iter().enumerate() {
        let offset = ((i + 1) * 8) as i32;
        let (ptr, _) = lower_value(
            arg,
            builder,
            module,
            vars,
            literals,
            &std::collections::HashMap::new(),
            externs,
            Some(exit_status_var),
        );
        builder.ins().stack_store(ptr, argv_slot, offset);
    }

    let null = builder.ins().iconst(I64, 0);
    builder
        .ins()
        .stack_store(null, argv_slot, (argc * 8) as i32);

    let envp_gv = module.declare_data_in_func(externs.envp, builder.func);
    let envp_addr = builder.ins().global_value(I64, envp_gv);
    let envp = builder.ins().load(I64, MemFlags::new(), envp_addr, 0);

    let argv_addr = builder.ins().stack_addr(I64, argv_slot, 0);
    let run_ref = module.declare_func_in_func(externs.run, builder.func);
    let call = builder.ins().call(run_ref, &[argv_addr, envp]);
    let exit_status = builder.inst_results(call)[0];
    // Set test_result_var: 1 if exit status == 0 (success), 0 otherwise
    let zero = builder.ins().iconst(I64, 0);
    let ok = builder.ins().icmp(IntCC::Equal, exit_status, zero);
    let result = builder.ins().uextend(I64, ok);
    builder.def_var(test_result_var, result);
    builder.def_var(exit_status_var, exit_status);
}
