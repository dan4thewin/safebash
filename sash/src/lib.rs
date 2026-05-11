// Modules used by tests (oracle_tests.rs) — must be pub
pub mod ir;
pub mod lower;
pub mod emit_janet;
pub mod oracle;

// Internal modules — not used outside the crate
pub(crate) mod printer;
pub(crate) mod ssa;
pub(crate) mod ssa_taint;
pub(crate) mod transform;
pub(crate) mod ssa_printer;
pub mod type_check;
pub(crate) mod emit_source;
pub mod codegen;
pub(crate) mod diagnostic;
pub(crate) mod validator;
pub(crate) mod rule;
pub(crate) mod rules;
pub(crate) mod taint;

pub fn parse(source: &str) -> tree_sitter::Tree {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_bash::LANGUAGE.into())
        .expect("Error loading Bash grammar");
    parser.parse(source, None).expect("Failed to parse")
}

/// DD#58: Preprocess source to protect sexpr syntax inside arithmetic.
/// Replaces `$(( content ))` with `$(( __ARITH_N__ ))` when content
/// contains sexpr patterns that tree-sitter can't parse.
/// Returns (preprocessed_source, map of placeholder → original content).
pub(crate) fn preprocess_arithmetic(source: &str) -> (String, std::collections::HashMap<String, String>) {
    use std::collections::HashMap;
    let mut result = String::with_capacity(source.len());
    let mut map = HashMap::new();
    let mut counter = 0usize;
    let bytes = source.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Look for $(( or ((
        let (is_dollar, start) = if i + 3 <= bytes.len() && &bytes[i..i+3] == b"$((" {
            (true, i + 3)
        } else if i + 2 <= bytes.len() && &bytes[i..i+2] == b"(("
            // Don't match (( inside words — must be at start of line or after whitespace/;
            && (i == 0 || matches!(bytes[i-1], b' ' | b'\t' | b'\n' | b';' | b'(' | b'!')) {
            (false, i + 2)
        } else {
            // Copy one UTF-8 character (may be multi-byte)
            let ch_len = utf8_char_len(bytes[i]);
            result.push_str(&source[i..i+ch_len]);
            i += ch_len;
            continue;
        };

        // Find matching )) respecting nesting
        let mut depth = 1i32;
        let mut j = start;
        while j + 1 < bytes.len() && depth > 0 {
            if j + 2 < bytes.len() && &bytes[j..j+3] == b"$((" {
                depth += 1; j += 3; continue;
            }
            if &bytes[j..j+2] == b"((" {
                depth += 1; j += 2; continue;
            }
            if &bytes[j..j+2] == b"))" {
                depth -= 1;
                if depth == 0 { break; }
                j += 2; continue;
            }
            j += 1;
        }
        if depth != 0 {
            // Unmatched — emit as-is
            if is_dollar { result.push_str("$(("); } else { result.push_str("(("); }
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
            if is_dollar { result.push_str("$(("); } else { result.push_str("(("); }
            result.push_str(content);
            result.push_str("))");
        }
        i = end;
    }
    (result, map)
}

fn utf8_char_len(b: u8) -> usize {
    if b < 0x80 { 1 }
    else if b < 0xE0 { 2 }
    else if b < 0xF0 { 3 }
    else { 4 }
}

fn has_sexpr_pattern(content: &str) -> bool {
    let bytes = content.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'(' {
            // Skip whitespace after (
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() { j += 1; }
            // Check if next token is an identifier (not a digit or operator)
            if j < bytes.len() && (bytes[j].is_ascii_alphabetic() || bytes[j] == b'_') {
                // Read the identifier
                let id_start = j;
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') { j += 1; }
                // After identifier, must have whitespace or ) — NOT an operator
                if j < bytes.len() && (bytes[j].is_ascii_whitespace() || bytes[j] == b')') {
                    // Check it's not just (var) which is valid grouping
                    while j < bytes.len() && bytes[j].is_ascii_whitespace() { j += 1; }
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

use std::sync::Mutex;
use std::collections::HashMap;

static ARITH_MAP: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

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
        eprintln!("Usage: sash <script.sh> [-o <output>] [--janet] [--ir] [--ssa] [--emit-source] [--extract <binary>]");
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
                if i < args.len() { output_path = Some(&args[i]); }
            }
            "-c" => {
                i += 1;
                if i < args.len() { inline_source = Some(&args[i]); }
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
            eprintln!("Usage: sash [--nolint] [--ir|--ssa|--emit-source|--annotate] [-c 'script'] [-o output] <script.sh>");
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

    // DD#58: preprocess sexpr syntax in arithmetic before parsing
    let (source, arith_map) = preprocess_arithmetic(&source);
    set_arith_map(arith_map);

    // Parse
    let tree = parse(&source);

    // --nolint skips shellcheck only; subset gate + taint always run
    let nolint = flags.contains(&"--nolint") || inline_source.is_some();
    if !nolint {
        match Command::new("shellcheck")
            .args(["--enable=all", "--exclude=SC2248,SC2250,SC2086,SC2046,SC2148,SC2030,SC2031", "--format=gcc", input])
            .output()
        {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stdout);
                    for line in stderr.lines() {
                        eprintln!("{}", line);
                    }
                    eprintln!("sash: shellcheck found issues (use --nolint to skip)");
                    std::process::exit(1);
                }
            }
            Err(_) => {
                eprintln!("sash: warning: shellcheck not found — install shellcheck for intent verification");
            }
        }
    }

    // Subset gate + taint tracking
    {
        let all_rules = rules::all_rules();
        let mut diags = validator::validate(tree.root_node(), source.as_bytes(), &all_rules);

        let mut tracker = taint::TaintTracker::new();
        diags.extend(tracker.analyze(tree.root_node(), source.as_bytes()));

        diags.sort_by_key(|d| (d.line, d.col));

        let is_emit_source = flags.contains(&"--emit-source");
        let mut has_error = false;
        for d in &diags {
            let prefix = match d.severity {
                diagnostic::Severity::Error => { has_error = true; "error" }
                diagnostic::Severity::Deferred => { has_error = true; "deferred" }
                diagnostic::Severity::Marker => "note",
            };
            eprintln!("{}:{}:{}: {} [{}] {}", input, d.line, d.col, prefix, d.code, d.message);
        }
        if has_error && !is_emit_source {
            std::process::exit(1);
        }
    }

    // Lower to IR
    emit_source::clear_coercions();
    let program = lower::lower_program(tree.root_node(), source.as_bytes());

    if flags.contains(&"--emit-source") {
        let annotate = flags.contains(&"--annotate");
        let result = emit_source::emit_source(source.as_bytes(), annotate);
        print!("{}", result);
        return;
    }
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

    // Build SSA for the taint verification pass
    let tree2 = parse(&source);
    let program2 = lower::lower_program(tree2.root_node(), source.as_bytes());
    let type_map2 = type_check::infer_types(&program2);
    let (ssa2, _) = transform::ir_to_ssa_typed(&program2, type_map2);
    let taint_ices = ssa_taint::verify_ssa_taint(&ssa2);
    if !taint_ices.is_empty() {
        for ice in &taint_ices {
            eprintln!("{}: {}", input, ice);
        }
        std::process::exit(1);
    }

    let obj_bytes = match codegen::compile_to_object(&source) {
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
        .args(["-o", &output, &obj_path, &rt_path, "-static", "--gc-sections",
               "-z", "noexecstack"])
        .status()
        .expect("ld failed");

    let _ = std::fs::remove_file(&obj_path);

    if !ld.success() {
        eprintln!("sash: linking failed");
        std::process::exit(1);
    }

    println!("sash: {} -> {} (static binary)", input, output);
}
