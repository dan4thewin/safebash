// Modules used by tests (oracle_tests.rs) — must be pub
pub mod emit_c;
pub mod emit_janet;
pub mod emit_janet_map;
pub mod ir;
pub mod oracle;
pub mod parse;

// Internal modules — not used outside the crate
pub mod codegen;
pub(crate) mod diagnostic;
pub(crate) mod regex_validate;
pub(crate) mod resolve;
pub(crate) mod ssa;
pub(crate) mod ssa_taint;
pub(crate) mod transform;
pub mod type_check;
pub(crate) mod validate;

use std::sync::Mutex;

static SYSROOT: Mutex<Option<String>> = Mutex::new(None);

/// Set the sysroot path for cross-compilation command resolution.
pub(crate) fn set_sysroot(path: String) {
    *SYSROOT.lock().unwrap() = Some(path);
}

/// Get the current sysroot path (if set).
pub(crate) fn get_sysroot() -> Option<String> {
    SYSROOT.lock().unwrap().clone()
}

/// Cache of squashfs file listings (path → set of files)
static SQUASHFS_CACHE: Mutex<Option<std::collections::HashSet<String>>> = Mutex::new(None);

/// Check if a path exists in the sysroot. Supports both directory sysroots
/// and .squashfs image files (via unsquashfs -l).
pub(crate) fn sysroot_file_exists(sysroot: &str, path: &str) -> bool {
    if sysroot.ends_with(".squashfs") || sysroot.ends_with(".sqsh") {
        let mut cache = SQUASHFS_CACHE.lock().unwrap();
        if cache.is_none() {
            // Populate cache from unsquashfs -l
            let output = std::process::Command::new("unsquashfs")
                .args(["-l", "-no-progress", sysroot])
                .output();
            let files: std::collections::HashSet<String> = match output {
                Ok(out) => {
                    String::from_utf8_lossy(&out.stdout)
                        .lines()
                        .filter_map(|line| {
                            // unsquashfs -l output: "squashfs-root/usr/bin/cat"
                            line.strip_prefix("squashfs-root").map(|p| p.to_string())
                        })
                        .collect()
                }
                Err(_) => std::collections::HashSet::new(),
            };
            *cache = Some(files);
        }
        cache.as_ref().unwrap().contains(path)
    } else {
        let full_path = format!("{}{}", sysroot, path);
        std::path::Path::new(&full_path).is_file()
    }
}

use std::process::Command;

pub fn cli_main(args: Vec<String>) {
    if args.len() < 2 {
        eprintln!(
            "Usage: sash <script.sh> [-o <output>] [--janet] [--manifest] [--extract <binary>]"
        );
        std::process::exit(1);
    }

    if args[1] == "--extract" {
        dump_sash_source(&args);
        return;
    }

    let parsed = parse_cli_args(&args);
    let input = parsed.input;
    let source = read_source(input, parsed.inline_source);

    apply_and_check_flags(&parsed.flags);

    let (program, parse_diags) = match parse::parse(&source) {
        Ok(result) => result,
        Err(diags) => {
            print_diags(input, &diags);
            std::process::exit(1);
        }
    };
    let mut program = program;

    print_diags(input, &parse_diags);

    // Structural validation (heredoc/export/declare/… rejections) runs
    // FIRST, before resolution: an unsupported construct must surface its
    // own diagnostic, not a spurious SB4032 on the tokens it leaves behind
    // (e.g. a rejected heredoc leaks its body lines as bare commands).
    run_structural_checks(input, &program);

    // DD#23: resolve external command heads to absolute paths BEFORE the
    // taint/type checks, so they see the real exec form (`/usr/bin/cat`,
    // not bare `cat`). An unresolved external is fatal here, not at runtime.
    let resolve_diags = resolve::resolve_commands(&mut program);
    if !resolve_diags.is_empty() {
        print_diags(input, &resolve_diags);
        std::process::exit(1);
    }

    run_semantic_checks(input, &program);

    // Transform-time compile errors (SB4016/17/18/…: ignored non-void
    // results, etc.) are backend-independent. native enforces them inside
    // codegen::compile_to_object; enforce them here too so the c / janet /
    // manifest oracles reject exactly the programs native rejects.
    if flags_request_alt_output(&parsed.flags) {
        enforce_transform_diags(input, &program);
    }

    if maybe_emit_alt_output(&parsed.flags, &program) {
        return;
    }

    compile_native(
        input,
        &source,
        &program,
        parsed.output_path,
        parsed.inline_source.is_some(),
        &parsed.flags,
    );
}

/// Apply `--sysroot=` and reject converter-only flags (DD#57). Exits on a
/// rejected flag.
fn apply_and_check_flags(flags: &[&str]) {
    for flag in flags {
        if let Some(path) = flag.strip_prefix("--sysroot=") {
            set_sysroot(path.to_string());
        }
    }
    for flag in flags {
        if matches!(*flag, "--emit-source" | "--annotate" | "--nolint") {
            eprintln!(
                "sash: '{}' is not a valid flag — use sash-convert for source transformations",
                flag
            );
            std::process::exit(1);
        }
    }
}

/// Run the front-end diagnostic gates that apply to every backend:
/// structural validation, taint analysis, and type checking (incl. the
/// DD#61 bless-form rules). Exits the process on the first failing gate.
/// Structural validation rules (DD-level rejections of unsupported bash
/// constructs: heredocs, export/declare, flat map literals, …). Runs
/// before command resolution so a rejected construct surfaces its own
/// diagnostic rather than a spurious SB4032 on the tokens it leaves behind.
fn run_structural_checks(input: &str, program: &crate::ir::Program) {
    let mut vdiags = validate::validate(program);
    vdiags.extend(validate::check_malformed_sed(program));
    vdiags.extend(validate::check_subscript_index(program));
    if !vdiags.is_empty() {
        print_diags(input, &vdiags);
        std::process::exit(1);
    }
}

/// Taint, positional, argv-splat, and type checks. Run after resolution so
/// the taint pass classifies on the resolved exec form.
fn run_semantic_checks(input: &str, program: &crate::ir::Program) {
    let mut tdiags = validate::check_taint(program);
    tdiags.extend(validate::check_positionals(program));
    tdiags.extend(validate::check_argv_splat(program));
    tdiags.extend(validate::check_undeclared_reads(program));
    tdiags.extend(validate::check_len_operands(program));
    tdiags.extend(validate::check_disposition_defaults(program));
    if !tdiags.is_empty() {
        print_diags(input, &tdiags);
        std::process::exit(1);
    }

    let type_errors = type_check::type_check(program);
    if !type_errors.is_empty() {
        for te in &type_errors {
            eprintln!(
                "{}:{}:{}: error [SB4030] {}",
                input, te.span.line, te.span.col, te.message
            );
        }
        std::process::exit(1);
    }
}

/// True if any flag selects an alternative (non-native) output backend.
fn flags_request_alt_output(flags: &[&str]) -> bool {
    flags.contains(&"--janet")
        || flags.contains(&"--janet-map")
        || flags.contains(&"--emit-c")
        || flags.contains(&"--manifest")
}

/// Run the AST→SSA transform purely for its compile-error diagnostics
/// (SB4016/17/18/…) and exit if any fired. Mirrors the diag surfacing in
/// codegen::compile_to_object so the alt-output backends reject the same
/// programs native does.
fn enforce_transform_diags(input: &str, program: &crate::ir::Program) {
    let type_map = type_check::infer_types(program);
    let (_ssa, diags) = transform::ir_to_ssa_typed(program, type_map);
    if !diags.is_empty() {
        for d in &diags {
            eprintln!("{}:{}", input, d);
        }
        std::process::exit(1);
    }
}

/// Handle the terminal "emit an alternative output and exit" flags
/// (`--janet`, `--emit-c`, `--manifest`). Returns `true` if one
/// fired (caller should return); `false` if none matched (caller
/// proceeds to native codegen).
fn maybe_emit_alt_output(flags: &[&str], program: &crate::ir::Program) -> bool {
    if flags.contains(&"--janet-map") {
        // Structural janet map (DD#15 amended) — the sash-rt.janet primitive
        // library is prepended so the emitted file is self-contained.
        print!(
            "{}\n{}",
            include_str!("../janet/sash-rt.janet"),
            emit_janet_map::emit_janet_map(program)
        );
        return true;
    }
    if flags.contains(&"--janet") {
        print!("{}", emit_janet::emit_janet(program));
        return true;
    }
    if flags.contains(&"--emit-c") {
        emit_c_to_stdout(program);
        return true;
    }
    if flags.contains(&"--manifest") {
        emit_manifest(program);
        return true;
    }
    false
}

/// `--emit-c`: lower to SSA, emit C source to stdout. The C oracle
/// (notes/tasks/add-c-oracle.md). gcc-compiled output of this C
/// linked against libsash_rt.a is a differential oracle against
/// Cranelift-compiled output of the same SSA — any disagreement
/// is a bug somewhere in the compiler.
fn emit_c_to_stdout(program: &crate::ir::Program) {
    let type_map = type_check::infer_types(program);
    let (ssa, _) = transform::ir_to_ssa_typed(program, type_map);
    print!("{}", emit_c::emit_c(&ssa));
}

/// `--extract`: dump the `.sash_source` section from a compiled binary.
fn dump_sash_source(args: &[String]) {
    if args.len() < 3 {
        eprintln!("Usage: sash --extract <binary>");
        std::process::exit(1);
    }
    let binary_path = &args[2];
    let data = std::fs::read(binary_path).unwrap_or_else(|e| {
        eprintln!("sash: {}: {}", binary_path, e);
        std::process::exit(1);
    });
    let obj = object::read::File::parse(&*data).unwrap_or_else(|e| {
        eprintln!("sash: {}: not a valid object file: {}", binary_path, e);
        std::process::exit(1);
    });
    use object::read::{Object, ObjectSection};
    for section in obj.sections() {
        if section.name() == Ok(".sash_source") {
            if let Ok(data) = section.data() {
                print!("{}", std::str::from_utf8(data).unwrap_or("<binary data>"));
                return;
            }
        }
    }
    eprintln!("sash: {}: no .sash_source section found", binary_path);
    std::process::exit(1);
}

struct ParsedCli<'a> {
    flags: Vec<&'a str>,
    output_path: Option<&'a str>,
    inline_source: Option<&'a str>,
    input: &'a str,
}

fn parse_cli_args(args: &[String]) -> ParsedCli<'_> {
    let mut flags: Vec<&str> = Vec::new();
    let mut positional: Vec<&str> = Vec::new();
    let mut output_path: Option<&str> = None;
    let mut inline_source: Option<&str> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                i += 1;
                if i < args.len() {
                    output_path = Some(&args[i]);
                }
            }
            "-c" => {
                i += 1;
                if i < args.len() {
                    inline_source = Some(&args[i]);
                }
            }
            s if s.starts_with("--") => flags.push(s),
            s => positional.push(s),
        }
        i += 1;
    }
    let input = match positional.first() {
        Some(f) => *f,
        None if inline_source.is_some() => "-c",
        None => {
            eprintln!("Usage: sash [-c 'script'] [-o output] [--janet] <script.sh>");
            std::process::exit(1);
        }
    };
    ParsedCli {
        flags,
        output_path,
        inline_source,
        input,
    }
}

fn read_source(input: &str, inline_source: Option<&str>) -> String {
    if let Some(src) = inline_source {
        src.to_string()
    } else {
        std::fs::read_to_string(input).unwrap_or_else(|e| {
            eprintln!("sash: {}: {}", input, e);
            std::process::exit(1);
        })
    }
}

fn print_diags(input: &str, diags: &[diagnostic::Diagnostic]) {
    for d in diags {
        let prefix = match d.severity {
            diagnostic::Severity::Error => "error",
            diagnostic::Severity::Deferred => "deferred",
        };
        eprintln!(
            "{}:{}:{}: {} [{}] {}",
            input, d.line, d.col, prefix, d.code, d.message
        );
    }
}

/// `--manifest`: emit one path per line for every external dependency.
///
/// The manifest is collected at the single command-resolution funnel
/// (`transform::resolution::resolve_pipeline_command`) during lowering and
/// surfaced on `SsaProgram::external_deps`. Because every external form —
/// direct commands, pipeline stages, and `$( … )` substitutions — passes
/// through that funnel, and only names that resolve to a real path (contain
/// `/`) are recorded, the manifest can neither miss an external nor leak a
/// builtin or compiler-internal sigil.
fn emit_manifest(program: &crate::ir::Program) {
    let type_map = type_check::infer_types(program);
    let (ssa, _) = transform::ir_to_ssa_typed(program, type_map);
    for dep in &ssa.external_deps {
        println!("{}", dep);
    }
}

/// Default path: type-check, taint-verify, codegen, and link a static ELF.
fn compile_native(
    input: &str,
    source: &str,
    program: &crate::ir::Program,
    output_path: Option<&str>,
    is_inline: bool,
    flags: &[&str],
) {
    let output = if let Some(p) = output_path {
        p.to_string()
    } else if is_inline {
        "a.out".to_string()
    } else {
        input
            .strip_suffix(".sh")
            .or_else(|| input.strip_suffix(".bash"))
            .unwrap_or(&format!("{}.out", input))
            .to_string()
    };

    let obj_path = format!("{}.o", output);

    let type_map = type_check::infer_types(program);

    let (ssa, _) = transform::ir_to_ssa_typed(program, type_map);
    let taint_ices = ssa_taint::verify_ssa_taint(&ssa);
    if !taint_ices.is_empty() {
        for ice in &taint_ices {
            eprintln!("{}: {}", input, ice);
        }
        std::process::exit(1);
    }

    let trace = flags.contains(&"--trace");
    let obj_bytes = match codegen::compile_to_object(source, trace) {
        Ok(bytes) => bytes,
        Err(diags) => {
            for d in &diags {
                eprintln!("{}:{}", input, d);
            }
            std::process::exit(1);
        }
    };
    std::fs::write(&obj_path, &obj_bytes).expect("write .o");

    let rt_path = format!(
        "{}/../sash-rt/target/release/libsash_rt.a",
        env!("CARGO_MANIFEST_DIR")
    );

    let ld = Command::new("ld")
        .args([
            &format!("-o{}", output),
            &obj_path,
            &rt_path,
            "-static",
            "--gc-sections",
            "-z",
            "noexecstack",
            "--strip-debug",
        ])
        .status()
        .expect("ld failed");

    let _ = std::fs::remove_file(&obj_path);

    if !ld.success() {
        eprintln!("sash: linking failed");
        std::process::exit(1);
    }

    println!("sash: {} -> {} (static binary)", input, output);
}
