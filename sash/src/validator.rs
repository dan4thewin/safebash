use tree_sitter::Node;
use crate::diagnostic::Diagnostic;
use crate::rule::Rule;

/// Walk the CST once, dispatching each node to all registered rules.
/// Returns collected diagnostics sorted by position.
pub fn validate(root: Node, source: &[u8], rules: &[Box<dyn Rule>]) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    walk(root, source, rules, &mut diags);
    diags.sort_by_key(|d| (d.line, d.col));
    diags
}

fn walk(node: Node, source: &[u8], rules: &[Box<dyn Rule>], diags: &mut Vec<Diagnostic>) {
    let kind = node.kind();
    for rule in rules {
        let kinds = rule.node_kinds();
        if kinds.is_empty() || kinds.contains(&kind) {
            diags.extend(rule.check(node, source));
        }
    }
    for child in node.children(&mut node.walk()) {
        walk(child, source, rules, diags);
    }
}
