title: Collapse the six out-target predicates in validate.rs onto one function
status: open
priority: P4
kind: hardening
source: audit-string-keyed-builtin-dispatch (re-filed 2026-10-14)
depends: []
gates:
  - one shared predicate answering "which argument of this command is the out-target?"
  - its doc states the question and names the DD for every name in the set
  - the dead `mapfile` alternatives are branch-probed before deletion
  - sweep green with the total predicted in advance; mk test green

# Collapse the six out-target predicates in validate.rs

Re-filed from `audit-string-keyed-builtin-dispatch`, which proposed a 57-site
sweep of string-keyed builtin dispatch. Measured, most of those sites are
"is this *this specific* builtin?" with one name on each side — routing them
through `BuiltinCmd::from_name` would swap a string compare for an enum compare
and change nothing. **This is the part of that task with actual value.**

## The shape

Six functions in `validate.rs` answer the same question — *which argument of
this command is the out-target?* — from six hand-written name lists:

| line | function | names |
|---|---|---|
| 5301 | `command_writes_args` | read/readline, match/matchall, argv, keys |
| 6847 | `taint_command` | read/readline/**mapfile**/argv |
| 7635 | `collect_read_targets` | read/readline, matchall/match, argv |
| 8572 | `insert_auto_collection_target` | read (**no readline**), match/matchall, argv, keys |
| 9283 | `bulk_fill_targets` | read/readline/**mapfile**/argv |
| 9556 | `fill_target_spec` | argv, keys, match/matchall |

Related, same keying: `taint_stmt_no_read_taint`:5927, the list at :3709,
`retired_bulk_read_diag`:10962, `BuiltinCmd::from_name` at `ssa.rs:204`.

## This is NOT a bug — checked 2026-10-13

The lists disagree, which is the shape behind six past bugs in this codebase
(see the "statement wrappers" and "shared walkers" findings), so it was checked
first under the zero-tolerance policy before being filed as cleanup. Both
divergences are correct:

- **`readline` absent from `insert_auto_collection_target` is by design.**
  DD#99: "`readline` binds a scalar. One record is one value." It never
  auto-creates a collection, so it must not appear there.
- **`mapfile` is vestigial.** DD#99 stage 6 retired it; `retired_bulk_read_diag`
  raises SB5060 for `mapfile`/`readarray`, so the name cannot survive to reach
  these predicates.

## Why it is still worth doing

Six copies of one question is six chances for a fix to land on five — exactly
what `types::var_slot` resolved for the codegen slot lookup (journal 506), and
what `walk_arith_expansions` and `emit_exec_static_argv` cost us before that.
Today the divergences happen to be right; nothing *makes* them right, and
nothing tells the next person which question each list is answering.

A collapse also makes the `readline` exception visible as an exception rather
than as an omission — the current code cannot distinguish "scalar-binding, so
deliberately absent" from "someone forgot".

## The `mapfile` claim is a hypothesis, not a result

Line coverage cannot see *which* alternative of a `matches!` fired, and the
lines are warm because every command evaluates them. Before deleting the
`mapfile` arms, branch-probe them (`mk cov-probe-build`, then
`contrib/cov-probe.sh` aimed at the **condition**). Per AGENTS.md: a negative
probe result needs a positive control — run something that must reach the arm
first and confirm it lands.

## Out of scope

- The other ~50 string-keyed sites. They are the superseded task's category 2
  and `from_name` cannot improve them.
- `sash-convert/src/convert.rs` (11 sites) — a different crate against stock
  tree-sitter-bash, on bash input rather than sash IR.
