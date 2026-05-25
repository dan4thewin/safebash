#[cfg(test)]
mod tests {
    use sash::emit_janet::emit_janet;
    use sash::oracle::{run_bash, run_janet_ok};

    /// Parse bash with new parser, emit Janet, return Janet source.
    fn to_janet(bash: &str) -> String {
        let (program, _diags) = sash::parse::parse(bash).expect("parse failed");
        emit_janet(&program)
    }

    /// Run bash and janet, compare stdout.
    fn assert_oracle_match(bash: &str) {
        let janet_code = to_janet(bash);
        let (_bash_code, bash_out) = run_bash(bash).expect("bash failed");
        let janet_out = run_janet_ok(&janet_code)
            .unwrap_or_else(|e| panic!("janet failed:\n{}\n\njanet source:\n{}", e, janet_code));

        assert_eq!(
            bash_out.trim(),
            janet_out.trim(),
            "\nbash output:  {:?}\njanet output: {:?}\njanet source:\n{}",
            bash_out.trim(),
            janet_out.trim(),
            janet_code
        );
    }

    // ═══════════════════════════════════════════════════════════
    // Phase 1: Janet subprocess works
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn janet_runs() {
        let out = run_janet_ok(r#"(print "hello from janet")"#).expect("janet failed");
        assert_eq!(out.trim(), "hello from janet");
    }

    #[test]
    fn janet_exit_code() {
        let (code, _, _) = sash::oracle::run_janet(r#"(os/exit 42)"#).expect("run failed");
        assert_eq!(code, 42);
    }

    // ═══════════════════════════════════════════════════════════
    // Phase 2: Assignment emission
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn emit_assignment() {
        let j = to_janet("x=hello\necho $x\n");
        assert!(j.contains("(var x"), "should declare x:\n{}", j);
        let out = run_janet_ok(&j).expect("janet failed");
        assert_eq!(out.trim(), "hello");
    }

    #[test]
    fn emit_reassignment() {
        let j = to_janet("x=1\nx=2\necho $x\n");
        assert!(j.contains("(var x"), "first should be var:\n{}", j);
        assert!(j.contains("(set x"), "second should be set:\n{}", j);
        let out = run_janet_ok(&j).expect("janet failed");
        assert_eq!(out.trim(), "2");
    }

    #[test]
    fn oracle_assignment() {
        assert_oracle_match("x=hello\necho $x\n");
    }

    #[test]
    fn oracle_reassignment() {
        assert_oracle_match("x=1\nx=2\necho $x\n");
    }

    #[test]
    fn oracle_readonly() {
        assert_oracle_match("readonly X=hello\necho $X\n");
    }

    #[test]
    fn emit_readonly_uses_def() {
        let j = to_janet("readonly X=hello\n");
        assert!(j.contains("(def X"), "readonly should use def:\n{}", j);
    }

    // ═══════════════════════════════════════════════════════════
    // Phase 3: Command emission
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn oracle_echo_literal() {
        assert_oracle_match("echo hello\n");
    }

    #[test]
    fn oracle_echo_multiple_args() {
        assert_oracle_match("echo hello world\n");
    }

    #[test]
    fn oracle_echo_quoted() {
        assert_oracle_match("echo \"hello world\"\n");
    }

    #[test]
    fn oracle_echo_variable() {
        assert_oracle_match("x=hello\necho \"$x\"\n");
    }

    #[test]
    fn oracle_echo_interpolation() {
        assert_oracle_match("name=world\necho \"hello $name\"\n");
    }

    #[test]
    fn oracle_general_command() {
        // printf is a general command, not special-cased
        assert_oracle_match("printf '%s\\n' hello\n");
    }

    // ═══════════════════════════════════════════════════════════
    // Phase 4: If/else emission
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn oracle_if_true() {
        assert_oracle_match("if [[ -d /tmp ]]; then\n  echo yes\nfi\n");
    }

    #[test]
    fn oracle_if_false() {
        assert_oracle_match(
            "if [[ -d /nonexistent_dir_xyz ]]; then\n  echo yes\nelse\n  echo no\nfi\n",
        );
    }

    #[test]
    fn oracle_if_string_equal() {
        assert_oracle_match(
            "x=hello\nif [[ \"$x\" = \"hello\" ]]; then\n  echo match\nelse\n  echo no\nfi\n",
        );
    }

    #[test]
    fn oracle_if_else_assignment() {
        assert_oracle_match(
            "if [[ -d /tmp ]]; then\n  status=exists\nelse\n  status=missing\nfi\necho $status\n",
        );
    }

    // ═══════════════════════════════════════════════════════════
    // Phase 5: Pipeline emission
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn oracle_pipeline() {
        assert_oracle_match("echo hello | cat\n");
    }

    #[test]
    fn oracle_pipeline_grep() {
        assert_oracle_match("echo 'hello world' | grep hello\n");
    }

    // ═══════════════════════════════════════════════════════════
    // Phase 6: For loop emission
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn oracle_for_loop() {
        assert_oracle_match("for x in a b c; do\n  echo $x\ndone\n");
    }

    // ═══════════════════════════════════════════════════════════
    // Phase 7: Function emission
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn oracle_function_def_and_call() {
        assert_oracle_match("greet() { echo hello; }\ngreet\n");
    }

    // ═══════════════════════════════════════════════════════════
    // Integration: multi-feature scripts
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn oracle_assign_and_echo() {
        assert_oracle_match("x=hello\ny=world\necho \"$x $y\"\n");
    }

    #[test]
    fn oracle_conditional_output() {
        assert_oracle_match(
            "x=5\nif [[ \"$x\" = \"5\" ]]; then\n  echo five\nelse\n  echo other\nfi\n",
        );
    }

    // ═══════════════════════════════════════════════════════════
    // Janet code quality
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn janet_code_is_valid() {
        // Verify emitted Janet parses without error
        let cases = vec![
            "x=1\n",
            "echo hello\n",
            "x=1\necho $x\n",
            "if [[ -d /tmp ]]; then echo y; fi\n",
            "for x in a b; do echo $x; done\n",
        ];
        for bash in cases {
            let j = to_janet(bash);
            let result = run_janet_ok(&j);
            assert!(
                result.is_ok(),
                "janet failed for bash: {}\njanet:\n{}\nerror: {}",
                bash,
                j,
                result.unwrap_err()
            );
        }
    }

    // ═══════════════════════════════════════════════════════════
    // Edge cases
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn empty_echo() {
        assert_oracle_match("echo\n");
    }

    #[test]
    fn multiple_echo() {
        assert_oracle_match("echo a\necho b\necho c\n");
    }

    #[test]
    fn comment_preserved() {
        // Comments shouldn't affect output
        assert_oracle_match("# this is a comment\necho hello\n");
    }

    // ═══════════════════════════════════════════════════════════
    // Phase 8: Parameter expansion (028 parity)
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn oracle_default_value_empty() {
        assert_oracle_match("echo \"${x:-fallback}\"\n");
    }

    #[test]
    fn oracle_default_value_set() {
        assert_oracle_match("x=hello\necho \"${x:-fallback}\"\n");
    }

    #[test]
    fn oracle_strip_suffix_longest() {
        assert_oracle_match("x=\"foo/bar/baz\"\necho \"${x%%/*}\"\n");
    }

    #[test]
    fn oracle_strip_suffix_shortest() {
        assert_oracle_match("x=\"foo/bar/baz\"\necho \"${x%/*}\"\n");
    }

    #[test]
    fn oracle_strip_prefix_shortest() {
        assert_oracle_match("x=\"foo/bar/baz\"\necho \"${x#*/}\"\n");
    }

    #[test]
    fn oracle_strip_prefix_longest() {
        assert_oracle_match("x=\"foo/bar/baz\"\necho \"${x##*/}\"\n");
    }

    // ═══════════════════════════════════════════════════════════
    // Phase 9: Command substitution (029 parity)
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn oracle_cmdsub_echo() {
        assert_oracle_match("x=$(echo hello)\necho \"$x\"\n");
    }

    #[test]
    fn oracle_cmdsub_with_args() {
        assert_oracle_match("x=$(uname -s)\necho \"$x\"\n");
    }

    #[test]
    fn oracle_cmdsub_inline() {
        assert_oracle_match("echo \"$(echo world)\"\n");
    }

    #[test]
    fn oracle_cmdsub_with_variable() {
        assert_oracle_match("name=\"world\"\nx=$(echo \"$name\")\necho \"$x\"\n");
    }

    // ── Phase 10: Type system (DD#16) ────────────────────────────

    /// Verify lowerer captures `declare int` type annotation.
    #[test]
    fn type_declare_int() {
        let src = "declare int x=5\necho $x";
        let (program, _) = sash::parse::parse(src).expect("parse failed");
        // Should lower successfully and produce the right output
        let janet = sash::emit_janet::emit_janet(&program);
        let out = sash::oracle::run_janet_ok(&janet).expect("janet failed");
        assert_eq!(out.trim(), "5");
    }

    /// Verify lowerer captures `declare -i` type annotation.
    #[test]
    fn type_declare_dash_i() {
        let src = "declare -i x=5\necho $x";
        let (program, _) = sash::parse::parse(src).expect("parse failed");
        let janet = sash::emit_janet::emit_janet(&program);
        let out = sash::oracle::run_janet_ok(&janet).expect("janet failed");
        assert_eq!(out.trim(), "5");
    }

    /// Verify lowerer captures `local int` type annotation.
    #[test]
    fn type_local_int() {
        let src = "f() { local int x=42; echo $x; }\nf";
        let (program, _) = sash::parse::parse(src).expect("parse failed");
        let janet = sash::emit_janet::emit_janet(&program);
        let out = sash::oracle::run_janet_ok(&janet).expect("janet failed");
        assert_eq!(out.trim(), "42");
    }

    /// Verify lowerer captures `fn` keyword.
    #[test]
    fn type_fn_keyword() {
        let src = "fn hello() { echo hi; }\nhello";
        let (program, _) = sash::parse::parse(src).expect("parse failed");
        let janet = sash::emit_janet::emit_janet(&program);
        let out = sash::oracle::run_janet_ok(&janet).expect("janet failed");
        assert_eq!(out.trim(), "hi");
    }

    /// Verify lowerer captures `-> int` on functions.
    #[test]
    fn type_fn_return_int() {
        use sash::ir::{StmtKind, VarType};
        let src = "get_port() -> int { return 8080; }";
        let (program, _) = sash::parse::parse(src).expect("parse failed");
        let func = program
            .statements
            .iter()
            .find_map(|s| {
                if let StmtKind::Function(f) = &s.kind {
                    Some(f)
                } else {
                    None
                }
            })
            .expect("no function found");
        assert_eq!(func.name, "get_port");
        assert_eq!(func.return_type, VarType::Int);
    }

    /// Verify lowerer captures `fn name -> int` variant.
    #[test]
    fn type_fn_keyword_return_int() {
        use sash::ir::{StmtKind, VarType};
        let src = "fn get_port() -> int { return 8080; }";
        let (program, _) = sash::parse::parse(src).expect("parse failed");
        let func = program
            .statements
            .iter()
            .find_map(|s| {
                if let StmtKind::Function(f) = &s.kind {
                    Some(f)
                } else {
                    None
                }
            })
            .expect("no function found");
        assert_eq!(func.name, "get_port");
        assert_eq!(func.return_type, VarType::Int);
    }

    /// Verify lowerer sets String return type for untyped functions.
    #[test]
    fn type_fn_default_string() {
        use sash::ir::{StmtKind, VarType};
        let src = "hello() { echo hi; }";
        let (program, _) = sash::parse::parse(src).expect("parse failed");
        let func = program
            .statements
            .iter()
            .find_map(|s| {
                if let StmtKind::Function(f) = &s.kind {
                    Some(f)
                } else {
                    None
                }
            })
            .expect("no function found");
        assert_eq!(func.name, "hello");
        assert_eq!(func.return_type, VarType::String);
    }

    /// Verify lowerer captures int type on declarations.
    #[test]
    fn type_declaration_vartype() {
        use sash::ir::{StmtKind, VarType};
        let src = "declare int x=5\ndeclare y=hello";
        let (program, _) = sash::parse::parse(src).expect("parse failed");

        let decls: Vec<_> = program
            .statements
            .iter()
            .filter_map(|s| {
                if let StmtKind::Declaration(d) = &s.kind {
                    Some(d)
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(decls.len(), 2);
        assert_eq!(decls[0].var_type, VarType::Int);
        assert_eq!(decls[1].var_type, VarType::String);
    }

    // ── Phase 10b: Type-aware Janet emission ─────────────────────

    /// Int variable in numeric comparison — no scan-number needed.
    #[test]
    fn type_int_no_scan_number() {
        let src = "declare int x=5\nif [[ $x -lt 10 ]]; then echo yes; fi";
        let janet = to_janet(src);
        // Should use (< x 10), not (< (scan-number x) (scan-number 10))
        assert!(
            !janet.contains("scan-number"),
            "expected no scan-number for int var, got:\n{}",
            janet
        );
        assert_oracle_match(src);
    }

    /// String variable in numeric comparison — scan-number required.
    #[test]
    fn type_string_needs_scan_number() {
        let src = "x=5\nif [[ $x -lt 10 ]]; then echo yes; fi";
        let janet = to_janet(src);
        // x is string (bare assignment), needs scan-number
        assert!(
            janet.contains("scan-number"),
            "expected scan-number for string var, got:\n{}",
            janet
        );
        assert_oracle_match(src);
    }

    /// Mixed: int var vs literal in comparison.
    #[test]
    fn type_int_vs_literal() {
        let src = "declare int count=3\nif [[ $count -eq 3 ]]; then echo match; fi";
        let janet = to_janet(src);
        assert!(
            !janet.contains("scan-number"),
            "expected no scan-number, got:\n{}",
            janet
        );
        assert_oracle_match(src);
    }

    /// fn keyword: round-trip through oracle.
    #[test]
    fn type_fn_int_oracle() {
        // fn is safebash syntax — not valid bash. Test Janet emission only.
        let src = "fn greet() { echo hello; }\ngreet";
        let janet = to_janet(src);
        let out = sash::oracle::run_janet_ok(&janet).expect("janet failed");
        assert_eq!(out.trim(), "hello");
    }
}

// ── Phase 10c: Type checker (DD#16) ──────────────────────────

fn check(src: &str) -> Vec<sash::type_check::TypeError> {
    let (program, _) = sash::parse::parse(src).expect("parse failed");
    sash::type_check::type_check(&program)
}

/// Assigning $# (int) to untyped var — error.
#[test]
fn type_err_int_to_string_var() {
    let src = "declare x=hello\nx=$#";
    let errs = check(src);
    assert!(
        !errs.is_empty(),
        "expected type error for int-to-string assignment"
    );
    assert!(
        errs[0].message.contains("int"),
        "error should mention int: {}",
        errs[0]
    );
}

/// Assigning $# to declare int var — OK.
#[test]
fn type_ok_int_to_int_var() {
    let src = "declare int x=$#";
    let errs = check(src);
    assert!(errs.is_empty(), "unexpected errors: {:?}", errs);
}

/// Assigning string to declare int var — error.
#[test]
fn type_err_string_to_int_var() {
    let src = "declare int x=hello";
    let errs = check(src);
    assert!(!errs.is_empty(), "expected type error for string-to-int");
    assert!(
        errs[0].message.contains("string"),
        "error should mention string: {}",
        errs[0]
    );
}

/// Numeric literal to int var — OK.
#[test]
fn type_ok_literal_number_to_int() {
    let src = "declare int x=42";
    let errs = check(src);
    assert!(errs.is_empty(), "unexpected errors: {:?}", errs);
}

/// exit with int variable — error.
#[test]
fn type_err_exit_int_var() {
    let src = "declare int code=256\nexit $code";
    let errs = check(src);
    assert!(
        !errs.is_empty(),
        "expected type error for exit with int var"
    );
    assert!(
        errs[0].message.contains("exit"),
        "error should mention exit: {}",
        errs[0]
    );
}

/// exit with literal — OK.
#[test]
fn type_ok_exit_literal() {
    let src = "exit 1";
    let errs = check(src);
    assert!(errs.is_empty(), "unexpected errors: {:?}", errs);
}

/// Bare assignment infers type — no error.
#[test]
fn type_ok_bare_assignment_infer() {
    let src = "x=$#";
    let errs = check(src);
    assert!(errs.is_empty(), "unexpected errors: {:?}", errs);
}

/// Int function in boolean context — error.
#[test]
fn type_err_int_func_in_if() {
    let src = "get_port() -> int { return 8080; }\nif get_port; then echo yes; fi";
    let errs = check(src);
    assert!(!errs.is_empty(), "expected type error for int func in if");
    assert!(
        errs[0].message.contains("boolean"),
        "error should mention boolean: {}",
        errs[0]
    );
}

/// String function in boolean context — OK.
#[test]
fn type_ok_string_func_in_if() {
    let src = "check() { return 0; }\nif check; then echo yes; fi";
    let errs = check(src);
    assert!(errs.is_empty(), "unexpected errors: {:?}", errs);
}
