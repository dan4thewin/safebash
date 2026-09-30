---
title: "bug: `for v in $(getcwd)` compiles, then aborts the program with exit 127"
status: done
priority: P1
kind: bug
depends: []
gates:
  - "`for v in $(getcwd) { … }` is rejected at compile time (SB5009), not at runtime"
  - "all eight scalar value intrinsics behave identically"
  - "the same holds in the C backend, not only native"
  - "a for-in source that is a single scalar is SB5009 whatever spelling carries it, quoted or not"
  - "`for v in $(echo a b c)`, `$(argv)`, `$(keys A)`, `@g`, globs and literal lists still work"
---

## What

A scalar **value intrinsic** used as a for-in source passed every compile-time
check and then killed the compiled program:

```sh
for v in $(getcwd) {
  echo "$v"
}
```

```
sash: bug: bare command passed to sash_exec (expected absolute path)
$ echo $?
127
```

Uniform across all eight scalar intrinsics (`getcwd`, `getpid`, `getppid`,
`status`, `random`, `argc`, `signum`, `epochseconds`). `getenv` is the same
shape but was masked by SB5020 — confirmed with `--no-taint`.

Not a native-backend quirk. `--emit-c` emits the bad call literally:

```c
const char* __t1_argv[] = { "getcwd", NULL };
sash_exec(...);
```

## Why

Two rules that were each locally correct disagreed about the same name.

`transform/control_flow.rs::try_dispatch_cmdsubst` rewrote **any** `$( )`
for-in source into a `while read`-style pipeline, with no check on what was
inside. `resolve.rs::INTRINSICS` exempts intrinsic names from DD#23 absolute
path resolution — correctly, because an intrinsic is not an external command
and has no path. But the exemption assumed a downstream intrinsic lowering
that this one path never provided, so the name survived as a bare argv[0] all
the way to `sash_exec`.

The runtime guard is what made it loud rather than silent. It was doing its
job.

DD#111 documents the identical failure for `$(echo hi; status)` — the
exemption is wider than the rewrite that justifies it. The for-in source was
the second instance of that class, unfixed.

## Fix

DG's call, on being shown the bug: *"probably any for loop on a single
scalar, quoted or not, should be an error"*.

So the rule is not "reject intrinsics here" but the general one: **a for-in
source must denote a collection.** A single scalar could only ever run one
iteration, so it is SB5009 whatever spelling carries it.

`validate.rs::check_for_word` now delegates to `for_source_scalar`, a
classifier **total over `ExprKind`** carrying the `do not add a wildcard`
marker. That totality is deliberate: the bug *was* a guard keyed on one
variant, and `contrib/totality-check.py` now enforces that this one cannot
regress the same way.

Three points the classifier has to get right:

- **Quoting is not the discriminator.** `"$v"` is the same scalar as `$v`.
  This closed the lone inconsistency in the family: `ws.q1`–`ws.q3` already
  rejected quoted *collections*, but quoting a *scalar* had been granting it
  iteration.
- **A literal word is not a scalar in this sense.** `for v in a b` is a list
  and `for v in a` its degenerate case; both stay legal.
- **`$( )` splits on what is inside it.** A command — external or a
  `BuiltinCmd` like `echo` — is a line stream and stays legal (DD#28/#40). A
  value intrinsic is a scalar and is SB5009. `argv` and `keys` are the two
  collection intrinsics; `resolve.rs::is_scalar_value_intrinsic` is derived
  from `INTRINSICS` minus those two, so a name added to the exemption list
  cannot silently acquire the wrong shape.

`$(<file)` is deliberately still a stream — see `dollar-lt-file-forks`.

## Fallout

Now SB5009, previously accepted: `for v in "$s"`, `"$v$w"`, `${v:-d}`,
`${a[0]}`, `$(( … ))`, `pre$s*post`.

`for v in $(getenv HOME)` reports SB5009 rather than SB5020 — the source-shape
rule fires before taint. No spec pinned the old code.

Four existing specs asserted the old acceptance and were retargeted
(`cov.173`, `cov.226`–`cov.228`). `cov.173` was not predicted: it turned out
`try_dispatch_glob` accepts only a bare `Literal` containing `*` and never a
`Concat`, so `pre$s*post` really is one scalar word. The rule was right and
the spec was wrong.

## Tests

`spec/control_flow.sh` `fs.1`–`fs.9`. `fs.1` is this bug's guard; `fs.8` and
`fs.9` guard against over-reach (the `$(echo …)` stream and the literal list).

## Found by

Coverage triage, not by a report. The cold-line queue pointed at
`try_nullary_intrinsic`'s `getcwd`/`getpid` arms; widening the probe set
across syntactic positions to find out why they were cold is what hit the
for-in case.
