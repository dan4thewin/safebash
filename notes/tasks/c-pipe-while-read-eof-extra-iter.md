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

## Re-measured 2026-09-13 — premise obsolete, repro no longer compiles

The filed repro is rejected by *both* backends now:

```
$ for x in $(/bin/echo a b c); do attest x [[ -n $x ]]; echo "$x"; done
error [SB5020] tainted command substitution used without attest
```

The language tightened underneath the filing (the repro still says `bless`,
which predates the `attest` rename). The DD#28/#40 `for x in $(cmd)` →
`cmd | while read x` transform may no longer be reachable by this route at
all.

**This does not clear the task** — it clears the *evidence*. Either the EOF
extra-iteration still exists behind some still-legal construct, or the
construct that exposed it is gone and the task is void. Deciding which needs
a fresh repro, not a status flip. Left `ready` with the premise flagged so
the next reader does not re-run a repro that cannot compile.
