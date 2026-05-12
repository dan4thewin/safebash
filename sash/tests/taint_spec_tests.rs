// Taint model spec tests — DD#24 amendment 2 + DD#46
//
// TDD: tests written against the spec. Some pass today (CST analyzer),
// some require the SSA taint pass or bless runtime enforcement.
// Each test is tagged:
//   [NOW]  — expected to pass with current implementation
//   [SSA]  — requires SSA taint pass
//   [RT]   — requires bless runtime enforcement
//   [DD46] — requires DD#46 (top-level positional rejection/coercion)

use std::process::Command;

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
    assert!(!out.status.success(), "expected compile failure for {}", name);
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
    assert!(out.status.success(), "compile failed for {}: {}", name, stderr);
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
    assert!(out.status.success(), "compile failed for {}: {}", name, stderr);
    let mut cmd = Command::new(&bin);
    for (k, v) in env { cmd.env(k, v); }
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
    assert!(out.status.success(), "compile failed for {}: {}", name, stderr);
    let mut cmd = Command::new(&bin);
    for (k, v) in env { cmd.env(k, v); }
    let run = cmd.output().expect("run failed");
    String::from_utf8_lossy(&run.stdout).to_string()
}

// ===================================================================
// Rejection tests — tainted reads without bless → SB4020
// ===================================================================

#[test] // [NOW]
fn taint_copy_is_read() {
    // b=$a where a is tainted — copying is reading
    let script = r#"#!/bin/bash
x=$(getenv UNTRUSTED)
b=$x
echo "$b"
"#;
    let stderr = compile_expect_fail("copy_read", script);
    assert!(stderr.contains("SB4020"), "copying tainted var should be SB4020, got: {}", stderr);
}

#[test] // [NOW]
fn taint_cmd_arg() {
    // Using tainted var as command argument
    let script = r#"#!/bin/bash
x=$(getenv UNTRUSTED)
echo "$x"
"#;
    let stderr = compile_expect_fail("cmd_arg", script);
    assert!(stderr.contains("SB4020"), "tainted var as cmd arg should be SB4020, got: {}", stderr);
}

#[test] // [SSA]
fn taint_as_array_index() {
    // Tainted value used as array index
    let script = r#"#!/bin/bash
array items=(a b c)
x=$(getenv IDX)
echo "${items[$x]}"
"#;
    let stderr = compile_expect_fail("arr_index", script);
    assert!(stderr.contains("SB4020"), "tainted index should be SB4020, got: {}", stderr);
}

#[test] // [SSA]
fn taint_in_arithmetic() {
    // Tainted value in arithmetic expression
    let script = r#"#!/bin/bash
x=$(getenv NUM)
(( x + 1 ))
"#;
    let stderr = compile_expect_fail("arith", script);
    assert!(stderr.contains("SB4020"), "tainted var in arithmetic should be SB4020, got: {}", stderr);
}

#[test] // [SSA]
fn taint_arith_expansion() {
    // Tainted value in $(( )) expansion
    let script = r#"#!/bin/bash
x=$(getenv NUM)
echo $(( x * 2 ))
"#;
    let stderr = compile_expect_fail("arith_exp", script);
    assert!(stderr.contains("SB4020"), "tainted var in $(()) should be SB4020, got: {}", stderr);
}

#[test] // [SSA]
fn taint_func_argument() {
    // Passing tainted var as function argument is a read
    let script = r#"#!/bin/bash
greet() { echo "hello $1"; }
x=$(getenv NAME)
greet "$x"
"#;
    let stderr = compile_expect_fail("func_arg", script);
    assert!(stderr.contains("SB4020"), "passing tainted var to function should be SB4020, got: {}", stderr);
}

#[test] // [SSA]
fn taint_default_expansion() {
    // ${x:-default} is a read of x
    let script = r#"#!/bin/bash
x=$(getenv VAL)
echo "${x:-fallback}"
"#;
    let stderr = compile_expect_fail("default_exp", script);
    assert!(stderr.contains("SB4020"), "default expansion of tainted var should be SB4020, got: {}", stderr);
}

#[test] // [SSA]
fn taint_length() {
    // ${#x} is a read of x
    let script = r#"#!/bin/bash
x=$(getenv VAL)
echo "${#x}"
"#;
    let stderr = compile_expect_fail("length", script);
    assert!(stderr.contains("SB4020"), "length of tainted var should be SB4020, got: {}", stderr);
}

#[test] // [SSA]
fn taint_prefix_strip() {
    // ${x#pat} is a read
    let script = r#"#!/bin/bash
x=$(getenv PATH_VAL)
echo "${x#/usr}"
"#;
    let stderr = compile_expect_fail("prefix_strip", script);
    assert!(stderr.contains("SB4020"), "prefix strip of tainted var should be SB4020, got: {}", stderr);
}

#[test] // [SSA]
fn taint_suffix_strip() {
    // ${x%%.*} is a read
    let script = r#"#!/bin/bash
x=$(getenv FILENAME)
echo "${x%%.*}"
"#;
    let stderr = compile_expect_fail("suffix_strip", script);
    assert!(stderr.contains("SB4020"), "suffix strip of tainted var should be SB4020, got: {}", stderr);
}

#[test] // [SSA]
fn taint_mapfile_element() {
    // mapfile produces tainted elements; reading one is rejected
    let script = r#"#!/bin/bash
mapfile -t lines < /dev/stdin
echo "${lines[0]}"
"#;
    let stderr = compile_expect_fail("mapfile", script);
    assert!(stderr.contains("SB4020"), "mapfile element read should be SB4020, got: {}", stderr);
}

#[test] // [SSA]
fn taint_retaint_after_bless() {
    // Re-assignment from tainted source after bless re-taints
    let script = r#"#!/bin/bash
x=$(getenv A)
bless x [[ -n $x ]]
x=$(getenv B)
echo "$x"
"#;
    let stderr = compile_expect_fail("retaint", script);
    assert!(stderr.contains("SB4020"), "re-tainted var should be SB4020, got: {}", stderr);
}

// ===================================================================
// DD#46 — top-level positional params
// ===================================================================

#[test] // [DD46]
fn taint_toplevel_dollar1_rejected() {
    // Top-level $1 in sash-native mode is a compile error
    let script = r#"#!/bin/bash
echo $1
"#;
    let stderr = compile_expect_fail("toplevel_1", script);
    // Could be SB4020 (tainted) or a dedicated "use argv" diagnostic
    assert!(stderr.contains("SB4020") || stderr.contains("argv"),
        "top-level $1 should be rejected, got: {}", stderr);
}

#[test] // [DD46]
fn taint_toplevel_at_rejected() {
    // Top-level $@ in sash-native mode is a compile error
    let script = r#"#!/bin/bash
for x in "$@"; do echo "$x"; done
"#;
    let stderr = compile_expect_fail("toplevel_at", script);
    assert!(stderr.contains("SB4020") || stderr.contains("argv"),
        "top-level $@ should be rejected, got: {}", stderr);
}

#[test] // [DD46]
fn taint_outofrange_positional() {
    // Function takes 1 arg but reads $2 → compile error
    let script = r#"#!/bin/bash
foo() { echo "$1"; echo "$2"; }
foo "only-one"
"#;
    let stderr = compile_expect_fail("outofrange", script);
    assert!(!stderr.is_empty(), "out-of-range positional $2 should be rejected, got: {}", stderr);
}

// ===================================================================
// Clean tests — should compile and run
// ===================================================================

#[test] // [NOW]
fn clean_bless_then_use() {
    let script = r#"#!/bin/bash
x=$(getenv MY_CUSTOM_VAR)
bless x [[ -n $x ]]
echo "$x"
"#;
    // Just check it compiles and runs
    compile_and_run("bless_use", script);
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
fn clean_herestring_literal() {
    // read from herestring of clean data → clean
    let script = r#"#!/bin/bash
read x <<< "safe"
echo "$x"
"#;
    assert_eq!(compile_and_run("herestring", script), "safe\n");
}

#[test] // [NOW]
fn clean_heredoc_no_cmdsub() {
    // read from heredoc with no cmdsub → clean
    let script = r#"#!/bin/bash
/bin/cat <<EOF
hello world
EOF
"#;
    assert_eq!(compile_and_run("heredoc_clean", script), "hello world\n");
}


#[test] // [NOW]
fn clean_tainted_inside_bless_expr() {
    // The one legal read: inside bless test expression
    let script = r#"#!/bin/bash
x=$(getenv MY_CUSTOM_VAR)
bless x [[ -n $x ]]
echo ok
"#;
    let output = compile_and_run_with_env("bless_expr", script,
        &[("MY_CUSTOM_VAR", "something")]);
    assert_eq!(output, "ok\n");
}

// ===================================================================
// Bless runtime enforcement — pattern mismatch must abort
// ===================================================================

#[test] // [RT]
fn bless_regex_mismatch_aborts() {
    // bless with regex that doesn't match → runtime abort (nonzero exit)
    let script = r#"#!/bin/bash
x=$(getenv TAINT_VAL)
bless x [[ $x =~ ^[0-9]+$ ]]
echo "$x"
"#;
    let code = compile_and_exit_code("bless_mismatch_re", script,
        &[("TAINT_VAL", "not-a-number")]);
    assert_ne!(code, 0, "bless with failing regex should abort, but exited 0");
}

#[test] // [RT]
fn bless_string_mismatch_aborts() {
    // bless with -n test on empty string → runtime abort
    let script = r#"#!/bin/bash
x=$(getenv TAINT_VAL)
bless x [[ -n $x ]]
echo "$x"
"#;
    let code = compile_and_exit_code("bless_mismatch_n", script,
        &[("TAINT_VAL", "")]);
    assert_ne!(code, 0, "bless with failing -n should abort, but exited 0");
}

#[test] // [RT]
fn bless_file_mismatch_aborts() {
    // bless with -f test on nonexistent file → runtime abort
    let script = r#"#!/bin/bash
x=$(getenv TAINT_VAL)
bless x [[ -f $x ]]
echo "$x"
"#;
    let code = compile_and_exit_code("bless_mismatch_f", script,
        &[("TAINT_VAL", "/nonexistent/path/zzz")]);
    assert_ne!(code, 0, "bless with failing -f should abort, but exited 0");
}

#[test] // [RT]
fn bless_dir_mismatch_aborts() {
    // bless with -d test on a file → runtime abort
    let script = r#"#!/bin/bash
x=$(getenv TAINT_VAL)
bless x [[ -d $x ]]
echo "$x"
"#;
    let code = compile_and_exit_code("bless_mismatch_d", script,
        &[("TAINT_VAL", "/etc/passwd")]);
    assert_ne!(code, 0, "bless with failing -d should abort, but exited 0");
}

#[test] // [RT]
fn bless_eq_mismatch_aborts() {
    // bless with == that doesn't match → runtime abort
    let script = r#"#!/bin/bash
x=$(getenv TAINT_VAL)
bless x [[ $x == "oranges" ]]
echo "$x"
"#;
    let code = compile_and_exit_code("bless_mismatch_eq", script,
        &[("TAINT_VAL", "apples")]);
    assert_ne!(code, 0, "bless with failing == should abort, but exited 0");
}

#[test] // [RT]
fn bless_match_succeeds() {
    // bless with matching pattern → exit 0, output printed
    let script = r#"#!/bin/bash
x=$(getenv TAINT_VAL)
bless x [[ $x =~ ^[0-9]+$ ]]
echo "$x"
"#;
    let output = compile_and_run_with_env("bless_match_ok", script,
        &[("TAINT_VAL", "42")]);
    assert_eq!(output, "42\n");
}

// ═══════════════════════════════════════════════════════════════
// AUDIT RUN-2 — TDD tests for findings T1–T12
// All expected to FAIL until remediation is implemented.
// ═══════════════════════════════════════════════════════════════

// --- T1: herestring/heredoc with tainted expansion should NOT be clean ---

#[test]
fn t1_herestring_tainted_expansion_rejected() {
    let script = r#"#!/bin/bash
x=$(getenv UNTRUSTED)
read y <<< "$x"
echo "$y"
"#;
    let stderr = compile_expect_fail("t1_hs_taint", script);
    assert!(stderr.contains("SB4020"), "expected SB4020, got: {}", stderr);
}

#[test]
fn t1_heredoc_tainted_expansion_rejected() {
    let script = "#!/bin/bash\nx=$(getenv UNTRUSTED)\nread y <<EOF\n$x\nEOF\necho \"$y\"\n";
    let stderr = compile_expect_fail("t1_heredoc_taint", script);
    assert!(stderr.contains("SB4020"), "expected SB4020, got: {}", stderr);
}

// --- T2: tautological bless (test doesn't reference blessed var) ---

#[test]
fn t2_bless_tautology_rejected() {
    let script = r#"#!/bin/bash
x=$(getenv MALICIOUS)
bless x [[ 1 == 1 ]]
echo "$x"
"#;
    let stderr = compile_expect_fail("t2_tautology", script);
    assert!(stderr.contains("SB4012") || stderr.contains("does not reference"),
        "expected tautology diagnostic, got: {}", stderr);
}

#[test]
fn t2_bless_wrong_var_rejected() {
    let script = r#"#!/bin/bash
x=$(getenv MALICIOUS)
y="safe"
bless x [[ -n $y ]]
echo "$x"
"#;
    let stderr = compile_expect_fail("t2_wrong_var", script);
    assert!(stderr.contains("SB4012") || stderr.contains("does not reference"),
        "expected tautology diagnostic, got: {}", stderr);
}

// --- T3: bless without valid test expression ---

#[test]
fn t3_bless_no_test_rejected() {
    let script = r#"#!/bin/bash
x=$(getenv SECRET)
bless x nope
echo "$x"
"#;
    let stderr = compile_expect_fail("t3_no_test", script);
    assert!(stderr.contains("SB4013") || stderr.contains("SB4010") || stderr.contains("bless"),
        "expected invalid bless diagnostic, got: {}", stderr);
}

#[test]
fn t3_bless_missing_brackets_rejected() {
    let script = r#"#!/bin/bash
x=$(getenv SECRET)
bless x -n "$x"
echo "$x"
"#;
    let stderr = compile_expect_fail("t3_no_brackets", script);
    assert!(stderr.contains("SB4013") || stderr.contains("SB4010") || stderr.contains("bless"),
        "expected invalid bless diagnostic, got: {}", stderr);
}

// --- T4: arithmetic assignment taint laundering ---

#[test]
fn t4_arith_assignment_propagates_taint() {
    let script = r#"#!/bin/bash
x=$(getenv NUM)
y=$((x + 1))
echo "$y"
"#;
    let stderr = compile_expect_fail("t4_arith_launder", script);
    assert!(stderr.contains("SB4020"), "expected SB4020, got: {}", stderr);
}

// --- T5: parameter expansion operand taint ---

#[test]
fn t5_param_default_tainted_rejected() {
    let script = r#"#!/bin/bash
safe=""
x=$(getenv SECRET)
echo "${safe:-$x}"
"#;
    let stderr = compile_expect_fail("t5_default_taint", script);
    assert!(stderr.contains("SB4020"), "expected SB4020, got: {}", stderr);
}

#[test]
fn t5_param_alternate_tainted_rejected() {
    let script = r#"#!/bin/bash
x=$(getenv SECRET)
safe="value"
echo "${safe:+$x}"
"#;
    let stderr = compile_expect_fail("t5_alternate_taint", script);
    assert!(stderr.contains("SB4020"), "expected SB4020, got: {}", stderr);
}

// --- T6: function global re-taint at call site ---

#[test]
fn t6_function_global_retaint_rejected() {
    let script = r#"#!/bin/bash
x="clean"
leak() { echo "$x"; }
x=$(getenv SECRET)
leak
"#;
    let stderr = compile_expect_fail("t6_global_retaint", script);
    assert!(stderr.contains("SB4020"), "expected SB4020, got: {}", stderr);
}

// --- T9: shift then positional read ---

#[test]
fn t9_shift_then_positional_rejected() {
    let script = r#"#!/bin/bash
f() { shift; echo "$1"; }
f one
"#;
    let stderr = compile_expect_fail("t9_shift_pos", script);
    assert!(stderr.contains("reads $") || stderr.contains("out-of-range") || stderr.contains("shift"),
        "expected positional diagnostic, got: {}", stderr);
}

// --- T10: direct $(getenv) in expression ---

#[test]
fn t10_direct_getenv_cmdsub_rejected() {
    let script = r#"#!/bin/bash
echo "$(getenv UNTRUSTED)"
"#;
    let stderr = compile_expect_fail("t10_direct_getenv", script);
    assert!(stderr.contains("SB4020"), "expected SB4020, got: {}", stderr);
}

// --- T11: case pattern taint ---

#[test]
fn t11_case_pattern_tainted_rejected() {
    let script = r#"#!/bin/bash
x=$(getenv PATTERN)
case "test" in
  $x) echo "matched" ;;
  *) echo "no match" ;;
esac
"#;
    let stderr = compile_expect_fail("t11_case_pattern", script);
    assert!(stderr.contains("SB4020"), "expected SB4020, got: {}", stderr);
}

// --- T7/T8/T12: regression locks for validator rules ---

#[test]
fn t7_indirect_expansion_rejected() {
    let script = r#"#!/bin/bash
x="hello"
ref="x"
echo "${!ref}"
"#;
    let stderr = compile_expect_fail("t7_indirect", script);
    assert!(stderr.contains("SB") , "expected validator diagnostic, got: {}", stderr);
}

#[test]
fn t8_nameref_rejected() {
    let script = r#"#!/bin/bash
x="hello"
declare -n ref=x
echo "$ref"
"#;
    let stderr = compile_expect_fail("t8_nameref", script);
    assert!(stderr.contains("SB"), "expected validator diagnostic, got: {}", stderr);
}

#[test]
fn t12_process_substitution_rejected() {
    let script = r#"#!/bin/bash
cat <(echo "test")
"#;
    let stderr = compile_expect_fail("t12_procsub", script);
    assert!(stderr.contains("SB"), "expected validator diagnostic, got: {}", stderr);
}
