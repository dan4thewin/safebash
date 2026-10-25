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
    let mut collections = Names {
        all: pass.names,
        maps: pass.maps,
        ints: collect_int_names(tree.root_node(), src),
        int_typed: HashSet::new(),
    };
    collections.int_typed = collect_int_typed(tree.root_node(), src, &collections.ints);

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
    // DD#30: sash's `[[ ]]` compares strings, and an int operand there is
    // SB4030. Bash draws no such line, so an operand the converted script
    // holds as an int is quoted, which is sash's int→string conversion. The
    // patches are zero-width, so the operand's own rewrite (`$?` →
    // `$(status)`) still applies between them.
    if is_int_expansion(node, source, &collections.int_typed)
        && is_string_test_operand(node, source)
    {
        patches.push(Patch::insert_at(node.start_byte(), "\""));
        patches.push(Patch::insert_at(node.end_byte(), "\""));
    }
    match node.kind() {
        "declaration_command" => {
            if convert_declaration(node, source, patches, prefix_lines, annotate, collections) {
                return;
            }
        }
        "command" => {
            if convert_command(node, source, patches, prefix_lines, annotate, collections) {
                return;
            }
        }
        "unset_command" => {
            if convert_unset_command(node, source, patches) {
                return;
            }
        }
        "test_command" => {
            if convert_test_command(node, source, patches, prefix_lines, annotate, collections) {
                return;
            }
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
        // A read in arithmetic needs an explicit string→int disposition
        // (SB4030). These three arms are the three spellings of one read:
        // `$i`, `${i}` and bare `i`. Each returns without recursing, because
        // the patch already covers the whole span.
        "simple_expansion" => {
            if let Some(name_node) = node.child_by_field_name("name").or_else(|| node.child(1)) {
                if name_node.kind() == "variable_name" {
                    if let Some(d) = arith_disposition(name_node, node, source, collections) {
                        patches.push(Patch::replace_node(
                            node,
                            d,
                            Some(node_text(node, source).to_string()),
                        ));
                        return;
                    }
                }
            }
            convert_simple_expansion(node, source, patches, prefix_lines);
        }
        "variable_name" => {
            if let Some(d) = arith_disposition(node, node, source, collections) {
                patches.push(Patch::replace_node(
                    node,
                    d,
                    Some(node_text(node, source).to_string()),
                ));
                return;
            }
        }
        "expansion" => {
            // `${i}` — a bare braced read. The node text has to be tested,
            // not just the child count: `${#s}` is also one `variable_name`
            // child, and treating it as a plain read drops the length
            // operator and silently changes what the program computes.
            if is_bare_braced_read(node, source) {
                if let Some(inner) = node.named_child(0) {
                    if inner.kind() == "variable_name" {
                        if let Some(d) = arith_disposition(inner, node, source, collections) {
                            patches.push(Patch::replace_node(
                                node,
                                d,
                                Some(node_text(node, source).to_string()),
                            ));
                            return;
                        }
                    }
                }
            }
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
        "unary_expression" => {
            convert_prefix_incdec(node, source, patches, prefix_lines);
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
            // Any other ERROR is a construct the grammar could not parse, so
            // no conversion arm ever sees it and the text is emitted verbatim.
            // That is not harmless silence: `[ a=b ]` parses as an assignment
            // prefix plus a command named `]`, so it survives conversion and
            // then *compiles*, with DD#23 resolving `[` to the external
            // /usr/bin/[. The user asked for a conversion and got a fork.
            // Nothing is unsound — the manifest discloses the external and
            // coreutils `[` is what bash's builtin implements — but a
            // downgrade has to be disclosed rather than inferred.
            //
            // Only the OUTERMOST ERROR is reported. tree-sitter fragments a
            // malformed region into nested ERRORs — `if [ a=b ]; then …`
            // yields one for the whole statement and another for the stray
            // `; then` inside it — and the inner spans are artefacts of the
            // failed parse, not separate things the user has to fix.
            //
            // The guard suppresses only the note: recursion continues, so a
            // recognisable construct nested deeper inside the bad span (a
            // DD#80 subscript, say) is still rewritten.
            if !has_ancestor_kind(node, "ERROR") {
                let line = node.start_position().row;
                prefix_lines.push((
                    line,
                    format!(
                        "# MANUAL: could not parse `{}` — passed through unconverted",
                        summarize_span(text)
                    ),
                ));
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

/// One-line, bounded rendering of an unparsed span for a `# MANUAL:` note.
///
/// The span can be long or multi-line, and the note has to stay a single
/// comment line. Collapsing to the first line keeps it readable while still
/// naming the construct the user has to look at.
fn summarize_span(text: &str) -> String {
    const MAX: usize = 48;
    let first = text.lines().next().unwrap_or("").trim();
    if first.chars().count() <= MAX {
        return first.to_string();
    }
    let head: String = first.chars().take(MAX).collect();
    format!("{}…", head)
}

// ── Declaration conversions ───────────────────────────────────

/// Returns true when the whole node was replaced — the caller must then
/// not recurse into the children (see `convert_test_command`).
fn convert_declaration(
    node: Node,
    source: &[u8],
    patches: &mut Vec<Patch>,
    prefix_lines: &mut Vec<(usize, String)>,
    annotate: bool,
    collections: &Names,
) -> bool {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    if children.is_empty() {
        return false;
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
        return false;
    }

    // export → MANUAL
    if keyword_text == "export" {
        let line = node.start_position().row;
        prefix_lines.push((line, "# MANUAL: export — env isolation in sash (DD#53), use /usr/bin/env for per-command env".into()));
        return false;
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
        return false;
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

    // The value side can hold anything the converter rewrites — a known env
    // var, a positional, `$?` — so it has to be the converted text.
    let mut sub_prefix = Vec::new();
    let assign_text = assign_start.map(|start| {
        render_range(
            node,
            source,
            start,
            node.end_byte(),
            &mut sub_prefix,
            annotate,
            collections,
        )
    });
    if let Some(ref text) = assign_text {
        let text = text.trim();
        if !text.is_empty() {
            parts.push(text);
        }
    }

    let replacement = parts.join(" ");
    let original = node_text(node, source).to_string();

    if replacement != original {
        patches.push(Patch::replace_node(node, replacement, Some(original)));
        prefix_lines.append(&mut sub_prefix);
        return true;
    }
    false
}

// ── Command conversions ───────────────────────────────────────

/// Returns true when the whole node was replaced. `unset` takes bare
/// names, so there is nothing inside it for the converter to rewrite —
/// but the caller must still skip the children, since any patch under a
/// replaced node corrupts the output.
fn convert_unset_command(node: Node, source: &[u8], patches: &mut Vec<Patch>) -> bool {
    // unset var → var=""
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "variable_name" {
            let var_name = node_text(child, source);
            let original = node_text(node, source).to_string();
            let replacement = format!("{}=\"\"", var_name);
            patches.push(Patch::replace_node(node, replacement, Some(original)));
            return true;
        }
    }
    false
}

/// Returns true when the whole node was replaced — the caller must then
/// not recurse into the children.
fn convert_command(
    node: Node,
    source: &[u8],
    patches: &mut Vec<Patch>,
    prefix_lines: &mut Vec<(usize, String)>,
    annotate: bool,
    collections: &Names,
) -> bool {
    let name_node = match node.child_by_field_name("name") {
        Some(n) => n,
        None => return false,
    };
    let name = node_text(name_node, source);
    let line = node.start_position().row;

    if matches!(name, "true" | "false") {
        add_true_false_hint(node, name, line, prefix_lines);
    }

    match name {
        // Mechanical: let → (( ))
        "let" => {
            let original = node_text(node, source).to_string();
            let mut exprs = Vec::new();
            let mut sub_prefix = Vec::new();
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() && child != name_node {
                    // The expression is arithmetic (`in_arithmetic` knows
                    // `let`), so `$?` becomes `(status)` and `${#s}`
                    // becomes `(strlen $s)` — the sexpr spellings.
                    let t = render_subtree(child, source, &mut sub_prefix, annotate, collections);
                    let t = t.trim_matches('"').trim_matches('\'');
                    exprs.push(t.to_string());
                }
            }
            let replacement = format!("(( {} ))", exprs.join(", "));
            patches.push(Patch::replace_node(node, replacement, Some(original)));
            prefix_lines.append(&mut sub_prefix);
            return true;
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
        "mapfile" | "readarray" => convert_mapfile(
            node,
            name_node,
            source,
            patches,
            prefix_lines,
            annotate,
            collections,
        ),

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
                        return true;
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
                        return true;
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
                        return true;
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
    false
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
fn mapfile_target<'a>(node: Node<'a>, source: &[u8]) -> Option<(Node<'a>, Option<Node<'a>>)> {
    let Some(parent) = node.parent() else {
        return Some((node, None));
    };
    if parent.kind() != "redirected_statement" {
        return Some((node, None));
    }
    let mut cursor = parent.walk();
    let redirects: Vec<Node> = parent
        .children_by_field_name("redirect", &mut cursor)
        .collect();
    let [redirect] = redirects[..] else {
        return None;
    };
    node_text(redirect, source)
        .starts_with('<')
        .then_some((parent, Some(redirect)))
}

fn convert_mapfile(
    node: Node,
    name_node: Node,
    source: &[u8],
    patches: &mut Vec<Patch>,
    prefix_lines: &mut Vec<(usize, String)>,
    annotate: bool,
    collections: &Names,
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
    // `mapfile -t a < "$HOME"` — the redirect word is carried onto `tr`,
    // so it must be carried in its converted form.
    let redirect = match redirect {
        Some(r) => format!(
            " {}",
            render_subtree(r, source, prefix_lines, annotate, collections)
        ),
        None => String::new(),
    };
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
    splice_patches(
        node_text(node, source),
        node.start_byte(),
        &mut sub,
        source,
        prefix_lines,
    )
}

/// Like `render_subtree`, but walks only `node`'s children — for a caller
/// that is itself the handler for `node`'s kind and would otherwise
/// recurse into itself.
fn render_children(
    node: Node,
    source: &[u8],
    prefix_lines: &mut Vec<(usize, String)>,
    annotate: bool,
    collections: &Names,
) -> String {
    let mut sub = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_node(child, source, &mut sub, prefix_lines, annotate, collections);
    }
    splice_patches(
        node_text(node, source),
        node.start_byte(),
        &mut sub,
        source,
        prefix_lines,
    )
}

/// Render the byte range `[from, to)` of `node`'s subtree with the
/// converter's own rewrites applied to a copy.
///
/// For a handler that replaces its whole node but builds the replacement
/// out of part of the source text: that text has to be the *converted*
/// text, or the inner rewrites are either lost or — worse — emitted
/// alongside the outer one and duplicate the source.
fn render_range(
    node: Node,
    source: &[u8],
    from: usize,
    to: usize,
    prefix_lines: &mut Vec<(usize, String)>,
    annotate: bool,
    collections: &Names,
) -> String {
    let mut sub = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_node(child, source, &mut sub, prefix_lines, annotate, collections);
    }
    sub.retain(|p| p.start >= from && p.end <= to);
    let text = std::str::from_utf8(&source[from..to]).unwrap_or("");
    splice_patches(text, from, &mut sub, source, prefix_lines)
}

/// Apply `patches` (absolute byte offsets) to `text`, which starts at
/// `base`. Patches that overlap an earlier one are skipped — they cannot
/// be expressed in the output, and emitting them anyway duplicates source.
fn splice_patches(
    text: &str,
    base: usize,
    patches: &mut [Patch],
    source: &[u8],
    prefix_lines: &mut Vec<(usize, String)>,
) -> String {
    // A spliced patch never reaches `convert`'s hoist loop, so carry its
    // own prefix line across here — `$HOME` inside a `[ … ]` still needs
    // its `getenv`+`attest` pair emitted above the statement.
    for patch in patches.iter() {
        if let Some(ref pl) = patch.prefix_line {
            let line = source[..patch.start]
                .iter()
                .filter(|b| **b == b'\n')
                .count();
            prefix_lines.push((line, pl.clone()));
        }
    }
    patches.sort_by_key(|p| (p.start, p.end));
    let mut out = String::with_capacity(text.len());
    let mut pos = base;
    for patch in patches.iter() {
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
///
/// Except for a name in `int_typed`: there the quotes are the int→string
/// conversion sash requires (DD#30), and stripping them is SB4030.
fn strip_unnecessary_quotes(inner: &str, int_typed: &HashSet<String>) -> String {
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
                let name = quoted.get(1..).unwrap_or("").trim_matches(['{', '}']);
                if is_simple_var && !int_typed.contains(name) {
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

/// Rewrite a `[ … ]` / `[[ … ]]` test. Returns true when it replaced the
/// whole node, in which case the caller must **not** recurse into the
/// children: their patches are already spliced into the replacement.
///
/// A whole-node replacement and a patch inside that node cannot both be
/// applied — the inner one re-emits source the outer one already consumed,
/// and the output is silently corrupt. `if [ "$1" = x ]` used to convert
/// to `if [[ $1 = x ]]$arg1" = x ]`, and every `[ … ]` holding a known env
/// var, a positional or a `${#s}` had the same shape.
fn convert_test_command(
    node: Node,
    source: &[u8],
    patches: &mut Vec<Patch>,
    prefix_lines: &mut Vec<(usize, String)>,
    annotate: bool,
    collections: &Names,
) -> bool {
    let raw = node_text(node, source);
    // Rendering the children has to happen before the decisions, because
    // they read the text — but its prefix lines are only correct if the
    // replacement is actually emitted, so they are held aside until then.
    let mut sub_prefix = Vec::new();
    let rendered = render_children(node, source, &mut sub_prefix, annotate, collections);
    let text = rendered.as_str();

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
                    Some(raw.to_string()),
                ));
                prefix_lines.append(&mut sub_prefix);
                return true;
            }
            NumericResult::TooComplex => {
                let line = node.start_position().row;
                prefix_lines.push((line, "# MANUAL: numeric comparison needs conversion to (( )) — operands too complex for auto-convert".to_string()));
                // Still convert [ ] → [[ ]] below if single bracket
                if is_double {
                    return false;
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
        let inner = strip_unnecessary_quotes(inner, &collections.int_typed);
        let inner = fix_test_operators(&inner);
        let replacement = format!("[[ {} ]]", inner);
        patches.push(Patch::replace_node(
            node,
            replacement,
            Some(raw.to_string()),
        ));
        prefix_lines.append(&mut sub_prefix);
        return true;
    }

    // Fix regex patterns: convert POSIX [-...] and [...-] to Plan 9 [\-...]
    if text.starts_with("[[ ") && text.ends_with(" ]]") && text.contains(" =~ ") {
        let inner = &text[3..text.len() - 3];
        if let Some(re_start) = inner.find(" =~ ") {
            let pattern = &inner[re_start + 4..];
            let fixed = fix_regex_dashes(pattern);
            if fixed != pattern {
                let full = format!("[[ {} =~ {} ]]", &inner[..re_start], fixed);
                patches.push(Patch::replace_node(node, full, Some(raw.to_string())));
                prefix_lines.append(&mut sub_prefix);
                return true;
            }
        }
    }
    // An input that was ALREADY `[[ ]]` skips the branch above, so the DD#118
    // operator rewrites have to be applied to it here too.
    if text.starts_with("[[ ") && text.ends_with(" ]]") {
        let inner = &text[3..text.len() - 3];
        let fixed = fix_test_operators(inner);
        if fixed != inner {
            patches.push(Patch::replace_node(
                node,
                format!("[[ {} ]]", fixed),
                Some(raw.to_string()),
            ));
            prefix_lines.append(&mut sub_prefix);
            return true;
        }
    }

    false
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

/// DD#118: sash rejects both of bash's loose test spellings — a single `=`
/// (which always assigns) and the implicit `-n` of a bare word. Rewrite them
/// to the explicit forms so converted output actually compiles.
fn fix_test_operators(inner: &str) -> String {
    let toks: Vec<&str> = inner.split_whitespace().collect();
    if toks.is_empty() {
        return inner.to_string();
    }
    let mut out: Vec<String> = Vec::new();
    let mut seg: Vec<&str> = Vec::new();
    for t in toks {
        if t == "&&" || t == "||" {
            out.push(fix_test_segment(&seg));
            out.push(t.to_string());
            seg.clear();
        } else {
            seg.push(t);
        }
    }
    out.push(fix_test_segment(&seg));
    out.join(" ")
}

/// One conjunct of a test. A leading `!` is held aside so the operand rules
/// below see the same shape either way.
fn fix_test_segment(toks: &[&str]) -> String {
    if toks.is_empty() {
        return String::new();
    }
    let negated = toks[0] == "!";
    let body = if negated { &toks[1..] } else { toks };
    if body.is_empty() {
        return toks.join(" ");
    }
    let mut v: Vec<String> = body.iter().map(|s| (*s).to_string()).collect();
    if body.len() == 1 {
        // A lone operand was bash's implicit `-n`.
        v.insert(0, "-n".to_string());
    } else if let Some(i) = body.iter().skip(1).position(|t| *t == "=") {
        // Only the first standalone `=` can be the operator; a later one is
        // inside a quoted operand.
        v[i + 1] = "==".to_string();
    }
    if negated {
        let mut r = vec!["!".to_string()];
        r.extend(v);
        r.join(" ")
    } else {
        v.join(" ")
    }
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

/// The DD#75 canonical keyword spelling of a punctuation special, or None
/// if the name is not one of the three.
///
/// The spelling depends on where the expansion sits. In command position
/// the keyword is a command substitution, `$(status)`; as an arithmetic
/// operand it is the DD#58 sexpr, `(status)`. They are not
/// interchangeable — `$(( $(status) + 0 ))` is SB4020, the same trap `$#`
/// hit (see the `(argc)` note in `convert_simple_expansion`).
///
/// `$?`/`$$`/`$!` are process-wide, so unlike the positional specials they
/// mean the same thing inside a function body as outside it.
fn canonical_special(name: &str, arith: bool) -> Option<&'static str> {
    let keyword = match name {
        "?" => "status",
        "$" => "getpid",
        "!" => "getcpid",
        _ => return None,
    };
    Some(match (keyword, arith) {
        ("status", false) => "$(status)",
        ("status", true) => "(status)",
        ("getpid", false) => "$(getpid)",
        ("getpid", true) => "(getpid)",
        ("getcpid", false) => "$(getcpid)",
        _ => "(getcpid)",
    })
}

// ── DD#67 / SB4030: string→int disposition (DG, 2026-09-14) ───
//
// Bash has one scalar type, so `i=1` is a string that arithmetic silently
// coerces. Sash makes the coercion explicit: a bare string in arithmetic is
// SB4030 and must carry `${i:-N}` (default) or `${i:?msg}` (abort).
//
// The converter cannot derive the right answer from bash source — the source
// does not contain it. DG chose **abort**: emit `${i:?msg}`. It preserves the
// "bad input is an error" reading rather than inventing a zero, at the cost of
// diverging from bash on non-numeric input, where bash would have used 0. That
// divergence is deliberate and is the point: a script that silently computed
// with a garbage operand now says so.
//
// Names declared `declare -i` convert to sash `int` and need nothing.

/// The message the converter attaches to every disposition it introduces. It
/// names the variable so a failure at runtime is self-explaining, and it is
/// recognisable as machine-written so a reader knows to revisit it.
fn abort_disposition(name: &str) -> String {
    format!("${{{}:?{} is not an integer}}", name, name)
}

/// Pre-pass: every name a `declare -i` / `local -i` / `typeset -i` covers.
/// These become sash `int`, so arithmetic on them is already well-typed.
fn collect_int_names(node: Node, source: &[u8]) -> HashSet<String> {
    let mut ints = HashSet::new();
    collect_int_names_into(node, source, &mut ints);
    ints
}

fn collect_int_names_into(node: Node, source: &[u8], ints: &mut HashSet<String>) {
    if node.kind() == "declaration_command" {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        let has_i = children.iter().skip(1).any(|c| {
            let t = node_text(*c, source);
            c.kind() == "word" && t.starts_with('-') && t[1..].contains('i')
        });
        if has_i {
            for child in children.iter().skip(1) {
                match child.kind() {
                    "variable_name" => {
                        ints.insert(node_text(*child, source).to_string());
                    }
                    "variable_assignment" => {
                        if let Some(n) = child.child_by_field_name("name") {
                            ints.insert(node_text(n, source).to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_int_names_into(child, source, ints);
    }
}

/// Pre-pass: every name the converted script may hold as a sash `int` — the
/// `-i` names, every arithmetic write target (`(( k = 3 ))`, `let j=4`,
/// `k++`, a C-style `for` variable) and, to a fixpoint, every copy of an
/// int-valued expansion (`rc=$?`, `n=$(( … ))`, `q=$p` for an int `p`).
///
/// A superset is safe: the set only decides which `[[ ]]` operands get
/// quoted (see [`is_int_expansion`]), and quoting a whole operand never
/// changes what a sash test means. A name it misses is SB4030 at compile
/// time, never a silent change.
fn collect_int_typed(root: Node, source: &[u8], ints: &HashSet<String>) -> HashSet<String> {
    let mut typed = ints.clone();
    collect_arith_writes(root, source, &mut typed);
    while collect_int_copies(root, source, &mut typed) {}
    typed
}

/// The assignment and `++`/`--` targets of every arithmetic context.
fn collect_arith_writes(node: Node, source: &[u8], typed: &mut HashSet<String>) {
    let written = match node.kind() {
        "binary_expression" if in_arithmetic(node, source) => node
            .child_by_field_name("operator")
            .filter(|op| is_assign_operator(node_text(*op, source)))
            .and_then(|_| node.child_by_field_name("left"))
            .and_then(|left| written_name(left, source)),
        "postfix_expression" | "unary_expression" if in_arithmetic(node, source) => node
            .child_by_field_name("operator")
            .filter(|op| matches!(node_text(*op, source), "++" | "--"))
            .and_then(|_| prefix_incdec_operand(node, source)),
        "variable_assignment"
            if node
                .parent()
                .is_some_and(|p| p.kind() == "c_style_for_statement") =>
        {
            node.child_by_field_name("name")
                .and_then(|name| written_name(name, source))
        }
        "command" => {
            collect_let_writes(node, source, typed);
            None
        }
        _ => None,
    };
    typed.extend(written);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_arith_writes(child, source, typed);
    }
}

/// The names a `let` command writes. tree-sitter parses its arguments as
/// words and strings rather than arithmetic, so they are scanned as text.
fn collect_let_writes(node: Node, source: &[u8], typed: &mut HashSet<String>) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    if node_text(name_node, source) != "let" {
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_named() && child != name_node {
            let text = node_text(child, source).trim_matches(['"', '\'']);
            typed.extend(let_written_names(text));
        }
    }
}

/// The identifiers an arithmetic expression assigns or steps: those followed
/// by an assignment operator or `++`/`--`, or preceded by `++`/`--`. A false
/// positive only over-quotes; see [`collect_int_typed`].
fn let_written_names(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut names = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if !ident(b[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < b.len() && ident(b[i]) {
            i += 1;
        }
        // A number (`10`, `0x1f`), or a read through `$x` / `${x}`.
        if b[start].is_ascii_digit() || (start > 0 && matches!(b[start - 1], b'$' | b'{')) {
            continue;
        }
        let mut j = i;
        while b.get(j).is_some_and(|c| c.is_ascii_whitespace()) {
            j += 1;
        }
        let after = &b[j..];
        let assigns = [
            "<<=", ">>=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "++", "--",
        ]
        .iter()
        .any(|op| after.starts_with(op.as_bytes()))
            || (after.starts_with(b"=") && !after.starts_with(b"=="));
        let before = text[..start].trim_end();
        if assigns || before.ends_with("++") || before.ends_with("--") {
            names.push(text[start..i].to_string());
        }
    }
    names
}

/// One round of the copy fixpoint: a name assigned an int-valued expansion
/// holds an int. Returns whether the set grew.
fn collect_int_copies(node: Node, source: &[u8], typed: &mut HashSet<String>) -> bool {
    let mut grew = false;
    if node.kind() == "variable_assignment" {
        let copies = node
            .child_by_field_name("value")
            .is_some_and(|value| is_int_expansion(value, source, typed));
        let name = node
            .child_by_field_name("name")
            .and_then(|name| written_name(name, source));
        if let (true, Some(name)) = (copies, name) {
            grew |= typed.insert(name);
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        grew |= collect_int_copies(child, source, typed);
    }
    grew
}

/// The name a scalar write target names. A cell write `a[i]` is not one: an
/// int collection is already in the set by its `-i` declaration, and any
/// other collection cannot take the int (SB4030), so no compilable program
/// needs it.
fn written_name(node: Node, source: &[u8]) -> Option<String> {
    if !matches!(node.kind(), "variable_name" | "word") {
        return None;
    }
    let text = node_text(node, source);
    is_clean_ident(text).then(|| text.to_string())
}

/// Whether `node` is an expansion the converted script evaluates as a sash
/// `int` (DD#30): `$(( … ))`; `$?`, `$$`, `$!` and `$#`, which convert to
/// `$(status)`, `$(getpid)`, `$(getcpid)` and an arithmetic count; a length
/// `${#…}`; or a read of a name in `int_typed`.
fn is_int_expansion(node: Node, source: &[u8], int_typed: &HashSet<String>) -> bool {
    match node.kind() {
        "arithmetic_expansion" => true,
        "simple_expansion" => node.named_child(0).is_some_and(|name| {
            let text = node_text(name, source);
            match name.kind() {
                "special_variable_name" => matches!(text, "?" | "$" | "!" | "#"),
                "variable_name" => int_typed.contains(text),
                _ => false,
            }
        }),
        "expansion" => {
            let text = node_text(node, source);
            let inner = text
                .strip_prefix("${")
                .and_then(|t| t.strip_suffix('}'))
                .unwrap_or("");
            if let Some(rest) = inner.strip_prefix('#') {
                return rest.is_empty() || length_operand(rest).is_some();
            }
            let end = inner
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(inner.len());
            matches!(inner, "?" | "$" | "!") || (end > 0 && int_typed.contains(&inner[..end]))
        }
        _ => false,
    }
}

/// Whether `node` is a whole operand of a `[ ]` / `[[ ]]` test that compares
/// strings: not part of a word or string, and not an operand of the
/// `-eq` family, which the converter rewrites to `(( ))`.
fn is_string_test_operand(node: Node, source: &[u8]) -> bool {
    let mut cur = node.parent();
    while let Some(n) = cur {
        match n.kind() {
            "test_command" => return true,
            "parenthesized_expression" | "unary_expression" => {}
            "binary_expression" => {
                let numeric = n.child_by_field_name("operator").is_some_and(|op| {
                    matches!(
                        node_text(op, source),
                        "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge"
                    )
                });
                if numeric {
                    return false;
                }
            }
            _ => return false,
        }
        cur = n.parent();
    }
    false
}

/// Whether an `expansion` is exactly `${name}` — one child, no operator, no
/// subscript, nothing before the name. `${#s}`, `${s:-x}` and `${a[i]}` are
/// all excluded: each does something the bare read does not.
fn is_bare_braced_read(node: Node, source: &[u8]) -> bool {
    if node.named_child_count() != 1 {
        return false;
    }
    let text = node_text(node, source);
    let Some(inner) = text.strip_prefix("${").and_then(|t| t.strip_suffix('}')) else {
        return false;
    };
    is_clean_ident(inner)
}

/// The disposition to emit for a `variable_name` read in arithmetic, or `None`
/// when the read needs none. `node` is the `variable_name`; `outer` is the node
/// the patch replaces (the `simple_expansion` for `$i`, else the name itself).
fn arith_disposition(
    node: Node,
    outer: Node,
    source: &[u8],
    collections: &Names,
) -> Option<String> {
    let name = node_text(node, source);
    if !is_clean_ident(name) || collections.ints.contains(name) {
        return None;
    }
    // A known env var is rewritten to a `$(getenv …)` + attest pair by
    // `convert_simple_expansion`; two patches over one span would collide.
    if known_externals::lookup_env_var(name).is_some() {
        return None;
    }
    if !in_arithmetic(outer, source) || is_arith_assign_target(outer) {
        return None;
    }
    Some(abort_disposition(name))
}

/// Whether `node` is written rather than read: the left side of `=`/`+=`/…,
/// the operand of `++`/`--`, or a c-style-for initialiser target. Wrapping a
/// write in `${…:?}` would be both wrong and unparseable.
fn is_arith_assign_target(node: Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    match parent.kind() {
        // tree-sitter models `c = a + 1` as a binary_expression whose operator
        // happens to be an assignment, so the operator text is the only tell.
        "binary_expression" => {
            parent.child_by_field_name("left") == Some(node)
                && parent
                    .child(1)
                    .is_some_and(|op| is_assign_operator(op.kind()))
        }
        // `k++` / `--k`
        "postfix_expression" | "unary_expression" => true,
        "variable_assignment" => parent.child_by_field_name("name") == Some(node),
        _ => false,
    }
}

/// An arithmetic operator that writes its left operand. `==`, `!=`, `<=` and
/// `>=` end in `=` but do not write, so the set is listed rather than tested.
fn is_assign_operator(op: &str) -> bool {
    matches!(
        op,
        "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "<<=" | ">>=" | "&=" | "|=" | "^="
    )
}

/// Rewrite an arithmetic expression given as **text** rather than as CST
/// nodes. Only [`subscript_key`] needs this: bash parses `${a[i+1]}`'s index
/// as one opaque `word`, so there are no operand nodes to patch, and the
/// converter synthesises the `$(( … ))` wrapper from the raw text.
fn arith_text_dispositions(expr: &str, collections: &Names) -> String {
    let bytes = expr.as_bytes();
    let mut out = String::with_capacity(expr.len());
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if !(c.is_ascii_alphabetic() || c == '_') {
            out.push(c);
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && ((bytes[i] as char).is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
        let ident = &expr[start..i];
        // `$i` / `${i}` already carries its own spelling, and a `#` after an
        // identifier is a bash base prefix (`16#ff`), not a variable read.
        let sigil = start > 0 && matches!(bytes[start - 1], b'$' | b'{');
        let base = i < bytes.len() && bytes[i] == b'#';
        if sigil || base || collections.ints.contains(ident) {
            out.push_str(ident);
        } else {
            out.push_str(&abort_disposition(ident));
        }
    }
    out
}

/// Whether `node` sits inside an arithmetic context — `$(( … ))`, a bare
/// `(( … ))` statement, or a C-style `for (( … ))` header.
///
/// Only the `for`'s header: its body holds ordinary statements, so a
/// `[[ $? == 0 ]]` there needs `$(status)`, not the DD#58 sexpr, and a
/// string read there needs no integer disposition. Nor is anything inside a
/// command substitution arithmetic, wherever the substitution sits — its
/// body is statements.
fn in_arithmetic(node: Node, source: &[u8]) -> bool {
    let mut prev = node;
    let mut cur = node.parent();
    while let Some(n) = cur {
        match n.kind() {
            "arithmetic_expansion" => return true,
            "c_style_for_statement" => return n.child_by_field_name("body") != Some(prev),
            "compound_statement" if n.child(0).is_some_and(|c| c.kind() == "((") => return true,
            "command_substitution" => return false,
            // `let "x = $?"` is arithmetic too, but tree-sitter sees a
            // plain command whose arguments happen to be strings.
            "command" => {
                return n
                    .child_by_field_name("name")
                    .is_some_and(|c| node_text(c, source) == "let");
            }
            _ => {}
        }
        prev = n;
        cur = n.parent();
    }
    false
}

/// DD#75 §5 / DD#81: in-function `$@` / `$*` / `$#` are the caller's
/// arguments, which sash exposes as the implicit `args` array.
///
/// | bash | sash |
/// |---|---|
/// | `$#` | `$(len args)`, or `(len args)` in arithmetic |
/// | `$*` / `"$*"` | `$(join args)` |
/// | `"$@"` as a for-in source or command argument | `@args` |
/// | `"$@"` to `echo` / `write` | `$(join args)` (SB5030 forbids a splat) |
/// | `"$@"` anywhere else | `$(join args)` |
///
/// `${#args}` is **not** a target: DD#75 §4 retired the length sugar and it is
/// SB4038. The splat has to be unquoted (DD#22) — sash reads `"@args"` as the
/// plain five-character literal, so a `$@` embedded in a larger string has no
/// mechanical answer and gets a MANUAL rather than a silent literal.
fn convert_function_args(
    node: Node,
    name: &str,
    start: usize,
    end: usize,
    source: &[u8],
    patches: &mut Vec<Patch>,
    prefix_lines: &mut Vec<(usize, String)>,
) {
    let join = |patches: &mut Vec<Patch>| {
        patches.push(Patch::replace_span(
            start,
            end,
            "$(join args)".to_string(),
            Some(node_text(node, source).to_string()),
        ));
    };
    match name {
        // DD#46: `$#` is genuinely function-local and *is* sash-native — it is
        // scalar, so its `$` sigil is consistent and nothing needs rewriting.
        // Only the braced spelling has to move: `${#}` is SB4034, since `#` in
        // `${…}` reads as the (absent) length operator.
        "#" => {
            if end - start > 2 {
                patches.push(Patch::replace_span(
                    start,
                    end,
                    "$#".to_string(),
                    Some(node_text(node, source).to_string()),
                ));
            }
        }
        "*" => join(patches),
        "@" if args_splat_context(node, source) => {
            let target = splat_replace_node(node, start);
            if target.id() == node.id() && node.parent().is_some_and(|p| p.kind() == "string") {
                prefix_lines.push((
                    node.start_position().row,
                    "# MANUAL: `$@` inside a larger string — bash word-splits it \
                     into several words there, and sash has no such form (a \
                     quoted `\"@args\"` is the plain literal). Rewrite as a loop \
                     over `@args`, or use `$(join args)` if one joined word was \
                     what you meant"
                        .into(),
                ));
                return;
            }
            let from = if target.id() == node.id() {
                start
            } else {
                target.start_byte()
            };
            // An *unquoted* bash `$@` re-splits each argument on IFS; `@args`
            // does not, so `f "a b"` reaching `g $@` passes two arguments in
            // bash and one here. sash catches the shape that matters (SB5042,
            // wrong arity) rather than diverging silently, but say so.
            if target.id() == node.id() {
                prefix_lines.push((
                    node.start_position().row,
                    "# CHECK: unquoted `$@` — bash re-splits each argument on \
                     IFS, `@args` passes them as they are. Equivalent unless an \
                     argument contains whitespace; `\"$@\"` was the exact form"
                        .into(),
                ));
            }
            patches.push(Patch::replace_span(
                from,
                target.end_byte(),
                "@args".to_string(),
                None,
            ));
        }
        "@" => join(patches),
        _ => {}
    }
}

/// Whether a `$@` here takes the splat (`@args`) rather than the joined form.
///
/// A for-in source and a command argument both take a list. `echo`/`write` are
/// the exception: they join their arguments into one stream, so a splat there
/// is SB5030 and the joined form is both legal and what bash prints anyway.
fn args_splat_context(node: Node, source: &[u8]) -> bool {
    if for_in_header_loop(node).is_some() {
        return true;
    }
    match argument_command_name(node, source).as_deref() {
        Some("echo") | Some("write") | None => false,
        Some(_) => true,
    }
}

/// The name of the command this node is an *argument* of, looking through the
/// quoting wrappers only. `None` when the node is the command name itself, or
/// is not in command-argument position at all.
///
/// Stricter than [`enclosing_command_name`], which keeps walking through any
/// ancestor: a `$@` on an assignment right-hand side has a `command` ancestor
/// in tree-sitter-bash, and treating that as an argument slot would emit a
/// splat where a joined string is meant.
fn argument_command_name(node: Node, source: &[u8]) -> Option<String> {
    let mut cur = node;
    while let Some(p) = cur.parent() {
        if p.kind() == "command" {
            let name_node = p.child_by_field_name("name")?;
            if name_node.id() == cur.id() {
                return None;
            }
            return Some(node_text(name_node, source).to_string());
        }
        if matches!(p.kind(), "string" | "concatenation" | "raw_string") {
            cur = p;
            continue;
        }
        return None;
    }
    None
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
                let name = node_text(child, source);
                // DD#75 §1: the keyword forms are canonical and the
                // punctuation is input sugar. These three are process-wide,
                // not positional, so they convert inside a function body too
                // — unlike $@/$*/$# below, which are the caller's args.
                if let Some(keyword) = canonical_special(name, in_arithmetic(node, source)) {
                    patches.push(Patch::replace_span(
                        start,
                        end,
                        keyword.to_string(),
                        Some(node_text(node, source).to_string()),
                    ));
                    continue;
                }
                if in_function {
                    convert_function_args(node, name, start, end, source, patches, prefix_lines);
                    continue;
                }
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
            if in_function {
                convert_function_args(
                    node,
                    inner,
                    node.start_byte(),
                    node.end_byte(),
                    source,
                    patches,
                    prefix_lines,
                );
            } else {
                convert_splat(node, node.start_byte(), source, patches, prefix_lines);
            }
            return;
        }
        if inner == "#" {
            if in_function {
                convert_function_args(
                    node,
                    inner,
                    node.start_byte(),
                    node.end_byte(),
                    source,
                    patches,
                    prefix_lines,
                );
            } else {
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
            if let Some(replacement) =
                length_expansion(rest, collections, in_arithmetic(node, source))
            {
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
    let rewritten = subscript_key(key, collections.maps.contains(name), collections)?;
    Some(format!("${{{}[{}]}}", name, rewritten))
}

/// The key half of [`subscript_expansion`]. `None` means "already direct,
/// leave it".
fn subscript_key(key: &str, is_map: bool, collections: &Names) -> Option<String> {
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
    Some(format!(
        "$(( {} ))",
        arith_text_dispositions(key, collections)
    ))
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
fn length_expansion(rest: &str, collections: &Names, arith: bool) -> Option<String> {
    let (base, subscript) = length_operand(rest)?;
    // In arithmetic the DD#58 sexpr is the only spelling that parses: a
    // command substitution is not an arithmetic operand (SB4020).
    let (open, close) = if arith { ("(", ")") } else { ("$(", ")") };
    Some(match subscript {
        Some("@") | Some("*") => format!("{}len {}{}", open, base, close),
        Some(index) => format!("{}strlen ${{{}[{}]}}{}", open, base, index, close),
        None if collections.all.contains(base) => {
            format!("{}strlen ${{{}[0]}}{}", open, base, close)
        }
        None => format!("{}strlen ${}{}", open, base, close),
    })
}

/// The base name and optional subscript of a `${#…}` length form, given the
/// text after the `#`. `None` is the same verdict [`length_expansion`]
/// gives, which [`is_int_expansion`] relies on.
fn length_operand(rest: &str) -> Option<(&str, Option<&str>)> {
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
    Some((base, subscript))
}

/// What the pre-pass learned about names, threaded through the rewrite walk.
pub(crate) struct Names {
    /// Every name used as a collection anywhere in the script.
    all: HashSet<String>,
    /// The subset declared `declare -A` / `local -A`, i.e. maps. A map
    /// subscript is a key, not an arithmetic index (DD#80).
    maps: HashSet<String>,
    /// Names declared `declare -i` / `local -i` / `typeset -i`, which convert
    /// to sash `int`. Arithmetic on these needs no disposition — see
    /// [`arith_disposition`].
    ints: HashSet<String>,
    /// Every name the converted script may hold as a sash `int` — a
    /// superset of `ints`. See [`collect_int_typed`].
    int_typed: HashSet<String>,
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

/// DD#31: *"Only postfix `x++` and `x--`. No prefix forms."* Bash has both,
/// so every prefix form the converter sees must go somewhere.
///
/// Rewrite where the rewrite is **sound**, annotate where it is not. The two
/// are distinguished by one question: is the expression's value used?
///
/// - Value discarded — a whole `(( ++i ))` statement, or the initializer or
///   update slot of a C-style `for` header. `++i` and `i++` differ only in
///   what they evaluate to, and nothing reads it, so `i++` is exact. DD#31
///   calls both "syntactic sugar for `x = x + 1`".
/// - Value used — `$(( ++i ))`, `(( y = ++i ))`, `for (( ; ++i < 3; ))`,
///   `if (( ++i ))`. Here `i++` would yield the value from *before* the
///   increment, so a rewrite would silently change the result. There is no
///   postfix spelling with the same meaning — and sash rejects mutation in a
///   read context outright — so the honest output is a `CHECK:` for a human.
fn convert_prefix_incdec(
    node: Node,
    source: &[u8],
    patches: &mut Vec<Patch>,
    prefix_lines: &mut Vec<(usize, String)>,
) {
    let Some(op) = node.child_by_field_name("operator") else {
        return;
    };
    let op_text = node_text(op, source);
    // `unary_expression` is also `-j` and `!j`; only ++/-- are in scope.
    if op_text != "++" && op_text != "--" {
        return;
    }
    let line = node.start_position().row;
    let Some(var) = prefix_incdec_operand(node, source) else {
        prefix_lines.push((
            line,
            format!(
                "# MANUAL: prefix `{}` on a non-scalar operand — sash has no prefix form (DD#31)",
                op_text
            ),
        ));
        return;
    };
    if prefix_incdec_value_discarded(node) {
        patches.push(Patch::replace_node(
            node,
            format!("{}{}", var, op_text),
            Some(node_text(node, source).to_string()),
        ));
    } else {
        let verb = if op_text == "++" {
            "increment"
        } else {
            "decrement"
        };
        prefix_lines.push((
            line,
            format!(
                "# CHECK: prefix `{}{}` yields the value AFTER the {}; sash has no prefix form (DD#31) \
                 and rejects mutation in a read context. Hoist it: `(( {}{} ))` on its own line, then use `{}`",
                op_text, var, verb, var, op_text, var
            ),
        ));
    }
}

/// The scalar a prefix `++`/`--` mutates.
///
/// The operand's CST kind depends on where the expression sits: inside a
/// `(( … ))` statement it is a `variable_name`, but in a C-style `for`
/// header tree-sitter-bash hands back a bare `word`. Accept either, and
/// only when the word is identifier-shaped — `++a[i]` yields a `subscript`
/// and has no scalar to name.
fn prefix_incdec_operand(node: Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "variable_name" => return Some(node_text(child, source).to_string()),
            "word" => {
                let t = node_text(child, source);
                let mut bytes = t.bytes();
                let ok = bytes
                    .next()
                    .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
                    && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_');
                return ok.then(|| t.to_string());
            }
            _ => {}
        }
    }
    None
}

/// Whether a prefix `++`/`--` sits where nothing reads its value.
fn prefix_incdec_value_discarded(node: Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    let field = field_name_of(node, parent);
    match parent.kind() {
        // `for (( ++i; …; ++i ))` — but NOT the condition, which is read.
        "c_style_for_statement" => matches!(field, Some("initializer") | Some("update")),
        // A bare `(( … ))` statement, including a comma element of one. The
        // same node under `if`/`while` is that statement's `condition` field,
        // where the value IS read.
        "compound_statement" => {
            parent.child(0).is_some_and(|c| c.kind() == "((")
                && parent
                    .parent()
                    .is_none_or(|gp| field_name_of(parent, gp) != Some("condition"))
        }
        _ => false,
    }
}

/// The field name `child` occupies within `parent`, if any.
fn field_name_of<'a>(child: Node, parent: Node) -> Option<&'a str> {
    (0..parent.child_count())
        .find(|i| parent.child(*i) == Some(child))
        .and_then(|i| parent.field_name_for_child(i as u32))
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

// ── DD#69 addendum / DD#23: `true` and `false` are externalized ───────────
//
// They resolve to `/usr/bin/true` / `/usr/bin/false`, not to inline
// non-forking builtins, and that changes two things a bash author will not
// expect. Which one matters depends entirely on whether the status is
// consumed, so the hint says one thing or the other rather than both:
//
//   - status DISCARDED — a bare `false` **aborts the program** (DD#55),
//     where bash merely sets `$?`. This is a semantic change and the more
//     serious of the two.
//   - status CONSUMED (a condition, a `&&`/`||` list, a negation) — the
//     meaning is bash's, but each evaluation now forks and execs. Measured
//     at ~0.55 ms per iteration, so a `while true` busy-loop that was free
//     in bash is not free here.
fn add_true_false_hint(
    node: Node,
    name: &str,
    line: usize,
    prefix_lines: &mut Vec<(usize, String)>,
) {
    // Only `false` can abort, and only where nothing reads its status. A bare
    // `true` is still a no-op — it just costs a fork now — so it takes the
    // second message, not a warning about a program exit that cannot happen.
    let msg = if name == "false" && !status_is_consumed(node) {
        "# CHECK: sash runs `false` as /usr/bin/false (externalized, DD#23) and \
         nothing reads its status here — so this ABORTS the program (DD#55), \
         where bash only sets `$?`. For an internal always-fail use `! :` or a \
         function that returns 1"
            .to_string()
    } else {
        format!(
            "# CHECK: sash runs `{name}` as /usr/bin/{name} (externalized, \
             DD#23), so the status matches bash but each evaluation forks and \
             execs — a `while true` loop is no longer free"
        )
    };
    prefix_lines.push((line, msg));
}

/// Does something read this command's exit status?
///
/// Walks out through the wrappers that consume a status. Anything else means
/// the status is discarded, which for `false` is the abort case.
fn status_is_consumed(node: Node) -> bool {
    let mut child = node;
    let mut current = node.parent();
    while let Some(p) = current {
        match p.kind() {
            // `&&` / `||`, `! cmd`, `$(…)`, and `if !`-style negation all read it.
            "list" | "negated_command" | "command_substitution" | "process_substitution" => {
                return true;
            }
            // Only the CONDITION of a loop/branch consumes it; a command in the
            // body is in discarded-status position like any other statement.
            "if_statement" | "while_statement" | "until_statement" | "elif_clause" => {
                return p
                    .child_by_field_name("condition")
                    .is_some_and(|c| c.id() == child.id() || contains_node(c, node));
            }
            // A pipeline's status is its last stage; earlier stages are dropped,
            // but sash does not abort on those, so treat the whole thing as read.
            "pipeline" => return true,
            // A subshell/compound is transparent — keep walking out.
            "subshell" | "compound_statement" | "program" | "function_definition" => {}
            _ => {}
        }
        child = p;
        current = p.parent();
    }
    false
}

fn contains_node(haystack: Node, needle: Node) -> bool {
    haystack.start_byte() <= needle.start_byte() && needle.end_byte() <= haystack.end_byte()
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
    // Backstop: a patch inside an earlier patch's span cannot be expressed
    // in the output. Emitting it anyway re-emits source the outer patch
    // already consumed — which is silent corruption, not an error, and is
    // how `if [ "$1" = x ]` once converted to `if [[ $1 = x ]]$arg1" = x ]`.
    // Every handler that replaces a whole node now splices its children's
    // patches in itself; this keeps a future one from corrupting output.
    {
        let mut reach = 0usize;
        sorted.retain(|p| {
            if p.start < reach {
                return false;
            }
            reach = reach.max(p.end);
            true
        });
    }

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
    // An annotation waits for the zero-width insertions at its patch's end:
    // those close the converted text (the quotes around a DD#30 test
    // operand), and a `# was:` written inside the quotes would become part
    // of the operand rather than a comment.
    let mut pending: Option<&str> = None;

    for patch in &sorted {
        if !(patch.start == patch.end && patch.start == pos) {
            push_annotation(&mut result, pending.take());
        }
        if patch.start > pos {
            result.push_str(std::str::from_utf8(&bytes[pos..patch.start]).unwrap_or(""));
        }
        result.push_str(&patch.replacement);
        if annotate {
            if let Some(ref orig) = patch.original {
                pending = Some(orig);
            }
        }
        pos = patch.end;
    }
    push_annotation(&mut result, pending.take());
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

/// Write a pending `--annotate` comment for a converted span.
fn push_annotation(out: &mut String, original: Option<&str>) {
    if let Some(orig) = original {
        out.push_str("  # was: ");
        out.push_str(orig);
    }
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
