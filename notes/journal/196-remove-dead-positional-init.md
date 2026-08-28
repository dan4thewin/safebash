# 196 — remove dead positional-init cluster (DG: verify by suite + revert)

The fully-uncovered-function scan flagged `codegen/setup.rs::init_numeric_positional`
(47 lines) and `init_at_var` (29 lines). I'd initially deferred these as
"can't gate-verify since they're uncovered." DG corrected the reasoning: *remove
it, run the full suite — a green run confirms nothing exercised depends on it,
and `git checkout` restores it if not.* That plus the static case is sufficient.

## Why dead

`init_positional_params` (program entry) initializes referenced positional
vars. But:
- Top-level `$1`/`$2`/`$#`/`$@` are all validation-rejected (SB5024/SB5045).
- Function positionals resolve through the `args` array (DD#60 Model B), never
  as bare numeric/`@` VarRefs.

So `collect_positional_refs` only ever surfaces `$?`/`$$` (which the loop
skips) plus special names that fall to the DD#53 empty default. The
missing-line map confirmed it: only `init_one_positional`'s catch-all
(`init_empty_var`) executes; the numeric branch, `#` arm, `@` arm, and both
`init_numeric_positional`/`init_at_var` bodies are all 0% covered — even though
the suite has function-positional tests (types.sh 75.17).

## Change

- Deleted `init_numeric_positional` + `init_at_var`.
- `init_one_positional` collapses to a thin wrapper that gives the referenced
  name the DD#53 empty default (dropped the dead numeric/`#`/`@` dispatch and
  the now-unused `externs`/`argv`/`argc`/`exit_status` params).
- `init_positional_params` drops the now-unused argv/argc loads; the freed
  params are `_`-prefixed (signature kept stable for `compile_ssa`).

## Verification (DG's method)

Full `SPEC_MODES="native c janet"` sweep: **0 passing→failing**, identical
passing counts (native 1213, c 1088, janet 820). Positional programs still
work: `fn f { echo $1 }; f hello` → hello; `$?`, `$$` unaffected. clippy clean.
Confirmed safe.
