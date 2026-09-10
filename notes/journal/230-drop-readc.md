# 230 — Drop `readc` entirely

DG (2026-09-02), after an ICE on `… | readc -a out`: "drop readc entirely;
pull in the absolute minimum from ~/contrib/util-linux to vendor getopt, patch
to default to NULL separation and no quoting and build as sash-getopt binary."

This journal covers the first half — removing `readc`. The `sash-getopt`
vendoring follows (journal 231).

## Why

`readc` (cooked read, shipped `688616a` as switch-free-read-family step 1) was
meant to parse `getopt`'s shell-quoted output without `eval` — cooked-split on
`\`-escaped whitespace, then a `gsub` to strip quotes. In review DG judged this
too fragile: parsing a program's shell-quoting is exactly the eval-adjacent
thing sash exists to avoid, and it's format-coupled to getopt. The robust idiom
is **NUL-splitting** (a NUL can't occur in an argv element): a patched
`sash-getopt` that emits NUL-separated, unquoted fields, piped into a
NUL-delimited `read`. So `readc` doesn't earn its place.

The trigger was also a real bug: `readc -a` (array read) was only half-wired
(scalar paths + type_check array *registration*, not the array *lowering*), so
`out` was registered as an array but never emitted → codegen ICE. Rather than
finish `readc -a`, DG dropped `readc`.

## Removed

- **runtime** (`sash-rt`): `sash_split_first_word_cooked`, `sash_deescape`.
- **SSA**: `"readc" => Read` in `from_name`; the `cooked: bool` field on
  `ReadLine`/`ReadLineMulti`.
- **transform**: the `cooked = name == "readc"` in `handle_read` (+ its `name`
  param) and the lastpipe path; `readc` from `is_lastpipe_read_last`; the
  for-cmd `cooked: false`.
- **codegen (native)**: the `cooked` params on `emit_readline`/
  `emit_readline_multi`; the de-escape paths; `ReadSplitRefs.{word_cooked,
  deescape}`; the `split_first_word_cooked`/`deescape` externs + `ExternFuncs`
  fields.
- **codegen (c)**: the `cooked` wiring in `emit_readline`/`_multi`/`_last`/
  `_field`; the two extern decls.
- **front-end recognition**: `readc` from `BUILTINS` (SB5011), taint (2 sites),
  `collect_read_targets`, `type_check::register_read_array_target`, and the
  janet `while read` recognizer.
- **specs**: `spec/wordsplit.sh` 22.20–22.24 → a single 22.20 asserting `readc`
  is now unknown (SB5011).
- task `janet-readc-cooked` (obsolete).

## Kept (independent of readc)

`emit_c::insert_read_target_types` — the c-backend fix (declare every read
target so an assigned-but-unread one compiles) was discovered alongside readc
but is orthogonal; it stays.

## Verification

`readc` is now SB5011 ("command not found"); the original ICE command is a
clean SB5011, no panic. Scalar `read`, multi-var `read a b`, and array
`read -a out` all still work (array-read + bless-all no longer ICE). `cargo
fmt`/`clippy` clean; tests build. Full 3-mode sweep vs HEAD `ddc237a`: **native
0 regressions, c 0, janet 0**.

## Next

`sash-getopt`: vendor the minimum from `~/contrib/util-linux/misc-utils/
getopt.c`, patch it to default to NUL field separation + no quoting, build as
`sash-getopt` (journal 231).
