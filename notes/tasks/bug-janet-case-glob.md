---
title: "c/janet: case glob patterns (h*) not matched (only exact literals + bare *)"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "spec control_flow cf.c6 (`case $x in h*) …`) passes in janet (and c if also affected)"
  - "star-only globs (DD#39) in case patterns match in all backends"
---

# BUG — janet case emits glob patterns as exact literals

> **Resolved 2026-07-04 (f71c04a), dissolved by DD#66** — as this task
> predicted ("Likely mooted by DD#66"). `case` is removed; case patterns
> can no longer be globs at all (dispatch is `==` exact or `=~` regex).
> The legacy string-match glob path (`=glob`/`fnmatch_impl`) is deleted in
> every backend, so janet can no longer miscompile it. spec cf.c6 was
> migrated to the regex dispatch form and now **passes in janet** (it was
> the one test this removal flipped green).

Found 2026-07-03 while fixing `bug-janet-case-multipattern`.

## Repro

```bash
$ sash --janet -c 'x=hello; case $x in h*) echo h-prefix ;; *) echo other ;; esac' | janet
other        # WRONG — bash/native: h-prefix
```

`emit_janet.rs::emit_case_item` emits a single non-`*` pattern verbatim as
a janet `match` literal (`"h*"`), so it only matches the exact string
`h*`, never the star-glob. Native lowers case patterns through fnmatch
(`transform/control_flow.rs::emit_multi_pattern_chain`) and matches
correctly; the c backend should be checked too (`cf.c6` runs in all
modes — verify whether c passes).

## Scope

Case patterns are star-only globs (DD#39: `?`/`[]` are literal — see
cf.c7/cf.c8). So janet needs a star-only glob matcher: translate `a*b*c`
into prefix/in-order-infix/suffix checks (or shell out to a matcher).
Compose with the multi-pattern guard alternation already in place
(`_casev` binding) — a multi-glob arm `h*|w*` becomes
`(_casev (or (glob? "h*" _casev) (glob? "w*" _casev)))`.

## Relationship

Sibling of `bug-janet-case-multipattern` (multi-pattern exact match —
fixed). Same family as the `cjanet-wordsplit-39-10-22-12` regex-class
gap. Likely mooted by DD#66 (`case` → `[[ ]]` dispatch) if that lands
first, but a live janet miscompile until then.
