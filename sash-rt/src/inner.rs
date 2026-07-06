// Safe inner functions extracted from lib.rs for testability.
// These are pure Rust with no unsafe, no no_std dependencies.
// Both lib.rs and test files include/reference these.
//
// Note: sash-rt-tests' overflow_tests.rs include!()s this file (textual
// inclusion at expression position), so an `#![allow(dead_code)]` inner
// attribute here would error 'inner attribute not permitted in this
// context'. Per-function `#[allow(dead_code)]` would silence the
// 'never used' false positives but each test crate calls a different
// subset, so the cleanup belongs at the include site, not here.

/// Parse a string to i64 for arithmetic coercion of runtime data (DD#72).
/// Honors `0x`/`0o`/`0b` prefixes; otherwise decimal with leading zeros
/// ignored (NO C-octal — `"010"` is 10, not 8). Returns 0 for empty/non-
/// numeric input. Panics on overflow (DD#26: integer overflow traps).
pub fn atoi_inner(bytes: &[u8]) -> i64 {
    let mut i = 0;
    while i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    }
    let mut negative = false;
    if i < bytes.len() && bytes[i] == b'-' {
        negative = true;
        i += 1;
    } else if i < bytes.len() && bytes[i] == b'+' {
        i += 1;
    }
    let magnitude = if let Some((radix, skip)) = radix_prefix(&bytes[i..]) {
        parse_radix_magnitude(&bytes[i + skip..], radix)
    } else {
        parse_decimal_magnitude(&bytes[i..])
    };
    if negative {
        magnitude
            .checked_neg()
            .expect("sash: integer overflow in atoi (negation)")
    } else {
        magnitude
    }
}

/// Detect a `0x`/`0o`/`0b` prefix (case-insensitive). Returns (radix, prefix
/// byte length) or None. A lone `0`/`0x`-with-no-digits is handled by the
/// caller's digit loop (yields 0).
fn radix_prefix(bytes: &[u8]) -> Option<(u32, usize)> {
    if bytes.len() >= 2 && bytes[0] == b'0' {
        match bytes[1] {
            b'x' | b'X' => return Some((16, 2)),
            b'o' | b'O' => return Some((8, 2)),
            b'b' | b'B' => return Some((2, 2)),
            _ => {}
        }
    }
    None
}

/// Parse leading decimal digits; stops at the first non-digit. Leading zeros
/// are ordinary decimal digits (no octal).
fn parse_decimal_magnitude(bytes: &[u8]) -> i64 {
    let mut result: i64 = 0;
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        result = result
            .checked_mul(10)
            .and_then(|r| r.checked_add((bytes[i] - b'0') as i64))
            .expect("sash: integer overflow in atoi");
        i += 1;
    }
    result
}

/// Parse leading digits in `radix` (2/8/16); stops at the first invalid digit.
fn parse_radix_magnitude(bytes: &[u8], radix: u32) -> i64 {
    let mut result: i64 = 0;
    let mut i = 0;
    while i < bytes.len() {
        let Some(d) = (bytes[i] as char).to_digit(radix) else {
            break;
        };
        result = result
            .checked_mul(radix as i64)
            .and_then(|r| r.checked_add(d as i64))
            .expect("sash: integer overflow in atoi");
        i += 1;
    }
    result
}

/// Format i64 as decimal into a provided buffer. Returns the slice used.
pub fn itoa_inner(val: i64, buf: &mut [u8; 21]) -> &[u8] {
    // Magnitude as u64 so i64::MIN (whose negation overflows i64) formats
    // correctly instead of panicking (bug-runtime-arith-overflow-wraps).
    let negative = val < 0;
    let mut n: u64 = val.unsigned_abs();
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
    if s_len == 0 {
        return 0;
    }

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
    if s.is_empty() {
        return 0;
    }

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
        b'+' => var_val
            .checked_add(operand)
            .expect("sash: integer overflow in arithmetic (+)"),
        b'-' => var_val
            .checked_sub(operand)
            .expect("sash: integer overflow in arithmetic (-)"),
        b'*' => var_val
            .checked_mul(operand)
            .expect("sash: integer overflow in arithmetic (*)"),
        b'/' => {
            if operand != 0 {
                var_val
                    .checked_div(operand)
                    .expect("sash: integer overflow in arithmetic (/)")
            } else {
                panic!("sash: division by zero")
            }
        }
        b'%' => {
            if operand != 0 {
                var_val
                    .checked_rem(operand)
                    .expect("sash: integer overflow in arithmetic (%)")
            } else {
                panic!("sash: division by zero")
            }
        }
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

// ═══════════════════════════════════════════════════════════════
// Fixed-point conversion (DD#36)
// ═══════════════════════════════════════════════════════════════

/// Convert decimal string to fixed-point integer.
/// dtoi(2, "19.99") → 1999
/// dtoi(1, "0.5") → 5
pub fn dtoi_inner(digits: i64, bytes: &[u8]) -> i64 {
    let digits = digits as usize;
    // Find decimal point
    let dot_pos = bytes.iter().position(|&b| b == b'.');

    let (int_part, frac_bytes) = match dot_pos {
        Some(pos) => (&bytes[..pos], &bytes[pos + 1..]),
        None => (bytes, &[] as &[u8]),
    };

    // Parse integer part
    let mut negative = false;
    let int_start = if !int_part.is_empty() && int_part[0] == b'-' {
        negative = true;
        1
    } else {
        0
    };

    let mut int_val: i64 = 0;
    for &b in &int_part[int_start..] {
        if b < b'0' || b > b'9' {
            panic!("sash: invalid decimal number in dtoi");
        }
        int_val = int_val
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as i64))
            .expect("sash: integer overflow in dtoi");
    }

    // Parse fractional part — must not exceed `digits` places
    if frac_bytes.len() > digits {
        panic!("sash: fractional part exceeds specified precision in dtoi");
    }

    let mut frac_val: i64 = 0;
    for &b in frac_bytes {
        if b < b'0' || b > b'9' {
            panic!("sash: invalid decimal number in dtoi");
        }
        frac_val = frac_val * 10 + (b - b'0') as i64;
    }
    // Right-pad with zeros: "0.5" with digits=3 → frac_val=500
    for _ in 0..(digits - frac_bytes.len()) {
        frac_val *= 10;
    }

    // Scale integer part and combine
    let mut scale: i64 = 1;
    for _ in 0..digits {
        scale = scale.checked_mul(10).expect("sash: scale overflow in dtoi");
    }

    let result = int_val
        .checked_mul(scale)
        .and_then(|v| v.checked_add(frac_val))
        .expect("sash: integer overflow in dtoi");

    if negative {
        result
            .checked_neg()
            .expect("sash: overflow in dtoi negation")
    } else {
        result
    }
}

/// Convert fixed-point integer to decimal string.
/// itod(2, 1999) → "19.99"
/// itod(2, 5) → "0.05"
pub fn itod_inner(digits: i64, value: i64, buf: &mut [u8]) -> usize {
    let digits = digits as usize;
    let negative = value < 0;
    let abs_val = if negative {
        value
            .checked_neg()
            .expect("sash: overflow in itod negation")
    } else {
        value
    };

    let mut scale: i64 = 1;
    for _ in 0..digits {
        scale *= 10;
    }

    let int_part = abs_val / scale;
    let frac_part = abs_val % scale;

    // Write integer part using itoa logic
    let mut pos = 0;
    if negative {
        buf[pos] = b'-';
        pos += 1;
    }

    // Convert integer part to string
    if int_part == 0 {
        buf[pos] = b'0';
        pos += 1;
    } else {
        let start = pos;
        let mut n = int_part;
        while n > 0 {
            buf[pos] = b'0' + (n % 10) as u8;
            n /= 10;
            pos += 1;
        }
        buf[start..pos].reverse();
    }

    // Decimal point
    buf[pos] = b'.';
    pos += 1;

    // Fractional part, zero-padded to `digits` places
    let frac_start = pos;
    let mut f = frac_part;
    for _ in 0..digits {
        buf[pos] = b'0';
        pos += 1;
    }
    let mut frac_pos = pos - 1;
    while f > 0 {
        buf[frac_pos] = b'0' + (f % 10) as u8;
        f /= 10;
        if frac_pos == frac_start {
            break;
        }
        frac_pos -= 1;
    }

    pos
}

/// Lexicographic string comparison. Returns <0, 0, or >0.
pub fn strcmp_inner(a: &[u8], b: &[u8]) -> i64 {
    let min_len = if a.len() < b.len() { a.len() } else { b.len() };
    for i in 0..min_len {
        if a[i] != b[i] {
            return (a[i] as i64) - (b[i] as i64);
        }
    }
    (a.len() as i64) - (b.len() as i64)
}
