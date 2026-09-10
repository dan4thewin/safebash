---
title: "read: -r as a no-op; -d as terminator not separator"
date: 2026-09-02
---

# `read -r` accepted; `read -d` uses terminator semantics

Follow-up to the read-flag unification (journal 232) and the survey it
produced (`notes/tasks/read-behaviors-not-wired.md`). DG picked the two top
recommendations:

> "so, we add -r as a no-op and we change -d to be terminator not separator"

## `-r` → accepted no-op

sash reads are **always raw** (no backslash processing), so bash's `-r`
requests behavior that is already the default. Previously `read -r x` was
rejected (SB5015 unknown flag), which taxed hand-written sash — bash scripts
use `read -r` ubiquitously. Now `ir::classify_read_flags` recognizes `-r` and
ignores it (a new arm in `classify_read_word`, before the unknown-flag
catch-all). No state; it consumes no value word.

Bundled single-char forms (`read -rd ''`) are still **not** supported — sash
bundles no flags (`-af` wouldn't work either). Standalone `read -r x` /
`read -r -d '' arr` are the supported forms. Bundling is a separate, larger
feature; left for a follow-up only if a use case appears.

## `-d` → terminator, not separator

The getopt idiom `sash-getopt … | read -a opts -d ''` produced a trailing
empty element, because `sash_split_into_array` split on the delimiter as a
**separator**: NUL-*terminated* input (getopt emits `tok\0` per token,
including the last) left a final empty field. bash's `mapfile -d ''` (and the
`find -print0` / `xargs -0` convention) treat the delimiter as a
**terminator** and drop that trailing empty.

The fix threads a `terminator: i64` argument through to the runtime split:

- **runtime** (`sash-rt/src/lib.rs` `sash_split_into_array`): at the virtual
  end of input, if `terminator != 0` and the scan is sitting exactly on a
  delimiter boundary (`start == len`, i.e. the input ended with a delimiter),
  the final empty record is elided. Interior empty records (`a::b`) are still
  kept in both modes — only the single trailing empty is dropped.
- **native codegen** (`codegen/setup.rs` signature, `codegen/dispatch.rs`
  `emit_read_array`): pass `delim.is_some() as i64`.
- **c backend** (`emit_c.rs`): extern decl + call site pass
  `i64::from(delim.is_some())`.

`ReadArray.delim` is `Option<u8>`: `None` = no `-d` (default space split),
`Some(b)` = explicit `-d b` (including `Some(0)` = NUL). So `delim.is_some()`
is exactly "an explicit `-d` was given" — terminator semantics apply only to
`-d`. The **default** whitespace/space split keeps separator semantics, which
DG scoped in ("change -d"). janet has no custom-delimiter array read, so it is
untouched.

### Behavior

```
printf "x\0y\0" | read -a arr -d ''   # arr = (x y)          — trailing empty dropped
printf "a:b:"   | read -a arr -d :    # arr = (a b)
printf "a::b:"  | read -a arr -d :    # arr = (a "" b)        — interior kept, trailing dropped
printf "x y "   | read -a arr         # arr = (x y "")        — default split UNCHANGED (separator)
sash-getopt -o ab: -- -a -b val 'has space' | read -a opts -d ''
# opts = (-a -b val -- 'has space')   — bless e in opts [[ -n $e ]] now passes
```

## Tests

`spec/wordsplit.sh` (plan 77 → 80):
- **22.25** `-r` accepted no-op (`echo hi | read -r x`), all modes.
- **22.26** `-d` NUL terminator drops the trailing empty (`printf "x\0y\0" |
  read -a arr -d ''` → 2 elements, all non-empty), native,c.
- **22.27** interior empty kept, trailing dropped (`a::b:` → `a,"",b`),
  native,c.

Full 3-mode sweep vs `0204fe9`: **native 0 regressions, c 0, janet 0**
(+3 native, +3 c, +1 janet). `cargo fmt`/`clippy` clean; complexity gate green
(`classify_read_word` cc=7 cog=6, under the 3× means).

## Files

- `sash/src/ir.rs` — `classify_read_word` `-r` arm; `ReadFlags` doc.
- `sash-rt/src/lib.rs` — `sash_split_into_array` gains `terminator`.
- `sash/src/codegen/setup.rs`, `sash/src/codegen/dispatch.rs` — native call.
- `sash/src/emit_c.rs` — c extern decl + call.
- `spec/wordsplit.sh` — 22.25-22.27.
- `notes/tasks/read-behaviors-not-wired.md` — marked done (both shipped).
