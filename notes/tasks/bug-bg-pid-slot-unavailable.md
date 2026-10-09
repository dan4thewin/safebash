---
title: "`$!` / `$(getcpid)` before any `&` is a silent empty string, where `$(status)` is SB5012"
status: next
priority: P1
kind: bug
depends: []
gates:
  - "`echo $!` with no preceding `&` is not a silent empty"
  - "`echo $(( getcpid ))` yields an int, never the empty string — `$(( … ))` producing `\"\"` is a value-model violation"
  - "native and `--emit-c` agree"
  - "specs pin the chosen behaviour in both modes"
---

# `$!` has no availability rule

Measured 2026-10-09 against `4ac831f` (so it predates the DD#125 bare-word
desugar; the bare form inherits it rather than introducing it):

```
$ echo $!                   # ""
$ echo $(getcpid)           # ""
$ echo $(( (getcpid) ))     # ""
$ echo $(( getcpid ))       # ""   (the new bare spelling, same fault)
```

`stateful_intrinsic_slot` maps `status` → the `?` slot and `getcpid` → the
`!` slot. Both are compiler-written slots that the intrinsic reads. `$?` has a
flow-sensitive availability state machine — `check_dollar_question` /
SB5012 — that rejects a read with no preceding command to have set it.
`$!` has nothing, so a read before any `&` returns the unset slot: the empty
string.

An arithmetic expression that evaluates to `""` is a value-model violation
however it is resolved.

## The fork, for DG

1. **Mirror SB5012.** A `$!`-availability state machine: `Undefined` until a
   list statement backgrounds a job. Most correct, most work, and the
   flow-sensitivity (branches, loops) is the expensive part.
2. **A sentinel.** Initialise the `!` slot to `-1` at program start, so `$!`
   is always a well-formed int and "no job yet" is a value the author can
   test. This is the shape DG chose for `wait` on ECHILD in DD#126, which
   argues for consistency.

Reversible either way, but it is a language-surface decision, so it is DG's.

## Note

DD#126 removes bare `wait`'s dependence on `$!` (bare `wait` becomes
`wait(2)`, reaping any child), so after that work `$!` is read only by an
explicit `wait $!` or `$(getcpid)`. That narrows the blast radius but does
not close the hole.
