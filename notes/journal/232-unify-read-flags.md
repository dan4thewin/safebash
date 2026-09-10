# 232 — Unify the read-flag parsers; `read -d ''` = NUL

DG: "unify them, support -d '', if there are other read behaviors not wired
in, call them out to me." Follows the `sash-getopt` vendoring (journal 231):
the getopt idiom needs a NUL-delimited read.

## One shared classifier

There were **four** independent read-argument parsers, each with its own bugs:

1. `transform/cmd_handlers::parse_read_flags` (standalone `read`) — struct out,
   `-a`, rejects unknown flags (SB5015).
2. `transform/stmt_handlers::parse_read_flags` (lastpipe `… | read`) — tuple
   out, no `-a`, **silently ignores** unknown flags; its `-d` split form was
   also buggy (discarded the delim value, forcing NUL).
3. `validate::collect_read_targets` — forward scan, registered every non-`-`
   word as a target, so a delim VALUE (`read -d : x` → `:`) became a phantom
   target → a **dead-var (SB5050) false positive** on `:`.
4. `type_check::register_read_array_target` — its own `-a` scan.

Replaced all four with a single `ir::classify_read_flags(&[Expr]) -> ReadFlags`
(next to `classify_bless`). It classifies the argument words once — `-a ARR`,
`-d DELIM`, `-f SEP`, var-name targets, and the first `unknown_flag` — using a
proper value extractor (`read_arg_value`) that unwraps one layer of quotes.
Consumers:

- `handle_read` / `handle_lastpipe_read` (transform): call it, emit SB5015 on
  `unknown_flag` (now **consistent** — the lastpipe path no longer silently
  ignores unknown flags), then lower to SSA.
- `collect_read_targets` (validate): `read` → its `var_names` + `array_name`
  (skips delim values → the SB5050 `:` false positive is gone).
- `register_read_array_target` (type_check): its `array_name`.

Left `taint_read_target` (validate) alone: it also serves `mapfile`/`argv`
(different flag shapes) and its reverse-scan already avoids the delim-value
bug — folding it in risked taint behavior, so it's out of scope (noted).

## `read -d ''` = NUL

The root cause of `-d ''` not working was `expr_to_string`, which
**re-serializes with quotes** (`''` → `''`, `""` → `""`), so the delim came out
as `'`/`"` (39/34) instead of empty. `read_arg_value` unwraps quotes (an empty
`''`/`""` → the empty string), and an empty byte-flag value → NUL (`0`). So:

```
sash-getopt -o ab: -- -a -b val 'has space' | read -a opts -d ''
# opts = [-a] [-b] [val] [--] [has space] []
```

Both forms — pipeline (`| read -a arr -d ''`, via `is_mapfile_last` →
`ReadArray`) and redirect (`read -a arr -d '' < f`) — split NUL-separated
fields correctly; a space-bearing field stays one element. (The runtime split
`sash_split_into_array` was already length-based / NUL-safe.)

`read -z` is still rejected — but now **consistently** in both paths (SB5015),
and NUL reading is `-d ''` (bash convention), so no `-z` alias is needed.

## Called out (filed `read-behaviors-not-wired`)

- **`-r`** — rejected, but sash is always raw so it's a no-op; bash scripts use
  it everywhere. Recommend accepting it as an ignored no-op.
- **NUL-terminator trailing-empty** — the getopt idiom leaves a trailing `[]`
  from getopt's terminating NUL; bash `mapfile -d ''` drops it. Recommend
  terminator semantics for `-d`.
- `-n`/`-N` (nchars), `-u FD`, and the interactive flags (`-p`/`-s`/`-t`/`-e`/
  `-i`) — unimplemented; leave unless needed. `-f` is a sash invention (no IFS).

## Verification

specs `spec/wordsplit.sh` 22.21–22.24 (`-d ''` NUL array; `-d :` no
false-positive; unknown-flag rejection consistent standalone + lastpipe; 21–22
gated native,c — janet lacks custom-delimiter array reads). `cargo fmt` +
`clippy` clean. Full 3-mode sweep vs HEAD `39705d8`: **native 0 regressions, c
0, janet 0**.
