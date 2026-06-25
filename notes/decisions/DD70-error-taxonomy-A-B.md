title: DD#70 — Error-handling taxonomy: (A) runtime abort-if-unchecked vs (B) compile-time must-consume
status: accepted

Decided with DG 2026-06-25, refining DD#55 (Implicit Exit Checks and
Error Handling Model). DD#55 established *that* the compiler inserts
implicit failure checks; this DD names the **two distinct kinds of
"something went wrong"** and fixes their differing semantics, the
suppression rule, and the diagnostic contract. It is the authoritative
framing every fallible operation must follow.

## The two channels

There are two unrelated notions of failure, and conflating them is the
mistake this DD prevents.

### (A) Runtime abort-if-unchecked

A *genuine failure the runtime observes at run time*. The compiler emits
an implicit check; an **uncaught** failure aborts the program (DD#55),
printing a diagnostic (below). Members:

- **Syscalls we issue**: `chdir` (cd), `open` (redirects), `write`
  (echo) — and, by extension, any future builtin that wraps a syscall.
- **Child exit status** reaped via `wait`: the implicit foreground wait
  after an external command/pipeline, and an explicit `wait` on a `&`
  job. Nonzero is a failure (pipefail always on, DD#20).
- **Dynamic `regcomp`** (a pattern not known at compile time can fail to
  compile at run time). Literal regex is validated at compile time and
  cannot reach (A).

Levers:
- **`-` suppresses** the command's *result-verdict* (DD#55): the command
  runs, its status is discarded, `$?` is unavailable afterward.
- **`if` / `while` / `&&` / `||`** *catch* the verdict — it becomes the
  branch condition (no abort, `$(status)` available in the handling
  position per DD#55).

### (B) Compile-time must-consume

A *pure, in-process logical result* — there is no syscall and nothing the
runtime can "fail" at. Members:

- **`match SUBJECT PATTERN`** as a boolean command (DD#63).
- **`[[ … ]]` tests** / comparisons (DD#62).
- **`-> int` function return values** (DD#55: a bare call of a non-void
  function is SB4016; a return-0-only `-> int` function is SB4017).

A (B) result that is produced and then **discarded is a compile error**.
There is **no synthetic `$?`** for a (B) result, so there is nothing to
"abort" at run time — a *false* boolean is not a *failure*.

Levers:
- **No `-` suppression.** `-` is meaningless on a (B) construct (there is
  no runtime verdict to throw away).
- To genuinely ignore a (B) result, **consume it explicitly**: `cmd || :`
  (`:` is the no-op), or handle it with `if` / `||`.

## The distinguishing principle

> `-` is for **opaque externals** (`grep`, whose no-match→1 you cannot
> change — you suppress the verdict). For **in-process logic you control**
> (`match`, a comparison, your own `-> int` function), you **restructure**
> (`if` / `||` / `|| :`) rather than suppress. — DG

This is *why* (B) has no suppression: you authored the logic, so the
language makes you say what the result means.

## Redirect open(2) is an (A) setup-failure, not a verdict

`echo foo > /bad` is, semantically, `exec 4>/bad; echo foo >&4` (DG). The
`open` is a checked syscall that runs **first**. A failed open makes the
command fail **without writing** (no misroute — the historical bug was the
open's `-1` being discarded, leaving fd 1 = stdout). Consequences:

- The open's abort is governed by **context**: it aborts unless it sits in
  a **condition** (`if`/`||`), where the failed final-command result
  decides the branch instead.
- **`-` does not reach the open** — the `-` binds to the *command*
  (echo's write verdict), not the prior implicit `exec N>file`. So
  `- echo > /bad` still **aborts**.
- Implementation note (no new IR field): the "in condition" signal is the
  last `Command`/`Pipeline` of a `Branch`-terminated block (codegen
  derives it the same way transform's `mark_unchecked_in_conditions`
  does), kept distinct from the `-`-polluted `checked` flag.

## Diagnostic contract

Every (A) abort writes one line to **stderr**, prefixed with the
program's own name (`argv[0]` — *not* a fixed `sash:`), then exits with
the failure status:

```
<argv0>: <context>: <reason>
```

- **cd**            → `<argv0>: cd: <path>: <reason>`
- **redirect open** → `<argv0>: <path>: <reason>`
- **echo write**    → `<argv0>: echo: write error: <reason>`
- **external/pipeline/cmdsub** → `<argv0>: <name>: command failed (exit N)`
- **background exec** → `<argv0>: <name>: <reason>`
- **arithmetic overflow** (DD#25) → `<argv0>: arithmetic overflow`
- **regcomp** → `<argv0>: regexp error: <msg>`

`<reason>` is the OS `strerror` text (ENOENT "No such file or directory",
ENOSPC "No space left on device", …) via a hand-rolled errno→string table
(`sash-rt` is `#![no_std]`, no libc). The diagnostic fires **only on the
actual abort** — a *caught* (`if`/`||`) or `-`-*suppressed* failure prints
**nothing** (it is handled, not fatal). Implementation lives at the codegen
abort site, not in the syscall wrapper, precisely so caught failures stay
silent.

## Relationship to other decisions

- **DD#55** — parent: implicit checks, `-` suppression, `$?` scoping,
  void/non-void functions, pipelines, background. This DD adds the
  (A)/(B) taxonomy and the diagnostic contract on top.
- **DD#25** — arithmetic overflow is its own abort mechanism; it belongs
  to (A) for diagnostic purposes (`<argv0>: arithmetic overflow`).
- **DD#56** — on abort with a trap set, the runtime routes through the
  END block (traps run) rather than exiting directly; the diagnostic is
  written before that jump.
- **SB4016 / SB4017** — the existing (B) enforcement for `-> int`
  functions. `match`'s (B) boolean form (DD#63) extends (B) to a builtin.

## Implementation status (2026-06-25)

- **Done** (commits 8a40ddc, f9d833b): (A) for cd / echo-write /
  redirect-open + the diagnostic facility (`sash_emsg`, `sash_last_errno`,
  errno table), native. (B) for `-> int` functions (SB4016/SB4017,
  pre-existing).
- **Pending**: (A) diagnostics for the status-based classes
  (external/pipeline/cmdsub/bg/overflow) — `abort-diagnostics-remaining`;
  (B) for `match` as a boolean command — DD#63 increment; the whole (A)
  channel in the C and janet oracles — `c-janet-dd55-abort-channel`.
