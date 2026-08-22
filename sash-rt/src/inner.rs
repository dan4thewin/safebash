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
/// numeric input. Panics on overflow (DD#26: integer overflow traps) — the
/// runtime extern `sash_atoi` uses `atoi_checked` instead, routing overflow to
/// the clean `sash_arith_overflow` trap; this panicking form is kept for the
/// pure-function tests.
pub fn atoi_inner(bytes: &[u8]) -> i64 {
    atoi_checked(bytes).expect("sash: integer overflow in atoi")
}

/// Lenient string→int like `atoi_inner`, but returns None on i64 overflow
/// instead of panicking, so the runtime can route to a clean DD#25 trap.
pub fn atoi_checked(bytes: &[u8]) -> Option<i64> {
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
        parse_radix_magnitude_checked(&bytes[i + skip..], radix)?
    } else {
        parse_decimal_magnitude_checked(&bytes[i..])?
    };
    if negative {
        magnitude.checked_neg()
    } else {
        Some(magnitude)
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

/// DD#77: strict integer-validity test — true iff the **entire** byte slice is a
/// complete integer: an optional `+`/`-` sign, then either a `0x`/`0o`/`0b`
/// prefix with ≥1 valid digit, or ≥1 decimal digit — and nothing else. Unlike
/// `atoi_inner` (lenient: leading-space skip, partial parse) this rejects
/// surrounding whitespace and trailing junk (`"42abc"` → false) and the empty
/// string. Range-checked (DD#77, DG 2026-08-17): a well-formed but out-of-range
/// value (`> i64::MAX` / `< i64::MIN`) is **false**, so `[[ -i $s ]]` means
/// "valid AND representable" — the conversion `$(( ${s:?} ))` cannot overflow.
pub fn is_int_inner(bytes: &[u8]) -> bool {
    parse_strict_i64(bytes).is_some()
}

/// Strict, range-checked parse of a complete integer: optional `+`/`-` sign,
/// then a `0x`/`0o`/`0b` prefix with ≥1 valid digit or ≥1 decimal digit, whole
/// slice, no surrounding whitespace or trailing junk. Returns the i64 value, or
/// None on a format error OR a value that does not fit i64 (honoring the
/// i64::MIN asymmetry). Shared by `is_int_inner` (`-i` / the DD#77 arith
/// disposition validity gate).
pub fn parse_strict_i64(bytes: &[u8]) -> Option<i64> {
    if bytes.is_empty() {
        return None;
    }
    let (neg, rest) = match bytes[0] {
        b'-' => (true, &bytes[1..]),
        b'+' => (false, &bytes[1..]),
        _ => (false, bytes),
    };
    if rest.is_empty() {
        return None; // sign with no digits
    }
    let (radix, digits) = match radix_prefix(rest) {
        Some((r, skip)) => (r, &rest[skip..]),
        None => (10, rest),
    };
    if digits.is_empty() {
        return None; // prefix with no digits
    }
    let mut mag: u64 = 0;
    for &b in digits {
        let d = (b as char).to_digit(radix)?; // invalid digit → None
        mag = mag.checked_mul(radix as u64)?.checked_add(d as u64)?;
    }
    // Range-check into i64 via i128 (u64 magnitude always fits i128), honoring
    // the MIN asymmetry: -(i64::MAX + 1) == i64::MIN is valid.
    let val: i128 = if neg { -(mag as i128) } else { mag as i128 };
    if val >= i64::MIN as i128 && val <= i64::MAX as i128 {
        Some(val as i64)
    } else {
        None
    }
}

/// Parse leading decimal digits; stops at the first non-digit. Leading zeros
/// are ordinary decimal digits (no octal).
/// Parse leading decimal digits; stops at the first non-digit. Leading zeros
/// are ordinary decimal digits (no octal). None on i64 overflow.
fn parse_decimal_magnitude_checked(bytes: &[u8]) -> Option<i64> {
    let mut result: i64 = 0;
    for &b in bytes {
        if !b.is_ascii_digit() {
            break;
        }
        result = result.checked_mul(10)?.checked_add((b - b'0') as i64)?;
    }
    Some(result)
}

/// Parse leading digits in `radix` (2/8/16); stops at the first invalid digit.
/// None on i64 overflow.
fn parse_radix_magnitude_checked(bytes: &[u8], radix: u32) -> Option<i64> {
    let mut result: i64 = 0;
    for &b in bytes {
        let Some(d) = (b as char).to_digit(radix) else {
            break;
        };
        result = result.checked_mul(radix as i64)?.checked_add(d as i64)?;
    }
    Some(result)
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
