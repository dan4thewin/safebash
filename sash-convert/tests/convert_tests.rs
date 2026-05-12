use std::process::Command;
use std::io::Write;

/// Run sash-convert on a script, return stdout.
fn convert(name: &str, script: &str) -> String {
    let src = format!("/tmp/sash_convert_test_{}.sh", name);
    std::fs::write(&src, script).expect("write test script");
    let output = Command::new(env!("CARGO_BIN_EXE_sash-convert"))
        .arg(&src)
        .output()
        .expect("run sash-convert");
    let _ = std::fs::remove_file(&src);
    assert!(output.status.success(),
        "sash-convert failed for {}: {}",
        name, String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).expect("utf8")
}

/// Run sash-convert --annotate on a script, return stdout.
fn convert_annotated(name: &str, script: &str) -> String {
    let src = format!("/tmp/sash_convert_test_{}.sh", name);
    std::fs::write(&src, script).expect("write test script");
    let output = Command::new(env!("CARGO_BIN_EXE_sash-convert"))
        .args(["--annotate", &src])
        .output()
        .expect("run sash-convert");
    let _ = std::fs::remove_file(&src);
    assert!(output.status.success(),
        "sash-convert --annotate failed for {}: {}",
        name, String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).expect("utf8")
}

// ═══════════════════════════════════════════════════════════════
// COERCION TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn coerce_declare_i_to_int() {
    let out = convert("coerce_int", "#!/bin/bash\ndeclare -i x=5\necho $x");
    assert!(out.contains("int x=5"), "expected 'int x=5', got:\n{}", out);
    assert!(!out.contains("declare"), "should not contain 'declare'");
}

#[test]
fn coerce_declare_a_to_array() {
    let out = convert("coerce_arr", "#!/bin/bash\ndeclare -a arr=(1 2 3)");
    assert!(out.contains("array arr=(1 2 3)"), "expected 'array', got:\n{}", out);
}

#[test]
fn coerce_declare_A_to_map() {
    let out = convert("coerce_map", "#!/bin/bash\ndeclare -A m=([a]=1)");
    assert!(out.contains("map m=([a]=1)"), "expected 'map', got:\n{}", out);
}

#[test]
fn coerce_readonly_to_const() {
    let out = convert("coerce_const", "#!/bin/bash\nreadonly msg=hello");
    assert!(out.contains("const msg=hello"), "expected 'const', got:\n{}", out);
}

#[test]
fn coerce_declare_ri_to_const_int() {
    let out = convert("coerce_ri", "#!/bin/bash\ndeclare -ri x=5");
    assert!(out.contains("const int x=5"), "expected 'const int', got:\n{}", out);
}

#[test]
fn coerce_let_to_arith() {
    let out = convert("coerce_let", "#!/bin/bash\nlet \"x=3+4\"");
    assert!(out.contains("(( x=3+4 ))"), "expected '(( x=3+4 ))', got:\n{}", out);
}

#[test]
fn coerce_local_i_to_local_int() {
    let out = convert("coerce_local_int", "#!/bin/bash\nf() { local -i x=5; echo $x; }\nf");
    assert!(out.contains("local int x=5"), "expected 'local int', got:\n{}", out);
}

#[test]
fn coerce_typeset_stripped() {
    let out = convert("coerce_typeset", "#!/bin/bash\ntypeset x=hello");
    assert!(!out.contains("typeset"), "should not contain 'typeset'");
}

#[test]
fn coerce_native_unchanged() {
    let out = convert("coerce_native", "#!/bin/bash\nint x=5\nconst msg=hello\narray arr=(1 2 3)");
    assert!(out.contains("int x=5"), "int should pass through");
    assert!(out.contains("const msg=hello"), "const should pass through");
    assert!(out.contains("array arr=(1 2 3)"), "array should pass through");
}

// ═══════════════════════════════════════════════════════════════
// ANNOTATION TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn annotate_declare_i() {
    let out = convert_annotated("ann_int", "#!/bin/bash\ndeclare -i x=5");
    assert!(out.contains("# was: declare -i x=5"), "expected annotation, got:\n{}", out);
}

#[test]
fn annotate_readonly() {
    let out = convert_annotated("ann_const", "#!/bin/bash\nreadonly msg=hello");
    assert!(out.contains("# was: readonly msg=hello"), "expected annotation, got:\n{}", out);
}

#[test]
fn annotate_let() {
    let out = convert_annotated("ann_let", "#!/bin/bash\nlet \"x=1\"");
    assert!(out.contains("# was: let \"x=1\""), "expected annotation, got:\n{}", out);
}

#[test]
fn annotate_native_no_annotation() {
    let out = convert_annotated("ann_native", "#!/bin/bash\nint x=5\nconst msg=hello");
    assert!(!out.contains("# was:"), "native syntax should have no annotations, got:\n{}", out);
}
