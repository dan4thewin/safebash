use std::process::Command;

/// Run sash-convert on a script, return stdout.
fn convert(name: &str, script: &str) -> String {
    let src = format!("/tmp/sash_convert_test_{}.sh", name);
    std::fs::write(&src, script).expect("write test script");
    let output = Command::new(env!("CARGO_BIN_EXE_sash-convert"))
        .arg(&src)
        .output()
        .expect("run sash-convert");
    let _ = std::fs::remove_file(&src);
    assert!(
        output.status.success(),
        "sash-convert failed for {}: {}",
        name,
        String::from_utf8_lossy(&output.stderr)
    );
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
    assert!(
        output.status.success(),
        "sash-convert --annotate failed for {}: {}",
        name,
        String::from_utf8_lossy(&output.stderr)
    );
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
    assert!(
        out.contains("array arr=(1 2 3)"),
        "expected 'array', got:\n{}",
        out
    );
}

#[test]
fn coerce_declare_a_to_map() {
    let out = convert("coerce_map", "#!/bin/bash\ndeclare -A m=([a]=1)");
    assert!(
        out.contains("map m=([a]=1)"),
        "expected 'map', got:\n{}",
        out
    );
}

#[test]
fn coerce_readonly_to_const() {
    let out = convert("coerce_const", "#!/bin/bash\nreadonly msg=hello");
    assert!(
        out.contains("const msg=hello"),
        "expected 'const', got:\n{}",
        out
    );
}

#[test]
fn coerce_declare_ri_to_const_int() {
    let out = convert("coerce_ri", "#!/bin/bash\ndeclare -ri x=5");
    assert!(
        out.contains("const int x=5"),
        "expected 'const int', got:\n{}",
        out
    );
}

#[test]
fn coerce_let_to_arith() {
    let out = convert("coerce_let", "#!/bin/bash\nlet \"x=3+4\"");
    assert!(
        out.contains("(( x=3+4 ))"),
        "expected '(( x=3+4 ))', got:\n{}",
        out
    );
}

#[test]
fn coerce_local_i_to_int() {
    // DD#86: `local` is removed — `local -i x=5` converts to `int x=5`
    // (the keyword is dropped; scope is structural / auto-local).
    let out = convert(
        "coerce_local_int",
        "#!/bin/bash\nf() { local -i x=5; echo $x; }\nf",
    );
    assert!(
        out.contains("int x=5") && !out.contains("local"),
        "expected 'int x=5' without 'local', got:\n{}",
        out
    );
}

#[test]
fn coerce_typeset_stripped() {
    let out = convert("coerce_typeset", "#!/bin/bash\ntypeset x=hello");
    assert!(!out.contains("typeset"), "should not contain 'typeset'");
}

#[test]
fn coerce_native_unchanged() {
    let out = convert(
        "coerce_native",
        "#!/bin/bash\nint x=5\nconst msg=hello\narray arr=(1 2 3)",
    );
    assert!(out.contains("int x=5"), "int should pass through");
    assert!(out.contains("const msg=hello"), "const should pass through");
    assert!(
        out.contains("array arr=(1 2 3)"),
        "array should pass through"
    );
}

// ═══════════════════════════════════════════════════════════════
// ANNOTATION TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn annotate_declare_i() {
    let out = convert_annotated("ann_int", "#!/bin/bash\ndeclare -i x=5");
    assert!(
        out.contains("# was: declare -i x=5"),
        "expected annotation, got:\n{}",
        out
    );
}

#[test]
fn annotate_readonly() {
    let out = convert_annotated("ann_const", "#!/bin/bash\nreadonly msg=hello");
    assert!(
        out.contains("# was: readonly msg=hello"),
        "expected annotation, got:\n{}",
        out
    );
}

#[test]
fn annotate_let() {
    let out = convert_annotated("ann_let", "#!/bin/bash\nlet \"x=1\"");
    assert!(
        out.contains("# was: let \"x=1\""),
        "expected annotation, got:\n{}",
        out
    );
}

#[test]
fn annotate_native_no_annotation() {
    let out = convert_annotated("ann_native", "#!/bin/bash\nint x=5\nconst msg=hello");
    assert!(
        !out.contains("# was:"),
        "native syntax should have no annotations, got:\n{}",
        out
    );
}

// ═══════════════════════════════════════════════════════════════
// MANUAL ANNOTATIONS
// ═══════════════════════════════════════════════════════════════

#[test]
fn manual_case_removed() {
    // DD#66: `case`/`esac` are removed — the converter flags them with a
    // MANUAL note directing to the `[[ ]]` dispatch form.
    let out = convert(
        "manual_case",
        "#!/bin/bash\ncase $x in a) echo a ;; *) echo other ;; esac\n",
    );
    assert!(
        out.contains("# MANUAL: case removed (DD#66)") && out.contains("dispatch form"),
        "expected case-removal MANUAL annotation, got:\n{}",
        out
    );
}

// ═══════════════════════════════════════════════════════════════
// DD#53 TILDE EXPANSION
// ═══════════════════════════════════════════════════════════════

#[test]
fn tilde_bare_home() {
    // `~` → hoisted $(getenv HOME) + attest, referenced as $home.
    let out = convert("tilde_bare", "#!/bin/bash\necho ~\n");
    assert!(
        out.contains("home=$(getenv HOME)")
            && out.contains("attest home")
            && out.contains("echo $home"),
        "expected ~ → $home hoist, got:\n{}",
        out
    );
}

#[test]
fn tilde_home_path() {
    // `~/foo` → `$home/foo` (leading ~ rewritten, tail preserved).
    let out = convert("tilde_path", "#!/bin/bash\necho ~/foo\n");
    assert!(
        out.contains("echo $home/foo") && out.contains("home=$(getenv HOME)"),
        "expected ~/foo → $home/foo, got:\n{}",
        out
    );
}

#[test]
fn tilde_user_is_manual() {
    // `~user` needs a passwd lookup sash does not do — flag it, leave it.
    let out = convert("tilde_user", "#!/bin/bash\necho ~root\n");
    assert!(
        out.contains("# MANUAL:") && out.contains("tilde") && out.contains("echo ~root"),
        "expected ~root MANUAL annotation, got:\n{}",
        out
    );
}

#[test]
fn tilde_quoted_is_literal() {
    // A quoted tilde is a literal — untouched, no getenv hoist.
    let out = convert("tilde_quoted", "#!/bin/bash\necho '~'\n");
    assert!(
        out.contains("echo '~'") && !out.contains("getenv HOME"),
        "expected quoted '~' untouched, got:\n{}",
        out
    );
}

// ═══════════════════════════════════════════════════════════════
// DD#99 READ FAMILY
// ═══════════════════════════════════════════════════════════════

#[test]
fn read_single_target_becomes_readline() {
    // DD#99 retires every read flag; `read [-r] VAR` is the one spelling
    // with an exact mechanical rewrite.
    let out = convert("read_single", "#!/bin/bash\nread -r line\n");
    assert!(
        out.contains("readline line") && !out.contains("read -r line"),
        "expected readline rewrite, got:\n{}",
        out
    );
}

#[test]
fn read_ifs_prefix_is_exact_and_drops_the_assignment() {
    // `IFS= read -r x` is what `readline` matches byte-for-byte, so no
    // CHECK — and the IFS= prefix goes away with the command it modified.
    let out = convert("read_ifs", "#!/bin/bash\nIFS= read -r line\n");
    assert!(
        out.contains("readline line") && !out.contains("IFS=") && !out.contains("# CHECK:"),
        "expected exact rewrite with no CHECK, got:\n{}",
        out
    );
}

#[test]
fn read_plain_gets_a_trim_check() {
    // Without `IFS=`, bash strips leading/trailing IFS whitespace and
    // `readline` does not. Convert anyway, but say so.
    let out = convert("read_trim", "#!/bin/bash\nread -r line\n");
    assert!(
        out.contains("# CHECK:") && out.contains("IFS whitespace"),
        "expected trim CHECK, got:\n{}",
        out
    );
}

#[test]
fn read_array_flag_is_manual_not_a_guess() {
    // bash `read -a` splits one line on IFS; sash `read` slurps all of
    // stdin on NUL. Converting would be a silent semantic change.
    let out = convert("read_array", "#!/bin/bash\nread -a arr\n");
    assert!(
        out.contains("# MANUAL:") && out.contains("NUL") && out.contains("read -a arr"),
        "expected MANUAL note with the command untouched, got:\n{}",
        out
    );
}

#[test]
fn read_multi_target_is_manual() {
    let out = convert("read_multi", "#!/bin/bash\nread a b c\n");
    assert!(
        out.contains("# MANUAL:") && out.contains("multi-target") && out.contains("read a b c"),
        "expected multi-target MANUAL note, got:\n{}",
        out
    );
}

#[test]
fn mapfile_moves_the_redirect_onto_tr() {
    // `mapfile -t a < f` is a redirected_statement. Replacing only the
    // command would re-bind `< f` to `read`, which would then read the
    // file raw instead of tr's NUL-separated output.
    let out = convert("mapfile_redir", "#!/bin/bash\nmapfile -t names < f\n");
    assert!(
        out.contains(r#"/usr/bin/tr "\n" "\0" < f | read names"#) && !out.contains("mapfile"),
        "expected the redirect to move onto tr, got:\n{}",
        out
    );
}

#[test]
fn mapfile_without_t_is_manual() {
    // Without -t bash keeps each trailing newline; sash `read` splits on
    // NUL and cannot retain the separator.
    let out = convert("mapfile_no_t", "#!/bin/bash\nmapfile lines < f\n");
    assert!(
        out.contains("# MANUAL:") && out.contains("without -t") && out.contains("mapfile lines"),
        "expected MANUAL note, got:\n{}",
        out
    );
}

#[test]
fn attest_hint_names_the_collection_form_for_bulk_targets() {
    // `readline` binds a scalar, `read`/`mapfile` bind a collection —
    // naming the wrong attest form sends the reader at a diagnostic they
    // cannot satisfy.
    let scalar = convert("attest_scalar", "#!/bin/bash\nread -r line\n");
    let bulk = convert("attest_bulk", "#!/bin/bash\nmapfile -t names < f\n");
    assert!(
        scalar.contains("attest line [[") && bulk.contains("attest x in names [["),
        "expected form to follow the target kind, got:\n{}\n{}",
        scalar,
        bulk
    );
}

#[test]
fn after_hints_sort_before_the_next_lines_before_hints() {
    // An ATTEST hint means "after this line" and a MANUAL means "before
    // this line" — the same insertion point. If the MANUAL wins, it reads
    // as belonging to the preceding command.
    let out = convert("hint_order", "#!/bin/bash\nread -r x\nread a b c\n");
    let attest = out.find("# ATTEST: x").expect("attest hint");
    let manual = out.find("# MANUAL:").expect("manual hint");
    assert!(
        attest < manual,
        "expected the ATTEST hint first, got:\n{}",
        out
    );
}
