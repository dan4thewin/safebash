use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

mod common;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn run_bash_native(bash_src: &str, args: &[&str]) -> (String, i32) {
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let obj = format!("/tmp/sash_032_{}.o", id);
    let bin = format!("/tmp/sash_032_{}", id);

    let obj_bytes = sash::codegen::compile_to_object(bash_src, false).expect("compile failed");
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
    for a in args {
        cmd.arg(a);
    }
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
    for a in args {
        cmd.arg(a);
    }
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
if (( x < 10 )); then echo yes; else echo no; fi
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
}

#[test]
fn numeric_eq() {
    let src = r#"x=42
if (( x == 42 )); then echo match; else echo nope; fi
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
}

#[test]
fn numeric_gt() {
    let src = r#"x=100
if (( x > 50 )); then echo big; else echo small; fi
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
}

#[test]
fn numeric_ne() {
    let src = r#"x=7
if (( x != 8 )); then echo diff; else echo same; fi
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
}

#[test]
fn numeric_le_ge() {
    let src = r#"x=5
if (( x <= 5 )); then echo le; fi
if (( x >= 5 )); then echo ge; fi
if (( x <= 4 )); then echo bad; fi
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
}

// ── While loop tests ────────────────────────────────────────

#[test]
fn while_countdown() {
    let src = r#"x=3
while (( x > 0 )); do
    echo "$x"
    x=$((x - 1))
done
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, bash_output(src, &[]));
}

// ── Regression tests ────────────────────────────────────────

#[test]
fn until_countdown() {
    let src = r#"x=3
until (( x <= 0 )); do
    echo "$x"
    x=$(( x - 1 ))
done
echo "done"
"#;
    let (out, _) = run_bash_native(src, &[]);
    assert_eq!(out, "3\n2\n1\ndone\n");
}

// ═══════════════════════════════════════════════════════════════
// Indexed array tests (DD#33 — unified hash table)
// ═══════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════
// DD#35 — map/array/int type keywords
// ═══════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════
// Pipe-while-read tests
// ═══════════════════════════════════════════════════════════════

// ── Noop colon builtin ──────────────────────────────────────

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

// ── File tests ──────────────────────────────────────────────

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

// ── [[ -t fd ]] terminal test ───────────────────────────────

// ── [[ file1 -nt file2 ]] ──────────────────────────────────

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

// ── Here-doc ────────────────────────────────────────────────

// ── Here-string ─────────────────────────────────────────────

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

// ── command -v ──────────────────────────────────────────────

#[test]
fn command_v_rejected() {
    // DD#23: `command` does not exist in sash-native — rejected at validation
    use std::io::Write;
    let tmp = "/tmp/sash_cmdv_test.sh";
    let mut f = std::fs::File::create(tmp).unwrap();
    writeln!(f, "#!/bin/bash").unwrap();
    writeln!(f, "command -v ls").unwrap();
    drop(f);
    let sash = env!("CARGO_BIN_EXE_sash");
    let output = Command::new(sash).arg(tmp).output().expect("run sash");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "expected compile failure, got success"
    );
    common::expect_diag(&stderr, "SB5011");
    let _ = std::fs::remove_file(tmp);
}

// ── Array subscript assignment and append ───────────────────

// ── ${#arr[@]} array length ─────────────────────────────────

// ── shift N ─────────────────────────────────────────────────

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

// ── Heredoc with variable expansion ─────────────────────────

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
// ── DD#41: Sash-native array syntax ──────────────────────────

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
// ═══════════════════════════════════════════════════════════
// getenv / argv builtins (DD#42)
// ═══════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════
// File test operators: -ot, -ef
// ═══════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════
// Parameter transformations: @U, @u, @L, @Q, @E
// ═══════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════
// COMPOUND COMMAND REDIRECTS
// ═══════════════════════════════════════════════════════════════
