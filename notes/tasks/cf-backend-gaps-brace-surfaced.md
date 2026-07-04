---
title: "c/janet control-flow gaps surfaced by DD#68 brace specs (elif, continue, until, list-cond)"
status: ready
priority: P3
kind: bug
depends: []
gates:
  - "janet elif selects the matching arm (x=2; if..elif [[ $x == 2 ]].. -> two, not z)"
  - "janet continue skips to the next iteration (not break): for x in a b c { if b { continue }; echo } -> a,c"
  - "c/janet: until (( )) loop, && / || compound conditions, list conditions, C-style for run correctly"
  - "then brace.sh can run in all three modes (drop its native-only guard)"
---

# Pre-existing c/janet control-flow gaps (surfaced, not caused, by DD#68)

DD#68 brace-block control flow is parser-only — the brace and `then/do`
forms build the identical IR, verified: for every failing case the
KEYWORD form fails identically in that backend. brace.sh is therefore
native-only (like convert.sh). These are the pre-existing backend bugs it
surfaced (all confirmed by comparing brace vs keyword output 2026-07-04):

- **janet elif** → wrong arm. `x=2; if [[ $x == 1 ]]; then echo one; elif
  [[ $x == 2 ]]; then echo two; else echo z; fi` → `z` (want `two`), both
  forms.
- **janet continue** → acts as break. `for x in a b c; do if [[ $x == b ]];
  then continue; fi; echo $x; done` → `a` (want `a c`). Known: emit_janet
  emits `(break)` for continue ("Janet's (each) doesn't support continue").
- **janet &&/|| compound condition + else**, **janet list condition**,
  **janet C-style for**, **janet redirect-in-body** — brace tests a6b/a7/
  b4/e3 fail identically in the keyword form.
- **c until (( ))** — `until (( i >= 2 )); do …` produces no iterations.

## Fix direction

Bring the c/janet backends to native parity on these constructs (janet
`continue` is the highest-value: needs a real continue, e.g. a per-body
`(prompt :continue …)`/`(label)` or a rewrite of the `(each)` loop).
Then drop brace.sh's `native`-only guard and let it run in all modes.
