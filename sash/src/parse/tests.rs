//! Parser unit tests.

#[cfg(test)]
mod test {
    use crate::ir::*;
    use crate::parse::parser::parse;

    fn parse_ok(source: &str) -> Program {
        let (program, diags) = parse(source).unwrap_or_else(|diags| {
            panic!("parse failed: {:?}", diags);
        });
        assert!(diags.is_empty(), "unexpected diagnostics: {:?}", diags);
        program
    }

    #[test]
    fn test_echo_hello() {
        let p = parse_ok("echo hello\n");
        assert_eq!(p.statements.len(), 1);
    }

    #[test]
    fn test_assignment() {
        let p = parse_ok("x=42\n");
        assert_eq!(p.statements.len(), 1);
        match &p.statements[0].kind {
            StmtKind::Assignment(a) => {
                assert_eq!(a.name, "x");
            }
            _ => panic!("expected assignment"),
        }
    }

    #[test]
    fn test_if_then_fi() {
        let p = parse_ok("if true; then echo yes; fi\n");
        assert_eq!(p.statements.len(), 1);
        assert!(matches!(p.statements[0].kind, StmtKind::If(_)));
    }

    #[test]
    fn test_for_loop() {
        let p = parse_ok("for x in a b c; do echo $x; done\n");
        assert_eq!(p.statements.len(), 1);
        assert!(matches!(p.statements[0].kind, StmtKind::For(_)));
    }

    // `eval` is rejected by validate (SB4001, dynamic execution), not by the
    // parser — the removed-builtin table that used to reject it at parse time
    // is gone. Spec cov.98 / cov.100 pin the rejection end-to-end.
    #[test]
    fn test_eval_parses_as_plain_command() {
        let p = parse_ok("eval echo hi\n");
        assert_eq!(p.statements.len(), 1);
    }

    // `export` / `declare` / `readonly` / `select` are NOT reserved words: the
    // parser's removed-builtin table is gone (DG 2026-09-12), so they parse as
    // ordinary commands and validate rejects them with the same SB5011 every
    // other unknown name gets. Covered end-to-end by spec cov.24 / cov.94.
    #[test]
    fn test_export_parses_as_plain_command() {
        let p = parse_ok("export FOO=bar\n");
        assert_eq!(p.statements.len(), 1);
    }

    #[test]
    fn test_pipeline() {
        let p = parse_ok("echo hello | cat\n");
        assert_eq!(p.statements.len(), 1);
        assert!(matches!(p.statements[0].kind, StmtKind::Pipeline(_)));
    }
    // ── DD#93 Part C: content-shape tests `i?` / `p?` / `g?` ──────────────
    // General unary test operators (usable anywhere a unary test is), spelled
    // with a `?` suffix to stay out of bash's saturated `-X` namespace.

    /// The `TestExpr` of a bare `[[ … ]]` statement, or None if it did not
    /// parse into one.
    fn parse_bare_test(source: &str) -> Option<TestExpr> {
        let (program, _diags) = parse(source).ok()?;
        match &program.statements.first()?.kind {
            StmtKind::Command(c) => match &c.args.first()?.kind {
                ExprKind::Test(t) => Some(t.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    #[test]
    fn test_shape_ops_are_unary_test_ops() {
        for op in ["i?", "p?", "g?"] {
            let src = format!("[[ {} $x ]]\n", op);
            match parse_bare_test(&src) {
                Some(TestExpr::Unary { op: got, operand }) => {
                    assert_eq!(got, op, "op mismatch for {}", op);
                    assert!(
                        matches!(operand.kind, ExprKind::SimpleExpansion(ref n) if n == "x"),
                        "operand for {}: {:?}",
                        op,
                        operand.kind
                    );
                }
                other => panic!("{} did not parse as a unary test: {:?}", op, other),
            }
        }
    }

    #[test]
    fn test_shape_ops_accept_literal_and_quoted_operands() {
        for src in ["[[ i? 42 ]]\n", "[[ p? \"a b\" ]]\n", "[[ g? 'x' ]]\n"] {
            assert!(
                matches!(parse_bare_test(src), Some(TestExpr::Unary { .. })),
                "did not parse as a unary test: {}",
                src
            );
        }
    }

    #[test]
    fn test_shape_ops_compose_with_not_and_and() {
        assert!(matches!(
            parse_bare_test("[[ ! i? $x ]]\n"),
            Some(TestExpr::Not(_))
        ));
        match parse_bare_test("[[ i? $x && p? $y ]]\n") {
            Some(TestExpr::And(l, r)) => {
                assert!(matches!(*l, TestExpr::Unary { ref op, .. } if op == "i?"));
                assert!(matches!(*r, TestExpr::Unary { ref op, .. } if op == "p?"));
            }
            other => panic!("expected And of two unary tests, got {:?}", other),
        }
    }

    #[test]
    fn test_shape_ops_are_general_operators() {
        // Not attest-only: valid wherever a unary test is (DD#93 Part C).
        parse_ok("if [[ p? $x ]]; then echo yes; fi\n");
        parse_ok("while [[ g? $x ]]; do echo y; done\n");
    }

    /// DD#93 Part C renamed `-i` to `i?` — hard, no alias.
    #[test]
    fn test_dash_i_no_longer_a_unary_test_op() {
        let got = parse_bare_test("[[ -i 42 ]]\n");
        assert!(
            !matches!(got, Some(TestExpr::Unary { ref op, .. }) if op == "-i"),
            "`-i` should no longer parse as a unary test op, got {:?}",
            got
        );
    }

    // ── DD#93 Part B: `attest VAR OP` unary shorthand ─────────────────────
    // `attest x OP` ≡ `attest x [[ OP $x ]]`. Because the operator binds the
    // attested name by construction, a wrong-var predicate is unwriteable.

    /// The args of an `attest` statement.
    fn attest_args(source: &str) -> Vec<Expr> {
        let p = parse_ok(source);
        match &p.statements[0].kind {
            StmtKind::Command(c) => {
                assert!(matches!(&c.name.kind, ExprKind::Literal(n) if n == "attest"));
                c.args.clone()
            }
            other => panic!("expected an attest command, got {:?}", other),
        }
    }

    /// The target name of a DD#93 Part D conversion predicate.
    fn convert_target<'a>(pred: &AttestPredicate<'a>) -> &'a str {
        match pred {
            AttestPredicate::ArithConvert { target, .. } => target,
            other => panic!("expected a conversion predicate, got {:?}", other),
        }
    }

    /// The `[[ ]]` predicate of an attest — the DD#93 Part D conversion form
    /// carries no `TestExpr`.
    fn attest_test<'a>(pred: &'a AttestPredicate<'a>) -> &'a TestExpr {
        match pred {
            AttestPredicate::Test(t) => t,
            other => panic!("expected a [[ ]] predicate, got {:?}", other),
        }
    }

    /// A `TestExpr::Unary` as (op, operand variable name).
    fn unary_of(t: &TestExpr) -> (String, String) {
        match t {
            TestExpr::Unary { op, operand } => match &operand.kind {
                ExprKind::SimpleExpansion(n) => (op.clone(), n.clone()),
                other => panic!("expected a $var operand, got {:?}", other),
            },
            other => panic!("expected a unary test, got {:?}", other),
        }
    }

    #[test]
    fn test_attest_scalar_shorthand() {
        let args = attest_args("attest x i?\n");
        let (form, pred) = classify_attest(&args).expect("should classify as attest");
        assert_eq!(form, AttestForm::Scalar { var: "x" });
        assert_eq!(
            unary_of(attest_test(&pred)),
            ("i?".to_string(), "x".to_string())
        );
    }

    #[test]
    fn test_attest_shorthand_is_sugar_for_the_full_form() {
        for (short, full) in [
            ("attest x i?\n", "attest x [[ i? $x ]]\n"),
            ("attest x -n\n", "attest x [[ -n $x ]]\n"),
            ("attest e in a p?\n", "attest e in a [[ p? $e ]]\n"),
        ] {
            let (sa, fa) = (attest_args(short), attest_args(full));
            let (sform, spred) = classify_attest(&sa).expect(short);
            let (fform, fpred) = classify_attest(&fa).expect(full);
            assert_eq!(sform, fform, "form differs: {} vs {}", short, full);
            assert_eq!(
                unary_of(attest_test(&spred)),
                unary_of(attest_test(&fpred)),
                "predicate differs: {} vs {}",
                short,
                full
            );
        }
    }

    #[test]
    fn test_attest_all_shorthand_binds_the_element_binder() {
        let args = attest_args("attest e in a i?\n");
        let (form, pred) = classify_attest(&args).expect("should classify as attest");
        assert_eq!(
            form,
            AttestForm::Indexed {
                binder: "e",
                array: "a"
            }
        );
        assert_eq!(
            unary_of(attest_test(&pred)),
            ("i?".to_string(), "e".to_string())
        );
    }

    #[test]
    fn test_attest_map_shorthand_binds_both_binders() {
        // DG 2026-09-07: `attest k v in m OP` ≡ `[[ OP $k && OP $v ]]`. The
        // map form already requires a predicate to reference BOTH binders
        // (SB5020), so binding only one would make the shorthand unusable.
        let args = attest_args("attest k v in m p?\n");
        let (form, pred) = classify_attest(&args).expect("should classify as attest");
        assert_eq!(
            form,
            AttestForm::Map {
                key: "k",
                val: "v",
                map: "m"
            }
        );
        match attest_test(&pred) {
            TestExpr::And(l, r) => {
                assert_eq!(unary_of(l), ("p?".to_string(), "k".to_string()));
                assert_eq!(unary_of(r), ("p?".to_string(), "v".to_string()));
            }
            other => panic!("expected `OP $k && OP $v`, got {:?}", other),
        }
    }

    #[test]
    fn test_attest_full_forms_still_parse() {
        for src in [
            "attest x [[ i? $x ]]\n",
            "attest e in a [[ -n $e ]]\n",
            "attest k v in m [[ -n $k && -n $v ]]\n",
        ] {
            let args = attest_args(src);
            assert!(
                classify_attest(&args).is_some(),
                "did not classify: {}",
                src
            );
        }
    }

    // ── DD#93 Part D: `attest SOURCE (( TARGET = SOURCE ))` ───────────────
    // The int-assignment is the predicate: the conversion can only succeed on
    // a valid, representable i64, so performing it proves the property.

    #[test]
    fn test_attest_arith_convert_form() {
        let args = attest_args("attest numstr (( myint = numstr ))\n");
        let (form, pred) = classify_attest(&args).expect("should classify as attest");
        assert_eq!(form, AttestForm::Scalar { var: "numstr" });
        assert_eq!(convert_target(&pred), "myint");
    }

    #[test]
    fn test_attest_arith_convert_accepts_dollar_source() {
        // `numstr` and `$numstr` are the same read in arithmetic.
        let args = attest_args("attest numstr (( myint = $numstr ))\n");
        let (_, pred) = classify_attest(&args).expect("should classify as attest");
        assert_eq!(convert_target(&pred), "myint");
    }

    #[test]
    fn test_attest_arith_convert_accepts_a_wider_rhs() {
        // DG 2026-09-07: computing a wider RHS still requires the conversion to
        // succeed, so the `i?` gate proves the same thing — the RHS only has to
        // REFERENCE the attested name.
        for src in [
            "attest s (( t = s * 2 ))\n",
            "attest s (( t = s + k ))\n",
            "attest s (( t = (s - 1) / 2 ))\n",
        ] {
            let args = attest_args(src);
            let (_, pred) = classify_attest(&args).unwrap_or_else(|| panic!("{}", src));
            assert_eq!(convert_target(&pred), "t", "for {}", src);
        }
    }

    #[test]
    fn test_attest_arith_convert_rejects_other_shapes() {
        // An RHS that never mentions the attested name proves nothing about it,
        // and a non-assignment is not the form at all. validate reports these
        // as SB5054.
        for src in [
            "attest s (( t = other ))\n",
            "attest s (( t = 5 ))\n",
            "attest s (( s ))\n",
        ] {
            let args = attest_args(src);
            assert!(
                classify_attest(&args).is_none(),
                "should not classify as an attest form: {}",
                src
            );
        }
    }
}

// ── Logos prototype equivalence tests ────────────────────────────────
//
// These tests are part of r47-decide-logos-vs-handroll (DD pending).
// For each input, we walk both the hand-rolled scanner and the logos
// recogniser through the substrings of the source that fall into the
// logos-eligible subset (i.e., the "structural" tokens), and verify
// that logos produces the same `Tok` value for every match.
#[cfg(test)]
mod logos_equivalence {
    use crate::parse::logos_tokens::{to_tok, LogosTok};
    use logos::Logos;

    fn tokens_of(src: &str) -> Vec<(usize, usize, LogosTok)> {
        let mut lex = LogosTok::lexer(src);
        let mut out = Vec::new();
        while let Some(Ok(tok)) = lex.next() {
            let span = lex.span();
            out.push((span.start, span.end, tok));
        }
        out
    }

    #[test]
    fn keywords_recognized() {
        let src = "if then else elif fi while until do done for in case esac fn function int const array map attest END";
        let toks = tokens_of(src);
        let mut got: Vec<LogosTok> = toks.into_iter().map(|(_, _, t)| t).collect();
        let expected = vec![
            LogosTok::If,
            LogosTok::Then,
            LogosTok::Else,
            LogosTok::Elif,
            LogosTok::Fi,
            LogosTok::While,
            LogosTok::Until,
            LogosTok::Do,
            LogosTok::Done,
            LogosTok::For,
            LogosTok::In,
            LogosTok::Case,
            LogosTok::Esac,
            LogosTok::Fn,
            LogosTok::Function,
            LogosTok::Int,
            LogosTok::Const,
            LogosTok::Array,
            LogosTok::Map,
            LogosTok::Attest,
            LogosTok::End,
        ];
        got.retain(|t| !matches!(t, LogosTok::Newline));
        assert_eq!(got, expected);
    }

    #[test]
    fn operators_longest_match() {
        // `&&` must beat `&`, `||` beats `|`, `>>` beats `>`, etc.
        let cases = [
            ("&&", LogosTok::AndAnd),
            ("&", LogosTok::Amp),
            ("||", LogosTok::OrOr),
            ("|", LogosTok::Pipe),
            (">>", LogosTok::DblGt),
            (">&", LogosTok::GtAmp),
            (">", LogosTok::Gt),
            ("&>", LogosTok::AmpGt),
            ("((", LogosTok::DblParen),
            ("))", LogosTok::DblParenClose),
            ("(", LogosTok::LParen),
            (")", LogosTok::RParen),
            ("{", LogosTok::LBrace),
            ("}", LogosTok::RBrace),
            ("[[", LogosTok::DblBracketOpen),
            ("]]", LogosTok::DblBracketClose),
            (";;", LogosTok::DblSemi),
            (";", LogosTok::Semi),
            ("<<<", LogosTok::HereString),
            ("+=", LogosTok::PlusAssign),
            ("!", LogosTok::Bang),
        ];
        for (src, want) in cases {
            let toks = tokens_of(src);
            assert_eq!(toks.len(), 1, "input {:?} produced {:?}", src, toks);
            assert_eq!(toks[0].2, want, "input {:?}", src);
            assert_eq!(toks[0].0, 0);
            assert_eq!(toks[0].1, src.len());
        }
    }

    #[test]
    fn comment_to_eol() {
        let src = "# this is a comment\n";
        let toks = tokens_of(src);
        assert_eq!(toks.len(), 2);
        assert_eq!(toks[0].2, LogosTok::Comment);
        assert_eq!(toks[0].0, 0);
        assert_eq!(toks[0].1, 19);
        assert_eq!(toks[1].2, LogosTok::Newline);
    }

    #[test]
    fn whitespace_skipped_continuations_swallowed() {
        // \\<newline> is a line continuation: skipped silently.
        let src = "if   \\\n then\n";
        let toks = tokens_of(src);
        let kinds: Vec<LogosTok> = toks.into_iter().map(|(_, _, t)| t).collect();
        assert_eq!(kinds, vec![LogosTok::If, LogosTok::Then, LogosTok::Newline]);
    }

    #[test]
    fn to_tok_maps_all_variants() {
        // Walk every variant except Comment and verify to_tok returns Some.
        let all = [
            LogosTok::If,
            LogosTok::Then,
            LogosTok::Else,
            LogosTok::Elif,
            LogosTok::Fi,
            LogosTok::While,
            LogosTok::Until,
            LogosTok::Do,
            LogosTok::Done,
            LogosTok::For,
            LogosTok::In,
            LogosTok::Case,
            LogosTok::Esac,
            LogosTok::Fn,
            LogosTok::Function,
            LogosTok::Int,
            LogosTok::Const,
            LogosTok::Array,
            LogosTok::Map,
            LogosTok::Attest,
            LogosTok::End,
            LogosTok::AndAnd,
            LogosTok::OrOr,
            LogosTok::AmpGt,
            LogosTok::DblParen,
            LogosTok::DblParenClose,
            LogosTok::DblBracketOpen,
            LogosTok::DblBracketClose,
            LogosTok::DblSemi,
            LogosTok::DblGt,
            LogosTok::GtAmp,
            LogosTok::HereString,
            LogosTok::PlusAssign,
            LogosTok::Pipe,
            LogosTok::Amp,
            LogosTok::Semi,
            LogosTok::LParen,
            LogosTok::RParen,
            LogosTok::LBrace,
            LogosTok::RBrace,
            LogosTok::Bang,
            LogosTok::Gt,
            LogosTok::Newline,
        ];
        for t in all {
            assert!(to_tok(t).is_some(), "to_tok({:?}) returned None", t);
        }
        assert!(to_tok(LogosTok::Comment).is_none());
    }
    /// `Scanner::next()` must be able to pull a token straight from the source
    /// when nothing has been peeked. Every caller in the parser peeks first, so
    /// branch coverage measured this arm at 0 hits against 86,406 for its
    /// sibling (journal 311). The arm is the scanner's only token source, not
    /// dead code — this test pins the no-peek contract so it stays honest.
    #[test]
    fn scanner_next_without_peek_pulls_from_source() {
        use crate::parse::lexer::Scanner;
        use crate::parse::lexer::Tok;

        let mut sc = Scanner::new("echo hi");

        let first = sc.next();
        assert_eq!(first.token, Tok::Word("echo".to_string()));

        let second = sc.next();
        assert_eq!(second.token, Tok::Word("hi".to_string()));

        assert_eq!(sc.next().token, Tok::Eof);
    }

    /// The peeked path and the direct path must agree token-for-token.
    #[test]
    fn scanner_next_agrees_with_peek_then_next() {
        use crate::parse::lexer::Scanner;

        let src = "if x { echo a; } else { echo b; }";
        let mut direct = Scanner::new(src);
        let mut peeked = Scanner::new(src);

        loop {
            let d = direct.next();
            peeked.peek();
            let p = peeked.next();
            assert_eq!(d.token, p.token);
            assert_eq!(d.span.start, p.span.start);
            assert_eq!(d.span.end, p.span.end);
            if matches!(d.token, crate::parse::lexer::Tok::Eof) {
                break;
            }
        }
    }
    /// The byte dispatcher hands ten structural bytes to the `logos` DFA. If
    /// the DFA failed to match one of them, `scan_via_logos` would silently
    /// fall back to a word scan that immediately breaks, yielding `Tok::Eof`
    /// and truncating the program mid-source. Both arms measured 0 hits
    /// (journal 311/312). This test proves the DFA covers every dispatched
    /// byte, so the fallback is unreachable rather than merely untested.
    #[test]
    fn logos_matches_every_dispatched_structural_byte() {
        use crate::parse::logos_tokens::LogosTok;
        use logos::Logos;

        for src in [
            "\n", "#c\n", ";", "|", "&", "(", ")", "{", "}", ">", ";;", "||", "&&", ">>", ">&",
            "))",
        ] {
            let matched = LogosTok::lexer(src).next();
            assert!(
                matches!(matched, Some(Ok(_))),
                "logos failed to match dispatched input {:?}: {:?}",
                src,
                matched
            );
        }
    }
}
