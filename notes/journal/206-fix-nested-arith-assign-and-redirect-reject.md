# 206 — Bug #3 (nested arith assignment) fixed + redirect-on-noncommand rejected

Two grind-surfaced bugs closed, both DG-directed ("fix #3; reject and delete").

## Bug #3 — nested / embedded arith assignment silently dropped (fork A)

`(( (a=2) ))`, `(( 1 + (a=2) ))`, `(( a=2, (b=3) ))` all used to compute a value
but **drop the store** — the variable kept its old value with no diagnostic.
Root cause: an assignment that isn't the top-level `(( … ))` mutation is lowered
by the *value* path (`arith_to_ssa`), which built an `ArithBinOp` value and never
emitted the store.

DG chose **fork A** (emit the store, match bash) — but only where the subset
already treats `(( … ))` as a mutation site, i.e. a **statement**. The consistent
rule is: *a mutation belongs in a `(( … ))` statement — never in a condition or a
`$(( … ))` value* (SUBSET.md: conditions are pure tests; `$(( ))` is read-only).
The value path is immutable (`&VersionState`, no block handle), so the store
emission lives where `&mut` is held:

- **transform** `lower_arith_test_stmt` (a bare `(( … ))` statement): when the
  expression contains a mutation, `hoist_arith_mutations` emits each embedded
  Assign/CompoundAssign/SubscriptAssign as a real `Assignment` (via the standard
  `lower_one` path, so taint/type/SSA bookkeeping match a top-level `(( a=2 ))`),
  then returns a mutation-free residual in which each assignment is a **read** of
  its now-stored target. Guarded by `arith_has_mutation`, so the common
  mutation-free case is untouched.

- **validate** rejects the forms that can't (or mustn't) be applied in place, so
  there is never a silent wrong value (gate 3):
  - a mutation in a `$(( … ))` **value** → SB4020 (`check_arith_value_mutation`
    — the value path has no side effects);
  - a mutation in a `(( … ))` / `if`/`while` **condition** → SB5007
    (`validate_arith_in_condition` now checks `arith_has_mutation`, not just
    top-level classification, so `if (( (a=2) ))` and `while (( (i=i+1) <= 3 ))`
    join `if (( x=5 ))` / `if (( x++ ))` as errors — conditions are pure tests);
  - in a `(( … ))` **statement**, an embedded `++`/`--`, an `a[i]` mutation, or a
    mutation under a short-circuit / ternary operator → SB4020
    (`check_arith_test_placement` → `arith_unhoistable_reason`).

  The pre-existing `(( x = y++ ))` reject (parser `reject_nested_mutation`) is
  unchanged.

Before/after (fresh HEAD baseline): four previously **silent-wrong** cases
(`$(( (a=2)+1 ))`, `(( 1+a++ ))`, `(( a?(b=2):0 ))`, and `if (( (a=2) ))` /
`while (( (i=i+1)<=3 ))`) are now clean errors; the statement gate cases now emit
the store. `arith_has_mutation` is a shared `pub(crate)` walker in `ir.rs`.

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

## Filed / follow-up

`bug-nested-bare-cmdsub-bless-taint.md` — filed as a P1 taint-nondeterminism
bug, then **corrected** (2026-08-29): nested tainted `$( … $() … )` is
**correctly** rejected SB5020 (DG: the inner tainted result is read as an
argument to the outer command; `bless x` covers only the outer assignment).
There is no bypass — the reject is deterministic (660/660). The "flip-flop" was
a **chk-id collision** (commands.sh 14.7/14.8 vs wordsplit.sh 14.7/14.8; the
`~420` both-status `spec.db` rows were the two colliding tests). Fixed:
commands.sh 14.7/14.8 rewritten to expect SB5020, renumbered to 14.17/14.18;
broader collision filed as `bug-commands-wordsplit-14x-chk-collision.md`.

New code: SB4020 reuse (arith mutation placement), SB4046 (redirect on
non-command). No new DD.
