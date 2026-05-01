# Experiment 018: Subshell elision analysis

**Date:** 2026-04-11

**Goal:** Prove that subshells and command substitutions don't always
need a real process.  Build a static analysis pass that classifies
each `$(...)` and `(...)` as **elidable** (can be compiled as inline
function calls / string buffers) or **required** (must spawn a real
subprocess).

**Insight (DG):** Bash conflates three things in `$(...)`:
1. Capture — getting output as a string.  Needs a pipe, not a process.
2. Isolation — preventing side effects from leaking.  SSA versioning
   already provides this at compile time.
3. Parallelism — concurrent execution.  This genuinely needs processes
   (and is probably outside the safe subset).

For safebash, only (3) truly requires a process.  For (1) and (2),
the compiler can lower `$(my_func)` to a function call returning a
string.  No fork, no pipe, no fd manipulation.

**Architecture:**
```
src/elision.rs  — classify subshells and command substitutions
```

Key types:
- `SubshellClass::Elidable` — no process needed
- `SubshellClass::Required(Vec<ProcessReason>)` — must fork
- `ProcessReason` — ExternalCommand, Pipeline, BackgroundJob, UnknownCommand
- `FunctionMap` — tracks function definitions and their classifications
- `ElisionReport` — per-program analysis results

**Classification rules:**
- Builtin commands (echo, printf, cd, local, etc.) → elidable
- Known function whose body is all-elidable → elidable
- Known function whose body has externals → required
- External/unknown command → required
- Multi-stage pipeline → required
- Nested command substitutions → recursive analysis
- Expressions inside assignments/declarations → checked for cmd subs

**Sample output** (realistic deploy script):
```
Functions:
  get_version  → ELIDABLE (no process needed)
  format_msg   → ELIDABLE (no process needed)
  log          → REQUIRED (external: date)
  get_timestamp → REQUIRED (external: date)

Elidable (3): $(get_version), $(format_msg), (cd /tmp && echo ...)
Required (4): $(date), $(get_timestamp), $(tar ...), $(pipeline)
```

**Tests:** 30 tests covering:
- Command substitutions: echo/printf/true/cd/known-function elidable
- Command substitutions: external/pipeline/unknown/function-calling-external required
- Subshells: builtins elidable, external required, isolation-only elidable
- Function analysis: pure-builtins/external/mixed classification
- Mixed scripts, nested cmd subs, realistic deploy script
- Edge cases: empty cmd sub, cmd sub in array, context strings
- Entry kinds (CommandSubstitution vs Subshell)

**Implications for Cranelift:**
When lowering to native code, elidable `$(func)` becomes a normal
function call returning a string.  No fork/exec/pipe.  This is a
significant optimization and simplification for AOT compilation.

**Conclusion:** Subshell elision analysis works.  The compiler can
now distinguish "needs a process" from "needs a string value" and
choose the appropriate compilation strategy.

---

