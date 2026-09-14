// End-to-end tests for sash compiler
//
// Tests compile real scripts, check outputs, diagnostics, coercions,
// and annotations. Each test uses the sash binary built by cargo.

use std::process::Command;

mod common;

fn sash_bin() -> String {
    let mut path = std::env::current_exe().unwrap();
    path.pop(); // deps/
    path.pop(); // debug/ or release/
    path.push("sash");
    path.to_string_lossy().to_string()
}

fn sash_cmd(args: &[&str]) -> std::process::Output {
    Command::new(sash_bin())
        .args(args)
        .output()
        .expect("failed to run sash")
}

fn write_tmp(name: &str, content: &str) -> String {
    let path = format!("/tmp/sash_e2e_{}", name);
    std::fs::write(&path, content).expect("write tmp");
    path
}

fn bash_output(script: &str) -> String {
    let out = Command::new("bash")
        .arg("-c")
        .arg(script)
        .output()
        .expect("bash failed");
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// Compile a script, run it, return stdout
fn compile_and_run(name: &str, script: &str) -> String {
    let src = write_tmp(&format!("{}.sh", name), script);
    let bin = format!("/tmp/sash_e2e_{}", name);
    let out = sash_cmd(&["-o", &bin, &src]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "compile failed for {}: {}",
        name,
        stderr
    );
    let run = Command::new(&bin).output().expect("run failed");
    String::from_utf8_lossy(&run.stdout).to_string()
}

/// Compile a script and run the binary with environment variables set.
fn compile_and_run_env(name: &str, script: &str, env: &[(&str, &str)]) -> String {
    let src = write_tmp(&format!("{}.sh", name), script);
    let bin = format!("/tmp/sash_e2e_{}", name);
    let out = sash_cmd(&["-o", &bin, &src]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "compile failed for {}: {}",
        name,
        stderr
    );
    let mut cmd = Command::new(&bin);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let run = cmd.output().expect("run failed");
    String::from_utf8_lossy(&run.stdout).to_string()
}

/// Compile a script, expect failure, return stderr
fn compile_expect_fail(name: &str, script: &str) -> String {
    let src = write_tmp(&format!("{}.sh", name), script);
    let bin = format!("/tmp/sash_e2e_{}_fail", name);
    let out = sash_cmd(&["-o", &bin, &src]);
    assert!(
        !out.status.success(),
        "expected compile failure for {}",
        name
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

/// Compile, then extract .sash_source from binary
fn extract_source(name: &str, script: &str) -> String {
    let src = write_tmp(&format!("{}.sh", name), script);
    let bin = format!("/tmp/sash_e2e_{}", name);
    let out = sash_cmd(&["-o", &bin, &src]);
    assert!(out.status.success(), "compile failed for {}", name);
    let ext = sash_cmd(&["--extract", &bin]);
    String::from_utf8_lossy(&ext.stdout).to_string()
}

// ═══════════════════════════════════════════════════════════════
// POSITIVE TESTS — compile + run, compare output to bash
// ═══════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════
// NEGATIVE TESTS — expected diagnostic codes
// ═══════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════
// EXTRACT TESTS — .sash_source ELF section
// ═══════════════════════════════════════════════════════════════

#[test]
fn extract_recovers_source() {
    let script = "#!/bin/bash\necho hello";
    let extracted = extract_source("extract", script);
    assert!(
        extracted.contains("echo hello"),
        "expected source in .sash_source, got:\n{}",
        extracted
    );
}

#[test]
fn extract_coerced_source() {
    let script = "#!/bin/bash\nint x=5\necho $x";
    let extracted = extract_source("extract_coerce", script);
    // The .sash_source should contain the source
    assert!(
        extracted.contains("int x=5"),
        "expected source with int, got:\n{}",
        extracted
    );
}

// ═══════════════════════════════════════════════════════════════
// BASH-SAMPLES TESTS — real scripts from ~/bash-samples
// ═══════════════════════════════════════════════════════════════

#[test]
fn sample_kh_taint_error() {
    let script = std::fs::read_to_string(format!(
        "{}/../../bash-samples/kh",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap_or_default();
    if script.is_empty() {
        return;
    }
    let src = write_tmp("kh.sh", &script);
    let out = sash_cmd(&["-o", "/tmp/sash_e2e_kh", &src]);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    common::expect_diag(&stderr, "SB4020");
}

// ═══════════════════════════════════════════════════════════════
// SED EXPRESSION TESTS — ${var:s/re/str/flags} end-to-end
// ═══════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════
// ADDITIONAL NEGATIVE TESTS
// ═══════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════
// FUNCTION ARGUMENT TESTS
// ═══════════════════════════════════════════════════════════════

// DD#46: unbound positional inside a function must NOT leak from
// program argv (or caller's positional slots). Reading $K where K > $#
// returns empty per bash semantics.
#[test]
fn pos_func_unbound_positional_is_empty() {
    // DD#46: reading $2 in a function called with 1 arg is now a compile error
    let script = r#"#!/bin/bash
foo() { echo "got: $2"; }
foo onearg
"#;
    let src = write_tmp("func_unbound_pos.sh", script);
    let bin = "/tmp/sash_e2e_func_unbound_pos".to_string();
    let out = sash_cmd(&["-o", &bin, &src]);
    assert!(
        !out.status.success(),
        "expected compile failure for out-of-range $2"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("reads $2") || stderr.contains("out-of-range"),
        "expected out-of-range positional diagnostic, got: {}",
        stderr
    );
}

// ═══════════════════════════════════════════════════════════════
// NESTED CONTROL FLOW TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn pos_if_in_while() {
    let script = r#"#!/bin/bash
int i=0
while (( i < 6 )); do
    if [[ $i == "0" ]] || [[ $i == "2" ]] || [[ $i == "4" ]]; then
        echo "even $i"
    fi
    (( i = i + 1 ))
done
"#;
    assert_eq!(
        compile_and_run("if_while", script),
        "even 0\neven 2\neven 4\n"
    );
}

// ═══════════════════════════════════════════════════════════════
// HEREDOC TESTS
// ═══════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════
// ARRAY TESTS
// ═══════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════
// ATTEST / TAINT FLOW TESTS
// ═══════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════
// COMMAND SUBSTITUTION TESTS
// ═══════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════
// ADDITIONAL COERCION AND EDGE CASE TESTS
// ═══════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════
// HEREDOC COMMAND SUBSTITUTION REJECTION (SB5002)
// ═══════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════
// HERESTRING TESTS
// ═══════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════
// ATTEST EDGE CASES
// ═══════════════════════════════════════════════════════════════

#[test]
fn pos_attest_regex_validation() {
    // Explicit attest required — no auto-attest in compiler
    let script = r#"#!/bin/bash
n=$(/bin/cat /etc/hostname)
attest n [[ $n =~ ^[a-zA-Z0-9]+$ ]]
echo $n
"#;
    let output = compile_and_run("attest_regex", script);
    assert!(!output.is_empty(), "expected hostname output");
}

// ═══════════════════════════════════════════════════════════════
// AUTO-ATTEST (known_externals)
// ═══════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════
// BRUSH RECOVERABLE CASES — verify diagnostic then native rewrite
// Each test: (1) original fails with correct code, (2) native compiles,
// (3) native output matches bash.
// ═══════════════════════════════════════════════════════════════

#[test]
fn brush_home_clean() {
    // $HOME requires getenv — no implicit env access (DD#53)
    let script = "#!/bin/bash\nx=$(getenv HOME)\nattest x [[ -n $x ]]\necho $x";
    let sash_out = compile_and_run("brush_home_clean", script);
    let bash_out = bash_output("echo $HOME");
    assert_eq!(sash_out, bash_out);
}

#[test]
fn brush_undefined_var() {
    // DD#53: Undefined vars are empty strings (not env lookups, not tainted).
    // They compile and produce empty output.
    let script = "#!/bin/bash\necho ${undefined}";
    let out = compile_and_run_env("brush_undefined", script, &[]);
    assert_eq!(out, "\n", "undefined var should expand to empty string");
}

// ═══════════════════════════════════════════════════════════════
// EXTGLOB REJECTION (SB5005 — DD#20/DD#39)
// ═══════════════════════════════════════════════════════════════

#[test]
fn neg_source_dot_rejected() {
    let script = "#!/bin/bash\n. ./file.sh";
    let stderr = compile_expect_fail("source_dot", script);
    // SB4001 (dynamic execution) — SB4002 had only one emitter, the parser's
    // removed-builtin table, and retired with it.
    common::expect_diag(&stderr, "SB4001");
}

// ── DD#55: suppress prefix (- cmd) ──────────────────────────

// ═══════════════════════════════════════════════════════════════
// --manifest — the external-program dependency list.
//
// Collected at the single command-resolution funnel, so it must list
// every external the program execs (including those inside `$( … )`)
// and must never leak a shell builtin or a compiler-internal sigil
// (e.g. `__let_exit` from `(( … ))`).
// ═══════════════════════════════════════════════════════════════

/// Run `sash <script> --manifest`, returning the emitted paths.
fn manifest(name: &str, script: &str) -> Vec<String> {
    let src = write_tmp(&format!("manifest_{}.sh", name), script);
    let out = sash_cmd(&[&src, "--manifest"]);
    assert!(
        out.status.success(),
        "manifest compile failed for {}: {}",
        name,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.to_string())
        .collect()
}

/// Every stage of a pipeline is an exec and must be listed.
#[test]
fn manifest_includes_pipeline_stages() {
    let deps = manifest("pipe", "date | cat\n");
    assert!(
        deps.iter().any(|d| d.ends_with("/date")) && deps.iter().any(|d| d.ends_with("/cat")),
        "expected both pipeline stages, got: {:?}",
        deps
    );
}

/// A user-defined function is inlined, not exec'd — its name must not
/// appear; the externals inside its body must.
#[test]
fn manifest_excludes_user_functions_lists_body() {
    let deps = manifest("userfn", "fn g() { date +%s; }\ng\n");
    assert!(
        deps.iter().any(|d| d.ends_with("/date")),
        "expected the function body's external, got: {:?}",
        deps
    );
    assert!(
        !deps.iter().any(|d| d == "g" || d.ends_with("/g")),
        "manifest leaked the user function name: {:?}",
        deps
    );
}

/// Shell builtins run in-process (no exec) and must not be listed.
#[test]
fn manifest_excludes_builtins() {
    let deps = manifest("builtins", "echo hi\n:\ntrue\n");
    assert!(
        deps.is_empty(),
        "expected an empty manifest for builtins only, got: {:?}",
        deps
    );
}
