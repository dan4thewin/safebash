---
title: "c backend: for-in-cmdsubst (pipe-while-read) does an extra empty iteration at EOF"
status: ready
priority: P3
kind: bug
depends: []
gates:
  - "`for x in $(echo a b c); do bless x [[ -n $x ]]; echo \"$x\"; done` in the c backend produces NO `bless: x failed the gate` on stderr (matches native)"
  - "the pipe-while-read loop stops at EOF without a trailing empty-`$x` iteration"
---

## Symptom

The DD#28/#40 `for x in $(cmd)` → `cmd | while read x` transform diverges
native vs c at EOF:

```
$ src='for x in $(echo a b c); do bless x [[ -n $x ]]; echo "$x"; done'
native: out=[a b c]  err=[]                              rc=0
c:      out=[a b c]  err=[bless: x failed the gate]      rc=0
```

Native reads the single line `a b c`, runs one iteration, stops at EOF. The c
backend runs one MORE iteration with an empty `x` (EOF), which fails the
`[[ -n $x ]]` bless gate and prints to stderr. stdout and rc still match, so
it's a stderr-only divergence, but it's a real read-loop EOF bug.

## Cause (hypothesis)

`emit_c`'s pipe-while-read lowering likely checks the read result after the
body, or treats a final partial/empty read as a valid iteration, rather than
breaking when `read` hits EOF (returns non-zero). Native's
`emit_cmdsubst_pipe_while_read` breaks correctly. Align the c read-loop
termination with native.

## Discovered

Coverage-grinding `emit_cmdsubst_pipe_while_read` (journal 197). spec
coverage.sh cov.61 is guarded `in_modes native` because of this divergence;
un-guard it once fixed.
