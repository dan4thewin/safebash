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
    // ── DD#93 Part C: content-shape tests `i?` / `p?` / `g?` ──────────────
    // General unary test operators (usable anywhere a unary test is), spelled
    // with a `?` suffix to stay out of bash's saturated `-X` namespace.

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

    // ── DD#93 Part D: `attest SOURCE (( TARGET = SOURCE ))` ───────────────
    // The int-assignment is the predicate: the conversion can only succeed on
    // a valid, representable i64, so performing it proves the property.

    /// `ir::intrinsic_call_name` is the inverse of `expr::intrinsic_expansion`,
    /// and the two live in different modules — the recogniser has to re-derive
    /// the shape the constructor builds. SB5012 goes silently vacuous if they
    /// ever disagree, so pin them together.
    #[test]
    fn intrinsic_expansion_round_trips() {
        use crate::ir::Span;
        for name in ["getpid", "getcpid", "status"] {
            let expr = crate::parse::expr::intrinsic_expansion(name, Span::new(1, 1));
            assert_eq!(
                crate::ir::intrinsic_call_name(&expr),
                Some(name),
                "intrinsic_call_name must recognise what intrinsic_expansion builds for `{name}`"
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
            ("(", LogosTok::LParen),
            (")", LogosTok::RParen),
            ("{", LogosTok::LBrace),
            ("}", LogosTok::RBrace),
            (";;", LogosTok::DblSemi),
            (";", LogosTok::Semi),
        ];
        for (src, want) in cases {
            let toks = tokens_of(src);
            assert_eq!(toks.len(), 1, "input {:?} produced {:?}", src, toks);
            assert_eq!(toks[0].2, want, "input {:?}", src);
            assert_eq!(toks[0].0, 0);
            assert_eq!(toks[0].1, src.len());
        }
    }

    /// `emit_logos_token` intercepts `Comment` before `to_tok` because it
    /// needs the matched slice. If that interception is ever removed the
    /// comment would silently lose its payload, so the arm is a loud ICE.
    /// Pins it, per the regression-guard carve-out.
    #[test]
    #[should_panic(expected = "intercepts LogosTok::Comment")]
    fn to_tok_comment_is_loud_ice() {
        let _ = to_tok(LogosTok::Comment);
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
        for src in [
            "if x { echo a; } else { echo b; }",
            "echo \"a$(((1+2)))b\"\"$((3+4))\" end",
            "echo \"$((1 + # )) ignored\n2))\" end",
        ] {
            for lookahead in [0, 1, 3] {
                assert_scanner_lookahead(src, lookahead);
            }
        }
    }

    fn assert_scanner_lookahead(src: &str, lookahead: usize) {
        use crate::parse::lexer::{Scanner, Tok};

        let mut direct = Scanner::new(src);
        let mut peeked = Scanner::new(src);
        let mut last_end = 0;
        loop {
            let d = direct.next();
            peeked.peek_n(lookahead);
            assert_eq!(peeked.next_is_adjacent(), d.span.start == last_end);
            let p = peeked.next();
            assert_eq!(d, p, "{src:?}, lookahead={lookahead}");
            last_end = d.span.end;
            if matches!(d.token, Tok::Eof) {
                break;
            }
        }
    }

    /// `wrap_pipeline_part` reaches `redirect_reject_noun` only when a
    /// trailing redirect is still pending, and every kind that can be holding
    /// one there is a kind `noncommand_redirect_target` names — so the `None`
    /// arm is a should-never-run ICE, not a branch any source text can take.
    /// A command is the case that proves it: it absorbs its own redirects in
    /// `parse_simple_command`, so seeing one here means that absorption
    /// stopped happening and the redirect is about to be dropped on the floor.
    /// Confirms the guard panics rather than returning a noun.
    #[test]
    fn redirect_reject_noun_is_loud_ice() {
        use crate::ir::{Command, Expr, ExprKind, Span, StmtKind};

        let span = Span::new(1, 1);
        let kind = StmtKind::Command(Command {
            name: Expr {
                kind: ExprKind::Literal("echo".to_string()),
                span,
            },
            args: vec![],
        });
        let caught = std::panic::catch_unwind(|| crate::parse::parser::redirect_reject_noun(&kind));
        assert!(
            caught.is_err(),
            "a redirect surviving to wrap_pipeline_part on a command must raise a loud ICE"
        );
    }

    // Four should-never-run guards, each unreachable from any source text
    // because a layer above already enforced the invariant. DG's rule for
    // these (2026-09-19): 100% coverage is the union of spec-test and
    // unit-test reachability, unit tests may bypass the other layers, and
    // anything that should be unreachable must halt with an ICE if it ever
    // ran. These are the unit half of that — the only way to execute the arm.

    /// `take_sed_stmt_payload` returns `None` unless the payload opens `s/`,
    /// so the parser's strip can only fail if scanner and parser have drifted.
    #[test]
    fn sed_prefix_missing_is_loud_ice() {
        let caught = std::panic::catch_unwind(|| crate::parse::parser::strip_sed_prefix("x/a/b/"));
        assert!(
            caught.is_err(),
            "a sed payload without the scanner-required `s/` must raise a loud ICE"
        );
    }

    /// `parse_word_arm` only routes words that `starts_with("@{")`, so the
    /// prefix strip cannot fail.
    #[test]
    fn at_brace_without_opener_is_loud_ice() {
        use crate::ir::Span;
        let caught = std::panic::catch_unwind(|| {
            crate::parse::expr::parse_at_brace_disposition("a:-x}", Span::new(1, 1))
        });
        assert!(
            caught.is_err(),
            "a word that does not open `@{{` must raise a loud ICE, not SB5033"
        );
    }

    /// The lexer's `@{…}` scan consumes through the matching `}` or fatals
    /// with SB4057, so a word that opens `@{` always closes.
    #[test]
    fn at_brace_without_closer_is_loud_ice() {
        use crate::ir::Span;
        let caught = std::panic::catch_unwind(|| {
            crate::parse::expr::parse_at_brace_disposition("@{a:-x", Span::new(1, 1))
        });
        assert!(
            caught.is_err(),
            "a word that opens `@{{` without closing must raise a loud ICE, not SB5033"
        );
    }
}
