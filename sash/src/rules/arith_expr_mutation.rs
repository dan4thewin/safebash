use tree_sitter::Node;
use crate::diagnostic::Diagnostic;
use crate::rule::Rule;

/// SB5007: Mutations inside $(( )) or condition (( )) are not allowed.
/// Mutations (=, +=, ++, --) in $(( )) are always rejected.
/// Mutations in (( )) are rejected when in if/while/until condition position.
pub struct ArithExprMutation;

impl Rule for ArithExprMutation {
    fn name(&self) -> &'static str { "arith-expr-mutation" }

    fn node_kinds(&self) -> &'static [&'static str] {
        &["arithmetic_expansion", "arithmetic_command"]
    }

    fn check(&self, node: Node, source: &[u8]) -> Vec<Diagnostic> {
        // arithmetic_expansion $(( )) — always reject mutations
        if node.kind() == "arithmetic_expansion" {
            let mut diags = Vec::new();
            find_mutations(node, source, &mut diags, "$(( ))");
            return diags;
        }
        // arithmetic_command (( )) — reject prefix ++/-- always (DD#31.6)
        if node.kind() == "arithmetic_command" {
            let mut diags = Vec::new();
            find_prefix_incdec(node, source, &mut diags);
            // Condition vs statement position
            if in_condition_position(node) {
                // Count named children — multiple means comma expression
                let mut c = node.walk();
                let named: Vec<Node> = node.children(&mut c)
                    .filter(|ch| ch.is_named()).collect();
                if named.len() > 1 {
                    // Comma: only reject mutations in the LAST expression
                    let last = named[named.len() - 1];
                    find_mutations(last, source, &mut diags, "condition (( )) final expression");
                } else {
                    find_mutations(node, source, &mut diags, "condition (( ))");
                }
            } else {
                // Statement position: check for nested mutations (DD#31.8)
                find_nested_mutations(node, source, &mut diags);
            }
            return diags;
        }
        vec![]
    }
}


/// Check if a binary_expression uses comma operator.
fn is_comma_expression(node: Node, source: &[u8]) -> bool {
    if node.kind() != "binary_expression" { return false; }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).filter(|c| c.is_named()).collect();
    if children.len() < 2 { return false; }
    let op_start = children[0].end_byte();
    let op_end = children[1].start_byte();
    if op_end <= op_start || op_end > source.len() { return false; }
    let op = std::str::from_utf8(&source[op_start..op_end]).unwrap_or("").trim();
    op == ","
}

/// Comma in condition: (( setup, setup, test ))
/// Comma associates left: (( a, b, c )) = (( (a, b), c ))
/// Right child is always the final (test) expression — must not mutate.
fn check_comma_condition(node: Node, source: &[u8], diags: &mut Vec<Diagnostic>) {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).filter(|c| c.is_named()).collect();
    if children.len() < 2 { return; }
    let right = children[children.len() - 1];
    // Final expression must not have mutations
    find_mutations(right, source, diags, "condition (( )) final comma expression");
    // Non-final (left) — if also comma, recurse
    let left = children[0];
    if is_comma_expression(left, source) {
        check_comma_condition(left, source, diags);
    }
}

/// Statement position: top-level mutation is fine, but nested mutations
/// inside operands are rejected (DD#31.8). E.g. `(( x = y++ ))`.
fn find_nested_mutations(node: Node, source: &[u8], diags: &mut Vec<Diagnostic>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !child.is_named() { continue; }
        match child.kind() {
            "postfix_expression" => {} // top-level postfix fine
            "binary_expression" if is_assignment_binop(child, source) => {
                // Top-level assignment fine, but check RHS for nested mutations
                let mut inner = child.walk();
                let named: Vec<Node> = child.children(&mut inner).filter(|c| c.is_named()).collect();
                for operand in named.iter().skip(1) {
                    // Check operand itself (it might be a mutation like y++)
                    match operand.kind() {
                        "postfix_expression" | "unary_expression" => {
                            let text = operand.utf8_text(source).unwrap_or("?");
                            diags.push(Diagnostic::error(
                                *operand,
                                "SB5007",
                                format!("mutation {} not allowed in {} expression context — use (( )) statement instead", text, "statement (( ))"),
                            ));
                        }
                        "binary_expression" if is_assignment_binop(*operand, source) => {
                            let text = operand.utf8_text(source).unwrap_or("?");
                            diags.push(Diagnostic::error(
                                *operand,
                                "SB5007",
                                format!("mutation {} not allowed in {} expression context — use (( )) statement instead", text, "statement (( ))"),
                            ));
                        }
                        _ => {
                            // Recurse into sub-expressions
                            find_mutations(*operand, source, diags, "statement (( ))");
                        }
                    }
                }
            }
            "binary_expression" if is_comma_expression(child, source) => {
                find_nested_mutations(child, source, diags);
            }
            _ => {
                find_nested_mutations(child, source, diags);
            }
        }
    }
}

fn find_mutations(node: Node, source: &[u8], diags: &mut Vec<Diagnostic>, ctx: &str) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "postfix_expression" => {
                let text = child.utf8_text(source).unwrap_or("?");
                diags.push(Diagnostic::error(
                    child,
                    "SB5007",
                    format!("mutation {} not allowed in {} expression context — use (( )) statement instead", text, ctx),
                ));
            }
            "unary_expression" => {
                let text = child.utf8_text(source).unwrap_or("?");
                if text.starts_with("++") || text.starts_with("--") {
                    diags.push(Diagnostic::error(
                        child,
                        "SB5007",
                        format!("mutation {} not allowed in {} expression context — use (( )) statement instead", text, ctx),
                    ));
                } else {
                    find_mutations(child, source, diags, ctx);
                }
            }
            "binary_expression" => {
                let text = child.utf8_text(source).unwrap_or("");
                if is_assignment_binop(child, source) {
                    diags.push(Diagnostic::error(
                        child,
                        "SB5007",
                        format!("mutation {} not allowed in {} expression context — use (( )) statement instead", text, ctx),
                    ));
                } else {
                    find_mutations(child, source, diags, ctx);
                }
            }
            _ => {
                find_mutations(child, source, diags, ctx);
            }
        }
    }
}

/// Check if an arithmetic_command is in condition position (if/while/until).
fn in_condition_position(node: Node) -> bool {
    // Walk up the tree. If we hit do_group before if/while/until, we're in a body.
    // If we hit if/while/until without passing through do_group, we're in a condition.
    let mut current = node.parent();
    let mut inside_do_group = false;
    while let Some(n) = current {
        match n.kind() {
            "do_group" => { inside_do_group = true; }
            "if_statement" | "elif_clause" => return !inside_do_group,
            "while_statement" | "until_statement" => return !inside_do_group,
            "function_definition" | "subshell" | "program" => return false,
            _ => {}
        }
        current = n.parent();
    }
    false
}

fn has_ancestor_kind(node: Node, kind: &str) -> bool {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == kind { return true; }
        current = n.parent();
    }
    false
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

/// SB5008: Arithmetic command (( )) in statement position must contain a mutation.
/// Pure comparisons like `(( x > 0 ))` are dead code — use `if (( x > 0 ))` instead.
pub struct ArithStmtNoEffect;

impl Rule for ArithStmtNoEffect {
    fn name(&self) -> &'static str { "arith-stmt-no-effect" }

    fn node_kinds(&self) -> &'static [&'static str] {
        &["arithmetic_command"]
    }

    fn check(&self, node: Node, source: &[u8]) -> Vec<Diagnostic> {
        // Only check statement position (not condition)
        if in_condition_position(node) {
            return vec![];
        }
        // Check if there's at least one mutation
        if has_mutation(node, source) {
            return vec![];
        }
        let text = node.utf8_text(source).unwrap_or("(( ... ))");
        vec![Diagnostic::error(
            node,
            "SB5008",
            format!("arithmetic statement {} has no side effect — use `if {}` for comparisons", text, text),
        )]
    }
}

fn has_mutation(node: Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "postfix_expression" => return true,
            "unary_expression" => {
                let text = child.utf8_text(source).unwrap_or("");
                if text.starts_with("++") || text.starts_with("--") {
                    return true;
                }
            }
            "binary_expression" => {
                if is_assignment_binop(child, source) {
                    return true;
                }
            }
            _ => {}
        }
        if has_mutation(child, source) {
            return true;
        }
    }
    false
}

/// Find prefix ++/-- anywhere in an arithmetic tree (banned per DD#31.6).
fn find_prefix_incdec(node: Node, source: &[u8], diags: &mut Vec<Diagnostic>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "unary_expression" {
            let text = child.utf8_text(source).unwrap_or("");
            if text.starts_with("++") || text.starts_with("--") {
                diags.push(Diagnostic::error(
                    child,
                    "SB5007",
                    format!("prefix {} is not allowed — use postfix (x++) or (( x += 1 )) instead", text),
                ));
                continue;
            }
        }
        find_prefix_incdec(child, source, diags);
    }
}
