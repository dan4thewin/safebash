//! Shared helpers for the integration test crates.
//!
//! Each `tests/*.rs` is compiled as its own integration crate. To share
//! code without making every helper crate-public, this file lives under
//! `tests/common/mod.rs` — Rust treats `mod.rs` files in `tests/` as
//! regular modules (not test targets), so a `mod common;` in any test
//! file pulls these helpers in directly.

#![allow(dead_code)] // not every helper is used by every test crate

/// Assert that `stderr` contains the given diagnostic code (e.g. `"SB4020"`).
/// Panic message shows the actual stderr for debugging.
pub fn expect_diag(stderr: &str, code: &str) {
    assert!(
        stderr.contains(code),
        "expected {} in diagnostics, got:\n{}",
        code,
        stderr
    );
}
