/// Source emitter — produces sash-native source from bash input.
///
/// Collects coercion patches during CST→IR lowering, then applies them
/// to the original source to produce sash-native output. Preserves
/// comments, whitespace, and formatting for uncoerced regions.
///
/// DD#45: coerce in lowerer, emit from CST.
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

/// Record a coercion during lowering.
pub fn record_coercion(start: usize, end: usize, replacement: String, original: String) {
    COERCIONS.with(|c| {
        c.borrow_mut().push(Coercion {
            start,
            end,
            replacement,
            original,
        });
    });
}

/// Clear all recorded coercions (call before lowering).
pub fn clear_coercions() {
    COERCIONS.with(|c| c.borrow_mut().clear());
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
