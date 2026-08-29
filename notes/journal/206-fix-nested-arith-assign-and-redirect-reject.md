# 206 — Bug #3 (nested arith assignment) fixed + redirect-on-noncommand rejected

Two grind-surfaced bugs closed, both DG-directed ("fix #3; reject and delete").

## Bug #3 — nested / embedded arith assignment silently dropped (fork A)

`(( (a=2) ))`, `(( 1 + (a=2) ))`, `(( a=2, (b=3) ))`, `if (( (a=2) ))`,
`x=$(( (a=2)+1 ))` all used to compute a value but **drop the store** — the
variable kept its old value with no diagnostic. Root cause: an assignment that
isn't the top-level `(( … ))` mutation is lowered by the *value* path
(`arith_to_ssa`), which built an `ArithBinOp` value and never emitted the store.

DG chose **fork A** (emit the store, match bash). The value path is immutable
(`&VersionState`, no block handle), so the fix lives where `&mut` is held:

- **transform** `lower_arith_test_stmt` (both bare `(( … ))` statements and
  `if`/`while` conditions route through it as `ArithTest`): when the expression
  contains a mutation, `hoist_arith_mutations` emits each embedded
  Assign/CompoundAssign/SubscriptAssign as a real `Assignment` (via the standard
  `lower_one` path, so taint/type/SSA bookkeeping match a top-level `(( a=2 ))`),
  then returns a mutation-free residual in which each assignment is a **read** of
  its now-stored target. Bash-exact for the assigned value; per-iteration correct
  in a `while` condition (evaluated in place each pass). Guarded by
  `arith_has_mutation`, so the common mutation-free condition is untouched.

- **validate** (SB4020) rejects the forms the hoist can't apply in place, so
  there is never a silent wrong value (gate 3): any mutation inside a `$(( … ))`
  **value** (`check_arith_value_mutation` — the value path has no side effects),
  and in a `(( … ))` statement an embedded `++`/`--`, an `a[i]` mutation, or a
  mutation under a short-circuit / ternary operator (`check_arith_test_placement`
  → `arith_unhoistable_reason`, which tracks a conditional-position flag). The
  pre-existing `(( x = y++ ))` reject (parser `reject_nested_mutation`) is
  unchanged.

Before/after (fresh HEAD baseline): three previously **silent-wrong** cases
(`$(( (a=2)+1 ))`, `(( 1+a++ ))`, `(( a?(b=2):0 ))`) are now clean SB4020;
the gate cases now emit the store. `arith_has_mutation` is a shared
`pub(crate)` walker in `ir.rs`.

**Dead code removed** (completing fork A): with validate+hoist guaranteeing no
mutation reaches the value path, `incdec_to_ssa`, `compound_assign_to_ssa`, and
the `Assign`/`CompoundAssign`/`IncDec`/`SubscriptAssign` arms of `arith_to_ssa`
are unreachable — deleted and replaced with one documented `unreachable!`.
Proven by re-running the native sweep: **0 new failures**, so no arm is hit
across the whole suite. This retires the half-wired path that was the bug.

Specs: `arithmetic.sh` 31.25-36 (9 gate cases + 3 rejects).

## Bug #4 — redirect on a non-command (reject + delete)

DG: `f(){…} >file` (a persistent per-call redirect that "falls out of the
grammar") is an ultra-rare, counter-intuitive bash corner, not in the safe
subset. A redirect may only attach to a **command**. Unified three
inconsistent behaviors into one spanned **SB4046**:

| input            | was                                   | now    |
|------------------|---------------------------------------|--------|
| `f(){…} >file`   | SB5011 "command f not found" (call)   | SB4046 |
| `int x=5 >file`  | SB4014 "expected variable name"       | SB4046 |
| `x=5 >file`      | **silently accepted, redirect dropped** | SB4046 |

Rejected at the `wrap_pipeline_part` parser chokepoint (shared by
`parse_statement` and `parse_list_operand`) via `noncommand_redirect_target`;
`parse_declaration_assignments` now stops at a redirect token so a declaration
reaches the reject instead of choking mid-parse. Compound redirects
(`while…done >f`) keep their existing SB4021, and command / bulk-read
(`read -a … <f`) redirects still work.

**Dead code deleted**: `lower_redirected_function` + its `lower_redirected_stmt`
dispatch, and the now-unreachable `function_redirects` machinery (ssa.rs
field/init, cmd_handlers push/pop). Specs `commands.sh` 14.13-16 (renumbered off
14.9 to avoid a pre-existing global-namespace collision with wordsplit.sh).

## Verification

Two independent **fresh HEAD-vs-change baselines** (stash → build HEAD → sweep →
pop → build → sweep), diffed by latest-run-per-test, for **native and c**:
**0 passing→failing** in both. All 16 new tests pass native+c. The unreachable!
removal re-verified with a third native sweep (0 new failures). Official
`run.sh` (native janet c) refreshed `spec.db`; authoritative `test_status`:
0 native failing, 0 c failing.

`cargo fmt` + `clippy` clean. Rust cargo suites remain benched — they fail
wholesale in this environment on a linker/setjmp `.note.GNU-stack` issue (all
~250 codegen/e2e/oracle/taint tests, unrelated to source); the lib.rs unit
tests that don't link a binary pass 12/12. Spec suite is the gate.

## Filed

`bug-nested-bare-cmdsub-bless-taint.md` (P1) — nested bare `$( … $() … )` with
`bless` flips pass/fail **per run** (HashSet-seed nondeterminism in the taint
walk; `spec.db` shows ~420 both-status rows for `native:14.7`). Pre-existing,
proven independent of this work. The taint-check flakiness (a security verdict
depending on a random seed) is the priority; also de-collide the shared
14.7/14.8/14.9 chk ids between commands.sh and wordsplit.sh.

New code: SB4020 reuse (arith mutation placement), SB4046 (redirect on
non-command). No new DD.
