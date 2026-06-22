---
title: "tighten cmdsub taint: execve(2)/execveat(2) output is tainted; delete the 'clean iff args clean' pass-through"
status: ready
priority: P2
kind: bug
depends: [advance-dd23-resolution]
gates:
  - "a command substitution whose command runs via execve/execveat has TAINTED capture, unconditionally — incl. printf and pwd (they exec /usr/bin/...)"
  - "internal builtins (echo, getcwd, getpid, getppid, epochseconds, random, argc, …) have CLEAN capture (no exec)"
  - "getenv/argv/arg stay tainted (external input); user-function captures stay clean (DD#24)"
  - "the args-inspection / 'pass-through builtins: clean iff all args clean' path is DELETED from cmd_subst_taint and its CST twin"
  - "an unblessed external cmdsub yields a clean SB5020 from the front-end (span-bearing), NOT the SSA ICE backstop"
---

# Tighten cmdsub taint — execve is the taint source

DG: "anytime execve(2)/execveat(2) occur, that is external and any output
captured is tainted." The current `cmd_subst_taint` (ssa_taint.rs) wrongly
treats `printf`/`echo`/`pwd` as pass-through ("clean iff args clean").

The args-clean logic is **dead anyway**: reading a tainted value is SB5020
*everywhere* except inside a bless test, so a cmdsub's args are
necessarily clean. Capture taint is therefore **command-only**:

- **execve/execveat** (external program — printf, pwd, cat, grep, …) →
  **tainted**.
- **internal builtins** (echo, getcwd, getpid, …) → **clean**.
- **getenv/argv/arg** → **tainted** (external input).
- **user functions** → **clean** (DD#24).

Delete the args-inspection path. With `advance-dd23-resolution` hoisting
resolution ahead of the front-end, the CST/validation taint pass sees the
resolved abs path and emits a clean span-bearing SB5020 — the SSA
`cmd_subst_taint` ICE backstop never fires for valid input.

Note: flips spec `commands.sh 14.2b` (`$(printf hi)`) from clean to
needs-bless — update that test when this lands.
