//! C oracle end-to-end tests.
//!
//! Each test:
//! 1. Runs a bash script through `sash --emit-c` to produce C source
//! 2. Compiles the C with `gcc -nostartfiles -static` against
//!    libsash_rt.a (same runtime as Cranelift output)
//! 3. Runs the binary, captures stdout + exit status
//! 4. Runs the same script with `bash`, captures stdout + exit
//! 5. Asserts stdout and exit status match
//!
//! The C path provides a differential oracle vs Cranelift codegen:
//! the same SsaProgram drives both compilation paths, the same
//! libsash_rt.a is linked, so any disagreement at runtime is a
//! bug in one of the codegens — not a runtime divergence.
//!
//! See `notes/tasks/add-c-oracle.md` for scope. Phase 1 covers
//! literal echo, exit, simple multi-statement programs.

use std::process::Command;

fn sash_bin() -> String {
    let p = std::path::PathBuf::from(env!("CARGO_BIN_EXE_sash"));
    p.to_str().unwrap().to_string()
}

/// Locate the sash-rt static library, relative to the test binary's
/// directory. Layout: $CARGO_TARGET_DIR/release/sash + a sibling
/// sash-rt/target/release/libsash_rt.a at the workspace root.
fn libsash_rt() -> String {
    let sash_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_sash"));
    // sash_bin lives at <workspace>/target/release/sash (or debug)
    // Walk up to find the workspace root, then descend into sash-rt.
    let workspace = sash_path
        .ancestors()
        .find(|p| p.join("sash-rt").is_dir())
        .expect("workspace root with sash-rt/ not found");
    workspace
        .join("sash-rt")
        .join("target")
        .join("release")
        .join("libsash_rt.a")
        .to_str()
        .unwrap()
        .to_string()
}

fn write_tmp(name: &str, content: &str) -> String {
    let path = format!("/tmp/sash_c_{}.sh", name);
    std::fs::write(&path, content).unwrap();
    path
}

/// Compile bash → C source.
fn to_c(name: &str, script: &str) -> String {
    let src = write_tmp(name, script);
    let out = Command::new(sash_bin())
        .args(["--emit-c", &src])
        .output()
        .expect("sash failed");
    assert!(
        out.status.success(),
        "sash --emit-c failed for {}: {}",
        name,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// Compile C → binary, run it. Returns (stdout, exit_code).
/// Links against libsash_rt.a with -nostartfiles so sash-rt's
/// own _start runs (sets up argc/argv/envp the same way the
/// Cranelift-compiled binary does).
fn run_c(name: &str, c_src: &str) -> (String, i32) {
    let cpath = format!("/tmp/sash_c_run_{}.c", name);
    let bpath = format!("/tmp/sash_c_run_{}.bin", name);
    std::fs::write(&cpath, c_src).unwrap();
    let lib = libsash_rt();
    assert!(
        std::path::Path::new(&lib).is_file(),
        "libsash_rt.a not found at {} — run `cargo build --release` in sash-rt/",
        lib
    );
    let gcc = Command::new("gcc")
        .args([
            "-nostartfiles",
            "-static",
            "-o",
            &bpath,
            &cpath,
            &lib,
            "-lpthread",
        ])
        .output()
        .expect("gcc not found");
    assert!(
        gcc.status.success(),
        "gcc failed for {}: {}\nC source:\n{}",
        name,
        String::from_utf8_lossy(&gcc.stderr),
        c_src
    );
    let out = Command::new(&bpath).output().expect("run failed");
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        out.status.code().unwrap_or(-1),
    )
}

/// Run bash script, return (stdout, exit_code).
fn run_bash(name: &str, script: &str) -> (String, i32) {
    let path = format!("/tmp/sash_c_bash_{}.sh", name);
    std::fs::write(&path, script).unwrap();
    let out = Command::new("bash")
        .arg(&path)
        .output()
        .expect("bash failed");
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        out.status.code().unwrap_or(-1),
    )
}

/// Full round-trip: bash script → C source → compile → run, compare to bash.
fn assert_c_matches_bash(name: &str, script: &str) {
    let c_src = to_c(name, script);
    let (bash_out, bash_exit) = run_bash(name, script);
    let (c_out, c_exit) = run_c(name, &c_src);
    assert_eq!(
        bash_out, c_out,
        "stdout mismatch for {}.\nBash: {:?}\nC:    {:?}\nC source:\n{}",
        name, bash_out, c_out, c_src
    );
    assert_eq!(
        bash_exit, c_exit,
        "exit mismatch for {} (bash={} c={}).\nC source:\n{}",
        name, bash_exit, c_exit, c_src
    );
}

// ═══════════════════════════════════════════════════════════════
// PHASE 1: BASIC OUTPUT
// ═══════════════════════════════════════════════════════════════

#[test]
fn c_echo_literal() {
    assert_c_matches_bash("echo_lit", "#!/bin/bash\necho hello");
}

#[test]
fn c_echo_two_words() {
    assert_c_matches_bash("echo_two", "#!/bin/bash\necho hello world");
}

#[test]
fn c_echo_three_words() {
    assert_c_matches_bash("echo_three", "#!/bin/bash\necho a b c");
}

#[test]
fn c_echo_empty() {
    assert_c_matches_bash("echo_empty", "#!/bin/bash\necho");
}

#[test]
fn c_three_statements() {
    assert_c_matches_bash("three_stmts", "#!/bin/bash\necho a\necho b\necho c");
}

#[test]
fn c_exit_nonzero() {
    assert_c_matches_bash("exit_7", "#!/bin/bash\nexit 7");
}

#[test]
fn c_exit_zero() {
    assert_c_matches_bash("exit_0", "#!/bin/bash\nexit 0");
}

#[test]
fn c_echo_then_exit() {
    assert_c_matches_bash("echo_exit", "#!/bin/bash\necho before\nexit 3");
}
