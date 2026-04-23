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
    static COERCIONS: RefCell<Vec<Coercion>> = RefCell::new(Vec::new());
}

/// Record a coercion during lowering.
pub fn record_coercion(start: usize, end: usize, replacement: String, original: String) {
    COERCIONS.with(|c| {
        c.borrow_mut().push(Coercion { start, end, replacement, original });
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
                eprintln!("sash: warning: overlapping coercions at {}..{} and {}..{}",
                    w[0].start, w[0].end, w[1].start, w[1].end);
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

/// Inject auto-bless lines after assignments of known externals.
/// Each bless line is inserted after the source line where the assignment occurs.
pub fn inject_auto_bless(source: &str, auto_blessed: &[crate::taint::AutoBless], annotate: bool) -> String {
    if auto_blessed.is_empty() {
        return source.to_string();
    }

    // Build a map: line_number → Vec<AutoBless>
    let mut by_line: std::collections::BTreeMap<usize, Vec<&crate::taint::AutoBless>> =
        std::collections::BTreeMap::new();
    for ab in auto_blessed {
        by_line.entry(ab.line).or_default().push(ab);
    }

    let mut out = String::with_capacity(source.len() + auto_blessed.len() * 80);
    for (line_num, line) in source.lines().enumerate() {
        out.push_str(line);
        out.push('\n');
        if let Some(bless_items) = by_line.get(&(line_num + 1)) {
            for ab in bless_items {
                out.push_str(&format!("bless {} [[ ${} =~ {} ]]", ab.var_name, ab.var_name, ab.pattern));
                if annotate {
                    out.push_str(&format!("  # auto: {}", ab.description));
                }
                out.push('\n');
            }
        }
    }
    // Handle trailing content without final newline
    if !source.ends_with('\n') {
        // Remove the extra newline we added on the last line
        out.pop();
    }

    out
}
