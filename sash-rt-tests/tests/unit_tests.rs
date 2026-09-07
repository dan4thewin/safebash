//! Integration tests for sash-rt.
//! Links against the sash-rt static library and tests all extern "C" functions.

// Tell the linker where to find libsash_rt.a
#[link(name = "sash_rt_test")]
extern "C" {
    fn sash_atoi(s_ptr: *const u8, s_len: i64) -> i64;
    fn sash_is_int(s_ptr: *const u8, s_len: i64) -> i64;
    fn sash_is_print(s_ptr: *const u8, s_len: i64) -> i64;
    fn sash_is_graph(s_ptr: *const u8, s_len: i64) -> i64;
    fn sash_itoa(val: i64, out_len: *mut i64) -> *mut u8;
    fn sash_default_value(s_len: i64) -> i64;

    fn sash_table_new(capacity: i64) -> *mut u8; // opaque pointer
    fn sash_table_set(
        t: *mut u8,
        key_ptr: *const u8,
        key_len: i64,
        val_ptr: *const u8,
        val_len: i64,
    );
    fn sash_table_get(
        t: *const u8,
        key_ptr: *const u8,
        key_len: i64,
        out_len: *mut i64,
    ) -> *const u8;
    fn sash_table_len(t: *const u8) -> i64;
    fn sash_table_key(t: *const u8, index: i64, out_len: *mut i64) -> *const u8;
    fn sash_table_value(t: *const u8, index: i64, out_len: *mut i64) -> *const u8;
}

// Helper: read a (ptr, len) result as a &[u8]
unsafe fn read_slice<'a>(ptr: *const u8, len: i64) -> &'a [u8] {
    if ptr.is_null() || len <= 0 {
        &[]
    } else {
        std::slice::from_raw_parts(ptr, len as usize)
    }
}

// ═══════════════════════════════════════════════════════════════
// sash_atoi tests
// ═══════════════════════════════════════════════════════════════

#[test]
fn atoi_zero() {
    unsafe {
        assert_eq!(sash_atoi(b"0".as_ptr(), 1), 0);
    }
}

#[test]
fn is_int_strict() {
    unsafe {
        let t = |s: &[u8]| sash_is_int(s.as_ptr(), s.len() as i64);
        // valid: decimal, sign, leading zeros, radix prefixes
        assert_eq!(t(b"0"), 1);
        assert_eq!(t(b"42"), 1);
        assert_eq!(t(b"-7"), 1);
        assert_eq!(t(b"+9"), 1);
        assert_eq!(t(b"010"), 1); // decimal leading zeros (not octal)
        assert_eq!(t(b"0xff"), 1);
        assert_eq!(t(b"0o17"), 1);
        assert_eq!(t(b"0b1010"), 1);
        // invalid: empty, non-numeric, trailing junk, sign-only, bad prefix digits
        assert_eq!(t(b""), 0);
        assert_eq!(t(b"hello"), 0);
        assert_eq!(t(b"42abc"), 0); // strict: no partial parse
        assert_eq!(t(b"-"), 0);
        assert_eq!(t(b"0x"), 0); // prefix, no digits
        assert_eq!(t(b"0o8"), 0); // 8 invalid for octal
        assert_eq!(t(b"0b2"), 0);
        assert_eq!(t(b" 42"), 0); // no leading whitespace
        assert_eq!(t(b"42 "), 0); // no trailing whitespace
                                  // range-checked (DD#77): a well-formed but out-of-range value is false
        assert_eq!(t(b"9223372036854775807"), 1); // i64::MAX
        assert_eq!(t(b"9223372036854775808"), 0); // MAX + 1 → out of range
        assert_eq!(t(b"-9223372036854775808"), 1); // i64::MIN (asymmetry)
        assert_eq!(t(b"-9223372036854775809"), 0); // MIN - 1 → out of range
        assert_eq!(t(b"99999999999999999999999999"), 0); // wildly out of range
    }
}

/// DD#93 Part C: `p?` — non-empty and every byte ASCII printable (0x20–0x7E).
#[test]
fn is_print_shape() {
    unsafe {
        let t = |s: &[u8]| sash_is_print(s.as_ptr(), s.len() as i64);
        // printable: letters, digits, punctuation, and the space itself
        assert_eq!(t(b"hello"), 1);
        assert_eq!(t(b"hello world"), 1); // 0x20 is printable
        assert_eq!(t(b" "), 1); // a lone space is non-empty and printable
        assert_eq!(t(b"!"), 1); // 0x21, low edge of graph
        assert_eq!(t(b"~"), 1); // 0x7E, high edge
        assert_eq!(t(b"/path/to:file-42"), 1);
        // empty is false (one-token sanitizer: "non-empty AND all in class")
        assert_eq!(t(b""), 0);
        // control characters are not printable
        assert_eq!(t(b"a\tb"), 0); // 0x09
        assert_eq!(t(b"a\nb"), 0); // 0x0A
        assert_eq!(t(b"\x00"), 0);
        assert_eq!(t(b"\x1f"), 0); // just below the range
        assert_eq!(t(b"\x7f"), 0); // DEL, just above
        // ASCII-only: high bytes are rejected (UTF-8 "café")
        assert_eq!(t(b"caf\xc3\xa9"), 0);
        assert_eq!(t(b"\x80"), 0);
        assert_eq!(t(b"\xff"), 0);
        // one bad byte anywhere poisons the whole value
        assert_eq!(t(b"good\x07bad"), 0);
    }
}

/// DD#93 Part C: `g?` — non-empty and every byte ASCII graph (0x21–0x7E),
/// i.e. printable *non-space*.
#[test]
fn is_graph_shape() {
    unsafe {
        let t = |s: &[u8]| sash_is_graph(s.as_ptr(), s.len() as i64);
        assert_eq!(t(b"hello"), 1);
        assert_eq!(t(b"!"), 1); // 0x21, low edge
        assert_eq!(t(b"~"), 1); // 0x7E, high edge
        assert_eq!(t(b"/path/to:file-42"), 1);
        // the space is the whole difference from p?
        assert_eq!(t(b"hello world"), 0);
        assert_eq!(t(b" "), 0);
        assert_eq!(t(b"trailing "), 0);
        // empty, controls, high bytes — same as p?
        assert_eq!(t(b""), 0);
        assert_eq!(t(b"a\tb"), 0);
        assert_eq!(t(b"a\nb"), 0);
        assert_eq!(t(b"\x7f"), 0);
        assert_eq!(t(b"caf\xc3\xa9"), 0);
        assert_eq!(t(b"\xff"), 0);
    }
}

#[test]
fn atoi_positive() {
    unsafe {
        assert_eq!(sash_atoi(b"42".as_ptr(), 2), 42);
    }
}

#[test]
fn atoi_negative() {
    unsafe {
        assert_eq!(sash_atoi(b"-7".as_ptr(), 2), -7);
    }
}

#[test]
fn atoi_large() {
    let s = b"2147483647";
    unsafe {
        assert_eq!(sash_atoi(s.as_ptr(), s.len() as i64), 2147483647);
    }
}

#[test]
fn atoi_large_negative() {
    let s = b"-2147483648";
    unsafe {
        assert_eq!(sash_atoi(s.as_ptr(), s.len() as i64), -2147483648);
    }
}

#[test]
fn atoi_leading_spaces() {
    let s = b"  123";
    unsafe {
        assert_eq!(sash_atoi(s.as_ptr(), s.len() as i64), 123);
    }
}

#[test]
fn atoi_plus_sign() {
    let s = b"+99";
    unsafe {
        assert_eq!(sash_atoi(s.as_ptr(), s.len() as i64), 99);
    }
}

#[test]
fn atoi_empty() {
    unsafe {
        assert_eq!(sash_atoi(b"".as_ptr(), 0), 0);
    }
}

#[test]
fn atoi_non_numeric() {
    let s = b"abc";
    unsafe {
        assert_eq!(sash_atoi(s.as_ptr(), s.len() as i64), 0);
    }
}

#[test]
fn atoi_trailing_junk() {
    let s = b"42abc";
    unsafe {
        assert_eq!(sash_atoi(s.as_ptr(), s.len() as i64), 42);
    }
}

#[test]
fn atoi_null_ptr() {
    unsafe {
        assert_eq!(sash_atoi(std::ptr::null(), 5), 0);
    }
}

// ═══════════════════════════════════════════════════════════════
// sash_itoa tests
// ═══════════════════════════════════════════════════════════════

#[test]
fn itoa_zero() {
    unsafe {
        let mut len: i64 = 0;
        let ptr = sash_itoa(0, &mut len);
        assert_eq!(read_slice(ptr, len), b"0");
    }
}

#[test]
fn itoa_positive() {
    unsafe {
        let mut len: i64 = 0;
        let ptr = sash_itoa(12345, &mut len);
        assert_eq!(read_slice(ptr, len), b"12345");
    }
}

#[test]
fn itoa_negative() {
    unsafe {
        let mut len: i64 = 0;
        let ptr = sash_itoa(-42, &mut len);
        assert_eq!(read_slice(ptr, len), b"-42");
    }
}

#[test]
fn itoa_one() {
    unsafe {
        let mut len: i64 = 0;
        let ptr = sash_itoa(1, &mut len);
        assert_eq!(read_slice(ptr, len), b"1");
    }
}

#[test]
fn itoa_large() {
    unsafe {
        let mut len: i64 = 0;
        let ptr = sash_itoa(999999999, &mut len);
        assert_eq!(read_slice(ptr, len), b"999999999");
    }
}

// ═══════════════════════════════════════════════════════════════
// sash_default_value tests
// ═══════════════════════════════════════════════════════════════

#[test]
fn default_value_empty() {
    unsafe {
        assert_eq!(sash_default_value(0), 0);
    }
}

#[test]
fn default_value_negative() {
    unsafe {
        assert_eq!(sash_default_value(-1), 0);
    }
}

#[test]
fn default_value_set() {
    unsafe {
        assert_eq!(sash_default_value(5), 1);
    }
}

// ═══════════════════════════════════════════════════════════════
// sash_table tests
// ═══════════════════════════════════════════════════════════════

#[test]
fn table_new_empty() {
    unsafe {
        let t = sash_table_new(4);
        assert_eq!(sash_table_len(t), 0);
    }
}

#[test]
fn table_set_get() {
    unsafe {
        let t = sash_table_new(4);
        let key = b"hello";
        let val = b"world";
        sash_table_set(t, key.as_ptr(), 5, val.as_ptr(), 5);
        assert_eq!(sash_table_len(t), 1);

        let mut out_len: i64 = 0;
        let result = sash_table_get(t, key.as_ptr(), 5, &mut out_len);
        assert_eq!(read_slice(result, out_len), b"world");
    }
}

#[test]
fn table_get_missing() {
    unsafe {
        let t = sash_table_new(4);
        let key = b"missing";
        let mut out_len: i64 = 0;
        let result = sash_table_get(t, key.as_ptr(), 7, &mut out_len);
        assert!(result.is_null());
        assert_eq!(out_len, 0);
    }
}

#[test]
fn table_update_existing() {
    unsafe {
        let t = sash_table_new(4);
        let key = b"k";
        let v1 = b"first";
        let v2 = b"second";
        sash_table_set(t, key.as_ptr(), 1, v1.as_ptr(), 5);
        sash_table_set(t, key.as_ptr(), 1, v2.as_ptr(), 6);
        assert_eq!(sash_table_len(t), 1);

        let mut out_len: i64 = 0;
        let result = sash_table_get(t, key.as_ptr(), 1, &mut out_len);
        assert_eq!(read_slice(result, out_len), b"second");
    }
}

#[test]
fn table_multiple_keys() {
    unsafe {
        let t = sash_table_new(4);
        let keys: &[&[u8]] = &[b"a", b"b", b"c", b"d", b"e"];
        let vals: &[&[u8]] = &[b"1", b"2", b"3", b"4", b"5"];
        for i in 0..5 {
            sash_table_set(
                t,
                keys[i].as_ptr(),
                keys[i].len() as i64,
                vals[i].as_ptr(),
                vals[i].len() as i64,
            );
        }
        assert_eq!(sash_table_len(t), 5);

        for i in 0..5 {
            let mut out_len: i64 = 0;
            let result = sash_table_get(t, keys[i].as_ptr(), keys[i].len() as i64, &mut out_len);
            assert_eq!(read_slice(result, out_len), vals[i]);
        }
    }
}

#[test]
fn table_key_value_iteration() {
    unsafe {
        let t = sash_table_new(4);
        sash_table_set(t, b"x".as_ptr(), 1, b"10".as_ptr(), 2);
        sash_table_set(t, b"y".as_ptr(), 1, b"20".as_ptr(), 2);

        let mut out_len: i64 = 0;
        let k0 = sash_table_key(t, 0, &mut out_len);
        assert_eq!(read_slice(k0, out_len), b"x");

        let v0 = sash_table_value(t, 0, &mut out_len);
        assert_eq!(read_slice(v0, out_len), b"10");

        let k1 = sash_table_key(t, 1, &mut out_len);
        assert_eq!(read_slice(k1, out_len), b"y");

        let v1 = sash_table_value(t, 1, &mut out_len);
        assert_eq!(read_slice(v1, out_len), b"20");
    }
}

#[test]
fn table_key_out_of_bounds() {
    unsafe {
        let t = sash_table_new(4);
        let mut out_len: i64 = 0;
        let k = sash_table_key(t, 0, &mut out_len);
        assert!(k.is_null());
        let k = sash_table_key(t, -1, &mut out_len);
        assert!(k.is_null());
    }
}

#[test]
fn table_grow_many_entries() {
    // Insert enough entries to trigger grow/resize
    unsafe {
        let t = sash_table_new(4);
        for i in 0..20u8 {
            let key = [b'K', i + b'A'];
            let val = [b'V', i + b'0'];
            sash_table_set(t, key.as_ptr(), 2, val.as_ptr(), 2);
        }
        assert_eq!(sash_table_len(t), 20);

        // Verify all entries still accessible
        for i in 0..20u8 {
            let key = [b'K', i + b'A'];
            let mut out_len: i64 = 0;
            let result = sash_table_get(t, key.as_ptr(), 2, &mut out_len);
            assert!(!result.is_null(), "key {:?} not found after grow", key);
        }
    }
}

#[test]
fn table_integer_keys() {
    // Test with raw 8-byte integer keys (how indexed arrays work)
    unsafe {
        let t = sash_table_new(4);
        let k0: i64 = 0;
        let k1: i64 = 1;
        let k99: i64 = 99;
        let v0 = b"zero";
        let v1 = b"one";
        let v99 = b"ninetynine";

        sash_table_set(
            t,
            &k0 as *const i64 as *const u8,
            8,
            v0.as_ptr(),
            v0.len() as i64,
        );
        sash_table_set(
            t,
            &k1 as *const i64 as *const u8,
            8,
            v1.as_ptr(),
            v1.len() as i64,
        );
        sash_table_set(
            t,
            &k99 as *const i64 as *const u8,
            8,
            v99.as_ptr(),
            v99.len() as i64,
        );

        assert_eq!(sash_table_len(t), 3);

        let mut out_len: i64 = 0;
        let r = sash_table_get(t, &k1 as *const i64 as *const u8, 8, &mut out_len);
        assert_eq!(read_slice(r, out_len), b"one");

        let r = sash_table_get(t, &k99 as *const i64 as *const u8, 8, &mut out_len);
        assert_eq!(read_slice(r, out_len), b"ninetynine");
    }
}
