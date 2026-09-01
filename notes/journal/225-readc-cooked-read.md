# 225 — `readc`: cooked read (switch-free-read-family, step 1)

DG: "capture the larger question, plus a survey of all switch use into a
decision task; implement readc now." The decision task
(`notes/tasks/switch-free-read-family.md`) captures the larger direction
(should `read` become `readr`/`readc`, drop `-a`/`-f`/`-d`/`-z`?). This
session lands the concrete first step: **`readc`**, a cooked read.

## What `readc` is

sash `read` is always **raw** (backslashes literal — the bash `read -r`
behavior). `readc` is the **cooked** counterpart: a `\` escapes the next
byte, so `\<whitespace>` does *not* split the field and the field is
de-escaped (backslash removed). This is bash's default `read`.

Given identical input bytes, `readc` ≡ bash cooked `read`, `read` ≡ bash
`read -r`:

    echo 'has\ space rest' | readc a b   →  a='has space'  b='rest'
    echo 'has\ space rest' | read  a b   →  a='has\'        b='space rest'

It unblocks the eval-free **getopt-csh idiom**: `getopt -s csh` emits
`'has'\ 'space'`; a cooked split keeps that as one field, then a trivial
`gsub` strips the quotes — no `eval`, no `-r` mangling.

## Mechanism

Two runtime primitives (`sash-rt`, both `alloc_cstr` NUL-terminated per
DD#87):

- `sash_split_first_word_cooked(p,len,&rest,&restlen) → (word,wordlen)` —
  like `sash_split_first_word` but a `\`-escaped space does not terminate
  the word, and the returned word is de-escaped. The *rest* is kept RAW so
  the next split sees the original bytes.
- `sash_deescape(p,len) → (p,len)` — de-escape a whole value (`\x`→`x`,
  trailing lone `\` kept). Used for a single-var `readc x` and for the last
  (remainder) var of a multi-var `readc a b c`.

`ReadLine`/`ReadLineMulti` carry a new `cooked: bool` (SSA). `"readc"`
resolves to `BuiltinCmd::Read` (`from_name`); the transform sets
`cooked = name == "readc"` on every read path (standalone, lastpipe,
for-cmd default false). Codegen (native + c):

- single-var `readc x` → de-escape the whole line via `deescape`;
- multi-var — each non-last field via `split_first_word_cooked` (when no
  `-f`), the last var (remainder) via `deescape`.

`readc` had to be recognized **everywhere** `read` is (flat literal
checks): `BUILTINS` (SB5011 resolution), taint (`taint_read_target`,
clean-pipe-source, `collect_read_targets`), `type_check`, the lastpipe
`is_lastpipe_read_last`, and janet's `while read` recognizer.

## janet: deferred (raw)

janet recognizes `readc` (so it neither SB5011s nor crashes) but its
codegen ignores `cooked` — it behaves as raw `read`. Cooked janet is
filed (`notes/tasks/janet-readc-cooked.md`). The readc specs are gated
`in_modes native,c` with `skip_chk` on janet.

## Pre-existing c bug fixed (native/c divergence)

Writing the specs surfaced a **pre-existing** c-backend bug: an unused
read target was never declared. `emit_c::collect_inst_var_types` walks
assignments/args/phis to decide which C vars to declare, but had no arm
for `ReadLine`/`ReadLineMulti` — a read target was declared only as a
side effect of being *referenced* later. `echo x y | read a b; echo $b`
(a used, but only b read) → C emitted `a_ptr = …` against an undeclared
`a_ptr`. Native always declares all read targets, so it compiled fine —
a native/c divergence. Reproduces at HEAD with raw `read` (not new to
`readc`). Fixed by recording every read target as a `String` var
(`insert_read_target_types`).

## Not a bug: double-quote `\\`

An early spec (`echo "x\\y" | readc v`) diverged from bash (`x\y` vs
bash `xy`). Root cause is **not** readc — it is DD#64 (no-shell-
backslash): sash double quotes keep `\\` as two backslashes (bash
collapses to one). Ratified behavior; the spec was dropped and the
de-escape is correct on the bytes sash actually produces.

## Complexity

The cooked branches pushed `emit_readline_multi` over the cognitive gate
(21 native). Refactored: extracted `emit_read_line_call` (shared fd/delim
read, also de-dups the single-var path), `emit_read_last_var`,
`emit_read_field_split` (native); `emit_readline_last`,
`insert_read_target_types` (c). All touched functions now clear
cc>3×mean / cog>3×mean.

## Verification

`spec/wordsplit.sh` 22.20–22.24 (native,c; janet skip): escaped-space
field kept+de-escaped, remainder de-escaped, single-var de-escape, raw
`read` contrast, external-source taint. Full 3-mode sweep, latest-run-
per-test diff vs HEAD `005c7d4`: **native 0 regressions +5**, **c 0
regressions +5**, janet 0/0. `cargo fmt` + `clippy` clean; tests compile
with the new `cooked` field.

## Deferred / filed

- `switch-free-read-family` (the larger decision: `readr` rename, drop
  `-a`/`-f`/`-d`/`-z`, unify the two `parse_read_flags`, resolve the `-r`
  three-way inconsistency) — still `ready`, awaiting DG.
- `janet-readc-cooked` — janet cooked de-escaping.
