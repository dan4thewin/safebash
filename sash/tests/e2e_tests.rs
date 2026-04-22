// End-to-end tests for sash compiler
//
// Tests compile real scripts, check outputs, diagnostics, coercions,
// and annotations. Each test uses the sash binary built by cargo.

use std::process::Command;

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
    let out = sash_cmd(&["--nolint", "-o", &bin, &src]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "compile failed for {}: {}", name, stderr);
    let run = Command::new(&bin).output().expect("run failed");
    String::from_utf8_lossy(&run.stdout).to_string()
}

/// Compile a script, expect failure, return stderr
fn compile_expect_fail(name: &str, script: &str) -> String {
    let src = write_tmp(&format!("{}.sh", name), script);
    let bin = format!("/tmp/sash_e2e_{}_fail", name);
    let out = sash_cmd(&["-o", &bin, &src]);
    assert!(!out.status.success(), "expected compile failure for {}", name);
    String::from_utf8_lossy(&out.stderr).to_string()
}

/// Run --emit-source on a script, return stdout
fn emit_source(name: &str, script: &str) -> String {
    let src = write_tmp(&format!("{}.sh", name), script);
    let out = sash_cmd(&["--nolint", "--emit-source", &src]);
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// Run --emit-source --annotate on a script, return stdout
fn emit_annotated(name: &str, script: &str) -> String {
    let src = write_tmp(&format!("{}.sh", name), script);
    let out = sash_cmd(&["--nolint", "--emit-source", "--annotate", &src]);
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// Compile, then extract .sash_source from binary
fn extract_source(name: &str, script: &str) -> String {
    let src = write_tmp(&format!("{}.sh", name), script);
    let bin = format!("/tmp/sash_e2e_{}", name);
    let out = sash_cmd(&["--nolint", "-o", &bin, &src]);
    assert!(out.status.success(), "compile failed for {}", name);
    let ext = sash_cmd(&["--extract", &bin]);
    String::from_utf8_lossy(&ext.stdout).to_string()
}

// ═══════════════════════════════════════════════════════════════
// POSITIVE TESTS — compile + run, compare output to bash
// ═══════════════════════════════════════════════════════════════

#[test]
fn pos_echo_literal() {
    let script = "#!/bin/bash\necho hello world";
    assert_eq!(compile_and_run("echo_lit", script), "hello world\n");
}

#[test]
fn pos_variables() {
    let script = "#!/bin/bash\nx=hello\ny=world\necho $x $y";
    assert_eq!(compile_and_run("vars", script), "hello world\n");
}

#[test]
fn pos_arithmetic() {
    let script = "#!/bin/bash\na=3\nb=4\necho $(( a + b ))\necho $(( a * b ))";
    assert_eq!(compile_and_run("arith", script), "7\n12\n");
}

#[test]
fn pos_if_else() {
    let script = r#"#!/bin/bash
x=5
if [[ $x -gt 3 ]]; then
    echo big
else
    echo small
fi
"#;
    assert_eq!(compile_and_run("if_else", script), "big\n");
}

#[test]
fn pos_while_loop() {
    let script = r#"#!/bin/bash
i=0
while [[ $i -lt 5 ]]; do
    echo $i
    (( i = i + 1 ))
done
"#;
    assert_eq!(compile_and_run("while", script), "0\n1\n2\n3\n4\n");
}

#[test]
fn pos_for_loop() {
    let script = "#!/bin/bash\nfor x in a b c; do echo $x; done";
    assert_eq!(compile_and_run("for", script), "a\nb\nc\n");
}

#[test]
fn pos_case() {
    let script = r#"#!/bin/bash
x=hello
case $x in
    hello) echo matched ;;
    *) echo nope ;;
esac
"#;
    assert_eq!(compile_and_run("case", script), "matched\n");
}

#[test]
fn pos_case_wildcard() {
    let script = r#"#!/bin/bash
x=unknown
case $x in
    hello) echo matched ;;
    *) echo default ;;
esac
"#;
    assert_eq!(compile_and_run("case_wc", script), "default\n");
}

#[test]
fn pos_case_empty() {
    let script = "#!/bin/bash\ncase x in esac\necho after";
    assert_eq!(compile_and_run("case_empty", script), "after\n");
}

#[test]
fn pos_string_length() {
    let script = "#!/bin/bash\nx=hello\necho ${#x}";
    assert_eq!(compile_and_run("strlen", script), "5\n");
}

#[test]
fn pos_default_value() {
    let script = "#!/bin/bash\nx=\"\"\necho ${x:-default}\nx=set\necho ${x:-default}";
    assert_eq!(compile_and_run("default", script), "default\nset\n");
}

#[test]
fn pos_strip_prefix() {
    let script = "#!/bin/bash\npath=/usr/local/bin/sash\necho ${path##*/}";
    assert_eq!(compile_and_run("strip_pre", script), "sash\n");
}

#[test]
fn pos_strip_suffix() {
    let script = "#!/bin/bash\nfile=script.sh\necho ${file%.sh}";
    assert_eq!(compile_and_run("strip_suf", script), "script\n");
}

#[test]
fn pos_substring() {
    let script = "#!/bin/bash\nx=hello\necho ${x:1:3}";
    assert_eq!(compile_and_run("substr", script), "ell\n");
}

#[test]
fn pos_negative_substring() {
    let script = "#!/bin/bash\nx=hello\necho ${x:0:-1}";
    assert_eq!(compile_and_run("neg_sub", script), "hell\n");
}

#[test]
fn pos_regex_replace_first() {
    let script = "#!/bin/bash\nvar=\"hello world\"\necho ${var/hello/goodbye}";
    assert_eq!(compile_and_run("rep_first", script), "goodbye world\n");
}

#[test]
fn pos_regex_replace_global() {
    let script = "#!/bin/bash\nvar=\"aoboc\"\necho ${var//o/O}";
    assert_eq!(compile_and_run("rep_global", script), "aObOc\n");
}

#[test]
fn pos_sed_expression() {
    let script = "#!/bin/bash\nvar=\"hello world\"\necho ${var:s/hello/goodbye/}";
    assert_eq!(compile_and_run("sed_expr", script), "goodbye world\n");
}

#[test]
fn pos_sed_global() {
    let script = "#!/bin/bash\nvar=\"aoboc\"\necho ${var:s/o/O/g}";
    assert_eq!(compile_and_run("sed_global", script), "aObOc\n");
}

#[test]
fn pos_sed_anchor_prefix() {
    let script = "#!/bin/bash\nvar=\"hello hello\"\necho ${var:s/^hello/FIRST/}";
    assert_eq!(compile_and_run("sed_prefix", script), "FIRST hello\n");
}

#[test]
fn pos_sed_anchor_suffix() {
    let script = "#!/bin/bash\nvar=\"hello hello\"\necho ${var:s/hello$/LAST/}";
    assert_eq!(compile_and_run("sed_suffix", script), "hello LAST\n");
}

#[test]
fn pos_command_substitution() {
    let script = "#!/bin/bash\nx=hello\ny=\"prefix_${x}_suffix\"\necho $y";
    assert_eq!(compile_and_run("cmdsub", script), "prefix_hello_suffix\n");
}

#[test]
fn pos_pipeline() {
    let script = "#!/bin/bash\necho hello world | tr ' ' '\\n'";
    assert_eq!(compile_and_run("pipe", script), "hello\nworld\n");
}

#[test]
fn pos_heredoc() {
    let script = "#!/bin/bash\ncat <<EOF\nhello\nworld\nEOF";
    assert_eq!(compile_and_run("heredoc", script), "hello\nworld\n");
}

#[test]
fn pos_regex_match() {
    let script = r#"#!/bin/bash
if [[ "abc123" =~ [0-9]+ ]]; then echo match; else echo no; fi
"#;
    assert_eq!(compile_and_run("regex_match", script), "match\n");
}

#[test]
fn pos_regex_dollar_anchor() {
    let script = r#"#!/bin/bash
if [[ "a" =~ a$ ]]; then echo match; else echo no; fi
"#;
    assert_eq!(compile_and_run("regex_dollar", script), "match\n");
}

#[test]
fn pos_and_or() {
    let script = "#!/bin/bash\ntrue && echo yes\nfalse || echo fallback";
    assert_eq!(compile_and_run("and_or", script), "yes\nfallback\n");
}

#[test]
fn pos_exit_status() {
    let script = "#!/bin/bash\ntrue\necho $?\nfalse\necho $?";
    assert_eq!(compile_and_run("exit_st", script), "0\n1\n");
}

#[test]
fn pos_declare_int() {
    let script = "#!/bin/bash\ndeclare -i x=5\nx+=3\necho $x";
    assert_eq!(compile_and_run("decl_int", script), "8\n");
}

#[test]
fn pos_int_keyword() {
    let script = "#!/bin/bash\nint x=5\nx+=3\necho $x";
    assert_eq!(compile_and_run("int_kw", script), "8\n");
}

#[test]
fn pos_const_keyword() {
    let script = "#!/bin/bash\nconst msg=hello\necho $msg";
    assert_eq!(compile_and_run("const_kw", script), "hello\n");
}

#[test]
fn pos_function_def_call() {
    let script = r#"#!/bin/bash
greet() {
    echo "hello from func"
}
greet
"#;
    assert_eq!(compile_and_run("func", script), "hello from func\n");
}

#[test]
fn pos_function_return() {
    let script = r#"#!/bin/bash
greet() {
    echo "hi there"
}
greet
"#;
    assert_eq!(compile_and_run("func_ret", script), "hi there\n");
}

#[test]
fn pos_int_capture() {
    let script = "#!/bin/bash\nint n=42\necho $n";
    assert_eq!(compile_and_run("int_cap", script), "42\n");
}

#[test]
fn pos_glob_match_case() {
    let script = r#"#!/bin/bash
file="test.sh"
case $file in
    *.sh) echo shell ;;
    *.py) echo python ;;
    *) echo other ;;
esac
"#;
    assert_eq!(compile_and_run("glob_case", script), "shell\n");
}

#[test]
fn pos_test_string_ops() {
    let script = r#"#!/bin/bash
[[ -n "hello" ]] && echo nonempty
[[ -z "" ]] && echo empty
"#;
    assert_eq!(compile_and_run("test_str", script), "nonempty\nempty\n");
}

#[test]
fn pos_test_numeric_ops() {
    let script = r#"#!/bin/bash
[[ 5 -gt 3 ]] && echo gt
[[ 3 -eq 3 ]] && echo eq
[[ 2 -lt 3 ]] && echo lt
"#;
    assert_eq!(compile_and_run("test_num", script), "gt\neq\nlt\n");
}

#[test]
fn pos_concat() {
    let script = "#!/bin/bash\na=hello\nb=world\nc=\"${a}_${b}\"\necho $c";
    assert_eq!(compile_and_run("concat", script), "hello_world\n");
}

#[test]
fn pos_multiline_string() {
    let script = "#!/bin/bash\necho \"line1\nline2\"";
    assert_eq!(compile_and_run("multiline", script), "line1\nline2\n");
}

#[test]
fn pos_uppercase() {
    let script = "#!/bin/bash\nx=hello\necho ${x^^}";
    assert_eq!(compile_and_run("upper", script), "HELLO\n");
}

#[test]
fn pos_lowercase() {
    let script = "#!/bin/bash\nx=HELLO\necho ${x,,}";
    assert_eq!(compile_and_run("lower", script), "hello\n");
}

// ═══════════════════════════════════════════════════════════════
// NEGATIVE TESTS — expected diagnostic codes
// ═══════════════════════════════════════════════════════════════

#[test]
fn neg_export_rejected() {
    let script = "#!/bin/bash\nexport FOO=bar\necho $FOO";
    let stderr = compile_expect_fail("export", script);
    assert!(stderr.contains("SB5001"), "expected SB5001 export rejection, got: {}", stderr);
}

#[test]
fn neg_tainted_env_var() {
    let script = "#!/bin/bash\necho $HOME";
    let stderr = compile_expect_fail("taint_env", script);
    assert!(stderr.contains("SB4020"), "expected SB4020 taint error, got: {}", stderr);
}

#[test]
fn neg_tainted_argv() {
    let script = "#!/bin/bash\necho $1";
    let stderr = compile_expect_fail("taint_argv", script);
    assert!(stderr.contains("SB4020"), "expected SB4020 taint error, got: {}", stderr);
}

#[test]
fn neg_eval_rejected() {
    let script = "#!/bin/bash\neval echo hello";
    let stderr = compile_expect_fail("eval", script);
    assert!(stderr.contains("SB4001"), "expected SB4001 eval rejection, got: {}", stderr);
}

// ═══════════════════════════════════════════════════════════════
// COERCION TESTS — --emit-source output
// ═══════════════════════════════════════════════════════════════

#[test]
fn coerce_declare_i_to_int() {
    let script = "#!/bin/bash\ndeclare -i x=5\necho $x";
    let out = emit_source("coerce_int", script);
    assert!(out.contains("int x=5"), "expected 'int x=5', got:\n{}", out);
    assert!(!out.contains("declare"), "should not contain 'declare'");
}

#[test]
fn coerce_declare_a_to_array() {
    let script = "#!/bin/bash\ndeclare -a arr=(1 2 3)";
    let out = emit_source("coerce_arr", script);
    assert!(out.contains("array arr=(1 2 3)"), "expected 'array', got:\n{}", out);
}

#[test]
fn coerce_declare_A_to_map() {
    let script = "#!/bin/bash\ndeclare -A m=([a]=1)";
    let out = emit_source("coerce_map", script);
    assert!(out.contains("map m=([a]=1)"), "expected 'map', got:\n{}", out);
}

#[test]
fn coerce_readonly_to_const() {
    let script = "#!/bin/bash\nreadonly msg=hello";
    let out = emit_source("coerce_const", script);
    assert!(out.contains("const msg=hello"), "expected 'const', got:\n{}", out);
}

#[test]
fn coerce_declare_ri_to_const_int() {
    let script = "#!/bin/bash\ndeclare -ri x=5";
    let out = emit_source("coerce_ri", script);
    assert!(out.contains("const int x=5"), "expected 'const int', got:\n{}", out);
}

#[test]
fn coerce_let_to_arith() {
    let script = "#!/bin/bash\nlet \"x=3+4\"";
    let out = emit_source("coerce_let", script);
    assert!(out.contains("(( x=3+4 ))"), "expected '(( x=3+4 ))', got:\n{}", out);
}

#[test]
fn coerce_local_i_to_local_int() {
    let script = "#!/bin/bash\nf() { local -i x=5; echo $x; }\nf";
    let out = emit_source("coerce_local_int", script);
    assert!(out.contains("local int x=5"), "expected 'local int', got:\n{}", out);
}

#[test]
fn coerce_typeset_stripped() {
    let script = "#!/bin/bash\ntypeset x=hello";
    let out = emit_source("coerce_typeset", script);
    assert!(!out.contains("typeset"), "should not contain 'typeset'");
}

#[test]
fn coerce_native_unchanged() {
    let script = "#!/bin/bash\nint x=5\nconst msg=hello\narray arr=(1 2 3)";
    let out = emit_source("coerce_native", script);
    assert!(out.contains("int x=5"), "int should pass through");
    assert!(out.contains("const msg=hello"), "const should pass through");
    assert!(out.contains("array arr=(1 2 3)"), "array should pass through");
}

// ═══════════════════════════════════════════════════════════════
// ANNOTATION TESTS — --emit-source --annotate shows # was: comments
// ═══════════════════════════════════════════════════════════════

#[test]
fn annotate_declare_i() {
    let script = "#!/bin/bash\ndeclare -i x=5";
    let out = emit_annotated("ann_int", script);
    assert!(out.contains("# was: declare -i x=5"), "expected annotation, got:\n{}", out);
}

#[test]
fn annotate_readonly() {
    let script = "#!/bin/bash\nreadonly msg=hello";
    let out = emit_annotated("ann_const", script);
    assert!(out.contains("# was: readonly msg=hello"), "expected annotation, got:\n{}", out);
}

#[test]
fn annotate_let() {
    let script = "#!/bin/bash\nlet \"x=1\"";
    let out = emit_annotated("ann_let", script);
    assert!(out.contains("# was: let \"x=1\""), "expected annotation, got:\n{}", out);
}

#[test]
fn annotate_native_no_annotation() {
    let script = "#!/bin/bash\nint x=5\nconst msg=hello";
    let out = emit_annotated("ann_native", script);
    assert!(!out.contains("# was:"), "native syntax should have no annotations, got:\n{}", out);
}

// ═══════════════════════════════════════════════════════════════
// EXTRACT TESTS — .sash_source ELF section
// ═══════════════════════════════════════════════════════════════

#[test]
fn extract_recovers_source() {
    let script = "#!/bin/bash\necho hello";
    let extracted = extract_source("extract", script);
    assert!(extracted.contains("echo hello"), "expected source in .sash_source, got:\n{}", extracted);
}

#[test]
fn extract_coerced_source() {
    let script = "#!/bin/bash\ndeclare -i x=5\necho $x";
    let extracted = extract_source("extract_coerce", script);
    // The .sash_source should contain the coerced form
    assert!(extracted.contains("int x=5"), "expected coerced source, got:\n{}", extracted);
}

// ═══════════════════════════════════════════════════════════════
// BASH-SAMPLES TESTS — real scripts from ~/bash-samples
// ═══════════════════════════════════════════════════════════════

#[test]
fn sample_gg_rejects_export() {
    let script = std::fs::read_to_string(format!("{}/../../bash-samples/gg", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_default();
    if script.is_empty() { return; } // skip if not available
    let src = write_tmp("gg.sh", &script);
    let out = sash_cmd(&["-o", "/tmp/sash_e2e_gg", &src]);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(stderr.contains("SB5001") || stderr.contains("SB4020"),
        "gg should fail with export or taint error, got:\n{}", stderr);
}

#[test]
fn sample_kh_taint_error() {
    let script = std::fs::read_to_string(format!("{}/../../bash-samples/kh", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_default();
    if script.is_empty() { return; }
    let src = write_tmp("kh.sh", &script);
    let out = sash_cmd(&["-o", "/tmp/sash_e2e_kh", &src]);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(stderr.contains("SB4020"),
        "kh should fail with taint error, got:\n{}", stderr);
}

// ═══════════════════════════════════════════════════════════════
// SED EXPRESSION TESTS — ${var:s/re/str/flags} end-to-end
// ═══════════════════════════════════════════════════════════════

#[test]
fn pos_sed_regex_metachar() {
    let script = "#!/bin/bash\nvar=\"foo.bar.baz\"\necho ${var:s/\\./−/g}";
    assert_eq!(compile_and_run("sed_meta", script), "foo−bar−baz\n");
}

#[test]
fn pos_sed_empty_replacement() {
    let script = "#!/bin/bash\nvar=\"hello world\"\necho ${var:s/hello //}";
    assert_eq!(compile_and_run("sed_empty", script), "world\n");
}

#[test]
fn pos_sed_in_assignment() {
    let script = "#!/bin/bash\nvar=\"hello world\"\nx=${var:s/hello/goodbye/}\necho $x";
    assert_eq!(compile_and_run("sed_assign", script), "goodbye world\n");
}

// ═══════════════════════════════════════════════════════════════
// ADDITIONAL NEGATIVE TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn neg_tainted_all_args() {
    let script = "#!/bin/bash\necho \"$@\"";
    let stderr = compile_expect_fail("taint_at", script);
    assert!(stderr.contains("SB4020"), "expected SB4020 for $@, got: {}", stderr);
}

#[test]
fn neg_export_declare_x() {
    let script = "#!/bin/bash\ndeclare -x FOO=bar\necho $FOO";
    let stderr = compile_expect_fail("decl_x", script);
    assert!(stderr.contains("SB5001"), "expected SB5001 for declare -x, got: {}", stderr);
}
