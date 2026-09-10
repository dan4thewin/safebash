---
title: "brace-`if` must require braces on `else`/`elif` consistently (mixed/missing braces silently mis-parse)"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "a brace-form `if` requires `{ }` on every clause (if / elif / else); a bare or missing-brace clause is a clear compile error, not a silent mis-parse"
---

# brace-`if`: `else`/`elif` must also be braced (and preceded by the closing `}`)

DG (2026-09-01): "if used with braces needs else/elsif to also have preceding
brace."

sash has two `if` forms: bash `if cond; then …; fi` and the brace form
`if cond { … }` (`parse/parser.rs`: `parse_if` @283, `parse_if_brace` @350,
`parse_brace_block` @261). The brace form does **not** consistently require
braces on the `else`/`elif` clauses, so some malformed layouts mis-parse
silently instead of erroring.

## Observed gaps

1. **Missing `}` before `else` → the `else` is swallowed.** `parse_brace_block`
   reads statements until `}`, and a bare `else` inside gets eaten (as a command
   argument), so no else clause is recognized and no error is raised:
   ```
   if [[ 1 = 2 ]] { /bin/echo a else /bin/echo b }   # compiles; `else` swallowed
   ```
   The `else` needs a **preceding** `}` (closing the if-body) — a bare `else`/
   `elif` keyword appearing inside a brace block should be a compile error.

2. **Inconsistent brace enforcement across clauses.**
   ```
   if [[ 1 = 1 ]] { … } else /bin/echo b     # → SB4014 "expected '{' block body"  (rejected, good)
   if [[ 1 = 1 ]] /bin/echo a else { … }     # → compiles  (bare if-body accepted — inconsistent)
   ```
   A braced `if` with a non-braced `else` is rejected, but a non-braced if-body
   with a braced `else` is accepted. The rule should be uniform: **once `if`
   uses the brace form, every clause (if / elif / else) is braced**, and a bare
   clause is a clear error.

## What to do

- In the brace-`if` path, require `{ }` on the `if`, every `elif`, and the
  `else` (each `else`/`elif` must follow the closing `}` of the prior block).
- Reject a bare `else`/`elif` keyword encountered inside a brace block body with
  a focused diagnostic (e.g. SB40xx "`else`/`elif` here needs a preceding `}` —
  close the block first") rather than swallowing it.
- Reject the bare (no-brace, no-`then`) if-body form so `if cond CMD` doesn't
  slip through.
- Specs in `spec/dispatch.sh` or `spec/control_flow.sh` pinning: both-braced ok;
  missing-`}`-before-else rejected; bare-else / bare-elif / bare-if-body
  rejected; the bash `if;then;fi` form still works.

## Related

- DD#68 / the brace/dispatch block form.

## Closed 2026-09-10 (backlog prune)

Re-verified 2026-09-10 — braces are required consistently and nothing
mis-parses silently:

| form | result |
|---|---|
| `if (( x )) { … } else { … }` | runs |
| `if (( x )) { … } elif (( 1 )) { … } else { … }` | runs |
| `if (( x )); then … else … fi` (classic) | runs |
| `if (( x )) { … } else` **without** a brace | **SB4014** "expected '{' block body" |
| `elif` without a brace | **SB4014** |
| mixed `then …` + `{ … }` | **SB4014** |

The gate asked that the mixed/missing cases stop mis-parsing silently; they
are clean compile errors. Closing.
