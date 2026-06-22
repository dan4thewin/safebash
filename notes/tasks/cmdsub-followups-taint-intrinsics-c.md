---
title: "cmdsub follow-ups: unblessed-external ICE vs clean SB5020, getpid-class intrinsics, C Phase-4 exec"
status: ready
priority: P3
kind: bug
depends: []
gates:
  - "an UNBLESSED bare-external command substitution (e.g. `x=$(cat f); echo $x`) is rejected with a clean SB5020 from the CST/validation taint pass, NOT the SSA ICE backstop ('CST taint pass missed this')"
  - "`$(getpid)` / `$(getppid)` / `$(getcwd)` / `$(epochseconds)` / `$(random)` / `$(argc)` etc. are intercepted as in-process intrinsics in command substitution (like getenv/argv/arg), not exec'd → no 'bare command' error"
  - "the C backend implements cmdsub exec (currently `/* TODO cmdsub exec … (Phase 4) */` → empty capture); `$(printf hi)` in c yields `hi`"
---

# Command-substitution follow-ups (surfaced by bug-cmdsubst-builtin-not-resolved)

Resolving the cmdsub head (DD#23) made bare-external `$(cat …)` /
`$(printf …)` actually run. Three pre-existing gaps surfaced:

1. **Unblessed-external ICE.** `x=$(cat f); echo $x` (no bless) is caught
   only by the SSA taint backstop, which emits an **ICE** ("CST taint pass
   missed this", ssa_taint.rs:464). The CST/validation taint pass
   (`cmdsub_is_taint_source`/`cmdsub_is_scalar_taint_source`, validate.rs)
   only classifies getenv/argv/arg//abs as tainted — it under-classifies
   bare externals. It needs to mirror the SSA `cmd_subst_taint` (basename
   + user-function list + pure-builtin/pass-through lists) so a clean
   SB5020 fires. NB: this duplicates classification across two passes —
   consider whether the SSA backstop should instead be promoted to a real
   enforcement layer (taint-architecture decision for DG).

2. **Intrinsics not intercepted in cmdsub.** Only getenv/argv/arg are
   intercepted; `getpid`/`getppid`/`getcwd`/`epochseconds`/`random`/
   `argc`/`len`/`keys`/`join`/`status`/`signum`/`dtoi`/`itod` are
   recognised as clean by `cmd_subst_taint` but are NOT handled in the
   cmdsub capture path → they reach sash_exec as bare names → "bare
   command passed to sash_exec".

3. **C cmdsub exec is a Phase-4 TODO.** `emit_c` emits `/* TODO cmdsub
   exec: … (Phase 4) */` and captures empty. Implement the fork/pipe/exec
   capture (mirror codegen `emit_cmdsub_exec`) so the C oracle matches
   native for `$(external …)`.
