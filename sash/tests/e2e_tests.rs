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
int x=5
if (( x > 3 )); then
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
int i=0
while (( i < 5 )); do
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
    let script = "#!/bin/bash\necho hello world | /usr/bin/tr ' ' '\\n'";
    assert_eq!(compile_and_run("pipe", script), "hello\nworld\n");
}

#[test]
fn pos_heredoc() {
    let script = "#!/bin/bash\n/bin/cat <<EOF\nhello\nworld\nEOF";
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
    let script = "#!/bin/bash\nint x=5\n(( x += 3 ))\necho $x";
    assert_eq!(compile_and_run("decl_int", script), "8\n");
}

#[test]
fn pos_int_keyword() {
    let script = "#!/bin/bash\nint x=5\n(( x += 3 ))\necho $x";
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
if (( 5 > 3 )); then echo gt; fi
if (( 3 == 3 )); then echo eq; fi
if (( 2 < 3 )); then echo lt; fi
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
    common::expect_diag(&stderr, "SB5001");
}

#[test]
fn neg_tainted_env_var() {
    let script = "#!/bin/bash\nx=$(getenv CUSTOM_VAR)\necho $x";
    let stderr = compile_expect_fail("taint_env", script);
    common::expect_diag(&stderr, "SB4020");
}

#[test]
fn neg_tainted_argv() {
    let script = "#!/bin/bash\necho $1";
    let stderr = compile_expect_fail("taint_argv", script);
    common::expect_diag(&stderr, "SB4020");
}

#[test]
fn neg_tainted_env_inside_function_body() {
    // DD#24: external data is quarantined at the boundary, regardless
    // of whether it appears at the top level or inside a function body.
    // getenv inside a function body must still produce a tainted value
    // and reject downstream unattested use.
    let script =
        "#!/bin/bash\nfoo() {\n  local x\n  x=$(getenv USER_INPUT)\n  echo \"$x\"\n}\nfoo\n";
    let stderr = compile_expect_fail("taint_in_fn_body", script);
    common::expect_diag(&stderr, "SB4020");
}

#[test]
fn neg_eval_rejected() {
    let script = "#!/bin/bash\neval echo hello";
    let stderr = compile_expect_fail("eval", script);
    common::expect_diag(&stderr, "SB4001");
}

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
fn sample_gg_rejects_export() {
    let script = std::fs::read_to_string(format!(
        "{}/../../bash-samples/gg",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap_or_default();
    if script.is_empty() {
        return;
    } // skip if not available
    let src = write_tmp("gg.sh", &script);
    let out = sash_cmd(&["-o", "/tmp/sash_e2e_gg", &src]);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("SB5001") || stderr.contains("SB4020"),
        "gg should fail with export or taint error, got:\n{}",
        stderr
    );
}

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
    common::expect_diag(&stderr, "SB4020");
}

#[test]
fn neg_export_declare_x() {
    let script = "#!/bin/bash\ndeclare -x FOO=bar\necho $FOO";
    let stderr = compile_expect_fail("decl_x", script);
    common::expect_diag(&stderr, "SB5001");
}

// ═══════════════════════════════════════════════════════════════
// FUNCTION ARGUMENT TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn pos_func_args_arithmetic() {
    let script = r#"#!/bin/bash
add() {
    echo $(( $1 + $2 ))
}
add 3 4
"#;
    assert_eq!(compile_and_run("func_arith", script), "7\n");
}

#[test]
fn pos_func_args_string() {
    let script = r#"#!/bin/bash
greet() {
    echo "hello $1"
}
greet world
"#;
    assert_eq!(compile_and_run("func_str", script), "hello world\n");
}

#[test]
fn pos_func_multiple_calls() {
    let script = r#"#!/bin/bash
double() {
    echo $(( $1 * 2 ))
}
double 5
double 7
double 0
"#;
    assert_eq!(compile_and_run("func_multi", script), "10\n14\n0\n");
}

#[test]
fn pos_func_two_functions() {
    let script = r#"#!/bin/bash
square() {
    echo $(( $1 * $1 ))
}
negate() {
    echo $(( 0 - $1 ))
}
square 4
negate 7
"#;
    assert_eq!(compile_and_run("func_two", script), "16\n-7\n");
}

#[test]
fn pos_func_argc() {
    let script = r#"#!/bin/bash
count() {
    echo $#
}
count a b c
"#;
    assert_eq!(compile_and_run("func_argc", script), "3\n");
}

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

#[test]
fn pos_for_in_if() {
    let script = r#"#!/bin/bash
x=yes
if [[ $x == "yes" ]]; then
    for n in 1 2 3; do
        echo $n
    done
fi
"#;
    assert_eq!(compile_and_run("for_if", script), "1\n2\n3\n");
}

#[test]
fn pos_case_in_for() {
    let script = r#"#!/bin/bash
for x in apple banana cherry; do
    case $x in
        apple) echo fruit1 ;;
        banana) echo fruit2 ;;
        *) echo other ;;
    esac
done
"#;
    assert_eq!(
        compile_and_run("case_for", script),
        "fruit1\nfruit2\nother\n"
    );
}

// ═══════════════════════════════════════════════════════════════
// HEREDOC TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn pos_heredoc_with_vars() {
    let script = "#!/bin/bash\nname=world\n/bin/cat <<ENDTAG\nhello $name\ngoodbye $name\nENDTAG";
    assert_eq!(
        compile_and_run("hd_vars", script),
        "hello world\ngoodbye world\n"
    );
}

#[test]
fn pos_heredoc_literal() {
    let script = "#!/bin/bash\n/bin/cat <<'EOF'\nhello $name\nEOF";
    assert_eq!(compile_and_run("hd_literal", script), "hello $name\n");
}

#[test]
fn pos_heredoc_tab_strip() {
    let script = "#!/bin/bash\n/bin/cat <<-EOF\n\t\thello\n\t\tworld\nEOF";
    assert_eq!(compile_and_run("hd_tabs", script), "hello\nworld\n");
}

// ═══════════════════════════════════════════════════════════════
// ARRAY TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn pos_array_basic() {
    let script = "#!/bin/bash\narr=(one two three)\necho ${arr[0]}\necho ${arr[1]}\necho ${arr[2]}";
    assert_eq!(compile_and_run("arr_basic", script), "one\ntwo\nthree\n");
}

#[test]
fn pos_array_length() {
    // DD#75 §4: `${#arr}` is the length (retired `${#arr[@]}`).
    let script = "#!/bin/bash\narray arr=(a b c d e)\necho ${#arr}";
    assert_eq!(compile_and_run("arr_len", script), "5\n");
}

#[test]
fn pos_array_assign_element() {
    let script = "#!/bin/bash\narr=(x y z)\narr[1]=Y\necho ${arr[0]} ${arr[1]} ${arr[2]}";
    assert_eq!(compile_and_run("arr_assign", script), "x Y z\n");
}

#[test]
fn pos_array_for_loop() {
    let script = r#"#!/bin/bash
arr=(alpha beta gamma)
for x in @arr; do
    echo $x
done
"#;
    assert_eq!(compile_and_run("arr_for", script), "alpha\nbeta\ngamma\n");
}

// ═══════════════════════════════════════════════════════════════
// ATTEST / TAINT FLOW TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn pos_attest_clears_taint() {
    let script = r#"#!/bin/bash
x=$(getenv MY_VAL)
attest x [[ $x =~ ^[0-9]+$ ]]
echo $x
"#;
    assert_eq!(
        compile_and_run_env("attest_ok", script, &[("MY_VAL", "42")]),
        "42\n"
    );
}

#[test]
fn pos_attest_with_default() {
    let script = r#"#!/bin/bash
x=$(getenv MY_VAL)
attest x [[ -n $x ]]
echo $x
"#;
    assert_eq!(
        compile_and_run_env("attest_default", script, &[("MY_VAL", "hello")]),
        "hello\n"
    );
}

#[test]
fn neg_cmdsub_tainted() {
    let script = "#!/bin/bash\nx=$(curl http://example.com)\necho $x";
    let stderr = compile_expect_fail("cmdsub_taint", script);
    common::expect_diag(&stderr, "SB4020");
}

#[test]
fn neg_tainted_after_reassign() {
    // Even if a var was attested, reassignment from unknown cmdsub re-taints it
    let script = r#"#!/bin/bash
x=$(curl http://example.com)
attest x [[ -n $x ]]
x=$(curl http://other.com)
echo $x
"#;
    let stderr = compile_expect_fail("retaint", script);
    common::expect_diag(&stderr, "SB4020");
}

// ═══════════════════════════════════════════════════════════════
// COMMAND SUBSTITUTION TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn pos_cmdsub_attested() {
    let script = r#"#!/bin/bash
x=$(getenv MY_VAL)
attest x [[ -n $x ]]
echo $x
"#;
    assert_eq!(
        compile_and_run_env("cmdsub_attest", script, &[("MY_VAL", "hello world")]),
        "hello world\n"
    );
}

#[test]
fn pos_cmdsub_in_arithmetic() {
    // Command substitution result used in arithmetic after attest
    let script = r#"#!/bin/bash
n=$(getenv MY_VAL)
attest n [[ $n =~ ^[0-9]+$ ]]
echo $(( n + 10 ))
"#;
    assert_eq!(
        compile_and_run_env("cmdsub_arith", script, &[("MY_VAL", "5")]),
        "15\n"
    );
}

// ═══════════════════════════════════════════════════════════════
// ADDITIONAL COERCION AND EDGE CASE TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn pos_herestring() {
    let script = "#!/bin/bash\nread x <<< \"hello\"\necho $x";
    assert_eq!(compile_and_run("herestring", script), "hello\n");
}

#[test]
fn pos_test_file_exists() {
    let script = "#!/bin/bash\n[[ -f /etc/passwd ]] && echo exists || echo missing";
    assert_eq!(compile_and_run("file_test", script), "exists\n");
}

#[test]
fn pos_test_string_eq() {
    let script = "#!/bin/bash\na=hello\nb=hello\n[[ $a == $b ]] && echo same || echo diff";
    assert_eq!(compile_and_run("str_eq", script), "same\n");
}

#[test]
fn pos_test_string_neq() {
    let script = "#!/bin/bash\na=hello\nb=world\n[[ $a != $b ]] && echo diff || echo same";
    assert_eq!(compile_and_run("str_neq", script), "diff\n");
}

#[test]
fn pos_pipeline_negation() {
    let script = "#!/bin/bash\n! false && echo negated";
    assert_eq!(compile_and_run("pipe_neg", script), "negated\n");
}

#[test]
fn pos_c_style_for() {
    let script = "#!/bin/bash\nfor (( i=0; i<3; i++ )); do echo $i; done";
    assert_eq!(compile_and_run("cfor", script), "0\n1\n2\n");
}

#[test]
fn pos_until_loop() {
    let script = r#"#!/bin/bash
int i=0
while (( i < 3 )); do
    echo $i
    (( i = i + 1 ))
done
"#;
    assert_eq!(compile_and_run("until", script), "0\n1\n2\n");
}

#[test]
fn pos_break_loop() {
    let script = r#"#!/bin/bash
for x in 1 2 3 4 5; do
    [[ $x == "3" ]] && break
    echo $x
done
"#;
    assert_eq!(compile_and_run("break", script), "1\n2\n");
}

#[test]
fn pos_continue_loop() {
    let script = r#"#!/bin/bash
for x in 1 2 3 4 5; do
    [[ $x == "3" ]] && continue
    echo $x
done
"#;
    assert_eq!(compile_and_run("continue", script), "1\n2\n4\n5\n");
}

#[test]
fn pos_alternate_value() {
    let script = "#!/bin/bash\nx=set\necho ${x:+alternate}\ny=\"\"\necho ${y:+alternate}";
    assert_eq!(compile_and_run("alt_val", script), "alternate\n\n");
}

// ═══════════════════════════════════════════════════════════════
// HEREDOC COMMAND SUBSTITUTION REJECTION (SB5002)
// ═══════════════════════════════════════════════════════════════

#[test]
fn neg_heredoc_cmdsub() {
    let script = "#!/bin/bash\n/bin/cat <<EOF\nhello $(/bin/hostname)\nEOF";
    let stderr = compile_expect_fail("hd_cmdsub", script);
    common::expect_diag(&stderr, "SB5002");
}

#[test]
fn pos_heredoc_attested_var() {
    // cat /etc/hostname is auto-attested via known_externals
    let script = "#!/bin/bash\nh=$(/bin/cat /etc/hostname)\n/bin/cat <<EOF\nhello $h\nEOF";
    let output = compile_and_run("hd_attested", script);
    assert!(
        output.starts_with("hello "),
        "expected 'hello <hostname>', got: {}",
        output
    );
}

// ═══════════════════════════════════════════════════════════════
// HERESTRING TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn pos_herestring_read_multi() {
    let script = "#!/bin/bash\nread a b <<< \"hello world\"\necho $a\necho $b";
    assert_eq!(compile_and_run("hs_multi", script), "hello\nworld\n");
}

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

#[test]
fn pos_attest_numeric_check() {
    let script =
        "#!/bin/bash\nx=$(getenv MY_VAL)\nattest x [[ $x =~ ^[0-9]+$ ]]\necho $(( x + 8 ))";
    assert_eq!(
        compile_and_run_env("attest_num", script, &[("MY_VAL", "42")]),
        "50\n"
    );
}

// ═══════════════════════════════════════════════════════════════
// AUTO-ATTEST (known_externals)
// ═══════════════════════════════════════════════════════════════

#[test]
fn pos_auto_attest_hostname() {
    // Explicit attest required — no auto-attest in compiler
    let script = "#!/bin/bash\nh=$(/bin/hostname)\nattest h [[ $h =~ ^[\\w\\-]+$ ]]\necho $h";
    let output = compile_and_run("auto_hostname", script);
    assert!(!output.is_empty(), "expected hostname output");
}

#[test]
fn pos_auto_attest_whoami() {
    let script = "#!/bin/bash\nu=$(/usr/bin/whoami)\nattest u [[ $u =~ ^[\\w\\-]+$ ]]\necho $u";
    let output = compile_and_run("auto_whoami", script);
    assert!(!output.is_empty(), "expected username output");
}

#[test]
fn pos_auto_attest_date() {
    let script = "#!/bin/bash\nd=$(/bin/date +%Y-%m-%d)\nattest d [[ $d =~ ^\\d\\d\\d\\d\\-\\d\\d\\-\\d\\d$ ]]\necho $d";
    let output = compile_and_run("auto_date", script);
    assert!(
        output.trim().len() == 10,
        "expected YYYY-MM-DD, got: {}",
        output
    );
}

#[test]
fn pos_auto_attest_nproc() {
    let script = "#!/bin/bash\nn=$(/usr/bin/nproc)\nattest n [[ $n =~ ^[0-9]+$ ]]\necho $n";
    let output = compile_and_run("auto_nproc", script);
    let n: u32 = output.trim().parse().expect("expected numeric nproc");
    assert!(n > 0, "expected positive nproc");
}

#[test]
fn pos_auto_attest_cat_hostname() {
    let script =
        "#!/bin/bash\nh=$(/bin/cat /etc/hostname)\nattest h [[ $h =~ ^[\\w\\-]+$ ]]\necho $h";
    let output = compile_and_run("auto_cat_hostname", script);
    assert!(!output.is_empty(), "expected hostname from /etc/hostname");
}

#[test]
fn pos_auto_attest_multiple() {
    let script = "#!/bin/bash\nh=$(/bin/hostname)\nattest h [[ $h =~ ^[\\w\\-]+$ ]]\nu=$(/usr/bin/whoami)\nattest u [[ $u =~ ^[\\w\\-]+$ ]]\necho \"$h $u\"";
    let output = compile_and_run("auto_multi", script);
    assert!(
        output.contains(' '),
        "expected 'host user', got: {}",
        output
    );
}

#[test]
fn neg_unknown_cmd_still_tainted() {
    let script = "#!/bin/bash\nx=$(some_unknown_cmd)\necho $x";
    let stderr = compile_expect_fail("unknown_taint", script);
    common::expect_diag(&stderr, "SB4020");
}

// ═══════════════════════════════════════════════════════════════
// BRUSH RECOVERABLE CASES — verify diagnostic then native rewrite
// Each test: (1) original fails with correct code, (2) native compiles,
// (3) native output matches bash.
// ═══════════════════════════════════════════════════════════════

#[test]
fn brush_arith_quoted() {
    // Original: pure-literal arithmetic — should now compile directly
    let script = r#"#!/bin/bash
echo " 1  + 1 == $((1+1))"
echo " 2  * 3 == $((2*3))"
echo " 9  / 3 == $((9/3))"
echo "10  % 3 == $((10%3))"
echo " 2 ** 3 == $((2**3))"
"#;
    let sash_out = compile_and_run("brush_arith_quoted", script);
    let bash_out = bash_output(&script[12..]); // skip shebang
    assert_eq!(sash_out, bash_out);
}

#[test]
fn brush_cmdsub_echo_clean() {
    // $(echo "literal") — builtin with clean args, should compile
    let script = r#"#!/bin/bash
var=$(echo "Batman")
echo $var
"#;
    let sash_out = compile_and_run("brush_cmdsub_echo", script);
    assert_eq!(sash_out, "Batman\n");
}

#[test]
fn brush_cmdsub_comment() {
    // $(# comment; echo "...") — comment inside cmdsub
    let script = "#!/bin/bash\nvar=$(\n  # I'm a comment\n  echo \"Batman\"\n)\necho $var\n";
    let sash_out = compile_and_run("brush_cmdsub_comment", script);
    assert_eq!(sash_out, "Batman\n");
}

#[test]
fn brush_let_clean() {
    // let x=10 — arithmetic literal assignment, x is clean
    let script = "#!/bin/bash\nlet x=10; echo \"x=10 => $?; x==${x}\"";
    let sash_out = compile_and_run("brush_let_clean", script);
    let bash_out = bash_output("let x=10; echo \"x=10 => $?; x==${x}\"");
    assert_eq!(sash_out, bash_out);
}

#[test]
fn brush_home_clean() {
    // $HOME requires getenv — no implicit env access (DD#53)
    let script = "#!/bin/bash\nx=$(getenv HOME)\nattest x [[ -n $x ]]\necho $x";
    let sash_out = compile_and_run("brush_home_clean", script);
    let bash_out = bash_output("echo $HOME");
    assert_eq!(sash_out, bash_out);
}

#[test]
fn brush_pwd_clean() {
    // $PWD requires getenv — no implicit env access (DD#53)
    let script = "#!/bin/bash\nx=$(getenv PWD)\nattest x [[ $x =~ ^/ ]]\necho $x";
    let sash_out = compile_and_run("brush_pwd_clean", script);
    assert!(!sash_out.is_empty(), "PWD should produce output");
}

#[test]
fn brush_path_check() {
    // $PATH requires getenv — no implicit env access (DD#53)
    let script = r#"#!/bin/bash
x=$(getenv PATH)
attest x [[ -n $x ]]
if [[ -z "$x" ]]; then
  echo "PATH is not set or is empty"
else
  echo "PATH is set and non-empty"
fi
"#;
    let sash_out = compile_and_run("brush_path_check", script);
    assert_eq!(sash_out, "PATH is set and non-empty\n");
}

#[test]
fn brush_cd_pwd() {
    // cd / then pwd — pwd uses getcwd(2), independent of $PWD (DD#53).
    let script = "#!/bin/bash\ncd /\npwd";
    let sash_out = compile_and_run("brush_cd_pwd", script);
    assert_eq!(sash_out, "/\n");
}

#[test]
fn brush_getenv_attest_native() {
    // Original: getenv produces tainted var — needs attest
    // Step 1: without attest, fails
    let raw = "#!/bin/bash\nx=$(getenv MY_VAR)\necho $x";
    let stderr = compile_expect_fail("brush_getenv_raw", raw);
    common::expect_diag(&stderr, "SB4020");

    // Step 2: with attest, compiles and output matches
    let attested = r#"#!/bin/bash
x=$(getenv MY_VAR)
attest x [[ -n $x ]]
echo $x
"#;
    let sash_out = compile_and_run_env("brush_getenv_attested", attested, &[("MY_VAR", "hello")]);
    assert_eq!(sash_out, "hello\n");
}

#[test]
fn brush_curl_tainted() {
    // curl output is correctly tainted — verify diagnostic
    let script = "#!/bin/bash\nx=$(curl http://evil.com)\necho $x";
    let stderr = compile_expect_fail("brush_curl", script);
    common::expect_diag(&stderr, "SB4020");
}

#[test]
fn brush_undefined_var() {
    // DD#53: Undefined vars are empty strings (not env lookups, not tainted).
    // They compile and produce empty output.
    let script = "#!/bin/bash\necho ${undefined}";
    let out = compile_and_run_env("brush_undefined", script, &[]);
    assert_eq!(out, "\n", "undefined var should expand to empty string");
}

#[test]
fn brush_heredoc_clean() {
    // Heredoc with no cmdsub — content is clean
    let script = "#!/bin/bash\n/bin/cat <<EOF\nhello world\nEOF\n";
    let sash_out = compile_and_run("brush_heredoc_clean", script);
    assert_eq!(sash_out, "hello world\n");
}

#[test]
fn brush_heredoc_cmdsub_rejected() {
    // Heredoc with command substitution — SB5002
    let script = "#!/bin/bash\n/bin/cat <<END\n$(echo hi)\nEND\n";
    let stderr = compile_expect_fail("brush_heredoc_cmdsub", script);
    common::expect_diag(&stderr, "SB5002");
}

#[test]
fn brush_indirect_rejected() {
    // ${!ref} — SB4003
    let script = "#!/bin/bash\nvar=hello\nref=var\necho ${!ref}";
    let stderr = compile_expect_fail("brush_indirect", script);
    common::expect_diag(&stderr, "SB4003");
}

#[test]
fn brush_export_rejected() {
    // export — SB5001
    let script = "#!/bin/bash\nexport MY_VAR=hello";
    let stderr = compile_expect_fail("brush_export", script);
    common::expect_diag(&stderr, "SB5001");
}

// ═══════════════════════════════════════════════════════════════
// EXTGLOB REJECTION (SB5005 — DD#20/DD#39)
// ═══════════════════════════════════════════════════════════════

#[test]
fn neg_extglob_at() {
    let script = "#!/bin/bash\ncase x in @(a|b)) echo match;; esac";
    let stderr = compile_expect_fail("extglob_at", script);
    common::expect_diag(&stderr, "SB5005");
}

#[test]
fn neg_extglob_plus() {
    let script = "#!/bin/bash\ncase x in +(a|b)) echo match;; esac";
    let stderr = compile_expect_fail("extglob_plus", script);
    common::expect_diag(&stderr, "SB5005");
}

#[test]
fn neg_extglob_question() {
    let script = "#!/bin/bash\ncase x in ?(a|b)) echo match;; esac";
    let stderr = compile_expect_fail("extglob_q", script);
    common::expect_diag(&stderr, "SB5005");
}

#[test]
fn neg_extglob_bang() {
    let script = "#!/bin/bash\ncase x in !(a|b)) echo match;; esac";
    let stderr = compile_expect_fail("extglob_bang", script);
    common::expect_diag(&stderr, "SB5005");
}

#[test]
fn pos_case_bare_star() {
    // bare * in case should NOT trigger SB5005
    let script = "#!/bin/bash\ncase hello in *) echo matched;; esac";
    assert_eq!(compile_and_run("case_star", script), "matched\n");
}

#[test]
fn neg_source_dot_rejected() {
    let script = "#!/bin/bash\n. ./file.sh";
    let stderr = compile_expect_fail("source_dot", script);
    common::expect_diag(&stderr, "SB4002");
}

#[test]
fn neg_source_keyword_rejected() {
    let script = "#!/bin/bash\nsource ./file.sh";
    let stderr = compile_expect_fail("source_kw", script);
    common::expect_diag(&stderr, "SB4002");
}

// ── DD#55: suppress prefix (- cmd) ──────────────────────────

#[test]
fn pos_suppress_prefix_false() {
    // - /bin/false; echo ok → should succeed, echo ok
    let script = "#!/bin/bash\n- /bin/false; echo ok";
    assert_eq!(compile_and_run("suppress_false", script), "ok\n");
}

#[test]
fn pos_suppress_pipeline_segment() {
    // - /bin/false | /bin/true → suppressed first segment, pipeline succeeds
    let script = "#!/bin/bash\n- /bin/false | /bin/true";
    // Should compile and run with exit 0
    let src = write_tmp("suppress_pipe.sh", script);
    let bin = "/tmp/sash_e2e_suppress_pipe";
    let out = sash_cmd(&["-o", bin, &src]);
    assert!(
        out.status.success(),
        "compile failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let run = Command::new(bin).output().expect("run failed");
    assert!(
        run.status.success(),
        "expected exit 0, got {:?}",
        run.status
    );
}

#[test]
fn neg_unsuppressed_false_aborts() {
    // /bin/false; echo should_not_reach → binary should abort (nonzero exit)
    let script = "#!/bin/bash\n/bin/false\necho should_not_reach";
    let src = write_tmp("unsup_false.sh", script);
    let bin = "/tmp/sash_e2e_unsup_false";
    let out = sash_cmd(&["-o", bin, &src]);
    assert!(
        out.status.success(),
        "compile failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let run = Command::new(bin).output().expect("run failed");
    assert!(!run.status.success(), "expected nonzero exit");
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        !stdout.contains("should_not_reach"),
        "should not reach after /bin/false"
    );
}

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

/// An external used *only* inside a `$( … )` substitution must appear
/// in the manifest (chain-of-trust: a rootfs built from the manifest
/// would otherwise be missing it). Regression for the demo-prep finding.
#[test]
fn manifest_includes_command_substitution_external() {
    let deps = manifest(
        "cmdsub",
        "x=$(date +%s); attest x [[ -n $x ]]; echo \"$x\"\n",
    );
    assert!(
        deps.iter().any(|d| d.ends_with("/date")),
        "expected the $(date) external in the manifest, got: {:?}",
        deps
    );
}

/// `(( … ))` as a statement/condition lowers to the internal `LetExit`
/// sigil. It must never appear in an external-programs manifest.
/// Regression for the demo-prep finding.
#[test]
fn manifest_excludes_internal_let_exit_sigil() {
    let deps = manifest("letexit", "if (( 1 + 1 )); then echo hi; fi\n");
    assert!(
        !deps.iter().any(|d| d.contains("__let_exit")),
        "manifest leaked the __let_exit sigil: {:?}",
        deps
    );
    // The only command is the `echo` builtin — no external to list.
    assert!(
        deps.is_empty(),
        "expected an empty manifest, got: {:?}",
        deps
    );
}

/// No compiler-internal `__…` sigil may ever reach the manifest.
#[test]
fn manifest_excludes_all_internal_sigils() {
    // `! …` emits the `NegateTestResult` sigil; `> file` emits the
    // redirect-push/pop sigils. Neither may appear in the manifest.
    let deps = manifest(
        "sigils",
        "if ! date; then echo hi > /tmp/sash_mani_sig; fi\n",
    );
    assert!(
        !deps.iter().any(|d| d.contains("__")),
        "manifest leaked an internal sigil: {:?}",
        deps
    );
    assert!(
        deps.iter().any(|d| d.ends_with("/date")),
        "expected the negated external in the manifest, got: {:?}",
        deps
    );
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
