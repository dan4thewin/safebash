//! Janet backend end-to-end tests.
//!
//! Each test:
//! 1. Runs a bash script through `sash sash --janet` to produce Janet source
//! 2. Verifies specific patterns in the Janet output
//! 3. Runs the Janet source through `janet` and compares output to bash
//!
//! Tests marked `runnable` verify the output is valid, executable Janet.

use std::process::Command;

fn sash_bin() -> String {
    let p = std::path::PathBuf::from(env!("CARGO_BIN_EXE_sash"));
    p.to_str().unwrap().to_string()
}

fn write_tmp(name: &str, content: &str) -> String {
    let path = format!("/tmp/sash_janet_{}.sh", name);
    std::fs::write(&path, content).unwrap();
    path
}

/// Compile bash → janet source
fn to_janet(name: &str, script: &str) -> String {
    let src = write_tmp(name, script);
    let out = Command::new(sash_bin())
        .args(&["--janet", &src])
        .output()
        .expect("sash failed");
    assert!(
        out.status.success(),
        "sash --janet failed for {}: {}",
        name,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// Run janet source, return stdout
fn run_janet(name: &str, janet_src: &str) -> String {
    let path = format!("/tmp/sash_janet_run_{}.janet", name);
    std::fs::write(&path, janet_src).unwrap();
    let out = Command::new("janet")
        .arg(&path)
        .output()
        .expect("janet not found");
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// Run bash script, return stdout
fn run_bash(name: &str, script: &str) -> String {
    let path = format!("/tmp/sash_janet_bash_{}.sh", name);
    std::fs::write(&path, script).unwrap();
    let out = Command::new("bash")
        .arg(&path)
        .output()
        .expect("bash failed");
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// Full round-trip: bash script → janet → run both, compare output
fn assert_janet_matches_bash(name: &str, script: &str) {
    let janet_src = to_janet(name, script);
    let bash_out = run_bash(name, script);
    let janet_out = run_janet(name, &janet_src);
    assert_eq!(
        bash_out, janet_out,
        "Output mismatch for {}.\nBash: {:?}\nJanet: {:?}\nJanet source:\n{}",
        name, bash_out, janet_out, janet_src
    );
}

// ═══════════════════════════════════════════════════════════════
// BASIC OUTPUT
// ═══════════════════════════════════════════════════════════════

#[test]
fn janet_echo_literal() {
    assert_janet_matches_bash("echo_lit", "#!/bin/bash\necho hello");
}

#[test]
fn janet_echo_variable() {
    assert_janet_matches_bash("echo_var", "#!/bin/bash\nx=world\necho $x");
}

#[test]
fn janet_echo_interpolation() {
    assert_janet_matches_bash(
        "echo_interp",
        "#!/bin/bash\nx=hello\ny=world\necho \"$x $y\"",
    );
}

#[test]
fn janet_echo_empty() {
    assert_janet_matches_bash("echo_empty", "#!/bin/bash\necho");
}

// ═══════════════════════════════════════════════════════════════
// ARITHMETIC
// ═══════════════════════════════════════════════════════════════

#[test]
fn janet_arith_add() {
    assert_janet_matches_bash("arith_add", "#!/bin/bash\na=5\nb=3\necho $(( a + b ))");
}

#[test]
fn janet_arith_mul() {
    assert_janet_matches_bash("arith_mul", "#!/bin/bash\na=5\nb=3\necho $(( a * b ))");
}

#[test]
fn janet_arith_sub() {
    assert_janet_matches_bash("arith_sub", "#!/bin/bash\na=5\nb=3\necho $(( a - b ))");
}

#[test]
fn janet_arith_div() {
    assert_janet_matches_bash("arith_div", "#!/bin/bash\na=10\nb=3\necho $(( a / b ))");
}

#[test]
fn janet_arith_mod() {
    assert_janet_matches_bash("arith_mod", "#!/bin/bash\na=10\nb=3\necho $(( a % b ))");
}

// ═══════════════════════════════════════════════════════════════
// CONTROL FLOW
// ═══════════════════════════════════════════════════════════════

#[test]
#[ignore] // Janet backend doesn't handle (( )) conditions yet
fn janet_if_else() {
    assert_janet_matches_bash(
        "if_else",
        "#!/bin/bash\nint x=5\nif (( x > 3 )); then\n  echo big\nelse\n  echo small\nfi",
    );
}

#[test]
#[ignore] // Janet backend doesn't handle (( )) conditions yet
fn janet_while_loop() {
    assert_janet_matches_bash(
        "while_loop",
        "#!/bin/bash\nint i=0\nwhile (( i < 3 )); do\n  echo $i\n  (( i = i + 1 ))\ndone",
    );
}

#[test]
fn janet_for_loop() {
    assert_janet_matches_bash(
        "for_loop",
        "#!/bin/bash\nfor x in a b c; do\n  echo $x\ndone",
    );
}

#[test]
fn janet_case() {
    assert_janet_matches_bash("case",
        "#!/bin/bash\nv=two\ncase $v in\n  one) echo 1 ;;\n  two) echo 2 ;;\n  *) echo other ;;\nesac");
}

#[test]
fn janet_and_or() {
    assert_janet_matches_bash(
        "and_or",
        "#!/bin/bash\ntrue && echo yes\nfalse || echo fallback",
    );
}

#[test]
fn janet_for_break() {
    assert_janet_matches_bash(
        "for_break",
        "#!/bin/bash\nfor x in 1 2 3 4 5; do\n  [[ $x == \"4\" ]] && break\n  echo $x\ndone",
    );
}

// ═══════════════════════════════════════════════════════════════
// FUNCTIONS
// ═══════════════════════════════════════════════════════════════

#[test]
fn janet_function_basic() {
    assert_janet_matches_bash(
        "func_basic",
        "#!/bin/bash\ngreet() {\n  echo \"hello $1\"\n}\ngreet world",
    );
}

#[test]
fn janet_function_arith() {
    assert_janet_matches_bash(
        "func_arith",
        "#!/bin/bash\nadd() {\n  echo $(( $1 + $2 ))\n}\nadd 3 4\nadd 10 20",
    );
}

// ═══════════════════════════════════════════════════════════════
// STRING OPERATIONS
// ═══════════════════════════════════════════════════════════════

#[test]
fn janet_strlen() {
    assert_janet_matches_bash("strlen", "#!/bin/bash\ns=\"Hello World\"\necho ${#s}");
}

#[test]
fn janet_toupper() {
    assert_janet_matches_bash("toupper", "#!/bin/bash\ns=hello\necho ${s^^}");
}

#[test]
fn janet_tolower() {
    assert_janet_matches_bash("tolower", "#!/bin/bash\ns=HELLO\necho ${s,,}");
}

#[test]
fn janet_default_value() {
    assert_janet_matches_bash("default", "#!/bin/bash\nz=\"\"\necho ${z:-default}");
}

#[test]
fn janet_prefix_strip() {
    assert_janet_matches_bash(
        "prefix_strip",
        "#!/bin/bash\npath=\"/usr/local/bin/sash\"\necho ${path##*/}",
    );
}

#[test]
fn janet_substring() {
    assert_janet_matches_bash("substring", "#!/bin/bash\ns=\"Hello World\"\necho ${s:0:5}");
}

#[test]
fn janet_substring_from() {
    assert_janet_matches_bash("substr_from", "#!/bin/bash\ns=\"Hello World\"\necho ${s:6}");
}

#[test]
fn janet_substring_negative() {
    assert_janet_matches_bash("substr_neg", "#!/bin/bash\ns=\"Hello\"\necho ${s:0:-1}");
}

// ═══════════════════════════════════════════════════════════════
// ARRAYS
// ═══════════════════════════════════════════════════════════════

#[test]
fn janet_array_index() {
    assert_janet_matches_bash(
        "arr_idx",
        "#!/bin/bash\narr=(one two three)\necho ${arr[0]}\necho ${arr[1]}\necho ${arr[2]}",
    );
}

#[test]
fn janet_array_length() {
    assert_janet_matches_bash("arr_len", "#!/bin/bash\narr=(a b c d)\necho ${#arr[@]}");
}

// ═══════════════════════════════════════════════════════════════
// HEREDOC
// ═══════════════════════════════════════════════════════════════

#[test]
fn janet_heredoc() {
    assert_janet_matches_bash(
        "heredoc",
        "#!/bin/bash\n/bin/cat <<EOF\nline one\nline two\nEOF",
    );
}

// ═══════════════════════════════════════════════════════════════
// EXIT STATUS
// ═══════════════════════════════════════════════════════════════

#[test]
fn janet_exit_status() {
    assert_janet_matches_bash("exit_status", "#!/bin/bash\ntrue\necho $?");
}

// ═══════════════════════════════════════════════════════════════
// DECLARATIONS
// ═══════════════════════════════════════════════════════════════

#[test]
fn janet_const() {
    let janet = to_janet("const", "#!/bin/bash\nconst PI=314\necho $PI");
    assert!(
        janet.contains("(def PI"),
        "expected (def for const, got: {}",
        janet
    );
    let out = run_janet("standalone", &janet);
    assert_eq!(out, "314\n");
}

#[test]
fn janet_int_keyword() {
    let janet = to_janet("int_kw", "#!/bin/bash\nint n=42\necho $n");
    let out = run_janet("standalone", &janet);
    assert_eq!(out, "42\n");
}

// ═══════════════════════════════════════════════════════════════
// PATTERN CHECKS (verify Janet source structure)
// ═══════════════════════════════════════════════════════════════

#[test]
fn janet_break_not_external() {
    let janet = to_janet(
        "brk",
        "#!/bin/bash\nfor x in 1 2; do\n  [[ $x == \"2\" ]] && break\n  echo $x\ndone",
    );
    assert!(
        !janet.contains("os/execute [\"break\"]"),
        "break should not be os/execute: {}",
        janet
    );
    assert!(janet.contains("(break)"), "expected (break): {}", janet);
}

#[test]
fn janet_continue_not_external() {
    let janet = to_janet(
        "cont",
        "#!/bin/bash\nfor x in 1 2; do\n  [[ $x == \"1\" ]] && continue\n  echo $x\ndone",
    );
    assert!(
        !janet.contains("os/execute [\"continue\"]"),
        "continue should not be os/execute: {}",
        janet
    );
}

#[test]
fn janet_bless_emitted() {
    // Use unknown command to taint x, then bless it
    let janet = to_janet(
        "bless",
        "#!/bin/bash\nx=$(/usr/bin/curl http://example.com)\nbless x [[ -n $x ]]\necho $x",
    );
    assert!(
        janet.contains("# bless x"),
        "expected bless comment: {}",
        janet
    );
}

#[test]
fn janet_heredoc_not_empty() {
    let janet = to_janet("hd_body", "#!/bin/bash\n/bin/cat <<EOF\nhello world\nEOF");
    assert!(
        janet.contains("hello world"),
        "heredoc body should be in output: {}",
        janet
    );
    assert!(
        !janet.contains("os/execute [\"cat\"]"),
        "cat heredoc should be inlined: {}",
        janet
    );
}

#[test]
fn janet_function_has_args() {
    let janet = to_janet("func_args", "#!/bin/bash\nf() { echo $1; }\nf hello");
    assert!(
        janet.contains("[& _args]"),
        "function with $1 should have varargs: {}",
        janet
    );
}

#[test]
fn janet_modulo_syntax() {
    let janet = to_janet("mod_syn", "#!/bin/bash\na=10\nb=3\necho $(( a % b ))");
    assert!(janet.contains("mod"), "expected (mod ...): {}", janet);
    assert!(
        !janet.contains("(scan-number a % b)"),
        "broken modulo syntax: {}",
        janet
    );
}

// ═══════════════════════════════════════════════════════════════
// KNOWN LIMITATIONS (documented, expected to differ)
// ═══════════════════════════════════════════════════════════════

#[test]
#[ignore] // Janet has no continue — break used as approximation
fn janet_continue_semantics() {
    assert_janet_matches_bash(
        "continue_sem",
        "#!/bin/bash\nfor x in 1 2 3 4 5; do\n  [[ $x == \"3\" ]] && continue\n  echo $x\ndone",
    );
}
