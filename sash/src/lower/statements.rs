//! Statement lowering: CST statement nodes → IR Statement.
//!
//! SECURITY: Each match arm handles one CST node type. Unknown node
//! types produce None (no IR) — this is safe because the validator
//! has already rejected anything not in the safe subset.

use super::arithmetic::{lower_arithmetic_command, lower_c_style_for};
use super::control_flow::{lower_case, lower_for, lower_if, lower_while};
use super::expressions::{lower_expr, lower_test_command};
use super::functions::{lower_function, lower_redirect, lower_redirected};
use super::helpers::child_field_name;
use super::lower_body;
use crate::emit_source::record_coercion;
use crate::ir::*;
use tree_sitter::Node;

/// Lower a single CST node to an IR Statement, or None for
/// nodes we skip (semicolons, newlines, keywords, etc.).
pub(super) fn lower_statement(node: Node, source: &[u8]) -> Option<Statement> {
    let span = Span::from_node(node);
    let kind = match node.kind() {
        "variable_assignment" => StmtKind::Assignment(lower_assignment(node, source)?),
        "declaration_command" => StmtKind::Declaration(lower_declaration(node, source)?),
        "command" => {
            // Coerce `let "expr"` → `(( expr ))` for source emission
            if let Some(name_node) = node.child_by_field_name("name") {
                let name_text = name_node.utf8_text(source).unwrap_or("");
                if name_text == "let" {
                    let original = node.utf8_text(source).unwrap_or("").to_string();
                    // Collect argument text, strip quotes
                    let mut exprs = Vec::new();
                    for child in node.children(&mut node.walk()) {
                        if child.is_named() && child != name_node {
                            let t = child.utf8_text(source).unwrap_or("");
                            let t = t.trim_matches('"').trim_matches('\'');
                            exprs.push(t.to_string());
                        }
                    }
                    let replacement = format!("(( {} ))", exprs.join(", "));
                    record_coercion(node.start_byte(), node.end_byte(), replacement, original);
                }
            }
            // Check if command has redirect children (heredoc, herestring, file_redirect)
            let has_redirects = node.children(&mut node.walk()).any(|c| {
                matches!(
                    c.kind(),
                    "file_redirect" | "heredoc_redirect" | "herestring_redirect"
                )
            });
            if has_redirects {
                // Wrap as Redirected statement
                let cmd = lower_command(node, source)?;
                let mut redirects = Vec::new();
                for child in node.children(&mut node.walk()) {
                    if matches!(
                        child.kind(),
                        "file_redirect" | "heredoc_redirect" | "herestring_redirect"
                    ) {
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
        "pipeline" => StmtKind::Pipeline(lower_pipeline(node, source)),
        "list" => {
            return Some(lower_list(node, source));
        }
        "if_statement" => StmtKind::If(lower_if(node, source)),
        "while_statement" => StmtKind::While(lower_while(node, source)),
        "for_statement" => {
            // Reject 'select' (parsed as for_statement variant)
            if let Some(kw) = node.child(0) {
                if kw.utf8_text(source).unwrap_or("") == "select" {
                    // Emit as a Command so SB5011 rejects it naturally
                    return Some(Statement {
                        kind: StmtKind::Command(Command {
                            name: Expr {
                                kind: ExprKind::Literal("select".to_string()),
                                span,
                            },
                            args: vec![],
                        }),
                        span,
                    });
                }
            }
            StmtKind::For(lower_for(node, source))
        }
        "c_style_for_statement" => {
            return Some(lower_c_style_for(node, source));
        }
        "case_statement" => StmtKind::Case(lower_case(node, source)),
        "function_definition" => {
            // Check if function has redirects attached (e.g., f() { ... } > file)
            let mut redirects = Vec::new();
            for child in node.children(&mut node.walk()) {
                if child.kind() == "file_redirect"
                    || child.kind() == "heredoc_redirect"
                    || child.kind() == "herestring_redirect"
                {
                    redirects.push(lower_redirect(child, source));
                }
            }
            let func = StmtKind::Function(lower_function(node, source));
            if redirects.is_empty() {
                func
            } else {
                StmtKind::Redirected(Redirected {
                    body: Box::new(Statement {
                        kind: func,
                        span: Span::from_node(node),
                    }),
                    redirects,
                })
            }
        }
        "redirected_statement" => StmtKind::Redirected(lower_redirected(node, source)),
        "negated_command" => lower_negated(node, source)?,
        "suppressed_command" => lower_suppressed(node, source)?,
        "subshell" => StmtKind::Subshell(Subshell {
            body: lower_body(node, source),
        }),
        "compound_statement" => StmtKind::Group(Group {
            body: lower_body(node, source),
        }),
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
                    span,
                },
                args,
            })
        }
        "test_command" => {
            // [[ expr ]] or [ expr ] as a standalone statement
            StmtKind::Command(Command {
                name: Expr {
                    kind: ExprKind::Literal("[[".to_string()),
                    span,
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
        "{" | "}" | ";" | "do" | "done" | "then" | "fi" | "else" | "elif" | "in" | "esac"
        | ";;" => {
            // Structural tokens — not statements, skip silently
            return None;
        }
        _ => {
            eprintln!(
                "UNKNOWN STMT KIND: {} text={:?}",
                node.kind(),
                node.utf8_text(source).unwrap_or("?")
            );
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
        if !child.is_named() {
            continue;
        }
        // Each element is a concatenation like ["[", "foo", "]", "=bar"]
        // or the full text is "[foo]=bar"
        let text = child.utf8_text(source).unwrap_or("").to_string();
        if let Some(rest) = text.strip_prefix('[') {
            if let Some(eq_pos) = rest.find("]=") {
                let key = &rest[..eq_pos];
                let val_raw = &rest[eq_pos + 2..];
                // Strip surrounding quotes if present
                let val = if (val_raw.starts_with('"') && val_raw.ends_with('"'))
                    || (val_raw.starts_with('\'') && val_raw.ends_with('\''))
                {
                    &val_raw[1..val_raw.len() - 1]
                } else {
                    val_raw
                };
                pairs.push((
                    key.to_string(),
                    Expr {
                        kind: ExprKind::Literal(val.to_string()),
                        span: Span::from_node(child),
                    },
                ));
            }
        }
    }

    Some(Assignment {
        index: None,
        op: AssignOp::Set,
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
            "+=" => {
                op = AssignOp::Append;
            }
            _ => {}
        }
    }

    if name.is_empty() {
        // Fallback: try field-based extraction
        name = node
            .child_by_field_name("name")?
            .utf8_text(source)
            .ok()?
            .to_string();
    }

    let value = if let Some(val_node) = node.child_by_field_name("value") {
        lower_expr(val_node, source)
    } else {
        Expr {
            kind: ExprKind::Literal(String::new()),
            span: Span::from_node(node),
        }
    };

    Some(Assignment {
        name,
        index,
        op,
        value,
    })
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
        "readonly" | "const" => DeclKeyword::Readonly,
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
                if let Some(flag_chars) = text.strip_prefix('-') {
                    // Parse combined flags: -ri → -r + -i
                    for ch in flag_chars.chars() {
                        match ch {
                            'i' => var_type = VarType::Int,
                            'a' => flags.push("-a".to_string()),
                            'A' => flags.push("-A".to_string()),
                            'r' => flags.push("-r".to_string()),
                            'x' => flags.push("-x".to_string()),
                            _ => flags.push(format!("-{}", ch)),
                        }
                    }
                }
            }
            "variable_name" => {
                let text = child.utf8_text(source).unwrap_or("");
                match text {
                    "int" => {
                        var_type = VarType::Int;
                    }
                    "const" => {
                        flags.push("-r".to_string());
                    }
                    "array" => {
                        flags.push("-a".to_string());
                    }
                    "map" => {
                        flags.push("-A".to_string());
                    }
                    _ => {
                        // Bare variable declaration like `array arr` or `declare x`
                        assignments.push(Assignment {
                            name: text.to_string(),
                            index: None,
                            op: AssignOp::Set,
                            value: Expr {
                                kind: ExprKind::Literal(String::new()),
                                span: Span { line: 0, col: 0 },
                            },
                        });
                    }
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

    // Record coercion if bash syntax differs from sash-native
    // Sash-native keywords (const, int, array, map) and `local` need no coercion
    // unless they have bash-style flags like -i, -a, -A, -r
    let sash_native_keyword = matches!(keyword_node.kind(), "const" | "int" | "array" | "map");
    let has_bash_flags = children[1..]
        .iter()
        .any(|c| c.kind() == "word" && c.utf8_text(source).unwrap_or("").starts_with('-'));
    let needs_coercion = !sash_native_keyword
        && (matches!(keyword_node.kind(), "declare" | "typeset" | "readonly")
            && (!flags.is_empty()
                || var_type != VarType::String
                || matches!(keyword_node.kind(), "typeset" | "readonly"))
            || (keyword == DeclKeyword::Local && has_bash_flags));

    if needs_coercion {
        let original = node.utf8_text(source).unwrap_or("").to_string();
        let mut parts = Vec::new();
        if keyword == DeclKeyword::Local {
            parts.push("local");
        }
        if keyword == DeclKeyword::Readonly || flags.contains(&"-r".to_string()) {
            parts.push("const");
        }
        if var_type == VarType::Int {
            parts.push("int");
        }
        if flags.contains(&"-A".to_string()) {
            parts.push("map");
        } else if flags.contains(&"-a".to_string()) {
            parts.push("array");
        }
        // Append the assignment text (everything after the keyword+flags)
        let mut assign_start = None;
        for child in &children[1..] {
            if !matches!(child.kind(), "word" | "type_annotation") {
                // Skip sash modifier keywords parsed as variable_name
                let text = child.utf8_text(source).unwrap_or("");
                if matches!(text, "int" | "const" | "array" | "map") {
                    continue;
                }
                assign_start = Some(child.start_byte());
                break;
            }
        }
        if let Some(start) = assign_start {
            let assign_text = std::str::from_utf8(&source[start..node.end_byte()]).unwrap_or("");
            parts.push(assign_text.trim());
        }
        let replacement = parts.join(" ");
        if replacement != original {
            record_coercion(node.start_byte(), node.end_byte(), replacement, original);
        }
    }

    Some(Declaration {
        keyword,
        var_type,
        flags,
        assignments,
    })
}

// ── Command lowering ──────────────────────────────────────────

fn lower_command(node: Node, source: &[u8]) -> Option<Command> {
    let name_node = node.child_by_field_name("name")?;
    let name = lower_expr(name_node, source);

    // bless VAR [[ test_expr ]] — reconstruct the test expression
    // tree-sitter parses this as a plain command with word arguments,
    // so we re-parse the [[ ... ]] portion to get a proper TestExpr.
    if let ExprKind::Literal(ref n) = name.kind {
        if n == "bless" {
            let mut args_raw: Vec<Node> = Vec::new();
            for child in node.children(&mut node.walk()) {
                if child.is_named() && child != name_node {
                    args_raw.push(child);
                }
            }
            // First arg is variable name, rest should be [[ ... ]]
            if !args_raw.is_empty() {
                let var_expr = lower_expr(args_raw[0], source);
                // Reconstruct the test portion from source bytes
                if args_raw.len() >= 2 {
                    let test_start = args_raw[1].start_byte();
                    let test_end = args_raw.last().unwrap().end_byte();
                    let test_text = &source[test_start..test_end];
                    let test_str = std::str::from_utf8(test_text).unwrap_or("");
                    // Re-parse as a standalone test command
                    let wrapper = format!("{}\n", test_str);
                    let mut parser = tree_sitter::Parser::new();
                    parser.set_language(&tree_sitter_bash::LANGUAGE.into()).ok();
                    if let Some(tree) = parser.parse(wrapper.as_bytes(), None) {
                        let root = tree.root_node();
                        // Look for test_command in the parsed tree
                        for child in root.children(&mut root.walk()) {
                            if child.kind() == "test_command" {
                                let test_kind = lower_test_command(child, wrapper.as_bytes());
                                if let ExprKind::Test(test_expr) = test_kind {
                                    return Some(Command {
                                        name,
                                        args: vec![
                                            var_expr,
                                            Expr {
                                                kind: ExprKind::Test(test_expr),
                                                span: Span::from_node(node),
                                            },
                                        ],
                                    });
                                }
                            }
                        }
                    }
                }
                // Fallback: return as plain command
                let mut args = vec![var_expr];
                for a in &args_raw[1..] {
                    args.push(lower_expr(*a, source));
                }
                return Some(Command { name, args });
            }
        }
    }

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
            if child.kind() == "=~" {
                after_op = true;
                continue;
            }
            if after_op && child.is_named() {
                regex_parts.push(lower_expr(child, source));
            }
        }
        return Some(Command {
            name,
            args: regex_parts,
        });
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
            "&&" => {
                op = ListOp::And;
                op_idx = i;
                break;
            }
            "||" => {
                op = ListOp::Or;
                op_idx = i;
                break;
            }
            _ => {}
        }
    }

    // Left side: everything before the operator
    let left = children[..op_idx]
        .iter()
        .filter_map(|c| lower_statement(*c, source))
        .next()
        .unwrap_or_else(|| Statement {
            kind: StmtKind::Command(Command {
                name: Expr {
                    kind: ExprKind::Literal("?".to_string()),
                    span,
                },
                args: vec![],
            }),
            span,
        });

    // Right side: everything after the operator
    // Could be another list (chained &&/||) — handle recursively
    let right_nodes: Vec<Node> = children[op_idx + 1..]
        .iter()
        .filter(|c| c.is_named())
        .copied()
        .collect();

    let right = if right_nodes.len() == 1 {
        lower_statement(right_nodes[0], source).unwrap_or_else(|| Statement {
            kind: StmtKind::Command(Command {
                name: Expr {
                    kind: ExprKind::Literal("?".to_string()),
                    span,
                },
                args: vec![],
            }),
            span,
        })
    } else {
        // Multiple nodes on right — shouldn't happen in practice
        // since tree-sitter nests lists, but handle gracefully
        right_nodes
            .iter()
            .filter_map(|c| lower_statement(*c, source))
            .next()
            .unwrap_or_else(|| Statement {
                kind: StmtKind::Command(Command {
                    name: Expr {
                        kind: ExprKind::Literal("?".to_string()),
                        span,
                    },
                    args: vec![],
                }),
                span,
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

// ── Suppressed command (- cmd) ────────────────────────────────

fn lower_suppressed(node: Node, source: &[u8]) -> Option<StmtKind> {
    // - cmd or - pipeline
    for child in node.children(&mut node.walk()) {
        if child.is_named() {
            if let Some(stmt) = lower_statement(child, source) {
                return Some(StmtKind::Suppressed(Box::new(stmt)));
            }
        }
    }
    None
}
