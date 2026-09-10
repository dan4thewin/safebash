# 184 — `$@`/`$*` universally rejected (Option A)

DG: "haven't we universally rejected `$*` and `$@` now?" — no, and the
question exposed two real gaps left by the `$@` drop (journal 182):

1. **In-function `$*` silently compiled.** `check_dollar_at` (SB5045)
   matched only `@`, not `*`. `f() { echo "$*" }` built and ran, printing
   empty — a silent semantic gap (bug policy: stop-everything).
2. **Top-level `$@`/`$*` were rejected (SB5024) but mislabeled
   "function-local."** Correct for `$1`/`$#` (which *are* valid in a
   function), wrong for `$@`/`$*` — those are rejected *everywhere*. The
   message literally invited wrapping `$@` in a function, where it's also
   rejected.

A third gap surfaced during the fix: **braced `${@}`/`${*}` (and modifier
forms `${@:-x}`) never triggered SB5045 at all** — the old `pe.name == "@"`
check was dead. `scan_param_name` only accepts `[A-Za-z0-9_]`, so `${@}`
parsed to an *empty-name* `ParameterExpansion` (the `@` sigil lost). The
SB5045 doc comment claimed `${@}` was covered; it never was.

## Decision (DG: Option A)

`$@` and `$*` are **not part of the language** — one diagnostic identity
that fires **everywhere**, separate from the genuinely-function-local
`$1`/`$#`. Context-sensitive hint:

- in a function → `` `@args` (the splat), or `$(join args)` for a joined string ``
- at top level  → `` at top level use `$(argv 1)` (the user-arg splat), or `$(arg N)` for one ``

## Implementation

**`sash/src/parse/expr.rs`** — `parse_parameter_expansion`: any `${…}` whose
content *starts* with `@` or `*` now parses to `SimpleExpansion("@"/"*")`,
identical to the unbraced `$@`/`$*`. This covers bare `${@}`/`${*}` and any
modifier spelling (`${@:-x}`, `${@#x}`, `${@Q}`); a dropped modifier is
immaterial since the whole form is rejected. Array `[@]`/`[*]` forms are
unaffected (their content starts with the array name, not the sigil — they
keep their own SB4038 "retired bracket splat" diagnostic).

**`sash/src/validate.rs`**:
- `check_dollar_at` now fires **everywhere** (removed the `if in_fn` emit
  gate), threading `in_fn` through the walk to pick the hint. Matches `*` as
  well as `@` in `SimpleExpansion` and `ParameterExpansion`. Added a
  `dollar_at_arith` walk (mirrors `check_positionals_arith`) so `$@`/`$*`
  inside `$(( ))` / `(( ))` are caught. Command substitutions inherit the
  enclosing `in_fn` (was hardcoded `true`).
- `dollar_at_diag(span, sigil, in_fn, …)` builds the context-sensitive
  SB5045 message, naming the exact sigil the user wrote.
- `flag_positional` (SB5024) drops its `@`/`*` branch — those are no longer
  "function-local"; only `$1..$N` and `$#` remain.

## Result (all three backends)

| form (in-fn and top-level)                    | before            | after   |
|-----------------------------------------------|-------------------|---------|
| `$@`                                          | SB5045 / SB5024   | SB5045  |
| `$*`                                          | **compiled** / SB5024 | SB5045 |
| `${@}` `${*}` `${@:-x}`                        | **compiled**      | SB5045  |
| `$(( $@ ))`                                    | (in-fn) uncaught  | SB5045  |
| `${a[@]}` `${a[*]}` (array)                    | SB4038            | SB4038  |
| `$1` `$#` (top level)                          | SB5024            | SB5024  |
| `@args` `${args[i]}`                          | ok                | ok      |

## Verification

Full `SPEC_MODES="native c janet"` sweep, latest-run-per-test diff vs base
`12d1ffd`: 0 passing→failing, 0 gone, native gate 0 failing. 12 new passing
(4 checks × 3 modes): functions.sh 81.15 (in-fn `$*`), 81.16 (in-fn `${*}`);
errors.sh e.dd46.t6 (top-level `${@}`), e.dd46.t7 (top-level `$(( $@ ))`).
e.dd46.t1/t3 assertions updated (SB5024 → SB5045, still direct to `$(argv 1)`);
cf.dd46.1/3 comments de-staled. Lib unit tests (incl. parser) green.

DD#46, DD#75 §5, DD#81 amended (2026-08-26, Option A).
