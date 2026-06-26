---
title: "BUG — left-shift `<<` in `(( ))` is mis-lexed (heredoc), gives wrong result; `<<=` no-op"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "`int x=3; (( y = x << 2 )); echo $y` → 12 (currently 3)"
  - "`int x=12; (( x <<= 2 )); echo $x` → 48 (currently no-op, 12)"
  - "native + c + janet"
---

# BUG — `<<` / `<<=` in arithmetic mis-lexed via heredoc

Found 2026-06-26 alongside `bug-compound-assign-non-plus`. Pre-existing
(reproduces with that fix stashed).

## Repro

```bash
$ printf 'int x=3\n(( y = x << 2 ))\necho $y\n' | sash …   # → 3, want 12
$ printf 'int x=12\n(( x <<= 2 ))\necho $x\n' | sash …      # → 12 (no-op), want 48
```

`>>` and `>>=` work correctly (12 >> 2 → 3; `x >>= 2` applies). Only the
left-shift forms are wrong.

## Cause (suspected)

The shell lexer tokenizes `<<` as a **heredoc** operator (it has no arith
context), so the `(( … << … ))` content is collected wrong by
`collect_arith_content` / `step_arith_collect` — the `<<` does not survive
as a shift operator the way `>>` (append-redirect) does. The sibling fix
`bug-compound-assign-non-plus` handled the `<op>=` split in
`needs_separator_space`, but `<<`/`<<=` are a heredoc-vs-shift
tokenization problem one layer up.

## Fix direction

Teach the arith content collector (or the scanner, in `((`-depth context)
that `<<` is a left-shift, not a heredoc — mirror whatever makes `>>`
work. Add spec cases to `spec/arithmetic.sh` (31.1 shift family + a `<<=`
compound-assign) once fixed.

## Note

`bug-compound-assign-non-plus` (the `-=`/`*=`/`/=`/`%=`/`&=`/`|=`/`^=`
no-op) is **fixed** (commit 2026-06-26); `<<=` is explicitly the one
compound-assign left failing, tracked here.
