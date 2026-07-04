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

impl Patch {
    /// Replace `node`'s source span with `replacement`. `original` is
    /// captured for --annotate; pass None when the original isn't
    /// already in hand.
    fn replace_node(
        node: tree_sitter::Node,
        replacement: String,
        original: Option<String>,
    ) -> Self {
        Patch {
            start: node.start_byte(),
            end: node.end_byte(),
            replacement,
            prefix_line: None,
            original,
        }
    }
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
    walk_node(
        tree.root_node(),
        src,
        &mut patches,
        &mut prefix_lines,
        annotate,
    );

    // Inject bless hints for command substitution assignments
    inject_bless_hints(tree.root_node(), src, &mut prefix_lines);

    // Add CHECK hints from shellcheck SC2086 findings
    add_sc2086_hints(sc_findings, &mut prefix_lines);

    // Collect prefix lines from patches (e.g., env var getenv+bless)
    for patch in &patches {
        if let Some(ref pl) = patch.prefix_line {
            // Insert before the line where the patch starts
            let line = source[..patch.start].matches('\n').count();
            prefix_lines.push((line, pl.clone()));
        }
    }

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
        "unset_command" => {
            convert_unset_command(node, source, patches);
        }
        "test_command" => {
            convert_test_command(node, source, patches, prefix_lines);
        }
        "subshell" => {
            // Subshell: mark as MANUAL for now (mechanical conversion is complex)
            let line = node.start_position().row;
            let text = node_text(node, source);
            prefix_lines.push((
                line,
                "# MANUAL: subshell ( ... ) — convert to function with local vars, or remove"
                    .to_string(),
            ));
            let _ = text; // pass through unchanged
        }
        "simple_expansion" => {
            convert_simple_expansion(node, source, patches);
        }
        "expansion" => {
            convert_expansion(node, source, patches, prefix_lines);
        }
        "for_statement" if node.child(0).is_some_and(|c| c.kind() == "select") => {
            let line = node.start_position().row;
            prefix_lines.push((
                line,
                "# MANUAL: select — not supported in sash, rewrite as while+read menu".into(),
            ));
        }
        "process_substitution" => {
            let line = node.start_position().row;
            prefix_lines.push((
                line,
                "# MANUAL: process substitution <()/>() not supported in sash".into(),
            ));
        }
        "command_substitution" => {
            if has_ancestor_kind(node, "heredoc_body") {
                let line = node.start_position().row;
                prefix_lines.push((line, "# MANUAL: command substitution $(…) inside heredoc — extract before heredoc or rewrite".into()));
            }
            convert_backtick_substitution(node, source, patches);
        }
        "variable_assignment" => {
            convert_arith_side_effect(node, source, patches, prefix_lines);
        }
        "case_statement" => {
            // DD#66 removed `case`/`esac` — the `[[ ]]` dispatch form replaces
            // it. A faithful mechanical rewrite (one dispatch op across mixed
            // exact/glob arms, glob→regex, nested arm-body patches) is a design
            // decision, so annotate for now like `subshell`. Recursion below
            // still converts inner constructs. (followup task:
            // converter-case-to-dispatch)
            let line = node.start_position().row;
            prefix_lines.push((
                line,
                "# MANUAL: case removed (DD#66) — use the [[ ]] dispatch form: \
                 `[[ $x == pat { … } other { … } || { … } ]]` (exact) or \
                 `[[ $x =~ regex { … } ]]` (regex)"
                    .into(),
            ));
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

fn convert_declaration(
    node: Node,
    source: &[u8],
    patches: &mut Vec<Patch>,
    prefix_lines: &mut Vec<(usize, String)>,
) {
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
        prefix_lines.push((
            line,
            "# MANUAL: declare -n (nameref) not supported in sash".into(),
        ));
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
    let needs_conversion = is_declare || is_readonly || (is_local && has_type_flag);

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
        patches.push(Patch::replace_node(node, replacement, Some(original)));
    }
}

// ── Command conversions ───────────────────────────────────────

fn convert_unset_command(node: Node, source: &[u8], patches: &mut Vec<Patch>) {
    // unset var → var=""
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "variable_name" {
            let var_name = node_text(child, source);
            let original = node_text(node, source).to_string();
            let replacement = format!("{}=\"\"", var_name);
            patches.push(Patch::replace_node(node, replacement, Some(original)));
            break;
        }
    }
}

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
            patches.push(Patch::replace_node(node, replacement, Some(original)));
        }

        // MANUAL annotations for unconvertible commands
        "eval" => {
            prefix_lines.push((
                line,
                "# MANUAL: eval — dynamic execution not supported in sash".into(),
            ));
        }
        "source" | "." => {
            prefix_lines.push((
                line,
                "# MANUAL: source/. — use sash-expand pipeline instead (DD#54)".into(),
            ));
        }
        "export" => {
            prefix_lines.push((line, "# MANUAL: export — env isolation in sash (DD#53), use /usr/bin/env for per-command env".into()));
        }
        "alias" => {
            prefix_lines.push((
                line,
                "# MANUAL: alias — not supported in sash, convert to a function".into(),
            ));
        }
        "set" => {
            prefix_lines.push((
                line,
                "# MANUAL: set — shell options not configurable in sash".into(),
            ));
        }
        "shopt" => {
            prefix_lines.push((
                line,
                "# MANUAL: shopt — shell options not configurable in sash".into(),
            ));
        }
        "select" => {
            prefix_lines.push((
                line,
                "# MANUAL: select — not supported in sash, rewrite as while+read menu".into(),
            ));
        }
        "coproc" => {
            prefix_lines.push((line, "# MANUAL: coproc — not supported in sash".into()));
        }

        // Mechanical: trap → signal N DISPOSITION
        "trap" => {
            let mut cursor = node.walk();
            let args: Vec<Node> = node
                .children(&mut cursor)
                .filter(|c| c.is_named() && *c != name_node)
                .collect();
            if let Some(first_arg) = args.first() {
                let arg_text = node_text(*first_arg, source);
                if arg_text == "\"\"" || arg_text == "''" {
                    // trap "" SIG → signal N IGN
                    let sigs: Vec<&str> = args[1..].iter().map(|a| node_text(*a, source)).collect();
                    let original = node_text(node, source).to_string();
                    let lines: Vec<String> = sigs
                        .iter()
                        .filter_map(|s| sig_to_num(s).map(|n| format!("signal {} IGN", n)))
                        .collect();
                    if !lines.is_empty() {
                        patches.push(Patch::replace_node(node, lines.join("\n"), Some(original)));
                    }
                } else if arg_text == "-" {
                    // trap - SIG → signal N DFL
                    let sigs: Vec<&str> = args[1..].iter().map(|a| node_text(*a, source)).collect();
                    let original = node_text(node, source).to_string();
                    let lines: Vec<String> = sigs
                        .iter()
                        .filter_map(|s| sig_to_num(s).map(|n| format!("signal {} DFL", n)))
                        .collect();
                    if !lines.is_empty() {
                        patches.push(Patch::replace_node(node, lines.join("\n"), Some(original)));
                    }
                } else if arg_text == ":" {
                    // trap : SIG → signal N FLAG
                    let sigs: Vec<&str> = args[1..].iter().map(|a| node_text(*a, source)).collect();
                    let original = node_text(node, source).to_string();
                    let lines: Vec<String> = sigs
                        .iter()
                        .filter_map(|s| sig_to_num(s).map(|n| format!("signal {} FLAG", n)))
                        .collect();
                    if !lines.is_empty() {
                        patches.push(Patch::replace_node(node, lines.join("\n"), Some(original)));
                    }
                } else {
                    // trap 'handler' SIG → MANUAL
                    prefix_lines.push((
                        line,
                        "# MANUAL: trap with handler body — restructure as END { ... } block"
                            .into(),
                    ));
                }
            }
        }

        _ => {}
    }
}

// ── Test command conversion: [ ] → [[ ]] ─────────────────────

/// Strip unnecessary double-quotes around `$var` / `${var}` in test expressions.
/// In `[[ ]]`, word splitting doesn't apply, so quoting simple variable
/// references is unnecessary.  This produces idiomatic `[[ $var ... ]]`.
fn strip_unnecessary_quotes(inner: &str) -> String {
    let mut result = String::with_capacity(inner.len());
    let chars: Vec<char> = inner.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '"' {
            // Look ahead: is this "...$var..." or just "$var"?
            if let Some(close) = chars[i + 1..].iter().position(|&c| c == '"') {
                let close = i + 1 + close;
                let quoted: String = chars[i + 1..close].iter().collect();
                // Only strip if the entire quoted content is a single $var or ${var}
                let is_simple_var = quoted.starts_with('$')
                    && quoted[1..]
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '{' || c == '}');
                if is_simple_var {
                    result.push_str(&quoted);
                    i = close + 1;
                    continue;
                }
            }
            result.push(chars[i]);
            i += 1;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

fn convert_test_command(
    node: Node,
    source: &[u8],
    patches: &mut Vec<Patch>,
    prefix_lines: &mut Vec<(usize, String)>,
) {
    let text = node_text(node, source);

    // Check for numeric comparisons — in either [ ] or [[ ]] form
    let (is_double, is_single) = (
        text.starts_with("[[ ") && text.ends_with(" ]]"),
        text.starts_with("[ ") && text.ends_with(" ]") && !text.starts_with("[[ "),
    );
    if is_double || is_single {
        let (start_trim, end_trim) = if is_double { (3, 3) } else { (2, 2) };
        let inner = text[start_trim..text.len() - end_trim].trim();
        match try_numeric_comparison(inner) {
            NumericResult::Converted(replacement) => {
                patches.push(Patch::replace_node(
                    node,
                    replacement,
                    Some(text.to_string()),
                ));
                return;
            }
            NumericResult::TooComplex => {
                let line = node.start_position().row;
                prefix_lines.push((line, "# MANUAL: numeric comparison needs conversion to (( )) — operands too complex for auto-convert".to_string()));
                // Still convert [ ] → [[ ]] below if single bracket
                if is_double {
                    return;
                }
            }
            NumericResult::NotNumeric => {}
        }
    }

    // Only convert [ ... ] to [[ ... ]], not already [[ ... ]]
    if text.starts_with("[ ") && text.ends_with(" ]") && !text.starts_with("[[ ") {
        let inner = &text[2..text.len() - 2];
        // In [[ ]], quoting around simple $var is unnecessary — strip it
        // to produce idiomatic [[ ]] syntax.
        let inner = strip_unnecessary_quotes(inner);
        let replacement = format!("[[ {} ]]", inner);
        patches.push(Patch::replace_node(
            node,
            replacement,
            Some(text.to_string()),
        ));
    }

    // Fix regex patterns: convert POSIX [-...] and [...-] to Plan 9 [\-...]
    if text.starts_with("[[ ") && text.ends_with(" ]]") && text.contains(" =~ ") {
        let inner = &text[3..text.len() - 3];
        if let Some(re_start) = inner.find(" =~ ") {
            let pattern = &inner[re_start + 4..];
            let fixed = fix_regex_dashes(pattern);
            if fixed != pattern {
                let full = format!("[[ {} =~ {} ]]", &inner[..re_start], fixed);
                patches.push(Patch::replace_node(node, full, Some(text.to_string())));
            }
        }
    }
}

/// Rewrite POSIX-style character class dashes to Plan 9 escaped form.
/// [-abc] → [\-abc], [abc-] → [abc\-], [a-z-] → [a-z\-]
fn fix_regex_dashes(pattern: &str) -> String {
    let bytes = pattern.as_bytes();
    let mut result = Vec::with_capacity(bytes.len() + 4);
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            result.push(b'[');
            i += 1;
            // handle optional ^
            if i < bytes.len() && bytes[i] == b'^' {
                result.push(b'^');
                i += 1;
            }
            // dash at start of class → \-
            if i < bytes.len() && bytes[i] == b'-' {
                result.push(b'\\');
                result.push(b'-');
                i += 1;
            }
            // rest of class
            while i < bytes.len() && bytes[i] != b']' {
                // dash before ] → \-
                if bytes[i] == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b']' {
                    result.push(b'\\');
                    result.push(b'-');
                    i += 1;
                } else {
                    result.push(bytes[i]);
                    i += 1;
                }
            }
            if i < bytes.len() {
                result.push(b']'); // the ]
                i += 1;
            }
        } else {
            result.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(result).unwrap_or_else(|_| pattern.to_string())
}

enum NumericResult {
    Converted(String),
    TooComplex,
    NotNumeric,
}

/// Try to convert a numeric comparison like `$x -gt 5` to `(( x > 5 ))`.
/// Only converts when operands are bare $var or integer literals.
fn try_numeric_comparison(inner: &str) -> NumericResult {
    let parts: Vec<&str> = inner.split_whitespace().collect();
    if parts.len() != 3 {
        return NumericResult::NotNumeric;
    }
    let op = match parts[1] {
        "-eq" => "==",
        "-ne" => "!=",
        "-lt" => "<",
        "-le" => "<=",
        "-gt" => ">",
        "-ge" => ">=",
        _ => return NumericResult::NotNumeric,
    };
    // Check if operands are simple: bare $var, ${var}, or integer literal
    let lhs_simple = is_simple_arith_operand(parts[0]);
    let rhs_simple = is_simple_arith_operand(parts[2]);
    if !lhs_simple || !rhs_simple {
        return NumericResult::TooComplex;
    }
    let lhs = strip_dollar(parts[0]);
    let rhs = strip_dollar(parts[2]);
    NumericResult::Converted(format!("(( {} {} {} ))", lhs, op, rhs))
}

/// A simple arithmetic operand is: $var, ${var}, or an integer literal (optional leading -)
fn is_simple_arith_operand(s: &str) -> bool {
    if let Some(rest) = s.strip_prefix('$') {
        // $var or ${var}
        let inner = rest
            .strip_prefix('{')
            .and_then(|r| r.strip_suffix('}'))
            .unwrap_or(rest);
        inner.chars().all(|c| c.is_alphanumeric() || c == '_')
    } else {
        // integer literal
        let s = s.strip_prefix('-').unwrap_or(s);
        !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
    }
}

/// Strip leading $ from a variable reference (for arithmetic context).
/// $var → var, ${var} → var, literals pass through unchanged.
fn strip_dollar(s: &str) -> &str {
    if let Some(rest) = s.strip_prefix('$') {
        if let Some(inner) = rest.strip_prefix('{') {
            inner.strip_suffix('}').unwrap_or(rest)
        } else {
            rest
        }
    } else {
        s
    }
}

// ── Expansion conversions ─────────────────────────────────────

fn convert_simple_expansion(node: Node, source: &[u8], patches: &mut Vec<Patch>) {
    // DD#46/DD#60: positionals are function-local. Inside a function body
    // they are the caller's args (clean) and pass through unchanged; only
    // top-level positionals are rewritten to the argv/arg/argc builtins.
    let in_function = has_ancestor_kind(node, "function_definition");
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "variable_name" => {
                let name = node_text(child, source);
                // Numeric positional parameter: $0 → $(arg 0), $N → $(arg N).
                if name.chars().all(|c| c.is_ascii_digit()) {
                    if !in_function {
                        let replacement = format!("$(arg {})", name);
                        patches.push(Patch::replace_node(node, replacement, None));
                    }
                } else if let Some(env) = crate::known_externals::lookup_env_var(name) {
                    // Known env var: $HOME → $(getenv HOME) + bless
                    let var = name.to_lowercase();
                    let _line = node.start_position().row + 1;
                    let _getenv = format!("$(getenv {})", name);
                    // Emit: var=$(getenv NAME); bless var [[ $var =~ pattern ]]
                    patches.push(Patch {
                        start: node.start_byte(),
                        end: node.end_byte(),
                        replacement: format!("${}", var),
                        prefix_line: Some(format!(
                            "{}=$(getenv {}); bless {} [[ ${} =~ {} ]]",
                            var, name, var, var, env.pattern
                        )),
                        original: None,
                    });
                }
            }
            "special_variable_name" => {
                if in_function {
                    continue;
                }
                let name = node_text(child, source);
                // DD#60: $@/$* → $(argv) (array splat); $# → user-arg count
                // ($(argc) is C argc, so subtract the program-name slot).
                // A splat is unquoted in sash-native (DD#22), so when the
                // splat sits inside a double-quoted string ("$@") replace
                // the whole string to drop the quotes.
                match name {
                    "@" | "*" => {
                        let target = splat_replace_node(node);
                        patches.push(Patch::replace_node(target, "$(argv)".to_string(), None));
                    }
                    "#" => {
                        patches.push(Patch::replace_node(
                            node,
                            "$(($(argc) - 1))".to_string(),
                            None,
                        ));
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

fn convert_expansion(
    node: Node,
    source: &[u8],
    patches: &mut Vec<Patch>,
    prefix_lines: &mut Vec<(usize, String)>,
) {
    let text = node_text(node, source);

    // ${1}, ${2}, etc. → $(arg N); ${@} → $(argv); ${#} → user-arg count.
    // Function-local positionals (inside a function body) pass through.
    if text.starts_with("${") && text.ends_with('}') {
        let inner = &text[2..text.len() - 1];
        let in_function = has_ancestor_kind(node, "function_definition");
        if inner.chars().all(|c| c.is_ascii_digit()) {
            if !in_function {
                patches.push(Patch::replace_node(node, format!("$(arg {})", inner), None));
            }
            return;
        }
        if inner == "@" || inner == "*" {
            if !in_function {
                let target = splat_replace_node(node);
                patches.push(Patch::replace_node(target, "$(argv)".to_string(), None));
            }
            return;
        }
        if inner == "#" {
            if !in_function {
                patches.push(Patch::replace_node(
                    node,
                    "$(($(argc) - 1))".to_string(),
                    None,
                ));
            }
            return;
        }
    }

    // ${var-x} → ${var:-x}  (but not ${var:-x} which already has the colon)
    // The expansion node text includes ${ and }
    // Look for pattern: ${NAME-default} where there's no : before the -
    if text.contains('-') && !text.contains(":-") && !text.contains(":+") && !text.contains(":?") {
        // Check it's a simple default: ${var-value}
        if let Some(dash_pos) = text.find('-') {
            // Make sure it's not ${var:offset:length} arithmetic
            let before_dash = &text[2..dash_pos]; // skip ${
            if !before_dash.contains(':')
                && before_dash.chars().all(|c| c.is_alphanumeric() || c == '_')
            {
                let replacement = format!("${{{}:-{}", before_dash, &text[dash_pos + 1..]);
                patches.push(Patch::replace_node(
                    node,
                    replacement,
                    Some(text.to_string()),
                ));
            }
        }
    }

    // ${!var} → MANUAL
    if text.starts_with("${!") {
        let line = node.start_position().row;
        prefix_lines.push((
            line,
            "# MANUAL: ${!var} indirect expansion not supported in sash".into(),
        ));
    }

    // ${var@X} → MANUAL
    if text.contains('@') && text.starts_with("${") && text.ends_with('}') {
        let inner = &text[2..text.len() - 1];
        if inner.contains('@') {
            let line = node.start_position().row;
            prefix_lines.push((line, "# MANUAL: ${var@X} parameter transformation not supported in sash — use ${var^^}/${var,,} for case".into()));
        }
    }
}

// ── Arithmetic side-effect extraction (DD#57) ─────────────────

/// If a variable_assignment has an arithmetic_expansion whose inner expression
/// contains an assignment (e.g. `y=$((x=10))`), extract the side-effect as a
/// separate `(( x=10 ))` statement on the preceding line and leave the variable
/// assigned to the result.
fn convert_arith_side_effect(
    node: Node,
    source: &[u8],
    _patches: &mut Vec<Patch>,
    prefix_lines: &mut Vec<(usize, String)>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "arithmetic_expansion" {
            // Get the inner text between $(( and ))
            let full = node_text(child, source);
            if let Some(inner) = full.strip_prefix("$((").and_then(|s| s.strip_suffix("))")) {
                let inner = inner.trim();
                // Check if it contains an assignment (= but not == or != or <= or >=)
                if contains_arith_assignment(inner) {
                    let line = node.start_position().row;
                    prefix_lines.push((line, format!("(( {} ))", inner)));
                }
            }
        }
    }
}

/// Check if an arithmetic expression contains an assignment operator.
/// Must distinguish `=` from `==`, `!=`, `<=`, `>=`.
fn contains_arith_assignment(expr: &str) -> bool {
    let bytes = expr.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] == b'=' {
            // Check it's not ==, !=, <=, >=
            let prev = if i > 0 { bytes[i - 1] } else { 0 };
            let next = if i + 1 < bytes.len() { bytes[i + 1] } else { 0 };
            if next == b'=' {
                continue;
            } // ==
            if prev == b'!' || prev == b'<' || prev == b'>' {
                continue;
            } // !=, <=, >=
              // Also skip +=, -=, *=, /=, %=, &=, |=, ^=, <<=, >>=
            if prev == b'+'
                || prev == b'-'
                || prev == b'*'
                || prev == b'/'
                || prev == b'%'
                || prev == b'&'
                || prev == b'|'
                || prev == b'^'
            {
                // These are compound assignments — still side effects, include them
                return true;
            }
            return true;
        }
    }
    false
}

// ── Bless injection ───────────────────────────────────────────

/// Scan for variable assignments from command substitution and inject bless hints.
/// Called after the main walk — examines assignment nodes.
fn inject_bless_hints(node: Node, source: &[u8], prefix_lines: &mut Vec<(usize, String)>) {
    if node.kind() == "variable_assignment" {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        // Look for var=$(cmd) pattern
        let var_name = children
            .first()
            .filter(|c| c.kind() == "variable_name")
            .map(|c| node_text(*c, source));

        let has_cmdsub = children.iter().any(|c| {
            c.kind() == "command_substitution" || has_descendant_kind(*c, "command_substitution")
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
                        prefix_lines.push((
                            line + 1,
                            format!(
                                "bless {} [[ ${} =~ {} ]]  # auto: {}",
                                var, var, known.pattern, known.description
                            ),
                        ));
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
                        if !t.starts_with('-') && t.chars().all(|c| c.is_alphanumeric() || c == '_')
                        {
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
        // SC2086: double quote to prevent globbing/word splitting
        // SC2248: prefer double quoting (optional check, supersedes SC2086 for simple cases)
        if f.code == 2086 || f.code == 2248 {
            prefix_lines.push((f.line.saturating_sub(1), "# CHECK: unquoted variable (SC2086) — word splitting won't occur in sash. Was splitting intended?".to_string()));
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

fn sig_to_num(name: &str) -> Option<u8> {
    let s = name.strip_prefix("SIG").unwrap_or(name);
    match s {
        "HUP" => Some(1),
        "INT" => Some(2),
        "QUIT" => Some(3),
        "ABRT" => Some(6),
        "KILL" => Some(9),
        "USR1" => Some(10),
        "USR2" => Some(12),
        "PIPE" => Some(13),
        "ALRM" => Some(14),
        "TERM" => Some(15),
        _ => s.parse().ok(),
    }
}

/// For a `$@`/`$*`/`${@}` splat that sits inside a double-quoted string
/// that contains nothing else (`"$@"`), return the enclosing string node so
/// the replacement drops the quotes — sash-native splats are bare (DD#22).
/// Otherwise return the node itself.
fn splat_replace_node(node: Node) -> Node {
    if let Some(parent) = node.parent() {
        if parent.kind() == "string"
            && parent.start_byte() + 1 == node.start_byte()
            && parent.end_byte() == node.end_byte() + 1
        {
            return parent;
        }
    }
    node
}

fn has_ancestor_kind(node: Node, kind: &str) -> bool {
    let mut current = node.parent();
    while let Some(p) = current {
        if p.kind() == kind {
            return true;
        }
        current = p.parent();
    }
    false
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

fn convert_backtick_substitution(node: Node, source: &[u8], patches: &mut Vec<Patch>) {
    // Only convert backtick-style command substitutions, not $() ones
    let text = node_text(node, source);
    if text.starts_with('`') && text.ends_with('`') {
        let start = node.start_byte();
        let end = node.end_byte();
        // Replace opening backtick with $(
        patches.push(Patch {
            start,
            end: start + 1,
            replacement: "$(".to_string(),
            prefix_line: None,
            original: None,
        });
        patches.push(Patch {
            start: end - 1,
            end,
            replacement: ")".to_string(),
            prefix_line: None,
            original: None,
        });
    }
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
