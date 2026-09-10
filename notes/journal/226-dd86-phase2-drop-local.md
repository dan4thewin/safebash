# 226 — DD#86 Phase 2: drop `local` (keyword + `local fn`), structural scope

DG: "let's finish the local-related work." DD#86 (variable scope: no
`local`, no shadowing, structural scope) is ratified; Phase 1 (`str`)
shipped. This is Phase 2 — removing the `local` keyword and all its
machinery. DG confirmed: **drop `local fn` entirely** (retire DD#18
block-scoped functions), include Phase 4 later, proceed incrementally.

Folded the `local fn` retirement into this phase (it shared every piece
of machinery with `local` vars — `__local__`, `local_fn_scope`,
`DeclKeyword::Local`, `parse_function(is_local)`), so Phase 5 is now just
converter + docs.

## What structural scope already was

Probing the pre-Phase-2 compiler showed the runtime model was mostly
already structural:

- A bare `x=v` in a function **writes a top-level global** frictionlessly.
- A new function-only name is **invisible at top level** (SB5031) — its
  isolation comes from SSA versioning + the DD#62 typo check, *not* from
  the `local` save/restore machinery.

`local` only added a **shadow**: `save_caller_locals` snapshotted a
caller global, let the body clobber it, and restored it after. DD#86
abolishes shadowing, so that machinery is pure deletion.

## Removed

- **Lexer/parser**: `Tok::Local` / `LogosTok::Local` + the `"local"`
  keyword map; `parse_local`; the `is_local` parameter threaded through
  `parse_function`/`finish_function_def`; the `__local__` name-mangling.
- **IR**: `DeclKeyword::Local`; `Function::is_local`.
- **Transform**: `collect_local_vars`, `save_caller_locals`,
  `restore_caller_state`, the capture-path `restore_saved_locals`, and
  the `resolve_function_name` local-fn lookup (now the bare name);
  `VersionState::local_fn_scope`; the `__local__` scope save/restore in
  `lower_function_stmt`.
- **validate/resolve**: the `__local__` prefix stripping (functions are
  registered under their bare name only).
- **emit_janet_map**: the `DeclKeyword::Local` arm.

`local x=5` now lexes `local` as an ordinary word → an unknown command
(SB5011); the converter will drop it (Phase 5). `local fn f {…}` is the
same. A nested `fn` (no `local`) is an ordinary top-level function,
callable from anywhere — the DD#86 "all functions top-level" model.

## Latent bug surfaced + fixed: function-local `const`

`local` was masking a real bug. A function-body `const int x=5` sets
`DeclKeyword::Readonly` → `readonly_vars.insert(x)`. A function body is
lowered **twice** (a throwaway template at the definition + once per
inlined call), so the second lowering saw `x` already readonly and the
`define()` backstop wrongly fired SB4044. `local const int` never hit
this because its keyword was `Local`, so it never populated
`readonly_vars` at all (meaning `local const` wasn't even enforced —
a second latent gap, now moot).

Fix: `enter_fn_body_scope`/`exit_fn_body_scope` snapshot & restore
`readonly_vars` around every function-body lowering (template, void
inline, int-func inline, `$( )` capture, pipeline stage). A function's
`const` is now a proper function-local: its readonly-ness doesn't leak,
so re-lowering the same declaration (or a second call) is clean, while a
top-level `const` stays in the restored set so global-const enforcement
(and the within-function `const x; x=…` reassignment error) is preserved.
Verified: call-once, call-twice, global-const reassign, fn-writes-global-
const, within-fn reassign all behave correctly.

## Specs

The 7 `local`-using specs were rewritten (native gate stayed the sole
regression check):

- `functions.sh` 18.1–18.4 (were `local fn`) → now document the removal:
  `local`/`local fn` → SB5011; a nested `fn` is a global callable from
  outside; a nested `fn` is usable within its definer.
- `types.sh` 35.1 `local const int` → `const int` in a fn.
- `arithmetic.sh` ivc.8 `local int array` → `int array` in a fn.
- `errors.sh` 55.7 `local x=$(cmd)` → `x=$(cmd)` (now reads `x` via bless
  so it isn't a dead variable when Phase 4 lands).

## Verification

`cargo fmt`/`clippy` clean; lib unit tests pass (parser/lexer). Full
3-mode sweep vs HEAD `688616a`: **native 0 regressions, c 0 regressions**,
janet +2 (the old `local fn` tests failed under janet; the rewrites
pass). No new passing on native/c because the 7 specs are rewrites of
already-passing tests.

Two validate.rs helpers I touched (`collect_functions`,
`check_predicate_definite_return`) then failed the complexity gate — the
mean dropped after Phase 2's deletions, tightening it. Brought both to
spec: `collect_functions` now uses the `child_bodies()` recursion (the
same shape as `resolve.rs`, and more complete — it also catches functions
nested in `case`/pipeline/list), and the SB4037 reporting loop moved to
`report_non_returning_predicates`.

## Next

Phase 3 (no-shadow redeclaration error), Phase 4 (dead-variable
rejection), Phase 5 (converter `local`→drop; amend DD#35 lifetime axis;
GUIDE/SUBSET).
