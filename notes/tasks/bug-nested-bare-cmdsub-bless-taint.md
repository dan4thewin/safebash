---
title: "NOT A BUG: nested tainted `$( … $() … )` is correctly rejected (SB5020)"
status: done
priority: P1
kind: bug
depends: []
gates:
  - "resolved — SB5020 is the correct behavior; specs fixed + collision filed"
---

## Resolution (2026-08-29) — mischaracterized; SB5020 is correct

DG: *"the results of the nested `$()` have to be read to become an argument to
the outer `$()`, so if the nested one is tainted, it violates the read-only-in-
bless-test rule."* Correct. In `x=$(/bin/echo $(/bin/echo hi))` the inner
`$(/bin/echo hi)` is external (tainted) and its result is **read** as an
argument to the outer command; `bless x` covers only the outer assignment, not
that inner read. So **SB5020 is the right answer** — the construct is not in
the safe subset.

There is **no taint bypass and no nondeterminism**. The apparent "per-run
flip-flop" was a **chk-id collision**: `commands.sh` 14.7/14.8 (this tainted
nested cmdsub → correctly failing) and `wordsplit.sh` 14.7/14.8 (`&&`-on-
compound → passing) share ids; `wordsplit.sh` sorts later so its `passing`
overwrote `commands.sh`'s `failing`, so `spec.db` showed `native:14.7 =
passing`. The taint check rejects **deterministically** (660/660 direct + 30/30
via the spec harness).

Fixed: `commands.sh` 14.7/14.8 rewritten to **expect SB5020** and renumbered to
14.17/14.18 to de-collide. The broader `commands.sh`↔`wordsplit.sh` 14.4-14.9
collision is filed separately (`bug-commands-wordsplit-14x-chk-collision.md`).

## Original (incorrect) symptom, kept for the record

The filing wrongly assumed the construct *should* compile. See above — it must
not; the inner tainted read is a use without bless.

## Original symptom text

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
  by building HEAD (`1c2a5c4`) with those changes stashed: 14.7/14.8 fail
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
