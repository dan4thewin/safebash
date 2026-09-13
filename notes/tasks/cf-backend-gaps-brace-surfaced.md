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

### janet compound-condition catch (2026-08-19)

`if a && b` / `if a || b` in **janet** don't catch correctly: `if true &&
false` → `y` (should be `n`), `if false || false` → `y` (should be `n`).
janet emits conditions from the AST, so it bypasses the SSA
`lower_list_stmt` caught-position fix (DD#55 addendum 2026-08-19) that
fixed native+c. spec errors.sh cc.1-4 are guarded `native,c`; widen to
janet once janet's `&&`/`||` condition evaluation matches. Reproduces with
`/bin/true`/`/bin/false`.

## Re-measured 2026-09-13 — every c gate passes; only the un-gating remains

All seven constructs this task names, run in c mode and compared against
native. **Every one is byte-identical:**

| construct | c | native |
|---|---|---|
| `elif` | `two` | `two` |
| `continue` | `a c` | `a c` |
| `break` | `a` | `a` |
| `&&` condition | `both` | `both` |
| `\|\|` condition | `either` | `either` |
| list condition (`if /bin/true && /bin/true`) | `listcond` | `listcond` |
| C-style `for (( ; ; ))` | `c=0 c=1 c=2` | `c=0 c=1 c=2` |
| `until (( ))` | `i=0 i=1 i=2` | `i=0 i=1 i=2` |

The janet gates are void (DD#104).

So the code gap is gone and only the task's last gate is outstanding: **drop
brace.sh's native-only guard.** Until that happens, 27 tests are withheld
from c coverage for a defect that no longer exists — the sweep flatters the
c backend by exactly that much.
