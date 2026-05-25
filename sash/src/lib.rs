// Modules used by tests (oracle_tests.rs) — must be pub
pub mod emit_janet;
pub mod ir;
pub mod lower;
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

/// DD#56: Preprocess END { ... } into fn __sash_end__ { ... }
/// so tree-sitter parses it as a function definition.
pub(crate) fn preprocess_end_block(source: &str) -> String {
    // Match `END` at start of line (or start of source) followed by optional whitespace and `{`
    let mut result = String::with_capacity(source.len());
    let mut remaining = source;
    while let Some(pos) = remaining.find("END") {
        // Check it's at line start (pos==0 or preceded by newline)
        let at_line_start = pos == 0 || remaining.as_bytes()[pos - 1] == b'\n';
        let after = &remaining[pos + 3..];
        let followed_by_brace = after.trim_start().starts_with('{');
        if at_line_start && followed_by_brace {
            result.push_str(&remaining[..pos]);
            result.push_str("fn __sash_end__");
            remaining = after;
        } else {
            result.push_str(&remaining[..pos + 3]);
            remaining = after;
        }
    }
    result.push_str(remaining);
    result
}

/// DD#58: Preprocess source to protect sexpr syntax inside arithmetic.
/// DD#18: Preprocess `local fn name` → `fn __local__name`
/// so tree-sitter parses it as a function_definition.
pub(crate) fn preprocess_local_fn(source: &str) -> String {
    // Match `local fn NAME` and replace with `fn __local__NAME`
    let mut result = String::with_capacity(source.len());
    for line in source.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("local fn ") {
            let indent = &line[..line.len() - trimmed.len()];
            result.push_str(indent);
            // rest is "name { ..." or "name {" etc
            if let Some(space_pos) = rest.find([' ', '{']) {
                let name = &rest[..space_pos].trim();
                let remainder = &rest[space_pos..];
                result.push_str(&format!("fn __local__{}{}", name, remainder));
            } else {
                result.push_str(&format!("fn __local__{}", rest));
            }
        } else {
            result.push_str(line);
        }
        result.push('\n');
    }
    // Remove trailing newline if source didn't have one
    if !source.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }
    result
}

/// DD#58: Preprocess source to protect sexpr syntax inside arithmetic.
/// Replaces `$(( content ))` with `$(( __ARITH_N__ ))` when content
/// contains sexpr patterns that tree-sitter can't parse.
/// Returns (preprocessed_source, map of placeholder → original content).
pub(crate) fn preprocess_arithmetic(
    source: &str,
) -> (String, std::collections::HashMap<String, String>) {
    use std::collections::HashMap;
    let mut result = String::with_capacity(source.len());
    let mut map = HashMap::new();
    let mut counter = 0usize;
    let bytes = source.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Look for $(( or ((
        let (is_dollar, start) = if i + 3 <= bytes.len() && &bytes[i..i + 3] == b"$((" {
            (true, i + 3)
        } else if i + 2 <= bytes.len() && &bytes[i..i+2] == b"(("
            // Don't match (( inside words — must be at start of line or after whitespace/;
            && (i == 0 || matches!(bytes[i-1], b' ' | b'\t' | b'\n' | b';' | b'(' | b'!'))
        {
            (false, i + 2)
        } else {
            // Copy one UTF-8 character (may be multi-byte)
            let ch_len = utf8_char_len(bytes[i]);
            result.push_str(&source[i..i + ch_len]);
            i += ch_len;
            continue;
        };

        // Find matching )) respecting nesting
        let mut depth = 1i32;
        let mut j = start;
        while j + 1 < bytes.len() && depth > 0 {
            if j + 2 < bytes.len() && &bytes[j..j + 3] == b"$((" {
                depth += 1;
                j += 3;
                continue;
            }
            if &bytes[j..j + 2] == b"((" {
                depth += 1;
                j += 2;
                continue;
            }
            if &bytes[j..j + 2] == b"))" {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                j += 2;
                continue;
            }
            j += 1;
        }
        if depth != 0 {
            // Unmatched — emit as-is
            if is_dollar {
                result.push_str("$((");
            } else {
                result.push_str("((");
            }
            i = start;
            continue;
        }

        let content = &source[start..j];
        let end = j + 2; // skip ))

        // Check if content has sexpr patterns: ( followed by identifier then space
        // Simple heuristic: contains a paren followed by alpha then space then non-)
        let needs_placeholder = has_sexpr_pattern(content);

        if needs_placeholder {
            let key = format!("__ARITH_{}__", counter);
            counter += 1;
            map.insert(key.clone(), content.trim().to_string());
            if is_dollar {
                result.push_str(&format!("$(({}))", key));
            } else {
                result.push_str(&format!("(({}))", key));
            }
        } else {
            // No sexpr — emit unchanged
            if is_dollar {
                result.push_str("$((");
            } else {
                result.push_str("((");
            }
            result.push_str(content);
            result.push_str("))");
        }
        i = end;
    }
    (result, map)
}

fn utf8_char_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b < 0xE0 {
        2
    } else if b < 0xF0 {
        3
    } else {
        4
    }
}

fn has_sexpr_pattern(content: &str) -> bool {
    let bytes = content.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'(' {
            // Skip whitespace after (
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            // Check if next token is an identifier (not a digit or operator)
            if j < bytes.len() && (bytes[j].is_ascii_alphabetic() || bytes[j] == b'_') {
                // Read the identifier
                let _id_start = j;
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                    j += 1;
                }
                // After identifier, must have whitespace or ) — NOT an operator
                if j < bytes.len() && (bytes[j].is_ascii_whitespace() || bytes[j] == b')') {
                    // Check it's not just (var) which is valid grouping
                    while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    if j < bytes.len() && bytes[j] != b')' {
                        return true;
                    }
                }
            }
        }
        i += 1;
    }
    false
}

use std::collections::HashMap;
use std::sync::Mutex;

static ARITH_MAP: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
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

/// Store the arithmetic placeholder map for the current compilation.
pub(crate) fn set_arith_map(map: HashMap<String, String>) {
    *ARITH_MAP.lock().unwrap() = Some(map);
}

/// Look up an arithmetic placeholder.
pub(crate) fn lookup_arith(key: &str) -> Option<String> {
    ARITH_MAP.lock().unwrap().as_ref()?.get(key).cloned()
}

use std::process::Command;

pub fn cli_main(args: Vec<String>) {
    if args.len() < 2 {
        eprintln!("Usage: sash <script.sh> [-o <output>] [--janet] [--ir] [--ssa] [--manifest] [--extract <binary>]");
        std::process::exit(1);
    }

    // --extract: dump .sash_source section from a compiled binary
    if args[1] == "--extract" {
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

    // Split args into flags and positional
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


    // Read source
    let source = if let Some(src) = inline_source {
        src.to_string()
    } else {
        match std::fs::read_to_string(input) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("sash: {}: {}", input, e);
                std::process::exit(1);
            }
        }
    };

    // DD#23: --sysroot=<path> for cross-compilation path resolution
    for flag in &flags {
        if let Some(path) = flag.strip_prefix("--sysroot=") {
            set_sysroot(path.to_string());
        }
    }

    // DD#57: reject converter flags
    for flag in &flags {
        if matches!(*flag, "--emit-source" | "--annotate" | "--nolint") {
            eprintln!(
                "sash: '{}' is not a valid flag — use sash-convert for source transformations",
                flag
            );
            std::process::exit(1);
        }
    }

    // Parse with new hand-written parser
    let (program, parse_diags) = match parse::parse(&source) {
        Ok(result) => result,
        Err(diags) => {
            for d in &diags {
                let prefix = match d.severity {
                    diagnostic::Severity::Error => "error",
                    diagnostic::Severity::Deferred => "deferred",
                };
                eprintln!(
                    "{}:{}:{}: {} [{}] {}",
                    input, d.line, d.col, prefix, d.code, d.message
                );
            }
            std::process::exit(1);
        }
    };

    // Emit non-fatal parse diagnostics
    for d in &parse_diags {
        let prefix = match d.severity {
            diagnostic::Severity::Error => "error",
            diagnostic::Severity::Deferred => "deferred",
        };
        eprintln!(
            "{}:{}:{}: {} [{}] {}",
            input, d.line, d.col, prefix, d.code, d.message
        );
    }

    // TODO: Validator rules and taint tracking will be reimplemented
    // as IR walkers. Tree-sitter removed.

    if flags.contains(&"--ir") {
        let ir = printer::print_program(&program);
        print!("{}", ir);
        return;
    }
    if flags.contains(&"--janet") {
        let janet = emit_janet::emit_janet(&program);
        print!("{}", janet);
        return;
    }
    if flags.contains(&"--ssa") {
        let (ssa, _diags) = transform::ir_to_ssa(&program);
        let ssa_text = ssa_printer::print_ssa(&ssa);
        print!("{}", ssa_text);
        return;
    }
    if flags.contains(&"--manifest") {
        let type_map = type_check::infer_types(&program);
        let (ssa, _) = transform::ir_to_ssa_typed(&program, type_map);
        let mut deps: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let builtins: std::collections::HashSet<&str> = [
            "echo", "true", "false", ":", "exit", "cd", "pwd", "wait",
            "shift", "read", "exec", "command", "[[",
            "__redirect_push", "__redirect_pop", "__store_bg_pid",
            "__subshell_wait", "__table_delete", "printf", "signal",
        ].iter().copied().collect();
        let path_var = std::env::var("PATH").unwrap_or_default();
        let sysroot = get_sysroot();
        let resolve = |name: &str| -> Option<String> {
            if name.starts_with('/') { return Some(name.to_string()); }
            if builtins.contains(name) { return None; }
            for dir in path_var.split(':') {
                let target_path = format!("{}/{}", dir, name);
                let exists = if let Some(ref sr) = sysroot {
                    crate::sysroot_file_exists(sr, &target_path)
                } else {
                    std::path::Path::new(&target_path).is_file()
                };
                if exists { return Some(target_path); }
            }
            Some(name.to_string())
        };
        for block in ssa.blocks.values() {
            for inst in &block.insts {
                if let ssa::SsaInst::Command { name, .. } = inst {
                    if let Some(path) = resolve(name) { deps.insert(path); }
                }
                if let ssa::SsaInst::Pipeline { stages, .. } = inst {
                    for (name, _, _) in stages {
                        if let Some(path) = resolve(name) { deps.insert(path); }
                    }
                }
            }
        }
        for dep in &deps { println!("{}", dep); }
        return;
    }

    // Default: compile to native binary
    let output = if let Some(p) = output_path {
        p.to_string()
    } else if inline_source.is_some() {
        "a.out".to_string()
    } else {
        input
            .strip_suffix(".sh")
            .or_else(|| input.strip_suffix(".bash"))
            .unwrap_or(&format!("{}.out", input))
            .to_string()
    };

    let obj_path = format!("{}.o", output);

    // Type checking
    let type_map = type_check::infer_types(&program);
    let type_errors = type_check::type_check(&program);
    if !type_errors.is_empty() {
        for te in &type_errors {
            eprintln!(
                "{}:{}:{}: error [SB4030] {}",
                input, te.span.line, te.span.col, te.message
            );
        }
        std::process::exit(1);
    }

    // SSA taint verification
    let (ssa, _) = transform::ir_to_ssa_typed(&program, type_map);
    let taint_ices = ssa_taint::verify_ssa_taint(&ssa);
    if !taint_ices.is_empty() {
        for ice in &taint_ices {
            eprintln!("{}: {}", input, ice);
        }
        std::process::exit(1);
    }

    let trace = flags.contains(&"--trace");
    let obj_bytes = match codegen::compile_to_object(&source, trace) {
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
