# 185 — taint laundering bypass: reads are use-sites in every position

DG (demo): `./sash -c 'argv 1 a; echo $(join a)' && ./a.out a b` — printed
the args. That surfaced a **taint-laundering bypass class**: a command
substitution that *reads* already-tainted data was only checked as a
use-site in **argument position**. In every other value position it silently
compiled and leaked the tainted data at runtime.

## The bug (all runtime-confirmed leaks, `argv 1 a` snapshot)

| position | example | before |
|---|---|---|
| argument | `echo $(join @a)` | ✅ SB5020 |
| for-word | `for x in $(join @a)` | ✅ SB5020 |
| scalar-assign RHS | `x=$(join @a)` | ❌ leaked "a b" |
| subscript-assign | `b[0]=$(join @a)` | ❌ leaked |
| array-literal elem | `array c=($(join @a))` | ❌ leaked |
| `[[ ]]` operand | `[[ $(join @a) = x ]]` | ❌ leaked |
| redirect target | `echo z > $(join @a)` | ❌ leaked |
| nested cmdsub | `s=$(echo $(join @a))` | ❌ leaked |

Two more facets found while fixing:
- **bare-name `join a`** (vs `@a`) was not a reader use-site even in argument
  position — `check_collection_reader_taint` special-cased only `keys`. DD#24
  lists `join` explicitly as a tainted-element read.
- **`[[ ]]` test operands and redirect targets were entirely un-taint-checked**
  — even a *direct* tainted scalar leaked: `[[ $x = y ]]`, `echo > $x`.

## Root cause (answering DG's "why aren't they `_tmp=$(…)`?")

Two compounding gaps:
1. **CST pass** — cmdsub inner-read checking lived only in the
   argument-position walker (`check_cmdsub_use_taint`, also wired into
   for-words). The other value-position walkers (`reject_scalar_rhs_read`,
   `reject_tainted_into_array`, `taint_redirected`) treated a value-position
   `$( )` as an opaque "assignment-from-source (allowed)" and skipped its
   inner reads. `check_expr_taint` had no `Test` arm, and `taint_redirected`
   never looked at redirect *targets*.
2. **SSA backstop** — `cmd_subst_taint` classifies join/keys/echo as
   unconditionally Clean by command name (the lone transform value that
   discards operand taint; every other propagates via `any_tainted`). So the
   defense-in-depth layer couldn't catch it either.

So the "`_tmp=$(…)`" model was never realized: the implicit temp never
inherited the operand's taint in either pass.

## Fix (CST — authoritative, this commit)

Made the read-check position-uniform — one shared machinery applied wherever
a `$( )` sits (DG's implicit-temp model). In `validate.rs`:

- Extracted `check_cmdsub_inner_reads` (collection readers + arg reads) from
  `check_cmdsub_use_taint`. Argument/for-word position = that + the bare
  scalar-source rejection; value positions (assignment RHS, array element)
  reuse the inner-read half only (a bare *source* is sanctioned there).
- `reject_scalar_rhs_read`: the `CommandSubstitution` arm now checks inner
  reads (was `_ => {}`).
- `reject_tainted_into_array`: added `check_value_cmdsub_reads` (walks
  concat/quote/array to any nested `$( )`).
- `check_expr_taint`: added a `Test` arm → `check_test_taint` recurses into
  `[[ ]]` operands (each a read position).
- `taint_redirected`: checks each redirect `target` expr.
- `check_collection_reader_taint`: now covers `join` as well as `keys`
  (both expose element data; `len` stays exempt — cardinality).

## SSA defense-in-depth — deferred

`cmd_subst_taint` propagation + `check_inst_uses` Test/Redirect coverage filed
as `ssa-taint-cmdsub-operand-propagation` (P2). It cannot fire on current
programs (all CST-cleared) so it is not a live leak; it hardens the backstop
against a future CST regression in this class. SSA taint errors are ICE-style,
so CST must remain the diagnostic authority.

## Verification

Every position now emits a clean SB5020 (splat, bare-name, and direct-var
forms); sanctioned source (`x=$(getenv X)`), blessed, and clean-array cases
still compile/run. Full `SPEC_MODES="native c janet"` sweep, diff vs base
`fe6d30e`: 0 passing→failing, 0 gone, native gate 0. 29 new passing
(taint.sh 60.37–60.46 × modes; 60.46 janet-skipped). DD#24 amended.

## Gap-fill (regression matrix, taint.sh 60.47–60.55)

DG: "fill gaps — protect against regressions." Extended the coverage across
read-form × source × position so a regression in any single code path is
caught:

- 60.47 `echo $t` (tainted **scalar** through a cmdsub) in assignment RHS
- 60.48 **mapfile**-sourced (not argv) tainted array joined in assignment RHS
- 60.49 `keys` of a tainted map in an **array-literal** element
- 60.50 bare-name `join a` reader reached through a `[[ ]]` operand
- 60.51 tainted **subscript** `${a[0]}` (value-flow) in a `[[ ]]` operand
- 60.52 bare-name `join a` reader as a **redirect target**
- 60.53 `echo $t` (tainted scalar) through a cmdsub in a `[[ ]]` operand
- 60.54 control (+): a **clean** `$(echo …)` capture still runs (native,c)
- 60.55 control (+): a **clean** var in a `[[ ]]` operand still runs

The two positives guard the newly-checked positions (assignment-RHS cmdsub
read, Test operand) against over-rejecting clean values. All pass
native/c/janet (60.46/60.54 janet-skipped — `_args`/capture gap). 0
regressions; native gate 0.
