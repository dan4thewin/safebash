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

    fn node_kinds(&self) -> &'static [&'static str] {
        &["command", "for_statement"]
    }

    fn check(&self, node: Node, source: &[u8]) -> Vec<Diagnostic> {
        // Reject 'select' (tree-sitter parses as for_statement variant)
        if node.kind() == "for_statement" {
            if let Some(kw) = node.child(0) {
                if kw.utf8_text(source).unwrap_or("") == "select" {
                    return vec![Diagnostic::error(
                        node,
                        "SB5011",
                        "'select' is not supported in sash".to_string(),
                    )];
                }
            }
            return vec![];
        }

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

        // DD#23: `command -v` rejected — command resolution is compile-time only
        if name == "command" {
            // Check if -v flag is present
            let mut cursor = node.walk();
            let mut has_v = false;
            if cursor.goto_first_child() {
                loop {
                    let child = cursor.node();
                    if child.utf8_text(source).unwrap_or("") == "-v" {
                        has_v = true;
                        break;
                    }
                    if !cursor.goto_next_sibling() { break; }
                }
            }
            if has_v {
                return vec![Diagnostic::error(
                    name_node,
                    "SB5011",
                    "'command -v' is not available in sash — command resolution is compile-time only".to_string(),
                )];
            }
            return vec![];
        }

        // Known keywords/special forms handled elsewhere
        if matches!(name,
            "eval" | "source" | "." | "alias" | "trap" | "signal"
            | "local" | "set"
            | "shopt" | "bless" | "getenv" | "fn"
            | "let"
            | "status" | "getcpid" | "getpid" | "getppid" | "getcwd" | "argc" | "argv" | "signum"
        ) {
            return vec![];
        }

        // Check if it's a function defined in this file
        let root = find_root(node);
        if is_function_name(root, name, source) {
            return vec![];
        }

        // DD#39: var =~ s/pat/rep/ is regex mutation, not a command
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "=~" {
                return vec![];
            }
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
    // DD#18: also check for local function variant
    is_function_name_walk(root, &mut cursor, name, source)
        || {
            let local_name = format!("__local__{}", name);
            let mut cursor2 = root.walk();
            is_function_name_walk(root, &mut cursor2, &local_name, source)
        }
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
