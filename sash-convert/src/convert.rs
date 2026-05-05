/// sash-convert conversion engine.
///
/// Walks the tree-sitter CST, collects source patches (mechanical conversions,
/// MANUAL annotations, CHECK hints, BLESS hints), and applies them to produce
/// sash-native source.

use crate::known_externals;
use crate::ScFinding;
use tree_sitter::Node;

/// A source-level patch: insert or replace text.
#[derive(Debug, Clone)]
struct Patch {
    start: usize,
    end: usize,
    replacement: String,
    /// If set, insert this line *before* the line containing start.
    prefix_line: Option<String>,
    /// Original text, for --annotate.
    original: Option<String>,
}

pub fn convert(source: &str, sc_findings: &[ScFinding], annotate: bool) -> String {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_bash::LANGUAGE.into())
        .expect("Error loading Bash grammar");
    let tree = parser.parse(source, None).expect("Failed to parse");

    let src = source.as_bytes();
    let mut patches: Vec<Patch> = Vec::new();
    let mut prefix_lines: Vec<(usize, String)> = Vec::new(); // (line_number, text)

    // Walk the CST
    walk_node(tree.root_node(), src, &mut patches, &mut prefix_lines, annotate);

    // Inject bless hints for command substitution assignments
    inject_bless_hints(tree.root_node(), src, &mut prefix_lines);

    // Add CHECK hints from shellcheck SC2086 findings
    add_sc2086_hints(sc_findings, &mut prefix_lines);

    // Apply patches to source
    apply_patches(source, &patches, &prefix_lines, annotate)
}

fn walk_node(
    node: Node,
    source: &[u8],
    patches: &mut Vec<Patch>,
    prefix_lines: &mut Vec<(usize, String)>,
    annotate: bool,
) {
    match node.kind() {
        "declaration_command" => {
            convert_declaration(node, source, patches, prefix_lines);
        }
        "command" => {
            convert_command(node, source, patches, prefix_lines, annotate);
        }
        "test_command" => {
            convert_test_command(node, source, patches);
        }
        "subshell" => {
            // Subshell: mark as MANUAL for now (mechanical conversion is complex)
            let line = node.start_position().row;
            let text = node_text(node, source);
            prefix_lines.push((line, format!(
                "# MANUAL: subshell ( ... ) — convert to function with local vars, or remove"
            )));
            let _ = text; // pass through unchanged
        }
        "expansion" => {
            convert_expansion(node, source, patches, prefix_lines);
        }
        "for_statement" if node.child(0).map_or(false, |c| c.kind() == "select") => {
            let line = node.start_position().row;
            prefix_lines.push((line, "# MANUAL: select — not supported in sash, rewrite as while+read menu".into()));
        }
        "process_substitution" => {
            let line = node.start_position().row;
            prefix_lines.push((line, "# MANUAL: process substitution <()/>() not supported in sash".into()));
        }
        _ => {}
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_node(child, source, patches, prefix_lines, annotate);
    }
}

// ── Declaration conversions ───────────────────────────────────

fn convert_declaration(node: Node, source: &[u8], patches: &mut Vec<Patch>, prefix_lines: &mut Vec<(usize, String)>) {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    if children.is_empty() {
        return;
    }

    let keyword_node = children[0];
    let keyword_text = node_text(keyword_node, source);

    // Detect flags
    let mut flags: Vec<String> = Vec::new();
    for child in &children[1..] {
        let t = node_text(*child, source);
        if t.starts_with('-') && child.kind() == "word" {
            // Expand combined flags: -ri → -r, -i
            for c in t[1..].chars() {
                flags.push(format!("-{}", c));
            }
        }
    }

    // Check for MANUAL cases
    if flags.contains(&"-n".to_string()) {
        // nameref — unconvertible
        let line = node.start_position().row;
        prefix_lines.push((line, "# MANUAL: declare -n (nameref) not supported in sash".into()));
        return;
    }

    // export → MANUAL
    if keyword_text == "export" {
        let line = node.start_position().row;
        prefix_lines.push((line, "# MANUAL: export — env isolation in sash (DD#53), use /usr/bin/env for per-command env".into()));
        return;
    }

    // Determine target keywords
    let mut parts: Vec<&str> = Vec::new();
    let is_local = keyword_text == "local";
    let is_declare = keyword_text == "declare" || keyword_text == "typeset";
    let is_readonly = keyword_text == "readonly";

    if is_local {
        parts.push("local");
    }

    if is_readonly || flags.contains(&"-r".to_string()) {
        parts.push("const");
    }

    let has_type_flag = flags.contains(&"-i".to_string())
        || flags.contains(&"-a".to_string())
        || flags.contains(&"-A".to_string());

    if flags.contains(&"-i".to_string()) {
        parts.push("int");
    }
    if flags.contains(&"-A".to_string()) {
        parts.push("map");
    } else if flags.contains(&"-a".to_string()) {
        parts.push("array");
    }

    // For bare `declare x=5` or `typeset x=5` with no flags → bare assignment
    let needs_conversion = is_declare || is_readonly
        || (is_local && has_type_flag);

    if !needs_conversion {
        return;
    }

    // Find the assignment part (everything after keyword + flags)
    let mut assign_start = None;
    for child in &children[1..] {
        let t = node_text(*child, source);
        if t.starts_with('-') && child.kind() == "word" {
            continue; // skip flags
        }
        assign_start = Some(child.start_byte());
        break;
    }

    if let Some(start) = assign_start {
        let assign_text = std::str::from_utf8(&source[start..node.end_byte()])
            .unwrap_or("")
            .trim();
        if !assign_text.is_empty() {
            parts.push(assign_text);
        }
    }

    let replacement = parts.join(" ");
    let original = node_text(node, source).to_string();

    if replacement != original {
        patches.push(Patch {
            start: node.start_byte(),
            end: node.end_byte(),
            replacement,
            prefix_line: None,
            original: Some(original),
        });
    }
}

// ── Command conversions ───────────────────────────────────────

fn convert_command(
    node: Node,
    source: &[u8],
    patches: &mut Vec<Patch>,
    prefix_lines: &mut Vec<(usize, String)>,
    _annotate: bool,
) {
    let name_node = match node.child_by_field_name("name") {
        Some(n) => n,
        None => return,
    };
    let name = node_text(name_node, source);
    let line = node.start_position().row;

    match name {
        // Mechanical: let → (( ))
        "let" => {
            let original = node_text(node, source).to_string();
            let mut exprs = Vec::new();
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() && child != name_node {
                    let t = node_text(child, source);
                    let t = t.trim_matches('"').trim_matches('\'');
                    exprs.push(t.to_string());
                }
            }
            let replacement = format!("(( {} ))", exprs.join(", "));
            patches.push(Patch {
                start: node.start_byte(),
                end: node.end_byte(),
                replacement,
                prefix_line: None,
                original: Some(original),
            });
        }

        // MANUAL annotations for unconvertible commands
        "eval" => {
            prefix_lines.push((line, "# MANUAL: eval — dynamic execution not supported in sash".into()));
        }
        "source" | "." => {
            prefix_lines.push((line, "# MANUAL: source/. — use sash-expand pipeline instead (DD#54)".into()));
        }
        "export" => {
            prefix_lines.push((line, "# MANUAL: export — env isolation in sash (DD#53), use /usr/bin/env for per-command env".into()));
        }
        "alias" => {
            prefix_lines.push((line, "# MANUAL: alias — not supported in sash, convert to a function".into()));
        }
        "set" => {
            prefix_lines.push((line, "# MANUAL: set — shell options not configurable in sash".into()));
        }
        "shopt" => {
            prefix_lines.push((line, "# MANUAL: shopt — shell options not configurable in sash".into()));
        }
        "select" => {
            prefix_lines.push((line, "# MANUAL: select — not supported in sash, rewrite as while+read menu".into()));
        }
        "coproc" => {
            prefix_lines.push((line, "# MANUAL: coproc — not supported in sash".into()));
        }

        _ => {}
    }
}

// ── Test command conversion: [ ] → [[ ]] ─────────────────────

fn convert_test_command(node: Node, source: &[u8], patches: &mut Vec<Patch>) {
    let text = node_text(node, source);
    // Only convert [ ... ] to [[ ... ]], not already [[ ... ]]
    if text.starts_with("[ ") && text.ends_with(" ]") && !text.starts_with("[[ ") {
        let inner = &text[2..text.len() - 2];
        let replacement = format!("[[ {} ]]", inner);
        patches.push(Patch {
            start: node.start_byte(),
            end: node.end_byte(),
            replacement,
            prefix_line: None,
            original: Some(text.to_string()),
        });
    }
}

// ── Expansion conversions ─────────────────────────────────────

fn convert_expansion(node: Node, source: &[u8], patches: &mut Vec<Patch>, prefix_lines: &mut Vec<(usize, String)>) {
    let text = node_text(node, source);

    // ${var-x} → ${var:-x}  (but not ${var:-x} which already has the colon)
    // The expansion node text includes ${ and }
    // Look for pattern: ${NAME-default} where there's no : before the -
    if text.contains('-') && !text.contains(":-") && !text.contains(":+")
        && !text.contains(":?") {
        // Check it's a simple default: ${var-value}
        if let Some(dash_pos) = text.find('-') {
            // Make sure it's not ${var:offset:length} arithmetic
            let before_dash = &text[2..dash_pos]; // skip ${
            if !before_dash.contains(':') && before_dash.chars().all(|c| c.is_alphanumeric() || c == '_') {
                let replacement = format!("${{{}:-{}", before_dash, &text[dash_pos + 1..]);
                patches.push(Patch {
                    start: node.start_byte(),
                    end: node.end_byte(),
                    replacement,
                    prefix_line: None,
                    original: Some(text.to_string()),
                });
            }
        }
    }

    // ${!var} → MANUAL
    if text.starts_with("${!") {
        let line = node.start_position().row;
        prefix_lines.push((line, "# MANUAL: ${!var} indirect expansion not supported in sash".into()));
    }

    // ${var@X} → MANUAL
    if text.contains('@') && text.starts_with("${") && text.ends_with('}') {
        let inner = &text[2..text.len()-1];
        if inner.contains('@') {
            let line = node.start_position().row;
            prefix_lines.push((line, "# MANUAL: ${var@X} parameter transformation not supported in sash — use ${var^^}/${var,,} for case".into()));
        }
    }
}

// ── Bless injection ───────────────────────────────────────────

/// Scan for variable assignments from command substitution and inject bless hints.
/// Called after the main walk — examines assignment nodes.
fn inject_bless_hints(
    node: Node,
    source: &[u8],
    prefix_lines: &mut Vec<(usize, String)>,
) {
    if node.kind() == "variable_assignment" {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        // Look for var=$(cmd) pattern
        let var_name = children.first()
            .filter(|c| c.kind() == "variable_name")
            .map(|c| node_text(*c, source));

        let has_cmdsub = children.iter().any(|c| {
            c.kind() == "command_substitution"
                || has_descendant_kind(*c, "command_substitution")
        });

        if let (Some(var), true) = (var_name, has_cmdsub) {
            let line = node.start_position().row;
            // Try to identify the command
            for child in &children {
                if let Some(cmd_name) = extract_command_name(*child, source) {
                    let args = extract_command_args(*child, source);
                    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                    if let Some(known) = known_externals::lookup_command(&cmd_name, &arg_refs) {
                        // Known external → real bless line
                        prefix_lines.push((line + 1, format!(
                            "bless {} [[ ${} =~ {} ]]  # auto: {}",
                            var, var, known.pattern, known.description
                        )));
                    } else {
                        // Unknown external → hint
                        prefix_lines.push((line + 1, format!(
                            "# BLESS: {} is assigned from '{}' — add a bless with appropriate pattern",
                            var, cmd_name
                        )));
                    }
                    return;
                }
            }
        }
    }

    // Check for read commands
    if node.kind() == "command" {
        if let Some(name_node) = node.child_by_field_name("name") {
            let name = node_text(name_node, source);
            if name == "read" {
                let line = node.start_position().row;
                // Extract variable names from read arguments
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.is_named() && child != name_node {
                        let t = node_text(child, source);
                        if !t.starts_with('-') && t.chars().all(|c| c.is_alphanumeric() || c == '_') {
                            prefix_lines.push((line + 1, format!(
                                "# BLESS: {} is read from input — add a bless with appropriate pattern", t
                            )));
                        }
                    }
                }
            }
        }
    }

    // Recurse
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        inject_bless_hints(child, source, prefix_lines);
    }
}

// ── SC2086 CHECK hints ────────────────────────────────────────

fn add_sc2086_hints(findings: &[ScFinding], prefix_lines: &mut Vec<(usize, String)>) {
    for f in findings {
        if f.code == 2086 {
            prefix_lines.push((f.line.saturating_sub(1), format!(
                "# CHECK: unquoted variable (SC2086) — word splitting won't occur in sash. Was splitting intended?"
            )));
        }
    }
}

// ── Patch application ─────────────────────────────────────────

fn apply_patches(
    source: &str,
    patches: &[Patch],
    prefix_lines: &[(usize, String)],
    annotate: bool,
) -> String {
    // Sort patches by start position
    let mut sorted: Vec<&Patch> = patches.iter().collect();
    sorted.sort_by_key(|p| p.start);

    // Build line→prefix_lines map
    let mut line_prefixes: std::collections::BTreeMap<usize, Vec<&str>> =
        std::collections::BTreeMap::new();
    for (line, text) in prefix_lines {
        line_prefixes.entry(*line).or_default().push(text);
    }

    // Apply patches to get the source with replacements
    let bytes = source.as_bytes();
    let mut result = String::with_capacity(source.len() + patches.len() * 32);
    let mut pos = 0;

    for patch in &sorted {
        if patch.start > pos {
            result.push_str(std::str::from_utf8(&bytes[pos..patch.start]).unwrap_or(""));
        }
        result.push_str(&patch.replacement);
        if annotate {
            if let Some(ref orig) = patch.original {
                result.push_str("  # was: ");
                result.push_str(orig);
            }
        }
        pos = patch.end;
    }
    if pos < bytes.len() {
        result.push_str(std::str::from_utf8(&bytes[pos..]).unwrap_or(""));
    }

    // Now insert prefix lines
    if line_prefixes.is_empty() {
        return result;
    }

    let lines: Vec<&str> = result.lines().collect();
    let mut final_out = String::with_capacity(result.len() + prefix_lines.len() * 80);
    for (i, line) in lines.iter().enumerate() {
        if let Some(prefixes) = line_prefixes.get(&i) {
            for p in prefixes {
                final_out.push_str(p);
                final_out.push('\n');
            }
        }
        final_out.push_str(line);
        final_out.push('\n');
    }
    // Handle prefix lines after the last source line
    for (line_num, prefixes) in &line_prefixes {
        if *line_num >= lines.len() {
            for p in prefixes {
                final_out.push_str(p);
                final_out.push('\n');
            }
        }
    }

    final_out
}

// ── Helpers ───────────────────────────────────────────────────

fn node_text<'a>(node: Node, source: &'a [u8]) -> &'a str {
    node.utf8_text(source).unwrap_or("")
}

fn has_descendant_kind(node: Node, kind: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind || has_descendant_kind(child, kind) {
            return true;
        }
    }
    false
}

fn extract_command_name(node: Node, source: &[u8]) -> Option<String> {
    if node.kind() == "command_substitution" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "command" {
                if let Some(name) = child.child_by_field_name("name") {
                    return Some(node_text(name, source).to_string());
                }
            }
        }
    }
    // Recurse into string/concatenation nodes
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(name) = extract_command_name(child, source) {
            return Some(name);
        }
    }
    None
}

fn extract_command_args(node: Node, source: &[u8]) -> Vec<String> {
    if node.kind() == "command_substitution" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "command" {
                let mut args = Vec::new();
                let mut past_name = false;
                let mut inner = child.walk();
                for arg in child.children(&mut inner) {
                    if arg.is_named() {
                        if !past_name {
                            past_name = true;
                            continue; // skip command name
                        }
                        args.push(node_text(arg, source).to_string());
                    }
                }
                return args;
            }
        }
    }
    Vec::new()
}
