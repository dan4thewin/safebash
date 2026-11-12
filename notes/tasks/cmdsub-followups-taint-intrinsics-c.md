title: "cmdsub follow-ups: unblessed-external ICE vs clean SB5020, getpid-class intrinsics, C Phase-4 exec"
status: done
priority: P3
kind: bug
depends: []
gates:
  - "an UNBLESSED bare-external command substitution (e.g. `x=$(cat f); echo $x`) is rejected with a clean SB5020 from the CST/validation taint pass, NOT the SSA ICE backstop ('CST taint pass missed this')"
  - "`$(getpid)` / `$(getppid)` / `$(getcwd)` / `$(epochseconds)` / `$(random)` / `$(argc)` etc. are intercepted as in-process intrinsics in command substitution (like getenv/argv/arg), not exec'd → no 'bare command' error"
  - "the C backend implements cmdsub exec (currently `/* TODO cmdsub exec … (Phase 4) */` → empty capture); `$(printf hi)` in c yields `hi`"

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

## Update 2026-09-08

Gate 1 (the unblessed-external ICE) is **no longer reproducible**:
`x=$(/bin/cat /etc/hostname); echo $x` now yields a clean **SB5020**, not the
SSA taint backstop's ICE. Closed by `tighten-cmdsub-taint-execve` (done). The
remaining live work in this task is the **C cmdsub exec** Phase-4 TODO.

## Resolved (DG, 2026-06-21) — reframed into three threads

DG corrected the framing: there are no "bare externals" (DD#23 maps every
external to an absolute path), and the taint criterion is operational —
**any output that passed through `execve(2)`/`execveat(2)` is tainted**;
`getenv`/`argv`/`arg` are tainted external *input*; everything the
compiler computes in-process (internal builtins, inlined user functions)
is clean. The "unblessed-external ICE" is just a symptom of DD#23 being
partial (resolution runs in transform, after the front-end taint pass).

Split into three threads (filed): `advance-dd23-resolution`,
`tighten-cmdsub-taint-execve`, `implement-getpid-builtins`. The C cmdsub
exec (Phase-4 TODO) remains pure implementation and stays in *this* task.

## Closed 2026-09-13 — re-measured, all three gates met

Gate 1 (unblessed-external ICE) closed 2026-09-08, as noted above.

Gate 2 (**intrinsics in cmdsub**) is no longer reproducible: `$(getpid)` works
bare (`echo "$(getpid)"`), through an assignment (`x=$(getpid)`), and as an
arith-sexpr operand (`$(( (getpid) > 0 ))`). No "bare command passed to
sash_exec".

Gate 3 (**C cmdsub exec**) is **implemented**. `--emit-c` on a script
containing `s=$(/usr/bin/printf hi)` emits **zero** `TODO` markers, and the
linked C binary prints `[hi]` — byte-identical to native. The Phase-4 TODO is
gone.

Re-measuring this one is also what surfaced journal 286's two arithmetic
bugs: the probe used to demonstrate gate 2 contained `$(( $(getpid) > 0 ))`,
which printed `0`. See `arith-cmdsub-support.md` for the language-surface
question that fell out of it.
