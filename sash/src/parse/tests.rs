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
}
