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

    #[test]
    fn test_eval_rejected() {
        let result = parse("eval echo hi\n");
        assert!(result.is_err());
        let diags = result.unwrap_err();
        assert!(diags.iter().any(|d| d.code == "SB4001"));
    }

    #[test]
    fn test_export_rejected() {
        let result = parse("export FOO=bar\n");
        assert!(result.is_err());
        let diags = result.unwrap_err();
        assert!(diags.iter().any(|d| d.code == "SB5001"));
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
}
