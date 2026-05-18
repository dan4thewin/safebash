title: Session — Error model, arithmetic refinement, audit triage
date: 2026-05-02

## DD audit triage — complete

Resolved all remaining audit tasks (~25 this session). Most were
documentation gaps or non-issues; several surfaced real design work.

### Resolved as non-issues or minor doc fixes

- **DD#24 mechanism change** — DD note already says "per-definition
  classification, no propagation." GUIDE.md had stale "flow-sensitive
  taint tracking" language. Fixed the line.

- **DD#40 --nolint guardrails** — `--nolint` only skips shellcheck.
  Validator and taint are non-skippable. No gap.

- **DD#50 "no propagation" tension** — SUBSET.md language is precise.
  DD#50 free-read tracking is analysis, not propagation.

- **DD#39 expression form amendments** — DD#39 already has the final
  syntax. Non-colon coercion is in DD#29. Auditor looked in the
  wrong place.

- **DD#52 not in SUBSET.md** — `${var@...}` (SB4015) missing from
  rejected constructs table. Added.

- **Authority order** — answered by the documents being called
  "design decisions." Added dual-role framing (compiler as
  specification) to README instead.

- **GUIDE.md sash_exec PATH** — accurately describes current
  implementation. DD#23 compile-time resolution not built yet.
  Added GUIDE update reminder to existing task.

- **GUIDE.md stale counts** — removed all hardcoded counts. They
  go stale immediately and create maintenance busywork.

- **Dangerous external arguments** — GPT-5.4 claimed sash disclaims
  the argument attack surface. Wrong. sash eliminates word splitting,
  controls glob expansion, requires taint validation on every external
  value. Considered flag injection detection, code-execution positions,
  `--` sentinel lint. All rejected as babysitting.

- **exec fd manipulation** — already implemented in codegen. No DD
  needed, same as if/while.

- **User-function capture trust** — derived property of taint model.
  Existing rules are clear.

- **$$ and $! clean** — process state from operations you initiated.

- **Stateful builtins** — if sash compiles it, it's trusted.

- **Source/binary identity** — DD#45 already answers.

- **Pinned limitations** — items stale or already documented. Deleted
  pinned.md entirely. Removed references from status and goosehints.
  Notable: pinned.md listed a `[[ -v var ]]` edge case, but the
  journal (042, line 692) already removed `-v` from the design.
  Variable existence is handled by `getenv`/`argv` returning empty.

- **Glob "restricted"** — changed to "rejected" in SUBSET.md.

- **"rejected (coerced to...)"** — incoherent language. Fixed to
  "coerced to" throughout SUBSET.md.

### Resolved with documentation or task updates

- **DD#25 overflow underspec** — wrapping in practice, not trapped.
  Documented in SUBSET.md. Tasks created for overflow traps and
  overflow recovery model decision.

- **DD#27 interaction with DD#38/DD#50** — DD#38 already documents
  fork+trampoline interaction. Subshell decomposition tasks exist.

- **DD#30 bare numeric amendment** — `x=5` is a string. Updated
  DD#30. Created arithmetic operand type checking task.

- **DD#30 numeric comparison contradiction** — journal intent wins:
  `[[ ]]` for strings/files, `(( ))` for arithmetic, no overlap.
  Updated SUBSET.md, created implementation task.

- **DD#32 GUIDE partial** — created task for `SASH_TRACE`.

- **DD#39 regex engine limits** — documented in DD#39 and SUBSET.md.
  Two tasks: compile-time validation, runtime precompilation.

- **DD#46 not in GUIDE taint sources** — updated GUIDE. Corrected:
  passing tainted var as function arg is a compile error (it's a
  read). Created positional rejection task.

- **DD#49 tautology bypass** — strengthened rule: $VAR on LHS only.
  Created task.

- **Unset semantics** — refined: scalar `unset` → compile error in
  sash-native, coerce in bash-mode. `unset` for collection element
  removal stays. Created task. Fixed SUBSET.md.

## DD#55 — Error handling model (new DD)

Extensive discussion. Key decisions:

- Compiler inserts implicit exit checks (set -e without blind spots)
- Programmer opts OUT with `-`, not in
- C system call analogy: if it can fail due to external state, checked
- `- cmd` suppresses; `$?` unavailable after `-` (compile error)
- `$?` scoped to handling expressions (`||` RHS, `if`/`while`)
- `- cmd; return $?` → error. `cmd || return $?` → valid.
- `local x=$(cmd)` — exit check fires (departure from bash)
- Non-void functions (any `return`) require explicit handling at call
- `return 0` alone → compile error
- Background: exec failure checked, wait optional
- SIGPIPE: programmer's responsibility

Four implementation tasks created.

## DD#31 amendments — arithmetic refinement

- Postfix `x++`/`x--` only, statement-only, never a value (Go rule)
- No prefix forms
- Comma operator desugars `(( a, b ))` → `(( a )); (( b ))`
- `if`/`while` allow multiple statements, final as condition
- Bare boolean at statement level → compile error

Updated existing task with all new rules.

## DD#56 — Signal and trap model (new DD)

All signal handling is flag-based. Runtime sets `signum` (int global,
keyword like `argv`/`getenv`) on signal arrival. `signal` builtin
configures disposition:

- `signal SIG IGN` — OS masks signal (SIG_IGN), invisible
- `signal SIG DFL` — restore kernel default
- `signal SIG FLAG` — flag only, programmer polls, zero overhead
- `signal SIG EXIT` — flag + compiler inserts checks → jumps to END

END block is a top-level declaration (exactly one). Universal cleanup.
Programmer dispatches with `case $(signum)`. Signal 0 = normal exit.

Dispositions can change mid-program (`FLAG` → `EXIT` or vice versa).

Four implementation tasks: signum keyword, flag traps, exiting
traps, END block rework.

### BUG — EXIT trap wrong scope

Found during signal/trap discussion. Current implementation inlines
EXIT trap before every Return terminator (wrong — fires on function
returns in trampoline model). Also misses `exit` builtin. Bug filed.

## README rewrite

First section rewritten. Opens with what sash IS (defines and
compiles a safe subset). Added "the compiler is also a specification."
Dropped stale claims. Voice guided by VOICE.md.

## Magic variable policy

Drew the line: `$$`, `$!`, `$?` for punctuation specials. Keywords
for syscall-backed values: `epochseconds`, `getpid`, `getppid`,
`getcwd`, `signum`, `argv`, `getenv`. `$RANDOM` dropped (bad LCG,
use /dev/urandom). Task created to review full taint clean list.
