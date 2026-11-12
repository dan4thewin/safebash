title: "Stale Rust test suites: sash-rt-tests broken + cargo core suites use old syntax"
status: done
superseded-by: audit-rust-test-suites
priority: P3
kind: chore

> **SUPERSEDED 2026-07-06** — consolidated into
> `notes/tasks/audit-rust-test-suites.md` (the canonical Rust-test-rot
> task). The unique finding here (the `sash-rt-tests` link breakage against
> removed `sash_streq` / `sash_arith_eval` externs, and wiring it into
> `mk test`) is now a gate + "Absorbed detail" note there. Kept for history.

# Stale Rust test suites (pre-existing, found during DD#63 captures)

Two pre-existing problems with the Rust test suites, surfaced while
verifying the captures work (2026-06-25). Neither is a captures
regression — flagged here so they don't keep masking real signal.

## 1. `sash-rt-tests` cannot link (references removed runtime symbols)

`sash-rt-tests/tests/unit_tests.rs` declares `extern` `sash_streq` and
`sash_arith_eval`, which **no longer exist** in `sash-rt/src/lib.rs`. The
test binary fails to link (`undefined symbol`). The harness is in the
workspace (`Cargo.toml` members) but **not** on `mk test`'s path (`mk
test` runs `cargo test` in sash/sash-convert/sash-expand + the C
`regex-test`, not sash-rt-tests), so it has rotted unnoticed.

Fix: drop or update the `streq_*` / `arith_*` tests (and any other
dead-symbol references), then add `sash-rt-tests` to `mk test` so it
can't rot again. The harness is valuable — it's the only place that
unit-tests the `extern "C"` runtime in isolation (e.g. the DD#63
`sash_regex_match_into` / backref tests were written against it before
being reverted because of this breakage).

## 2. Cargo core suites: ~122 failures from old syntax

On committed `hand-parser` (ea556b3), `cargo test -p sash --release`:

| suite                | pass | fail |
|----------------------|-----:|-----:|
| codegen_tests        |  202 |   54 |
| e2e_tests            |  101 |   23 |
| oracle_tests         |   51 |   12 |
| janet_tests          |   26 |    9 |
| taint_spec_tests     |   18 |   24 |
| brush_compat_tests   |    — |   27 | (brush not installed)

Spot-checked failures use **pre-subset syntax** the compiler now rejects:
`declare -a arr=(…)` (SB5001), `${#arr[@]}`, etc. These tests predate the
`int`/`const`/`array`/`map` + `$a[i]` migration and were never updated.

This means the AGENTS.md completion item "All tests pass (`mk test`)" does
not currently hold — the **spec suite** (`spec/*.sh`, native 0-failing) is
the live gate. Either update/retire the stale cargo tests, or make the
checklist explicit that the spec suite is authoritative and the cargo
suites are partially legacy.

DG to decide scope/priority — this is a cleanup, not a correctness bug in
shipped code.

## Closed 2026-09-10 (backlog prune)

Bookkeeping only — closed 2026-09-10. Successor `audit-rust-test-suites` is live.
