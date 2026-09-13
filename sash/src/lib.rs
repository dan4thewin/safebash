// Modules used by integration tests — must be pub
pub mod emit_c;
pub mod emit_ir;
pub mod ir;
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

static HEAP_SIZE: Mutex<Option<usize>> = Mutex::new(None);

/// Record the `--heap-size` operand for the backends to bake in.
pub(crate) fn set_heap_size(bytes: usize) {
    *HEAP_SIZE.lock().unwrap() = Some(bytes);
}

/// The compiled-in heap reservation, or `None` to leave the runtime's weak
/// 256 MB default in place. Deliberately a *compile-time* setting: safebash
/// keeps the environment from steering a running binary, so there is no
/// `$SASH_HEAP` counterpart to this flag.
pub(crate) fn get_heap_size() -> Option<usize> {
    *HEAP_SIZE.lock().unwrap()
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

/// The one usage text. Every call site prints this — there used to be three
/// different strings, each advertising a different subset of the real flags.
///
/// `--no-taint-cst` is deliberately absent (DG, 2026-09-12): it is a testing
/// aid, not a general-use flag. It is still accepted; it is just not
/// advertised here. `docs/sash.1` documents it, marked as such.
const USAGE: &str = "\
Usage: sash [OPTION]... <script.sh>
       sash [OPTION]... -c 'script'
       sash --extract <binary>

  -c 'script'         compile SCRIPT, given inline
  -o <output>         write the binary to OUTPUT
  -h, --help          print this help on stdout and exit 0
  --emit-c            emit C source on stdout instead of a binary
  --emit-ir           emit the IR as inert janet data on stdout (DD#104)
  --manifest          print the program manifest on stdout
  --extract <binary>  print the .sash_source section of BINARY
  --no-taint          disable all taint checking (DD#82 escape hatch)
  --trace             compile in runtime tracing (DD#32)
  --heap-size=<size>  heap reservation, e.g. 512K, 64M, 2G (default 256M)
  --sysroot=<path>    resolve external commands under PATH";

/// Every `--` flag the driver accepts. An unknown one used to be collected
/// and never looked at, so `sash --emit-C f.sash` silently produced a native
/// ELF and exited 0 — a different artifact than the one asked for.
/// `--sysroot=` is prefix-matched rather than listed, since it carries a
/// value in the same token.
const KNOWN_FLAGS: &[&str] = &[
    "--emit-c",
    "--emit-ir",
    "--manifest",
    "--no-taint",
    "--no-taint-cst",
    "--trace",
    "--help",
];

/// Converter-only flags (DD#57). Recognised so the diagnostic can point at
/// `sash-convert` rather than reporting a plain unknown flag.
const CONVERTER_FLAGS: &[&str] = &["--emit-source", "--annotate", "--nolint"];

fn is_known_flag(flag: &str) -> bool {
    KNOWN_FLAGS.contains(&flag)
        || flag.starts_with("--sysroot=")
        || flag.starts_with("--heap-size=")
}

pub fn cli_main(args: Vec<String>) {
    if args.len() < 2 {
        eprintln!("{USAGE}");
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

    run_semantic_checks(input, &program, &parsed.flags);

    // Transform-time compile errors (SB4016/17/18 ignored non-void results,
    // SB4044 const-reassign, SB5015/16 read-flags, SB5041 command/function
    // collision, …). Enforce them for every backend so the c/manifest
    // paths reject exactly what native does — AND so they precede the
    // dead-variable gate below.
    enforce_transform_diags(input, &program);

    // SB5050 (DD#86): the dead-variable gate runs LAST — after taint (SB5020),
    // type (SB4030), and the transform errors above — so a variable that is
    // also involved in a more-specific error surfaces that error first
    // (DG, 2026-09-02: more-specific error wins).
    let dead = validate::check_dead_variables(&program);
    if !dead.is_empty() {
        print_diags(input, &dead);
        std::process::exit(1);
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

/// Apply `--sysroot=`. Flag *validity* is checked earlier, in
/// `check_flags`, so an unknown or converter-only flag is rejected before
/// the source file is even read.
fn apply_and_check_flags(flags: &[&str]) {
    for flag in flags {
        if let Some(path) = flag.strip_prefix("--sysroot=") {
            set_sysroot(path.to_string());
        }
        if let Some(size) = flag.strip_prefix("--heap-size=") {
            set_heap_size(parse_heap_size(size));
        }
    }
}

/// Split a size operand into its digit run and its multiplier.
fn heap_size_scale(spec: &str) -> (&str, usize) {
    let body = &spec[..spec.len().saturating_sub(1)];
    match spec.as_bytes().last() {
        Some(b'K' | b'k') => (body, 1024),
        Some(b'M' | b'm') => (body, 1024 * 1024),
        Some(b'G' | b'g') => (body, 1024 * 1024 * 1024),
        _ => (spec, 1),
    }
}

/// Parse a `--heap-size=` operand into bytes.
///
/// Rejects junk, zero, and overflow rather than clamping: a heap that
/// silently differs from the one asked for would resurface much later as an
/// unexplained `out of memory`, far from its cause.
fn parse_heap_size(spec: &str) -> usize {
    let (digits, scale) = heap_size_scale(spec);
    let bytes = if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        None
    } else {
        digits
            .parse::<usize>()
            .ok()
            .and_then(|n| n.checked_mul(scale))
    };
    match bytes {
        Some(n) if n > 0 => n,
        _ => {
            eprintln!("sash: --heap-size={spec}: expected a positive size like 512K, 64M, or 2G");
            eprintln!("{USAGE}");
            std::process::exit(1);
        }
    }
}

/// Handle `--help` and reject anything not in `KNOWN_FLAGS`.
///
/// `--help` used to be collected as an unknown flag, leave no positional
/// input, and fall into the "no input" branch — usage on *stderr* with exit
/// 1. It now prints on stdout and exits 0.
fn check_flags(flags: &[&str]) {
    if flags.contains(&"--help") {
        println!("{USAGE}");
        std::process::exit(0);
    }
    if let Some(f) = flags.iter().find(|f| CONVERTER_FLAGS.contains(*f)) {
        eprintln!("sash: '{f}' is not a valid flag — use sash-convert for source transformations");
        std::process::exit(1);
    }
    if let Some(f) = flags.iter().find(|f| !is_known_flag(f)) {
        eprintln!("sash: unrecognized option '{f}'");
        eprintln!("{USAGE}");
        std::process::exit(1);
    }
}

/// Run the front-end diagnostic gates that apply to every backend:
/// structural validation, taint analysis, and type checking (incl. the
/// DD#61 attest-form rules). Exits the process on the first failing gate.
/// Structural validation rules (DD-level rejections of unsupported bash
/// constructs: heredocs, export/declare, flat map literals, …). Runs
/// before command resolution so a rejected construct surfaces its own
/// diagnostic rather than a spurious SB4032 on the tokens it leaves behind.
fn run_structural_checks(input: &str, program: &crate::ir::Program) {
    let mut vdiags = validate::validate(program);
    vdiags.extend(validate::check_malformed_sed(program));
    vdiags.extend(validate::check_subscript_index(program));
    // DD#99: a source-level `echo -n` is retired. This is a language-surface
    // rule, so it runs with the structural checks — crucially BEFORE
    // `desugar_write`, which synthesises exactly that form internally.
    vdiags.extend(validate::check_echo_no_flags(program));
    if !vdiags.is_empty() {
        print_diags(input, &vdiags);
        std::process::exit(1);
    }
}

/// DD#82: `--no-taint-cst` (and the stronger `--no-taint`) disable the CST
/// (front-end) taint pass — a supported bypass, and the switch that lets the
/// SSA taint backstop be exercised in isolation.
fn cst_taint_disabled(flags: &[&str]) -> bool {
    flags.contains(&"--no-taint") || flags.contains(&"--no-taint-cst")
}

/// DD#82: run the SSA taint backstop unless `--no-taint` disabled it. Exits
/// the process on a rejection. Kept as its own function so `compile_native`
/// stays flat (complexity gate).
fn run_ssa_taint_gate(input: &str, ssa: &crate::ssa::SsaProgram, flags: &[&str]) {
    if ssa_taint_disabled(flags) {
        return;
    }
    let taint_ices = ssa_taint::verify_ssa_taint(ssa);
    if !taint_ices.is_empty() {
        for ice in &taint_ices {
            eprintln!("{}: {}", input, ice);
        }
        std::process::exit(1);
    }
}

/// DD#82: `--no-taint` disables ALL taint checking (both the CST front-end
/// pass and the SSA backstop) — the full escape hatch.
fn ssa_taint_disabled(flags: &[&str]) -> bool {
    flags.contains(&"--no-taint")
}

/// Taint, positional, argv-splat, and type checks. Run after resolution so
/// the taint pass classifies on the resolved exec form.
fn run_semantic_checks(input: &str, program: &crate::ir::Program, flags: &[&str]) {
    let mut tdiags = if cst_taint_disabled(flags) {
        Vec::new()
    } else {
        validate::check_taint(program)
    };
    tdiags.extend(validate::check_positionals(program));
    tdiags.extend(validate::check_dollar_at(program));
    tdiags.extend(validate::check_argv_splat(program));
    tdiags.extend(validate::check_undeclared_reads(program));
    tdiags.extend(validate::check_len_operands(program));
    tdiags.extend(validate::check_join_form(program));
    tdiags.extend(validate::check_operand_spelling(program));
    tdiags.extend(validate::check_fill_target(program));
    tdiags.extend(validate::check_collection_scalar_read(program));
    tdiags.extend(validate::check_scalar_subscript(program));
    tdiags.extend(validate::check_collection_slots(program));
    tdiags.extend(validate::check_collection_args(program));
    tdiags.extend(validate::check_fn_taints_global(program));
    tdiags.extend(validate::check_disposition_defaults(program));
    tdiags.extend(validate::check_elide_positions(program));
    tdiags.extend(validate::check_args_immutable(program));
    tdiags.extend(validate::check_discard_reads(program));
    tdiags.extend(validate::check_readline_arity(program));
    tdiags.extend(validate::check_match_arity(program));
    tdiags.extend(validate::check_map_from_array(program));
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
/// (`--emit-ir`, `--emit-c`, `--manifest`). Returns `true` if one
/// fired (caller should return); `false` if none matched (caller
/// proceeds to native codegen).
fn maybe_emit_alt_output(flags: &[&str], program: &crate::ir::Program) -> bool {
    if flags.contains(&"--emit-ir") {
        print!("{}", emit_ir::emit_ir(program));
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
        eprintln!("{USAGE}");
        std::process::exit(1);
    }
    let binary_path = &args[2];
    let data = std::fs::read(binary_path).unwrap_or_else(|e| {
        eprintln!("sash: {}: {}", binary_path, e);
        std::process::exit(1);
    });
    match read_sash_source_section(&data) {
        Ok(source) => print!("{source}"),
        Err(msg) => {
            eprintln!("sash: {binary_path}: {msg}");
            std::process::exit(1);
        }
    }
}

/// Read the `.sash_source` section body out of an object-file image.
///
/// An unreadable section reports the underlying error rather than claiming
/// the section is absent, which is what the previous loop did.
fn read_sash_source_section(image: &[u8]) -> Result<String, String> {
    use object::read::{Object, ObjectSection};
    let obj =
        object::read::File::parse(image).map_err(|e| format!("not a valid object file: {e}"))?;
    let section = obj
        .sections()
        .find(|s| s.name() == Ok(".sash_source"))
        .ok_or_else(|| "no .sash_source section found".to_string())?;
    let body = section
        .data()
        .map_err(|e| format!(".sash_source: unreadable: {e}"))?;
    Ok(std::str::from_utf8(body)
        .unwrap_or("<binary data>")
        .to_string())
}

struct ParsedCli<'a> {
    flags: Vec<&'a str>,
    output_path: Option<&'a str>,
    inline_source: Option<&'a str>,
    input: &'a str,
}

#[derive(Default)]
struct CliAccum<'a> {
    flags: Vec<&'a str>,
    positional: Vec<&'a str>,
    output_path: Option<&'a str>,
    inline_source: Option<&'a str>,
}

fn parse_cli_args(args: &[String]) -> ParsedCli<'_> {
    let mut acc = CliAccum::default();
    let mut i = 1;
    while i < args.len() {
        i += take_cli_arg(args, i, &mut acc);
    }
    check_flags(&acc.flags);
    ParsedCli {
        input: resolve_input(&acc.positional, acc.inline_source),
        flags: acc.flags,
        output_path: acc.output_path,
        inline_source: acc.inline_source,
    }
}

/// Consume the argument at `i`, returning how many tokens it used — 2 for an
/// option that takes an operand, 1 otherwise.
fn take_cli_arg<'a>(args: &'a [String], i: usize, acc: &mut CliAccum<'a>) -> usize {
    match args[i].as_str() {
        "-o" => {
            acc.output_path = Some(cli_operand(args, i, "-o"));
            2
        }
        "-c" => {
            acc.inline_source = Some(cli_operand(args, i, "-c"));
            2
        }
        "-h" => {
            acc.flags.push("--help");
            1
        }
        s if s.starts_with("--") => {
            acc.flags.push(s);
            1
        }
        s => {
            acc.positional.push(s);
            1
        }
    }
}

/// The operand of `-o` / `-c`. A missing `-o` operand used to be silently
/// ignored, so `sash f.sash -o` compiled to the default path and exited 0.
fn cli_operand<'a>(args: &'a [String], i: usize, flag: &str) -> &'a str {
    match args.get(i + 1) {
        Some(v) => v.as_str(),
        None => {
            eprintln!("sash: '{flag}' needs an operand");
            eprintln!("{USAGE}");
            std::process::exit(1);
        }
    }
}

fn resolve_input<'a>(positional: &[&'a str], inline_source: Option<&str>) -> &'a str {
    match positional.first() {
        Some(f) => f,
        None if inline_source.is_some() => "-c",
        None => {
            eprintln!("{USAGE}");
            std::process::exit(1);
        }
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
/// Where a native build writes its binary: an explicit `-o`, `a.out` for
/// `-c`, else the source name with its shell suffix dropped.
fn native_output_path(input: &str, output_path: Option<&str>, is_inline: bool) -> String {
    if let Some(p) = output_path {
        return p.to_string();
    }
    if is_inline {
        return "a.out".to_string();
    }
    input
        .strip_suffix(".sh")
        .or_else(|| input.strip_suffix(".bash"))
        .unwrap_or(&format!("{}.out", input))
        .to_string()
}

/// Link the object against the runtime, then drop the object.
///
/// `-o` and the name must be SEPARATE arguments: `ld` uses
/// `getopt_long_only`, so a glued `-ou` is matched against single-dash long
/// options, where `ou` is an unambiguous prefix of `--out-implib` — which
/// takes an argument, so ld swallowed the object file and left `_start`
/// with an undefined reference to `main`.
fn link_object(obj_path: &str, output: &str) {
    let rt_path = format!(
        "{}/../sash-rt/target/release/libsash_rt.a",
        env!("CARGO_MANIFEST_DIR")
    );
    let ld = Command::new("ld")
        .args([
            "-o",
            output,
            obj_path,
            &rt_path,
            "-static",
            "--gc-sections",
            "-z",
            "noexecstack",
            "--strip-debug",
        ])
        .status()
        .expect("ld failed");

    let _ = std::fs::remove_file(obj_path);

    if !ld.success() {
        eprintln!("sash: linking failed");
        std::process::exit(1);
    }
}

fn compile_native(
    input: &str,
    source: &str,
    program: &crate::ir::Program,
    output_path: Option<&str>,
    is_inline: bool,
    flags: &[&str],
) {
    let output = native_output_path(input, output_path, is_inline);
    let obj_path = format!("{}.o", output);

    let type_map = type_check::infer_types(program);

    let (ssa, _) = transform::ir_to_ssa_typed(program, type_map);
    run_ssa_taint_gate(input, &ssa, flags);

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

    link_object(&obj_path, &output);

    println!("sash: {} -> {} (static binary)", input, output);
}
