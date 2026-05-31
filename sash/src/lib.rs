// Modules used by tests (oracle_tests.rs) — must be pub
pub mod emit_janet;
pub mod ir;
pub mod oracle;
pub mod parse;

// Internal modules — not used outside the crate
pub mod codegen;
pub(crate) mod diagnostic;
pub(crate) mod emit_source;
pub(crate) mod printer;
pub(crate) mod regex_validate;
pub(crate) mod ssa;
pub(crate) mod ssa_printer;
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
        eprintln!("Usage: sash <script.sh> [-o <output>] [--janet] [--ir] [--ssa] [--manifest] [--extract <binary>]");
        std::process::exit(1);
    }

    if args[1] == "--extract" {
        dump_sash_source(&args);
        return;
    }

    let parsed = parse_cli_args(&args);
    let input = parsed.input;
    let source = read_source(input, parsed.inline_source);

    for flag in &parsed.flags {
        if let Some(path) = flag.strip_prefix("--sysroot=") {
            set_sysroot(path.to_string());
        }
    }

    // DD#57: reject converter flags
    for flag in &parsed.flags {
        if matches!(*flag, "--emit-source" | "--annotate" | "--nolint") {
            eprintln!(
                "sash: '{}' is not a valid flag — use sash-convert for source transformations",
                flag
            );
            std::process::exit(1);
        }
    }

    let (program, parse_diags) = match parse::parse(&source) {
        Ok(result) => result,
        Err(diags) => {
            print_diags(input, &diags);
            std::process::exit(1);
        }
    };

    print_diags(input, &parse_diags);

    let vdiags = validate::validate(&program);
    let mut tdiags = validate::check_taint(&program);
    tdiags.extend(vdiags);
    if !tdiags.is_empty() {
        print_diags(input, &tdiags);
        std::process::exit(1);
    }

    if parsed.flags.contains(&"--ir") {
        print!("{}", printer::print_program(&program));
        return;
    }
    if parsed.flags.contains(&"--janet") {
        print!("{}", emit_janet::emit_janet(&program));
        return;
    }
    if parsed.flags.contains(&"--ssa") {
        let (ssa, _diags) = transform::ir_to_ssa(&program);
        print!("{}", ssa_printer::print_ssa(&ssa));
        return;
    }
    if parsed.flags.contains(&"--manifest") {
        emit_manifest(&program);
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
            eprintln!("Usage: sash [-c 'script'] [-o output] [--ir|--ssa] <script.sh>");
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
fn emit_manifest(program: &crate::ir::Program) {
    let type_map = type_check::infer_types(program);
    let (ssa, _) = transform::ir_to_ssa_typed(program, type_map);
    let builtins: std::collections::HashSet<&str> = [
        "echo",
        "true",
        "false",
        ":",
        "exit",
        "cd",
        "pwd",
        "wait",
        "shift",
        "read",
        "exec",
        "command",
        "[[",
        "__redirect_push",
        "__redirect_pop",
        "__store_bg_pid",
        "__subshell_wait",
        "__table_delete",
        "printf",
        "signal",
    ]
    .iter()
    .copied()
    .collect();
    let path_var = std::env::var("PATH").unwrap_or_default();
    let sysroot = get_sysroot();
    let mut deps: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for block in ssa.blocks.values() {
        for inst in &block.insts {
            if let ssa::SsaInst::Command { name, .. } = inst {
                if let Some(path) = resolve_dep(name, &builtins, &path_var, sysroot.as_deref()) {
                    deps.insert(path);
                }
            }
            if let ssa::SsaInst::Pipeline { stages, .. } = inst {
                for (name, _, _) in stages {
                    if let Some(path) = resolve_dep(name, &builtins, &path_var, sysroot.as_deref())
                    {
                        deps.insert(path);
                    }
                }
            }
        }
    }
    for dep in &deps {
        println!("{}", dep);
    }
}

fn resolve_dep(
    name: &str,
    builtins: &std::collections::HashSet<&str>,
    path_var: &str,
    sysroot: Option<&str>,
) -> Option<String> {
    if name.starts_with('/') {
        return Some(name.to_string());
    }
    if builtins.contains(name) {
        return None;
    }
    for dir in path_var.split(':') {
        let target_path = format!("{}/{}", dir, name);
        let exists = if let Some(sr) = sysroot {
            crate::sysroot_file_exists(sr, &target_path)
        } else {
            std::path::Path::new(&target_path).is_file()
        };
        if exists {
            return Some(target_path);
        }
    }
    Some(name.to_string())
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
