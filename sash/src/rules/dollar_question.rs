use tree_sitter::Node;
use crate::diagnostic::Diagnostic;
use crate::rule::Rule;

/// SB5012: $? is only valid after a command in the same scope.
///
/// DD#55 restricts $? availability:
/// - After `- cmd`: compile error (suppression means you don't care about status)
/// - With no preceding command: compile error ($? is undefined)
/// - After a bare command: valid (always 0 since nonzero aborts)
/// - In || RHS, if/while body: valid (handling position, first expression only)
pub struct DollarQuestion;

/// Result of checking handling position.
enum HandlingCheck {
    /// Not in any handling position — use normal $? rules
    NotHandling,
    /// In handling position and $? is the first expression — valid
    ValidFirst,
    /// In handling position but $? is NOT the first expression — error
    StaleStatus,
}

impl Rule for DollarQuestion {
    fn name(&self) -> &'static str { "dollar-question" }

    fn node_kinds(&self) -> &'static [&'static str] {
        &["simple_expansion", "command_substitution"]
    }

    fn check(&self, node: Node, source: &[u8]) -> Vec<Diagnostic> {
        // Check for $? (simple_expansion -> special_variable_name("?"))
        // or $(status) (command_substitution -> command -> "status")
        let is_dollar_question = if node.kind() == "simple_expansion" {
            node.children(&mut node.walk()).any(|child| {
                child.kind() == "special_variable_name"
                    && child.utf8_text(source).unwrap_or("") == "?"
            })
        } else if node.kind() == "command_substitution" {
            // Check for $(status) — single bare command named "status"
            let inner_stmts: Vec<_> = node.children(&mut node.walk())
                .filter(|c| c.is_named())
                .collect();
            inner_stmts.len() == 1
                && inner_stmts[0].kind() == "command"
                && inner_stmts[0].child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(|n| n == "status")
                    .unwrap_or(false)
                && inner_stmts[0].children(&mut inner_stmts[0].walk())
                    .filter(|c| c.is_named() && c.kind() != "command_name")
                    .count() == 0
        } else {
            false
        };
        if !is_dollar_question {
            return vec![];
        }

        // DD#55: check handling position (|| RHS, if/while body)
        match check_handling_position(node) {
            HandlingCheck::ValidFirst => return vec![],
            HandlingCheck::StaleStatus => {
                return vec![Diagnostic::error(
                    node,
                    "SB5014",
                    "$?/$(status) is only valid as the first expression in a handling branch — another command has already executed".to_string(),
                )];
            }
            HandlingCheck::NotHandling => {}
        }

        // Walk up to find the statement-level ancestor
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
        match check_preceding_command(stmt) {
            Err(()) => {
                return vec![Diagnostic::error(
                    node,
                    "SB5013",
                    "$? is not available after `- cmd` — suppression discards exit status".to_string(),
                )];
            }
            Ok(false) => {
                return vec![Diagnostic::error(
                    node,
                    "SB5012",
                    "$? is not available here — no preceding command".to_string(),
                )];
            }
            Ok(true) => {}
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
fn check_preceding_command(stmt: Node) -> Result<bool, ()> {
    let mut sibling = stmt.prev_named_sibling();
    while let Some(sib) = sibling {
        let kind = sib.kind();
        if kind == "suppressed_command" {
            return Err(());
        }
        if matches!(kind,
            "command" | "pipeline" | "subshell" | "if_statement"
            | "while_statement" | "for_statement" | "case_statement"
            | "negated_command" | "list"
        ) {
            return Ok(true);
        }
        if kind == "variable_assignment" || kind == "declaration_command" {
            return Ok(true);
        }
        if kind == "function_definition" {
            sibling = sib.prev_named_sibling();
            continue;
        }
        sibling = sib.prev_named_sibling();
    }
    Ok(false)
}

/// DD#55: Check if $? is in a handling position and whether it's the
/// first expression (before any other command has executed).
fn check_handling_position(node: Node) -> HandlingCheck {
    let mut current = node;
    while let Some(parent) = current.parent() {
        let pk = parent.kind();
        if pk == "list" {
            // Check if we're in the RHS of || or &&
            let mut found_operator = false;
            for child in parent.children(&mut parent.walk()) {
                let ck = child.kind();
                if ck == "||" || ck == "&&" {
                    found_operator = true;
                }
                if found_operator && child.id() == current.id() {
                    // We're in the RHS — check first-expression rule
                    if has_preceding_command_in_scope(node) {
                        return HandlingCheck::StaleStatus;
                    }
                    return HandlingCheck::ValidFirst;
                }
            }
        }
        if pk == "then" || pk == "else" {
            if let Some(if_node) = parent.parent() {
                if if_node.kind() == "if_statement" || if_node.kind() == "elif_clause" {
                    if has_preceding_command_in_scope(node) {
                        return HandlingCheck::StaleStatus;
                    }
                    return HandlingCheck::ValidFirst;
                }
            }
        }
        current = parent;
    }
    HandlingCheck::NotHandling
}

/// Check if there's any command-like statement before the one containing $?
/// within its immediate scope (compound_statement, etc.).
fn has_preceding_command_in_scope(dollar_q: Node) -> bool {
    let stmt = match find_statement_ancestor(dollar_q) {
        Some(s) => s,
        None => return false,
    };
    let mut sib = stmt.prev_named_sibling();
    while let Some(s) = sib {
        let kind = s.kind();
        if matches!(kind,
            "command" | "pipeline" | "subshell" | "if_statement"
            | "while_statement" | "for_statement" | "case_statement"
            | "negated_command" | "suppressed_command" | "list"
            | "variable_assignment" | "declaration_command"
        ) {
            return true;
        }
        sib = s.prev_named_sibling();
    }
    false
}
