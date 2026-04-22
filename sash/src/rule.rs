use tree_sitter::Node;
use crate::diagnostic::Diagnostic;

/// A validation rule that inspects CST nodes and emits diagnostics.
///
/// Rules are stateless — each `check` call receives a single node
/// and the source bytes.  The framework handles tree walking and
/// diagnostic collection.
///
/// To add a new rule:
///   1. Create a struct implementing this trait
///   2. Return the node kinds it cares about from `node_kinds()`
///   3. Implement the check logic in `check()`
///   4. Register it in `all_rules()`
pub trait Rule: Sync {
    /// Human-readable name for this rule.
    fn name(&self) -> &'static str;

    /// CST node kinds this rule wants to inspect.
    /// The framework only calls `check()` for nodes matching these kinds.
    /// Return `&[]` to receive every node (use sparingly).
    fn node_kinds(&self) -> &'static [&'static str];

    /// Inspect a node and return any diagnostics.
    fn check(&self, node: Node, source: &[u8]) -> Vec<Diagnostic>;
}
