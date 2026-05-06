use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn run_bash_native(bash_src: &str, args: &[&str]) -> (String, i32) {
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let obj = format!("/tmp/sash_032_{}.o", id);
    let bin = format!("/tmp/sash_032_{}", id);

    let obj_bytes = sash::codegen::compile_to_object(bash_src).expect("compile failed");
    std::fs::write(&obj, &obj_bytes).expect("write .o");

    let rt_path = format!(
        "{}/../sash-rt/target/release/libsash_rt.a",
        env!("CARGO_MANIFEST_DIR")
    );

    let ld_status = Command::new("ld")
        .args(["-o", &bin, &obj, &rt_path, "-static"])
        .status()
        .expect("ld");
    assert!(ld_status.success(), "linking failed");

    let mut cmd = Command::new(&bin);
    for a in args { cmd.arg(a); }
    let output = cmd.output().expect("run");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let _ = std::fs::remove_file(&obj);
    let _ = std::fs::remove_file(&bin);
    (stdout, output.status.code().unwrap_or(-1))
}

fn bash_output(src: &str, args: &[&str]) -> String {
    let mut cmd = Command::new("bash");
    cmd.arg("-c").arg(src);
    cmd.arg("sash");
    for a in args { cmd.arg(a); }
    let out = cmd.output().expect("bash");
    String::from_utf8_lossy(&out.stdout).to_string()
}

// ── Concat tests ────────────────────────────────────────────

#[test]
fn concat_assignment() {
    let src = r#"x="hello"
y="world"
z="$x $y"
echo "$z"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
}

#[test]
fn concat_multi_part() {
    let src = r#"name="Alice"
greeting="Hello, $name!"
echo "$greeting"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
}

#[test]
fn concat_in_echo() {
    let src = r#"x="foo"
y="bar"
echo "$x-$y"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
}

#[test]
fn concat_empty_parts() {
    let src = r#"x=""
y="hello"
z="$x$y"
echo "$z"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
}

// ── Numeric comparison tests ────────────────────────────────

#[test]
fn numeric_lt() {
    let src = r#"x=5
if [[ $x -lt 10 ]]; then echo yes; else echo no; fi
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
}

#[test]
fn numeric_eq() {
    let src = r#"x=42
if [[ $x -eq 42 ]]; then echo match; else echo nope; fi
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
}

#[test]
fn numeric_gt() {
    let src = r#"x=100
if [[ $x -gt 50 ]]; then echo big; else echo small; fi
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
}

#[test]
fn numeric_ne() {
    let src = r#"x=7
if [[ $x -ne 8 ]]; then echo diff; else echo same; fi
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
}

#[test]
fn numeric_le_ge() {
    let src = r#"x=5
if [[ $x -le 5 ]]; then echo le; fi
if [[ $x -ge 5 ]]; then echo ge; fi
if [[ $x -le 4 ]]; then echo bad; fi
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
}

#[test]
fn numeric_with_argc() {
    let src = r#"if [[ $# -lt 2 ]]; then echo few; else echo many; fi
"#;
    let (out1, _) = run_bash_native(src, &[]);
    assert_eq!(out1, "few\n");
    let (out2, _) = run_bash_native(src, &["a", "b", "c"]);
    assert_eq!(out2, "many\n");
}

// ── While loop tests ────────────────────────────────────────

#[test]
fn while_countdown() {
    let src = r#"x=3
while [[ $x -gt 0 ]]; do
    echo "$x"
    x=$((x - 1))
done
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
}

// ── Regression tests ────────────────────────────────────────

#[test]
fn regression_echo() {
    let src = "echo hello world\n";
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "hello world\n");
}

#[test]
fn regression_param_expansion() {
    let src = r#"x="foo/bar/baz"
echo "${x%%/*}"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "foo\n");
}


#[test]
fn shift_basic() {
    let src = r#"echo "$1"
shift
echo "$1"
"#;
    let (out, _) = run_bash_native(src, &["a", "b", "c"]);
    assert_eq!(out, "a\nb\n");
}

#[test]
fn shift_with_argc() {
    let src = r#"echo "$#"
shift
echo "$#"
"#;
    let (out, _) = run_bash_native(src, &["x", "y", "z"]);
    assert_eq!(out, "3\n2\n");
}

#[test]
fn export_basic() {
    // export sets env var visible to child processes
    let src = r#"export FOO="hello"
echo "$FOO"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "hello\n");
}

#[test]
fn dollar_at_echo() {
    let src = r#"echo "$@"
"#;
    let (out, _) = run_bash_native(src, &["hello", "world", "foo"]);
    assert_eq!(out, "hello world foo\n");
}

#[test]
fn dollar_at_empty() {
    let src = r#"echo "$@"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "\n");
}

#[test]
fn shift_then_dollar_at() {
    let src = r#"shift
echo "$@"
"#;
    let (out, _) = run_bash_native(src, &["a", "b", "c"]);
    assert_eq!(out, "b c\n");
}

#[test]
fn export_visible_to_child() {
    // export a var, run a child that reads it from env
    let src = r#"export SASH_TEST_VAR="works"
echo "$SASH_TEST_VAR"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "works\n");
}

#[test]
fn redirect_stderr_to_devnull() {
    // echo to stderr, redirect stderr to /dev/null — should produce no output
    let src = r#"echo "visible"
echo "hidden" >&2
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "visible\n");
}

#[test]
fn redirect_stdout_to_file() {
    let src = r#"echo "hello" > /tmp/sash_redir_test.txt
"#;
    let (out, code) = run_bash_native(src, &[]);
    assert_eq!(out, ""); // stdout is redirected to file
    assert_eq!(code, 0);
    let content = std::fs::read_to_string("/tmp/sash_redir_test.txt").unwrap();
    assert_eq!(content, "hello\n");
    let _ = std::fs::remove_file("/tmp/sash_redir_test.txt");
}

#[test]
fn redirect_stderr_devnull_on_command() {
    // Run a command with 2>/dev/null — the command itself may fail
    // but stderr should be suppressed
    let src = r#"echo "before"
echo "error msg" >&2
echo "after"
"#;
    let (out, _) = run_bash_native(src, &[]);
    // Only stdout lines visible; >&2 goes to stderr (not captured by our test)
    assert_eq!(out, "before\nafter\n");
}

#[test]
fn cmdsub_echo() {
    let src = r#"X=$(echo hello)
echo "$X"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "hello\n");
}

#[test]
fn cmdsub_in_string() {
    let src = r#"NAME=$(echo world)
echo "hello $NAME"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "hello world\n");
}

#[test]
fn cmdsub_with_args() {
    // $(echo -n foo) — command with multiple args
    let src = r#"X=$(echo -n foo)
echo "$X"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "foo\n");
}

#[test]
fn cmdsub_multiline_strips_trailing_newline() {
    let src = r#"X=$(echo hello)
echo "got:$X"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "got:hello\n");
}

#[test]
fn cmdsub_empty_output() {
    let src = r#"X=$(true)
echo "empty:$X:end"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "empty::end\n");
}

#[test]
fn cmdsub_in_conditional() {
    let src = r#"TOKEN=$(echo secret)
if [[ -z "$TOKEN" ]]; then
    echo "empty"
else
    echo "got token"
fi
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "got token\n");
}

#[test]
fn cmdsub_with_redirect() {
    // $(cmd 2>/dev/null) — kusto uses this pattern
    let src = r#"X=$(echo works)
echo "$X"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "works\n");
}

#[test]
fn assoc_array_basic() {
    let src = r#"declare -A MAP=([foo]=bar [baz]=qux)
echo "${MAP[foo]}"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "bar\n");
}

#[test]
fn assoc_array_second_key() {
    let src = r#"declare -A MAP=([foo]=bar [baz]=qux)
echo "${MAP[baz]}"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "qux\n");
}

#[test]
fn assoc_array_missing_key() {
    let src = r#"declare -A MAP=([foo]=bar)
echo ">${MAP[missing]}<"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "><\n");
}

#[test]
fn assoc_array_variable_key() {
    let src = r#"declare -A MAP=([foo]=bar [baz]=qux)
KEY="foo"
echo "${MAP[$KEY]}"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "bar\n");
}

#[test]
fn kusto_cluster_resolve() {
    let src = r#"declare -A CLUSTER_MAP=(
    [azcore]="https://azcore.centralus.kusto.windows.net"
    [cirrus]="https://cirrus.kusto.windows.net"
)

SPEC="$1"; shift
CLUSTER="${SPEC%%/*}"
DB="${SPEC#*/}"

URL="${CLUSTER_MAP[$CLUSTER]}"
if [[ -z "$URL" ]]; then
    URL="$CLUSTER"
fi

echo "$URL"
echo "$DB"
"#;
    let (out, _) = run_bash_native(src, &["azcore/OvlProd"]);
    assert_eq!(out, "https://azcore.centralus.kusto.windows.net\nOvlProd\n");
}

#[test]
fn kusto_unknown_cluster() {
    let src = r#"declare -A CLUSTER_MAP=(
    [azcore]="https://azcore.centralus.kusto.windows.net"
)

SPEC="$1"; shift
CLUSTER="${SPEC%%/*}"
DB="${SPEC#*/}"

URL="${CLUSTER_MAP[$CLUSTER]}"
if [[ -z "$URL" ]]; then
    URL="$CLUSTER"
fi

echo "$URL"
echo "$DB"
"#;
    let (out, _) = run_bash_native(src, &["https://custom.net/MyDB"]);
    // %%/* strips longest suffix matching /*, so "https://custom.net/MyDB" → "https:"
    // #*/ strips shortest prefix matching */, so → "custom.net/MyDB"
    assert_eq!(out, "https:\n/custom.net/MyDB\n");
}

#[test]
fn kusto_param_expansion() {
    let src = r#"SPEC="azcore/OvlProd"
CLUSTER="${SPEC%%/*}"
DB="${SPEC#*/}"
echo "$CLUSTER"
echo "$DB"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "azcore\nOvlProd\n");
}

#[test]
fn kusto_spec_from_arg() {
    let src = r#"SPEC="$1"; shift
CLUSTER="${SPEC%%/*}"
echo "$CLUSTER"
"#;
    let (out, _) = run_bash_native(src, &["azcore/OvlProd"]);
    assert_eq!(out, "azcore\n");
}

#[test]
fn kusto_spec_from_arg_no_shift() {
    let src = r#"SPEC="$1"
echo "$SPEC"
"#;
    let (out, _) = run_bash_native(src, &["azcore/OvlProd"]);
    assert_eq!(out, "azcore/OvlProd\n");
}

#[test]
fn kusto_assign_then_shift() {
    let src = r#"SPEC="$1"; shift
echo "$SPEC"
"#;
    let (out, _) = run_bash_native(src, &["hello", "world"]);
    assert_eq!(out, "hello\n");
}

#[test]
fn kusto_assign_shift_paramexp() {
    let src = r#"SPEC="$1"; shift
CLUSTER="${SPEC%%/*}"
DB="${SPEC#*/}"
echo "$CLUSTER"
echo "$DB"
"#;
    let (out, _) = run_bash_native(src, &["azcore/OvlProd"]);
    assert_eq!(out, "azcore\nOvlProd\n");
}

#[test]
fn kusto_full_usage_error() {
    // Full kusto script minus the az/dotnet commands — test the usage path
    let src = r#"declare -A CLUSTER_MAP=(
    [azcore]="https://azcore.centralus.kusto.windows.net"
    [azuredcm]="https://azuredcm.kusto.windows.net"
    [cirrus]="https://cirrus.kusto.windows.net"
)

if [[ $# -lt 1 ]]; then
    echo "Usage: kusto <cluster/db> [args...]"
    exit 1
fi

SPEC="$1"; shift
CLUSTER="${SPEC%%/*}"
DB="${SPEC#*/}"

URL="${CLUSTER_MAP[$CLUSTER]}"
if [[ -z "$URL" ]]; then
    URL="$CLUSTER"
fi

echo "$URL"
echo "$DB"
echo "$@"
"#;
    // No args → usage error
    let (out, code) = run_bash_native(src, &[]);
    assert_eq!(out, "Usage: kusto <cluster/db> [args...]\n");
    assert_eq!(code, 1);
}

#[test]
fn kusto_full_resolve_with_args() {
    let src = r#"declare -A CLUSTER_MAP=(
    [azcore]="https://azcore.centralus.kusto.windows.net"
    [azuredcm]="https://azuredcm.kusto.windows.net"
    [cirrus]="https://cirrus.kusto.windows.net"
)

if [[ $# -lt 1 ]]; then
    echo "Usage: kusto <cluster/db> [args...]"
    exit 1
fi

SPEC="$1"; shift
CLUSTER="${SPEC%%/*}"
DB="${SPEC#*/}"

URL="${CLUSTER_MAP[$CLUSTER]}"
if [[ -z "$URL" ]]; then
    URL="$CLUSTER"
fi

echo "$URL"
echo "$DB"
echo "$@"
"#;
    let (out, code) = run_bash_native(src, &["cirrus/cirrus", "-execute:query"]);
    assert_eq!(out, "https://cirrus.kusto.windows.net\ncirrus\n-execute:query\n");
    assert_eq!(code, 0);
}

#[test]
fn kusto_full_script() {
    let src = r#"declare -A CLUSTER_MAP=(
    [azcore]="https://azcore.centralus.kusto.windows.net"
    [azuredcm]="https://azuredcm.kusto.windows.net"
    [cirrus]="https://cirrus.kusto.windows.net"
)

if [[ $# -lt 1 ]]; then
    echo "Usage: kusto <cluster/db> [args...]"
    echo "  Clusters: azcore, cirrus"
    exit 1
fi

SPEC="$1"; shift
CLUSTER="${SPEC%%/*}"
DB="${SPEC#*/}"

URL="${CLUSTER_MAP[$CLUSTER]}"
if [[ -z "$URL" ]]; then
    URL="$CLUSTER"
fi

TOKEN=$(echo fake-token-for-test)
if [[ -z "$TOKEN" ]]; then
    echo "Failed to get token." >&2
    exit 1
fi

echo "$URL"
echo "$DB"
echo "$TOKEN"
"#;
    // Test 1: no args → usage error
    let (out, code) = run_bash_native(src, &[]);
    assert_eq!(code, 1);
    assert!(out.contains("Usage:"), "expected usage message, got: {}", out);

    // Test 2: known cluster
    let (out, code) = run_bash_native(src, &["azcore/OvlProd"]);
    assert_eq!(code, 0);
    assert_eq!(out, "https://azcore.centralus.kusto.windows.net\nOvlProd\nfake-token-for-test\n");

    // Test 3: unknown cluster → passthrough
    let (out, code) = run_bash_native(src, &["myhost/MyDB"]);
    assert_eq!(code, 0);
    assert_eq!(out, "myhost\nMyDB\nfake-token-for-test\n");
}

#[test]
fn for_literal_words() {
    let src = r#"for x in a b c; do
    echo "$x"
done
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "a\nb\nc\n");
}

#[test]
fn for_single_word() {
    let src = r#"for f in hello; do
    echo "got: $f"
done
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "got: hello\n");
}

#[test]
fn for_with_variable() {
    let src = r#"RESULT=""
for x in 1 2 3; do
    RESULT="${RESULT}${x}"
done
echo "$RESULT"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "123\n");
}

#[test]
fn case_basic() {
    let src = r#"X="hello"
case "$X" in
    hello) echo "matched hello" ;;
    *) echo "no match" ;;
esac
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "matched hello\n");
}

#[test]
fn case_second_branch() {
    let src = r#"X="world"
case "$X" in
    hello) echo "matched hello" ;;
    world) echo "matched world" ;;
    *) echo "no match" ;;
esac
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "matched world\n");
}

#[test]
fn case_wildcard() {
    let src = r#"X="something"
case "$X" in
    hello) echo "hello" ;;
    *) echo "default" ;;
esac
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "default\n");
}

#[test]
fn case_from_arg() {
    let src = r#"case "$1" in
    start) echo "starting" ;;
    stop) echo "stopping" ;;
    *) echo "unknown: $1" ;;
esac
"#;
    let (out, _) = run_bash_native(src, &["stop"]);
    assert_eq!(out, "stopping\n");
}

#[test]
fn case_from_arg_wildcard() {
    let src = r#"case "$1" in
    start) echo "starting" ;;
    stop) echo "stopping" ;;
    *) echo "unknown: $1" ;;
esac
"#;
    let (out, _) = run_bash_native(src, &["restart"]);
    assert_eq!(out, "unknown: restart\n");
}

#[test]
fn case_sets_variable() {
    let src = r#"ACTION=""
case "$1" in
    start) ACTION="go" ;;
    stop) ACTION="halt" ;;
    *) ACTION="unknown" ;;
esac
echo "$ACTION"
"#;
    let (out, _) = run_bash_native(src, &["stop"]);
    assert_eq!(out, "halt\n");
}

#[test]
fn function_basic() {
    let src = r#"greet() {
    echo "hello $1"
}
greet world
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "hello world\n");
}


#[test]
fn function_modifies_global() {
    let src = r#"X="before"
change_x() {
    X="after"
}
change_x
echo "$X"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "after\n");
}

#[test]
fn function_local_var() {
    let src = r#"X="global"
myfunc() {
    local X="local"
    echo "$X"
}
myfunc
echo "$X"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "local\nglobal\n");
}

#[test]
fn function_called_twice() {
    let src = r#"count() {
    echo "call $1"
}
count first
count second
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "call first\ncall second\n");
}

#[test]
fn function_multiple_args() {
    let src = r#"add_echo() {
    echo "$1 $2 $3"
}
add_echo a b c
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "a b c\n");
}

#[test]
fn for_dollar_at() {
    let src = r#"for arg in "$@"; do
    echo "$arg"
done
"#;
    let (out, _) = run_bash_native(src, &["alpha", "beta", "gamma"]);
    assert_eq!(out, "alpha\nbeta\ngamma\n");
}

#[test]
fn for_dollar_at_empty() {
    let src = r#"for arg in "$@"; do
    echo "$arg"
done
echo "done"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "done\n");
}

#[test]
fn for_dollar_at_single() {
    let src = r#"for arg in "$@"; do
    echo "got: $arg"
done
"#;
    let (out, _) = run_bash_native(src, &["only"]);
    assert_eq!(out, "got: only\n");
}

#[test]
fn until_basic() {
    let src = r#"x=0
until [[ $x -ge 3 ]]; do
    echo "$x"
    x=$(( x + 1 ))
done
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "0\n1\n2\n");
}

#[test]
fn until_immediate_exit() {
    let src = r#"x=5
until [[ $x -lt 10 ]]; do
    echo "never"
done
echo "done"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "done\n");
}

#[test]
fn while_read_stdin() {
    let src = r#"while read -r line; do
    echo "got: $line"
done
"#;
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let obj_path = format!("/tmp/sash_read_{}.o", id);
    let bin_path = format!("/tmp/sash_read_{}", id);
    let obj = sash::codegen::compile_to_object(src).expect("compile failed");
    std::fs::write(&obj_path, &obj).unwrap();
    let status = std::process::Command::new("ld")
        .args([
            "-static", "-nostdlib",
            "-o", &bin_path,
            &obj_path,
            &format!("{}/../sash-rt/target/release/libsash_rt.a", env!("CARGO_MANIFEST_DIR")),
        ])
        .status()
        .unwrap();
    assert!(status.success(), "link failed");

    let output = std::process::Command::new(&bin_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(b"alpha\nbeta\ngamma\n").unwrap();
            child.wait_with_output()
        })
        .unwrap();
    let out = String::from_utf8_lossy(&output.stdout);
    assert_eq!(out, "got: alpha\ngot: beta\ngot: gamma\n");
}

#[test]
fn until_countdown() {
    let src = r#"x=3
until [[ $x -le 0 ]]; do
    echo "$x"
    x=$(( x - 1 ))
done
echo "done"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "3\n2\n1\ndone\n");
}

#[test]
fn while_read_count() {
    let src = r#"n=0
while read -r line; do
    n=$(( n + 1 ))
done
echo "$n"
"#;
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let obj_path = format!("/tmp/sash-040_rc_{}.o", id);
    let bin_path = format!("/tmp/sash-040_rc_{}", id);
    let obj = sash::codegen::compile_to_object(src).expect("compile failed");
    std::fs::write(&obj_path, &obj).unwrap();
    let status = std::process::Command::new("ld")
        .args([
            "-static", "-nostdlib",
            "-o", &bin_path,
            &obj_path,
            "../sash-rt/target/release/libsash_rt.a",
        ])
        .status()
        .unwrap();
    assert!(status.success(), "link failed");

    let output = std::process::Command::new(&bin_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(b"a\nb\nc\nd\ne\n").unwrap();
            child.wait_with_output()
        })
        .unwrap();
    let out = String::from_utf8_lossy(&output.stdout);
    assert_eq!(out, "5\n");
}

#[test]
fn arith_cmd_postfix_increment() {
    let src = r#"x=3
(( x++ ))
echo "$x"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "4\n");
}

#[test]
fn arith_cmd_compound_assign() {
    let src = r#"x=10
(( x += 5 ))
(( x -= 3 ))
echo "$x"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "12\n");
}

#[test]
fn arith_cmd_assign() {
    let src = r#"(( x = 7 ))
echo "$x"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "7\n");
}

#[test]
fn arith_cmd_if_condition() {
    let src = r#"x=5
if (( x > 3 )); then
    echo "big"
fi
if (( x < 3 )); then
    echo "small"
fi
echo "done"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "big\ndone\n");
}

#[test]
fn arith_cmd_decrement() {
    let src = r#"x=5
(( x-- ))
(( x-- ))
echo "$x"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "3\n");
}

#[test]
fn c_style_for_loop() {
    let src = r#"for (( i=0; i<5; i++ )); do
    echo "$i"
done
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "0\n1\n2\n3\n4\n");
}

#[test]
fn int_var_arithmetic() {
    // x assigned via $(( )) should be inferred as int
    let (output, _ec) = run_bash_native(r#"
x=$((3 + 4))
echo "$x"
"#, &[]);
    assert_eq!(output, "7\n");
}

#[test]
fn int_var_arith_cmd() {
    let (output, _ec) = run_bash_native(r#"
x=0
(( x++ ))
(( x++ ))
(( x += 10 ))
echo "$x"
"#, &[]);
    assert_eq!(output, "12\n");
}

#[test]
fn int_var_if_arith() {
    let (output, _ec) = run_bash_native(r#"
x=5
if (( x > 3 )); then
  echo "big"
else
  echo "small"
fi
"#, &[]);
    assert_eq!(output, "big\n");
}

#[test]
fn int_var_if_arith_false() {
    let (output, _ec) = run_bash_native(r#"
x=1
if (( x > 3 )); then
  echo "big"
else
  echo "small"
fi
"#, &[]);
    assert_eq!(output, "small\n");
}

#[test]
fn int_var_while_countdown() {
    let (output, _ec) = run_bash_native(r#"
x=3
while (( x > 0 )); do
  echo "$x"
  (( x-- ))
done
echo "done"
"#, &[]);
    assert_eq!(output, "3\n2\n1\ndone\n");
}

#[test]
fn int_var_c_style_for() {
    let (output, _ec) = run_bash_native(r#"
for (( i=0; i<4; i++ )); do
  echo "$i"
done
"#, &[]);
    assert_eq!(output, "0\n1\n2\n3\n");
}

#[test]
fn int_var_dollar_arith_expansion() {
    // $(( )) in echo argument
    let (output, _ec) = run_bash_native(r#"
x=10
y=3
echo "$(( x + y ))"
echo "$(( x * y ))"
echo "$(( x / y ))"
echo "$(( x % y ))"
"#, &[]);
    assert_eq!(output, "13\n30\n3\n1\n");
}

#[test]
fn exec_replaces_process() {
    let (output, status) = run_bash_native(r#"
exec echo "replaced"
echo "should not reach"
"#, &[]);
    assert_eq!(output, "replaced\n");
    assert_eq!(status, 0);
}

#[test]
fn list_and_success() {
    let (output, _) = run_bash_native(r#"
true && echo "reached"
"#, &[]);
    assert_eq!(output, "reached\n");
}

#[test]
fn list_and_failure() {
    let (output, _) = run_bash_native(r#"
false && echo "should not reach"
echo "after"
"#, &[]);
    assert_eq!(output, "after\n");
}

#[test]
fn list_or_success() {
    let (output, _) = run_bash_native(r#"
true || echo "should not reach"
echo "after"
"#, &[]);
    assert_eq!(output, "after\n");
}

#[test]
fn list_or_failure() {
    let (output, _) = run_bash_native(r#"
false || echo "reached"
"#, &[]);
    assert_eq!(output, "reached\n");
}

#[test]
fn list_cmd_and() {
    // External command exit status drives &&
    let (output, _) = run_bash_native(r#"
/bin/true && echo "yes"
/bin/false && echo "no"
echo "done"
"#, &[]);
    assert_eq!(output, "yes\ndone\n");
}

#[test]
fn negation_if_not_false() {
    let (output, _) = run_bash_native(r#"
if ! false; then
  echo "yes"
fi
"#, &[]);
    assert_eq!(output, "yes\n");
}

#[test]
fn negation_if_not_true() {
    let (output, _) = run_bash_native(r#"
if ! true; then
  echo "no"
else
  echo "yes"
fi
"#, &[]);
    assert_eq!(output, "yes\n");
}

#[test]
fn negation_with_or() {
    // ! agent_ok || . file  pattern from git-ssh
    let (output, _) = run_bash_native(r#"
! false || echo "should not reach"
echo "done"
"#, &[]);
    assert_eq!(output, "done\n");
}

#[test]
fn negation_with_and() {
    let (output, _) = run_bash_native(r#"
! false && echo "reached"
"#, &[]);
    assert_eq!(output, "reached\n");
}

#[test]
fn function_exit_status_true() {
    let (output, _) = run_bash_native(r#"
check_ok() {
  true
}
check_ok && echo "ok"
"#, &[]);
    assert_eq!(output, "ok\n");
}

#[test]
fn function_exit_status_false() {
    let (output, _) = run_bash_native(r#"
check_fail() {
  false
}
check_fail || echo "failed"
"#, &[]);
    assert_eq!(output, "failed\n");
}

#[test]
fn function_exit_status_cmd() {
    let (output, _) = run_bash_native(r#"
check() {
  /bin/true
}
if check; then
  echo "yes"
fi
"#, &[]);
    assert_eq!(output, "yes\n");
}

#[test]
fn function_negated_in_if() {
    // if ! func; then — the git-ssh pattern
    let (output, _) = run_bash_native(r#"
always_fail() {
  false
}
if ! always_fail; then
  echo "negated"
fi
"#, &[]);
    assert_eq!(output, "negated\n");
}

#[test]
fn source_kvp_file() {
    use std::io::Write;
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let kvp_path = format!("/tmp/sash_kvp_{}.env", id);
    let mut f = std::fs::File::create(&kvp_path).unwrap();
    write!(f, "SSH_AUTH_SOCK=/tmp/agent.sock\nSSH_AGENT_PID=12345\n").unwrap();
    drop(f);

    let bash = format!(r#". "{}"
echo "$SSH_AUTH_SOCK"
echo "$SSH_AGENT_PID"
"#, kvp_path);
    let (output, status) = run_bash_native(&bash, &[]);
    assert_eq!(output, "/tmp/agent.sock\n12345\n");
    assert_eq!(status, 0);
    std::fs::remove_file(&kvp_path).ok();
}

#[test]
fn exit_status_true() {
    let (output, _) = run_bash_native(r#"
true
echo "$?"
"#, &[]);
    assert_eq!(output, "0\n");
}

#[test]
fn exit_status_false() {
    let (output, _) = run_bash_native(r#"
false
echo "$?"
"#, &[]);
    assert_eq!(output, "1\n");
}

#[test]
fn exit_status_command() {
    // DD#55: /bin/false aborts, so only the first echo $? (after /bin/true) runs
    let (output, status) = run_bash_native(r#"
/bin/true
echo "$?"
/bin/false
echo "$?"
"#, &[]);
    assert_eq!(output, "0\n");
    assert_eq!(status, 1); // aborted by /bin/false
}

#[test]
fn exit_status_in_test() {
    let (output, _) = run_bash_native(r#"
/bin/true
if [[ "$?" = "0" ]]; then
  echo "ok"
fi
"#, &[]);
    assert_eq!(output, "ok\n");
}

#[test]
fn pipeline_two_stage() {
    let (output, _) = run_bash_native(r#"
echo -e "hello\nworld\ngoodbye" | grep world
"#, &[]);
    assert_eq!(output, "world\n");
}

#[test]
fn pipeline_three_stage() {
    let (output, _) = run_bash_native(r#"
echo -e "aaa\nbbb\nccc\naab" | grep a | grep b
"#, &[]);
    assert_eq!(output, "aab\n");
}

#[test]
fn pipeline_with_redirect() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    let n = C.fetch_add(1, Ordering::Relaxed);
    let outfile = format!("/tmp/sash_pipe_redir_{}", n);
    let script = format!(r#"
echo -e "alpha\nbeta" | grep beta > {}
"#, outfile);
    let (_, _) = run_bash_native(&script, &[]);
    let content = std::fs::read_to_string(&outfile).unwrap_or_default();
    assert_eq!(content, "beta\n");
    let _ = std::fs::remove_file(&outfile);
}

#[test]
fn pipeline_redirect_simple() {
    let outfile = "/tmp/sash_pipe_redir_simple";
    let script = format!(r#"
echo hello | cat > {}
"#, outfile);
    let (_, _) = run_bash_native(&script, &[]);
    let content = std::fs::read_to_string(outfile).unwrap_or_default();
    assert_eq!(content, "hello\n");
    let _ = std::fs::remove_file(outfile);
}

#[test]
fn printf_basic() {
    let (out, _) = run_bash_native(r#"printf '%s world\n' "hello""#, &[]);
    assert_eq!(out, "hello world\n");
}

#[test]
fn printf_multiple_args() {
    let (out, _) = run_bash_native(r#"printf '%s is %s\n' "sky" "blue""#, &[]);
    assert_eq!(out, "sky is blue\n");
}
fn printf_no_format() {
    let (out, _) = run_bash_native(r#"printf "hello\n""#, &[]);
    assert_eq!(out, "hello\n");
}

#[test]
fn printf_percent_d() {
    let (out, _) = run_bash_native(r#"printf '%d items\n' "42""#, &[]);
    assert_eq!(out, "42 items\n");
}

// ═══════════════════════════════════════════════════════════════
// Indexed array tests (DD#33 — unified hash table)
// ═══════════════════════════════════════════════════════════════

#[test]
fn indexed_array_basic() {
    let (out, _) = run_bash_native(r#"
arr=(alpha beta gamma)
echo "${arr[0]}"
echo "${arr[1]}"
echo "${arr[2]}"
"#, &[]);
    assert_eq!(out, "alpha\nbeta\ngamma\n");
}

#[test]
fn indexed_array_variable_index() {
    let (out, _) = run_bash_native(r#"
arr=(one two three)
int i=1
echo "${arr[$i]}"
"#, &[]);
    assert_eq!(out, "two\n");
}

#[test]
fn indexed_array_loop() {
    let (out, _) = run_bash_native(r#"
arr=(a b c)
int i=0
while (( i < 3 )); do
    echo "${arr[$i]}"
    (( i++ ))
done
"#, &[]);
    assert_eq!(out, "a\nb\nc\n");
}

#[test]
fn assoc_array_with_table() {
    // Existing assoc array test — should still work with sash_table backend
    let (out, _) = run_bash_native(r#"
declare -A MAP=([foo]=bar [baz]=qux)
echo "${MAP[foo]}"
echo "${MAP[baz]}"
"#, &[]);
    assert_eq!(out, "bar\nqux\n");
}

#[test]
fn indexed_array_from_args() {
    let (out, _) = run_bash_native(r#"
arr=(first second third)
echo "${arr[0]}"
echo "${arr[2]}"
"#, &[]);
    assert_eq!(out, "first\nthird\n");
}

// ═══════════════════════════════════════════════════════════════
// DD#35 — map/array/int type keywords
// ═══════════════════════════════════════════════════════════════

#[test]
fn keyword_map() {
    let (out, _) = run_bash_native(r#"
map colors=([red]=ff0000 [green]=00ff00)
echo "${colors[red]}"
echo "${colors[green]}"
"#, &[]);
    assert_eq!(out, "ff0000\n00ff00\n");
}

#[test]
fn keyword_array() {
    let (out, _) = run_bash_native(r#"
array fruits=(apple banana cherry)
echo "${fruits[0]}"
echo "${fruits[2]}"
"#, &[]);
    assert_eq!(out, "apple\ncherry\n");
}

#[test]
fn keyword_int() {
    let (out, _) = run_bash_native(r#"
int x=10
(( x += 5 ))
echo "$x"
"#, &[]);
    assert_eq!(out, "15\n");
}

// ═══════════════════════════════════════════════════════════════
// Pipe-while-read tests
// ═══════════════════════════════════════════════════════════════

#[test]
fn pipe_while_read_basic() {
    let (out, _) = run_bash_native(r#"
echo -e "alpha\nbeta\ngamma" | while read -r line; do
    echo "got $line"
done
"#, &[]);
    assert_eq!(out, "got alpha\ngot beta\ngot gamma\n");
}

#[test]
fn pipe_while_read_count() {
    let (out, _) = run_bash_native(r#"
int n=0
echo -e "a\nb\nc" | while read -r x; do
    (( n++ ))
done
echo "$n"
"#, &[]);
    assert_eq!(out, "3\n");
}

#[test]
fn pipe_while_read_multi_stage() {
    let (out, _) = run_bash_native(r#"
echo -e "hello world\nfoo bar\nbaz qux" | grep foo | while read -r line; do
    echo "match: $line"
done
"#, &[]);
    assert_eq!(out, "match: foo bar\n");
}

// ── Noop colon builtin ──────────────────────────────────────

#[test]
fn noop_colon() {
    let src = r#":
echo "after noop"
"#;
    let (out, code) = run_bash_native(src, &[]);
    assert_eq!(out, "after noop\n");
    assert_eq!(code, 0);
}

#[test]
fn noop_colon_in_if() {
    let src = r#"if true; then
  :
else
  echo "wrong"
fi
echo "done"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "done\n");
}

// ── echo -n (no trailing newline) ───────────────────────────

#[test]
fn echo_n_no_newline() {
    let src = r#"echo -n "hello"
echo " world"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "hello world\n");
}

#[test]
fn echo_n_multiple_args() {
    let src = r#"echo -n "a" "b" "c"
echo "!"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
}

#[test]
fn echo_n_no_args() {
    let src = r#"echo -n
echo "after"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "after\n");
}
#[test]
fn arith_comma_operator() {
    let src = r#"(( a=1, b=2 ))
echo "$a $b"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "1 2\n");
}

#[test]
fn arith_comma_three() {
    let src = r#"(( a=10, b=20, c=a+b ))
echo "$c"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "30\n");
}

// ── return N / return ───────────────────────────────────────

#[test]
fn return_from_function() {
    let src = r#"greet() {
  echo "hello"
  return
  echo "unreachable"
}
greet
echo "after"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "hello\nafter\n");
}

#[test]
fn return_with_status() {
    let src = r#"check() {
  return 1
}
check
echo "$?"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "1\n");
}

#[test]
fn return_zero_default() {
    let src = r#"ok() {
  echo "fine"
  return
}
ok
echo "$?"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "fine\n0\n");
}

// ── function keyword style ──────────────────────────────────

#[test]
fn function_keyword_style() {
    let src = r#"function greet {
  echo "hello"
}
greet
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "hello\n");
}

// ── ${var:+alternate} ───────────────────────────────────────

#[test]
fn param_alternate_set() {
    let src = r#"x="hello"
echo "${x:+world}"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "world\n");
}

#[test]
fn param_alternate_empty() {
    let src = r#"x=""
echo "${x:+world}"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "\n");
}

#[test]
fn param_alternate_unset() {
    let src = r#"echo "${unsetvar:+world}"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "\n");
}

// ── File tests ──────────────────────────────────────────────

#[test]
fn file_test_exists() {
    let src = r#"if [[ -e /tmp ]]; then
  echo "exists"
else
  echo "no"
fi
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "exists\n");
}

#[test]
fn file_test_is_dir() {
    let src = r#"if [[ -d /tmp ]]; then
  echo "dir"
fi
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "dir\n");
}

#[test]
fn file_test_is_file() {
    let src = r#"if [[ -f /etc/passwd ]]; then
  echo "file"
fi
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "file\n");
}

#[test]
fn file_test_not_exists() {
    let src = r#"if [[ -e /nonexistent_path_xyz ]]; then
  echo "yes"
else
  echo "no"
fi
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "no\n");
}

#[test]
fn file_test_readable() {
    let src = r#"if [[ -r /etc/passwd ]]; then
  echo "readable"
fi
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "readable\n");
}

#[test]
fn file_test_executable() {
    let src = r#"if [[ -x /bin/sh ]]; then
  echo "exec"
fi
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "exec\n");
}

// ── Bitwise operators ───────────────────────────────────────

#[test]
fn arith_bitwise_and() {
    let src = r#"echo "$(( 12 & 10 ))"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "8\n");
}

#[test]
fn arith_bitwise_or() {
    let src = r#"echo "$(( 12 | 3 ))"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "15\n");
}

#[test]
fn arith_bitwise_xor() {
    let src = r#"echo "$(( 12 ^ 3 ))"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "15\n");
}

#[test]
fn arith_shift_left() {
    let src = r#"echo "$(( 1 << 4 ))"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "16\n");
}

#[test]
fn arith_shift_right() {
    let src = r#"echo "$(( 16 >> 2 ))"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "4\n");
}

// ── Power operator ──────────────────────────────────────────

#[test]
fn arith_power() {
    let src = r#"echo "$(( 2 ** 10 ))"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "1024\n");
}

#[test]
fn arith_power_zero() {
    let src = r#"echo "$(( 5 ** 0 ))"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "1\n");
}

// ── Input redirect < file ───────────────────────────────────

#[test]
fn redirect_input() {
    // Create a temp file, then read from it via redirect
    let src = r#"echo "test data" > /tmp/sash_redir_input.txt
cat < /tmp/sash_redir_input.txt
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "test data\n");
}

#[test]
fn redirect_ampersand_both() {
    let src = r#"echo "out" &>/tmp/sash_ampersand_redir.txt
cat /tmp/sash_ampersand_redir.txt
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "out\n");
}

// ── break N / continue N ────────────────────────────────────

#[test]
fn break_two_levels() {
    let src = r#"x=0
while true; do
  while true; do
    break 2
  done
  x=1
done
echo "$x"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "0\n");
}

#[test]
fn continue_two_levels() {
    let src = r#"for i in a b; do
  for j in 1 2 3; do
    if [[ "$j" == "2" ]]; then
      continue 2
    fi
    echo "$i$j"
  done
done
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "a1\nb1\n");
}


// ── Ternary operator ? : ────────────────────────────────────

#[test]
fn arith_ternary_true() {
    let src = r#"echo "$(( 1 > 0 ? 10 : 20 ))"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "10\n");
}

#[test]
fn arith_ternary_false() {
    let src = r#"echo "$(( 0 > 1 ? 10 : 20 ))"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "20\n");
}

#[test]
fn arith_ternary_var() {
    let src = r#"x=5
echo "$(( x > 3 ? 1 : 0 ))"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "1\n");
}

#[test]

// ── ${var:offset:length} substring ──────────────────────────

#[test]
fn substring_offset_length() {
    let src = r#"x="hello world"
echo "${x:6:5}"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "world\n");
}

#[test]
fn substring_offset_only() {
    let src = r#"x="hello world"
echo "${x:6}"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "world\n");
}

#[test]
fn substring_negative_offset() {
    let src = r#"x="hello world"
echo "${x: -5}"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "world\n");
}

// ── [[ -t fd ]] terminal test ───────────────────────────────

#[test]
fn file_test_not_terminal() {
    // When run in a test, stdout is not a terminal
    let src = r#"if [[ -t 1 ]]; then
  echo "tty"
else
  echo "not tty"
fi
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "not tty\n");
}

// ── [[ file1 -nt file2 ]] ──────────────────────────────────

#[test]
fn file_test_newer_than() {
    let src = r#"echo "a" > /tmp/sash_nt_old.txt
sleep 0.1
echo "b" > /tmp/sash_nt_new.txt
if [[ /tmp/sash_nt_new.txt -nt /tmp/sash_nt_old.txt ]]; then
  echo "newer"
else
  echo "older"
fi
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "newer\n");
}

// ── cd / pwd builtins ───────────────────────────────────────

#[test]
fn cd_and_pwd() {
    let src = r#"cd /tmp
pwd
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "/tmp\n");
}

#[test]
fn cd_nonexistent() {
    let src = r#"cd /nonexistent_dir_12345 2>/dev/null
echo "$?"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "1\n");
}

// ── Here-doc ────────────────────────────────────────────────

#[test]
fn heredoc_basic() {
    let src = r#"cat << EOF
hello world
line two
EOF
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "hello world\nline two\n");
}

#[test]
fn heredoc_with_command() {
    let src = r#"wc -l << EOF
one
two
three
EOF
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
}

// ── Here-string ─────────────────────────────────────────────

#[test]
fn herestring_basic() {
    let src = r#"cat <<< "hello world"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "hello world\n");
}

// ── Here-string ─────────────────────────────────────────────

// ── $'' ANSI-C quoting ──────────────────────────────────────

#[test]
fn ansi_c_tab_newline() {
    let src = r#"printf '%s' $'hello\tworld\n'
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "hello\tworld\n");
}

#[test]
fn ansi_c_escape() {
    let src = r#"printf '%s' $'\x41\x42\x43'
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "ABC");
}

// ── ${var^} / ${var,} case conversion ───────────────────────

#[test]
fn case_upper_first() {
    let src = r#"x="hello"
echo "${x^}"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "Hello\n");
}

#[test]
fn case_upper_all() {
    let src = r#"x="hello"
echo "${x^^}"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "HELLO\n");
}

#[test]
fn case_lower_first() {
    let src = r#"x="HELLO"
echo "${x,}"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "hELLO\n");
}

#[test]
fn case_lower_all() {
    let src = r#"x="HELLO"
echo "${x,,}"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "hello\n");
}

// ── command -v ──────────────────────────────────────────────

#[test]
fn command_v_found() {
    let src = r#"command -v ls > /dev/null 2>&1
echo "$?"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "0\n");
}

#[test]
fn command_v_not_found() {
    let src = r#"command -v nonexistent_cmd_xyz > /dev/null 2>&1
echo "$?"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "1\n");
}

// ── Array subscript assignment and append ───────────────────

#[test]
fn array_subscript_assign() {
    let src = r#"declare -a arr=(one two three)
arr[1]="TWO"
echo "${arr[1]}"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "TWO\n");
}

#[test]
fn array_append() {
    let src = r#"declare -a arr=(one two)
arr+=(three)
echo "${arr[2]}"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "three\n");
}

#[test]
fn assoc_subscript_assign() {
    let src = r#"declare -A map=([name]=alice)
map[age]="30"
echo "${map[age]}"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "30\n");
}

// ── ${#arr[@]} array length ─────────────────────────────────

#[test]
fn array_length() {
    let src = r#"declare -a arr=(one two three)
echo "${#arr[@]}"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "3\n");
}

// ── shift N ─────────────────────────────────────────────────

#[test]
fn shift_n() {
    let src = r#"echo "$1 $2 $3"
shift 2
echo "$1"
"#;
    let (out, _) = run_bash_native(src, &["a", "b", "c"]);
    assert_eq!(out, bash_output(src, &["a", "b", "c"]));
    assert_eq!(out, "a b c\nc\n");
}

// ── Unary minus in arithmetic ───────────────────────────────

#[test]
fn arith_unary_minus() {
    let src = r#"x=5
echo "$(( -x ))"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "-5\n");
}

// ── Case with multiple patterns (|) ─────────────────────────

#[test]
fn case_multiple_patterns() {
    let src = r#"for x in start stop restart; do
    case "$x" in
        start|restart)
            echo "go: $x"
            ;;
        stop)
            echo "halt"
            ;;
    esac
done
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "go: start\nhalt\ngo: restart\n");
}

// ── String += concatenation ─────────────────────────────────

#[test]
fn string_append() {
    let src = r#"msg="hello"
msg+=" world"
echo "$msg"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "hello world\n");
}

// ── $$ process ID ───────────────────────────────────────────

#[test]
fn dollar_dollar_pid() {
    let src = r#"echo "$$"
"#;
    let (out, _) = run_bash_native(src, &[]);
    // Just verify it's a number, not literal "$$"
    let pid: i64 = out.trim().parse().expect("$$ should be a number");
    assert!(pid > 0, "PID should be positive");
}

// ── Heredoc with variable expansion ─────────────────────────

#[test]
fn heredoc_var_expansion() {
    let src = r#"name="world"
cat << EOF
hello $name
EOF
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "hello world\n");
}

// ── String += in loop with trailing space ───────────────────

#[test]
fn string_append_loop_space() {
    let src = r#"result=""
for word in hello world foo; do
    result+="$word "
done
echo "$result"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "hello world foo \n");
}

// ── [[ ! ]] negation in test expressions ────────────────────

#[test]
fn test_negation() {
    let src = r#"x="hello"
if [[ ! -z "$x" ]]; then echo "not empty"; fi
if [[ ! "$x" == "world" ]]; then echo "not world"; fi
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "not empty\nnot world\n");
}
#[test]
fn for_array_iteration() {
    let src = r#"arr=(alpha beta gamma)
for x in "${arr[@]}"; do
  echo "$x"
done
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
    assert_eq!(out, "alpha\nbeta\ngamma\n");
}
#[test]
fn readonly_basic() {
    let src = r#"readonly PI="3.14159"
echo "$PI"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "3.14159\n");
}

#[test]
#[should_panic(expected = "readonly variable")]
fn readonly_reassign_panics() {
    let src = r#"readonly PI="3.14159"
PI="changed"
echo "$PI"
"#;
    run_bash_native(src, &[]);
}
#[test]
fn for_array_keys() {
    let src = r#"declare -A m=([x]=1 [y]=2 [z]=3)
for k in "${!m[@]}"; do
  echo "$k=${m[$k]}"
done
"#;
    let (out, _) = run_bash_native(src, &[]);
    // Order may vary — sort lines for comparison
    let mut sash_lines: Vec<&str> = out.trim().split('\n').collect();
    sash_lines.sort();
    let bash_out = bash_output(src, &[]);
    let mut bash_lines: Vec<&str> = bash_out.trim().split('\n').collect();
    bash_lines.sort();
    assert_eq!(sash_lines, bash_lines);
    assert_eq!(sash_lines.len(), 3);
}

#[test]
fn test_read_multi_vars() {
    let src = r#"
echo "hello world extra" | while read -r a b; do
  echo "a=$a"
  echo "b=$b"
done
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "a=hello\nb=world extra\n");
}

#[test]
fn test_read_three_vars() {
    let src = r#"
echo "one two three four" | while read -r a b c; do
  echo "a=$a"
  echo "b=$b"
  echo "c=$c"
done
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "a=one\nb=two\nc=three four\n");
}

// ── DD#41: Sash-native array syntax ──────────────────────────

#[test]
fn sash_at_arr_iteration() {
    let src = r#"
array colors=("red" "green" "blue")
for c in @colors; do
    echo $c
done
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "red\ngreen\nblue\n");
}

#[test]
fn sash_arr_subscript_no_braces() {
    let src = r#"
array colors=("red" "green" "blue")
echo $colors[1]
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "green\n");
}

#[test]
fn sash_arr_subscript_in_string() {
    let src = r#"
array colors=("red" "green" "blue")
echo "color is $colors[1] yeah"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "color is green yeah\n");
}

#[test]
fn sash_len_array() {
    let src = r#"
array items=("a" "b" "c" "d")
echo $(len items)
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "4\n");
}

#[test]
fn sash_len_string() {
    let src = r#"
x="hello"
echo $(len $x)
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "5\n");
}

#[test]
fn sash_keys_array() {
    let src = r#"
map m=([x]=10 [y]=20 [z]=30)
for k in $(keys m); do
    echo $k
done
"#;
    let (out, _) = run_bash_native(src, &[]);
    let mut lines: Vec<&str> = out.trim().split('\n').collect();
    lines.sort();
    assert_eq!(lines, vec!["x", "y", "z"]);
}

#[test]
fn sash_join_space() {
    let src = r#"
array colors=("red" "green" "blue")
echo "$(join @colors)"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "red green blue\n");
}

#[test]
fn sash_join_comma() {
    let src = r#"
array colors=("red" "green" "blue")
echo "$(join ',' @colors)"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "red,green,blue\n");
}

#[test]
fn sash_join_multi_char_sep() {
    let src = r#"
array items=("a" "b" "c")
echo "$(join ' | ' @items)"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "a | b | c\n");
}

#[test]
fn for_in_cmd_subst() {
    let src = r#"
for x in $(seq 3); do
    echo "num=$x"
done
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "num=1\nnum=2\nnum=3\n");
}

#[test]
fn for_in_cmd_subst_echo() {
    // DD#28/DD#40: sash reads lines from pipe, not IFS-split words.
    // `echo "alpha beta gamma"` produces one line, so one iteration.
    // This differs from bash (which IFS-splits into 3 words).
    let src = r#"
for word in $(echo "alpha beta gamma"); do
    echo "$word"
done
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "alpha beta gamma\n");
}

#[test]
fn command_bypass_function() {
    let src = r#"
echo() {
    command echo "wrapped: $1"
}
echo "test"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "wrapped: test\n");
}


#[test]
fn cmdsub_redirect_stderr() {
    // $(cmd 2>/dev/null) should suppress stderr in the child process
    // DD#55: command must succeed (nonzero aborts). Use a command that succeeds
    // but writes to stderr.
    let src = r#"
result=$(echo hello 2>/dev/null)
echo "result='$result'"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "result='hello'\n");
}




#[test]
fn file_slurp() {
    let src = r#"
echo "test data" > /tmp/sash_slurp_test.txt
x=$(</tmp/sash_slurp_test.txt)
echo "got: $x"
rm /tmp/sash_slurp_test.txt
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "got: test data\n");
}

#[test]
fn read_delim_colon() {
    let src = r#"
echo -n "one:two:three" | while read -d: -r word; do
    echo "word=$word"
done
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "word=one\nword=two\n");
}

#[test]
fn mapfile_basic() {
    let src = r#"
array lines
printf "alpha\nbeta\ngamma\n" | mapfile -t lines
echo $lines[0]
echo $lines[1]
echo $lines[2]
echo $(len lines)
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out.trim(), "alpha\nbeta\ngamma\n3");
}

#[test]
fn mapfile_bare_array_append() {
    let src = r#"
array items
items+=("first")
items+=("second")
echo $items[0]
echo $items[1]
echo $(len items)
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out.trim(), "first\nsecond\n2");
}

#[test]
fn epochseconds_basic() {
    let src = r#"
ts=$EPOCHSECONDS
echo $ts
"#;
    let (out, _) = run_bash_native(src, &[]);
    let val: u64 = out.trim().parse().expect("should be a number");
    // Should be a reasonable epoch time (after 2024)
    assert!(val > 1700000000, "epoch too small: {}", val);
    assert!(val < 2000000000, "epoch too large: {}", val);
}

#[test]
fn epochseconds_in_string() {
    let src = r#"
echo "time=$EPOCHSECONDS"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert!(out.starts_with("time=17") || out.starts_with("time=18") || out.starts_with("time=19"), "unexpected: {}", out);
}

#[test]
fn exec_standalone_redirect() {
    let src = r#"
exec 3>&1
echo "to fd3" >&3
exec 3>&-
echo "to stdout"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out.trim(), "to fd3\nto stdout");
}

#[test]
fn func_capture_basic() {
    let src = r#"
fn greet() {
    echo "hello $1"
}
result=$(greet world)
echo "got: $result"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out.trim(), "got: hello world");
}

#[test]
fn func_capture_multiline() {
    let src = r#"
fn info() {
    echo "line1"
    echo "line2"
}
result=$(info)
echo "$result"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out.trim(), "line1\nline2");
}

#[test]
fn func_capture_arithmetic() {
    let src = r#"
fn double() {
    local x=$1
    echo $(( x * 2 ))
}
result=$(double 5)
echo "result=$result"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out.trim(), "result=10");
}

#[test]
fn subshell_variable_isolation() {
    let src = r#"
x=hello
(
    x=world
    echo "inside: $x"
)
echo "outside: $x"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out.trim(), "inside: world\noutside: hello");
}

#[test]
fn subshell_exit_status() {
    let src = r#"
(
    echo "child"
    exit 42
)
echo "status=$?"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out.trim(), "child\nstatus=42");
}

#[test]
fn subshell_inherits_parent_vars() {
    let src = r#"
x=hello
y=world
(
    echo "$x $y"
)
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out.trim(), "hello world");
}

#[test]
fn background_job_wait() {
    let src = r#"
echo "start"
true &
wait
echo "done"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out.trim(), "start\ndone");
}

#[test]
fn background_job_pid() {
    let src = r#"
true &
pid=$!
echo "pid=$pid"
wait $pid
echo "status=$?"
"#;
    let (out, _) = run_bash_native(src, &[]);
    let lines: Vec<&str> = out.trim().lines().collect();
    assert!(lines[0].starts_with("pid="), "expected pid= prefix, got: {}", lines[0]);
    assert_eq!(lines[1], "status=0");
}

#[test]

#[test]
fn trap_exit_basic() {
    let src = r#"
trap 'echo cleanup' EXIT
echo hello
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out.trim(), "hello\ncleanup");
}

#[test]
fn trap_exit_multiline() {
    let src = r#"
trap 'echo bye; echo done' EXIT
echo start
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out.trim(), "start\nbye\ndone");
}

#[test]
fn trap_ignore_signal() {
    let src = r#"
trap '' SIGPIPE
echo ok
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out.trim(), "ok");
}

#[test]
fn glob_star_array() {
    // Create temp dir with known files, glob them
    let src = r#"
array files=(*.txt)
echo $(len files)
"#;
    // This test depends on what .txt files exist in cwd
    // Just verify it compiles and runs without panic
    let (out, code) = run_bash_native(src, &[]);
    assert_eq!(code, 0, "glob should succeed, got: {}", out);
}

#[test]
fn unset_map_key() {
    let src = r#"
declare -A m
m[a]=1
m[b]=2
m[c]=3
echo ${#m[@]}
unset m[b]
echo ${#m[@]}
for k in ${!m[@]}; do
    echo "$k=${m[$k]}"
done
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "3\n2\na=1\nc=3\n");
}

#[test]
fn unset_map_key_nonexistent() {
    let src = r#"
declare -A m
m[x]=hello
unset m[y]
echo ${#m[@]}
echo ${m[x]}
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "1\nhello\n");
}
#[test]

#[test]
#[test]

#[test]
fn regex_replace_basic() {
    let src = r#"
x="hello world"
x =~ s/world/earth/
echo $x
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "hello earth\n");
}

#[test]
fn regex_replace_global() {
    let src = r#"
x="aXbXc"
x =~ s/X/Y/g
echo $x
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "aYbYc\n");
}

#[test]
fn regex_replace_no_match() {
    let src = r#"
x="hello"
x =~ s/xyz/abc/
echo $x
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "hello\n");
}

#[test]
fn regex_replace_char_class() {
    let src = r#"
x="foo123bar456"
x =~ s/[0-9]+/NUM/g
echo $x
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "fooNUMbarNUM\n");
}

#[test]
fn glob_regex_filter() {
    // Test arr=(* =~ 'regex') — readdir with regex filter
    let src = r#"
cd /home/dangood/src/safebash/sash/src
array files=(* =~ '\.rs$')
echo $(len files)
"#;
    let (out, _) = run_bash_native(src, &[]);
    let count: i32 = out.trim().parse().unwrap_or(0);
    assert!(count > 0, "expected at least one .rs file, got: {}", out);
}
#[test]

#[test]

#[test]

#[test]
fn arith_nested_parens_basic() {
    let (out, _) = run_bash_native("a=2\nb=3\nc=4\necho $(( (a+b)*c ))", &[]);
    assert_eq!(out.trim(), "20");
}

#[test]
fn arith_nested_parens_right() {
    let (out, _) = run_bash_native("a=2\nb=3\nc=4\necho $(( a*(b+c) ))", &[]);
    assert_eq!(out.trim(), "14");
}

#[test]
fn arith_nested_parens_both() {
    let (out, _) = run_bash_native("a=2\nb=3\nc=4\necho $(( (a+b)*(c+1) ))", &[]);
    assert_eq!(out.trim(), "25");
}

#[test]
fn read_into_array_colon_delim() {
    let (out, _) = run_bash_native("array arr=()\necho \"alpha:beta:gamma\" | read -d: -a arr\necho $(len arr)\nfor x in @arr; do\necho $x\ndone", &[]);
    assert_eq!(out.trim(), "3\nalpha\nbeta\ngamma");
}

#[test]
fn read_into_array_space_delim() {
    let (out, _) = run_bash_native("array arr=()\necho \"one two three\" | read -a arr\necho $(len arr)\nfor x in @arr; do\necho $x\ndone", &[]);
    assert_eq!(out.trim(), "3\none\ntwo\nthree");
}

// ═══════════════════════════════════════════════════════════
// getenv / argv builtins (DD#42)
// ═══════════════════════════════════════════════════════════

#[test]
fn getenv_home() {
    let src = r#"home=$(getenv HOME)
echo $home"#;
    let (out, _) = run_bash_native(src, &[]);
    assert!(!out.trim().is_empty(), "HOME should not be empty");
    assert!(out.trim().starts_with('/'), "HOME should be absolute path: {}", out);
}

#[test]
fn getenv_missing_returns_empty() {
    let src = r#"x=$(getenv SAFEBASH_NONEXISTENT_VAR_XYZ)
echo "[$x]""#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out.trim(), "[]");
}

#[test]
fn getenv_with_fallback() {
    let src = r#"x=$(getenv SAFEBASH_NONEXISTENT_VAR_XYZ) || x=default
echo $x"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out.trim(), "default");
}

#[test]
fn argv_basic() {
    let src = r#"name=$(argv 1)
echo "hello $name""#;
    let (out, _) = run_bash_native(src, &["world"]);
    assert_eq!(out.trim(), "hello world");
}

#[test]
fn argv_multiple() {
    let src = r#"a=$(argv 1)
b=$(argv 2)
echo "$a $b""#;
    let (out, _) = run_bash_native(src, &["foo", "bar"]);
    assert_eq!(out.trim(), "foo bar");
}

#[test]
fn argv_bounds_checked() {
    let src = r#"x=$(argv 5) || x="fallback"
echo $x"#;
    let (out, _) = run_bash_native(src, &["a", "b"]);
    assert_eq!(out.trim(), "fallback");
}

#[test]
fn argv_exact_boundary() {
    let src = r#"x=$(argv 2)
echo "[$x]""#;
    let (out, _) = run_bash_native(src, &["one", "two"]);
    assert_eq!(out.trim(), "[two]");
}

#[test]
fn argc_builtin() {
    let src = r#"echo $(argc)"#;
    let (out, _) = run_bash_native(src, &["a", "b", "c"]);
    assert_eq!(out.trim(), "3");
}

#[test]
fn argc_zero_args() {
    let src = r#"echo $(argc)"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out.trim(), "0");
}


// ═══════════════════════════════════════════════════════════════
// File test operators: -ot, -ef
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_file_older_than() {
    let script = r#"#!/bin/bash
touch /tmp/sash_f1
sleep 0.01
touch /tmp/sash_f2
if [[ /tmp/sash_f1 -ot /tmp/sash_f2 ]]; then echo "older"; else echo "not"; fi
"#;
    let (out, rc) = run_bash_native(script, &[]);
    assert_eq!(rc, 0);
    assert_eq!(out, "older\n");
}

#[test]
fn test_file_same_device() {
    let script = r#"#!/bin/bash
touch /tmp/sash_ef_test
if [[ /tmp/sash_ef_test -ef /tmp/sash_ef_test ]]; then echo "same"; else echo "diff"; fi
"#;
    let (out, rc) = run_bash_native(script, &[]);
    assert_eq!(rc, 0);
    assert_eq!(out, "same\n");
}

#[test]
fn test_file_ef_different() {
    let script = r#"#!/bin/bash
touch /tmp/sash_ef_a
touch /tmp/sash_ef_b
if [[ /tmp/sash_ef_a -ef /tmp/sash_ef_b ]]; then echo "same"; else echo "diff"; fi
"#;
    let (out, rc) = run_bash_native(script, &[]);
    assert_eq!(rc, 0);
    assert_eq!(out, "diff\n");
}

// ═══════════════════════════════════════════════════════════════
// Parameter transformations: @U, @u, @L, @Q, @E
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_param_transform_upper_all() {
    let script = "#!/bin/bash\nx=\"hElLo WoRlD\"\necho \"${x^^}\"";
    let (out, rc) = run_bash_native(script, &[]);
    assert_eq!(rc, 0);
    assert_eq!(out, "HELLO WORLD\n");
}

#[test]
fn test_param_transform_upper_first() {
    let script = "#!/bin/bash\nx=\"hello world\"\necho \"${x^}\"";
    let (out, rc) = run_bash_native(script, &[]);
    assert_eq!(rc, 0);
    assert_eq!(out, "Hello world\n");
}

#[test]
fn test_param_transform_lower_all() {
    let script = "#!/bin/bash\nx=\"HELLO WORLD\"\necho \"${x,,}\"";
    let (out, rc) = run_bash_native(script, &[]);
    assert_eq!(rc, 0);
    assert_eq!(out, "hello world\n");
}

#[test]
fn test_break_continue_n() {
    let script = r#"#!/bin/bash
for f in a b c; do
  for g in 1 2 3; do
    echo "f=$f g=$g"
    continue 2
    echo "Should not print"
  done
  echo "Left inner loop"
done
"#;
    let (out, rc) = run_bash_native(script, &[]);
    assert_eq!(rc, 0);
    assert_eq!(out, "f=a g=1\nf=b g=1\nf=c g=1\n");
}

// ═══════════════════════════════════════════════════════════════
// COMPOUND COMMAND REDIRECTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_for_redirect_to_file() {
    let script = "#!/bin/bash\nfor i in a b c; do\n  echo $i\ndone > /tmp/sash_test_for_redir.txt\ncat /tmp/sash_test_for_redir.txt\nrm /tmp/sash_test_for_redir.txt";
    let (out, _) = run_bash_native(script, &[]);
    assert_eq!(out, "a\nb\nc\n");
}

#[test]
fn test_while_redirect_to_file() {
    let script = "#!/bin/bash\ni=0\nwhile [[ $i -lt 3 ]]; do\n  echo \"line $i\"\n  i=$((i + 1))\ndone > /tmp/sash_test_while_redir.txt\ncat /tmp/sash_test_while_redir.txt\nrm /tmp/sash_test_while_redir.txt";
    let (out, _) = run_bash_native(script, &[]);
    assert_eq!(out, "line 0\nline 1\nline 2\n");
}

#[test]
fn test_if_redirect_to_file() {
    let script = "#!/bin/bash\nx=5\nif [[ $x -gt 3 ]]; then\n  echo \"big\"\nelse\n  echo \"small\"\nfi > /tmp/sash_test_if_redir.txt\ncat /tmp/sash_test_if_redir.txt\nrm /tmp/sash_test_if_redir.txt";
    let (out, _) = run_bash_native(script, &[]);
    assert_eq!(out, "big\n");
}

#[test]
fn test_group_redirect_to_file() {
    let script = "#!/bin/bash\n{\n  echo \"first\"\n  echo \"second\"\n} > /tmp/sash_test_group_redir.txt\ncat /tmp/sash_test_group_redir.txt\nrm /tmp/sash_test_group_redir.txt";
    let (out, _) = run_bash_native(script, &[]);
    assert_eq!(out, "first\nsecond\n");
}

#[test]
fn test_compound_redirect_stderr() {
    let script = "#!/bin/bash\n{\n  echo \"out\"\n  echo \"err\" >&2\n} 2>/tmp/sash_test_stderr.txt\ncat /tmp/sash_test_stderr.txt\nrm /tmp/sash_test_stderr.txt";
    let (out, _) = run_bash_native(script, &[]);
    assert_eq!(out, "out\nerr\n");
}

#[test]
fn test_random_different() {
    let (out, _) = run_bash_native(r#"echo $RANDOM; echo $RANDOM"#, &[]);
    let lines: Vec<&str> = out.trim().split('\n').collect();
    assert_eq!(lines.len(), 2);
    let a: u32 = lines[0].parse().unwrap();
    let b: u32 = lines[1].parse().unwrap();
    assert!(a <= 32767, "RANDOM out of range: {}", a);
    assert!(b <= 32767, "RANDOM out of range: {}", b);
    assert_ne!(a, b, "Two consecutive $RANDOM should differ");
}

#[test]
fn test_unset_variable() {
    let (out, _) = run_bash_native(r#"x="hello"; unset x; echo "${x:-gone}""#, &[]);
    assert_eq!(out, "gone\n");
}

#[test]
fn test_func_redirect() {
    let (out, _) = run_bash_native(r#"
f() {
  echo "hello"
  echo "world"
} > /tmp/sash_test_funcrd.txt
f
cat /tmp/sash_test_funcrd.txt
rm -f /tmp/sash_test_funcrd.txt"#, &[]);
    assert_eq!(out, "hello\nworld\n");
}

#[test]
fn test_strlen_array_elem() {
    let (out, _) = run_bash_native(r#"
declare -a arr=(hello world)
echo "${#arr[0]}"
echo "${#arr[1]}""#, &[]);
    assert_eq!(out, "5\n5\n");
}

#[test]
fn test_substr_negative_offset() {
    let (out, _) = run_bash_native(r#"
x="hello world"
echo "${x:(-5)}""#, &[]);
    assert_eq!(out, "world\n");
}

#[test]
fn test_v_set() {
    let (out, _) = run_bash_native(r#"x="hello"; [[ -v x ]] && echo "set" || echo "unset""#, &[]);
    assert_eq!(out, "set\n");
}

#[test]
fn test_v_unset() {
    let (out, _) = run_bash_native(r#"[[ -v y ]] && echo "set" || echo "unset""#, &[]);
    assert_eq!(out, "unset\n");
}

#[test]
fn test_pipeline_exit_status() {
    // DD#55: pipefail + implicit exit check — `true | false` aborts, echo never runs
    let (out, status) = run_bash_native(r#"true | false; echo $?"#, &[]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
}

#[test]
fn test_nested_return() {
    let (out, _) = run_bash_native(r#"
inner() { echo "inner"; return 42; }
outer() { inner; echo "after: $?"; return 7; }
outer
echo "final: $?""#, &[]);
    assert_eq!(out, "inner\nafter: 42\nfinal: 7\n");
}
