use tree_sitter::Node;
use crate::diagnostic::Diagnostic;
use crate::rule::Rule;

/// SB5011: Bare commands must be builtins or absolute paths.
/// sash does not resolve commands via PATH at runtime.
pub struct CommandResolution;

const BUILTINS: &[&str] = &[
    "echo", "printf", "cd", "pwd", "wait", "shift", "true", "false", ":",
    "read", "mapfile", "readarray", "unset", "test", "[", "[[",
    "break", "continue", "return", "exit",
];

impl Rule for CommandResolution {
    fn name(&self) -> &'static str { "command-resolution" }

    fn node_kinds(&self) -> &'static [&'static str] {
        &["command"]
    }

    fn check(&self, node: Node, source: &[u8]) -> Vec<Diagnostic> {
        let name_node = match node.child_by_field_name("name") {
            Some(n) => n,
            None => return vec![],
        };
        let name = match name_node.utf8_text(source) {
            Ok(s) => s,
            Err(_) => return vec![],
        };

        // Builtins
        if BUILTINS.contains(&name) {
            return vec![];
        }

        // Absolute or relative path
        if name.contains('/') {
            return vec![];
        }

        // Known keywords/special forms handled elsewhere
        if matches!(name,
            "eval" | "source" | "." | "alias" | "command" | "trap" | "signal"
            | "local" | "export" | "declare" | "typeset" | "readonly" | "set"
            | "shopt" | "select" | "coproc" | "bless" | "getenv" | "fn"
            | "let"
        ) {
            return vec![];
        }

        // Check if it's a function defined in this file
        let root = find_root(node);
        if is_function_name(root, name, source) {
            return vec![];
        }

        vec![Diagnostic::error(
            name_node,
            "SB5011",
            format!("unresolved command '{}' — use an absolute path (e.g. /usr/bin/{})", name, name),
        )]
    }
}

fn find_root(node: Node) -> Node {
    let mut n = node;
    while let Some(p) = n.parent() {
        n = p;
    }
    n
}

fn is_function_name(root: Node, name: &str, source: &[u8]) -> bool {
    let mut cursor = root.walk();
    is_function_name_walk(root, &mut cursor, name, source)
}

fn is_function_name_walk<'a>(node: Node<'a>, cursor: &mut tree_sitter::TreeCursor<'a>, name: &str, source: &[u8]) -> bool {
    if node.kind() == "function_definition" {
        if let Some(name_node) = node.child_by_field_name("name") {
            if name_node.utf8_text(source).unwrap_or("") == name {
                return true;
            }
        }
    }
    if cursor.goto_first_child() {
        loop {
            if is_function_name_walk(cursor.node(), cursor, name, source) {
                return true;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
    false
}
