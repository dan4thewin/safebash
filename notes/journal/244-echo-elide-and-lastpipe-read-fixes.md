---
title: "two bug fixes: builtin-echo elide ICE + fresh-array lastpipe read segfault"
date: 2026-09-04
---

# Builtin-echo elide ICE + fresh-array lastpipe read segfault

DG hit `echo -n $ckd:! | read -d '' -a norm` and got an ICE:
`ElideEmpty (:~) must be expanded by a list builder, not lowered as a scalar
value`. Investigating surfaced **two pre-existing bugs** (both independent of the
DD#91 `:!` work — they reproduce with `:~` and with no elide at all), each a
stop-everything per the bug policy.

## Bug 1 — builtin `echo` doesn't handle an elide (`:~`/`:!`) arg

`echo $x:~` / `echo $x:!` (the builtin, inline or in a pipeline stage) reached
scalar `lower_value` on the `ElideEmpty` → ICE (native), or wrote a stray
separator (c/janet: `a  b` instead of `a b`). The external `/bin/echo` was fine
(its argv builder already elides). A pre-existing DD#84 gap; `:!` inherited it.

Fix: builtin echo now writes an elide arg's *(separator + value)* only when the
value is non-empty — matching the argv builder. `:~`/`:!` only appear as a whole
arg (SB5046 forbids them in a concat), so the arg-level case covers it.
- native `codegen/commands.rs`: `echo_write_elide_arg` (a `brif` on empty,
  threading the write-failure accumulator through a block param).
- c `emit_c.rs`: `emit_echo_elide_arg` (`if (len > 0) { … }`).
- janet: still lacks `-n` and elide (pre-existing) → filed
  `janet-echo-flags-and-elide`; the multi-arg specs are gated native,c.

## Bug 2 — a FRESH `read -a` / `mapfile` target in a called function segfaults

`echo hi | read -a norm` (norm not pre-declared, inside a called function)
segfaulted in `sash_table_set` — a **NULL array table**. Root cause: a function
body is lowered twice (a dead `fn_` template + the inlined call). The template's
`autodeclare_bulk_target` registered `norm` in `state.collections`, and that
registration **leaked into the inline**, so the inline's autodeclare deduped
itself away — no table-allocating `Assign norm = IndexedArray([])` before the
`ReadArray` → the runtime read filled a null table.

This is the exact class of bug DD#86 fixed for `readonly_vars` (which
`enter/exit_fn_body_scope` already snapshot for the same "body lowered more than
once" reason). Fix: `enter/exit_fn_body_scope` now also snapshot/restore
`collections` and `assoc_arrays`, so each lowering (template, each inline)
re-allocates its own table. Pre-declaring the array already worked (it allocates
before the read); the bug was fresh targets only.

## Verification

- `wordsplit.sh` 22.28 (fresh `read -a` in a fn — no segfault), 22.29/22.30
  (builtin echo elide empty→`a b` / non-empty→`a hi b`), 22.31 (the reported
  line end-to-end). native,c; janet gated (read-array trailing-newline split +
  echo `-n`/elide gaps). plan 80→84.
- Full 3-mode sweep: **0 regressions**, 12 new (8 passing, 4 janet skips).
- Rust e2e 47 / taint_spec 25 = baseline (the `functions.rs` scope change is
  inert for non-collection code). fmt/clippy clean.

Both bugs predate DD#91; the `:!` line just surfaced them. Filed
`janet-echo-flags-and-elide`.
