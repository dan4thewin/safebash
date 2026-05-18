use crate::diagnostic::Diagnostic;
use crate::rule::Rule;
use tree_sitter::Node;

/// SB5009: IFS assignment is not allowed (word splitting doesn't exist in sash).
pub struct IfsRejected;

impl Rule for IfsRejected {
    fn node_kinds(&self) -> &'static [&'static str] {
        &["variable_assignment"]
    }

    fn check(&self, node: Node, source: &[u8]) -> Vec<Diagnostic> {
        // Check if the variable name is IFS
        if let Some(name_node) = node.child_by_field_name("name") {
            let name = name_node.utf8_text(source).unwrap_or("");
            if name == "IFS" {
                return vec![Diagnostic::error(
                    node,
                    "SB5009",
                    "IFS does not exist in sash — word splitting is eliminated by design"
                        .to_string(),
                )];
            }
        }
        vec![]
    }
}

/// SB5010: $var in for-in list — sash does not word-split.
/// Without word splitting, `for x in $var` iterates once over the whole string.
/// Use `array arr=(...)` and `for x in @arr`.
pub struct ForInExpansion;

impl Rule for ForInExpansion {
    fn node_kinds(&self) -> &'static [&'static str] {
        &["for_statement"]
    }

    fn check(&self, node: Node, source: &[u8]) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        // Walk the word list (children between "in" and ";" or do)
        let mut cursor = node.walk();
        let mut in_word_list = false;
        for child in node.children(&mut cursor) {
            let kind = child.kind();
            if kind == "in" {
                in_word_list = true;
                continue;
            }
            if kind == "do_group" || kind == ";" {
                break;
            }
            if in_word_list {
                // Check for unquoted expansions in word items
                check_unquoted_expansion(child, source, &mut diags);
            }
        }
        diags
    }
}

fn check_unquoted_expansion(node: Node, source: &[u8], diags: &mut Vec<Diagnostic>) {
    // If this word contains a bare simple_expansion (not inside quotes), reject
    let kind = node.kind();
    if kind == "simple_expansion" || kind == "expansion" {
        // Check if parent is string (quoted) — if so, it's fine
        if let Some(parent) = node.parent() {
            if parent.kind() == "string" || parent.kind() == "string_content" {
                return;
            }
        }
        let text = node.utf8_text(source).unwrap_or("$var");
        diags.push(Diagnostic::error(
            node,
            "SB5010",
            format!("{} in for-in list — sash does not word-split. Use array arr=(...) and for x in @arr", text),
        ));
        return;
    }
    // Check for "${arr[@]}" — this IS fine (expands to multiple items)
    if kind == "string" {
        return; // Quoted — OK
    }
    // Recurse into concatenation etc.
    if kind == "concatenation" || kind == "word" {
        let mut c = node.walk();
        for child in node.children(&mut c) {
            check_unquoted_expansion(child, source, diags);
        }
    }
}
