---
title: "sash-convert emits `read` spellings the compiler now rejects"
status: ready
priority: P1
kind: task
depends: []
gates:
  - "`read -r line` converts to `readline line`"
  - "a multi-target or flagged bash `read` produces a diagnostic, never silently-invalid output"
  - "spec convert.sh 57.37 asserts the converted output, not the passthrough"
---

## What

`sash-convert` does not rewrite `read` at all — it passes the bash command
through verbatim and adds an `# ATTEST:` comment above it
(`sash-convert/src/convert.rs:968`). Spec `convert.sh` 57.37 pins exactly that,
asserting the output still contains `read -r line`.

DD#99 stage 6 retires every read flag and the multi-target form, so that output
is now **rejected by the compiler**. The converter's job is to produce
something that compiles, so this is a correctness break, not a cosmetic one.

## Mapping

| bash | sash |
|---|---|
| `read VAR` / `read -r VAR` | `readline VAR` |
| `read -a ARR` | `read ARR` (NUL fields) — needs `tr` upstream if the input is line- or space-separated |
| `read -d '' -a ARR` | `read ARR` |
| `read A B C` | no direct equivalent — split explicitly (`matchall`), so **diagnose** |
| `mapfile -t A` / `readarray A` | `matchall $s "[^\x0a]+" A` over the file's contents |
| `mapfile -m M` | `matchall` + `map m = @{p:DISP}` |

The single-target case is the overwhelming majority and is a clean mechanical
rewrite. The multi-target case has no faithful one-line equivalent, so the
converter should emit a diagnostic rather than guess — silently-invalid output
is the one outcome to avoid.

## Note

Not attempted during stage 6: the converter has no command-rewriting path
today (it only inserts annotation lines), so this is a real addition rather
than an edit. Filed so the gap is not mistaken for done.
