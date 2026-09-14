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

// ===================================================================
// Rejection tests — tainted reads without attest → SB4020
// ===================================================================

// ===================================================================
// DD#46 — top-level positional params
// ===================================================================

// ===================================================================
// Clean tests — should compile and run
// ===================================================================

// ===================================================================
// Attest runtime enforcement — pattern mismatch must abort
// ===================================================================

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
