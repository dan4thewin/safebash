title: DD#55 — Implicit Exit Checks and Error Handling Model
status: implemented

No `set -e` at runtime. The compiler inserts implicit exit checks on
every operation that can fail at runtime. Failure = nonzero exit or
runtime error (e.g., `regcomp` on dynamic pattern). `- cmd` suppresses.

## What gets checked

Anything that can fail due to external state or data:

- External commands (fork/exec → wait → check status)
- Builtins that wrap syscalls (`cd`, `echo`/`printf` via `write(2)`)
- Runtime calls that can fail on data (`regcomp` on dynamic patterns)
- Command substitution (`x=$(cmd)` — check before assignment)

## What does not get checked

- Variable assignment (`x="hello"`)
- Arithmetic evaluation (`(( x = x + 1 ))`)
- Pure string operations
- Literal regex (validated at compile time, can't fail at runtime)

## Suppression with `-`

`- cmd` suppresses the implicit check. The command runs, its exit
status is discarded. `$?` is not available after `- cmd` — reading
`$?` after `-` is a compile error. You said you don't care; you
don't get to inspect it.

## Explicit handling

Three patterns for a command that can fail:

| Pattern | Meaning |
|---------|---------|
| `cmd` | Implicit check — abort on nonzero |
| `- cmd` | Suppressed — don't care, `$?` unavailable |
| `cmd \|\| action` / `if cmd` / `while cmd` | Explicit handling — `$?` available in handling position |

`$?` is scoped to the handling expression (`||` RHS, `if`/`while`
body), not available as a general-purpose variable in subsequent
statements. Furthermore, `$?` (or `$(status)`) is only valid as the
**first expression evaluated** in the handling branch — once another
command executes, the status is consumed and unavailable. This
prevents stale-status bugs where `$?` refers to something other
than the command that caused entry to the branch.

## `local` declarations

`local x=$(cmd)` — the capture's exit check fires regardless of
`local`. sash does not swallow exit codes in declaration context.
This is an explicit departure from bash, where `local` is the
command and its success masks the substitution's failure.

## Command substitution

`x=$(cmd)` — implicit check fires on cmd's exit status. If cmd
fails, the program aborts. `x` is never assigned.

`x=$(- cmd)` — suppressed. cmd runs, output captured, exit status
discarded. `x` gets whatever output cmd produced before it exited.

`x=$(cmd) || x=default` — explicit handling. If cmd fails, the
`||` branch executes.

## Pipelines

Pipefail always on (DD#20). Implicit check fires on the pipeline's
aggregate status. Per-segment suppression with `-`:

```
- cmd1 | cmd2       # cmd1 failure suppressed, cmd2 checked
cmd1 | - cmd2       # cmd1 checked, cmd2 failure suppressed
- cmd1 | - cmd2     # both suppressed
```

If any unsuppressed segment fails, the program aborts. SIGPIPE is
the programmer's responsibility — use `- cmd` on the producing
side or choose tools that don't trigger it (`sed -n 1p` over
`head -1`).

Per-segment `!` negation is the companion operator (DD#70 addendum
2026-08-31): `cmd1 | ! cmd2` inverts the **terminal** segment's status
(0↔nonzero) before it joins the worst-of below. Only the terminal segment
may be negated (a non-terminal `!` is SB4040); `-` and `!` don't combine on
one segment (SB4019).

### Aggregate is context-independent; position decides the consequence (2026-07-24)

The pipeline's truth value is **one** definition everywhere: the worst
exit status over its **non-suppressed** segments (pipefail). *Position*
decides only the consequence, never the aggregate:

- **statement position** — a nonzero aggregate aborts (routes through END).
- **condition position** (`if` / `while` / `until` test, operand of `!`) —
  the aggregate is the truth value; it does not abort.

So `if /bin/false | /bin/true; then …` takes the **else** branch — a
first-segment failure counts, exactly like bash `set -o pipefail`. A
`-` on **any** segment (first, middle, or last) excludes only that
segment's status from the aggregate; the segment still **runs** (it is a
real pipe stage — `echo x | - grep y` runs grep, it is not dropped).

This corrects a prior implementation bug where a pipeline in a non-abort
context silently ignored its **first** segment (an undocumented
`i == 0 && !checked` skip that conflated leading-`-` suppression with
condition context), and where a non-first `-` dropped the stage entirely.

### `&&` / `||` / `!` do not take a pipeline operand

A pipeline may be an **entire** condition (`if a | b; then …`, `while a | b`),
but it may **never** be combined inline with `&&`, `||`, or `!`:

- `a | b && c` / `a | b || c` — **SB4023**.
- `! a | b` — **SB4040** (DG 2026-08-19).

The reason is **reviewability, not semantics**. bash's precedence resolves all
of these unambiguously (`|` binds tighter than `!` and `&&`/`||`), so the exit
status is well-defined. But **sash is meant to be easier to review than bash**:
a non-expert reviewer shouldn't have to know operator-vs-pipe precedence to see
what `&&` tests or how far `!` reaches. So sash rejects the inline forms and
requires the explicit shape:

```
if a | b; then c; fi                  # instead of  a | b && c
if a | b; then :; else c; fi          # instead of  a | b || c
f() { if a | b; then return 0; fi; return 1 }   # instead of  ! a | b
if ! f; then …                        #   (wrap the pipeline in a predicate fn)
```

A pipeline as the **consequent** (`cmd && a | b`), a trailing redirect
(`a | b > f`), and a background pipeline (`a | b &`) remain legal — only the
inline `pipeline &&/||/!` combinations are rejected. `!` on a single unit (one
command, `[[ ]]`, `(( ))`) is fine, and binds only that unit — `! X && Y` is
`(!X) && Y` (DD#70).

## Background processes (`cmd &`)

Exec failure (fork or exec fails) is checked — the parent aborts.
A successfully forked background process runs independently and
can outlive the sash program. `wait` is optional. If used, `wait`'s
return value follows normal implicit check rules.

## User-defined functions

### Void functions

No `return` statement, or unreachable (all paths abort or loop).
Bare call at call site is fine — no check needed.

### Non-void functions

Any `return` statement makes the function non-void. Call sites must
explicitly handle the return value:

- `if myfunc` / `myfunc || handle` — explicit handling
- `- myfunc` — suppressed
- bare `myfunc` — **compile error** (unhandled non-void return)

`return 0` alone is a compile error — if the only possible return
is zero, the `return` is ceremony. Omit it (void function).

### Bare call of int-typed function

Compile error. If the function returns a value, capture it or
handle it. A bare call discards the return value.

### `return $?`

Valid only in handling position: `cmd || return $?`. Propagates
the failure to the caller, making the function non-void, which
requires explicit handling at every call site.

`- cmd; return $?` is a compile error — `$?` is unavailable
after `-`.

## Comparison with bash `set -e`

Bash `set -e` has known blind spots:

- Functions called from `if` lose errexit for their entire body
- `local x=$(cmd)` swallows exit codes
- `(( 0 ))` kills the script
- Subshells inherit errexit inconsistently

sash has none of these. Checks are inserted per-operation by the
compiler. Condition position is recognized syntactically. `local`
doesn't swallow. Arithmetic doesn't produce exit codes.

## Compound-condition catch + list/body abort (2026-08-19, DG)

Externalizing `true`/`false` unmasked a pre-existing bug and two design
questions; DG resolved them:

- **`&&`/`||` operands in a condition are CAUGHT.** `if a && b` / `while
  a || b` treat the whole `&&`/`||` aggregate as the condition — every
  operand's failure is caught, never aborted. Previously only the operand
  whose block terminated the condition's `Branch` was unchecked (the left of
  each `&&`/`||`, via the short-circuit block); the **right** operand sat in a
  `Goto`-terminated block and aborted. Fix: in `transform/statements.rs::
  lower_list_stmt`, when the List is in a *caught* position (`in_condition >
  0` — a condition, or the left of an enclosing `&&`/`||`), uncheck the right
  operand's terminal command too.

- **`if a; b` is `a; if b`** (DG). A `;` list's non-final member is a **bare
  command** whose status is discarded → it **aborts** (bare-command-uncaught).
  Only `&&`/`||` operands (logical parts of one boolean) are caught; a `;`
  sequence is not. So `if a; b` runs `a` (aborts on failure), then `if b`.

- **A failing FINAL command in an if/else BODY aborts** (DG). Its status is
  discarded, exactly like a top-level `false` — bare-command-uncaught. The old
  `bc.e1`/`bc.e2` rc-0 expectation was a non-aborting-builtin artifact.

Coupled fix: the `||` arm reset `$?=0` after the right side ("the list
succeeded") — correct for a top-level guard (`a || fallback`), but wrong in a
condition, where `if false || false` must take the else. The reset is now
skipped in a caught position (the aggregate's real status is the condition
value). native+c; janet emits conditions from the AST (pre-existing
control-flow gap, `cf-backend-gaps-brace-surfaced` — brace.sh is native-only).
spec brace.sh bc.a6b/a7/a8/a9/e1/e2, errors.sh cc.1-4. Journal 153.
