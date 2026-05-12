use tree_sitter::Node;
use crate::diagnostic::Diagnostic;
use crate::rule::Rule;

/// SB5019: Numeric comparison operators in [[ ]] are rejected.
/// Use (( )) for arithmetic comparisons.
pub struct NumericTestRejected;

const NUMERIC_OPS: &[&str] = &["-gt", "-lt", "-ge", "-le", "-eq", "-ne"];

impl Rule for NumericTestRejected {
    fn node_kinds(&self) -> &'static [&'static str] {
        &["test_command"]
    }

    fn check(&self, node: Node, source: &[u8]) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        find_numeric_ops(node, source, &mut diags);
        diags
    }
}

fn find_numeric_ops(node: Node, source: &[u8], diags: &mut Vec<Diagnostic>) {
    if node.kind() == "binary_expression" {
        if let Some(op_node) = node.child_by_field_name("operator") {
            let op = op_node.utf8_text(source).unwrap_or("");
            if NUMERIC_OPS.contains(&op) {
                diags.push(Diagnostic::error(
                    op_node,
                    "SB5019",
                    format!(
                        "'{}' is a numeric operator — use (( )) for arithmetic comparisons, not [[ ]]",
                        op
                    ),
                ));
            }
        }
    }
    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_named() {
            find_numeric_ops(child, source, diags);
        }
    }
}
