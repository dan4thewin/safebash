/// sash-convert conversion engine.
///
/// Walks the tree-sitter CST, collects source patches (mechanical conversions,
/// MANUAL annotations, CHECK hints, ATTEST hints), and applies them to produce
/// sash-native source.
use crate::known_externals;
use crate::ScFinding;
use std::collections::HashSet;
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
    /// A zero-width insertion at `at`. Patches nested inside the same
    /// construct still apply, because this consumes none of the source.
    fn insert_at(at: usize, replacement: &str) -> Self {
        Patch {
            start: at,
            end: at,
            replacement: replacement.to_string(),
            prefix_line: None,
            original: None,
        }
    }

    /// Replace `node`'s source span with `replacement`. `original` is
    /// captured for --annotate; pass None when the original isn't
    /// already in hand.
    fn replace_node(
        node: tree_sitter::Node,
        replacement: String,
        original: Option<String>,
    ) -> Self {
        Patch::replace_span(node.start_byte(), node.end_byte(), replacement, original)
    }

    /// Replace an explicit byte range. Needed when the span to replace is
    /// not exactly a node: a command carrying a here-string must stop
    /// short of the `<<<`, which another patch is moving.
    fn replace_span(
        start: usize,
        end: usize,
        replacement: String,
        original: Option<String>,
    ) -> Self {
        Patch {
            start,
            end,
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

    // Ordering matters: `inject_attest_hints` places a hint *after* the
    // line it refers to (line + 1), while `walk_node` and the SC2086 pass
    // place theirs *before* (line). Those are the same insertion point,
    // and `apply_patches` keeps intra-line hints in push order — so the
    // after-hints must go in first or a MANUAL note reads as belonging to
    // the preceding command.
    inject_attest_hints(tree.root_node(), src, &mut prefix_lines);

    // Two things have to be settled before any expansion is rewritten: which
    // names are collections (`${#a}` means something different if `a` is one
    // — see `length_expansion`), and where bash declared one implicitly so a
    // DD#97 keyword has to be introduced.
    let mut pass = CollectionPass {
        patches: &mut patches,
        prefix_lines: &mut prefix_lines,
        names: HashSet::new(),
        maps: HashSet::new(),
        declared: HashSet::new(),
    };
    pass.walk(tree.root_node(), src);
    let collections = Names {
        all: pass.names,
        maps: pass.maps,
    };

    // Walk the CST
    walk_node(
        tree.root_node(),
        src,
        &mut patches,
        &mut prefix_lines,
        annotate,
        &collections,
    );

    // Add CHECK hints from shellcheck SC2086 findings
    add_sc2086_hints(sc_findings, &mut prefix_lines);

    // Collect prefix lines from patches (e.g., env var getenv+attest)
    for patch in &patches {
        if let Some(ref pl) = patch.prefix_line {
            // Insert before the line where the patch starts
            let line = source[..patch.start].matches('\n').count();
            prefix_lines.push((line, pl.clone()));
        }
    }

    // Two reads of the same source on one line (`echo "$1 $1"`, `echo "$HOME"
    // "$HOME"`) each request the same hoist. Emitting it twice is valid but is
    // pure noise, and the noise scales with how often the script reads the
    // value. Identical text on the same target line is one line.
    {
        let mut seen = std::collections::HashSet::new();
        prefix_lines.retain(|(line, text)| seen.insert((*line, text.clone())));
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
    collections: &Names,
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
            convert_simple_expansion(node, source, patches, prefix_lines);
        }
        "expansion" => {
            convert_expansion(node, source, patches, prefix_lines, collections);
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
        "word" => {
            convert_tilde_word(node, source, patches, prefix_lines);
        }
        // DD#83 removed here-strings. `<<<` is exactly a pipe in sash (no
        // subshell on either side, so a binding the consumer makes still
        // survives), and the word moves to the front of the statement.
        // Returning early skips the recursion into that word: its patches
        // were already applied to the copy being moved, and applying them
        // again here would collide with the deletion below.
        "herestring_redirect" => {
            convert_herestring(node, source, patches, prefix_lines, annotate, collections);
            return;
        }
        // tree-sitter-bash 0.25 cannot parse a nested subscript — `${a[a[0]]}`
        // yields an ERROR node spanning the whole expansion instead of an
        // `expansion`. The text is still exact, so the DD#80 rewrite applies
        // unchanged. Returning early skips the recursion, which would
        // otherwise patch the malformed children underneath this span.
        "ERROR" => {
            let text = node_text(node, source);
            if let Some(inner) = text.strip_prefix("${").and_then(|t| t.strip_suffix('}')) {
                if let Some(replacement) = subscript_expansion(inner, collections) {
                    patches.push(Patch::replace_node(
                        node,
                        replacement,
                        Some(text.to_string()),
                    ));
                    return;
                }
            }
        }
        _ => {}
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_node(child, source, patches, prefix_lines, annotate, collections);
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

    // DD#86: `local` is removed from sash — scope is structural (a name first
    // assigned in a function is auto-local). Drop the keyword; the type
    // keywords (int/array/map/const) from the flags stay.

    if is_readonly || flags.contains(&"-r".to_string()) {
        parts.push("const");
    }

    if flags.contains(&"-i".to_string()) {
        parts.push("int");
    }
    if flags.contains(&"-A".to_string()) {
        parts.push("map");
    } else if flags.contains(&"-a".to_string()) {
        parts.push("array");
    }

    // For bare `declare x=5` / `typeset x=5` / `local x=5` (no flags) → a bare
    // assignment (DD#86: `local` stripped, the keyword-less form is auto-local).
    let needs_conversion = is_declare || is_readonly || is_local;

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

        // DD#99: the read family. `read [-r] VAR` is the one mechanical
        // rewrite; every other spelling gets a MANUAL note rather than a
        // guess.
        "read" => convert_read(node, name_node, source, patches, prefix_lines),
        "mapfile" | "readarray" => convert_mapfile(node, name_node, source, patches, prefix_lines),

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

// ── DD#99: the read family ────────────────────────────────────

/// One `read`/`mapfile` argument, classified.
enum ReadArg {
    Flag(String),
    Name(String),
    Other,
}

fn classify_read_arg(text: &str) -> ReadArg {
    if text.starts_with('-') && text.len() > 1 {
        return ReadArg::Flag(text.to_string());
    }
    if !text.is_empty()
        && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !text.starts_with(|c: char| c.is_ascii_digit())
    {
        return ReadArg::Name(text.to_string());
    }
    ReadArg::Other
}

/// Split a command's arguments into flags and bare target names. `other`
/// is set when an argument is neither — a quoted string, an expansion, a
/// flag's own operand — which always forces a MANUAL note.
fn read_args(node: Node, name_node: Node, source: &[u8]) -> (Vec<String>, Vec<String>, bool) {
    let mut flags = Vec::new();
    let mut names = Vec::new();
    let mut other = false;
    let mut cursor = node.walk();
    let args = node
        .children(&mut cursor)
        .filter(|c| c.is_named() && *c != name_node && c.kind() != "variable_assignment")
        // A here-string is a redirect, not an argument — tree-sitter just
        // files it under `command` (see `herestring_child`). Counting it
        // made `read v <<< "$x"` look like a computed target.
        .filter(|c| c.kind() != "herestring_redirect");
    for child in args {
        match classify_read_arg(node_text(child, source)) {
            ReadArg::Flag(f) => flags.push(f),
            ReadArg::Name(n) => names.push(n),
            ReadArg::Other => other = true,
        }
    }
    (flags, names, other)
}

/// `IFS= read -r x` is the spelling `readline` matches exactly; every
/// other spelling of `read` trims leading/trailing IFS whitespace, which
/// `readline` does not.
fn has_empty_ifs_prefix(node: Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .filter(|c| c.kind() == "variable_assignment")
        .any(|c| node_text(c, source) == "IFS=");
    found
}

/// Why this `read` cannot be rewritten, or None if it can.
fn read_manual_reason(flags: &[String], names: &[String], other: bool) -> Option<String> {
    if let Some(f) = flags.iter().find(|f| f.as_str() != "-r") {
        return Some(read_flag_reason(f));
    }
    if other {
        return Some("# MANUAL: read target is not a plain name — sash binds a declared name, not a computed one (DD#99)".into());
    }
    match names.len() {
        1 => None,
        0 => Some(
            "# MANUAL: bare `read` fills $REPLY — sash has no REPLY, name a target: `readline VAR` (DD#99)"
                .into(),
        ),
        _ => Some(
            "# MANUAL: multi-target `read a b c` — no sash equivalent; use `readline ln` then `matchall $ln \"[^ ]+\" arr` (DD#99)"
                .into(),
        ),
    }
}

fn read_flag_reason(flag: &str) -> String {
    if flag.contains('a') {
        return "# MANUAL: `read -a` splits one line on IFS; sash `read` slurps all of stdin on NUL — use `readline ln` then `matchall $ln \"[^ ]+\" arr` (DD#99)".to_string();
    }
    format!(
        "# MANUAL: `read {}` — DD#99 retires every read flag; sash has only `readline VAR` and `read ARR`",
        flag
    )
}

fn convert_read(
    node: Node,
    name_node: Node,
    source: &[u8],
    patches: &mut Vec<Patch>,
    prefix_lines: &mut Vec<(usize, String)>,
) {
    let line = node.start_position().row;
    let (flags, names, other) = read_args(node, name_node, source);
    if let Some(reason) = read_manual_reason(&flags, &names, other) {
        prefix_lines.push((line, reason));
        return;
    }
    let end = span_end_before_herestring(node, source);
    let original = node_text(node, source)[..end - node.start_byte()].to_string();
    let replacement = format!("readline {}", names[0]);
    patches.push(Patch::replace_span(
        node.start_byte(),
        end,
        replacement,
        Some(original),
    ));
    if !has_empty_ifs_prefix(node, source) {
        prefix_lines.push((line, "# CHECK: bash `read` strips leading/trailing IFS whitespace; `readline` keeps the line verbatim (`IFS= read -r` was the exact equivalent)".into()));
    }
}

/// The node a `mapfile` rewrite must replace, plus the redirect text to
/// carry onto `tr`. `mapfile -t a < f` is a `redirected_statement`, and
/// replacing only the command would re-bind `< f` to `read`. None means
/// the redirect shape is not one we can move.
fn mapfile_target<'a>(node: Node<'a>, source: &[u8]) -> Option<(Node<'a>, String)> {
    let Some(parent) = node.parent() else {
        return Some((node, String::new()));
    };
    if parent.kind() != "redirected_statement" {
        return Some((node, String::new()));
    }
    let mut cursor = parent.walk();
    let redirects: Vec<Node> = parent
        .children_by_field_name("redirect", &mut cursor)
        .collect();
    let [redirect] = redirects[..] else {
        return None;
    };
    let text = node_text(redirect, source);
    text.starts_with('<')
        .then(|| (parent, format!(" {}", text)))
}

fn convert_mapfile(
    node: Node,
    name_node: Node,
    source: &[u8],
    patches: &mut Vec<Patch>,
    prefix_lines: &mut Vec<(usize, String)>,
) {
    let line = node.start_position().row;
    let cmd = node_text(name_node, source);
    let (flags, names, other) = read_args(node, name_node, source);
    let convertible = flags == ["-t"] && names.len() == 1 && !other;
    let target = mapfile_target(node, source).filter(|_| convertible);
    let Some((span, redirect)) = target else {
        prefix_lines.push((line, mapfile_manual_reason(cmd, &flags)));
        return;
    };
    // A here-string on the mapfile itself is being moved to the front of
    // the statement, so stop short of it rather than swallowing it:
    // `mapfile -t a <<< "$x"` becomes `echo $x | tr … | read a`.
    let end = span_end_before_herestring(span, source);
    let original = node_text(span, source)[..end - span.start_byte()].to_string();
    let replacement = format!(
        "/usr/bin/tr \"\\n\" \"\\0\"{} | read {}",
        redirect, names[0]
    );
    patches.push(Patch::replace_span(
        span.start_byte(),
        end,
        replacement,
        Some(original),
    ));
}

// ── Here-strings: `consumer <<< WORD` → `producer | consumer` (DD#83) ──

/// The `<<<` child of a command, if any.
///
/// tree-sitter-bash puts a here-string redirect *inside* the `command`
/// node, unlike `<` and `>`, which wrap the command in a
/// `redirected_statement`. Every pass that walks a command's children
/// therefore sees `<<<` as an argument unless it looks for it.
fn herestring_child<'a>(node: Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .find(|c| c.kind() == "herestring_redirect");
    found
}

/// The end of `node`'s span with any trailing here-string (and the blanks
/// in front of it) excluded. A rewrite that replaces a whole command must
/// stop here, or it swallows the `<<<` that the here-string rewrite is
/// separately moving to the front.
fn span_end_before_herestring(node: Node, source: &[u8]) -> usize {
    match herestring_child(node) {
        Some(hs) => leading_blank_start(hs, source),
        None => node.end_byte(),
    }
}

/// Byte offset at which the blanks preceding `node` begin.
fn leading_blank_start(node: Node, source: &[u8]) -> usize {
    let mut at = node.start_byte();
    while at > 0 && (source[at - 1] == b' ' || source[at - 1] == b'\t') {
        at -= 1;
    }
    at
}

/// Render `node`'s text with the converter's own rewrites applied to a
/// copy of it.
///
/// A here-string word moves to the front of the statement, so it cannot be
/// patched where it sits — the patches have to be applied to the text
/// being moved. `prefix_lines` is shared with the caller because the
/// rewrite never changes which line a statement starts on.
fn render_subtree(
    node: Node,
    source: &[u8],
    prefix_lines: &mut Vec<(usize, String)>,
    annotate: bool,
    collections: &Names,
) -> String {
    let mut sub = Vec::new();
    walk_node(node, source, &mut sub, prefix_lines, annotate, collections);
    let text = node_text(node, source);
    let base = node.start_byte();
    sub.sort_by_key(|p| (p.start, p.end));
    let mut out = String::with_capacity(text.len());
    let mut pos = base;
    for patch in &sub {
        if patch.start < pos {
            continue;
        }
        out.push_str(&text[pos - base..patch.start - base]);
        out.push_str(&patch.replacement);
        pos = patch.end;
    }
    out.push_str(&text[pos - base..]);
    out
}

/// The sash command that produces what bash's `<<< WORD` feeds in.
///
/// `<<<` expands the word and appends a newline, without word-splitting.
/// sash `echo` does exactly that — it never word-splits, so the bash
/// quoting carries over unchanged rather than having to be interpreted.
///
/// Two cases are not `echo`:
///
/// * A `$(…)` word. `echo $(cmd)` does not compile — a tainted command
///   substitution cannot be used inline (SB5020) — so the substituted
///   command becomes the producer directly. That also matches the taint
///   model, which tracks the pipe rather than a capture.
/// * The literal word `-n`. Both bash's and sash's `echo` read that as the
///   retired no-newline flag (SB5059 for the bare spelling, silence for
///   the quoted one), so neither spelling prints `-n`.
fn herestring_producer(
    word: Node,
    source: &[u8],
    prefix_lines: &mut Vec<(usize, String)>,
    annotate: bool,
    collections: &Names,
) -> String {
    if let Some(inner) = sole_command_substitution(word, source) {
        return render_subtree(inner, source, prefix_lines, annotate, collections);
    }
    let text = render_subtree(word, source, prefix_lines, annotate, collections);
    if matches!(text.as_str(), "-n" | "\"-n\"" | "'-n'") {
        return "printf \"%s\\n\" -n".to_string();
    }
    format!("echo {}", text)
}

/// The command inside a here-string word that is nothing but a command
/// substitution — `$(ls)` or `"$(ls)"`. Returns the substituted command,
/// which becomes the pipeline's producer.
fn sole_command_substitution<'a>(word: Node<'a>, source: &[u8]) -> Option<Node<'a>> {
    let sub = match word.kind() {
        "command_substitution" => word,
        "string" if word.named_child_count() == 1 => {
            let only = word.named_child(0)?;
            (only.kind() == "command_substitution" && only.end_byte() + 1 == word.end_byte())
                .then_some(only)?
        }
        _ => return None,
    };
    let text = node_text(sub, source);
    let inner = text.strip_prefix("$(")?.strip_suffix(')')?;
    (!inner.trim().is_empty()).then_some(())?;
    // The substituted command is every named child between `$(` and `)`;
    // one child is the overwhelmingly common case and the only one whose
    // text can be lifted verbatim.
    (sub.named_child_count() == 1).then(|| sub.named_child(0))?
}

fn convert_herestring(
    node: Node,
    source: &[u8],
    patches: &mut Vec<Patch>,
    prefix_lines: &mut Vec<(usize, String)>,
    annotate: bool,
    collections: &Names,
) {
    let line = node.start_position().row;
    let Some(word) = node.named_child(0) else {
        return;
    };
    let Some(statement) = node.parent() else {
        return;
    };
    let produces_capture = sole_command_substitution(word, source).is_some();
    let producer = herestring_producer(word, source, prefix_lines, annotate, collections);
    patches.push(Patch::insert_at(
        statement.start_byte(),
        &format!("{} | ", producer),
    ));
    patches.push(Patch::replace_span(
        leading_blank_start(node, source),
        node.end_byte(),
        String::new(),
        Some(node_text(node, source).to_string()),
    ));
    if produces_capture {
        prefix_lines.push((line, "# CHECK: `<<< \"$(cmd)\"` collapsed cmd's trailing newlines to one; the pipe passes them through".into()));
    }
}

fn mapfile_manual_reason(cmd: &str, flags: &[String]) -> String {
    if !flags.iter().any(|f| f.contains('t')) {
        return format!(
            "# MANUAL: `{}` without -t keeps each trailing newline — sash `read` splits on NUL and cannot retain the separator (DD#99)",
            cmd
        );
    }
    format!(
        "# MANUAL: `{}` — the convertible form is `{} -t ARR [< file]`, which becomes `/usr/bin/tr \"\\n\" \"\\0\" [< file] | read ARR` (DD#99)",
        cmd, cmd
    )
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

/// DD#53: bash tilde expansion. `~` and `~/path` expand to `$HOME`, which the
/// sash compiler rejects (SB4036, expr.rs). Rewrite the safe home forms to a
/// hoisted `$(getenv HOME)` + attest — mirroring the `$HOME` env-var conversion
/// — and leave the forms we cannot mechanically resolve unchanged with a
/// MANUAL note, so the SB4036 compile error forces a human to review them:
///   - `~user` / `~user/…`  (needs a passwd lookup sash does not do; the
///     compiler treats it as a literal, so behaviour silently differs)
///   - multi-tilde words like `~/bin:~/lib` (a partial rewrite of only the
///     leading `~` would silently drop the second one)
///   - a tilde in command-name position (`~/bin/prog`), which would become a
///     dynamic command path that DD#23 rejects.
///
/// `~/$x` parses as `concatenation(word "~/", simple_expansion)`, so the `~/`
/// arrives here as its own word: rewriting it to `$home/` and letting the
/// recursion handle `$x` composes correctly.
fn convert_tilde_word(
    node: Node,
    source: &[u8],
    patches: &mut Vec<Patch>,
    prefix_lines: &mut Vec<(usize, String)>,
) {
    let w = node_text(node, source);
    if !w.starts_with('~') {
        return;
    }
    let line = node.start_position().row;

    if node.parent().is_some_and(|p| p.kind() == "command_name") {
        prefix_lines.push((
            line,
            "# MANUAL: tilde in command position — use an explicit path".into(),
        ));
        return;
    }

    // Only the home forms `~` and `~/…` are mechanically resolvable, and only
    // when the tail holds no further tilde (`~/bin:~/lib` and friends must not
    // be partially rewritten).
    let resolvable = w == "~" || (w.starts_with("~/") && !w[2..].contains('~'));
    if !resolvable {
        prefix_lines.push((
            line,
            "# MANUAL: tilde form (~user / multi-tilde) not mechanically \
             resolvable — rewrite to an explicit $(getenv …) + attest"
                .into(),
        ));
        return;
    }

    // Mirror the $HOME env-var hoist: home=$(getenv HOME); attest home [[ … ]].
    let env = known_externals::lookup_env_var("HOME").expect("HOME is a known env var");
    patches.push(Patch {
        start: node.start_byte(),
        end: node.end_byte(),
        replacement: format!("$home{}", &w[1..]),
        prefix_line: Some(format!(
            "home=$(getenv HOME); attest home [[ $home =~ {} ]]",
            env.pattern
        )),
        original: None,
    });
}

fn convert_simple_expansion(
    node: Node,
    source: &[u8],
    patches: &mut Vec<Patch>,
    prefix_lines: &mut Vec<(usize, String)>,
) {
    // DD#46/DD#60: positionals are function-local. Inside a function body
    // they are the caller's args (clean) and pass through unchanged; only
    // top-level positionals are rewritten to the argv/arg/argc builtins.
    let in_function = has_ancestor_kind(node, "function_definition");
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let start = expansion_start(node, child, source);
        let end = node.end_byte();
        match child.kind() {
            "variable_name" => {
                let name = node_text(child, source);
                // Numeric positional parameter: $0 → $(arg 0), $N → $(arg N).
                if name.chars().all(|c| c.is_ascii_digit()) {
                    if !in_function {
                        patches.push(positional_hoist(start, end, name));
                    }
                } else if let Some(env) = crate::known_externals::lookup_env_var(name) {
                    // Known env var: $HOME → $(getenv HOME) + attest
                    let var = name.to_lowercase();
                    // Emit: var=$(getenv NAME); attest var [[ $var =~ pattern ]]
                    patches.push(Patch {
                        start,
                        end,
                        replacement: format!("${}", var),
                        prefix_line: Some(format!(
                            "{}=$(getenv {}); attest {} [[ ${} =~ {} ]]",
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
                // DD#60: $@/$* → $(argv 1) (array splat); $# → user-arg count
                // ($(argc) is C argc, so subtract the program-name slot).
                // A splat is unquoted in sash-native (DD#22), so when the
                // splat sits inside a double-quoted string ("$@") replace
                // the whole string to drop the quotes.
                match name {
                    "@" | "*" => convert_splat(node, start, source, patches, prefix_lines),
                    "#" => {
                        // `(argc)` is the DD#58 arithmetic sexpr call. The
                        // command-substitution spelling `$(argc)` is not an
                        // arithmetic operand: it used to parse as literal 0,
                        // so a converted `$#` printed **-1**, and it is now a
                        // hard SB4020.
                        patches.push(Patch {
                            start,
                            end,
                            replacement: "$(( (argc) - 1 ))".to_string(),
                            prefix_line: None,
                            original: None,
                        });
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

/// The byte offset of the `$` that opens this expansion.
///
/// Not `node.start_byte()`. Stock tree-sitter-bash 0.25 gives the second and
/// later `simple_expansion` nodes inside one double-quoted string a span that
/// includes the preceding whitespace: in `"$1 $1"` the second node spans
/// `" $1"` and in `"$HOME $HOME"` it spans `" $HOME"`. Replacing that span
/// deletes the separator, so `echo "$1 $1"` converted to `echo "$a$a"` — a
/// silent change to the program's output, in every expansion family the
/// converter rewrites.
///
/// The name child's own start is exact, and a `simple_expansion` is always a
/// `$` immediately followed by the name, so step back one byte and check.
fn expansion_start(node: Node, child: Node, source: &[u8]) -> usize {
    let candidate = child.start_byte().saturating_sub(1);
    if source.get(candidate) == Some(&b'$') {
        candidate
    } else {
        node.start_byte()
    }
}

/// The placeholder attest predicate. Permissive enough that the converted
/// script builds, weak enough that leaving it in place is visibly wrong.
const PLACEHOLDER_PREDICATE: &str = "-n";

/// A top-level `$N` read, hoisted to a named variable with a placeholder
/// attest — the `$HOME` model above.
///
/// `$(arg N)` is tainted (DD#60), so the inline substitution the converter
/// used to emit could never compile: every use raised SB5020. Emitting a
/// scaffold that builds, plus a MANUAL marker, beats emitting output whose
/// only feedback is a diagnostic the reader has to reverse-engineer.
fn positional_hoist(start: usize, end: usize, index: &str) -> Patch {
    let var = format!("arg{index}");
    Patch {
        start,
        end,
        replacement: format!("${var}"),
        prefix_line: Some(format!(
            "{var}=$(arg {index}); attest {var} [[ {PLACEHOLDER_PREDICATE} ${var} ]]  \
             # MANUAL: tighten this predicate — sash-convert cannot know what ${index} must look like"
        )),
        original: None,
    }
}

/// `$@` / `$*`. In a for-in header this is the iterate-the-arguments idiom and
/// converts mechanically; anywhere else it is a splat into a command, where
/// bash's empty case has no sash spelling and the author must choose one.
fn convert_splat(
    node: Node,
    start: usize,
    source: &[u8],
    patches: &mut Vec<Patch>,
    prefix_lines: &mut Vec<(usize, String)>,
) {
    let target = splat_replace_node(node, start);
    let Some(loop_node) = for_in_header_loop(node) else {
        // DD#22/SB5032: `cmd @a` is rejected because an empty array would run
        // the command with too few arguments, and the two spellings that state
        // the empty case (`@{a:-WORD}`, `@{a:?WORD}`) both diverge from bash,
        // which runs the command with zero arguments. There is no faithful
        // mechanical answer, so say so rather than pick one silently.
        prefix_lines.push((
            node.start_position().row,
            "# MANUAL: `\"$@\"` as a command argument — snapshot it (`argv 1 a`; \
             `attest e in a [[ -n $e ]]`) and pass `@{a:-WORD}` or `@{a:?WORD}`, \
             which states what an empty argument list should do (SB5032)"
                .into(),
        ));
        return;
    };

    // DD#60: `$(argv)` includes argv[0], the program name. Bash's `"$@"` is
    // argv[1:], so the slice form is the faithful one — `$(argv)` iterated one
    // extra time, with the program's own path as the first value.
    patches.push(Patch {
        start: if target.id() == node.id() {
            start
        } else {
            target.start_byte()
        },
        end: target.end_byte(),
        replacement: "$(argv 1)".to_string(),
        prefix_line: None,
        original: None,
    });

    // The loop variable is tainted per-iteration (DD#60 Q5), so the body needs
    // a leading attest. Insert it after `do` rather than as a prefix line: a
    // one-line `for …; do …; done` has no line of its own to prefix, and a
    // trailing `#` comment there would swallow the rest of the loop.
    let (Some(var), Some(body)) = (
        loop_node.child_by_field_name("variable"),
        loop_node.child_by_field_name("body"),
    ) else {
        return;
    };
    let Some(do_tok) = body.child(0) else {
        return;
    };
    let name = node_text(var, source);
    patches.push(Patch::insert_at(
        do_tok.end_byte(),
        &format!(" attest {name} [[ {PLACEHOLDER_PREDICATE} ${name} ]];"),
    ));
    prefix_lines.push((
        loop_node.start_position().row,
        format!(
            "# MANUAL: tighten the attest predicate on `{name}` — sash-convert \
             cannot know what an argument must look like"
        ),
    ));
}

/// The `for_statement` this node is the word-list of, if any. A node inside the
/// loop *body* is not in the header and returns `None`.
fn for_in_header_loop(node: Node) -> Option<Node> {
    let mut current = node.parent();
    while let Some(p) = current {
        if p.kind() == "for_statement" {
            let body = p.child_by_field_name("body")?;
            return (node.start_byte() < body.start_byte()).then_some(p);
        }
        current = p.parent();
    }
    None
}

fn convert_expansion(
    node: Node,
    source: &[u8],
    patches: &mut Vec<Patch>,
    prefix_lines: &mut Vec<(usize, String)>,
    collections: &Names,
) {
    let text = node_text(node, source);

    // ${1}, ${2}, etc. → $(arg N); ${@} → $(argv); ${#} → user-arg count.
    // Function-local positionals (inside a function body) pass through.
    if text.starts_with("${") && text.ends_with('}') {
        let inner = &text[2..text.len() - 1];
        let in_function = has_ancestor_kind(node, "function_definition");
        // The braced spellings are the same positionals as the bare ones, so
        // they take the same treatment — hoist + placeholder attest, the
        // argv[1:] slice, and the `(argc)` sexpr. `${…}` is matched on its own
        // text here, so its span is exact and needs no `expansion_start`.
        if inner.chars().all(|c| c.is_ascii_digit()) {
            if !in_function {
                patches.push(positional_hoist(node.start_byte(), node.end_byte(), inner));
            }
            return;
        }
        if inner == "@" || inner == "*" {
            if !in_function {
                convert_splat(node, node.start_byte(), source, patches, prefix_lines);
            }
            return;
        }
        if inner == "#" {
            if !in_function {
                patches.push(Patch::replace_node(
                    node,
                    "$(( (argc) - 1 ))".to_string(),
                    None,
                ));
            }
            return;
        }
        // DD#75 §4: the bash `${#x}` length sugar is retired in sash-native.
        // `${#}` (empty) is the arg count, handled above.
        if let Some(rest) = inner.strip_prefix('#') {
            if let Some(replacement) = length_expansion(rest, collections) {
                patches.push(Patch::replace_node(node, replacement, None));
                return;
            }
        }
        if let Some((name, kind)) = bracket_splat(inner) {
            convert_bracket_splat(node, name, kind, source, patches, prefix_lines, collections);
            return;
        }
        if let Some(replacement) = subscript_expansion(inner, collections) {
            patches.push(Patch::replace_node(
                node,
                replacement,
                Some(text.to_string()),
            ));
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
        if is_param_transform(inner) {
            let line = node.start_position().row;
            prefix_lines.push((line, "# MANUAL: ${var@X} parameter transformation not supported in sash — use ${var^^}/${var,,} for case".into()));
        }
    }
}

// ── Arithmetic side-effect extraction (DD#57) ─────────────────

/// DD#75 retired the `[@]`/`[*]` bracket forms (SB4038). Recognises
/// `${a[@]}`, `${a[*]}`, `${!a[@]}` and `${!a[*]}`, returning the collection
/// name and which form it was: `@` splat, `*` join, `!` keys.
///
/// `${#a[@]}` is *not* handled here — it reaches [`length_expansion`] first,
/// which already answers `$(len a)`.
fn bracket_splat(inner: &str) -> Option<(&str, char)> {
    let (body, keys) = match inner.strip_prefix('!') {
        Some(rest) => (rest, true),
        None => (inner, false),
    };
    let open = body.find('[')?;
    let name = &body[..open];
    if !is_clean_ident(name) {
        return None;
    }
    let sub = &body[open + 1..body.strip_suffix(']')?.len()];
    if sub != "@" && sub != "*" {
        return None;
    }
    Some((name, if keys { '!' } else { sub.chars().next()? }))
}

/// What replaces a retired bracket form depends on where it sits, because the
/// sash spellings are not interchangeable: `@a` is a splat and is legal only
/// as a for-in source, `$(join a)` is a scalar, and `$(keys m)` is a splat
/// that cannot be joined at all (SB5025).
fn convert_bracket_splat(
    node: Node,
    name: &str,
    kind: char,
    source: &[u8],
    patches: &mut Vec<Patch>,
    prefix_lines: &mut Vec<(usize, String)>,
    collections: &Names,
) {
    // A form that is a whole quoted word carries its quotes into the rewrite —
    // `"@a"` is rejected (SB5026) and `"$(join a)"` would be redundant. A form
    // spliced into a larger string (`"pre${a[@]}"`) is not a whole word, and
    // only the expansion itself is replaced.
    let target = match node.parent() {
        Some(p) if p.kind() == "string" && p.child_count() == 3 => p,
        _ => node,
    };
    let whole_arg = target
        .parent()
        .is_some_and(|p| p.kind() == "command" || p.kind() == "for_statement");
    let quoted_join = target.id() != node.id() && kind == '*';
    let in_for = for_in_header_loop(node).is_some();
    let line = node.start_position().row;

    if kind == '!' {
        if !in_for {
            prefix_lines.push((
                line,
                format!(
                    "# MANUAL: `${{!{name}[@]}}` outside a for-in header — `$(keys {name})` is a \
                 splat and cannot be joined or passed as an argument (SB5025). Iterate it, \
                 or build the string you need in the loop"
                ),
            ));
            return;
        }
        if collections.maps.contains(name) {
            patches.push(Patch::replace_node(target, format!("$(keys {name})"), None));
        } else {
            // An indexed array's "keys" are its indices 0..len-1, and sash has
            // no index-range sugar. Guessing a C-style loop would change the
            // program's shape, so say what the author has to write.
            prefix_lines.push((
                line,
                format!(
                    "# MANUAL: `${{!{name}[@]}}` on an indexed array yields the indices 0..len-1; \
                 sash has no index-range sugar. Use `for (( i = 0; i < $(len {name}); i++ ))`, \
                 or iterate the values with `for v in @{name}`"
                ),
            ));
        }
        return;
    }

    // `"${a[*]}"` is the joined string even in a for-in header — it is one
    // word, so the loop runs once. `${a[@]}` and unquoted `${a[*]}` are the
    // elements.
    if in_for && !quoted_join {
        patches.push(Patch::replace_node(target, format!("@{name}"), None));
        return;
    }

    // Outside a for-in header a splat is only legal into a command, and there
    // bash's empty case has no faithful sash spelling — the same wall `"$@"`
    // hits (SB5032). `echo` is the exception: it rejects a splat outright
    // (SB5030) and directs to a join, which is what bash's quoted form
    // produces anyway. `[*]` is always the joined string, so it never lands
    // here.
    if kind == '@' && whole_arg && enclosing_command_name(node, source).is_some_and(|c| c != "echo")
    {
        prefix_lines.push((
            line,
            format!(
                "# MANUAL: `${{{name}[@]}}` as a command argument — an empty array would run the \
             command with too few arguments, so state the empty case: `@{{{name}:-WORD}}` or \
             `@{{{name}:?WORD}}` (SB5032). Use `$(join {name})` if one joined argument was meant"
            ),
        ));
        return;
    }
    patches.push(Patch::replace_node(target, format!("$(join {name})"), None));
}

/// The name of the command this node is an argument of, if it is one.
fn enclosing_command_name<'a>(node: Node, source: &'a [u8]) -> Option<&'a str> {
    let mut current = node.parent();
    while let Some(p) = current {
        if p.kind() == "command" {
            let mut cursor = p.walk();
            let name = p
                .children(&mut cursor)
                .find(|c| c.kind() == "command_name")?;
            return Some(node_text(name, source));
        }
        current = p.parent();
    }
    None
}

/// DD#80: `${a[K]}` takes a **direct** key — an integer literal, `$var`, an
/// expansion, a quoted string, or (maps only) a clean-identifier bareword.
/// Bash evaluates every subscript arithmetically instead, so the idioms it
/// accepts convert to sash that does not compile: `${a[i]}` is SB4030 (bare
/// string in arithmetic) and `${a[i+1]}` / `${a[a[0]]}` are SB4041 (ambiguous
/// braced index).
///
/// Returns the rewritten `${…}` text, or `None` to leave the expansion alone.
///
/// `inner` is the text between `${` and `}`.
fn subscript_expansion(inner: &str, collections: &Names) -> Option<String> {
    let open = inner.find('[')?;
    if !inner.ends_with(']') {
        return None;
    }
    let name = &inner[..open];
    if name.is_empty() || !is_clean_ident(name) {
        return None;
    }
    let key = &inner[open + 1..inner.len() - 1];
    let rewritten = subscript_key(key, collections.maps.contains(name))?;
    Some(format!("${{{}[{}]}}", name, rewritten))
}

/// The key half of [`subscript_expansion`]. `None` means "already direct,
/// leave it".
fn subscript_key(key: &str, is_map: bool) -> Option<String> {
    // Already direct: an expansion, a quoted literal, or a splat. An integer
    // literal is direct too, and is by far the common case.
    if key.is_empty()
        || key.starts_with('$')
        || key.starts_with('"')
        || key.starts_with('\'')
        || key == "@"
        || key == "*"
        || is_int_literal(key)
    {
        return None;
    }
    // A bareword key on a map is already a direct key — that is the one place
    // DD#80 keeps the bare spelling. On an array the same text is a variable
    // read in bash, so it has to become one here.
    if is_clean_ident(key) {
        return if is_map {
            None
        } else {
            Some(format!("${}", key))
        };
    }
    // A nested subscript `b[…]` is a value, so it nests as an expansion rather
    // than opening arithmetic: `${a[${b[0]}]}`. Anything else is an expression
    // and gets explicit arithmetic. Per DD#66 the arithmetic form is the safe
    // default for an ambiguous name, since an int-indexed array is what bash
    // meant.
    if let Some(open) = key.find('[') {
        if key.ends_with(']') && is_clean_ident(&key[..open]) {
            return Some(format!("${{{}}}", key));
        }
    }
    Some(format!("$(( {} ))", key))
}

/// `${var@X}` — the parameter-transformation form, which sash does not have.
/// `@` also appears as a splat subscript (`${a[@]}`, `${a[@]:1}`), which is a
/// different construct entirely and must not draw the MANUAL note.
fn is_param_transform(inner: &str) -> bool {
    let Some((base, op)) = inner.rsplit_once('@') else {
        return false;
    };
    if op.len() != 1 || !op.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return false;
    }
    let base = base.split_once('[').map_or(base, |(name, _)| name);
    is_clean_ident(base)
}

/// A bash/sash identifier: leading letter or `_`, then alphanumerics or `_`.
fn is_clean_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A decimal integer literal, optionally signed.
fn is_int_literal(s: &str) -> bool {
    let digits = s.strip_prefix('-').unwrap_or(s);
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

/// If a variable_assignment has an arithmetic_expansion whose inner expression
/// contains an assignment (e.g. `y=$((x=10))`), extract the side-effect as a
/// separate `(( x=10 ))` statement on the preceding line and leave the variable
/// assigned to the result.
/// bash's `${#…}` sugar collapses four different questions into one
/// spelling family, and the old converter collapsed them further by stripping
/// the subscript — so `${#a}` (bash: 1) and `${#a[@]}` (bash: 3) both became
/// `$(len a)`. After DD#75 split `len` from `strlen`, each row is expressible:
///
/// | bash             | meaning                  | sash                |
/// |------------------|--------------------------|---------------------|
/// | `${#s}`          | byte length of a scalar  | `$(strlen $s)`      |
/// | `${#a}`          | byte length of element 0 | `$(strlen ${a[0]})` |
/// | `${#a[@]}`/`[*]` | element count            | `$(len a)`          |
/// | `${#a[i]}`       | byte length of element i | `$(strlen ${a[i]})` |
///
/// The subscript decides every row but the bare one, which depends on whether
/// the name is a collection — hence `collections`. A wrong guess there cannot
/// pass silently: `$a` on a collection is SB5064 and `s[0]` on a scalar is
/// SB5068, both naming the fix.
///
/// `None` leaves the expansion alone: `${##…}` is a prefix strip, not a
/// length, and a non-identifier base is some other form entirely.
fn length_expansion(rest: &str, collections: &Names) -> Option<String> {
    if rest.is_empty() || rest.starts_with('#') {
        return None;
    }
    let (base, subscript) = match rest.split_once('[') {
        Some((b, sub)) => (b, Some(sub.strip_suffix(']')?)),
        None => (rest, None),
    };
    if base.is_empty() || !base.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    Some(match subscript {
        Some("@") | Some("*") => format!("$(len {})", base),
        Some(index) => format!("$(strlen ${{{}[{}]}})", base, index),
        None if collections.all.contains(base) => format!("$(strlen ${{{}[0]}})", base),
        None => format!("$(strlen ${})", base),
    })
}

/// What the pre-pass learned about names, threaded through the rewrite walk.
pub(crate) struct Names {
    /// Every name used as a collection anywhere in the script.
    all: HashSet<String>,
    /// The subset declared `declare -A` / `local -A`, i.e. maps. A map
    /// subscript is a key, not an arithmetic index (DD#80).
    maps: HashSet<String>,
}

/// The pre-pass state: what names are collections, and which constructs need
/// a DD#97 keyword introduced.
struct CollectionPass<'a> {
    patches: &'a mut Vec<Patch>,
    prefix_lines: &'a mut Vec<(usize, String)>,
    /// Every name used as a collection anywhere in the script, for
    /// [`length_expansion`].
    names: HashSet<String>,
    /// The subset of `names` declared `declare -A` / `local -A`, i.e. maps.
    /// A map subscript is a key, not an arithmetic index, so it converts
    /// differently (DD#80) — see [`subscript_expansion`].
    maps: HashSet<String>,
    /// Names that already carry a keyword at this point in the source, so the
    /// keyword is introduced once, at the first construct that mentions each.
    declared: HashSet<String>,
}

impl CollectionPass<'_> {
    /// DD#97: a collection is introduced by the `array`/`map` keyword, but
    /// bash declares implicitly — `a=(x y z)`, `a[0]=x`, `mapfile -t a` and
    /// `read -a a` all create an array with no declaration to convert. Left
    /// alone they convert to a program that does not compile: SB5055 for the
    /// literal, SB5068 for the subscript.
    ///
    /// `declare -a`/`-A` already convert to `array`/`map`
    /// (see `convert_declaration`), so names they cover are left alone.
    ///
    /// Pre-order recursion is source order, which is what "first mention"
    /// means here.
    fn walk(&mut self, node: Node, source: &[u8]) {
        match node.kind() {
            "variable_assignment" => self.assignment(node, source),
            "declaration_command" => self.declaration(node, source),
            "command" => self.command(node, source),
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk(child, source);
        }
    }

    /// Record `name` as a collection. Returns whether this mention is the one
    /// that has to carry the keyword.
    fn introduce(&mut self, name: &str) -> bool {
        self.names.insert(name.to_string());
        self.declared.insert(name.to_string())
    }

    /// `array a` on its own line, for the forms that cannot take the keyword
    /// inline — a subscripted assignment or a command that fills an array.
    fn declare_before(&mut self, node: Node, name: &str) {
        if self.introduce(name) {
            self.prefix_lines
                .push((node.start_position().row, format!("array {}", name)));
        }
    }

    /// `a=(x y z)` takes the keyword inline; `a[0]=x` needs its own line.
    fn assignment(&mut self, node: Node, source: &[u8]) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        let Some(target) = children.first() else {
            return;
        };
        if target.kind() == "subscript" {
            if let Some(name) = child_variable_name(*target, source) {
                self.declare_before(node, &name);
            }
            return;
        }
        if target.kind() != "variable_name" || !children.iter().any(|c| c.kind() == "array") {
            return;
        }
        let name = node_text(*target, source).to_string();
        if self.introduce(&name) {
            // A zero-width insert, so patches on words *inside* the literal
            // (tilde expansion, say) still apply.
            self.patches
                .push(Patch::insert_at(node.start_byte(), "array "));
        }
    }

    /// `declare -a`/`-A`, including combined flags like `-ra`, and the
    /// `declare -a a=(x y)` form that nests the name in an assignment.
    fn declaration(&mut self, node: Node, source: &[u8]) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        let flag = children.iter().find_map(|c| {
            let t = node_text(*c, source);
            (c.kind() == "word" && t.starts_with('-') && t.contains(['a', 'A'])).then_some(t)
        });
        let Some(flag) = flag else {
            return;
        };
        let is_map = flag.contains('A');
        for child in &children {
            let name = match child.kind() {
                "variable_name" => Some(node_text(*child, source).to_string()),
                "variable_assignment" => child_variable_name(*child, source),
                _ => None,
            };
            if let Some(name) = name {
                if is_map {
                    self.maps.insert(name.clone());
                }
                self.introduce(&name);
            }
        }
    }

    /// `mapfile`/`readarray` fill an array, and so does `read -a`.
    fn command(&mut self, node: Node, source: &[u8]) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        let Some(name_node) = children.iter().find(|c| c.kind() == "command_name") else {
            return;
        };
        let words: Vec<&Node> = children.iter().filter(|c| c.kind() == "word").collect();
        let has_a_flag = words.iter().any(|w| {
            let t = node_text(**w, source);
            t.starts_with('-') && t.contains('a')
        });
        let fills_array = match node_text(*name_node, source) {
            "mapfile" | "readarray" => true,
            "read" => has_a_flag,
            _ => return,
        };
        if !fills_array {
            return;
        }
        for word in words {
            let t = node_text(*word, source).to_string();
            if !t.starts_with('-') && t.chars().all(|c| c.is_alphanumeric() || c == '_') {
                self.declare_before(node, &t);
            }
        }
    }
}

/// The `variable_name` a node holds directly, as an owned string — the cursor
/// a tree-sitter child search needs cannot outlive the borrow otherwise.
fn child_variable_name(node: Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .find(|n| n.kind() == "variable_name")?;
    Some(node_text(found, source).to_string())
}

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

// ── Attest injection ───────────────────────────────────────────

/// Scan for variable assignments from command substitution and inject attest hints.
/// Called after the main walk — examines assignment nodes.
fn inject_attest_hints(node: Node, source: &[u8], prefix_lines: &mut Vec<(usize, String)>) {
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
                        // Known external → real attest line
                        prefix_lines.push((
                            line + 1,
                            format!(
                                "attest {} [[ ${} =~ {} ]]  # auto: {}",
                                var, var, known.pattern, known.description
                            ),
                        ));
                    } else {
                        // Unknown external → hint
                        prefix_lines.push((line + 1, format!(
                            "# ATTEST: {} is assigned from '{}' — add a attest with appropriate pattern",
                            var, cmd_name
                        )));
                    }
                    return;
                }
            }
        }
    }

    // Check for read-family commands. The converted target is tainted
    // either way; only the attest *form* differs (DD#99: `readline` binds
    // a scalar, `read`/`mapfile` bind a collection).
    if node.kind() == "command" {
        if let Some(name_node) = node.child_by_field_name("name") {
            let name = node_text(name_node, source);
            if matches!(name, "read" | "mapfile" | "readarray") {
                push_read_attest_hints(node, name_node, source, prefix_lines);
            }
        }
    }

    // Recurse
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        inject_attest_hints(child, source, prefix_lines);
    }
}

/// A collection target needs `attest x in NAME [[ … ]]`; a scalar needs
/// `attest NAME [[ … ]]`. Naming the wrong one would send the reader at a
/// diagnostic they cannot satisfy.
fn push_read_attest_hints(
    node: Node,
    name_node: Node,
    source: &[u8],
    prefix_lines: &mut Vec<(usize, String)>,
) {
    let line = node.start_position().row;
    let cmd = node_text(name_node, source);
    let (flags, names, _) = read_args(node, name_node, source);
    let collection = cmd != "read" || flags.iter().any(|f| f.contains('a'));
    let form = if collection {
        "attest x in {} [[ … ]]"
    } else {
        "attest {} [[ … ]]"
    };
    for target in names {
        prefix_lines.push((
            line + 1,
            format!(
                "# ATTEST: {} is read from input — add `{}`",
                target,
                form.replace("{}", &target)
            ),
        ));
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
    // Tie-break on `end` so a zero-width insertion at X is emitted before
    // a replacement that starts at X and consumes forward. Without it the
    // here-string rewrite (insert the producer at the statement start,
    // rewrite the consumer in place) depends on push order.
    sorted.sort_by_key(|p| (p.start, p.end));

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
fn splat_replace_node(node: Node, start: usize) -> Node {
    if let Some(parent) = node.parent() {
        if parent.kind() == "string"
            && parent.start_byte() + 1 == start
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
