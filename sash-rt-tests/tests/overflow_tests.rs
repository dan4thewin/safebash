//! Overflow tests for sash-rt (DD#26: integer overflow traps).
//!
//! The inner functions are pure safe Rust that use .expect()/.panic!()
//! for overflow. We test them directly here — the standard test harness
//! catches panics normally without hitting the no_std panic handler.

// inner.rs is include!()d below; this crate only exercises a subset of
// the functions it defines, so silence 'never used' for the rest.
#![allow(dead_code)]

// Import the inner functions directly from sash-rt source.
// These are pub and have no no_std dependencies.
include!("../../sash-rt/src/inner.rs");

// ═══════════════════════════════════════════════════════════════
// atoi overflow
// ═══════════════════════════════════════════════════════════════

#[test]
fn atoi_max_i64() {
    assert_eq!(atoi_inner(b"9223372036854775807"), i64::MAX);
}

#[test]
fn atoi_near_max() {
    assert_eq!(atoi_inner(b"9223372036854775806"), i64::MAX - 1);
}

#[test]
fn atoi_min_i64_representable() {
    // -(i64::MAX) works — parsed as positive then negated
    assert_eq!(atoi_inner(b"-9223372036854775807"), -9223372036854775807);
}

#[test]
#[should_panic(expected = "integer overflow")]
fn atoi_overflow_large() {
    atoi_inner(b"99999999999999999999");
}

#[test]
#[should_panic(expected = "integer overflow")]
fn atoi_overflow_max_plus_one() {
    atoi_inner(b"9223372036854775808");
}

#[test]
#[should_panic(expected = "integer overflow")]
fn atoi_overflow_negative_large() {
    atoi_inner(b"-99999999999999999999");
}

#[test]
#[should_panic(expected = "integer overflow")]
fn atoi_overflow_min_i64() {
    // -9223372036854775808: positive part overflows before negation
    atoi_inner(b"-9223372036854775808");
}

// ═══════════════════════════════════════════════════════════════
// itoa overflow
// ═══════════════════════════════════════════════════════════════

#[test]
fn itoa_max_i64() {
    let mut buf = [0u8; 21];
    assert_eq!(itoa_inner(i64::MAX, &mut buf), b"9223372036854775807");
}

#[test]
fn itoa_neg_max() {
    let mut buf = [0u8; 21];
    assert_eq!(
        itoa_inner(-9223372036854775807, &mut buf),
        b"-9223372036854775807"
    );
}

#[test]
fn itoa_min_i64() {
    // i64::MIN formats correctly via unsigned_abs (journal 120) — it does NOT
    // panic (the old `#[should_panic]` expectation was stale).
    let mut buf = [0u8; 21];
    assert_eq!(itoa_inner(i64::MIN, &mut buf), b"-9223372036854775808");
}

// ═══════════════════════════════════════════════════════════════
// arith overflow
// ═══════════════════════════════════════════════════════════════

#[test]
#[should_panic(expected = "integer overflow")]
fn arith_add_overflow() {
    arith_eval_inner(i64::MAX, b'+', 1);
}

#[test]
#[should_panic(expected = "integer overflow")]
fn arith_sub_overflow() {
    arith_eval_inner(i64::MIN, b'-', 1);
}

#[test]
#[should_panic(expected = "integer overflow")]
fn arith_mul_overflow() {
    arith_eval_inner(i64::MAX, b'*', 2);
}

#[test]
#[should_panic(expected = "division by zero")]
fn arith_div_zero() {
    arith_eval_inner(42, b'/', 0);
}

#[test]
#[should_panic(expected = "division by zero")]
fn arith_mod_zero() {
    arith_eval_inner(42, b'%', 0);
}

#[test]
#[should_panic(expected = "integer overflow")]
fn arith_div_min_neg1() {
    // i64::MIN / -1 would be i64::MAX + 1
    arith_eval_inner(i64::MIN, b'/', -1);
}

#[test]
fn arith_boundary_safe() {
    // These should NOT overflow
    assert_eq!(arith_eval_inner(i64::MAX - 1, b'+', 1), i64::MAX);
    assert_eq!(arith_eval_inner(i64::MIN + 1, b'-', 1), i64::MIN);
    assert_eq!(arith_eval_inner(i64::MAX / 2, b'*', 2), i64::MAX - 1);
}

// ═══════════════════════════════════════════════════════════════
// Fixed-point dtoi/itod tests
// ═══════════════════════════════════════════════════════════════

#[path = "../../sash-rt/src/inner.rs"]
mod inner;

#[test]
fn dtoi_basic() {
    assert_eq!(inner::dtoi_inner(2, b"19.99"), 1999);
}

#[test]
fn dtoi_no_fraction() {
    assert_eq!(inner::dtoi_inner(2, b"5"), 500);
}

#[test]
fn dtoi_short_fraction() {
    assert_eq!(inner::dtoi_inner(3, b"0.5"), 500);
}

#[test]
fn dtoi_zero() {
    assert_eq!(inner::dtoi_inner(2, b"0.00"), 0);
}

#[test]
fn dtoi_negative() {
    assert_eq!(inner::dtoi_inner(2, b"-3.50"), -350);
}

#[test]
fn dtoi_one_digit() {
    assert_eq!(inner::dtoi_inner(1, b"0.5"), 5);
}

#[test]
#[should_panic(expected = "precision")]
fn dtoi_excess_precision() {
    inner::dtoi_inner(2, b"1.234");
}

#[test]
#[should_panic(expected = "invalid")]
fn dtoi_invalid_input() {
    inner::dtoi_inner(2, b"abc");
}

#[test]
fn itod_basic() {
    let mut buf = [0u8; 32];
    let len = inner::itod_inner(2, 1999, &mut buf);
    assert_eq!(&buf[..len], b"19.99");
}

#[test]
fn itod_zero_padded() {
    let mut buf = [0u8; 32];
    let len = inner::itod_inner(2, 5, &mut buf);
    assert_eq!(&buf[..len], b"0.05");
}

#[test]
fn itod_zero() {
    let mut buf = [0u8; 32];
    let len = inner::itod_inner(2, 0, &mut buf);
    assert_eq!(&buf[..len], b"0.00");
}

#[test]
fn itod_negative() {
    let mut buf = [0u8; 32];
    let len = inner::itod_inner(2, -350, &mut buf);
    assert_eq!(&buf[..len], b"-3.50");
}

#[test]
fn itod_one_digit() {
    let mut buf = [0u8; 32];
    let len = inner::itod_inner(1, 5, &mut buf);
    assert_eq!(&buf[..len], b"0.5");
}

#[test]
fn dtoi_itod_roundtrip() {
    let val = inner::dtoi_inner(2, b"42.07");
    let mut buf = [0u8; 32];
    let len = inner::itod_inner(2, val, &mut buf);
    assert_eq!(&buf[..len], b"42.07");
}

#[test]
fn dtoi_itod_roundtrip_negative() {
    let val = inner::dtoi_inner(3, b"-1.500");
    let mut buf = [0u8; 32];
    let len = inner::itod_inner(3, val, &mut buf);
    assert_eq!(&buf[..len], b"-1.500");
}
