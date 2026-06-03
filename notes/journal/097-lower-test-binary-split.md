# Journal 097 — lower_test_binary op-per-helper split

## Goal

Seventh Phase 10 target. `lower_test_binary`
(`codegen/tests.rs:290`, CC 22) — C-verdict from audit row 139.
Recipe: "Op-keyed match with `lower_expr_value` repeated. Extract
`lower_pair(...)`."

## Pattern

The function held a 9-arm match dispatching on Bash test-op strings
(`=`, `==`, `=~`, `!=`, `=glob`, `>`/`<`, `-eq|-ne|...`, `-nt`,
`-ot`, `-ef`). Three sources of CC bloat:

1. **Duplicated 2-value lowering** — every string-ish arm called
   `lower_expr_value(left, …)` followed by `lower_expr_value(right,
   …)`, 16 lines of boilerplate per arm.
2. **Triplicated `is_quoted` test** — `=`/`==`, `=~`, `!=` all
   probed `right.kind` for DoubleQuoted | SingleQuoted to choose
   string_eq vs the unquoted backend (fnmatch / regex_match).
3. **Inner 8-arm numeric match** — the `-eq | -ne | -lt | …` arm
   contained its own op → IntCC mapping.

Total: 9 outer arms + 3 inner `if is_quoted` + 7 inner numeric arms
≈ CC 22.

## Refactor

Three shared helpers + 9 per-op emit helpers:

| helper | CC | role |
| --- | --: | --- |
| `lower_pair` | 1 | 8-arg loader returning 4-tuple `(l_ptr, l_len, r_ptr, r_len)` — 8 callers |
| `right_is_quoted` | 2 | `matches!(right.kind, DoubleQuoted \| SingleQuoted)` — 3 callers |
| `numeric_cc` | 8 | pure op-name → IntCC table; cyclo-allow'd (arms == CC) |
| `emit_eq` | 2 | quoted: string_eq, unquoted: fnmatch |
| `emit_glob` | 1 | case-pattern fnmatch with arg swap |
| `emit_regex` | 2 | quoted: string_eq, unquoted: regex_match |
| `emit_ne` | 2 | inverse of emit_eq via `isub(1, …)` |
| `emit_lex_cmp` | 2 | strcmp + icmp on `>` / `<` |
| `emit_numeric_cmp` | 1 | atoi + icmp + numeric_cc lookup |
| `emit_file_newer` | 1 | `-nt` → file_newer |
| `emit_file_older` | 1 | `-ot` → file_newer with arg swap |
| `emit_file_same` | 1 | `-ef` → file_same_dev |

`lower_test_binary` is now a 9-arm dispatcher delegating to the
helpers. CC 11 (= 9 arms + wildcard + small overhead). No
cyclo-allow needed — under the audit band.

The 8-arm numeric subtable is captured by `numeric_cc(op) -> IntCC`,
which keeps the table inspection-friendly as a pure pattern match
and is annotated cyclo-allow as a dispatcher floor.

## Result

| function | before | after |
| --- | --: | --: |
| `lower_test_binary` | 22 | 11 |

All emit_* helpers are tiny. `numeric_cc` CC 8 is intrinsic.
`right_is_quoted` documents the rule.

## Insight

The audit's recipe ("Extract `lower_pair`") was the right starting
point but not sufficient on its own — `lower_pair` saves lines, not
branches. The real CC pump was the 9-arm match with three `is_quoted`
branches and a 7-arm inner match. Splitting per-op puts each rule in
its own named function and shrinks the dispatcher to its decision
shape.

The naming exercise produced `right_is_quoted` — a one-line predicate
that documents a real Bash semantic rule: in `[[ x == y ]]`, whether
`y` is quoted determines whether `==` means exact match or glob
match. Naming it pulls that rule out of the noise of three nested
match-arm bodies.

`numeric_cc` is the cleanest piece of the refactor. Eight rows
mapping op-name → IntCC variant. Pure table. The cyclo-allow
annotation rests on the same dispatcher-floor rule we've been
applying all of Phase 10.

## Followup noticed (not done)

`file_test_op_code` (codegen/tests.rs, CC 20) — another op-keyed
match in this same file. Not on the active queue but a likely Phase
10 candidate. Filed mental note; no task added yet (no audit row
exists; would need a fresh verdict).

## Gates

- `cargo build --release` clean
- `cargo clippy -D warnings` clean
- `cargo test --lib` 12/12
- `spec/run.sh` 291/291

## Files touched

- `sash/src/codegen/tests.rs` — 12 helpers added; dispatcher
  rewritten.
- `notes/tasks/r47-abstraction-audit.md:139` — annotated.
