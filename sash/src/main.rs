use std::process::Command;

fn main() {
    let args: Vec<String> = std::env::args().collect();

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

    let input = &args[1];
    let flags: Vec<&str> = args[2..].iter().map(|s| s.as_str()).collect();

    // Read source
    let source = match std::fs::read_to_string(input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("sash: {}: {}", input, e);
            std::process::exit(1);
        }
    };

    // Parse
    let tree = sash::parse(&source);

    // --nolint skips shellcheck only; subset gate + taint always run
    let nolint = flags.contains(&"--nolint");
    if !nolint {
        // Shellcheck pre-flight (DD#40 — lint by default, --nolint to skip)
        match Command::new("shellcheck")
            .args(["--enable=all", "--exclude=SC2248,SC2250,SC2086,SC2046", "--format=gcc", input])
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

    // Subset gate + taint tracking — always run (sash safety guarantees are structural)
    {
        let rules = sash::rules::all_rules();
        let mut diags = sash::validator::validate(tree.root_node(), source.as_bytes(), &rules);

        let mut tracker = sash::taint::TaintTracker::new();
        diags.extend(tracker.analyze(tree.root_node(), source.as_bytes()));

        diags.sort_by_key(|d| (d.line, d.col));

        let mut has_error = false;
        for d in &diags {
            let prefix = match d.severity {
                sash::diagnostic::Severity::Error => { has_error = true; "error" }
                sash::diagnostic::Severity::Deferred => { has_error = true; "deferred" }
                sash::diagnostic::Severity::Marker => "note",
            };
            eprintln!("{}:{}:{}: {} [{}] {}", input, d.line, d.col, prefix, d.code, d.message);
        }
        if has_error {
            std::process::exit(1);
        }
    }

    // Lower to IR
    // Clear coercion state, then lower to IR (records coercions)
    sash::emit_source::clear_coercions();
    let program = sash::lower::lower_program(tree.root_node(), source.as_bytes());

    // --emit-source: apply coercions, print sash-native source, exit
    if flags.contains(&"--emit-source") {
        let annotate = flags.contains(&"--annotate");
        let result = sash::emit_source::emit_source(source.as_bytes(), annotate);
        print!("{}", result);
        return;
    }

    // --ir: print IR and exit
    if flags.contains(&"--ir") {
        let ir = sash::printer::print_program(&program);
        print!("{}", ir);
        return;
    }

    // --janet: emit Janet and exit
    if flags.contains(&"--janet") {
        let janet = sash::emit_janet::emit_janet(&program);
        print!("{}", janet);
        return;
    }

    // --ssa: print SSA and exit
    if flags.contains(&"--ssa") {
        let ssa = sash::transform::ir_to_ssa(&program);
        let ssa_text = sash::ssa_printer::print_ssa(&ssa);
        print!("{}", ssa_text);
        return;
    }

    // Default: compile to native binary
    let output = if let Some(pos) = flags.iter().position(|&f| f == "-o") {
        args[pos + 3].clone()  // skip binary name + input
    } else {
        input
            .strip_suffix(".sh")
            .or_else(|| input.strip_suffix(".bash"))
            .unwrap_or(&format!("{}.out", input))
            .to_string()
    };

    let obj_path = format!("{}.o", output);
    let obj_bytes = sash::codegen::compile_to_object(&source);
    std::fs::write(&obj_path, &obj_bytes).expect("write .o");

    // Find sash-rt
    let rt_path = format!(
        "{}/../sash-rt/target/release/libsash_rt.a",
        env!("CARGO_MANIFEST_DIR")
    );

    // Link
    let ld = Command::new("ld")
        .args(["-o", &output, &obj_path, &rt_path, "-static", "--gc-sections"])
        .status()
        .expect("ld failed");

    let _ = std::fs::remove_file(&obj_path);

    if !ld.success() {
        eprintln!("sash: linking failed");
        std::process::exit(1);
    }

    eprintln!("sash: {} -> {} (static binary)", input, output);
}
