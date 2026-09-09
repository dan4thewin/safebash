---
title: "bug: `match` accepts a wrong arity silently and matches nothing"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "`match S P ARR EXTRA` is a compile error, not a silent success"
  - "`match S` (no pattern) is a compile error"
  - "no `match` form reports success while filling no array"
---

## What

`match` validates neither its upper arity nor the 1-argument form. The extra
arguments are accepted, the command reports **success**, and the out-array is
left empty — so a conditional takes the *then* branch on a match that never
happened.

```sh
array g=()
array h=()
if match "a=1" "(a)" g h; then echo "leng=$(len g) lenh=$(len h)"; fi
# -> leng=0 lenh=0   ... and the branch was taken

if match "a=1" "(a)" g x y; then echo "len=$(len g)"; fi   # -> len=0, taken
match "a=1"                                                # accepted
```

Found while probing for a global/repeat flag: `match S P g -g` compiles, so a
mistaken flag is silently swallowed rather than reported.

## Why it is not caught elsewhere

`validate.rs:253` notes that a builtin "whose arity check fails falls through
to a bare external", and DD#23 then rejects an unresolvable command (SB5011 —
verified: `nosuchcmd a b` is rejected). So the intended safety net exists. It
does not fire here: these forms compile to a real `match`, not to an external,
which means the arity check is not failing — it is not being made.

The documented shape is `match S P` (boolean / value) and `match S P ARR`
(captures into the trailing array), so 4+ arguments and the 1-argument form
should both be rejected.

## Notes

Same shape as the batch fixed in commit 346cf1f: a path that accepts input and
gives an undefined answer, invisible because no spec pinned the rejection.

## Resolved 2026-09-09

`check_match_arity` (validate) enforces `match` = 2 or 3 args and `matchall`
= exactly 3, as SB4045. Specs `regex.sh` 63.ar1-63.ar4.
