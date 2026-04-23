//! SB5002: reject command substitution inside heredoc body
//!
//! $(cmd) inside heredocs creates untraceable taint — the external
//! command output flows directly to another external command's stdin
//! with no named variable to bless. Extract to a variable and bless first.

use crate::diagnostic::Diagnostic;
use crate::rule::Rule;

pub struct HeredocCmdSub;

impl Rule for HeredocCmdSub {
    fn name(&self) -> &'static str {
        "heredoc-cmdsub"
    }

    fn node_kinds(&self) -> &'static [&'static str] {
        &["heredoc_body"]
    }

    fn check(&self, node: tree_sitter::Node, source: &[u8]) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        let mut cursor = node.walk();
        walk_for_cmdsub(&node, &mut cursor, source, &mut diags);
        diags
    }
}

fn walk_for_cmdsub(
    _node: &tree_sitter::Node,
    cursor: &mut tree_sitter::TreeCursor,
    source: &[u8],
    diags: &mut Vec<Diagnostic>,
) {
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "command_substitution" {
                let text = child.utf8_text(source).unwrap_or("$(...)");
                diags.push(Diagnostic::error(
                    child,
                    "SB5002",
                    format!(
                        "command substitution {} inside heredoc — extract to a variable and bless before use",
                        text
                    ),
                ));
            } else {
                walk_for_cmdsub(&child, cursor, source, diags);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}
