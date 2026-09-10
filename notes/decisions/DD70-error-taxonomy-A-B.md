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
- **`-> int` function return values** (DD#55).

**A boolean (or any (B) result) must be consumed — enforced by the
compiler (DG, 2026-06-25).** Producing a (B) result and then **discarding
it is a compile error**, uniformly: a bare `match S P`, a bare `[[ … ]]`,
and a bare `-> int` call are all errors. There is **no synthetic `$?`**
for a (B) result, so there is nothing to "abort" at run time — a *false*
boolean is not a *failure*; the language simply makes you say what the
result means.

This resolves the bare-`[[ … ]]` question: a bare test is **not** an
allowed assertion that aborts on false (that would be the (A) reading) —
it is a must-consume compile error. Consume it (`if [[ … ]]`,
`[[ … ]] || …`) or, if you truly don't care, `[[ … ]] || :`.

Enforcement codes — **kept adjacent to the existing ignored-return family**
(SB4016 = bare non-void `-> int` call, SB4017 = return-0-only `-> int`):
- **SB4018** — a (B) boolean result is produced and discarded (bare
  `match` / bare `[[ … ]]`).
- **SB4019** — `-` applied to a (B) construct (a must-consume result has
  no runtime verdict to suppress).

(SB4018/SB4019 are the proposed adjacent numbers; the constraint is that
the "ignored result/value" diagnostics stay together in the SB401x band.
The `$?`-staleness diagnostic SB5012 — reading a stale *status* — is a
separate concern and stays in the 50xx validator band.)

Levers:
- **No `-` suppression** (→ SB4019). `-` is meaningless on a (B) construct.
- To genuinely ignore a (B) result, **consume it explicitly**: `cmd || :`
  (`:` is the no-op), or handle it with `if` / `||`.

## The distinguishing principle

> `-` is for **opaque externals** (`grep`, whose no-match→1 you cannot
> change — you suppress the verdict). For **in-process logic you control**
> (`match`, a comparison, your own `-> int` function), you **restructure**
> (`if` / `||` / `|| :`) rather than suppress. — DG

This is *why* (B) has no suppression: you authored the logic, so the
language makes you say what the result means.

## Addendum (2026-09-09) — `-` discards ANY result

DG: *"I don't like the asymmetry of `- cmd` and boolean `|| :`. `-` should
work for both."* Approved after discussion. This **supersedes** the "No `-`
suppression (→ SB4019)" lever above and the distinguishing principle that
justified it.

`- X` now uniformly means **"run X, discard its result"**, whether X is an
opaque external (A) or in-process logic (B): `- match`, `- matchall`,
`- [[ … ]]`, `- ! cmd`, and `- f` for an `-> int` function are all legal.

### Why the original rule did not hold up

Two things, neither visible when this DD was written.

1. **`|| :` was always allowed.** So the result was already discardable, and
   the rule never protected it — it only dictated *which* explicit discard the
   author had to write, and mandated the more indirect one. `cmd || :`
   expresses discard as a side effect of short-circuit control flow; `- cmd`
   says it directly. What SB4018 actually guards against is the **silent**
   discard — a bare `[[ -n $x ]]` written as if it asserted, the real bash
   habit — and `-` is not silent.

2. **(B) gained members whose primary effect is a side effect.** When this DD
   was written, (B) was bare `match S P`, `[[ … ]]`, and an `-> int` return —
   all *purely* about their result, where "you authored the logic, say what
   the result means" is compelling. `match S P ARR` and DD#63's `matchall`
   are different: the ARRAY is the point and the boolean is incidental, so
   "I don't care about the boolean" is the normal case rather than a smell.
   The old rule forced the noisiest spelling precisely where the result
   genuinely does not matter — including on DD#99's most common migrated line.

The mechanical objection ("`-` suppresses a runtime verdict, and a (B)
construct has none") is real but not decisive: this widens `-` from *discard
the verdict* to *discard the result*, which is a generalization rather than a
contradiction. The (A)/(B) split survives everywhere it does work — abort on
failure, and `$(status)` availability.

### What does NOT change

- **A bare (B) result is still SB4018.** Unmarked discard stays an error.
- **`! - cmd` is still rejected.** That is not a must-consume rule: `-` has
  already discarded the result, so `!` has nothing left to invert. A
  composition error, so the addendum does not reach it. (It keeps SB4019,
  which that code is also used for elsewhere — the `-`/`!` combination and a
  variable/function name collision.)
- **`- a | b` still suppresses only the first segment** (DD#55). But a
  *negated* pipeline keeps the suppression outside, because there the `-`
  applies to the negation's result; pushing it into the first segment left
  the `!` unsuppressed and still must-consume.

### Retired

**SB4019 no longer means "`-` on a must-consume result".** Its remaining uses
are the `-`/`!` composition error and the (unrelated, pre-existing)
variable/function collision.

Specs: `regex.sh` 63.2f, 63.cap7, 63.dash1-dash3; `errors.sh` e.b70.tdash,
bng.5, bng.11.


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
program's own name — the **verbatim `argv[0]`** as invoked (DG: not a
basename, *not* a fixed `sash:` — e.g. `./prog` prints `./prog: …`), then
exits with the failure status:

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
- **SB4016 / SB4017 / SB4018 / SB4019** — the "ignored result/value"
  family (DG: keep these adjacent). SB4016 = bare non-void `-> int` call,
  SB4017 = return-0-only `-> int`; SB4018 = discarded (B) boolean (bare
  `match` / bare `[[ … ]]`), SB4019 = `-` on a (B) construct. Distinct
  from SB5012 (`$?`-staleness — reading a stale *status*, 50xx band).

## Implementation status (2026-06-25)

- **Done** (commits c232b75, 313843a): (A) for cd / echo-write /
  redirect-open + the diagnostic facility (`sash_emsg`, `sash_last_errno`,
  errno table), native. (B) for `-> int` functions (SB4016/SB4017,
  pre-existing).
- **Pending**: (A) diagnostics for the status-based classes
  (external/pipeline/cmdsub/bg/overflow) — `abort-diagnostics-remaining`;
  (B) for `match` as a boolean command — DD#63 increment; the whole (A)
  channel in the C and janet oracles — `c-janet-dd55-abort-channel`.

## Addendum (2026-07-24): `!` (negation) is a (B) must-consume boolean

`! X` produces a logical negation of `X`'s truth value — pure in-process
logic you authored — so it is a channel-(B) result for **any** operand
(`! external`, `! pipeline`, `! [[ … ]]`, `! match`). It has **exactly**
`[[ … ]]`'s positional rules:

- bare `! X` → **SB4018** (result unused). *(Previously a bare `! external`
  was silently accepted and aborted at run time — a bug: `! /bin/false;
  echo done` never printed `done`.)*
- `Y && ! X` / `Y || ! X` (consequent position) → **SB4018**.
- `- ! X` → **SB4019** (`-` is meaningless on a (B) construct).
- consumed positions — `if`/`while`/`until ! X`, `! X && …`, `! X || …`,
  `! X || :` — negate the aggregate. The operand is **caught** (unchecked,
  no abort); `!` flips the resulting truth value.

`!` consumes its operand: the operand is lowered in a boolean context (so an
inner `[[ … ]]`/`match` does not itself fire SB4018 — the `!` is its
consumer), and the `! X` result is what must be consumed. This makes
`! external` uniform with `! [[ … ]]` (which was already SB4018 when bare).

DG (2026-07-24): "a bare command is not a boolean context … `! /bin/false;
echo done` seems worse than useless." Reuses SB4018/SB4019 — no new codes.

## Addendum (2026-08-19, DG): `!` binds a single unit; no pipeline operand

The line above ("`! pipeline` … channel-(B) result for **any** operand") is
**superseded**. DG refined `!` for **reviewability** (sash must be easier to
review than bash — see DD#55):

- **`!` negates a single reviewer-atomic unit** — one command, `[[ … ]]`, or
  `(( … ))`. It binds only that unit, so `! X && Y` is `(!X) && Y` (bash-
  consistent), not `!(X && Y)`. (Previously `! X && Y` was wrongly SB4018,
  because `!` greedily wrapped the whole `X && Y`.)
- **`!` on a multi-stage pipeline is rejected — SB4040.** A reviewer can't tell
  how far `!` reaches in `! a | b`. Escape hatch: wrap the pipeline in a
  predicate function and negate that — `f() { if a | b; then return 0; fi;
  return 1 }` then `if ! f; then …`. (This mirrors SB4023 for `pipeline &&/||`;
  a pipeline may be a whole `if`/`while` condition but never inline with
  `!`/`&&`/`||`.)

The rest of the positional template still holds: bare `! X` → SB4018, `- ! X`
→ SB4019 (now emitted at parse time, so it reaches all backends, not just
native), consumed positions negate the aggregate. Implemented in
`parse/parser.rs::parse_negated` (single-unit parse + SB4040) and
`parse_suppressed` (`- !` → SB4019). spec errors.sh bng.1-6. Journal 156.

## Addendum (2026-08-31, DG): `!` may negate the TERMINAL pipeline segment

The 2026-08-19 line ("`!` on a multi-stage pipeline is rejected — SB4040") is
**refined**, not reversed. DG: `-` and `!` are both *per-segment* pipeline
operators. `-Si` suppresses a segment from the DD#55 aggregate; the **terminal**
segment supplies the pipeline's result, so it may take the same `!` a standalone
simple command takes.

- **`a | ! b` — negating the FINAL stage — is allowed.** The stage still runs;
  its status is inverted (0↔nonzero) *before* the DD#55 worst-of fold (a
  per-segment negate, **not** a whole-pipeline negate — that would revive the
  "how far does `!` reach?" ambiguity). So `echo abc | ! grep -q z` is true
  exactly when `grep` fails. `a | b | ! c` (negate the terminal of a 3-stage)
  likewise.
- **`!` on a NON-terminal stage stays SB4040.** `! a | b` (= `(!a) | b`, negate
  segment 0) and `a | ! b | c` (negate a middle stage) are rejected — a
  non-terminal status is plumbing, and negating it then folding it into the
  aggregate is illegible. This *unifies* the old leading-`!` rejection: it is
  simply "you negated a non-terminal segment," with a precise message.
- **`-` and `!` don't combine on one segment.** `- ! cmd` (SB4019, existing) and
  its mirror `! - cmd` (SB4019, new) are both rejected — suppress discards a
  status, `!` inverts it; asking for both is contradictory.
- **`return ! X`** is a negated producer (DD#74). `return ! X | Y` routes through
  the same rule → SB4040 for the non-terminal `!` (was a confusing bare-`!`
  SB4014).

Representation: a terminal `! b` stage is a per-segment `StmtKind::Negated(b)`
wrapper (symmetric with `Suppressed`), lowered to a `negated: Vec<bool>` mask on
the SSA `Pipeline` (parallel to `suppressed`); the native `wait_for_children` and
c `emit_pipeline_wait` invert a negated stage's status before the worst-of.
janet mirrors it in `emit_pipeline_do`. The standalone `! cmd` path
(`Pipeline.negated: bool` → `NegateTestResult`) is unchanged. All three backends
implement the runtime negation. Implemented in `parse/parser.rs`
(`append_pipeline_stage`, `parse_negated`, `parse_return_producer_head`),
`transform/stmt_handlers.rs::handle_regular_pipeline`, codegen + emit_c +
emit_janet. spec errors.sh bng.7-12. Journal 220.
