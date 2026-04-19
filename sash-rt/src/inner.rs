// Safe inner functions extracted from lib.rs for testability.
// These are pure Rust with no unsafe, no no_std dependencies.
// Both lib.rs and test files include/reference these.

/// Parse decimal bytes to i64. Returns 0 for empty/non-numeric input.
/// Panics on overflow (DD#26: integer overflow traps).
pub fn atoi_inner(bytes: &[u8]) -> i64 {
    let mut result: i64 = 0;
    let mut negative = false;
    let mut i = 0;
    while i < bytes.len() && bytes[i] == b' ' { i += 1; }
    if i < bytes.len() && bytes[i] == b'-' { negative = true; i += 1; }
    else if i < bytes.len() && bytes[i] == b'+' { i += 1; }
    while i < bytes.len() && bytes[i] >= b'0' && bytes[i] <= b'9' {
        result = result.checked_mul(10)
            .and_then(|r| r.checked_add((bytes[i] - b'0') as i64))
            .expect("sash: integer overflow in atoi");
        i += 1;
    }
    if negative {
        result.checked_neg().expect("sash: integer overflow in atoi (negation)")
    } else {
        result
    }
}

/// Format i64 as decimal into a provided buffer. Returns the slice used.
pub fn itoa_inner(val: i64, buf: &mut [u8; 21]) -> &[u8] {
    let mut n = val;
    let negative = n < 0;
    if negative { n = n.checked_neg().expect("sash: integer overflow in itoa"); }
    let mut i = buf.len();
    if n == 0 {
        i -= 1;
        buf[i] = b'0';
    } else {
        while n > 0 {
            i -= 1;
            buf[i] = b'0' + (n % 10) as u8;
            n /= 10;
        }
    }
    if negative {
        i -= 1;
        buf[i] = b'-';
    }
    &buf[i..]
}

/// ${var%%pat} / ${var%pat} — returns new length (ptr unchanged).
pub fn strip_suffix_inner(s: &[u8], pat: &[u8], longest: bool) -> usize {
    let s_len = s.len();
    if s_len == 0 { return 0; }

    if pat.ends_with(b"*") && pat.len() > 1 {
        let prefix = &pat[..pat.len() - 1];
        if longest {
            if let Some(pos) = s.windows(prefix.len()).position(|w| w == prefix) {
                return pos;
            }
        } else {
            if let Some(pos) = s.windows(prefix.len()).rposition(|w| w == prefix) {
                return pos;
            }
        }
    } else if pat.starts_with(b"*") && pat.len() > 1 {
        let suffix = &pat[1..];
        if longest {
            if let Some(pos) = s.windows(suffix.len()).position(|w| w == suffix) {
                return pos;
            }
        } else {
            if let Some(pos) = s.windows(suffix.len()).rposition(|w| w == suffix) {
                return pos;
            }
        }
    } else if !pat.is_empty() && s.len() >= pat.len() && &s[s.len() - pat.len()..] == pat {
        return s_len - pat.len();
    }
    s_len
}

/// ${var##pat} / ${var#pat} — returns offset (bytes to skip from start).
pub fn strip_prefix_inner(s: &[u8], pat: &[u8], longest: bool) -> usize {
    if s.is_empty() { return 0; }

    if pat.starts_with(b"*") && pat.len() > 1 {
        let suffix = &pat[1..];
        if longest {
            if let Some(pos) = s.windows(suffix.len()).rposition(|w| w == suffix) {
                return pos + suffix.len();
            }
        } else {
            if let Some(pos) = s.windows(suffix.len()).position(|w| w == suffix) {
                return pos + suffix.len();
            }
        }
    } else if pat.ends_with(b"*") && pat.len() > 1 {
        let prefix = &pat[..pat.len() - 1];
        if longest {
            if let Some(pos) = s.windows(prefix.len()).rposition(|w| w == prefix) {
                return pos + prefix.len();
            }
        } else {
            if let Some(pos) = s.windows(prefix.len()).position(|w| w == prefix) {
                return pos + prefix.len();
            }
        }
    } else if !pat.is_empty() && s.len() >= pat.len() && &s[..pat.len()] == pat {
        return pat.len();
    }
    0
}

/// Compare two byte slices for equality.
pub fn streq_inner(a: &[u8], b: &[u8]) -> bool {
    a == b
}

/// Evaluate arithmetic. Panics on overflow (DD#26).
pub fn arith_eval_inner(var_val: i64, op: u8, operand: i64) -> i64 {
    match op {
        b'+' => var_val.checked_add(operand).expect("sash: integer overflow in arithmetic (+)"),
        b'-' => var_val.checked_sub(operand).expect("sash: integer overflow in arithmetic (-)"),
        b'*' => var_val.checked_mul(operand).expect("sash: integer overflow in arithmetic (*)"),
        b'/' => if operand != 0 { var_val.checked_div(operand).expect("sash: integer overflow in arithmetic (/)") } else { panic!("sash: division by zero") },
        b'%' => if operand != 0 { var_val.checked_rem(operand).expect("sash: integer overflow in arithmetic (%)") } else { panic!("sash: division by zero") },
        _ => 0,
    }
}

/// FNV-1a hash.
pub fn table_hash_inner(key: &[u8]) -> u32 {
    let mut h: u32 = 2166136261;
    for &b in key {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}
