// Taint model spec tests — DD#24 amendment 2 + DD#46
//
// TDD: tests written against the spec. Some pass today (CST analyzer),
// some require the SSA taint pass or attest runtime enforcement.
// Each test is tagged:
//   [NOW]  — expected to pass with current implementation
//   [SSA]  — requires SSA taint pass
//   [RT]   — requires attest runtime enforcement
//   [DD46] — requires DD#46 (top-level positional rejection/coercion)

use std::process::Command;

mod common;

fn sash_bin() -> String {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("sash");
    path.to_string_lossy().to_string()
}

fn write_tmp(name: &str, content: &str) -> String {
    let path = format!("/tmp/sash_taint_{}", name);
    std::fs::write(&path, content).expect("write tmp");
    path
}

/// Compile, expect failure, return stderr
fn compile_expect_fail(name: &str, script: &str) -> String {
    let src = write_tmp(&format!("{}.sh", name), script);
    let bin = format!("/tmp/sash_taint_{}", name);
    let out = Command::new(sash_bin())
        .args(["-o", &bin, &src])
        .output()
        .expect("failed to run sash");
    assert!(
        !out.status.success(),
        "expected compile failure for {}",
        name
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

/// Compile and run, return stdout
fn compile_and_run(name: &str, script: &str) -> String {
    let src = write_tmp(&format!("{}.sh", name), script);
    let bin = format!("/tmp/sash_taint_{}", name);
    let out = Command::new(sash_bin())
        .args(["-o", &bin, &src])
        .output()
        .expect("failed to run sash");
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

/// Compile and run with env vars, return exit code
fn compile_and_exit_code(name: &str, script: &str, env: &[(&str, &str)]) -> i32 {
    let src = write_tmp(&format!("{}.sh", name), script);
    let bin = format!("/tmp/sash_taint_{}", name);
    let out = Command::new(sash_bin())
        .args(["-o", &bin, &src])
        .output()
        .expect("failed to run sash");
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
    run.status.code().unwrap_or(-1)
}

/// Compile and run with env vars, return stdout
fn compile_and_run_with_env(name: &str, script: &str, env: &[(&str, &str)]) -> String {
    let src = write_tmp(&format!("{}.sh", name), script);
    let bin = format!("/tmp/sash_taint_{}", name);
    let out = Command::new(sash_bin())
        .args(["-o", &bin, &src])
        .output()
        .expect("failed to run sash");
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

// ===================================================================
// Rejection tests — tainted reads without attest → SB4020
// ===================================================================

// ===================================================================
// DD#46 — top-level positional params
// ===================================================================

#[test] // [DD46]
fn taint_toplevel_at_rejected() {
    // Top-level $@ in sash-native mode is a compile error
    let script = r#"#!/bin/bash
for x in "$@"; do echo "$x"; done
"#;
    let stderr = compile_expect_fail("toplevel_at", script);
    assert!(
        stderr.contains("SB4020") || stderr.contains("argv"),
        "top-level $@ should be rejected, got: {}",
        stderr
    );
}

#[test] // [DD46]
fn taint_outofrange_positional() {
    // Function takes 1 arg but reads $2 → compile error
    let script = r#"#!/bin/bash
foo() { echo "$1"; echo "$2"; }
foo "only-one"
"#;
    let stderr = compile_expect_fail("outofrange", script);
    assert!(
        !stderr.is_empty(),
        "out-of-range positional $2 should be rejected, got: {}",
        stderr
    );
}

// ===================================================================
// Clean tests — should compile and run
// ===================================================================

#[test] // [NOW]
fn clean_attest_then_use() {
    let script = r#"#!/bin/bash
x=$(getenv MY_CUSTOM_VAR)
attest x [[ -n $x ]]
echo "$x"
"#;
    // Just check it compiles and runs
    compile_and_run("attest_use", script);
}

#[test] // [NOW]
fn clean_user_func_capture() {
    // $(user_func) is clean — user functions can't introduce taint
    let script = r#"#!/bin/bash
greet() { echo "hello"; }
x=$(greet)
echo "$x"
"#;
    assert_eq!(compile_and_run("func_capture", script), "hello\n");
}

#[test] // [NOW]
fn clean_tainted_inside_attest_expr() {
    // The one legal read: inside attest test expression
    let script = r#"#!/bin/bash
x=$(getenv MY_CUSTOM_VAR)
attest x [[ -n $x ]]
echo ok
"#;
    let output = compile_and_run_with_env("attest_expr", script, &[("MY_CUSTOM_VAR", "something")]);
    assert_eq!(output, "ok\n");
}

// ===================================================================
// Attest runtime enforcement — pattern mismatch must abort
// ===================================================================

#[test] // [RT]
fn attest_regex_mismatch_aborts() {
    // attest with regex that doesn't match → runtime abort (nonzero exit)
    let script = r#"#!/bin/bash
x=$(getenv TAINT_VAL)
attest x [[ $x =~ ^[0-9]+$ ]]
echo "$x"
"#;
    let code = compile_and_exit_code(
        "attest_mismatch_re",
        script,
        &[("TAINT_VAL", "not-a-number")],
    );
    assert_ne!(
        code, 0,
        "attest with failing regex should abort, but exited 0"
    );
}

#[test] // [RT]
fn attest_string_mismatch_aborts() {
    // attest with -n test on empty string → runtime abort
    let script = r#"#!/bin/bash
x=$(getenv TAINT_VAL)
attest x [[ -n $x ]]
echo "$x"
"#;
    let code = compile_and_exit_code("attest_mismatch_n", script, &[("TAINT_VAL", "")]);
    assert_ne!(code, 0, "attest with failing -n should abort, but exited 0");
}

#[test] // [RT]
fn attest_file_mismatch_aborts() {
    // attest with -f test on nonexistent file → runtime abort
    let script = r#"#!/bin/bash
x=$(getenv TAINT_VAL)
attest x [[ -f $x ]]
echo "$x"
"#;
    let code = compile_and_exit_code(
        "attest_mismatch_f",
        script,
        &[("TAINT_VAL", "/nonexistent/path/zzz")],
    );
    assert_ne!(code, 0, "attest with failing -f should abort, but exited 0");
}

#[test] // [RT]
fn attest_dir_mismatch_aborts() {
    // attest with -d test on a file → runtime abort
    let script = r#"#!/bin/bash
x=$(getenv TAINT_VAL)
attest x [[ -d $x ]]
echo "$x"
"#;
    let code = compile_and_exit_code("attest_mismatch_d", script, &[("TAINT_VAL", "/etc/passwd")]);
    assert_ne!(code, 0, "attest with failing -d should abort, but exited 0");
}

#[test] // [RT]
fn attest_eq_mismatch_aborts() {
    // attest with == that doesn't match → runtime abort
    let script = r#"#!/bin/bash
x=$(getenv TAINT_VAL)
attest x [[ $x == "oranges" ]]
echo "$x"
"#;
    let code = compile_and_exit_code("attest_mismatch_eq", script, &[("TAINT_VAL", "apples")]);
    assert_ne!(code, 0, "attest with failing == should abort, but exited 0");
}

#[test] // [RT]
fn attest_match_succeeds() {
    // attest with matching pattern → exit 0, output printed
    let script = r#"#!/bin/bash
x=$(getenv TAINT_VAL)
attest x [[ $x =~ ^[0-9]+$ ]]
echo "$x"
"#;
    let output = compile_and_run_with_env("attest_match_ok", script, &[("TAINT_VAL", "42")]);
    assert_eq!(output, "42\n");
}

// ═══════════════════════════════════════════════════════════════
// AUDIT RUN-2 — TDD tests for findings T1–T12
// All expected to FAIL until remediation is implemented.
// ═══════════════════════════════════════════════════════════════

// --- T1: herestring/heredoc with tainted expansion should NOT be clean ---

// --- T2: tautological attest (test doesn't reference attested var) ---

#[test]
fn t2_attest_tautology_rejected() {
    let script = r#"#!/bin/bash
x=$(getenv MALICIOUS)
attest x [[ 1 == 1 ]]
echo "$x"
"#;
    let stderr = compile_expect_fail("t2_tautology", script);
    assert!(
        stderr.contains("SB4012") || stderr.contains("does not reference"),
        "expected tautology diagnostic, got: {}",
        stderr
    );
}

#[test]
fn t2_attest_wrong_var_rejected() {
    let script = r#"#!/bin/bash
x=$(getenv MALICIOUS)
y="safe"
attest x [[ -n $y ]]
echo "$x"
"#;
    let stderr = compile_expect_fail("t2_wrong_var", script);
    assert!(
        stderr.contains("SB4012") || stderr.contains("does not reference"),
        "expected tautology diagnostic, got: {}",
        stderr
    );
}

// --- T3: attest without valid test expression ---

#[test]
fn t3_attest_no_test_rejected() {
    let script = r#"#!/bin/bash
x=$(getenv SECRET)
attest x nope
echo "$x"
"#;
    let stderr = compile_expect_fail("t3_no_test", script);
    assert!(
        stderr.contains("SB4013") || stderr.contains("SB4010") || stderr.contains("attest"),
        "expected invalid attest diagnostic, got: {}",
        stderr
    );
}

#[test]
fn t3_attest_missing_brackets_rejected() {
    let script = r#"#!/bin/bash
x=$(getenv SECRET)
attest x -n "$x"
echo "$x"
"#;
    let stderr = compile_expect_fail("t3_no_brackets", script);
    assert!(
        stderr.contains("SB4013") || stderr.contains("SB4010") || stderr.contains("attest"),
        "expected invalid attest diagnostic, got: {}",
        stderr
    );
}

// --- T4: arithmetic assignment taint laundering ---

// --- T5: parameter expansion operand taint ---

// --- T6: function global re-taint at call site ---

// --- T9: shift then positional read ---

#[test]
fn t9_shift_then_positional_rejected() {
    let script = r#"#!/bin/bash
f() { shift; echo "$1"; }
f one
"#;
    let stderr = compile_expect_fail("t9_shift_pos", script);
    assert!(
        stderr.contains("reads $") || stderr.contains("out-of-range") || stderr.contains("shift"),
        "expected positional diagnostic, got: {}",
        stderr
    );
}

// --- T10: direct $(getenv) in expression ---

// --- T11: case pattern taint ---

// --- T7/T8/T12: regression locks for validator rules ---

#[test]
fn t7_indirect_expansion_rejected() {
    let script = r#"#!/bin/bash
x="hello"
ref="x"
echo "${!ref}"
"#;
    let stderr = compile_expect_fail("t7_indirect", script);
    assert!(
        stderr.contains("SB"),
        "expected validator diagnostic, got: {}",
        stderr
    );
}

#[test]
fn t8_nameref_rejected() {
    let script = r#"#!/bin/bash
x="hello"
declare -n ref=x
echo "$ref"
"#;
    let stderr = compile_expect_fail("t8_nameref", script);
    assert!(
        stderr.contains("SB"),
        "expected validator diagnostic, got: {}",
        stderr
    );
}

#[test]
fn t12_process_substitution_rejected() {
    let script = r#"#!/bin/bash
cat <(echo "test")
"#;
    let stderr = compile_expect_fail("t12_procsub", script);
    assert!(
        stderr.contains("SB"),
        "expected validator diagnostic, got: {}",
        stderr
    );
}
