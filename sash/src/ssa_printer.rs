/// SSA pretty-printer.
///
/// Renders SSA basic blocks, phi nodes, versioned variables,
/// and terminators as readable text.

use crate::ssa::*;

pub fn print_ssa(program: &SsaProgram) -> String {
    let mut out = String::new();

    for (id, block) in &program.blocks {
        // Block header
        out.push_str(&format!("bb{} [{}]:\n", id, block.label));

        // Phi nodes
        for phi in &block.phis {
            out.push_str(&format!("  {} = phi(", phi.target.display()));
            for (i, (bid, vref)) in phi.sources.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&format!("bb{}:{}", bid, vref.display()));
            }
            out.push_str(")\n");
        }

        // Instructions
        for inst in &block.insts {
            match inst {
                SsaInst::Assign { target, value } => {
                    out.push_str(&format!("  {} = {}\n",
                        target.display(),
                        print_value(value)));
                }
                SsaInst::Command { name, args, .. } => {
                    out.push_str(&format!("  {}",  name));
                    for arg in args {
                        out.push_str(&format!(" {}", print_value(arg)));
                    }
                    out.push('\n');
                }
                SsaInst::Phi(_) => {} // printed above

                SsaInst::Test(test_expr) => {
                    out.push_str(&format!("  test {:?}\n", test_expr));
                }
                SsaInst::ReadLine { target, fd, delim: _, .. } => {
                    out.push_str(&format!("  {} = read_line(fd={})\n", target.display(), fd));
                }
                SsaInst::ReadLineMulti { targets, fd, delim: _, .. } => {
                    let names: Vec<String> = targets.iter().map(|t| t.display()).collect();
                    out.push_str(&format!("  {} = read_line_multi(fd={})\n", names.join(", "), fd));
                }
                SsaInst::Pipeline { stages, .. } => {
                    let names: Vec<&str> = stages.iter().map(|(n, _, _)| n.as_str()).collect();
                    out.push_str(&format!("  pipeline: {}\n", names.join(" | ")));
                }
                SsaInst::Mapfile { array, fd, map_mode, .. } => {
                    let flag = if *map_mode { "-m" } else { "-t" };
                    out.push_str(&format!("  mapfile {} {} (fd={})\n", flag, array, fd));
                }
                SsaInst::ReadArray { array, fd, delim } => {
                    let d = delim.map(|b| format!(" -d'{}'", b as char)).unwrap_or_default();
                    out.push_str(&format!("  read -a {}{} (fd={})\n", array, d, fd));
                }
                SsaInst::CaptureStart => {
                    out.push_str("  capture_start\n");
                }
                SsaInst::CaptureEnd { target } => {
                    out.push_str(&format!("  {} = capture_end\n", target.display()));
                }
                SsaInst::SubscriptAssign { array, index, value, .. } => {
                    out.push_str(&format!("  {}[{}] = {}\n", array, print_value(index), print_value(value)));
                }
                SsaInst::ArrayAppend { array, value } => {
                    out.push_str(&format!("  {} += ({})\n", array, print_value(value)));
                }
                SsaInst::Bless { target, source, test } => {
                    out.push_str(&format!("  {} = bless {} [[ {:?} ]]\n", target.display(), source.display(), test));
                }
            }
        }

        // Terminator
        match &block.terminator {
            Terminator::Goto(dest) => {
                out.push_str(&format!("  → bb{}\n", dest));
            }
            Terminator::Branch { condition, true_block, false_block } => {
                out.push_str(&format!("  br {} → bb{}, bb{}\n",
                    condition, true_block, false_block));
            }
            Terminator::Return => {
                out.push_str("  ret\n");
            }
            Terminator::Loop { condition, body_block, exit_block } => {
                out.push_str(&format!("  loop {} → bb{}, exit bb{}\n",
                    condition, body_block, exit_block));
            }
        }

        out.push('\n');
    }

    out
}

fn print_value(val: &SsaValue) -> String {
    match val {
        SsaValue::Literal(s) => format!("\"{}\"", s),
        SsaValue::Var(vref) => vref.display(),
        SsaValue::Expr(s) => s.clone(),
        SsaValue::FuncCall(name) => format!("call {}()", name),
        SsaValue::IntFuncCall { name, args } => {
            let arg_strs: Vec<String> = args.iter().map(|a| print_value(a)).collect();
            format!("(int:{} {})", name, arg_strs.join(" "))
        }
        SsaValue::CmdSubst { argv, .. } => {
            let parts: Vec<String> = argv.iter().map(|v| print_value(v)).collect();
            format!("$({})", parts.join(" "))
        }
        SsaValue::Concat(parts) => {
            let ps: Vec<String> = parts.iter().map(|v| print_value(v)).collect();
            ps.join(" ++ ")
        }
        SsaValue::ParamExpansion { var, operator, operand } => {
            format!("${{{}{}{}}}",  var.display(), operator, operand)
        }
        SsaValue::ArithExpr { var, op, operand } => {
            format!("$(({} {} {}))", var.display(), op.display(), operand)
        }
        SsaValue::ArithBinOp { left, op, right } => {
            format!("$(({} {} {}))", print_value(left), op.display(), print_value(right))
        }
        SsaValue::AssocArray(pairs) => {
            let inner: Vec<String> = pairs.iter()
                .map(|(k, v)| format!("[{}]={}", k, print_value(v)))
                .collect();
            format!("({})", inner.join(" "))
        }
        SsaValue::Subscript { array, index, .. } => {
            format!("${{{}[{}]}}", array, print_value(index))
        }
        SsaValue::ArgvIndex(idx) => {
            format!("argv[{}]", idx.display())
        }
        SsaValue::IndexedArray(elems) => {
            let inner: Vec<String> = elems.iter().map(|v| print_value(v)).collect();
            format!("({})", inner.join(" "))
        }
        SsaValue::Ternary { cond, true_val, false_val } => {
            format!("({} ? {} : {})", print_value(cond), print_value(true_val), print_value(false_val))
        }
        SsaValue::TableValue { array, index } => {
            format!("table_value({}[{}])", array, index.display())
        }
        SsaValue::TableKey { array, index } => {
            format!("table_key({}[{}])", array, index.display())
        }
    }
}
