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

/// Compile sash script → native binary via Cranelift, run it.
/// Returns (stdout, exit_code).
fn run_cranelift(name: &str, script: &str) -> (String, i32) {
    let src = write_tmp(name, script);
    let bin = format!("/tmp/sash_c_cl_{}.bin", name);
    let out = Command::new(sash_bin())
        .args(["-o", &bin, &src])
        .output()
        .expect("sash failed");
    assert!(
        out.status.success(),
        "sash codegen failed for {}: {}",
        name,
        String::from_utf8_lossy(&out.stderr)
    );
    let run = Command::new(&bin).output().expect("run failed");
    (
        String::from_utf8_lossy(&run.stdout).to_string(),
        run.status.code().unwrap_or(-1),
    )
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
            // Stack canaries read from %fs:0x28 (libc TLS) which
            // sash-rt's _start doesn't set up. Disable.
            "-fno-stack-protector",
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

/// Differential round-trip for sash extensions that don't run under
/// bash: C oracle vs Cranelift output, both compiled from the same
/// SSA. Disagreement = compiler bug in one of them.
fn assert_c_matches_cranelift(name: &str, script: &str) {
    let c_src = to_c(name, script);
    let (cl_out, cl_exit) = run_cranelift(name, script);
    let (c_out, c_exit) = run_c(name, &c_src);
    assert_eq!(
        cl_out, c_out,
        "stdout mismatch for {} (Cranelift vs C).\nCL: {:?}\nC:  {:?}\nC source:\n{}",
        name, cl_out, c_out, c_src
    );
    assert_eq!(
        cl_exit, c_exit,
        "exit mismatch for {} (cranelift={} c={}).\nC source:\n{}",
        name, cl_exit, c_exit, c_src
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

// ═══════════════════════════════════════════════════════════════
// PHASE 2: ARITHMETIC + CONDITIONALS
// ═══════════════════════════════════════════════════════════════

#[test]
fn c_arith_add() {
    assert_c_matches_bash("arith_add", "#!/bin/bash\necho $((2 + 3))");
}

#[test]
fn c_arith_sub() {
    assert_c_matches_bash("arith_sub", "#!/bin/bash\necho $((10 - 4))");
}

#[test]
fn c_arith_mul() {
    assert_c_matches_bash("arith_mul", "#!/bin/bash\necho $((6 * 7))");
}

#[test]
fn c_arith_div() {
    assert_c_matches_bash("arith_div", "#!/bin/bash\necho $((100 / 4))");
}

#[test]
fn c_arith_mod() {
    assert_c_matches_bash("arith_mod", "#!/bin/bash\necho $((17 % 5))");
}

#[test]
fn c_arith_precedence() {
    assert_c_matches_bash("arith_prec", "#!/bin/bash\necho $((2 + 3 * 4))");
}

#[test]
fn c_int_var_arith() {
    // Uses 'int' sash extension; compare against Cranelift not bash.
    assert_c_matches_cranelift("int_arith", "#!/bin/bash\nint x=5\necho $((x + 3))");
}

#[test]
fn c_int_var_two_step() {
    assert_c_matches_cranelift(
        "int_two",
        "#!/bin/bash\nint x=10\nint y=$((x * 2))\necho $y",
    );
}

#[test]
fn c_if_string_eq_true() {
    assert_c_matches_bash(
        "if_str_eq_t",
        "#!/bin/bash\nif [[ \"hello\" = \"hello\" ]]; then echo match; fi",
    );
}

#[test]
fn c_if_string_eq_false_with_else() {
    assert_c_matches_bash(
        "if_str_eq_f",
        "#!/bin/bash\nif [[ \"hello\" = \"world\" ]]; then echo no; else echo yes; fi",
    );
}

#[test]
fn c_if_string_neq() {
    assert_c_matches_bash(
        "if_str_neq",
        "#!/bin/bash\nif [[ \"hello\" != \"world\" ]]; then echo neq; fi",
    );
}

#[test]
fn c_if_empty_eq_empty() {
    assert_c_matches_bash(
        "if_empty",
        "#!/bin/bash\nif [[ \"\" = \"\" ]]; then echo empty; fi",
    );
}

#[test]
fn c_if_n_nonempty() {
    assert_c_matches_bash(
        "if_n",
        "#!/bin/bash\nif [[ -n \"abc\" ]]; then echo nonempty; fi",
    );
}

#[test]
fn c_if_z_empty() {
    assert_c_matches_bash(
        "if_z",
        "#!/bin/bash\nif [[ -z \"\" ]]; then echo zempty; fi",
    );
}

#[test]
fn c_if_var_string_eq() {
    assert_c_matches_bash(
        "if_var_eq",
        "#!/bin/bash\na=hello\nb=hello\nif [[ \"$a\" = \"$b\" ]]; then echo eq; fi",
    );
}

// ═══════════════════════════════════════════════════════════════
// PHASE 3: PARAMETER EXPANSION + COMMAND SUBSTITUTION
// ═══════════════════════════════════════════════════════════════

#[test]
fn c_pe_strlen() {
    assert_c_matches_bash("pe_strlen", "#!/bin/bash\nx=hello\necho \"${#x}\"");
}

#[test]
fn c_pe_strip_suffix() {
    assert_c_matches_bash(
        "pe_strip_suffix",
        "#!/bin/bash\nx=hello.txt\necho \"${x%.txt}\"",
    );
}

#[test]
fn c_pe_strip_suffix_longest() {
    assert_c_matches_bash(
        "pe_strip_suffix_l",
        "#!/bin/bash\nx=hello.world.txt\necho \"${x%%.*}\"",
    );
}

#[test]
fn c_pe_strip_prefix() {
    assert_c_matches_bash(
        "pe_strip_prefix",
        "#!/bin/bash\nx=hello.txt\necho \"${x#hello.}\"",
    );
}

#[test]
fn c_pe_strip_prefix_longest() {
    assert_c_matches_bash(
        "pe_strip_prefix_l",
        "#!/bin/bash\nx=/usr/bin/foo\necho \"${x##*/}\"",
    );
}

#[test]
fn c_pe_default_unset() {
    assert_c_matches_bash(
        "pe_def_unset",
        "#!/bin/bash\nunset y\necho \"${y:-default}\"",
    );
}

#[test]
fn c_pe_default_set() {
    assert_c_matches_bash(
        "pe_def_set",
        "#!/bin/bash\ny=present\necho \"${y:-default}\"",
    );
}

#[test]
fn c_pe_alternate_unset() {
    assert_c_matches_bash("pe_alt_unset", "#!/bin/bash\nunset y\necho \"${y:+alt}\"");
}

#[test]
fn c_pe_alternate_set() {
    assert_c_matches_bash("pe_alt_set", "#!/bin/bash\ny=present\necho \"${y:+alt}\"");
}

#[test]
fn c_pe_substring() {
    assert_c_matches_bash("pe_substring", "#!/bin/bash\nx=hello\necho \"${x:1:3}\"");
}

#[test]
fn c_pe_substring_to_end() {
    assert_c_matches_bash(
        "pe_substring_end",
        "#!/bin/bash\nx=long_string\necho \"${x:5}\"",
    );
}

#[test]
fn c_pe_toupper_first() {
    assert_c_matches_bash("pe_toupper1", "#!/bin/bash\nx=hello\necho \"${x^}\"");
}

#[test]
fn c_pe_tolower_all() {
    assert_c_matches_bash("pe_tolower_all", "#!/bin/bash\nx=HELLO\necho \"${x,,}\"");
}

#[test]
fn c_cmdsub_epochseconds() {
    // epochseconds is a sash intrinsic — bash has no such builtin.
    // Can't strict-equality C vs Cranelift: the two runs may
    // straddle a second boundary. Test that the C-emitted version
    // produces a 10+ digit epoch.
    let script = "#!/bin/bash\necho \"$(epochseconds)\"";
    let c_src = to_c("cs_epoch", script);
    let (c_out, c_exit) = run_c("cs_epoch", &c_src);
    assert_eq!(
        c_exit, 0,
        "C exited with {} for epochseconds\n{}",
        c_exit, c_src
    );
    let trimmed = c_out.trim();
    assert!(
        !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_digit()) && trimmed.len() >= 10,
        "Expected 10+ digit epoch, got {:?}\nC source:\n{}",
        trimmed,
        c_src
    );
}

#[test]
fn c_cmdsub_getenv() {
    // getenv is a sash intrinsic. bash has no such builtin.
    // Both Cranelift and C oracle should return the env var value
    // (PATH is set in any reasonable env).
    assert_c_matches_cranelift("cs_getenv", "#!/bin/bash\necho \"$(getenv PATH)\"");
}

#[test]
fn c_cmdsub_getpid() {
    // getpid yields different values per process, so we compare
    // via Cranelift — but each run uses a different process, so
    // even comparing C-vs-Cranelift gives different values. Test
    // that both produce *some* non-empty numeric output.
    let script = "#!/bin/bash\necho \"$(getpid)\"";
    let c_src = to_c("cs_getpid", script);
    let (c_out, c_exit) = run_c("cs_getpid", &c_src);
    assert_eq!(c_exit, 0, "C exited with {} for getpid\n{}", c_exit, c_src);
    let trimmed = c_out.trim();
    assert!(
        !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_digit()),
        "Expected numeric pid, got {:?}\nC source:\n{}",
        trimmed,
        c_src
    );
}

// ═══════════════════════════════════════════════════════════════
// PHASE 4: EXTERNAL EXEC, REDIRECTS, PIPELINES, FUNCTIONS
// ═══════════════════════════════════════════════════════════════

#[test]
fn c_exec_true() {
    assert_c_matches_bash("exec_true", "#!/bin/bash\n/bin/true");
}

#[test]
fn c_exec_false_recovered() {
    assert_c_matches_bash(
        "exec_false_or",
        "#!/bin/bash\n/bin/false || /bin/echo recovered",
    );
}

#[test]
fn c_exec_echo_with_args() {
    assert_c_matches_bash("exec_echo", "#!/bin/bash\n/bin/echo hello from /bin/echo");
}

#[test]
fn c_exec_and_chain() {
    assert_c_matches_bash("exec_and", "#!/bin/bash\n/bin/true && /bin/echo yes");
}

#[test]
fn c_redirect_write() {
    assert_c_matches_bash(
        "rd_write",
        "#!/bin/bash\n/bin/echo hi > /tmp/sash_oracle_w.txt\n/bin/cat /tmp/sash_oracle_w.txt",
    );
}

#[test]
fn c_redirect_append() {
    assert_c_matches_bash(
        "rd_append",
        "#!/bin/bash\n/bin/echo first > /tmp/sash_oracle_a.txt\n/bin/echo second >> /tmp/sash_oracle_a.txt\n/bin/cat /tmp/sash_oracle_a.txt",
    );
}

#[test]
fn c_redirect_stderr_drop() {
    assert_c_matches_bash(
        "rd_stderr_drop",
        "#!/bin/bash\n/bin/echo to_stdout\n/bin/echo dropped >&2 2>/dev/null",
    );
}

#[test]
fn c_pipeline_two_stage() {
    assert_c_matches_bash(
        "pipe2",
        "#!/bin/bash\n/bin/echo hello | /usr/bin/tr a-z A-Z",
    );
}

#[test]
fn c_pipeline_three_stage() {
    assert_c_matches_bash(
        "pipe3",
        "#!/bin/bash\n/usr/bin/seq 1 5 | /usr/bin/head -n 3 | /usr/bin/tail -n 1",
    );
}

#[test]
fn c_pipeline_with_builtin_echo() {
    assert_c_matches_bash(
        "pipe_builtin",
        "#!/bin/bash\necho \"one two three\" | /usr/bin/wc -w",
    );
}

#[test]
fn c_fn_simple_call() {
    // Bash doesn't accept sash 'fn' syntax — use Cranelift oracle.
    assert_c_matches_cranelift(
        "fn_simple",
        "#!/bin/bash\nfn say() {\n    echo hello\n}\nsay\nsay\n",
    );
}

#[test]
fn c_fn_with_positional() {
    assert_c_matches_cranelift(
        "fn_pos",
        "#!/bin/bash\nfn greet() {\n    echo \"hello $1\"\n}\ngreet world\n",
    );
}

// ═══════════════════════════════════════════════════════════════
// PHASE 5: ARRAYS, MAPS, SUBSCRIPTS, READ
// ═══════════════════════════════════════════════════════════════

#[test]
fn c_array_subscript() {
    assert_c_matches_cranelift(
        "arr_sub",
        "#!/bin/bash\narray arr=(one two three)\necho \"${arr[0]}\"\necho \"${arr[2]}\"\n",
    );
}

#[test]
fn c_array_length() {
    // DD#75 §4/§2: `${#arr}` is the length (retired `${#arr[@]}`).
    assert_c_matches_cranelift(
        "arr_len",
        "#!/bin/bash\narray arr=(foo bar baz)\necho \"${#arr}\"\n",
    );
}

#[test]
fn c_array_join_all() {
    // DD#75 §4: `${arr[@]}` splat retired — join via `$(join arr)`.
    assert_c_matches_cranelift(
        "arr_all",
        "#!/bin/bash\narray arr=(x y z)\necho \"$(join arr)\"\n",
    );
}

#[test]
fn c_array_assign_element() {
    assert_c_matches_cranelift(
        "arr_set",
        "#!/bin/bash\narray arr=(p q r)\narr[1]=changed\necho \"${arr[0]} ${arr[1]} ${arr[2]}\"\n",
    );
}

#[test]
fn c_array_for_iter() {
    // DD#75 §4: `@arr` is the sole splat (`"${arr[@]}"` retired).
    assert_c_matches_cranelift(
        "arr_for",
        "#!/bin/bash\narray arr=(a b c)\nfor x in @arr; do\n    echo \"$x\"\ndone\n",
    );
}

#[test]
fn c_map_lookup() {
    assert_c_matches_cranelift(
        "map_lookup",
        "#!/bin/bash\nmap m=([key1]=\"val1\" [key2]=\"val2\")\necho \"${m[key1]}\"\necho \"${m[key2]}\"\n",
    );
}

#[test]
fn c_map_assign_element() {
    assert_c_matches_cranelift(
        "map_set",
        "#!/bin/bash\nmap m=([a]=\"first\")\nm[a]=\"updated\"\necho \"${m[a]}\"\n",
    );
}

#[test]
fn c_map_new_key() {
    assert_c_matches_cranelift(
        "map_new",
        "#!/bin/bash\nmap m=([a]=\"1\")\nm[b]=\"2\"\necho \"${m[b]}\"\n",
    );
}

// ═══════════════════════════════════════════════════════════════
// PHASE 6: BULK PARITY HOLES + DOLLAR-QUESTION + CASE
// ═══════════════════════════════════════════════════════════════

#[test]
fn c_dollar_question_after_true() {
    assert_c_matches_bash("dq_after_true", "#!/bin/bash\ntrue\necho $?");
}

#[test]
fn c_dollar_question_after_false() {
    assert_c_matches_bash("dq_after_false", "#!/bin/bash\nfalse\necho $?");
}

#[test]
fn c_dollar_question_after_extern() {
    assert_c_matches_bash("dq_after_extern", "#!/bin/bash\n/bin/true\necho $?");
}

#[test]
fn c_and_chain_true_yes() {
    // && reads test_result after true. Bash and C agree.
    assert_c_matches_bash(
        "and_true_yes",
        "#!/bin/bash\ntrue && echo yes\nfalse || echo fallback",
    );
}

#[test]
fn c_while_arith_condition() {
    // (( i < 3 )) uses LetExit to convert arith result → test_result.
    assert_c_matches_cranelift(
        "while_arith",
        "#!/bin/bash\nint i=0\nwhile (( i < 3 )); do\n  echo $i\n  (( i = i + 1 ))\ndone",
    );
}

#[test]
fn c_case_pattern_match() {
    // case uses =glob via sash_fnmatch.
    assert_c_matches_bash(
        "case_match",
        "#!/bin/bash\nv=two\ncase $v in\n  one) echo 1 ;;\n  two) echo 2 ;;\n  *) echo other ;;\nesac",
    );
}

#[test]
fn c_case_default_branch() {
    assert_c_matches_bash(
        "case_default",
        "#!/bin/bash\nv=unknown\ncase $v in\n  one) echo 1 ;;\n  two) echo 2 ;;\n  *) echo other ;;\nesac",
    );
}

#[test]
fn c_if_arith_condition() {
    // if (( x > 3 )) uses LetExit + arith comparison.
    assert_c_matches_cranelift(
        "if_arith",
        "#!/bin/bash\nint x=5\nif (( x > 3 )); then\n  echo big\nelse\n  echo small\nfi",
    );
}
