use tree_sitter::Node;
use crate::diagnostic::Diagnostic;
use crate::rule::Rule;

/// SB5007: Mutations inside $(( )) (expression context) are not allowed.
/// Mutations (=, +=, ++, --) belong in (( )) statement context only.
pub struct ArithExprMutation;

impl Rule for ArithExprMutation {
    fn name(&self) -> &'static str { "arith-expr-mutation" }

    fn node_kinds(&self) -> &'static [&'static str] {
        &["arithmetic_expansion"]
    }

    fn check(&self, node: Node, source: &[u8]) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        find_mutations(node, source, &mut diags);
        diags
    }
}

fn find_mutations(node: Node, source: &[u8], diags: &mut Vec<Diagnostic>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "postfix_expression" => {
                let text = child.utf8_text(source).unwrap_or("?");
                diags.push(Diagnostic::error(
                    child,
                    "SB5007",
                    format!("mutation {} not allowed in $(( )) expression context — use (( )) statement instead", text),
                ));
            }
            "unary_expression" => {
                let text = child.utf8_text(source).unwrap_or("?");
                // Only reject ++ and -- prefix, not unary - or !
                if text.starts_with("++") || text.starts_with("--") {
                    diags.push(Diagnostic::error(
                        child,
                        "SB5007",
                        format!("mutation {} not allowed in $(( )) expression context — use (( )) statement instead", text),
                    ));
                } else {
                    // Recurse into unary expressions like -(x=1)
                    find_mutations(child, source, diags);
                }
            }
            "binary_expression" => {
                // Check if the operator is an assignment
                let text = child.utf8_text(source).unwrap_or("");
                if is_assignment_binop(child, source) {
                    diags.push(Diagnostic::error(
                        child,
                        "SB5007",
                        format!("mutation {} not allowed in $(( )) expression context — use (( )) statement instead", text),
                    ));
                } else {
                    // Recurse — nested mutations like x + (y = 1)
                    find_mutations(child, source, diags);
                }
            }
            _ => {
                find_mutations(child, source, diags);
            }
        }
    }
}

/// Check if a binary_expression uses an assignment operator.
/// Tree-sitter doesn't expose the operator as a named child,
/// so we look for assignment patterns in the text between operands.
fn is_assignment_binop(node: Node, source: &[u8]) -> bool {
    // Get positions of first and last named children (operands)
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor)
        .filter(|c| c.is_named())
        .collect();
    if children.len() < 2 {
        return false;
    }
    // The operator text is between the two operands
    let op_start = children[0].end_byte();
    let op_end = children[1].start_byte();
    if op_end <= op_start || op_end > source.len() {
        return false;
    }
    let op = std::str::from_utf8(&source[op_start..op_end]).unwrap_or("").trim();
    matches!(op, "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "<<=" | ">>=" | "&=" | "|=" | "^=")
}
