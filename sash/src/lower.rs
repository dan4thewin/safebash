/// CST → IR lowering.
///
/// Converts tree-sitter-bash CST nodes into safebash IR types.
/// This module handles expression lowering (values on the RHS of
/// assignments, command arguments, etc.) and statement lowering
/// for all constructs that pass the safe-subset validator.

use tree_sitter::Node;
use crate::ir::*;

// ── Public entry point ────────────────────────────────────────

/// Lower a tree-sitter program node to an IR Program.
pub fn lower_program(root: Node, source: &[u8]) -> Program {
    assert_eq!(root.kind(), "program");
    Program {
        statements: lower_body(root, source),
    }
}

/// Lower all named children of a node as a statement list.
/// Used for program, function bodies, do_group, compound_statement, etc.
pub fn lower_body(node: Node, source: &[u8]) -> Vec<Statement> {
    let mut stmts = Vec::new();
    let children: Vec<Node> = node.children(&mut node.walk()).collect();
    let mut i = 0;
    while i < children.len() {
        let child = children[i];
        // Check if next sibling is "&" (background operator)
        let is_background = i + 1 < children.len() && children[i + 1].kind() == "&";
        if let Some(s) = lower_statement(child, source) {
            if is_background {
                // Wrap in a List with Background op
                let bg = Statement {
                    kind: StmtKind::List(List {
                        op: ListOp::Background,
                        left: Box::new(s),
                        right: Box::new(Statement {
                            kind: StmtKind::Command(Command {
                                name: Expr { kind: ExprKind::Literal("true".to_string()), span: Span::from_node(child) },
                                args: vec![],
                            }),
                            span: Span::from_node(child),
                        }),
                    }),
                    span: Span::from_node(child),
                };
                stmts.push(bg);
                i += 2; // skip the "&"
            } else {
                stmts.push(s);
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    stmts
}

// ── Statement lowering ────────────────────────────────────────

/// Lower a single CST node to an IR Statement, or None for
/// nodes we skip (semicolons, newlines, keywords, etc.).
fn lower_statement(node: Node, source: &[u8]) -> Option<Statement> {
    let span = Span::from_node(node);
    let kind = match node.kind() {
        "variable_assignment" => {
            StmtKind::Assignment(lower_assignment(node, source)?)
        }
        "declaration_command" => {
            StmtKind::Declaration(lower_declaration(node, source)?)
        }
        "command" => {
            // Check if command has redirect children (heredoc, herestring, file_redirect)
            let has_redirects = node.children(&mut node.walk()).any(|c| {
                matches!(c.kind(), "file_redirect" | "heredoc_redirect" | "herestring_redirect")
            });
            if has_redirects {
                // Wrap as Redirected statement
                let cmd = lower_command(node, source)?;
                let mut redirects = Vec::new();
                for child in node.children(&mut node.walk()) {
                    if matches!(child.kind(), "file_redirect" | "heredoc_redirect" | "herestring_redirect") {
                        redirects.push(lower_redirect(child, source));
                    }
                }
                StmtKind::Redirected(Redirected {
                    body: Box::new(Statement {
                        kind: StmtKind::Command(cmd),
                        span,
                    }),
                    redirects,
                })
            } else {
                StmtKind::Command(lower_command(node, source)?)
            }
        }
        "pipeline" => {
            StmtKind::Pipeline(lower_pipeline(node, source))
        }
        "list" => {
            return Some(lower_list(node, source));
        }
        "if_statement" => {
            StmtKind::If(lower_if(node, source))
        }
        "while_statement" => {
            StmtKind::While(lower_while(node, source))
        }
        "for_statement" => {
            StmtKind::For(lower_for(node, source))
        }
        "c_style_for_statement" => {
            return Some(lower_c_style_for(node, source));
        }
        "case_statement" => {
            StmtKind::Case(lower_case(node, source))
        }
        "function_definition" => {
            StmtKind::Function(lower_function(node, source))
        }
        "redirected_statement" => {
            StmtKind::Redirected(lower_redirected(node, source))
        }
        "negated_command" => {
            lower_negated(node, source)?
        }
        "subshell" => {
            StmtKind::Subshell(Subshell {
                body: lower_body(node, source),
            })
        }
        "compound_statement" => {
            StmtKind::Group(Group {
                body: lower_body(node, source),
            })
        }
        "unset_command" => {
            // unset var or unset map[key]
            // Parse as a command so transform can handle it
            let mut args = Vec::new();
            for child in node.children(&mut node.walk()) {
                if child.kind() != "unset" {
                    args.push(lower_expr(child, source));
                }
            }
            StmtKind::Command(Command {
                name: Expr {
                    kind: ExprKind::Literal("unset".to_string()),
                    span: span.clone(),
                },
                args,
            })
        }
        "test_command" => {
            // [[ expr ]] or [ expr ] as a standalone statement
            StmtKind::Command(Command {
                name: Expr {
                    kind: ExprKind::Literal("[[".to_string()),
                    span: span.clone(),
                },
                args: vec![lower_expr(node, source)],
            })
        }
        "comment" => {
            let text = node.utf8_text(source).unwrap_or("").to_string();
            StmtKind::Comment(text)
        }
        "arithmetic_command" => {
            return lower_arithmetic_command(node, source);
        }
        "{" | "}" | ";" | "do" | "done" | "then" | "fi" | "else" | "elif"
        | "in" | "esac" | ";;" => {
            // Structural tokens — not statements, skip silently
            return None;
        }
        _ => {
            eprintln!("UNKNOWN STMT KIND: {} text={:?}", node.kind(), node.utf8_text(source).unwrap_or("?"));
            return None;
        }
    };
    Some(Statement { kind, span })
}

// ── Assignment lowering ───────────────────────────────────────

fn lower_assoc_assignment(node: Node, source: &[u8]) -> Option<Assignment> {
    let name = node
        .child_by_field_name("name")?
        .utf8_text(source)
        .ok()?
        .to_string();

    // Find the array child and parse [key]=value pairs
    let val_node = node.child_by_field_name("value")?;
    let mut pairs = Vec::new();
    let mut cursor = val_node.walk();
    for child in val_node.children(&mut cursor) {
        if !child.is_named() { continue; }
        // Each element is a concatenation like ["[", "foo", "]", "=bar"]
        // or the full text is "[foo]=bar"
        let text = child.utf8_text(source).unwrap_or("").to_string();
        if let Some(rest) = text.strip_prefix('[') {
            if let Some(eq_pos) = rest.find("]=") {
                let key = &rest[..eq_pos];
                let val_raw = &rest[eq_pos + 2..];
                // Strip surrounding quotes if present
                let val = if (val_raw.starts_with('"') && val_raw.ends_with('"'))
                    || (val_raw.starts_with('\'') && val_raw.ends_with('\'')) {
                    &val_raw[1..val_raw.len()-1]
                } else {
                    val_raw
                };
                pairs.push((key.to_string(), Expr {
                    kind: ExprKind::Literal(val.to_string()),
                    span: Span::from_node(child),
                }));
            }
        }
    }

    Some(Assignment { index: None, op: AssignOp::Set,
        name,
        value: Expr {
            kind: ExprKind::AssocArray(pairs),
            span: Span::from_node(node),
        },
    })
}

fn lower_assignment(node: Node, source: &[u8]) -> Option<Assignment> {
    let mut name = String::new();
    let mut index = None;
    let mut op = AssignOp::Set;

    // Check for subscript (arr[i]=val, map[key]=val)
    for child in node.children(&mut node.walk()) {
        match child.kind() {
            "variable_name" => {
                name = child.utf8_text(source).ok()?.to_string();
            }
            "subscript" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    name = name_node.utf8_text(source).ok()?.to_string();
                }
                if let Some(idx_node) = child.child_by_field_name("index") {
                    index = Some(lower_expr(idx_node, source));
                }
            }
            "+=" => { op = AssignOp::Append; }
            _ => {}
        }
    }

    if name.is_empty() {
        // Fallback: try field-based extraction
        name = node.child_by_field_name("name")?.utf8_text(source).ok()?.to_string();
    }

    let value = if let Some(val_node) = node.child_by_field_name("value") {
        lower_expr(val_node, source)
    } else {
        Expr {
            kind: ExprKind::Literal(String::new()),
            span: Span::from_node(node),
        }
    };

    Some(Assignment { name, index, op, value })
}

// ── Declaration lowering ──────────────────────────────────────

fn lower_declaration(node: Node, source: &[u8]) -> Option<Declaration> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();

    let mut var_type = VarType::String;
    let mut flags = Vec::new();
    let keyword_node = children.first()?;
    let keyword = match keyword_node.kind() {
        "declare" | "typeset" => DeclKeyword::Declare,
        "local" => DeclKeyword::Local,
        "export" => DeclKeyword::Export,
        "readonly" => DeclKeyword::Readonly,
        "int" => {
            var_type = VarType::Int;
            DeclKeyword::Declare
        }
        "map" => {
            flags.push("-A".to_string());
            DeclKeyword::Declare
        }
        "array" => {
            flags.push("-a".to_string());
            DeclKeyword::Declare
        }
        _ => return None,
    };

    let mut assignments = Vec::new();

    for child in &children[1..] {
        match child.kind() {
            "type_annotation" => {
                var_type = VarType::Int;
            }
            "word" => {
                let text = child.utf8_text(source).unwrap_or("").to_string();
                if text == "-i" {
                    var_type = VarType::Int;
                } else if text.starts_with('-') {
                    flags.push(text);
                }
            }
            "variable_name" => {
                let text = child.utf8_text(source).unwrap_or("");
                if text == "int" {
                    var_type = VarType::Int;
                } else {
                    // Bare variable declaration like `array arr` or `declare x`
                    assignments.push(Assignment {
                        name: text.to_string(),
                        index: None,
                        op: AssignOp::Set,
                        value: Expr { kind: ExprKind::Literal(String::new()), span: Span { line: 0, col: 0 } },
                    });
                }
            }
            "variable_assignment" => {
                let is_assoc = flags.contains(&"-A".to_string());
                if is_assoc {
                    if let Some(a) = lower_assoc_assignment(*child, source) {
                        assignments.push(a);
                    }
                } else if let Some(a) = lower_assignment(*child, source) {
                    assignments.push(a);
                }
            }
            _ => {}
        }
    }

    Some(Declaration { keyword, var_type, flags, assignments })
}

// ── Command lowering ──────────────────────────────────────────

fn lower_command(node: Node, source: &[u8]) -> Option<Command> {
    let name_node = node.child_by_field_name("name")?;
    let name = lower_expr(name_node, source);

    // Detect `var =~ s/pattern/replacement/flags` regex mutation
    let mut has_regex_op = false;
    for child in node.children(&mut node.walk()) {
        if child.kind() == "=~" {
            has_regex_op = true;
            break;
        }
    }
    if has_regex_op {
        // Collect all named children after =~ as the regex expression
        let mut after_op = false;
        let mut regex_parts = Vec::new();
        for child in node.children(&mut node.walk()) {
            if child.kind() == "=~" { after_op = true; continue; }
            if after_op && child.is_named() {
                regex_parts.push(lower_expr(child, source));
            }
        }
        return Some(Command { name, args: regex_parts });
    }

    let mut args = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_named() && child != name_node {
            if let Some(field) = child_field_name(child, node) {
                if field == "argument" {
                    args.push(lower_expr(child, source));
                }
            }
        }
    }

    Some(Command { name, args })
}

// ── Pipeline lowering ─────────────────────────────────────────

fn lower_pipeline(node: Node, source: &[u8]) -> Pipeline {
    let mut commands = Vec::new();
    let mut negated = false;
    for child in node.children(&mut node.walk()) {
        // tree-sitter puts `!` as a negated_command wrapping the first
        // pipeline element.  Detect and unwrap it.
        if child.kind() == "negated_command" {
            negated = true;
            // Extract the actual command inside the negated_command
            for inner in child.children(&mut child.walk()) {
                if inner.is_named() && inner.kind() != "!" {
                    if let Some(s) = lower_statement(inner, source) {
                        commands.push(s);
                    }
                }
            }
        } else if child.is_named() {
            if let Some(s) = lower_statement(child, source) {
                commands.push(s);
            }
        }
    }
    Pipeline { commands, negated }
}

// ── List lowering (&&, ||) ────────────────────────────────────

fn lower_list(node: Node, source: &[u8]) -> Statement {
    let span = Span::from_node(node);
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();

    // Find the operator (&&, ||) — it's an anonymous node
    let mut op = ListOp::And;
    let mut op_idx = 0;
    for (i, child) in children.iter().enumerate() {
        match child.kind() {
            "&&" => { op = ListOp::And; op_idx = i; break; }
            "||" => { op = ListOp::Or; op_idx = i; break; }
            _ => {}
        }
    }

    // Left side: everything before the operator
    let left = children[..op_idx].iter()
        .filter_map(|c| lower_statement(*c, source))
        .next()
        .unwrap_or_else(|| Statement {
            kind: StmtKind::Command(Command {
                name: Expr { kind: ExprKind::Literal("?".to_string()), span: span.clone() },
                args: vec![],
            }),
            span: span.clone(),
        });

    // Right side: everything after the operator
    // Could be another list (chained &&/||) — handle recursively
    let right_nodes: Vec<Node> = children[op_idx + 1..].iter()
        .filter(|c| c.is_named())
        .copied()
        .collect();

    let right = if right_nodes.len() == 1 {
        lower_statement(right_nodes[0], source).unwrap_or_else(|| Statement {
            kind: StmtKind::Command(Command {
                name: Expr { kind: ExprKind::Literal("?".to_string()), span: span.clone() },
                args: vec![],
            }),
            span: span.clone(),
        })
    } else {
        // Multiple nodes on right — shouldn't happen in practice
        // since tree-sitter nests lists, but handle gracefully
        right_nodes.iter()
            .filter_map(|c| lower_statement(*c, source))
            .next()
            .unwrap_or_else(|| Statement {
                kind: StmtKind::Command(Command {
                    name: Expr { kind: ExprKind::Literal("?".to_string()), span: span.clone() },
                    args: vec![],
                }),
                span: span.clone(),
            })
    };

    Statement {
        kind: StmtKind::List(List {
            op,
            left: Box::new(left),
            right: Box::new(right),
        }),
        span,
    }
}

// ── Negated command (! cmd) ───────────────────────────────────

fn lower_negated(node: Node, source: &[u8]) -> Option<StmtKind> {
    // ! pipeline or ! command
    for child in node.children(&mut node.walk()) {
        if child.is_named() {
            if child.kind() == "pipeline" {
                let mut p = lower_pipeline(child, source);
                p.negated = true;
                return Some(StmtKind::Pipeline(p));
            }
            // ! single_command — wrap in a 1-element negated pipeline
            if let Some(stmt) = lower_statement(child, source) {
                return Some(StmtKind::Pipeline(Pipeline {
                    commands: vec![stmt],
                    negated: true,
                }));
            }
        }
    }
    None
}

// ── If lowering ───────────────────────────────────────────────

fn lower_if(node: Node, source: &[u8]) -> If {
    let mut condition = Vec::new();
    let mut then_body = Vec::new();
    let mut elif_clauses = Vec::new();
    let mut else_body = None;

    // States: gathering condition, then body, elif, else
    let mut in_then = false;

    for child in node.children(&mut node.walk()) {
        match child.kind() {
            "if" | "fi" | ";" => {}
            "then" => { in_then = true; }
            "elif_clause" => {
                elif_clauses.push(lower_elif(child, source));
            }
            "else_clause" => {
                else_body = Some(lower_body(child, source));
            }
            _ => {
                if !in_then {
                    // condition
                    if let Some(s) = lower_statement(child, source) {
                        condition.push(s);
                    }
                } else {
                    // then body
                    if let Some(s) = lower_statement(child, source) {
                        then_body.push(s);
                    }
                }
            }
        }
    }

    If { condition, then_body, elif_clauses, else_body }
}

fn lower_elif(node: Node, source: &[u8]) -> ElifClause {
    let mut condition = Vec::new();
    let mut body = Vec::new();
    let mut in_then = false;

    for child in node.children(&mut node.walk()) {
        match child.kind() {
            "elif" | ";" => {}
            "then" => { in_then = true; }
            _ => {
                if !in_then {
                    if let Some(s) = lower_statement(child, source) {
                        condition.push(s);
                    }
                } else {
                    if let Some(s) = lower_statement(child, source) {
                        body.push(s);
                    }
                }
            }
        }
    }

    ElifClause { condition, body }
}

// ── While/Until lowering ──────────────────────────────────────

fn lower_while(node: Node, source: &[u8]) -> While {
    let mut condition = Vec::new();
    let mut body = Vec::new();
    let mut until = false;

    for child in node.children(&mut node.walk()) {
        match child.kind() {
            "while" => { until = false; }
            "until" => { until = true; }
            "do_group" => {
                body = lower_body(child, source);
            }
            ";" => {}
            _ => {
                // condition nodes (before do_group)
                if let Some(s) = lower_statement(child, source) {
                    condition.push(s);
                }
            }
        }
    }

    While { condition, body, until }
}

// ── For lowering ──────────────────────────────────────────────

fn lower_for(node: Node, source: &[u8]) -> For {
    let variable = node
        .child_by_field_name("variable")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("?")
        .to_string();

    // Collect the word list (field "value" children)
    let mut words = Vec::new();
    let mut body = Vec::new();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "do_group" => {
                body = lower_body(child, source);
            }
            "for" | "in" | ";" | "variable_name" => {}
            _ => {
                if let Some(field) = child_field_name(child, node) {
                    if field == "value" {
                        words.push(lower_expr(child, source));
                    }
                }
            }
        }
    }

    For { variable, words, body }
}

// ── Case lowering ─────────────────────────────────────────────

fn lower_case(node: Node, source: &[u8]) -> Case {
    let value = node
        .child_by_field_name("value")
        .map(|n| lower_expr(n, source))
        .unwrap_or_else(|| Expr {
            kind: ExprKind::Literal("?".to_string()),
            span: Span::from_node(node),
        });

    let mut items = Vec::new();
    for child in node.children(&mut node.walk()) {
        if child.kind() == "case_item" {
            items.push(lower_case_item(child, source));
        }
    }

    Case { value, items }
}

fn lower_case_item(node: Node, source: &[u8]) -> CaseItem {
    let mut patterns = Vec::new();
    let mut body = Vec::new();
    let mut fallthrough = false;
    let mut resume = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            ")" | "|" => {}
            ";;" => { fallthrough = false; resume = false; }
            ";&" => { fallthrough = true; resume = false; }
            ";;&" => { fallthrough = false; resume = true; }
            _ => {
                if let Some(field) = child_field_name(child, node) {
                    match field {
                        "value" => {
                            patterns.push(lower_expr(child, source));
                        }
                        "termination" => {
                            let text = child.utf8_text(source).unwrap_or(";;");
                            fallthrough = text == ";&";
                            resume = text == ";;&";
                        }
                        _ => {
                            if let Some(s) = lower_statement(child, source) {
                                body.push(s);
                            }
                        }
                    }
                } else {
                    if let Some(s) = lower_statement(child, source) {
                        body.push(s);
                    }
                }
            }
        }
    }

    CaseItem { patterns, body, fallthrough, resume }
}

// ── Function lowering ─────────────────────────────────────────

fn lower_function(node: Node, source: &[u8]) -> Function {
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("?")
        .to_string();

    let return_type = if node.child_by_field_name("type").is_some() {
        VarType::Int
    } else {
        VarType::String
    };

    let body = node
        .child_by_field_name("body")
        .map(|b| lower_body(b, source))
        .unwrap_or_default();

    Function { name, return_type, body }
}

// ── Redirected statement lowering ─────────────────────────────

fn lower_redirected(node: Node, source: &[u8]) -> Redirected {
    let mut body_stmt = None;
    let mut redirects = Vec::new();

    for child in node.children(&mut node.walk()) {
        match child.kind() {
            "file_redirect" | "heredoc_redirect" | "herestring_redirect" => {
                let redir = lower_redirect(child, source);
                // Check if this is &> (tree-sitter gives &> as the operator)
                let mut is_ampersand_redirect = false;
                for gc in child.children(&mut child.walk()) {
                    if gc.kind() == "&>" {
                        is_ampersand_redirect = true;
                        break;
                    }
                }
                if is_ampersand_redirect {
                    // &>file desugars to >file 2>&1
                    let span = redir.target.span;
                    redirects.push(Redirect {
                        fd: Some(1),
                        op: RedirectOp::Write,
                        target: redir.target,
                    });
                    redirects.push(Redirect {
                        fd: Some(2),
                        op: RedirectOp::DupOutput,
                        target: Expr { kind: ExprKind::Literal("1".to_string()), span },
                    });
                } else {
                    redirects.push(redir);
                }
            }
            _ => {
                if body_stmt.is_none() {
                    if let Some(s) = lower_statement(child, source) {
                        body_stmt = Some(s);
                    }
                }
            }
        }
    }

    let body = body_stmt.unwrap_or_else(|| Statement {
        kind: StmtKind::Command(Command {
            name: Expr {
                kind: ExprKind::Literal("?".to_string()),
                span: Span::from_node(node),
            },
            args: vec![],
        }),
        span: Span::from_node(node),
    });

    Redirected {
        body: Box::new(body),
        redirects,
    }
}

fn lower_redirect(node: Node, source: &[u8]) -> Redirect {
    let mut fd = None;
    let mut op = RedirectOp::Write;
    let mut strip_tabs = false;
    let mut target = Expr {
        kind: ExprKind::Literal(String::new()),
        span: Span::from_node(node),
    };

    for child in node.children(&mut node.walk()) {
        match child.kind() {
            "file_descriptor" => {
                if let Ok(text) = child.utf8_text(source) {
                    fd = text.parse::<u32>().ok();
                }
            }
            ">" => op = RedirectOp::Write,
            ">>" => op = RedirectOp::Append,
            "<" => op = RedirectOp::Read,
            ">&" => op = RedirectOp::DupOutput,
            "<&" => op = RedirectOp::DupInput,
            "<>" => op = RedirectOp::ReadWrite,
            "<<" | "<<-" => {
                op = RedirectOp::HereDoc;
                if child.utf8_text(source).unwrap_or("") == "<<-" {
                    strip_tabs = true;
                }
            }
            "<<<" => op = RedirectOp::HereString,
            "heredoc_body" => {
                // Heredoc body — walk children for variable expansion
                // If it has named children (simple_expansion, etc.), treat like double-quoted
                if child.named_child_count() > 0 {
                    // Build parts like lower_string does
                    let mut parts = Vec::new();
                    let content_start = child.start_byte();
                    let content_end = child.end_byte();
                    let mut last_end = content_start;
                    for gc in child.children(&mut child.walk()) {
                        if !gc.is_named() { continue; }
                        let gc_start = gc.start_byte();
                        if gc_start > last_end {
                            let gap = std::str::from_utf8(&source[last_end..gc_start]).unwrap_or("");
                            if !gap.is_empty() {
                                parts.push(Expr { kind: ExprKind::Literal(gap.to_string()), span: Span::from_node(gc) });
                            }
                        }
                        parts.push(lower_expr(gc, source));
                        last_end = gc.end_byte();
                    }
                    if last_end < content_end {
                        let gap = std::str::from_utf8(&source[last_end..content_end]).unwrap_or("");
                        if !gap.is_empty() {
                            parts.push(Expr { kind: ExprKind::Literal(gap.to_string()), span: Span::from_node(child) });
                        }
                    }
                    target = Expr {
                        kind: if parts.len() == 1 { parts.into_iter().next().unwrap().kind } else { ExprKind::DoubleQuoted(parts) },
                        span: Span::from_node(child),
                    };
                } else {
                    // No expansions — literal text
                    let mut body_text = child.utf8_text(source).unwrap_or("").to_string();
                    if strip_tabs {
                        body_text = body_text.lines()
                            .map(|l| l.trim_start_matches('\t'))
                            .collect::<Vec<_>>()
                            .join("\n");
                        if child.utf8_text(source).unwrap_or("").ends_with('\n') {
                            body_text.push('\n');
                        }
                    }
                    target = Expr {
                        kind: ExprKind::Literal(body_text),
                        span: Span::from_node(child),
                    };
                }
            }
            "string" | "raw_string" | "word" if op == RedirectOp::HereString => {
                // Here-string: <<< "text" — bash adds trailing newline
                let mut text = match child.kind() {
                    "string" | "raw_string" => {
                        // Strip quotes
                        let raw = child.utf8_text(source).unwrap_or("");
                        if (raw.starts_with('"') && raw.ends_with('"')) || 
                           (raw.starts_with('\'') && raw.ends_with('\'')) {
                            raw[1..raw.len()-1].to_string()
                        } else {
                            raw.to_string()
                        }
                    }
                    _ => child.utf8_text(source).unwrap_or("").to_string(),
                };
                text.push('\n'); // bash adds trailing newline to here-strings
                target = Expr {
                    kind: ExprKind::Literal(text),
                    span: Span::from_node(child),
                };
            }
            _ => {
                if let Some(field) = child_field_name(child, node) {
                    if field == "destination" {
                        target = lower_expr(child, source);
                    }
                }
            }
        }
    }

    Redirect { fd, op, target }
}

// ── Helpers ───────────────────────────────────────────────────

fn child_field_name(child: Node, parent: Node) -> Option<&'static str> {
    let mut cursor = parent.walk();
    cursor.goto_first_child();
    loop {
        if cursor.node() == child {
            return cursor.field_name();
        }
        if !cursor.goto_next_sibling() {
            return None;
        }
    }
}

// ── Expression lowering ───────────────────────────────────────

/// Lower any CST expression node to an IR Expr.
fn lower_expr(node: Node, source: &[u8]) -> Expr {
    let span = Span::from_node(node);
    let kind = match node.kind() {
        "word" | "extglob_pattern" => {
            let text = node.utf8_text(source).unwrap_or("").trim().to_string();
            if text.starts_with("~/") {
                // Tilde expansion: ~/foo → $HOME/foo
                ExprKind::Concat(vec![
                    Expr { kind: ExprKind::SimpleExpansion("HOME".to_string()), span },
                    Expr { kind: ExprKind::Literal(text[1..].to_string()), span },
                ])
            } else if text == "~" {
                ExprKind::SimpleExpansion("HOME".to_string())
            } else if text.starts_with('@') && text.len() > 1 {
                // Sash-native: @arr → all array elements (DD#41)
                let name = text[1..].to_string();
                ExprKind::ParameterExpansion(ParameterExpansion {
                    name,
                    operator: Some("arrvals".to_string()),
                    operand: None,
                })
            } else {
                ExprKind::Literal(text)
            }
        }

        "string" => lower_string(node, source),

        "raw_string" => {
            let text = node.utf8_text(source).unwrap_or("''");
            let inner = &text[1..text.len().saturating_sub(1)];
            ExprKind::SingleQuoted(inner.to_string())
        }

        "ansi_c_string" => {
            // $'...' — compile-time desugaring of escape sequences
            let text = node.utf8_text(source).unwrap_or("$''");
            let inner = &text[2..text.len().saturating_sub(1)]; // strip $' and '
            let mut result = String::new();
            let bytes = inner.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    match bytes[i + 1] {
                        b'n' => { result.push('\n'); i += 2; }
                        b't' => { result.push('\t'); i += 2; }
                        b'r' => { result.push('\r'); i += 2; }
                        b'a' => { result.push('\x07'); i += 2; }
                        b'b' => { result.push('\x08'); i += 2; }
                        b'e' | b'E' => { result.push('\x1b'); i += 2; }
                        b'f' => { result.push('\x0c'); i += 2; }
                        b'v' => { result.push('\x0b'); i += 2; }
                        b'\\' => { result.push('\\'); i += 2; }
                        b'\'' => { result.push('\''); i += 2; }
                        b'"' => { result.push('"'); i += 2; }
                        b'0' => {
                            // Octal: \0NNN
                            let mut val = 0u8;
                            let mut j = i + 2;
                            let end = (j + 3).min(bytes.len());
                            while j < end && bytes[j] >= b'0' && bytes[j] <= b'7' {
                                val = val * 8 + (bytes[j] - b'0');
                                j += 1;
                            }
                            result.push(val as char);
                            i = j;
                        }
                        b'x' => {
                            // Hex: \xNN
                            let mut val = 0u8;
                            let mut j = i + 2;
                            let end = (j + 2).min(bytes.len());
                            while j < end {
                                let d = match bytes[j] {
                                    b'0'..=b'9' => bytes[j] - b'0',
                                    b'a'..=b'f' => bytes[j] - b'a' + 10,
                                    b'A'..=b'F' => bytes[j] - b'A' + 10,
                                    _ => break,
                                };
                                val = val * 16 + d;
                                j += 1;
                            }
                            result.push(val as char);
                            i = j;
                        }
                        _ => { result.push('\\'); result.push(bytes[i+1] as char); i += 2; }
                    }
                } else {
                    result.push(bytes[i] as char);
                    i += 1;
                }
            }
            ExprKind::Literal(result)
        }

        "string_content" => {
            let text = node.utf8_text(source).unwrap_or("").to_string();
            ExprKind::Literal(unescape_double_quoted(&text))
        }

        "simple_expansion" => {
            let var = node
                .child(1)
                .and_then(|c| c.utf8_text(source).ok())
                .unwrap_or("?")
                .to_string();
            ExprKind::SimpleExpansion(var)
        }

        "expansion" => lower_parameter_expansion(node, source),

        "command_substitution" => {
            let mut stmts = Vec::new();
            let mut is_file_slurp = false;
            let mut slurp_path = String::new();
            for child in node.children(&mut node.walk()) {
                if child.kind() == "file_redirect" {
                    // Check for $(<file) — file slurp pattern
                    let mut has_read = false;
                    let mut path = String::new();
                    let mut rc = child.walk();
                    for rchild in child.children(&mut rc) {
                        if rchild.kind() == "<" { has_read = true; }
                        if rchild.kind() == "word" || rchild.kind() == "string" || rchild.kind() == "concatenation" {
                            path = rchild.utf8_text(source).unwrap_or("").to_string();
                        }
                    }
                    if has_read && !path.is_empty() {
                        is_file_slurp = true;
                        slurp_path = path;
                    }
                } else if child.is_named() {
                    if let Some(s) = lower_statement(child, source) {
                        stmts.push(s);
                    }
                }
            }
            if is_file_slurp && stmts.is_empty() {
                // $(<file) → CommandSubstitution with a single `cat file` command
                ExprKind::CommandSubstitution(vec![
                    Statement {
                        kind: StmtKind::Command(Command {
                            name: Expr { kind: ExprKind::Literal("cat".to_string()), span: Span { line: 0, col: 0 } },
                            args: vec![Expr { kind: ExprKind::Literal(slurp_path), span: Span { line: 0, col: 0 } }],
                        }),
                        span: Span { line: 0, col: 0 },
                    }
                ])
            } else {
                ExprKind::CommandSubstitution(stmts)
            }
        }

        "arithmetic_expansion" => {
            // Walk tree-sitter's AST instead of text-parsing
            // For comma operator: multiple named children — use the last one
            let count = node.named_child_count();
            if count > 1 {
                // Comma operator: $((a, b, c)) → evaluate all, return last
                let last = node.named_child(count - 1).unwrap();
                lower_arith_expr_to_exprkind(last, source)
            } else if let Some(expr_node) = node.named_child(0) {
                lower_arith_expr_to_exprkind(expr_node, source)
            } else {
                // Fallback: extract text
                let text = node.utf8_text(source).unwrap_or("$((??))");
                let inner = text
                    .strip_prefix("$((")
                    .unwrap_or(text)
                    .strip_suffix("))")
                    .unwrap_or(text)
                    .trim()
                    .to_string();
                ExprKind::ArithmeticExpansion(inner)
            }
        }

        "concatenation" => {
            let mut parts = Vec::new();
            for child in node.children(&mut node.walk()) {
                if child.is_named() {
                    parts.push(lower_expr(child, source));
                }
            }
            // Sash-native: $arr[i] → Subscript (DD#41)
            // CST: concatenation(simple_expansion($arr), word([), number(i), word(]))
            // or:  concatenation(simple_expansion($arr), word([i]))
            let full_text = node.utf8_text(source).unwrap_or("");
            if parts.len() >= 2 {
                if let ExprKind::SimpleExpansion(ref name) = parts[0].kind {
                    // Check if remaining text after $name is [index]
                    let after_expansion = &full_text[name.len() + 1..]; // skip $name
                    if after_expansion.starts_with('[') && after_expansion.ends_with(']') {
                        let idx = &after_expansion[1..after_expansion.len()-1];
                        let index_expr = if idx == "@" || idx == "*" {
                            // $arr[@] → all values
                            return Expr {
                                kind: ExprKind::ParameterExpansion(ParameterExpansion {
                                    name: name.clone(),
                                    operator: Some("arrvals".to_string()),
                                    operand: None,
                                }),
                                span,
                            };
                        } else {
                            Expr {
                                kind: ExprKind::Literal(idx.to_string()),
                                span,
                            }
                        };
                        return Expr {
                            kind: ExprKind::Subscript {
                                name: name.clone(),
                                index: Box::new(index_expr),
                            },
                            span,
                        };
                    }
                }
            }
            ExprKind::Concat(parts)
        }

        "array" => {
            let mut elems = Vec::new();
            for child in node.children(&mut node.walk()) {
                if child.is_named() {
                    elems.push(lower_expr(child, source));
                }
            }
            ExprKind::Array(elems)
        }

        "command_name" => {
            if let Some(inner) = node.child(0) {
                return lower_expr(inner, source);
            }
            ExprKind::Literal("?".to_string())
        }

        "number" => {
            let text = node.utf8_text(source).unwrap_or("0").to_string();
            ExprKind::Literal(text)
        }

        "test_command" => {
            lower_test_command(node, source)
        }

        _ => {
            let text = node.utf8_text(source).unwrap_or("??").to_string();
            ExprKind::Literal(text)
        }
    };

    Expr { kind, span }
}

/// Lower a [[ ]] or [ ] test_command to a Test expression.
fn lower_test_command(node: Node, source: &[u8]) -> ExprKind {
    // Find the expression child (unary_expression, binary_expression, etc.)
    for child in node.children(&mut node.walk()) {
        if child.is_named() {
            match child.kind() {
                "unary_expression" | "binary_expression"
                | "negation_expression" | "parenthesized_expression" => {
                    return ExprKind::Test(lower_test_expr(child, source));
                }
                // Bare word/string in [[ ]] → implicit -n (non-empty test)
                "word" | "string" | "raw_string" | "string_content" |
                "simple_expansion" | "expansion" | "concatenation" => {
                    let expr = lower_expr(child, source);
                    return ExprKind::Test(TestExpr::Unary {
                        op: "-n".to_string(),
                        operand: Box::new(expr),
                    });
                }
                _ => {}
            }
        }
    }
    // Fallback: just capture the text
    let text = node.utf8_text(source).unwrap_or("??").to_string();
    ExprKind::Literal(text)
}

/// Lower a test expression node recursively.
fn lower_test_expr(node: Node, source: &[u8]) -> TestExpr {
    match node.kind() {
        "unary_expression" => {
            let op = node.child_by_field_name("operator")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("?")
                .to_string();
            if op == "!" {
                // [[ ! expr ]] — negation: operand is itself a test expression
                // The "!" is the operator (unnamed or field), inner expr is first named child after it
                let inner = node.children(&mut node.walk())
                    .filter(|c| c.is_named() && c.kind() != "test_operator")
                    .find(|c| c.start_byte() > node.child_by_field_name("operator").map(|n| n.end_byte()).unwrap_or(0))
                    .map(|c| lower_test_expr(c, source))
                    .unwrap_or_else(|| TestExpr::Unary {
                        op: "?".to_string(),
                        operand: Box::new(Expr { kind: ExprKind::Literal("?".to_string()), span: Span::from_node(node) }),
                    });
                TestExpr::Not(Box::new(inner))
            } else {
                // -z, -n, -f, etc. — standard unary test
                let operand = node.children(&mut node.walk())
                    .filter(|c| c.is_named())
                    .nth(1)
                    .map(|c| lower_expr(c, source))
                    .unwrap_or_else(|| Expr {
                        kind: ExprKind::Literal("?".to_string()),
                        span: Span::from_node(node),
                    });
                TestExpr::Unary { op, operand: Box::new(operand) }
            }
        }
        "binary_expression" => {
            // Check if left side is a negation: [[ ! expr == expr ]] means !(expr == expr)
            let left_node = node.child_by_field_name("left");
            let is_negated_binary = left_node
                .map(|n| n.kind() == "unary_expression" &&
                    n.child_by_field_name("operator")
                        .and_then(|o| o.utf8_text(source).ok())
                        .map(|o| o == "!")
                        .unwrap_or(false))
                .unwrap_or(false);

            if is_negated_binary {
                // Extract the real left operand from inside the unary_expression
                let left_unary = left_node.unwrap();
                let real_left = left_unary.children(&mut left_unary.walk())
                    .filter(|c| c.is_named() && c.kind() != "test_operator")
                    .find(|c| c.start_byte() > left_unary.child_by_field_name("operator").map(|n| n.end_byte()).unwrap_or(0))
                    .map(|c| lower_expr(c, source))
                    .unwrap_or_else(|| Expr {
                        kind: ExprKind::Literal("?".to_string()),
                        span: Span::from_node(left_unary),
                    });
                let op = node.child_by_field_name("operator")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("?")
                    .to_string();
                let right = node.child_by_field_name("right")
                    .map(|n| lower_expr(n, source))
                    .unwrap_or_else(|| Expr {
                        kind: ExprKind::Literal("?".to_string()),
                        span: Span::from_node(node),
                    });
                let inner = TestExpr::Binary { left: Box::new(real_left), op, right: Box::new(right) };
                TestExpr::Not(Box::new(inner))
            } else {
                let left = left_node
                    .map(|n| lower_expr(n, source))
                    .unwrap_or_else(|| Expr {
                        kind: ExprKind::Literal("?".to_string()),
                        span: Span::from_node(node),
                    });
                let op = node.child_by_field_name("operator")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("?")
                    .to_string();
                let right = node.child_by_field_name("right")
                    .map(|n| lower_expr(n, source))
                    .unwrap_or_else(|| Expr {
                        kind: ExprKind::Literal("?".to_string()),
                        span: Span::from_node(node),
                    });
                // Handle && and || as And/Or, not binary operators
                if op == "&&" {
                    let left_te = lower_test_expr(left_node.unwrap(), source);
                    let right_te = node.child_by_field_name("right")
                        .map(|n| lower_test_expr(n, source))
                        .unwrap_or_else(|| TestExpr::Unary {
                            op: "?".to_string(),
                            operand: Box::new(Expr { kind: ExprKind::Literal("?".to_string()), span: Span::from_node(node) }),
                        });
                    TestExpr::And(Box::new(left_te), Box::new(right_te))
                } else if op == "||" {
                    let left_te = lower_test_expr(left_node.unwrap(), source);
                    let right_te = node.child_by_field_name("right")
                        .map(|n| lower_test_expr(n, source))
                        .unwrap_or_else(|| TestExpr::Unary {
                            op: "?".to_string(),
                            operand: Box::new(Expr { kind: ExprKind::Literal("?".to_string()), span: Span::from_node(node) }),
                        });
                    TestExpr::Or(Box::new(left_te), Box::new(right_te))
                } else {
                    TestExpr::Binary { left: Box::new(left), op, right: Box::new(right) }
                }
            }
        }
        "negation_expression" => {
            // [[ ! expr ]] → Not(inner)
            let inner = node.children(&mut node.walk())
                .find(|c| c.is_named() && c.kind() != "!")
                .map(|c| lower_test_expr(c, source))
                .unwrap_or_else(|| TestExpr::Unary {
                    op: "?".to_string(),
                    operand: Box::new(Expr { kind: ExprKind::Literal("?".to_string()), span: Span::from_node(node) }),
                });
            TestExpr::Not(Box::new(inner))
        }
        "parenthesized_expression" => {
            let inner = node.children(&mut node.walk())
                .find(|c| c.is_named())
                .map(|c| lower_test_expr(c, source))
                .unwrap_or_else(|| TestExpr::Unary {
                    op: "?".to_string(),
                    operand: Box::new(Expr { kind: ExprKind::Literal("?".to_string()), span: Span::from_node(node) }),
                });
            TestExpr::Paren(Box::new(inner))
        }
        _ => {
            // Fallback — wrap as unary with the full text
            let text = node.utf8_text(source).unwrap_or("?").to_string();
            TestExpr::Unary {
                op: "?".to_string(),
                operand: Box::new(Expr {
                    kind: ExprKind::Literal(text),
                    span: Span::from_node(node),
                }),
            }
        }
    }
}

/// Process bash double-quote escape sequences: \" → ", \\ → \, \$ → $, \` → `, \newline → ""
fn unescape_double_quoted(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'"' | b'\\' | b'$' | b'`' => {
                    result.push(bytes[i + 1] as char);
                    i += 2;
                }
                b'\n' => {
                    // line continuation — skip both
                    i += 2;
                }
                _ => {
                    // Not a recognized escape — keep the backslash
                    result.push('\\');
                    i += 1;
                }
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

fn lower_string(node: Node, source: &[u8]) -> ExprKind {
    let mut parts = Vec::new();
    let mut last_end;

    // First pass: find the content range (inside the quotes)
    let mut content_start = node.start_byte();
    let mut content_end = node.end_byte();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !child.is_named() && child.kind() == "\"" {
            if child.start_byte() == node.start_byte() {
                content_start = child.end_byte();
            } else {
                // Closing quote: use end_byte - 1 to skip only the actual " char.
                // tree-sitter may include preceding whitespace in the quote token.
                content_end = child.end_byte() - 1;
            }
        }
    }
    last_end = content_start;

    // Second pass: collect named children and capture gaps as literals
    let mut cursor2 = node.walk();
    for child in node.children(&mut cursor2) {
        if !child.is_named() {
            continue;
        }

        // For simple_expansion inside strings, tree-sitter may include
        // leading whitespace in the node's range.  Detect and emit prefix.
        let child_start = child.start_byte();
        if child.kind() == "simple_expansion" || child.kind() == "expansion" {
            if let Some(first) = child.child(0) {
                let token_text = first.utf8_text(source).unwrap_or("");
                let trimmed = token_text.trim_start();
                let prefix_len = token_text.len() - trimmed.len();
                if prefix_len > 0 {
                    let prefix = &token_text[..prefix_len];
                    parts.push(Expr {
                        kind: ExprKind::Literal(prefix.to_string()),
                        span: Span { line: 0, col: 0 },
                    });
                }
            }
        }

        // Capture any gap between previous part and this child
        if child_start > last_end {
            let gap = std::str::from_utf8(&source[last_end..child_start])
                .unwrap_or("");
            if !gap.is_empty() {
                parts.push(Expr {
                    kind: ExprKind::Literal(unescape_double_quoted(gap)),
                    span: Span { line: 0, col: 0 },
                });
            }
        }
        parts.push(lower_expr(child, source));
        last_end = child.end_byte();
    }

    // Capture trailing text after last named child
    if last_end < content_end {
        let gap = std::str::from_utf8(&source[last_end..content_end])
            .unwrap_or("");
        if !gap.is_empty() {
            parts.push(Expr {
                kind: ExprKind::Literal(unescape_double_quoted(gap)),
                span: Span { line: 0, col: 0 },
            });
        }
    }

    if parts.is_empty() {
        ExprKind::DoubleQuoted(vec![])
    } else {
        // Sash-native: $arr[i] inside strings (DD#41)
        // CST gives: SimpleExpansion("arr") + Literal("[0] rest")
        // Rewrite to: Subscript{arr, 0} + Literal(" rest")
        let mut rewritten = Vec::new();
        let mut i = 0;
        while i < parts.len() {
            if let ExprKind::SimpleExpansion(ref name) = parts[i].kind {
                if i + 1 < parts.len() {
                    if let ExprKind::Literal(ref lit) = parts[i + 1].kind {
                        if lit.starts_with('[') {
                            if let Some(close) = lit.find(']') {
                                let idx = &lit[1..close];
                                let rest = &lit[close + 1..];
                                if idx == "@" || idx == "*" {
                                    rewritten.push(Expr {
                                        kind: ExprKind::ParameterExpansion(ParameterExpansion {
                                            name: name.clone(),
                                            operator: Some("arrvals".to_string()),
                                            operand: None,
                                        }),
                                        span: parts[i].span,
                                    });
                                } else {
                                    rewritten.push(Expr {
                                        kind: ExprKind::Subscript {
                                            name: name.clone(),
                                            index: Box::new(Expr {
                                                kind: ExprKind::Literal(idx.to_string()),
                                                span: parts[i].span,
                                            }),
                                        },
                                        span: parts[i].span,
                                    });
                                }
                                if !rest.is_empty() {
                                    rewritten.push(Expr {
                                        kind: ExprKind::Literal(rest.to_string()),
                                        span: parts[i + 1].span,
                                    });
                                }
                                i += 2;
                                continue;
                            }
                        }
                    }
                }
            }
            rewritten.push(parts[i].clone());
            i += 1;
        }
        ExprKind::DoubleQuoted(rewritten)
    }
}

fn lower_parameter_expansion(node: Node, source: &[u8]) -> ExprKind {
    let mut name = String::new();
    let mut operator = None;
    let mut operand = None;
    let mut is_subscript = false;
    let mut subscript_index: Option<Expr> = None;

    // Check for substring pattern: ${var:offset:length} or ${var:offset}
    // tree-sitter gives: variable_name, ":", number, ":", number
    let mut colon_count = 0;
    let mut substring_parts: Vec<String> = Vec::new();
    let mut has_named_op = false;

    // Pre-scan for substring pattern
    for child in node.children(&mut node.walk()) {
        match child.kind() {
            ":" if !name.is_empty() && !has_named_op => {
                colon_count += 1;
            }
            "number" if colon_count > 0 && !has_named_op => {
                substring_parts.push(child.utf8_text(source).unwrap_or("0").to_string());
            }
            _ if child.kind().starts_with(':') && child.kind().len() > 1 => {
                has_named_op = true; // :- :+ := :? — not substring
            }
            "variable_name" => {
                name = child.utf8_text(source).unwrap_or("").to_string();
            }
            _ => {}
        }
    }

    if colon_count >= 1 && !substring_parts.is_empty() && !has_named_op {
        // ${var:offset} or ${var:offset:length}
        let offset = substring_parts[0].clone();
        let length = if substring_parts.len() > 1 { substring_parts[1].clone() } else { String::new() };
        let operand_str = if length.is_empty() { offset } else { format!("{}:{}", offset, length) };
        return ExprKind::ParameterExpansion(ParameterExpansion {
            name,
            operator: Some("substring".to_string()),
            operand: Some(Box::new(Expr {
                kind: ExprKind::Literal(operand_str),
                span: Span::from_node(node),
            })),
        });
    }

    // Reset for normal parsing
    let mut name = String::new();

    for child in node.children(&mut node.walk()) {
        match child.kind() {
            "variable_name" => {
                name = child.utf8_text(source).unwrap_or("").to_string();
            }
            "#" | "##" if name.is_empty() => {
                // ${#var} — string length (# before variable_name)
                operator = Some("strlen".to_string());
            }
            "!" if name.is_empty() => {
                // ${!arr[@]} — indirect expansion / all keys
                operator = Some("!".to_string());
            }
            _ if child.kind().starts_with(':')
                || child.kind() == "%"
                || child.kind() == "%%"
                || child.kind() == "#"
                || child.kind() == "##"
                || child.kind() == "/"
                || child.kind() == "//"
                || child.kind() == "^"
                || child.kind() == "^^"
                || child.kind() == ","
                || child.kind() == ",," =>
            {
                operator = Some(child.kind().to_string());
            }
            "subscript" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    name = name_node.utf8_text(source).unwrap_or("").to_string();
                }
                if let Some(idx) = child.child_by_field_name("index") {
                    let idx_expr = lower_expr(idx, source);
                    is_subscript = true;
                    subscript_index = Some(idx_expr);
                } else {
                    // No index field → ${arr[@]} or ${#arr[@]}
                    is_subscript = true;
                }
            }
            "word" | "string" | "raw_string" | "regex"
            | "simple_expansion" | "expansion"
            | "command_substitution" | "concatenation" => {
                if operator.is_some() {
                    operand = Some(Box::new(lower_expr(child, source)));
                }
            }
            _ => {}
        }
    }

    if is_subscript {
        if let Some(ref op) = operator {
            if op == "strlen" {
                // ${#arr[@]} — array length
                return ExprKind::ParameterExpansion(ParameterExpansion {
                    name,
                    operator: Some("arrlen".to_string()),
                    operand: None,
                });
            }
        }
        if let Some(idx) = subscript_index {
            // ${arr[@]} or ${arr[*]} — all elements
            if matches!(&idx.kind, ExprKind::Literal(s) if s == "@" || s == "*") {
                if operator.as_deref() == Some("!") {
                    return ExprKind::ParameterExpansion(ParameterExpansion {
                        name,
                        operator: Some("arrkeys".to_string()),
                        operand: None,
                    });
                }
                return ExprKind::ParameterExpansion(ParameterExpansion {
                    name,
                    operator: Some("arrvals".to_string()),
                    operand: None,
                });
            }
            return ExprKind::Subscript { name, index: Box::new(idx) };
        }
        // ${arr[@]} or ${!arr[@]} — all elements/keys
        // For now, return as a special parameter expansion
        if operator.as_deref() == Some("!") {
            return ExprKind::ParameterExpansion(ParameterExpansion {
                name,
                operator: Some("arrkeys".to_string()),
                operand: None,
            });
        }
        return ExprKind::ParameterExpansion(ParameterExpansion {
            name,
            operator: Some("arrvals".to_string()),
            operand: None,
        });
    }

    ExprKind::ParameterExpansion(ParameterExpansion {
        name,
        operator,
        operand,
    })
}

// ── Arithmetic command (( expr )) ────────────────────────────

fn lower_arithmetic_command(node: Node, source: &[u8]) -> Option<Statement> {
    let span = Span::from_node(node);
    // Check for comma operator: multiple named children means comma-separated expressions
    let named_count = node.named_child_count();
    if named_count > 1 {
        // Comma operator: (( a=1, b=2, c=a+b )) → Group of individual assignments
        let mut stmts = Vec::new();
        for i in 0..named_count {
            let child = node.named_child(i).unwrap();
            // Create a synthetic single-expression arithmetic command
            // by recursively lowering each sub-expression
            if let Some(stmt) = lower_single_arith_expr(child, source, span) {
                stmts.push(stmt);
            }
        }
        return Some(Statement {
            kind: StmtKind::Group(Group { body: stmts }),
            span,
        });
    }
    let expr_node = node.named_child(0)?;
    let kind = lower_single_arith_expr_kind(expr_node, source, span)?;
    Some(Statement { kind, span })
}

/// Lower a single arithmetic sub-expression (used by both single and comma forms)
fn lower_single_arith_expr(node: Node, source: &[u8], span: Span) -> Option<Statement> {
    let kind = lower_single_arith_expr_kind(node, source, span)?;
    Some(Statement { kind, span })
}

fn lower_single_arith_expr_kind(expr_node: Node, source: &[u8], span: Span) -> Option<StmtKind> {
    let kind = match expr_node.kind() {
        "postfix_expression" => {
            let var_node = expr_node.named_child(0)?;
            let var_name = var_node.utf8_text(source).ok()?.to_string();
            let op_text = expr_node.utf8_text(source).ok()?;
            let arith_op = if op_text.ends_with("++") { "+" } else { "-" };
            StmtKind::Assignment(Assignment { index: None, op: AssignOp::Set,
                name: var_name.clone(),
                value: Expr {
                    kind: ExprKind::ArithmeticExpansion(format!("{} {} 1", var_name, arith_op)),
                    span,
                },

            })
        }
        "binary_expression" => {
            let left_node = expr_node.child_by_field_name("left")?;
            let right_node = expr_node.child_by_field_name("right")?;
            let left_text = arith_node_to_string(left_node, source);
            let right_text = arith_node_to_string(right_node, source);
            // Find the operator — it's the unnamed child between left and right
            let mut operator = "=";
            for i in 0..expr_node.child_count() {
                let child = expr_node.child(i).unwrap();
                if !child.is_named() {
                    let op = child.utf8_text(source).unwrap_or("");
                    if !op.trim().is_empty() {
                        operator = match op.trim() {
                            "+=" => "+=",
                            "-=" => "-=",
                            "*=" => "*=",
                            "/=" => "/=",
                            "%=" => "%=",
                            "=" => "=",
                            ">" | "<" | ">=" | "<=" | "==" | "!=" => {
                                // This is a comparison — desugar to test_command
                                let test_op = match op.trim() {
                                    ">" => "-gt",
                                    "<" => "-lt",
                                    ">=" => "-ge",
                                    "<=" => "-le",
                                    "==" => "-eq",
                                    "!=" => "-ne",
                                    _ => unreachable!(),
                                };
                                let make_arith_kind = |text: &str| -> ExprKind {
                                    let t = text.trim_matches(|c| c == '(' || c == ')').trim();
                                    if t.chars().all(|c| c.is_ascii_digit() || c == '-') && !t.is_empty() {
                                        ExprKind::Literal(t.to_string())
                                    } else if t.chars().all(|c| c.is_alphanumeric() || c == '_') && !t.is_empty() {
                                        ExprKind::SimpleExpansion(t.to_string())
                                    } else {
                                        ExprKind::ArithmeticExpansion(t.to_string())
                                    }
                                };
                                let left_kind = make_arith_kind(&left_text);
                                let right_kind = make_arith_kind(&right_text);
                                let test_expr = Expr {
                                    kind: ExprKind::Test(TestExpr::Binary {
                                        left: Box::new(Expr {
                                            kind: left_kind,
                                            span,
                                        }),
                                        op: test_op.to_string(),
                                        right: Box::new(Expr {
                                            kind: right_kind,
                                            span,
                                        }),
                                    }),
                                    span,
                                };
                                return Some(StmtKind::Command(Command {
                                    name: Expr {
                                        kind: ExprKind::Literal("[[".to_string()),
                                        span,
                                    },
                                    args: vec![test_expr],
                                }));
                            }
                            other => other,
                        };
                        break;
                    }
                }
            }
            // It's an assignment: x = rhs, x += rhs, etc.
            let arith_expr = match operator {
                "+=" => format!("{} + {}", left_text, right_text),
                "-=" => format!("{} - {}", left_text, right_text),
                "*=" => format!("{} * {}", left_text, right_text),
                "/=" => format!("{} / {}", left_text, right_text),
                "%=" => format!("{} % {}", left_text, right_text),
                "=" => right_text.clone(),
                _ => format!("{} {} {}", left_text, operator, right_text),
            };
            StmtKind::Assignment(Assignment { index: None, op: AssignOp::Set,
                name: left_text,
                value: Expr {
                    kind: ExprKind::ArithmeticExpansion(arith_expr),
                    span,
                },

            })
        }
        "unary_expression" => {
            // ++x or --x
            let var_node = expr_node.named_child(0)?;
            let var_name = var_node.utf8_text(source).ok()?.to_string();
            let op_text = expr_node.utf8_text(source).ok()?;
            let arith_op = if op_text.starts_with("++") { "+" } else { "-" };
            StmtKind::Assignment(Assignment { index: None, op: AssignOp::Set,
                name: var_name.clone(),
                value: Expr {
                    kind: ExprKind::ArithmeticExpansion(format!("{} {} 1", var_name, arith_op)),
                    span,
                },

            })
        }
        "parenthesized_expression" => {
            // (( (expr) )) — recurse into the inner expression
            if let Some(inner) = expr_node.named_child(0) {
                return lower_single_arith_expr_kind(inner, source, span);
            }
            return None;
        }
        _ => {
            // Bare expression — could be a number (used as condition) or variable
            let text = expr_node.utf8_text(source).ok()?.trim().to_string();
            // Desugar (( expr )) to [[ expr -ne 0 ]] for use as a test
            let test_var = if text.chars().all(|c| c.is_ascii_digit()) {
                // Literal number: (( 0 )) → false, (( 5 )) → true
                ExprKind::Literal(text)
            } else {
                // Variable: (( x )) → [[ $x -ne 0 ]]
                ExprKind::SimpleExpansion(text)
            };
            StmtKind::Command(Command {
                name: Expr { kind: ExprKind::Literal("[[".to_string()), span },
                args: vec![Expr {
                    kind: ExprKind::Test(TestExpr::Binary {
                        left: Box::new(Expr { kind: test_var, span }),
                        op: "-ne".to_string(),
                        right: Box::new(Expr { kind: ExprKind::Literal("0".to_string()), span }),
                    }),
                    span,
                }],
            })
        }
    };
    Some(kind)
}

fn lower_c_style_for(node: Node, source: &[u8]) -> Statement {
    let span = Span::from_node(node);

    // Initializer: variable_assignment(s) or expression(s)
    let mut init_stmts = Vec::new();
    if let Some(init) = node.child_by_field_name("initializer") {
        if init.kind() == "variable_assignment" {
            if let Some(s) = lower_statement(init, source) {
                init_stmts.push(s);
            }
        } else {
            // Expression like i=0 inside (( )) context — parse as arithmetic
            let text = init.utf8_text(source).unwrap_or("").to_string();
            if let Some(eq) = text.find('=') {
                let name = text[..eq].trim().to_string();
                let val = text[eq+1..].trim().to_string();
                init_stmts.push(Statement {
                    kind: StmtKind::Assignment(Assignment { index: None, op: AssignOp::Set,
                        name,
                        value: Expr { kind: ExprKind::Literal(val), span },
                    }),
                    span,
                });
            }
        }
    }

    // Condition: binary_expression → desugar to [[ test ]]
    let condition = if let Some(cond) = node.child_by_field_name("condition") {
        let _cond_text = cond.utf8_text(source).unwrap_or("true");
        // Parse "i<5" style expressions
        let cond_stmt = if cond.kind() == "binary_expression" {
            let left = cond.child_by_field_name("left")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("0").to_string();
            let right = cond.child_by_field_name("right")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("0").to_string();
            // Find operator
            let mut op = "<";
            for i in 0..cond.child_count() {
                let child = cond.child(i).unwrap();
                if !child.is_named() {
                    let t = child.utf8_text(source).unwrap_or("");
                    if matches!(t, "<" | ">" | "<=" | ">=" | "==" | "!=") {
                        op = match t {
                            "<" => "-lt",
                            ">" => "-gt",
                            "<=" => "-le",
                            ">=" => "-ge",
                            "==" => "-eq",
                            "!=" => "-ne",
                            _ => "-lt",
                        };
                        break;
                    }
                }
            }
            Statement {
                kind: StmtKind::Command(Command {
                    name: Expr { kind: ExprKind::Literal("[[".to_string()), span },
                    args: vec![Expr {
                        kind: ExprKind::Test(TestExpr::Binary {
                            left: Box::new(Expr { kind: ExprKind::SimpleExpansion(left), span }),
                            op: op.to_string(),
                            right: Box::new(Expr { kind: ExprKind::Literal(right), span }),
                        }),
                        span,
                    }],
                }),
                span,
            }
        } else {
            // Fallback: treat as literal test
            Statement {
                kind: StmtKind::Command(Command {
                    name: Expr { kind: ExprKind::Literal("true".to_string()), span },
                    args: vec![],
                }),
                span,
            }
        };
        vec![cond_stmt]
    } else {
        vec![Statement {
            kind: StmtKind::Command(Command {
                name: Expr { kind: ExprKind::Literal("true".to_string()), span },
                args: vec![],
            }),
            span,
        }]
    };

    // Update: postfix_expression, binary_expression, etc.
    let mut update_stmts = Vec::new();
    if let Some(upd) = node.child_by_field_name("update") {
        // Create a fake arithmetic_command-like node and reuse the same logic
        let upd_text = upd.utf8_text(source).unwrap_or("").to_string();
        if upd.kind() == "postfix_expression" {
            let var_node = upd.named_child(0);
            let var_name = var_node.and_then(|n| n.utf8_text(source).ok()).unwrap_or("x").to_string();
            let arith_op = if upd_text.ends_with("++") { "+" } else { "-" };
            update_stmts.push(Statement {
                kind: StmtKind::Assignment(Assignment { index: None, op: AssignOp::Set,
                    name: var_name.clone(),
                    value: Expr {
                        kind: ExprKind::ArithmeticExpansion(format!("{} {} 1", var_name, arith_op)),
                        span,
                    },
                }),
                span,
            });
        } else if upd.kind() == "binary_expression" {
            let left = upd.child_by_field_name("left")
                .and_then(|n| n.utf8_text(source).ok()).unwrap_or("x").to_string();
            let right = upd.child_by_field_name("right")
                .and_then(|n| n.utf8_text(source).ok()).unwrap_or("1").to_string();
            let mut operator = "+=";
            for i in 0..upd.child_count() {
                let child = upd.child(i).unwrap();
                if !child.is_named() {
                    let t = child.utf8_text(source).unwrap_or("").trim();
                    if !t.is_empty() { operator = match t { "+=" => "+=", "-=" => "-=", _ => t }; break; }
                }
            }
            let arith = match operator {
                "+=" => format!("{} + {}", left, right),
                "-=" => format!("{} - {}", left, right),
                _ => format!("{} {} {}", left, operator, right),
            };
            update_stmts.push(Statement {
                kind: StmtKind::Assignment(Assignment { index: None, op: AssignOp::Set,
                    name: left,
                    value: Expr { kind: ExprKind::ArithmeticExpansion(arith), span },
                }),
                span,
            });
        }
    }

    // Body
    let body_node = node.child_by_field_name("body");
    let mut body = body_node.map(|n| lower_body(n, source)).unwrap_or_default();
    body.extend(update_stmts);

    // Desugar: init; while cond; do body; update; done
    // Return as a Group containing init stmts + while
    let while_stmt = Statement {
        kind: StmtKind::While(While {
            condition,
            body,
            until: false,
        }),
        span,
    };

    let mut group_body = init_stmts;
    group_body.push(while_stmt);
    Statement {
        kind: StmtKind::Group(Group { body: group_body }),
        span,
    }
}

/// Walk tree-sitter arithmetic AST nodes to produce an ExprKind.
/// Handles parenthesized_expression, binary_expression, unary_expression,
/// ternary_expression, number, variable_name, etc.
fn lower_arith_expr_to_exprkind(node: Node, source: &[u8]) -> ExprKind {
    match node.kind() {
        "parenthesized_expression" => {
            // ( expr ) → recurse into inner
            if let Some(inner) = node.named_child(0) {
                lower_arith_expr_to_exprkind(inner, source)
            } else {
                ExprKind::ArithmeticExpansion("0".to_string())
            }
        }
        "binary_expression" => {
            let left = node.child_by_field_name("left");
            let right = node.child_by_field_name("right");
            if let (Some(l), Some(r)) = (left, right) {
                // Find operator — unnamed child between left and right
                let mut op_str = "+";
                for i in 0..node.child_count() {
                    let child = node.child(i).unwrap();
                    if !child.is_named() {
                        let text = child.utf8_text(source).unwrap_or("").trim();
                        if !text.is_empty() && text != "(" && text != ")" {
                            op_str = child.utf8_text(source).unwrap_or("+").trim();
                            break;
                        }
                    }
                }
                let l_expr = arith_node_to_string(l, source);
                let r_expr = arith_node_to_string(r, source);
                ExprKind::ArithmeticExpansion(format!("{} {} {}", l_expr, op_str, r_expr))
            } else {
                ExprKind::ArithmeticExpansion(node.utf8_text(source).unwrap_or("0").to_string())
            }
        }
        "unary_expression" => {
            let text = node.utf8_text(source).unwrap_or("0").trim().to_string();
            ExprKind::ArithmeticExpansion(text)
        }
        "ternary_expression" => {
            let text = node.utf8_text(source).unwrap_or("0").trim().to_string();
            ExprKind::ArithmeticExpansion(text)
        }
        "number" | "variable_name" | "word" | "simple_expansion" => {
            let text = node.utf8_text(source).unwrap_or("0").trim().to_string();
            let text = text.trim_start_matches('$').to_string();
            ExprKind::ArithmeticExpansion(text)
        }
        _ => {
            let text = node.utf8_text(source).unwrap_or("0").trim().to_string();
            ExprKind::ArithmeticExpansion(text)
        }
    }
}

/// Recursively convert an arithmetic AST node to a flat string representation
/// that the existing text-based transform pipeline can handle.
/// Parenthesized expressions are recursively expanded so the text parser
/// never sees raw parens.
fn arith_node_to_string(node: Node, source: &[u8]) -> String {
    match node.kind() {
        "parenthesized_expression" => {
            if let Some(inner) = node.named_child(0) {
                // Recurse — the parenthesized sub-expression becomes its own
                // ArithmeticExpansion string, which the transform will parse
                // as a nested expression
                format!("({})", arith_node_to_string(inner, source))
            } else {
                "0".to_string()
            }
        }
        "binary_expression" => {
            let left = node.child_by_field_name("left");
            let right = node.child_by_field_name("right");
            if let (Some(l), Some(r)) = (left, right) {
                let mut op_str = "+";
                for i in 0..node.child_count() {
                    let child = node.child(i).unwrap();
                    if !child.is_named() {
                        let text = child.utf8_text(source).unwrap_or("").trim();
                        if !text.is_empty() && text != "(" && text != ")" {
                            op_str = child.utf8_text(source).unwrap_or("+").trim();
                            break;
                        }
                    }
                }
                format!("{} {} {}", arith_node_to_string(l, source), op_str, arith_node_to_string(r, source))
            } else {
                node.utf8_text(source).unwrap_or("0").to_string()
            }
        }
        "unary_expression" => {
            // e.g. -x, !x
            node.utf8_text(source).unwrap_or("0").trim().to_string()
        }
        "number" => node.utf8_text(source).unwrap_or("0").to_string(),
        "variable_name" | "word" => node.utf8_text(source).unwrap_or("0").trim_start_matches('$').to_string(),
        "simple_expansion" => {
            // $var → var
            node.utf8_text(source).unwrap_or("0").trim_start_matches('$').to_string()
        }
        _ => node.utf8_text(source).unwrap_or("0").trim().to_string(),
    }
}
