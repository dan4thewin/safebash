use std::process::Command;

fn expand(input: &str) -> String {
    let id = std::thread::current().name().unwrap_or("t").replace(" ", "_");
    let path = format!("/tmp/sash_expand_test_{}.conf", id);
    std::fs::write(&path, input).expect("write test conf");
    let output = Command::new(env!("CARGO_BIN_EXE_sash-expand"))
        .arg(&path)
        .output()
        .expect("run sash-expand");
    let _ = std::fs::remove_file(&path);
    assert!(output.status.success(),
        "sash-expand failed: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).expect("utf8")
}

fn expand_fails(input: &str) -> String {
    let id = std::thread::current().name().unwrap_or("t").replace(" ", "_");
    let path = format!("/tmp/sash_expand_test_{}.conf", id);
    std::fs::write(&path, input).expect("write test conf");
    let output = Command::new(env!("CARGO_BIN_EXE_sash-expand"))
        .arg(&path)
        .output()
        .expect("run sash-expand");
    let _ = std::fs::remove_file(&path);
    assert!(!output.status.success(), "expected sash-expand to fail");
    String::from_utf8(output.stderr).expect("utf8")
}

// --- Expansion ---

#[test]
fn literal() {
    assert_eq!(expand("X=hello\n"), "X=hello\n");
}

#[test]
fn simple_expansion() {
    assert_eq!(expand("A=world\nB=hello ${A}\n"), "A=world\nB=hello world\n");
}

#[test]
fn expansion_braces() {
    assert_eq!(expand("A=test\nB=${A}_value\n"), "A=test\nB=test_value\n");
}

#[test]
fn default_value() {
    assert_eq!(expand("X=${MISSING:-fallback}\n"), "X=fallback\n");
}

#[test]
fn alternate_value() {
    assert_eq!(expand("A=yes\nB=${A:+set}\n"), "A=yes\nB=set\n");
}

#[test]
fn alternate_unset() {
    assert_eq!(expand("B=${MISSING:+set}\n"), "B=\n");
}

#[test]
fn strip_suffix() {
    assert_eq!(expand("A=hello\nB=${A%%lo}\n"), "A=hello\nB=hel\n");
}

#[test]
fn strip_prefix() {
    assert_eq!(expand("A=hello\nB=${A#hel}\n"), "A=hello\nB=lo\n");
}

#[test]
fn chained() {
    assert_eq!(expand("A=base\nB=${A}_ext\nC=${B}_final\n"),
        "A=base\nB=base_ext\nC=base_ext_final\n");
}

#[test]
fn blank_lines_and_comments() {
    let out = expand("A=1\n\n# comment\nB=2\n");
    assert!(out.contains("A=1\n"));
    assert!(out.contains("B=2\n"));
    assert!(!out.contains("#"));
}

#[test]
fn strip_suffix_longest() {
    assert_eq!(expand("A=foo.tar.gz\nB=${A%%.*}\n"), "A=foo.tar.gz\nB=foo\n");
}

#[test]
fn strip_suffix_shortest() {
    assert_eq!(expand("A=foo.tar.gz\nB=${A%.*}\n"), "A=foo.tar.gz\nB=foo.tar\n");
}

#[test]
fn strip_prefix_longest() {
    assert_eq!(expand("A=/home/user/file\nB=${A##*/}\n"), "A=/home/user/file\nB=file\n");
}

#[test]
fn strip_prefix_shortest() {
    assert_eq!(expand("A=/home/user/file\nB=${A#*/}\n"), "A=/home/user/file\nB=home/user/file\n");
}

#[test]
fn default_when_set() {
    // default not used when var is set
    assert_eq!(expand("A=real\nB=${A:-fallback}\n"), "A=real\nB=real\n");
}

#[test]
fn alternate_when_unset() {
    assert_eq!(expand("B=${MISSING:+alt}\n"), "B=\n");
}

// --- Rejections ---

#[test]
fn reject_command_substitution() {
    let err = expand_fails("X=$(cmd)\n");
    assert!(err.contains("command substitution"), "got: {}", err);
}

#[test]
fn reject_backtick() {
    let err = expand_fails("X=`cmd`\n");
    assert!(err.contains("backtick") || err.contains("command"), "got: {}", err);
}

#[test]
fn reject_indirect() {
    let err = expand_fails("A=hello\nB=${!A}\n");
    assert!(err.contains("indirect") || err.contains("!"), "got: {}", err);
}

#[test]
fn reject_arithmetic() {
    let err = expand_fails("X=$((1+2))\n");
    assert!(err.contains("arithmetic") || err.contains("command"), "got: {}", err);
}
