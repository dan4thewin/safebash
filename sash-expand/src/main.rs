//! sash-expand — safe KVP config file loader (DD#1)
//!
//! Reads a KEY=VALUE config file, validates each line, expands safe
//! variable operations against the accumulated environment, and outputs
//! a trivial `KEY=value` format on stdout.
//!
//! Allowed value forms:
//!   $VAR / ${VAR}           — simple expansion
//!   ${VAR:-default}         — default if unset/empty
//!   ${VAR:+alternate}       — alternate if set
//!   ${VAR%%pat}             — remove longest suffix
//!   ${VAR%pat}              — remove shortest suffix
//!   ${VAR##pat}             — remove longest prefix
//!   ${VAR#pat}              — remove shortest prefix
//!
//! Dies on: $(cmd), backticks, ${!indirect}, arithmetic, any other form.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: sash-expand <file>");
        process::exit(1);
    }

    let path = &args[1];
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("sash-expand: {}: {}", path, e);
            process::exit(1);
        }
    };

    // Start with inherited environment
    let mut vars: HashMap<String, String> = env::vars().collect();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for (lineno, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        // Skip blanks and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Must be KEY=VALUE
        let eq_pos = match trimmed.find('=') {
            Some(p) => p,
            None => {
                eprintln!("sash-expand: {}:{}: not a KEY=VALUE line: {}", path, lineno + 1, trimmed);
                process::exit(1);
            }
        };

        let key = &trimmed[..eq_pos];
        let raw_val = &trimmed[eq_pos + 1..];

        // Validate key: must be [A-Za-z_][A-Za-z0-9_]*
        if key.is_empty() || !key.chars().next().unwrap().is_ascii_alphabetic() && key.as_bytes()[0] != b'_' {
            eprintln!("sash-expand: {}:{}: invalid key: {}", path, lineno + 1, key);
            process::exit(1);
        }
        if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            eprintln!("sash-expand: {}:{}: invalid key: {}", path, lineno + 1, key);
            process::exit(1);
        }

        // Strip optional quotes around value
        let val_str = if (raw_val.starts_with('"') && raw_val.ends_with('"'))
            || (raw_val.starts_with('\'') && raw_val.ends_with('\''))
        {
            &raw_val[1..raw_val.len() - 1]
        } else {
            raw_val
        };

        // Expand variables in value
        let expanded = match expand_value(val_str, &vars, path, lineno + 1) {
            Ok(v) => v,
            Err(msg) => {
                eprintln!("sash-expand: {}:{}: {}", path, lineno + 1, msg);
                process::exit(1);
            }
        };

        vars.insert(key.to_string(), expanded.clone());
        writeln!(out, "{}={}", key, expanded).unwrap();
    }
}

/// Expand safe variable operations in a value string.
fn expand_value(
    s: &str,
    vars: &HashMap<String, String>,
    _path: &str,
    _lineno: usize,
) -> Result<String, String> {
    let bytes = s.as_bytes();
    let mut result = String::new();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'$' {
            if i + 1 >= bytes.len() {
                result.push('$');
                i += 1;
                continue;
            }

            if bytes[i + 1] == b'(' {
                return Err(format!("command substitution not allowed: {}", s));
            }
            if bytes[i + 1] == b'`' || (i + 1 < bytes.len() && bytes[i + 1] == b'`') {
                return Err(format!("backtick substitution not allowed: {}", s));
            }

            if bytes[i + 1] == b'{' {
                // Complex expansion ${...}
                let start = i + 2;
                let end = match find_matching_brace(bytes, start) {
                    Some(e) => e,
                    None => return Err(format!("unterminated ${{...}}: {}", s)),
                };
                let inner = &s[start..end];
                let expanded = expand_braced(inner, vars)?;
                result.push_str(&expanded);
                i = end + 1;
            } else {
                // Simple $VAR
                let start = i + 1;
                let mut end = start;
                while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                    end += 1;
                }
                let name = &s[start..end];
                result.push_str(vars.get(name).map(|s| s.as_str()).unwrap_or(""));
                i = end;
            }
        } else if bytes[i] == b'`' {
            return Err(format!("backtick substitution not allowed: {}", s));
        } else if bytes[i] == b'\\' && i + 1 < bytes.len() {
            // Simple escape
            i += 1;
            result.push(bytes[i] as char);
            i += 1;
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }

    Ok(result)
}

/// Expand a braced expression (contents between ${ and }).
fn expand_braced(inner: &str, vars: &HashMap<String, String>) -> Result<String, String> {
    // Reject dangerous forms
    if inner.starts_with('!') {
        return Err(format!("indirect expansion not allowed: ${{{}}}", inner));
    }
    if inner.contains("$(") || inner.contains('`') {
        return Err(format!("command substitution not allowed in expansion: ${{{}}}", inner));
    }

    // ${VAR:-default}
    if let Some(pos) = inner.find(":-") {
        let name = &inner[..pos];
        let default = &inner[pos + 2..];
        let val = vars.get(name).filter(|v| !v.is_empty());
        return Ok(val.map(|s| s.clone()).unwrap_or_else(|| default.to_string()));
    }

    // ${VAR:+alternate}
    if let Some(pos) = inner.find(":+") {
        let name = &inner[..pos];
        let alt = &inner[pos + 2..];
        let val = vars.get(name).filter(|v| !v.is_empty());
        return Ok(if val.is_some() { alt.to_string() } else { String::new() });
    }

    // ${VAR%%pat} — longest suffix
    if let Some(pos) = inner.find("%%") {
        let name = &inner[..pos];
        let pat = &inner[pos + 2..];
        let val = vars.get(name).map(|s| s.as_str()).unwrap_or("");
        return Ok(strip_longest_suffix(val, pat));
    }

    // ${VAR%pat} — shortest suffix
    if let Some(pos) = inner.find('%') {
        let name = &inner[..pos];
        let pat = &inner[pos + 1..];
        let val = vars.get(name).map(|s| s.as_str()).unwrap_or("");
        return Ok(strip_shortest_suffix(val, pat));
    }

    // ${VAR##pat} — longest prefix
    if let Some(pos) = inner.find("##") {
        let name = &inner[..pos];
        let pat = &inner[pos + 2..];
        let val = vars.get(name).map(|s| s.as_str()).unwrap_or("");
        return Ok(strip_longest_prefix(val, pat));
    }

    // ${VAR#pat} — shortest prefix
    if let Some(pos) = inner.find('#') {
        let name = &inner[..pos];
        let pat = &inner[pos + 1..];
        let val = vars.get(name).map(|s| s.as_str()).unwrap_or("");
        return Ok(strip_shortest_prefix(val, pat));
    }

    // Simple ${VAR}
    if inner.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Ok(vars.get(inner).cloned().unwrap_or_default());
    }

    Err(format!("unsupported expansion: ${{{}}}", inner))
}

fn find_matching_brace(bytes: &[u8], start: usize) -> Option<usize> {
    let mut depth = 1;
    let mut i = start;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            depth += 1;
        } else if bytes[i] == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Star-glob pattern matching for suffix/prefix stripping.
/// Pattern: literal chars, * matches any sequence.
fn glob_match(pattern: &str, text: &str) -> bool {
    let pat = pattern.as_bytes();
    let txt = text.as_bytes();
    let mut pi = 0;
    let mut ti = 0;
    let mut star_pi = usize::MAX;
    let mut star_ti = 0;

    while ti < txt.len() {
        if pi < pat.len() && (pat[pi] == txt[ti] || pat[pi] == b'?') {
            pi += 1;
            ti += 1;
        } else if pi < pat.len() && pat[pi] == b'*' {
            star_pi = pi;
            star_ti = ti;
            pi += 1;
        } else if star_pi != usize::MAX {
            pi = star_pi + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }
    while pi < pat.len() && pat[pi] == b'*' {
        pi += 1;
    }
    pi == pat.len()
}

fn strip_shortest_suffix(val: &str, pat: &str) -> String {
    for i in (0..=val.len()).rev() {
        if glob_match(pat, &val[i..]) {
            return val[..i].to_string();
        }
    }
    val.to_string()
}

fn strip_longest_suffix(val: &str, pat: &str) -> String {
    for i in 0..=val.len() {
        if glob_match(pat, &val[i..]) {
            return val[..i].to_string();
        }
    }
    val.to_string()
}

fn strip_shortest_prefix(val: &str, pat: &str) -> String {
    for i in 0..=val.len() {
        if glob_match(pat, &val[..i]) {
            return val[i..].to_string();
        }
    }
    val.to_string()
}

fn strip_longest_prefix(val: &str, pat: &str) -> String {
    for i in (0..=val.len()).rev() {
        if glob_match(pat, &val[..i]) {
            return val[i..].to_string();
        }
    }
    val.to_string()
}
