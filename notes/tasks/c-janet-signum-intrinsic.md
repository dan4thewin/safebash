---
title: "c/janet backends don't implement the `signum` value intrinsic (emit a TODO stub)"
status: ready
priority: P3
kind: bug
depends: []
gates:
  - "`$(signum)` / `$(( (signum) ))` computes the DD#56 last-signal number on the c and janet backends (native already does)"
  - "spec arithmetic.sh sx.7 widens from `native` to all modes"
  - "no regression"
---

## Symptom

The c backend emits a placeholder for the `signum` intrinsic instead of loading
the signal-number global:

```
$ sash --emit-c -c 'echo $(( (signum) ))'
...    /* TODO cmdsub exec: signum (Phase 4) */
```

So `$(signum)` / the sexpr `(signum)` do not actually compute on c (they yield a
default 0). It passes the native-gated spec only because DD#56 `signum` is 0
with no signal received — a coincidental match, not a real implementation.
janet has the same gap (execs/omits the intrinsic).

## Context

Found while wiring the sexpr int intrinsics (DD#58/Option C,
bug-arith-sexpr-intrinsics-zero). native computes `(signum)` directly
(`int_intrinsic_i64` → `sash_signum` global load). The other nullary int
intrinsics (argc/getpid/getppid/epochseconds/status/random) ARE implemented on
c (they pass the native,c spec `sx.1`–`sx.6`, `sx.8`); only `signum` is stubbed.

spec arithmetic.sh `sx.7` is therefore gated `in_modes native`; widen to all
modes once c/janet load the `sash_signum` global (the runtime symbol exists —
native uses `externs.signum`).
