/// Source emitter — renders the text embedded in the `.sash_source` ELF
/// section (DD#45; retrievable via `--extract`). Applies recorded
/// `Coercion` patches to the original source bytes (replace
/// `source[start..end]` with `replacement`), preserving comments,
/// whitespace, and formatting for unpatched regions.
///
/// NOTE: the patch *recorder* lived in the old CST→IR lowerer, which was
/// removed with tree-sitter (commit 73caecf). Nothing currently pushes
/// into `COERCIONS`, so the list is empty and the source is embedded
/// verbatim. The hand-written parser instead *rejects* non-conforming
/// bash with diagnostics; structural bash→sash-native rewriting lives in
/// the separate `sash-convert` crate. This passthrough is the dormant
/// remainder of that mechanism (kept for the `.sash_source` section).
use std::cell::RefCell;

/// A single source-level coercion: replace source[start..end] with `replacement`.
#[derive(Debug, Clone)]
pub struct Coercion {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
    pub original: String,
}

thread_local! {
    static COERCIONS: RefCell<Vec<Coercion>> = const { RefCell::new(Vec::new()) };
}

/// Apply recorded coercions to original source, producing sash-native source.
/// Annotates each coercion with `# was: <original>`.
pub fn emit_source(source: &[u8], annotate: bool) -> String {
    COERCIONS.with(|c| {
        let mut coercions = c.borrow().clone();
        coercions.sort_by_key(|c| c.start);

        // Check for overlaps (shouldn't happen, but be safe)
        for w in coercions.windows(2) {
            if w[0].end > w[1].start {
                eprintln!(
                    "sash: warning: overlapping coercions at {}..{} and {}..{}",
                    w[0].start, w[0].end, w[1].start, w[1].end
                );
            }
        }

        let mut out = String::with_capacity(source.len() + coercions.len() * 32);
        let mut pos = 0;

        for coercion in &coercions {
            // Copy verbatim up to coercion start
            if coercion.start > pos {
                out.push_str(std::str::from_utf8(&source[pos..coercion.start]).unwrap_or(""));
            }
            // Emit replacement
            out.push_str(&coercion.replacement);
            if annotate {
                out.push_str("  # was: ");
                out.push_str(&coercion.original);
            }
            pos = coercion.end;
        }

        // Copy remaining source
        if pos < source.len() {
            out.push_str(std::str::from_utf8(&source[pos..]).unwrap_or(""));
        }

        out
    })
}
