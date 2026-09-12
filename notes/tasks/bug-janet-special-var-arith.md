---
title: "bug: janet backend can't use `$$` (pid) as an arithmetic operand — emits unknown symbol `$`"
status: deferred
deferred-by: DD104
priority: P3
kind: bug
depends: []
gates:
  - "`echo $(( $$ > 0 ))` compiles + runs on the janet backend (currently a janet compile error)"
  - "parity with native/c, which compute the pid; specs 31.20-24 can widen from `native,c` to all modes"
  - "no janet regression"
---

## Symptom

On the janet backend, `$$` (pid) as an arithmetic operand emits a reference to
a bare janet symbol `$`, which janet cannot resolve:

```
$ sash --janet -c 'echo $(( $$ > 0 ))'
...
(print (string (if (> (_num $) 0) 1 0)))
$ janet …
compile error: unknown symbol $
```

Native and c compute the pid correctly (native fixed via `lower_value_as_int`;
c emits `sash_getpid()`). The spec tests for special vars as arithmetic
operands (arithmetic.sh 31.20–24) are therefore guarded `in_modes native,c`;
once janet computes `$$`/`$?` in arith they can widen to all modes.

## Root cause (to confirm)

emit_janet lowers an arithmetic `DollarVar("$")` to a bare janet symbol instead
of a `(sash-getpid)` call (and `$?` likely to `__last_exit`, which may already
work — verify). The janet arith lowering needs the same computed-special
handling the native/c paths have.

## Scope

Part of the janet backend-parity effort (see janet-join-intrinsic-gaps,
cjanet-function-stdout-capture). Native is the gate; this is a janet-only gap.

## Also (word context, found 2026-08-14 via misc.sh spv.1-3)

Beyond arithmetic, janet has two word-context runtime gaps for the specials:

- **getpid-class intrinsics are execed as external commands.** `q=$(getpid)`
  on the janet backend emits `(os/spawn "getpid" ...)` → `error: ("getpid"):
  No such file or directory`. janet needs getpid/getcpid/getppid/getcwd/
  epochseconds/random as in-process intrinsics (native/c have them). (spv.4
  `$(getcpid)` happens to pass only because both it and `$!` are empty.)
- **`$?`/`$(status)` mis-tracked after arithmetic.** `(( 0 )); echo $?` prints
  `0` on janet (native/c: `1` — arith-false sets status 1). janet's arith
  statement doesn't set `__last_exit` from the truthiness of the result.

specs misc.sh spv.1–3 are therefore guarded `in_modes native,c`; widen to all
modes once janet implements the intrinsics + arith-status. Part of the janet
backend-parity effort.


## Deferred to post-beta (DD#104, 2026-09-12)

The in-tree janet backend is removed; `--emit-ir` replaces it with an
inert IR capture that an out-of-tree oracle can consume. This gap is
**not fixed** — it is recorded here as part of the diagnosis a restart
should read. The code it describes lives at tag `janet-babel`.
