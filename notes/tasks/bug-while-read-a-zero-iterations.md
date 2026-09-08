---
title: "bug: `while read -a arr` runs ZERO iterations (silent no-op); `read -a` default splits on space only"
status: ready
priority: P1
kind: bug
depends: []
gates:
  - "`while read -a l; do echo ITER; done` over two lines runs the body (bash runs it twice); today it runs zero times and emits no diagnostic"
  - "`read -a` documents and implements ONE default separator consistently (today: space splits, tab and newline do not)"
  - "if `while read -a` is deliberately unsupported, it is REJECTED with an SB code, not silently skipped"
---

## Found

2026-09-08, while comparing `read`/`readline` design options.

## (a) `while read -a` is a silent no-op

```sh
array l=()
while read -a l; do
  attest e in l [[ $e =~ ^.*$ ]]
  echo "ITER"
done
echo "AFTER"
```

Input `foo\nbar\n`. sash prints only `AFTER` — the body never runs. bash prints
`ITER` twice. It compiles cleanly: no diagnostic, no SB code.

Likely cause: `is_lastpipe_read_last` explicitly excludes `-a`
(`n == "read" && !args.iter().any(|a| a == "-a")`) and `is_mapfile_last` peels
array reads off elsewhere, so the `while`-condition shape falls between the two
handlers and lowers to nothing. Either shape is fine to reject — silently
succeeding while doing nothing is not.

## (b) `read -a`'s default separator is space, not whitespace

| input | elements |
|---|---|
| `a b c` | 3 |
| `a<TAB>b` | 1 |
| `a<NL>b` | 1 |

The code and comments describe the default as a whitespace split
(`ReadLineMulti`: "split on whitespace into N vars"), and bash splits `read -a`
on IFS (space/tab/newline). sash has no IFS (DD#22), so sash is free to define
this — but it should define it *once* and say so. Today "whitespace" means
space only, and nothing pins tab/newline behavior.

## Note

Both sit inside the `read`/`mapfile` family that `switch-free-read-family`
proposes to replace. If that redesign lands, (a) and (b) may be resolved by
construction — but neither should wait on an open decision, and the redesign
needs these behaviors pinned as specs either way.
