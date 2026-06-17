---
title: non-brace dynamic subscript `$a[$i]` not recognised (brace form works)
status: ready
priority: P3
kind: bug
depends: []
gates:
  - $a[$i] (non-brace) reads the same as ${a[$i]} (brace)
  - a spec test locks the non-brace dynamic read
---

# non-brace dynamic subscript read `$a[$i]`

Found 2026-06-17 during the dynamic-subscript implementation. The brace
form `${a[$i]}` / `${m[$k]}` now expands the index correctly (types.sh
33.5/33.6). The **non-brace** form `$a[$i]` does not:

```
$ sash -c 'array a=(x y z); i=2; echo $a[$i]'
[2]          # want: z  — $a expands alone, then [$i] is literal text
```

## Root cause

The lexer breaks a word on `$` (lexer.rs:1080), so `$a[$i]` tokenises as
`Dollar("a")`, `Word("[")`, `Dollar("i")`, `Word("]")`. `try_subscript_arm`
expects the post-`$a` word to be a single `[idx]` Word containing `]`;
`Word("[")` has no `]`, so subscript recognition fails and the parts are
emitted separately. Same lexer-split root cause as the write side (which
was fixed in the parser via `parse_split_subscript_index`); the non-brace
*read* path would need analogous multi-token gathering in
`try_subscript_arm`.

## Workaround

Use the brace form `${a[$i]}` — it works and is the idiomatic form.

## Proposed spec (TDD)

- `array a=(x y z); i=2; echo $a[$i]` -> `z`
- `map m=([a]=1 [b]=2); k=b; echo $m[$k]` -> `2`

Low priority: the brace form covers the documented/idiomatic cases.
