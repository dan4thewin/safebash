---
title: "`!` precedence vs `&&`/`||` left-operand + `- ! X` → SB4019"
status: done
priority: P2
kind: bug
depends: []
resolved: "2026-08-19 (DG) — reframed via reviewability: `!` binds a single reviewer-atomic unit, so `! X && Y` = `(!X) && Y`; `!` on a pipeline → new SB4040 (escape hatch: wrap in a predicate fn); `- ! X` → SB4019 at parse time. parse_negated single-unit parse; parse_suppressed direct reject. DD#55/DD#70 addenda, spec errors.sh bng.1-6. c/janet `!` runtime parity still separate."
gates:
  - "`! X && Y` parses as `(!X) && Y` (bash semantics), not `!(X && Y)` — currently rejected SB4018"
  - "`! X || Y` parses as `(!X) || Y`, not `!(X || Y)`"
  - "`- ! X` emits SB4019 (`-` cannot suppress a boolean), not the current garbage SB4014 (`expected operand in test expression`)"
  - "`!` matches the ratified `[[ ]]` positional template on ALL rows (bare→SB4018, left-of-&&/||→accept, consequent→SB4018, if/while→negate)"
  - "0 native spec regressions; add specs pinning `! X && Y` / `! X || Y` / `- ! X`"
---

# Remaining `!`-as-boolean work (precedence + suppress)

## Context

DG ratified (2026-07-24) that `!` is a channel-(B) must-consume boolean
(DD#70) with **exactly** the `[[ ]]` positional rules. The core change
shipped in the same session as this task was filed:

- **must-consume enforcement** (`stmt_handlers.rs::check_negation_must_consume`
  + `handle_pipeline` brackets the operand with `in_condition += 1`): bare
  `! X` → SB4018, consequent `Y && ! X` → SB4018.
- **correct negation in conditions** (`entry.rs::mark_unchecked_in_conditions`
  skips the `__negate_test_result` sigil): `if ! external`, `if ! pipeline`,
  `if ! [[ ]]` all negate the aggregate and branch (no more spurious abort).

Verified green (native 0 regressions, 801 passing); c/janet backend parity
for `!` is a **separate** remaining item (see below).

## What remains — two parser-level gaps

Both surfaced by the must-consume rule and both need `parse/parser.rs`
changes, so they were deferred to DG per Decision Authority.

### 1. `!` binds too greedily across `&&`/`||`

`! X && Y` currently parses as `!(X && Y)`. Because the whole `X && Y`
becomes the negated operand and the result is bare, it now trips SB4018.
bash (and the `[[ ]]` template) treat `!` as binding only its immediate
pipeline: `(!X) && Y`.

```
sash '! /bin/false && /bin/echo x'   # REJECT[SB4018]  — want ACCEPT, runs echo
sash '! /bin/true  || /bin/echo y'   # REJECT[SB4018]  — want ACCEPT, runs echo
bash -c '! true && echo A'           # rc 1, no output   → (!true) && A
bash -c '! false || echo B'          # rc 0, no output   → (!false) || B
```

This is the **only** divergence from the `[[ ]]` template
(measured 2026-07-24):

| case                    | `[[ ]]` template | `!` now  | want   |
|-------------------------|------------------|----------|--------|
| bare                    | SB4018           | SB4018   | ✓      |
| `X && Y` (X = left)     | ACCEPT           | SB4018   | ACCEPT |
| `Y && X` (X = conseq.)  | SB4018           | SB4018   | ✓      |
| `X \|\| Y` (X = left)   | ACCEPT           | SB4018   | ACCEPT |
| `if X` / `while X`      | negate           | negate   | ✓      |

**Fix sketch:** `parse_negated` (parser.rs:906) wraps the immediately
following `parse_statement` in a negated pipeline. But `parse_statement`
→ `maybe_wrap_redirects` → `parse_trailing_list_op` (parser.rs:2011)
has *already* folded the trailing `&& Y` into a `List` before `!` gets
to wrap. Reassociate: negation must apply to the **leftmost leaf of the
left-spine** of a trailing `&&`/`||` `List`, not the whole `List`. Either
(a) descend `List.left` chain and set `negated` on the leftmost pipeline
leaf, or (b) parse the negated operand at pipeline precedence (below
`&&`/`||`) so the list op is never absorbed. Prefer (b) if it doesn't
disturb the `a | b && c` absorption quirk documented in DD#55.

Beware: `! a | b && c` currently → SB4023 (the `&&`-after-pipe reject
from DD#55). Keep that; only the *non-piped* `! X && Y` should change.

### 2. `- ! X` emits the wrong code

`parse_suppressed` (parser.rs:940) only recognizes word-like / compound
lookahead after `-`; `Tok::Bang` isn't in the set, so it falls through to
`parse_simple_or_assignment`, which eventually reports **SB4014**
("expected operand in test expression") — garbage for this input.

Per DD#70, `-` on a (B) boolean is **SB4019** ("`-` cannot suppress a
boolean"). Either add `Tok::Bang` to the `parse_suppressed` lookahead so
the suppression reaches `check_negation_must_consume` (which already emits
SB4019 when `suppressed > 0`), or reject `- !` directly at parse time with
SB4019.

```
sash '- ! /bin/false'   # REJECT[SB4014]  — want SB4019
```

## Also remaining — c/janet backend parity for `!` (lower priority)

The native path is correct. The differential backends need the negation
plumbed (they were left untouched to keep the core change gate-clean):

- **emit_c.rs**: `__negate_test_result` currently emits
  `/* TODO command: __negate_test_result */`. Implement as
  `__test_result = (__test_result == 0) ? 1 : 0;`. It also inherits the
  `mark_unchecked_in_conditions` fix (shares the SSA `checked` field).
- **emit_janet.rs**: `emit_condition_inline` has no negated-pipeline arm.
  A 1-command negated pipeline (`! external`, `! [[ ]]`) currently falls
  to `_ => "true"`. Add: `(not (zero? <do-block>))` for len > 1; for
  len == 1 negate the single command's condition-truth.

These are backend-only (native is the gate) and can land after the parser
fixes, with their own spec cross-checks under `--emit-c` / `--janet`.

## Specs to add (spec/errors.sh, 55.41+ or a new series)

- `! X && Y` → ACCEPT, runs `Y` iff `!X` succeeds (bash-diff)
- `! X || Y` → ACCEPT
- `- ! X` → SB4019
- (regression pins already covered: bare `! X`→SB4018, `Y && ! X`→SB4018,
  `if ! X`→negate, `if ! p1 | p2`→negate aggregate, `! [[ ]]` bare→SB4018)
