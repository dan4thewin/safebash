---
title: "BUG: nested bare `$( … $() … )` with bless rejected SB5020 (spec 14.7/14.8, pre-existing)"
status: pending
priority: P1
kind: bug
depends: []
gates:
  - "`x=$(/bin/echo $(/bin/echo hi)); bless x [[ -n $x ]]; echo \"$x\"` compiles and prints `hi`"
  - "spec commands.sh 14.7 / 14.8 pass in native mode"
---

## Symptom

A nested *bare* command substitution used as an argument inside an outer
command substitution is rejected even when the outer result is blessed:

```
$ sash -c 'x=$(/bin/echo $(/bin/echo hi)); bless x [[ -n $x ]]; echo "$x"'
-c:1:15: error [SB5020] tainted command substitution used without bless —
         assign it to a variable and bless before use
```

The inner `$(/bin/echo hi)` (an argument to the outer `/bin/echo`) is flagged,
though `bless x` covers the whole `x=$( … )`.

## Status / provenance

- **Pre-existing — NOT caused by the bug#3/bug#4 work (2026-08-28).** Verified
  by building HEAD (`dcda5bf`) with those changes stashed: 14.7/14.8 fail
  identically. Two independent fresh HEAD-vs-change native baseline diffs both
  showed **0 passing→failing** flips.
- **⚠ PER-RUN NONDETERMINISTIC (the real bug).** Same source, same binary:
  12/12 fail in a tight manual `sash -c` loop, yet the very same input **passed**
  in the official `run.sh` sweep minutes later. This is not commit-level — it is
  run-to-run flakiness, almost certainly **HashSet iteration order** (default
  `RandomState`, seeded per process) inside the taint walk (`check_taint`,
  validate.rs). `spec.db` has ~420 `native:14.7` rows with *both* statuses,
  consistent with per-run flipping. A taint (security) check whose verdict
  depends on a random seed is the priority to fix.
- **Also masked by an id collision**: `14.7`/`14.8`/`14.9` are shared by
  `commands.sh` and `wordsplit.sh` (global chk namespace). When one instance
  fails and its later twin passes, the authoritative latest-per-test view nets
  to `passing`, hiding the failure. Worth de-colliding these ids too.

## Next steps

1. Decide whether nested-bare-cmdsub-as-argument-under-bless *should* compile
   (bash: yes) — i.e. does `bless x` extend to the inner cmdsub that feeds the
   outer one? If yes, this is a taint-propagation bug in `check_taint`
   (validate.rs, SB5020 sites ~2680/2806/2849). If no, update 14.7/14.8 to
   expect SB5020 and note the subset restriction.
2. If the cross-commit flip-flop is compilation-order (HashSet iteration)
   dependent, make the taint walk order-independent.

## Found

While validating the bug#3/bug#4 spec additions (commands.sh) — 14.7/14.8
surfaced as failing at HEAD independent of the change under test.
