title: "Compound if/while conditions abort on a non-terminal command's failure (should catch) — plus if-body failing-command disposition"
status: done
priority: P2
kind: bug
depends: []
resolved: "DG 2026-08-19 — &&/|| condition operands caught; `if a; b` = `a; if b` (a aborts); if-body final command aborts. See DD#55 addendum + journal 153."
gates:
  - "DECISION (DG): does `if a; b` (a LIST condition) catch `a`'s failure like `if a && b` does, or does `a`'s discarded status abort like a bare command? `&&`/`||` are clearly logical (whole aggregate is the condition → catch both sides); a `;` list is ambiguous under DD#55 (only `b`'s status is the condition; `a`'s is discarded)."
  - "DECISION (DG): a failing FINAL command in an if/else BODY (`if true { false }`) — abort (DD#55 bare-command-uncaught, sash diverges from bash) or propagate the body status as the if's result (bash rc 1, no abort)? Old specs bc.e1/bc.e2 expected rc 0, which was a builtin artifact."
  - "Once decided: `if a && b`/`if a || b`/`if a; b` conditions never abort on a caught side (per decision); un-skip brace.sh bc.a6b, bc.a7"
  - "Once decided: if-body failing-command behavior pinned; un-skip/rewrite brace.sh bc.e1, bc.e2"
  - "0 native spec regressions; c/janet parity noted (all reproduce with /bin/false in all backends)"

# Compound-condition abort: a non-terminal condition command aborts instead of catching

**Relation to conditional-block sugar (2026-11-09):** this completed
DD55 rule also governs `cmd && { ... }` and `cmd || { ... }` through
their explicit `if` counterparts (DD70, journal 596). Condition handling
must not catch a bare body's failure. General-group retirement does
not retire this condition/body distinction or reopen the settled rule.

## Discovery

Surfaced 2026-07-30 while **externalizing `true`/`false`** (they were
inline non-aborting builtins; now they resolve to `/usr/bin/true|false`).
Externalizing unmasked a **pre-existing** DD#55 abort bug: it reproduces
identically with `/bin/false`, and the true/false-as-builtins case simply
never exercised the abort path (a builtin never forks/aborts). It was also
masked because real conditions almost always use `[[ ]]`/`(( ))`, which
never abort.

## The bug (condition context)

DD#55 (`notes/decisions/DD55-error-model.md`): `if cmd` / `while cmd` are
**handling positions** — the command's failure is *caught*, not aborted.
But `mark_unchecked_in_conditions` (`sash/src/transform/entry.rs`) only
unchecks the **single** status-bearing inst that *directly* terminates in
the condition's `Branch`/`Loop`. In a **compound** condition the other
sub-commands are lowered into separate blocks whose result flows through a
`Goto`→phi/merge, so they stay `checked = true` and **abort** on failure.

Verified (`/bin/false`, native):

| condition | failing cmd | sash | bash |
|---|---|---|---|
| `if a; b`      | non-final `a` | **ABORT** | catch |
| `if true; b`   | final `b`     | catch     | catch |
| `if a && b`    | RHS `b`       | **ABORT** | catch |
| `if a && b`    | LHS `a` (short-circuits) | catch | catch |
| `if a \|\| b`  | RHS `b`       | **ABORT** | catch |
| `if a \|\| b`  | LHS `a`, RHS ok | catch  | catch |

Real code that trips it: `if grep -q x f && test -n "$y"; then …` aborts
when `grep` succeeds and `test` fails, instead of taking the `else`.

## Likely fix (condition context)

Extend `mark_unchecked_in_conditions` / `uncheck_last_condition_inst` to
uncheck **every** status-bearing command that feeds a condition, not just
the terminal one before the `Branch`/`Loop`. This needs the region of
blocks that make up a condition (the `&&`/`||`/list decomposition between
the condition's entry and its terminating `Branch`/`Loop`), so the
unchecking is scoped to the condition and does not leak into consequent/
body blocks. Confirm the DG decision on `if a; b` first — if `a`'s
discarded status should abort, the list case is *not* unchecked (only the
`&&`/`||` operands are).

## Related: if-body failing final command (bc.e1 / bc.e2)

Same "externalize unmasked abort" family, but **body** context, not a
condition:

- `if true { false }` — bash rc 1 (no abort); sash **aborts**.
- `if false { true } else { false }` — else-body `false` — sash **aborts**.

Under DD#55 a bare failing command with a discarded status aborts, so the
abort is arguably *correct* and the old specs (which asserted `rc 0`) were
encoding the builtin artifact. But the bc.e1 comment ("a failing body
command is caught by the if (rc 0, no abort)") reads like a deliberate
design intent that contradicts DD#55. DG must reconcile.

## Deferred specs (skipped, brace.sh)

- `bc.a6b` — `if true && false { echo y } else { echo n }` (→ n) — `&&` RHS
- `bc.a7`  — `if false; true { echo y } else { echo n }` (→ y) — list non-final
- `bc.e1`  — `if true { false }` (rc 0) — if-body
- `bc.e2`  — `if false { true } else { false }` (rc 0) — else-body

Un-skip (and rewrite bc.e1/bc.e2 to the ratified body behavior) once the
two decisions above land.

## Backend note

All four reproduce in every backend (native/c/janet) — the abort channel
is shared front-end machinery. Fix once in the transform; re-verify c/janet.
