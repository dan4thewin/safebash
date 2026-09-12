---
title: "c backend doesn't implement the `signum` value intrinsic (emits a TODO stub)"
status: ready
scope: c/janet backends only (native complete)
priority: P3
kind: bug
depends: []
gates:
  - "[DONE c] `$(signum)` / `$(( (signum) ))` computes the DD#56 last-signal number on the c backend (reads the sash_signum global)"
  - "[c] spec arithmetic.sh sx.7 widened native -> native,c (janet still stubbed; deferred)"
  - "[PENDING janet] janet backend still execs/omits the intrinsic"
  - "no regression"
---

> **c DONE (2026-08-20):** `try_native_intrinsic_c` gained a `signum` arm
> that reads the `extern i64 sash_signum;` global (via `sash_itoa`). Both the
> `$(signum)` cmdsub form and the `(signum)` arith sexpr route through it.
> sx.7 widened to `native,c`. janet deferred (DG: wait on janet).


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


## Rescoped to c-only (DD#104, 2026-09-12)

The janet backend is removed. This task was filed against both
backends; the **c** half is still real and still open — it shows up
as one of the c-mode skips. The janet half is gone, not fixed.
