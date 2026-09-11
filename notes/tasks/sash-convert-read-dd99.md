---
title: "sash-convert emits `read` spellings the compiler now rejects"
status: done
priority: P1
kind: task
depends: []
gates:
  - "`read -r line` converts to `readline line`"
  - "a multi-target or flagged bash `read` produces a diagnostic, never silently-invalid output"
  - "spec convert.sh 57.37 asserts the converted output, not the passthrough"
---

## What

`sash-convert` did not rewrite `read` at all — it passed the bash command
through verbatim and added an `# ATTEST:` comment above it. Spec `convert.sh`
57.37 pinned exactly that, asserting the output still contained `read -r line`.

DD#99 stage 6 retires every read flag and the multi-target form, so that output
is rejected by the compiler. The converter's job is to produce something that
compiles, so this was a correctness break, not a cosmetic one.

## Two premises in the original filing were wrong

**1. "The converter has no command-rewriting path today (it only inserts
annotation lines)."** It has one. `let` → `(( ))` and `trap` → `signal N IGN`
both replace the whole command node via `Patch::replace_node`. This was an
edit, not an addition.

**2. The mapping table copied DD#99's "What is retired" table.** That table is
a **sash → sash** rename map — the "26 in-tree `read -a` uses" it cites were
old *sash* spellings that already had NUL semantics. Bash's `read -a arr`
splits **one line on IFS**; sash's `read arr` slurps **all of stdin on NUL**.
Copying it would have emitted silently-wrong programs. The mapping below is
re-derived from bash semantics.

## What landed (DG ruling 2026-09-11: scope B, trim=check, keep ATTEST)

| bash | sash | note |
|---|---|---|
| `read VAR`, `read -r VAR` | `readline VAR` | + `# CHECK:` on the trim difference |
| `IFS= read -r VAR` | `readline VAR` | exact; no CHECK, and `IFS=` is dropped |
| `mapfile -t A`, `readarray -t A` | `/usr/bin/tr "\n" "\0" \| read A` | |
| `mapfile -t A < f` | `/usr/bin/tr "\n" "\0" < f \| read A` | redirect moves onto `tr` |
| `cmd \| mapfile -t A` | `cmd \| /usr/bin/tr … \| read A` | pipeline stage |
| `read a b c` | — | `# MANUAL:` |
| `read -a arr`, `read -ra arr` | — | `# MANUAL:` — IFS vs NUL, never a guess |
| `read -p/-s/-n/-N/-t/-u/-d` | — | `# MANUAL:` naming the flag |
| bare `read` (`$REPLY`) | — | `# MANUAL:` |
| `mapfile A` (no `-t`) | — | `# MANUAL:` — cannot retain the separator |

Measured, and the reason for the CHECK: sash `readline ln` ≡ bash
`IFS= read -r ln`. Plain `read -r ln` strips leading/trailing IFS whitespace.
On `"   spaced   "` bash gives `[spaced]`, sash gives `[   spaced   ]`.

The `# ATTEST:` hint is kept and now names the **form** that matches the target
kind — `attest NAME [[ … ]]` for a `readline` scalar, `attest x in NAME [[ … ]]`
for a `read`/`mapfile` collection. Naming the wrong one sent the reader at a
diagnostic they could not satisfy.

## Also fixed: hint ordering

An `# ATTEST:` hint is placed *after* the line it refers to (`line + 1`); a
`# MANUAL:`/`# CHECK:` is placed *before* (`line`). Those are the same
insertion point, and `apply_patches` keeps intra-line hints in push order — so
a MANUAL note for line N+1 was printed above the ATTEST hint for line N and
read as belonging to the preceding command. `inject_attest_hints` now runs
before `walk_node`. Pre-existing, but this task made it common.

## Specs

`spec/convert.sh` 57.37 inverted; 57.56–57.59, 57.61–57.73 added (17 new).
57.73 is the end-to-end gate: convert → fill in the attest → compile → run.
`sash-convert/tests/convert_tests.rs` +9.

## Still out of scope (separately filed)

- `done < f` — a redirect on a compound command is SB4021
  (`converter-compound-postfix`).
- `${names[@]}` — retired bracket splat (`converter-retire-bracket-splat`),
  which also misfires a `${var@X}` MANUAL note on it.
