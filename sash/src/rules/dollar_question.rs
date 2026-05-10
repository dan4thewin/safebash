use tree_sitter::Node;
use crate::diagnostic::Diagnostic;
use crate::rule::Rule;

/// SB5012: $? is only valid after a command in the same scope.
///
/// DD#55 restricts $? availability:
/// - After `- cmd`: compile error (suppression means you don't care about status)
/// - With no preceding command: compile error ($? is undefined)
/// - After a bare command: valid (always 0 since nonzero aborts)
/// - In || RHS, if/while body: valid (handling position)
pub struct DollarQuestion;

impl Rule for DollarQuestion {
    fn name(&self) -> &'static str { "dollar-question" }

    fn node_kinds(&self) -> &'static [&'static str] {
        &["simple_expansion"]
    }

    fn check(&self, node: Node, source: &[u8]) -> Vec<Diagnostic> {
        // Check if this simple_expansion references $?
        // CST: simple_expansion -> "$" special_variable_name("?")
        let mut is_dollar_question = false;
        for child in node.children(&mut node.walk()) {
            if child.kind() == "special_variable_name" {
                if child.utf8_text(source).unwrap_or("") == "?" {
                    is_dollar_question = true;
                }
            }
        }
        if !is_dollar_question {
            return vec![];
        }

        // Walk up to find the statement-level ancestor (direct child of program,
        // compound_statement, subshell, or similar block)
        let stmt = match find_statement_ancestor(node) {
            Some(s) => s,
            None => {
                return vec![Diagnostic::error(
                    node,
                    "SB5012",
                    "$? is not available here — no preceding command".to_string(),
                )];
            }
        };

        // Check if there's a preceding command-like sibling
        if !has_preceding_command(stmt) {
            return vec![Diagnostic::error(
                node,
                "SB5012",
                "$? is not available here — no preceding command".to_string(),
            )];
        }

        vec![]
    }
}

/// Walk up the tree to find the statement-level node (direct child of a block).
fn find_statement_ancestor(node: Node) -> Option<Node> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        let pk = parent.kind();
        if matches!(pk,
            "program" | "compound_statement" | "subshell"
            | "do_group" | "then" | "else"
        ) {
            return Some(current);
        }
        current = parent;
    }
    None
}

/// Check if the given statement node has a preceding sibling that is a
/// command, pipeline, or other construct that sets $?.
fn has_preceding_command(stmt: Node) -> bool {
    let mut sibling = stmt.prev_named_sibling();
    while let Some(sib) = sibling {
        let kind = sib.kind();
        if matches!(kind,
            "command" | "pipeline" | "subshell" | "if_statement"
            | "while_statement" | "for_statement" | "case_statement"
            | "negated_command" | "list"
        ) {
            return true;
        }
        // A variable_assignment with command_substitution also counts
        // (e.g. x=$(cmd); echo $?)
        if kind == "variable_assignment" || kind == "declaration_command" {
            return true;
        }
        // Function definitions don't set $? but function calls (commands) do
        if kind == "function_definition" {
            // skip, keep looking
            sibling = sib.prev_named_sibling();
            continue;
        }
        // Any other statement type — keep looking
        sibling = sib.prev_named_sibling();
    }
    false
}
