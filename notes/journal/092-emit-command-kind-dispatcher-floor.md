# Journal 092 — emit_command_kind dispatcher floor

## Goal

Second Phase 10 target. `emit_command_kind` (`codegen/dispatch.rs`,
CC 28) — inherited from j058 when the inner `match kind` was split
out of `emit` (42 → 15). This is the inner dispatcher itself; 21
arms over `BuiltinCmd`, most arms 1-line delegates already, but
several arms inline 5–10 lines of logic with their own conditionals.

## Pattern

Eight arms had inline bodies with logic the rest of the dispatcher
delegated to `emit_X` helpers:

- `NegateTestResult` — 10 lines, no decisions
- `SubshellStart` — 5 lines
- `SubshellWait` — 5 lines
- `RedirectPush` — 10 lines
- `RedirectPop` — 3 lines (`if let`)
- `TrapSignal` — 10 lines (`if len + if let`)
- `SignalFlag` — 5 lines (`if let`)
- `Noop | True | False` — 6 lines (2 conditionals)

These inline bodies broke the dispatcher's "every arm delegates"
shape and pushed CC above the audit threshold.

## Refactor

Hoisted each into a private `emit_X` method, matching the existing
peer methods (`emit_echo`, `emit_let_exit`, `emit_exit`,
`emit_table_delete`, …). Function signatures follow the local
convention: `&self, [arm-specific args], builder, module, ctx`,
with `ctx: &mut InstCtx` carrying `externs`, `vars`, `literals`,
`test_result_var`, `exit_status_var`, `redirect_stack`.

Left the 2-line `BuiltinCmd::Test` legacy-path arm inline — it has
no decisions and breaking it out for symmetry alone would be noise.

## Result

| function | before | after |
| --- | --: | --: |
| `emit_command_kind` | 28 | 22 |
| `emit_negate_test_result` (new) | — | 1 |
| `emit_subshell_start` (new) | — | 1 |
| `emit_subshell_wait` (new) | — | 1 |
| `emit_redirect_push` (new) | — | 1 |
| `emit_redirect_pop` (new) | — | 2 |
| `emit_trap_signal` (new) | — | 3 |
| `emit_signal_flag` (new) | — | 2 |
| `emit_noop_true_false` (new) | — | 3 |

The dispatcher is now structurally pure: 21 BuiltinCmd arms +
wildcard, each a single method call or top-level delegate (CC = 1
entry + 20 explicit arms + 1 wildcard = 22). Audit annotation
`// cyclo-allow:` added; pre-commit's filtered top-5 now skips it.

## Insight

This is the audit's recurring pattern at full visibility: a
dispatcher that's "mostly delegated" is structurally honest; one
that mixes delegate arms with multi-line inline arms is dishonest
about its layering. The CC drop quantifies it (28 → 22), but the
real win is the reviewer's at-a-glance check: every BuiltinCmd
variant now points to a named function.

The 8 new helpers all have CC ≤ 3 — none became their own hotspot.
This is the test for "is this a real abstraction or just code
movement?" Real abstractions name a coherent operation; if the new
helper's CC is just-below-parent's, the split was probably wrong.

## Gates

- `cargo build --release` clean
- `cargo clippy -D warnings` clean
- `cargo test --lib` 12/12
- `spec/run.sh` 291/291

## Files touched

- `sash/src/codegen/dispatch.rs` — extracted 8 helpers; dispatcher
  arms collapsed to single-line calls; `// cyclo-allow:` annotated.
- `notes/tasks/r47-abstraction-audit.md:117` — appended follow-up
  note on the j058 row.
