# 133 — `!` as a channel-(B) must-consume boolean (DD#70)

DG ratified (2026-07-24): `!` (negation) is a channel-(B) must-consume
boolean per DD#70 — it must have **exactly** the `[[ … ]]` positional
rules for any operand (external / pipeline / `[[ ]]` / match). This entry
covers the native core, shipped this session; the parser-precedence tail
was deferred and filed.

## Two pre-existing `!` bugs (found probing DD#55 edge cases)

1. **`! external` / `! pipeline` in a condition aborted** instead of
   negating. `if ! /bin/false; then …` took *no* branch. Root cause:
   `NegateTestResult` is a `SsaInst::Command` sigil (`__negate_test_result`)
   pushed *after* the operand. `mark_unchecked_in_conditions`
   (`transform/entry.rs`) scans a Branch block's insts in `.rev()` and
   unchecks the *first* Command/Pipeline it finds, then breaks — so it hit
   the sigil, unchecked *that* (a no-op), and never reached the real
   operand. The operand stayed `checked` → runtime abort-if-nonzero fired
   before the negation could run.

2. **`! external` bare silently aborted** (`! /bin/false; echo done`)
   rather than being a compile error. `!` produces a boolean that nobody
   consumed — exactly the (B) must-consume violation, but the enforcement
   only keyed on `match`/`[[ ]]`, never on `!`.

Confirmed pre-existing via a HEAD worktree (9af7ce2) before touching
anything (bug policy: investigate, don't route around).

## The rule (mirror `[[ ]]`)

`!` gets the ratified `[[ ]]` positional template verbatim:

| position                | result   |
|-------------------------|----------|
| bare                    | SB4018   |
| left of `&&` / `||`     | accept   |
| consequent (`Y && ! X`) | SB4018   |
| `if` / `while` / `until`| negate   |
| `- ! X`                 | SB4019   |

## Native implementation (shipped)

- **`transform/stmt_handlers.rs`** — `handle_pipeline` now wraps a negated
  pipeline: call `check_negation_must_consume` (SB4018 when
  `in_condition == 0 && suppressed == 0`; SB4019 when `suppressed > 0`),
  then lower the operand with `in_condition += 1` so an inner `[[ ]]`/match
  doesn't *also* fire — the `!` is its consumer — then push the
  `NegateTestResult` sigil. Extracted `lower_pipeline_body` +
  `dispatch_multistage_pipeline` to keep every touched fn under the
  cognitive gate (handle_pipeline dropped 11 → 1).
- **`transform/entry.rs`** — `mark_unchecked_in_conditions` now
  `continue`s past the `__negate_test_result` sigil so the *real* operand
  is the one marked unchecked. That alone fixes bug (1): the operand no
  longer aborts, the aggregate flows into the negate, the branch is taken.

Verified (native): bare `! false`→SB4018; `Y && ! false`→SB4018;
`if ! false`→NEG (no abort); `if ! false | true`→NEG (negated aggregate);
`if ! [[ 1==2 ]]`→NEG; `! false || :`… see tail. **0 native spec
regressions** (801 passing); c/janet unaffected by this change (their
`!` specs are all already-SB4025 dispatch/`x=foo` forms).

## Deferred (filed: `notes/tasks/bang-precedence-and-suppress.md`)

Two parser-level gaps that the must-consume rule *surfaced* but that need
`parse/parser.rs` changes (design-sensitive → DG):

- **`! X && Y` binds too greedily** → parses `!(X && Y)`, now bare → SB4018.
  bash / the `[[ ]]` template want `(!X) && Y` (accept). Same for `||`.
  This is the *only* row where `!` still diverges from the template.
- **`- ! X` emits SB4014** ("expected operand in test expression") because
  `parse_suppressed` doesn't recognize `Tok::Bang` after `-`; per DD#70 it
  should be SB4019.

Also filed there: c/janet backend parity for `!`
(`emit_c` `__negate_test_result` is a TODO; `emit_janet`
`emit_condition_inline` lacks a negated-pipeline arm) — backend-only,
native is the gate.
