// Modules used by integration tests — must be pub
pub mod emit_c;
pub mod emit_ir;
pub mod ir;
pub mod parse;

// Internal modules — not used outside the crate
pub(crate) mod bindings;
pub(crate) mod capture_purity;
pub mod codegen;
pub(crate) mod diagnostic;
pub(crate) mod output_taint;
pub(crate) mod regex_validate;
pub(crate) mod resolve;
pub(crate) mod ssa;
pub(crate) mod ssa_taint;
pub(crate) mod transform;
pub mod type_check;
pub(crate) mod validate;

use std::collections::HashMap;
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

/// What kind of thing a resolution root has at a path.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum FileKind {
    Regular,
    Directory,
    /// A device node, socket, FIFO — present, but not a program.
    Other,
}

/// What a resolution root reports about one path: its kind and its permission
/// bits. DD#23 resolves command paths at compile time, so "is something there"
/// is not a sufficient question — a directory, a device node and a mode-0444
/// regular file are all present and none of them can be `exec`ed.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PathFacts {
    pub(crate) kind: FileKind,
    pub(crate) mode: u32,
}

impl PathFacts {
    /// Can *some* user run this? The coarse test DD#23 applies at compile
    /// time: a regular file carrying at least one execute bit. *Which* user
    /// is a finer question the compiler cannot answer without being told who
    /// the target runs as — see `notes/tasks/target-user-access-check.md`.
    pub(crate) fn is_executable(&self) -> bool {
        self.kind == FileKind::Regular && self.mode & 0o111 != 0
    }
}

/// One entry of a squashfs listing. `link` is `Some` only for a symlink, and
/// holds the raw target as the image records it.
#[derive(Clone)]
struct SquashEntry {
    facts: PathFacts,
    link: Option<String>,
}

/// Cache of the squashfs listing (absolute in-image path → entry), keyed by
/// the image it was read from. The key is what makes the cache safe to hold
/// in a `static`: without it a second compile against a *different* image in
/// the same process would silently be answered from the first image's
/// listing. One compile per process is all the CLI does today, so the key
/// costs nothing and removes the trap rather than documenting it.
static SQUASHFS_CACHE: Mutex<Option<(String, std::collections::HashMap<String, SquashEntry>)>> =
    Mutex::new(None);

/// The three bits one `rwx` triple of an `ls`-style mode string carries. A
/// lowercase `s`/`t` holds its execute bit and an uppercase `S`/`T` does not,
/// which is the whole reason this reads the character rather than testing it
/// for `'x'`.
fn triple_bits(t: &[u8]) -> u32 {
    (u32::from(t[0] == b'r') << 2)
        | (u32::from(t[1] == b'w') << 1)
        | u32::from(matches!(t[2], b'x' | b's' | b't'))
}

/// Permission bits from an `ls`-style mode string (`-rwxr-sr-t`). Only the
/// nine permission characters are read; the leading type character is
/// `kind_from_listing`'s business.
fn mode_from_listing(modestr: &str) -> u32 {
    let b = modestr.as_bytes();
    if b.len() < 10 {
        return 0;
    }
    (triple_bits(&b[1..4]) << 6) | (triple_bits(&b[4..7]) << 3) | triple_bits(&b[7..10])
}

/// The file type an `ls`-style mode string's leading character names. A
/// symlink's own kind is never the answer — `squashfs_path_facts` follows it
/// and reports the target's — so it lands in `Other` and is overwritten there.
fn kind_from_listing(modestr: &str) -> Option<FileKind> {
    match modestr.as_bytes().first()? {
        b'-' => Some(FileKind::Regular),
        b'd' => Some(FileKind::Directory),
        _ => Some(FileKind::Other),
    }
}

/// The in-image path and symlink target on one listing line. The listing is
/// columnar, but a device node's size column is `major, minor` and a file
/// name may contain spaces, so the path is located by its `squashfs-root`
/// prefix rather than by counting fields. `None` is the image root itself.
fn path_and_link(line: &str, is_link: bool) -> Option<(String, Option<String>)> {
    let rest = line[line.find(" squashfs-root")? + 1..].strip_prefix("squashfs-root")?;
    let (path, link) = match rest.split_once(" -> ") {
        Some((p, target)) if is_link => (p, Some(target.to_string())),
        _ => (rest, None),
    };
    (!path.is_empty()).then(|| (path.to_string(), link))
}

/// Parse one `unsquashfs -ll` line into an in-image absolute path and its
/// entry.
fn parse_squashfs_line(line: &str) -> Option<(String, SquashEntry)> {
    let modestr = line.split_whitespace().next()?;
    let kind = kind_from_listing(modestr)?;
    let (path, link) = path_and_link(line, modestr.starts_with('l'))?;
    let facts = PathFacts {
        kind,
        mode: mode_from_listing(modestr),
    };
    Some((path, SquashEntry { facts, link }))
}

/// Read the squashfs listing for `image`, or say why it cannot be read.
///
/// Separated from the cache so the up-front `--sysroot` check and the
/// resolution path share one reader: a fault must read the same whether it is
/// found before compilation starts or during it.
fn read_squashfs_listing(image: &str) -> Result<HashMap<String, SquashEntry>, String> {
    let output = std::process::Command::new("unsquashfs")
        .args(["-ll", "-no-progress", image])
        .output()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "unsquashfs is not installed".to_string(),
            _ => format!("could not run unsquashfs: {e}"),
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let why = stderr
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("unsquashfs could not read it");
        return Err(why.to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_squashfs_line)
        .collect())
}

/// Reject a `--sysroot` the compiler cannot resolve against, before
/// compilation starts.
///
/// This is a *usage* error, not an SB-coded language error (DG, 2026-10-10 —
/// "I would not expect compilation to even start if the image was
/// unreadable"). The alternative was worse than useless: an unreadable image
/// yielded an empty listing, so every single command head in the program
/// reported SB5095 "no such file" and the one real fault was buried under a
/// diagnostic per line, each of them blaming the user's source for a problem
/// in the compiler's arguments.
///
/// A successful read *primes* the cache, so `unsquashfs` runs exactly once
/// even though the check happens long before the first path lookup.
fn check_sysroot(path: &str) {
    if let Some(why) = sysroot_fault(path) {
        eprintln!("sash: --sysroot={path}: {why}");
        std::process::exit(1);
    }
}

/// Why `path` cannot serve as a resolution root, or `None` if it can.
fn sysroot_fault(path: &str) -> Option<String> {
    if path.ends_with(".squashfs") || path.ends_with(".sqsh") {
        return image_sysroot_fault(path);
    }
    dir_sysroot_fault(path)
}

/// A successful read *primes* the cache, so `unsquashfs` runs exactly once
/// even though this happens long before the first path lookup.
fn image_sysroot_fault(path: &str) -> Option<String> {
    match read_squashfs_listing(path) {
        Ok(entries) => {
            *SQUASHFS_CACHE.lock().unwrap() = Some((path.to_string(), entries));
            None
        }
        Err(why) => Some(why),
    }
}

/// A directory sysroot has the identical failure shape to an unreadable image
/// — every lookup under a root that is not there reports "no such file" — so
/// it earns the same up-front answer.
fn dir_sysroot_fault(path: &str) -> Option<String> {
    match std::fs::metadata(path) {
        Ok(m) if m.is_dir() => None,
        Ok(_) => Some("not a directory".to_string()),
        Err(e) => Some(e.to_string()),
    }
}

/// The squashfs listing for `sysroot`.
///
/// `check_sysroot` reads the image once, before compilation starts, and
/// primes the cache, so this only ever hands back what is already there. The
/// lazy re-read this used to carry could not run — `check_flags` primes the
/// cache for the same `--sysroot` string that `root_path_facts` later asks
/// about — and a second, unreachable copy of the read is exactly the shape
/// that has hidden bugs here before: a fix lands on one copy and the other
/// quietly keeps the old behaviour.
///
/// The assert is orthogonal to the gate rather than a restatement of it:
/// `check_sysroot` judges the flag string, this judges the cache. It fires if
/// a second sysroot entry point appears, if the cache key changes, or if
/// priming becomes conditional.
fn squashfs_listing(
    sysroot: &str,
) -> std::sync::MutexGuard<'static, Option<(String, HashMap<String, SquashEntry>)>> {
    let cache = SQUASHFS_CACHE.lock().unwrap();
    assert!(
        matches!(cache.as_ref(), Some((root, _)) if root == sysroot),
        "check_sysroot primes SQUASHFS_CACHE for the --sysroot image before \
         compilation starts, so the listing for '{sysroot}' is already here"
    );
    cache
}

/// Why a resolution root has no runnable facts for a path.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PathMiss {
    /// Nothing is there.
    Absent,
    /// A symlink chain that did not terminate within the hop limit. Distinct
    /// from `Absent` on purpose: the end of a loop is not an empty directory
    /// entry, and reporting it as one is a wrong answer in true answer's
    /// clothing.
    LinkLoop,
}

/// Lexically resolve `.` and `..` against the components to their left. A
/// squashfs listing holds literal paths — no entry in it ever contains a `..`
/// — so a symlink target that uses one (`../lib/tool`, which is how real
/// images spell most cross-directory links) can only be found once the join
/// has been normalised. Without this the lookup misses and the path reports
/// `Absent`: a runnable program called a false reject, which is the one
/// failure SB5095 cannot afford.
///
/// `..` at the root stays at the root, as the kernel does.
fn normalize_path(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    format!("/{}", parts.join("/"))
}

/// Resolve `path` within a squashfs image, following symlinks. The hop limit
/// is what keeps a cyclic link from hanging the compiler; a target is read
/// relative to the link's own directory unless it is absolute, exactly as the
/// kernel would read it inside that image.
fn squashfs_path_facts(sysroot: &str, path: &str) -> Result<PathFacts, PathMiss> {
    let listing = squashfs_listing(sysroot);
    let entries = &listing.as_ref().ok_or(PathMiss::Absent)?.1;
    let mut current = normalize_path(path);
    for _ in 0..8 {
        let entry = entries.get(&current).ok_or(PathMiss::Absent)?;
        let Some(target) = &entry.link else {
            return Ok(entry.facts);
        };
        current = normalize_path(&if target.starts_with('/') {
            target.clone()
        } else {
            let dir = current.rsplit_once('/').map_or("", |(d, _)| d);
            format!("{}/{}", dir, target)
        });
    }
    Err(PathMiss::LinkLoop)
}

/// What the resolution root says about `path`: a directory sysroot, a
/// `.squashfs`/`.sqsh` image, or — with no `--sysroot` — the build host.
pub(crate) fn root_path_facts(sysroot: &Option<String>, path: &str) -> Result<PathFacts, PathMiss> {
    match sysroot {
        Some(sr) if sr.ends_with(".squashfs") || sr.ends_with(".sqsh") => {
            squashfs_path_facts(sr, path)
        }
        Some(sr) => host_path_facts(&format!("{}{}", sr, path)),
        None => host_path_facts(path),
    }
}

/// Build-host facts for `path`. Symlinks are followed (`metadata`, not
/// `symlink_metadata`): `/bin/sh -> dash` is a runnable program and the
/// compiler must say so. A chain that loops surfaces as `ELOOP`, which the
/// squashfs branch reaches by its own hop limit.
fn host_path_facts(path: &str) -> Result<PathFacts, PathMiss> {
    use std::os::unix::fs::PermissionsExt;
    let md = std::fs::metadata(path).map_err(|e| miss_from_io(&e))?;
    Ok(PathFacts {
        kind: kind_from_metadata(&md),
        mode: md.permissions().mode() & 0o7777,
    })
}

/// Which miss a failed `metadata` call is. Only `ELOOP` is worth separating:
/// every other error — absent, unreadable parent, bad name — leaves the
/// compiler with nothing at that path, which is `Absent`.
fn miss_from_io(e: &std::io::Error) -> PathMiss {
    const ELOOP: i32 = 40;
    if e.raw_os_error() == Some(ELOOP) {
        PathMiss::LinkLoop
    } else {
        PathMiss::Absent
    }
}

fn kind_from_metadata(md: &std::fs::Metadata) -> FileKind {
    if md.is_file() {
        FileKind::Regular
    } else if md.is_dir() {
        FileKind::Directory
    } else {
        FileKind::Other
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
       sash --extract <binary> [source|ir|manifest]

  -c 'script'         compile SCRIPT, given inline
  -o <output>         write the binary to OUTPUT
  -h, --help          print this help on stdout and exit 0
  --emit-c            emit C source on stdout instead of a binary
  --emit-ir           emit the IR as inert janet data on stdout (DD#104)
  --manifest          print the program manifest on stdout
  --extract <binary> [section]
                      print an embedded section of BINARY: source
                      (default), ir, or manifest (DD#45, DD#109)
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

    let program = parse_and_resolve(input, &source);
    let (bound, names) = bindings::resolve(&program, &source);
    let type_map = run_semantic_checks(input, &bound, &parsed.flags, &names);

    // Transform-time compile errors precede the dead-variable gate for every backend.
    let mut ssa = transform_or_exit(input, &bound, &names, type_map);
    ssa.source_names = names.clone();

    // DD#86: more-specific semantic/type/transform errors win over SB5050.
    let mut dead = validate::check_dead_variables(&bound);
    if !dead.is_empty() {
        restore_diagnostics(&mut dead, &names);
        print_diags(input, &dead);
        std::process::exit(1);
    }

    if maybe_emit_alt_output(input, &parsed.flags, &program, &ssa) {
        return;
    }

    compile_native(
        input,
        &source,
        &program,
        &ssa,
        parsed.output_path,
        parsed.inline_source.is_some(),
        &parsed.flags,
    );
}

fn parse_and_resolve(input: &str, source: &str) -> ir::Program {
    let (program, parse_diags) = match parse::parse(source) {
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

    program
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
    if let Some(path) = flags.iter().find_map(|f| f.strip_prefix("--sysroot=")) {
        check_sysroot(path);
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
    // SB5092 (DD#125), the *binding* half — the very first gate, on purpose.
    // A naming violation is logically prior to every semantic check: once a
    // program binds `status`, the DD#125 desugar reads a bare `status` in
    // `$(( … ))` as the intrinsic, and the SB5012 availability error that
    // follows is an artifact of the author's own clash, not a second fault.
    // Reporting the cause alone is the same judgement the SB5002 retain
    // below makes for a heredoc's cascade.
    //
    // `CommandTargets` deliberately stays in the later batch — see
    // `ReservedScope`.
    let reserved = validate::check_reserved_names(program, validate::ReservedScope::Bindings);
    if !reserved.is_empty() {
        print_diags(input, &reserved);
        std::process::exit(1);
    }
    let mut vdiags = validate::validate(program);
    vdiags.extend(validate::check_malformed_sed(program));
    vdiags.extend(validate::check_subscript_index(program));
    // DD#99: a source-level `echo -n` is retired. This is a language-surface
    // rule, so it runs with the structural checks — crucially BEFORE
    // `desugar_write`, which synthesises exactly that form internally.
    vdiags.extend(validate::check_echo_no_flags(program));
    // A heredoc leaks its body lines into the token stream as bare commands,
    // and its own `__HEREDOC__` marker reaches the command-head check, so the
    // batch carries SB5011s naming an internal sentinel. SB5002 is the real
    // error; report it alone rather than the cascade it provokes.
    if vdiags.iter().any(|d| d.code == "SB5002") {
        vdiags.retain(|d| d.code == "SB5002");
    }
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
            eprintln!("{}: {}", input, ssa.source_names.restore(ice));
        }
        std::process::exit(1);
    }
}

/// DD#82: `--no-taint` disables ALL taint checking (both the CST front-end
/// pass and the SSA backstop) — the full escape hatch.
fn ssa_taint_disabled(flags: &[&str]) -> bool {
    flags.contains(&"--no-taint")
}

/// Const writes, taint, positional, argv-splat, and type checks. Run after
/// resolution so the taint pass classifies on the resolved exec form, and on
/// the bound program so every local carries its scope.
fn run_semantic_checks(
    input: &str,
    program: &crate::ir::Program,
    flags: &[&str],
    names: &bindings::SourceNames,
) -> type_check::CheckedTypes {
    // SB4044 sits outside the taint switch: a const write is no taint
    // question, and `--no-taint` must not carry one into lowering, where
    // `VersionState::define` asserts it never arrives.
    let mut tdiags = validate::check_const_writes(program);
    if !cst_taint_disabled(flags) {
        tdiags.extend(validate::check_taint(program));
    }
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
    tdiags.extend(validate::check_scalar_splat(program));
    tdiags.extend(validate::check_collection_slots(program));
    tdiags.extend(validate::check_collection_args(program));
    tdiags.extend(validate::check_disposition_defaults(program));
    tdiags.extend(validate::check_elide_positions(program));
    tdiags.extend(validate::check_args_immutable(program));
    tdiags.extend(validate::check_reserved_loop_var(program));
    // SB5092 (DD#125), the *command out-param* half. Sits in this batch, not
    // with the binding half in `run_structural_checks`, because it reads a
    // name out of a shape an earlier check may already have rejected —
    // `read -a arr -d :` is SB5057, and the read-target extractor hands back
    // the `-d` flag's argument `:`. From the earlier batch it masked the
    // arity error entirely and asked the user to rename a delimiter.
    tdiags.extend(validate::check_reserved_names(
        program,
        validate::ReservedScope::CommandTargets,
    ));
    tdiags.extend(validate::check_loop_control_context(program));
    tdiags.extend(validate::check_discard_reads(program));
    tdiags.extend(validate::check_readline_arity(program));
    tdiags.extend(validate::check_match_arity(program));
    tdiags.extend(validate::check_argv_form(program));
    tdiags.extend(validate::check_map_from_array(program));
    tdiags.extend(validate::check_assign_value_shape(program));
    tdiags.extend(validate::check_attest_predicate_code(program));
    if !tdiags.is_empty() {
        restore_diagnostics(&mut tdiags, names);
        print_diags(input, &tdiags);
        std::process::exit(1);
    }

    type_check_or_exit(input, program, names)
}

/// Type check, reporting every SB4030 before exiting. Split out of
/// `run_semantic_checks` so that function stays a flat list of passes.
fn type_check_or_exit(
    input: &str,
    program: &crate::ir::Program,
    names: &bindings::SourceNames,
) -> type_check::CheckedTypes {
    let type_errors = match type_check::type_check(program) {
        Ok(types) => return types,
        Err(errors) => errors,
    };
    for te in &type_errors {
        eprintln!(
            "{}:{}:{}: error [SB4030] {}",
            input,
            te.span.line,
            te.span.col,
            names.restore(&te.message)
        );
    }
    std::process::exit(1);
}

/// Run the AST→SSA transform **once** for the whole compile, exit on its
/// compile-error diagnostics (SB4016/17/18/…), and hand the SSA back for
/// reuse by the taint gate and codegen.
///
/// There used to be three of these per native compile: this one discarded the
/// SSA and kept the diags, `compile_native` discarded the diags and kept the
/// SSA, and `codegen::compile_to_object` re-parsed the source and built a
/// third. That is ~a quarter of native compile time spent recomputing an
/// identical result, and it meant the taint gate and codegen ran on different
/// SSA objects with nothing forcing them to agree (journal 348).
///
/// Runs before `maybe_emit_alt_output` so the alt-output backends reject
/// exactly the programs native does.
fn transform_or_exit(
    input: &str,
    program: &crate::ir::Program,
    names: &bindings::SourceNames,
    type_map: type_check::CheckedTypes,
) -> crate::ssa::SsaProgram {
    let (ssa, diags) = transform::ir_to_ssa_typed(program, type_map);
    if !diags.is_empty() {
        for d in &diags {
            eprintln!("{}:{}", input, names.restore(d));
        }
        std::process::exit(1);
    }
    ssa
}

/// Handle the terminal "emit an alternative output and exit" flags
/// (`--emit-ir`, `--emit-c`, `--manifest`). Returns `true` if one
/// fired (caller should return); `false` if none matched (caller
/// proceeds to native codegen).
fn maybe_emit_alt_output(
    input: &str,
    flags: &[&str],
    program: &crate::ir::Program,
    ssa: &crate::ssa::SsaProgram,
) -> bool {
    if flags.contains(&"--emit-ir") {
        print!("{}", emit_ir::emit_ir(program));
        return true;
    }
    if flags.contains(&"--emit-c") {
        emit_c_to_stdout(input, flags, ssa);
        return true;
    }
    if flags.contains(&"--manifest") {
        emit_manifest(ssa);
        return true;
    }
    false
}

/// `--emit-c`: lower to SSA, emit C source to stdout. The C oracle
/// (notes/tasks/add-c-oracle.md). gcc-compiled output of this C
/// linked against libsash_rt.a is a differential oracle against
/// Cranelift-compiled output of the same SSA — any disagreement
/// is a bug somewhere in the compiler.
fn emit_c_to_stdout(input: &str, flags: &[&str], ssa: &crate::ssa::SsaProgram) {
    // DD#82: the SSA taint backstop runs here too. It is the *second* layer —
    // the CST pass normally rejects first — but `--no-taint-cst` disables that
    // one, and the backstop also covers documented CST gaps (see the note at
    // validate.rs:2569). Omitting it here left `--emit-c` with no taint
    // enforcement whatsoever under `--no-taint-cst`: `x=$(getenv HOME); echo $x`
    // compiled and printed the value, where native exits 1. The c backend is
    // also the differential oracle, so it has to run the same gates or the
    // comparison is between two different compilers.
    run_ssa_taint_gate(input, ssa, flags);
    print!("{}", emit_c::emit_c(ssa));
}

/// `--extract <binary> [source|ir|manifest]`: dump one embedded artifact
/// section from a compiled binary. The section defaults to `source`, so the
/// DD#45 spelling `--extract <binary>` keeps its original meaning.
fn dump_sash_source(args: &[String]) {
    if args.len() < 3 {
        eprintln!("{USAGE}");
        std::process::exit(1);
    }
    let binary_path = &args[2];
    let which = args.get(3).map(String::as_str).unwrap_or("source");
    let section = match which {
        "source" => ".sash_source",
        "ir" => ".sash_ir",
        "manifest" => ".sash_manifest",
        other => {
            eprintln!(
                "sash: --extract: unknown section `{other}` (expected source, ir, or manifest)"
            );
            std::process::exit(1);
        }
    };
    let data = std::fs::read(binary_path).unwrap_or_else(|e| {
        eprintln!("sash: {}: {}", binary_path, e);
        std::process::exit(1);
    });
    match read_artifact_section(&data, section) {
        Ok(source) => print!("{source}"),
        Err(msg) => {
            eprintln!("sash: {binary_path}: {msg}");
            std::process::exit(1);
        }
    }
}

/// Read one embedded artifact section body out of an object-file image.
///
/// An unreadable section reports the underlying error rather than claiming
/// the section is absent, which is what the previous loop did.
fn read_artifact_section(image: &[u8], section: &str) -> Result<String, String> {
    use object::read::{Object, ObjectSection};
    let obj =
        object::read::File::parse(image).map_err(|e| format!("not a valid object file: {e}"))?;
    let Some(found) = obj.sections().find(|s| s.name() == Ok(section)) else {
        if empty_artifact_section(&obj, section) {
            return Ok(String::new());
        }
        return Err(format!("no {section} section found"));
    };
    let body = found
        .data()
        .map_err(|e| format!("{section}: unreadable: {e}"))?;
    std::str::from_utf8(body)
        .map(str::to_owned)
        .map_err(|e| format!("{section}: invalid UTF-8: {e}"))
}

/// The linker drops empty source/manifest sections; the mandatory, nonempty IR
/// also identifies an empty-source binary. Source keeps older outputs readable.
fn empty_artifact_section(obj: &object::read::File<'_>, section: &str) -> bool {
    use object::read::Object;
    matches!(section, ".sash_source" | ".sash_manifest")
        && (obj.section_by_name(".sash_source").is_some()
            || obj.section_by_name(".sash_ir").is_some())
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
        eprintln!(
            "{}:{}:{}: error [{}] {}",
            input, d.line, d.col, d.code, d.message
        );
    }
}

fn restore_diagnostics(diags: &mut [diagnostic::Diagnostic], names: &bindings::SourceNames) {
    for diag in diags {
        diag.message = names.restore(&diag.message);
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
fn emit_manifest(ssa: &crate::ssa::SsaProgram) {
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
    ssa: &crate::ssa::SsaProgram,
    output_path: Option<&str>,
    is_inline: bool,
    flags: &[&str],
) {
    let output = native_output_path(input, output_path, is_inline);
    let obj_path = format!("{}.o", output);

    run_ssa_taint_gate(input, ssa, flags);

    let trace = flags.contains(&"--trace");
    let obj_bytes = match codegen::compile_to_object(program, ssa, source, trace) {
        Ok(bytes) => bytes,
        Err(diags) => {
            for d in &diags {
                eprintln!("{}:{}", input, ssa.source_names.restore(d));
            }
            std::process::exit(1);
        }
    };
    std::fs::write(&obj_path, &obj_bytes).expect("write .o");

    link_object(&obj_path, &output);

    println!("sash: {} -> {} (static binary)", input, output);
}

/// Facts about `unsquashfs -ll` output format that no spec can reach.
///
/// `spec/cli.sh` `sq.1`-`sq.15` resolve real commands against a real image
/// and cover the rest of this parser. They cannot cover a path containing a
/// space: a command head with a space in it is rejected by SB5011 long before
/// resolution, so the listing line can never be the thing under test. The
/// sample lines below are verbatim `unsquashfs -ll` output.
#[cfg(test)]
mod invariant_tests {
    use super::*;

    fn parse(line: &str) -> (String, SquashEntry) {
        parse_squashfs_line(line).expect("line should parse")
    }

    #[test]
    fn path_may_contain_a_space() {
        let (path, entry) =
            parse("-rwxr-xr-x dg/dg                 39256 2026-10-09 22:21 squashfs-root/my tool");
        assert_eq!(
            path, "/my tool",
            "the name is not the last whitespace field"
        );
        assert_eq!(entry.facts.kind, FileKind::Regular);
        assert!(entry.facts.is_executable());
    }

    #[test]
    fn device_size_column_is_two_fields() {
        // `1,  5` where every other line carries a single size. A parser that
        // counts columns reads the path from the wrong place on this line.
        let (path, entry) =
            parse("crw-rw-rw- root/root             1,  5 2026-10-09 22:16 squashfs-root/dev/zero");
        assert_eq!(path, "/dev/zero");
        assert_eq!(entry.facts.kind, FileKind::Other);
        assert!(!entry.facts.is_executable(), "a device is not a program");
    }

    #[test]
    fn symlink_target_is_split_off() {
        let (path, entry) = parse(
            "lrwxrwxrwx dg/dg                    15 2026-10-09 22:18 squashfs-root/usr/bin/good -> ../lib/realtool",
        );
        assert_eq!(path, "/usr/bin/good");
        assert_eq!(entry.link.as_deref(), Some("../lib/realtool"));
    }

    #[test]
    fn an_arrow_in_a_regular_file_name_is_not_a_target() {
        // Only a mode string starting with `l` makes " -> " a separator; a
        // regular file is free to have one in its name.
        let (path, entry) =
            parse("-rw-r--r-- dg/dg                    7 2026-10-09 22:21 squashfs-root/a -> b");
        assert_eq!(path, "/a -> b");
        assert_eq!(entry.link, None);
    }

    #[test]
    fn the_image_root_itself_is_not_an_entry() {
        assert!(
            parse_squashfs_line(
                "drwxrwxr-x dg/dg                   124 2026-10-09 22:16 squashfs-root"
            )
            .is_none(),
            "the root would key the empty path"
        );
    }

    #[test]
    fn setuid_case_decides_the_execute_bit() {
        let (_, upper) =
            parse("-rwSr--r-- dg/dg                 39256 2026-10-09 22:16 squashfs-root/x");
        let (_, lower) =
            parse("-rwsr-xr-x dg/dg                 39256 2026-10-09 22:21 squashfs-root/y");
        assert_eq!(upper.facts.mode, 0o644, "uppercase S holds no execute bit");
        assert_eq!(lower.facts.mode, 0o755, "lowercase s holds one");
    }

    #[test]
    fn a_malformed_mode_string_does_not_panic() {
        // `triple_bits` indexes bytes 1..10, so a short mode string would be
        // an out-of-bounds panic — the compiler crashing on whatever an
        // `unsquashfs` we do not control decided to print. The guard turns
        // that into "no permissions", which cannot be mistaken for runnable.
        let (path, entry) = parse("x dg/dg 1 2026-10-09 22:21 squashfs-root/odd");
        assert_eq!(path, "/odd");
        assert_eq!(entry.facts.mode, 0, "an unreadable mode grants nothing");
        assert!(!entry.facts.is_executable());
    }

    #[test]
    fn dotdot_cannot_climb_past_the_root() {
        // The kernel clamps here; a `pop` on an empty component list has to
        // do the same, or a target of `../..` would escape the image.
        assert_eq!(normalize_path("/../../etc/passwd"), "/etc/passwd");
        assert_eq!(normalize_path("/usr/bin/../lib/x"), "/usr/lib/x");
        assert_eq!(normalize_path("/./a//b/"), "/a/b");
    }

    /// `squashfs_listing` hands back a cache `check_sysroot` has already
    /// primed. It used to re-read the image when the cache missed — code that
    /// could not run, because `check_flags` primes for the same `--sysroot`
    /// string `root_path_facts` later asks about, and a second unreachable
    /// copy of a read is how a fix lands on one copy and misses the other.
    ///
    /// The spec suite cannot test this: it can only run `sash`, and every way
    /// of reaching the listing goes through the priming. So reach past the
    /// CLI and ask for an image the cache does not hold. The panic IS the
    /// claim that the invariant still holds; if this stops panicking, a path
    /// to the listing has appeared that does not prime, which is the finding.
    #[test]
    fn listing_requires_the_cache_to_be_primed() {
        let caught = std::panic::catch_unwind(|| {
            drop(super::squashfs_listing(
                "/no_such_image_for_this_test.squashfs",
            ));
        });
        let err = caught.expect_err("an unprimed listing must not be answered");
        let msg = err
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default();
        assert!(
            msg.contains("check_sysroot primes"),
            "the panic must name the invariant that guarantees it, got: {msg}"
        );
    }
}

#[cfg(test)]
mod native_error_tests {
    #[test]
    fn codegen_errors_exit_with_annotated_diagnostics() {
        const CHILD: &str = "SASH_TEST_NATIVE_CODEGEN_ERROR_CHILD";
        if std::env::var_os(CHILD).is_some() {
            let program = crate::ir::Program {
                statements: vec![],
                end_block: None,
            };
            let mut ssa = crate::ssa::SsaProgram::new();
            let entry = ssa.new_block("entry");
            ssa.block_mut(entry)
                .insts
                .push(crate::ssa::SsaInst::command("join", vec![], vec![], false));
            super::compile_native(
                "native-codegen-fixture.sash",
                "",
                &program,
                &ssa,
                Some("/dev/null/not-created"),
                false,
                &[],
            );
            unreachable!("the native driver must exit on a codegen error");
        }
        let output = std::process::Command::new(std::env::current_exe().expect("unit test binary"))
            .args([
                "--exact",
                "native_error_tests::codegen_errors_exit_with_annotated_diagnostics",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .expect("run isolated native-driver failure");
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let stderr = String::from_utf8(output.stderr).expect("diagnostic UTF-8");
        assert!(
            stderr.contains(
                "native-codegen-fixture.sash:ICE: Join is an expression intrinsic resolved in transform"
            ),
            "{stderr}"
        );
        let stdout = String::from_utf8(output.stdout).expect("test-runner UTF-8");
        assert!(!stdout.contains("(static binary)"), "{stdout}");
    }
}
