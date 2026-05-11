pub mod ir;
pub mod lower;
pub mod printer;
pub mod ssa;
pub mod ssa_taint;
pub mod transform;
pub mod ssa_printer;
pub mod emit_janet;
pub mod oracle;
pub mod type_check;
pub mod emit_source;
pub mod codegen;
pub mod diagnostic;
pub mod validator;
pub mod rule;
pub mod rules;

pub fn parse(source: &str) -> tree_sitter::Tree {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_bash::LANGUAGE.into())
        .expect("Error loading Bash grammar");
    parser.parse(source, None).expect("Failed to parse")
}

/// DD#58: Preprocess source to protect sexpr syntax inside arithmetic.
/// Replaces `$(( content ))` with `$(( __ARITH_N__ ))` when content
/// contains sexpr patterns that tree-sitter can't parse.
/// Returns (preprocessed_source, map of placeholder → original content).
pub fn preprocess_arithmetic(source: &str) -> (String, std::collections::HashMap<String, String>) {
    use std::collections::HashMap;
    let mut result = String::with_capacity(source.len());
    let mut map = HashMap::new();
    let mut counter = 0usize;
    let bytes = source.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Look for $(( or ((
        let (is_dollar, start) = if i + 3 <= bytes.len() && &bytes[i..i+3] == b"$((" {
            (true, i + 3)
        } else if i + 2 <= bytes.len() && &bytes[i..i+2] == b"(("
            // Don't match (( inside words — must be at start of line or after whitespace/;
            && (i == 0 || matches!(bytes[i-1], b' ' | b'\t' | b'\n' | b';' | b'(' | b'!')) {
            (false, i + 2)
        } else {
            // Copy one UTF-8 character (may be multi-byte)
            let ch_len = utf8_char_len(bytes[i]);
            result.push_str(&source[i..i+ch_len]);
            i += ch_len;
            continue;
        };

        // Find matching )) respecting nesting
        let mut depth = 1i32;
        let mut j = start;
        while j + 1 < bytes.len() && depth > 0 {
            if j + 2 < bytes.len() && &bytes[j..j+3] == b"$((" {
                depth += 1; j += 3; continue;
            }
            if &bytes[j..j+2] == b"((" {
                depth += 1; j += 2; continue;
            }
            if &bytes[j..j+2] == b"))" {
                depth -= 1;
                if depth == 0 { break; }
                j += 2; continue;
            }
            j += 1;
        }
        if depth != 0 {
            // Unmatched — emit as-is
            if is_dollar { result.push_str("$(("); } else { result.push_str("(("); }
            i = start;
            continue;
        }

        let content = &source[start..j];
        let end = j + 2; // skip ))

        // Check if content has sexpr patterns: ( followed by identifier then space
        // Simple heuristic: contains a paren followed by alpha then space then non-)
        let needs_placeholder = has_sexpr_pattern(content);

        if needs_placeholder {
            let key = format!("__ARITH_{}__", counter);
            counter += 1;
            map.insert(key.clone(), content.trim().to_string());
            if is_dollar {
                result.push_str(&format!("$(({}))", key));
            } else {
                result.push_str(&format!("(({}))", key));
            }
        } else {
            // No sexpr — emit unchanged
            if is_dollar { result.push_str("$(("); } else { result.push_str("(("); }
            result.push_str(content);
            result.push_str("))");
        }
        i = end;
    }
    (result, map)
}

fn utf8_char_len(b: u8) -> usize {
    if b < 0x80 { 1 }
    else if b < 0xE0 { 2 }
    else if b < 0xF0 { 3 }
    else { 4 }
}

fn has_sexpr_pattern(content: &str) -> bool {
    let bytes = content.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'(' {
            // Skip whitespace after (
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() { j += 1; }
            // Check if next token is an identifier (not a digit or operator)
            if j < bytes.len() && (bytes[j].is_ascii_alphabetic() || bytes[j] == b'_') {
                // Read the identifier
                let id_start = j;
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') { j += 1; }
                // After identifier, must have whitespace or ) — NOT an operator
                if j < bytes.len() && (bytes[j].is_ascii_whitespace() || bytes[j] == b')') {
                    // Check it's not just (var) which is valid grouping
                    while j < bytes.len() && bytes[j].is_ascii_whitespace() { j += 1; }
                    if j < bytes.len() && bytes[j] != b')' {
                        return true;
                    }
                }
            }
        }
        i += 1;
    }
    false
}

use std::sync::Mutex;
use std::collections::HashMap;

static ARITH_MAP: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// Store the arithmetic placeholder map for the current compilation.
pub fn set_arith_map(map: HashMap<String, String>) {
    *ARITH_MAP.lock().unwrap() = Some(map);
}

/// Look up an arithmetic placeholder.
pub fn lookup_arith(key: &str) -> Option<String> {
    ARITH_MAP.lock().unwrap().as_ref()?.get(key).cloned()
}
pub mod taint;
