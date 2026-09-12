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

/// True iff `bytes` is **non-empty** and every byte satisfies `pred`. Shared
/// by the DD#93 content-shape tests, which are all of the form "non-empty and
/// all bytes in a class" — adding a further class (`a?`, `d?`, `l?`, `s?`, …)
/// is one line here plus one dispatch arm per backend.
fn all_bytes_match(bytes: &[u8], pred: fn(u8) -> bool) -> bool {
    !bytes.is_empty() && bytes.iter().copied().all(pred)
}

/// DD#93 Part C: `p?` — true iff the value is non-empty and every byte is
/// ASCII **printable** (0x20–0x7E, space through `~`). High bytes (≥ 0x80)
/// are rejected: ambiguous non-ASCII is part of what a shape test screens
/// out (a UTF-8-aware variant would get a distinct spelling).
pub fn is_print_inner(bytes: &[u8]) -> bool {
    all_bytes_match(bytes, |b| (0x20..=0x7E).contains(&b))
}

/// DD#93 Part C: `g?` — true iff the value is non-empty and every byte is
/// ASCII **graph** (0x21–0x7E), i.e. printable non-space. Differs from
/// `is_print_inner` only in excluding 0x20.
pub fn is_graph_inner(bytes: &[u8]) -> bool {
    all_bytes_match(bytes, |b| (0x21..=0x7E).contains(&b))
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
