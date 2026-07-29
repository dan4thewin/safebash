# 134 — `--manifest` collected at the resolution funnel

Demo-prep finding (DG): `--manifest` had two defects that break the
chain-of-trust use (a rootfs built from the manifest would be wrong):

1. **Externals inside `$( )` were omitted.** `x=$(/bin/date +%s)` listed
   nothing for `/bin/date`, though the same `/bin/date` at top level *was*
   listed.
2. **The internal sigil `__let_exit` leaked** into the list, triggered by
   `(( … ))` used as a statement/condition (in a `for` body or `if`).

## Root cause — the manifest was a denylist

`emit_manifest` scanned the SSA and *subtracted* a hand-maintained
`builtin_command_set()` string list. That design is drift-prone in **both**
directions:

- it only inspected `SsaInst::Command`/`Pipeline`, never descending into
  **values** — but command substitution is an `SsaValue::CmdSubst { argv }`
  embedded in assignment RHS / args / `[[ -n $(…) ]]`, so `argv[0]` was
  invisible (Bug 1);
- the string list had drifted from `BuiltinCmd` and was missing `__let_exit`
  (and most other sigils), so `resolve_dep` fell through and printed the raw
  sigil (Bug 2).

## Fix — collect at the single exec funnel (DG's design)

DG reframed it: *"why wouldn't we add to a set every time we compile in an
exec?"* Flip the denylist to an **allowlist collected where the compiler
decides to exec an external**. That choke point already exists:
`transform::resolution::resolve_pipeline_command` — every external form
(direct command, pipeline stage, `$( )` argv[0], cmdsub-pipe-while, trap
body) resolves through it; inline builtins short-circuit, and sigils /
user-functions never reach it.

The recording predicate needs **no list at all**: record the resolved name
iff it contains `/` (a real path we will exec). `echo`/`true`/`false`/`:`
return bare → skipped; `__let_exit` never calls the resolver → can't leak.

Mechanics:

- `resolve_pipeline_command(name, state)` records into
  `VersionState.external_deps` (a `RefCell<BTreeSet<String>>`). The `RefCell`
  lets the **immutable** expression-evaluation path (`expr_to_value` →
  `build_argv`, `&VersionState`, 29 callers) record `$( )` externals without
  a `&mut` ripple through the whole evaluator. Safe here because transform
  threads a single `&mut VersionState` and never whole-clones it (only
  targeted per-field save/restore for function inlining), so there is no
  lost-update-through-clone hazard.
- Surfaced onto `SsaProgram.external_deps` at the end of `ir_to_ssa_typed`,
  mirroring the existing `ssa.exiting_signals = state.exiting_signals` line.
- `emit_manifest` prints `ssa.external_deps`. The whole denylist —
  `builtin_command_set`, `collect_external_deps`, `collect_inst_deps`,
  `resolve_dep` (~90 lines) — is **deleted**.

Bonus: resolution honors `--sysroot` internally (`resolve_command_path` →
`get_sysroot`), so the manifest now honors sysroot **consistently with the
real compile**, for free (the old `resolve_dep` re-implemented its own
sysroot-aware PATH walk).

### DG design decision (RefCell)

DG pushed on "why avoid a RefCell in the tree?" The honest answer: the only
objection that would make it *unsafe* — lost updates if `VersionState` were
cloned — does not apply (verified: single threaded `&mut`, no whole-clone).
The residual costs are soft (runtime borrow-checking vs compile-time; the
`&VersionState` signature no longer signals "pure read"; first interior
mutability in the tree). DG accepted those and chose the RefCell so the
recorder sits *literally at the exec funnel*.

## Tests

Zero `--manifest` coverage existed (that's why this shipped broken). Added 6
integration tests in `sash/tests/e2e_tests.rs` (mode-independent compiler
output ⇒ Rust test, not the bash-oracle spec harness):

- `manifest_includes_command_substitution_external` — Bug 1 regression.
- `manifest_excludes_internal_let_exit_sigil` — Bug 2 regression (empty
  manifest for `if (( 1+1 )); then echo hi; fi`).
- `manifest_excludes_all_internal_sigils` — no `__…` from `! …` / redirects.
- `manifest_includes_pipeline_stages`, `manifest_excludes_user_functions_lists_body`,
  `manifest_excludes_builtins`.

Native spec suite: 0 regressions (801). e2e: 86→92 pass, 38 fail unchanged
(pre-existing stale-syntax failures, verified by stash-compare).

## Separate finding (already filed, reinforced here)

While reproducing, hit a **silent runtime bug**: a pipeline inside `$( )`
— `x=$(/bin/echo hi | /bin/cat)` — compiles but emits the literal
placeholder text `$(...)` at runtime. Same root cause as the existing
`notes/tasks/bug-multistatement-cmdsub-placeholder.md`:
`handle_command_substitution`/`single_command_subst` returns `None` unless
the body is exactly one `Command`, and the fallback stringifies to the
`$(...)` debug placeholder. A pipeline body is single-statement-but-not-a-
`Command`, so it hits the same hole. Augmented that task with the pipeline
instance + the manifest dovetail (once lowered structurally, its stages
resolve through the same funnel and auto-appear in the manifest) rather
than filing a duplicate. Awaiting DG's lower-vs-reject decision.
