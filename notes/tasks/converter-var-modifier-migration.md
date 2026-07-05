---
title: "sash-convert: migrate dropped ${var:OP} modifiers → native builtins/forms"
status: ready
priority: P3
kind: feature
depends: [vet-var-modifiers]
gates:
  - "`${x^^}`/`${x,,}` → `$(toupper x)`/`$(tolower x)`"
  - "`${x^}`/`${x,}` (first-char case) → toupper/tolower on the first char + concat, or MANUAL if not 1-to-1"
  - "`${x#pat}`/`${x##pat}`/`${x%pat}`/`${x%%pat}` → `$(sub …)` (or `$(basename)`/`$(dirname)` where the pattern is `.*/`/`/[^/]*$`)"
  - "`${x/pat/rep}`/`${x//pat/rep}` → `$(sub x pat rep)`/`$(gsub x pat rep)` (glob→regex where needed)"
  - "`${x:=default}` → `x=${x:-default}` (assign-default desugar)"
  - "`${x:+alt}` → `[[ -n $x ]] && …` form or a conditional; MANUAL if not 1-to-1"
  - "`${x:?msg}` → an explicit emptiness check + error; MANUAL (murky under sash's no-unset model)"
  - "ambiguous/non-1-to-1 cases fall back to a MANUAL annotation (never emit wrong code)"
---

# sash-convert: migrate the dropped `${var:OP}` modifiers

DD#65 dropped the `${var:OP}` operators from the safe subset; the sash
compiler now rejects them with **SB4034** (`is_dropped_param_operator`,
`check_rejected_param_expansion` in `parse/parser.rs`). Per the DD#65
decision (DG, 2026-07-04), **replacement/migration is sash-convert's job**,
not the compiler's — the compiler diagnostic only points at the builtin
families generically.

This task implements the mechanical bash→sash-native rewrites in
`sash-convert` so a user with legacy bash var-modifiers gets an automatic
migration to the kept forms and the string builtins.

## Dropped → replacement map

| dropped bash              | native replacement                          |
|---------------------------|---------------------------------------------|
| `${x^^}` / `${x,,}`       | `$(toupper x)` / `$(tolower x)` (builtins)  |
| `${x^}` / `${x,}`         | first-char case (toupper/tolower + concat), else MANUAL |
| `${x#p}` `${x##p}`        | `$(sub x 'p_regex' '')` or `$(basename x)` when `p`=`*/` |
| `${x%p}` `${x%%p}`        | `$(sub x 'p_regex$' '')` or `$(dirname x)` when `p`=`/*` |
| `${x/p/r}` `${x//p/r}`    | `$(sub x p r)` / `$(gsub x p r)` (glob→regex) |
| `${x:=d}`                 | `x=${x:-d}` (assign-default desugar)         |
| `${x:+a}`                 | `[[ -n $x ]] && a` / conditional, else MANUAL |
| `${x:?m}`                 | explicit emptiness check + error (MANUAL)    |

Note the kept forms are unchanged and need no rewrite: `${x:-default}`
(now a full interpolating word, `:- ≡ "…"`), `${x:off:len}` (substring),
`${x:s/re/sub/[g]}` (sed — stays `/`//` operator internally).

## Notes

- Follow the converter's existing MANUAL-annotation convention (see
  `converter-case-to-dispatch`, `dd57-convert-*`): when a form is not an
  unambiguous 1-to-1 rewrite, emit a MANUAL annotation rather than
  guessing — never emit wrong code.
- `toupper`/`tolower` are builtins; `basename`/`dirname`/`sub`/`gsub` are
  the DD#63 regex-string family (all already implemented in the compiler).
- The compiler-side reject (SB4034) is done; this task is purely the
  converter's migration support.
