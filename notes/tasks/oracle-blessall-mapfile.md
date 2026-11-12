title: C/Janet oracle support for bless-all + mapfile
status: done
scope: c/janet backends only (native complete)
priority: P2
kind: feature
depends: []
gates:
  - emit_c handles SsaInst::BlessAll / BlessRetag (mirror codegen loop)
  - emit_c handles SsaInst::Mapfile / ReadArray + RedirectPush/Pop
  - DD#61 runtime tests (taint.sh 60.* native-guarded) un-guard for c
  - decide whether janet (IR oracle, elides bless) should grow bless-all

# C/Janet oracle support for bless-all + mapfile

DD#61 (array taint + bless-all) is fully implemented and tested under
native codegen. The janet (IR) and c (SSA) oracles are partial and don't
cover it, so the bless-all **runtime** spec tests (`taint.sh` 60.9–60.12,
60.15–60.18, 60.21–60.23, 60.25, 60.29, 60.31) are guarded `in_modes
native` + `skip_chk`.

## Why guarded, not implemented

`emit_c` (the SSA differential oracle) emits `/* TODO inst */` no-ops for
`Mapfile`, `ReadArray`, `RedirectPush/Pop`, `Bless`, `BlessAll`, and
`BlessRetag` — mapfile/redirects/bless were never wired (see the
deferred list in `add-c-oracle.md`). `emit_janet` deliberately elides
bless validation ("validation elided in janet").

The DD#61 **compile-time** checks (construction restriction SB5021,
bless-form errors SB4030, binder rules SB5020) DO run in every backend
(they're front-end), so those tests are cross-mode already.

## Work

1. `emit_c`: implement `Mapfile`/`ReadArray` (sash_read_line loop into a
   table, map-mode split) and `RedirectPush`/`Pop`, then `BlessAll`
   (mirror `dispatch.rs::emit_bless_all`'s loop) and `BlessRetag` (copy
   the table slot). This gives differential-oracle coverage of the
   bless-all codegen — its whole purpose.
2. Un-guard the DD#61 runtime tests for c mode (keep native; decide janet).
3. Janet: decide whether the IR oracle should grow bless-all at all
   (it elides bless by design — may stay native+c only).

## Notes

- The codegen loop to mirror is `sash/src/codegen/dispatch.rs`
  `emit_bless_all` / `emit_bless_retag`.
- Runtime table helpers already declared in emit_c: `sash_table_len`,
  `sash_table_value`, `sash_table_key`.

## Resolution (re-measured 2026-09-13)

**Gates 1-3 are done** — landed 2026-08-20, before the `bless` -> `attest`
rename, which is why this filing reads as open: it is written entirely in the
retired vocabulary. `SsaInst::BlessAll` does not exist anywhere in `sash/src`;
the instruction is `AttestAll`.

Measured against the live binary:

- `emit_c` implements `ReadArray`, `Mapfile`, `RedirectPush`/`RedirectPop`
  and `AttestAll` (`sash/src/emit_c.rs` lines 58-59, 418, 720, 1758-1769,
  1888-1889, 2023-2065). No `TODO` arm remains for any of them.
- `spec/taint.sh` has **zero** `in_modes native` guards; every DD#61 runtime
  test (60.9-60.12, 60.15-60.18, 60.21-60.23, 60.25, 60.29, 60.31) runs
  cross-mode. The nine remaining `c:` skips in `spec.db` are `104.1`-`104.4`,
  `32.1`-`32.2` and `57.46a`-`57.46c` — unrelated.

A stale comment in `spec/taint.sh` still claimed the runtime tests were
"guarded to native", contradicting the note 120 lines below it; corrected in
the same commit.

**Gate 4 (janet) is split out**, not met: whether the janet IR oracle should
grow attest-all is part of DG's undecided decoupled-janet question, and all
`janet-*` tasks are deferred behind it. See `notes/tasks/` for that cohort.
