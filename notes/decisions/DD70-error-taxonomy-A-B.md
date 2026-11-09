title: DD#70 — Error-handling taxonomy: (A) runtime abort-if-unchecked vs (B) compile-time must-consume-or-discard
status: accepted

Decided with DG 2026-06-25, refining DD#55 (Implicit Exit Checks and
Error Handling Model). DD#55 established *that* the compiler inserts
implicit failure checks; this DD names the **two distinct kinds of
"something went wrong"** and fixes their differing semantics, the
suppression rule, and the diagnostic contract. It is the authoritative
framing every fallible operation must follow.

DD130 (DG, 2026-11-06) separately distinguishes **recoverable failures**
from **fatal faults**. The channels below concern result handling; a fatal
fault is not a result to consume in either channel. A value-producing
operation can encounter a fatal fault without changing its result kind.

## The two channels

There are two result-handling channels, and conflating them is the mistake
this DD prevents.

### (A) Recoverable runtime failures: abort-if-unchecked

A *recoverable failure the runtime observes at run time*. The compiler emits
an implicit check; an **uncaught** failure aborts the program (DD#55),
printing a diagnostic (below). Members:

- **Recoverable syscall failures**: `chdir` (cd), path-specific `open`
  errors (redirects), `write` (echo) — and, by extension, future builtins
  wrapping syscalls. DD130 requires cause-specific classification rather
  than assuming every failure of one syscall belongs here.
- **Child exit status** from the implicit foreground wait after an
  external command/pipeline. Nonzero is a failure (pipefail always on,
  DD#20). Explicit `wait` follows DD126's pid/result-latch contract;
  it does not implicitly abort on a nonzero child status.
- **Dynamic `regcomp`** (a pattern not known at compile time can fail to
  compile at run time). Literal regex is validated at compile time and
  cannot reach (A).

Levers:
- **`-` suppresses** the operation's recoverable result (DD#55/DD130);
  its status is discarded and `$?` is unavailable afterward. A failed
  required redirect still prevents the command from running.
- **`if` / `while` / `&&` / `||`** *catch* the verdict — it becomes the
  branch condition (no abort, `$(status)` available in the handling
  position per DD#55).

### (B) Compile-time must-consume-or-discard

A logical or typed value, rather than a recoverable operation's failure
verdict. The producing operation can still have operand/body effects or
encounter runtime failures; those are not the value being consumed.
Members:

- **`match SUBJECT PATTERN`** as a boolean command (DD#63).
- **`[[ … ]]` tests** / comparisons (DD#62).
- **`-> int` function return values** (DD#55).

**A boolean (or any (B) result) must be consumed or explicitly discarded**
(DG, 2026-09-09, refining the original 2026-06-25 rule). Producing a
(B) result and then **silently discarding it is a compile error**:
a bare `match S P`, a bare `[[ ... ]]` and a bare `-> int` call are
all errors. A false boolean is not a recoverable failure; it is a
result whose use must be explicit.

This resolves the bare-`[[ … ]]` question: a bare test is **not** an
allowed assertion that aborts on false (that would be the (A) reading) —
it is a must-consume-or-discard compile error. Consume it (`if [[ ... ]]`,
`[[ ... ]] || ...`) or explicitly discard its result. The separate
statement-effect check below rejects known pointless computation even
when the discard itself is legal; `[[ ... ]] || :` is not an escape.

Enforcement codes — **kept adjacent to the existing ignored-return family**
(SB4016 = an unconsumed non-void function result; SB4017's zero-return
ceremony rule is retired by DD#55's 2026-11-06 ruling and removed in journal 577):
- **SB4018** — a (B) boolean result is produced and silently discarded (bare
  `match` / bare `[[ … ]]`).
- **SB4019** — invalid `-`/`!` composition, not a ban on discarding a
  (B) result. The unrelated variable/function collision also uses this
  code. The new pointless-statement diagnostic is not yet assigned.

The `$?`-staleness diagnostic SB5012, reading a stale status, is a
separate concern in the 50xx validator band.

Levers:
- **`-` explicitly discards** an otherwise discardable result, without
  reaching into operands or called bodies (DD55).
- **`if` / `while` / `&&` / `||` consume a status**, not an integer
  value. `cmd || :` can discard a status indirectly; the same
  statement-effect rule applies as for `- cmd`.
- **Known pointless statements are an error**, including result-only
  unused computation and an explicitly empty word-list `for`.
  Intentional `:` remains the deliberate no-op.

## Original distinguishing principle (superseded)

> `-` is for **opaque externals** (`grep`, whose no-match→1 you cannot
> change — you suppress the verdict). For **in-process logic you control**
> (`match`, a comparison, your own `-> int` function), you **restructure**
> (`if` / `||` / `|| :`) rather than suppress. — DG

This was the rationale for the original prohibition. DG replaced it
with explicit result discard on 2026-09-09, then separately required
the conservative statement-effect check on 2026-11-08.

## Addendum (2026-09-09) — `-` discards ANY result

DG: *"I don't like the asymmetry of `- cmd` and boolean `|| :`. `-` should
work for both."* Approved after discussion. This **supersedes** the "No `-`
suppression (→ SB4019)" lever above and the distinguishing principle that
justified it.

`- X` now uniformly means **"run X, discard its result"**, whether X is an
opaque external (A) or in-process logic (B). Boolean tests and non-void
calls are eligible for explicit discard; eligibility does not guarantee
that a whole statement is useful. The result/context restrictions below
and the separate statement-effect check both apply.
`- ! cmd` is SB4019 again (addendum 2026-10-26).
(**Under question, 2026-10-28:** `- f` on a user-function call; see
`notes/tasks/q-suppression-on-user-functions.md`. **Ruled 2026-10-29/30,
landed by F6 (DD#55 addenda 2026-10-30):** `- f` stays eligible for discard
when f returns an int or a status, is SB4076 on a void one, and discards
only f's own answer, not the failures inside its body. The separate
statement-effect check still applies.)

**Command adjacency (DG, 2026-11-08; DD55).** The `X` being prefixed is
one command or call, not an assignment or a whole logical list.
`- x=$(cmd)` is rejected; write `x=$(- cmd)`. In `- a || b` the prefix
belongs only to `a`, never to `b`. Enforcement of adjacency is queued in
`reject-nonadjacent-suppression`; result-kind, no-reach and fatal-fault
rules remain unchanged.

**Discard is not a status producer (DG, 2026-11-08; DD55).** A directly
suppressed result cannot feed a condition, an `&&`/`||` decision or a
DD74 status return producer. This rejects `- a || b` and capture
conditions that consume a discarded status, rather than inventing success
or exposing the discarded result. An all-suppressed pipeline has no status for
such a consumer. Partial pipelines still supply their unsuppressed-stage
aggregate. A final `a || - b` at statement level remains legal.
`reject-discarded-status-consumers` owns enforcement; Q3 in
`q-suppression-and-stage-residuals` is settled (journal 589).

**A consumed value is not a spare status (DG, 2026-11-08; DD55).**
Reject `$(- toupper abc)` and other value-intrinsic captures with no
separate status. The capture already consumes the value; the prefix
discards nothing. `getenv`'s lookup status remains a distinct channel.
Ordinary `- int_fn` and `- wait` still discard their actual values,
rather than capture them. Fatal faults do not create a discardable
status. `reject-statusless-intrinsic-suppression` owns pending enforcement
(journal 590).

**A control transfer is not a discard site (DG, 2026-11-08; DD55).**
Reject source `-` on `return`, `exit`, `fail`, `break` and `continue`,
including through redirects. It neither discards an outgoing return
value nor makes a terminal or loop transfer resumable. Ordinary `- f`
at a non-void call site remains eligible for discard; no-reach and
result kinds stay.
`reject-control-transfer-suppression` owns pending enforcement (journal
591). DD31 separately rejects arithmetic prefixes (journal 593): a
mutation has no result, and a comparison cannot become an arithmetic
statement by discarding its result. The statement-effect rule below
preserves intentional no-ops without exempting pointless computations.

### Pointless statements (DG, 2026-11-08 and 2026-11-09)

DG proposed keeping discard legal while giving an error for a pointless
statement, and approved the conservative effect-based rule:

**Reject a statement whose only work is computing an ignored result,
when the absence of other observable effects is known.** This is an
error about the statement, not about `-` being an invalid discard
operator. Do not silently delete it or turn a discarded false result
into an assertion failure.

The principle is not limited to computations that produce an unused
answer. DG also rejected an explicitly empty word-list `for` on
2026-11-09: it has nothing to iterate, so the body cannot do any work.
DD68 requires `in` and at least one source item, and rejects `;`
before `in`. This is a source-header rule; effects written in an
unreachable body cannot rescue it, nor do outer marks/redirects make
the header legal. A written item such as `""` is not an empty list,
and data-dependent runtime emptiness remains ordinary iteration.

For otherwise legal string operands:

```sash
- [[ $s == yes ]]        # reject: only the discarded comparison
[[ $s == yes ]] || :     # reject in statement position for the same reason
- match "$s" "^a" hits   # retain: populates hits
- : >optional.log        # retain: file operation
```

Inspect the whole unused statement, not just its prefix. Routing an answer
through a condition solely to choose no-ops does not give the computation
a purpose. Equivalent handling spellings follow the same rule. Using a
result to choose observable work or supply a return or stored value is
different; this does not require proving that every stored value is later
read.

Writes, I/O and observable effects in operands or called bodies prevent
this rejection. Uncertain effects do too: do not infer that a command or
call is harmless merely because its result is ignored. Account for
redirects, nested value/capture evaluation and lazy control flow. Capture
buffer eligibility is not proof of effect freedom.

Only failures that remain observable can count. A recoverable regex
failure that `-` also discards is not a purpose for the statement: this
DD's diagnostic contract makes handled failures silent. A checked
operand failure, such as a `:?` abort, is different because no-reach
leaves it observable. Incidental allocation and possible OOM do not
justify otherwise pointless calculation. None of this changes DD130's
failure classification or suppresses fatal faults in retained programs.

**Explicit `:` remains the intentional no-op**, including its otherwise
legal `- :` form. It still evaluates arguments and follows redirects
(DD69). It is permitted without effects, not classified as an effect
that can rescue an enclosing `test || :`.

**Constant-returning functions remain legal.** The check concerns the
use of an invocation, not the values a definition may return. A known
effect-free `- f` can be pointless even though f is non-void; using f's
result remains legal, as does discarding a result when the invocation has
other effects. No same-result check or SB4017 ceremony is reintroduced.

This is a new rejection, not permission to bypass existing syntax,
typing, placement, taint, result-channel or status-consumption rules.
In particular, it does not relax DD31's arithmetic placement discipline
or legalize `-` on a void call, control transfer or no-result write.
DD31's 2026-11-08 ruling rejects arithmetic comparison/test statements
even when operands have effects; the context rule is not an effect
analysis. Other eligible boolean discard retains the rule above.

**Enforcement pending:** `reject-pointless-statements` owns the
result-only effect/use check, its diagnostic and directly affected
advice/specs. `q-for-without-in-list` separately owns the word-list
header refusals, including the explicitly empty list (journal 598).
Neither guard's refusal is evidence that the other works. SB4018 must not
recommend `-` or `|| :` as an unconditional escape for a pointless test.
No diagnostic number or implementation architecture is selected here.
Q5 in `q-suppression-and-stage-residuals` is settled as policy; journal
592 records the decision.

### Why the original rule did not hold up

Two observations motivated the 2026-09-09 change; they do not exempt a
statement from the separate 2026-11-08 effect check.

1. **`|| :` was always allowed.** So the result was already discardable, and
   the rule never protected it — it only dictated *which* explicit discard the
   author had to write, and mandated the more indirect one. `cmd || :`
   expresses discard as a side effect of short-circuit control flow; `- cmd`
   says it directly. What SB4018 actually guards against is the **silent**
   discard — a bare `[[ -n $x ]]` written as if it asserted, the real bash
   habit — and `-` is not silent.

2. **(B) gained members whose primary effect is a side effect.** When this DD
   was written, (B) was bare `match S P`, `[[ … ]]`, and an `-> int` return —
   treated as *purely* about their result, where "you authored the logic,
   say what the result means" seemed compelling. `match S P ARR` and
   DD#63's `matchall`
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
- **`- a | b` suppresses only the first segment** (DD#55). Neither
  `- !` nor `! -` is legal; the former negated-pipeline exception was
  retired on 2026-10-26.

### Retired

**SB4019 no longer means "`-` on a must-consume result".** Its remaining uses
are the `-`/`!` composition error and the (unrelated, pre-existing)
variable/function collision.

Specs: `regex.sh` 63.2f, 63.cap7, 63.dash1-dash3; `errors.sh` e.b70.tdash,
bng.5, bng.11.


## Redirect failures follow their recovery class (DG, 2026-11-06)

DD130 replaces the rule that a redirect open was excluded from `-` merely
because it happened before the command. A path-specific open failure is
part of the owning operation's recoverable failure surface:

- Unchecked, it follows the ordinary implicit abort.
- In a legal condition, it supplies the failed operation's result.
- Under a legal `-`, it is discarded and execution continues.
- A classified fatal cause still aborts under any of those forms.

A failed open must prevent the command from running. Never discard the
open's `-1` and let output go to the original stdout. Restore scoped
descriptor state before continuation; stop at the first failed redirect,
and report the actual failing path. Earlier file side effects are not
rolled back.

This covers the attached redirect, not a failure inside its operand
capture or a called body. It does not change function result kinds.
Descriptor integrity and resource-error classification are obligations,
not a blanket exemption for redirects or backup/restore machinery.

**Implementation is pending** in `runtime-failure-classification` and
`recoverable-file-failures`. The old statement-level `- echo > /bad`
abort remains a compiler behavior to repair, not the accepted rule.

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

DD55's accepted 2026-11-08 direct-exec rule removes the implicit checked
wrapper from a single-external-command background job. After successful
exec, no sash process remains there to emit a completion diagnostic;
neither does the parent's `wait` emit one. A user function backgrounded
explicitly retains ordinary body checks and their diagnostics.
`background-external-direct-exec` owns this pending change.

`<reason>` is the OS `strerror` text (ENOENT "No such file or directory",
ENOSPC "No space left on device", …) via a hand-rolled errno→string table
(`sash-rt` is `#![no_std]`, no libc). The abort-site diagnostic fires
**only on the actual abort** — a *caught* (`if`/`||`) or `-`-*suppressed* failure prints
**nothing** (it is handled, not fatal). Implementation lives at the codegen
abort site, not in the syscall wrapper, precisely so caught failures stay
silent. The existing child-owned exec errno report is separate: if exec
itself fails, the child reports that error and exits 127, including
without a wrapper (`bg-exec-errno-message`, resolved 2026-07-08).

## Conditional blocks are `if` sugar (DG, 2026-11-09)

DG retired general command groups on 2026-10-30, retaining the failure
block as an `if` shorthand. He has now accepted the symmetric success
block:

```sh
cmd && { a; b }          # exactly: if cmd { a; b }
cmd || { a; b }          # exactly: if ! cmd { a; b }
```

The braces delimit an `if` body, not an independent command-list
operand. This does not restore standalone `{ a; b }`, a group under
`!` or `-`, a group return producer, or a group used as a stage,
background job or redirected command. Use ordinary statements, an `if`,
or a named function as appropriate. Ordinary function, control-flow,
capture and dispatch bodies remain.

The corresponding `if` owns scope, checking, control transfers and
status availability (DD55). DD95 still rejects mixing `&&` and `||`
in one list; the body's own lists are separate, just as in an explicit
`if`. DD68 now also settles that `if` and loops are resultless
statements: neither a direct construct nor its shorthand supplies a
value or status to consume. An otherwise legal unconsumed final action
remains a statement (DD74); body failures keep their normal checks.
`q-compound-as-condition` owns that pending enforcement (journal 597).

**Implementation is pending** in the existing P3
`retire-command-groups`; no new diagnostic code is allocated here.
Journal 596 records the ruling.

## Relationship to other decisions

- **DD#55** — parent: implicit checks, `-` suppression, `$?` scoping,
  void/non-void functions, pipelines, background. This DD adds the
  (A)/(B) taxonomy and the diagnostic contract on top.
- **DD#25** — arithmetic overflow is its own abort mechanism; it belongs
  to (A) for diagnostic purposes (`<argv0>: arithmetic overflow`).
- **DD#56 / DD#128** — an uncaught failure reports its diagnostic, then
  calls ERR if declared. Orderly termination and configured EXIT signals
  select EXIT instead. Neither handler runs in a forked child.
- **SB4016 / SB4018 / SB4019** — the "ignored result/value"
  family (DG: keep these adjacent). SB4016 = an unconsumed non-void function
  result; SB4018 = silently discarded (B) boolean (bare
  `match` / bare `[[ … ]]`), SB4019 = invalid `-`/`!` composition. Distinct
  from SB5012 (`$?`-staleness — reading a stale *status*, 50xx band).
- **SB4017** — the former int-only zero-return check, retired by DG on
  2026-11-06. Do not replace it with a same-result check or repurpose its
  code. Source removal shipped in journal 577 (`retire-return-ceremony-check`).

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

## Addendum (2026-09-24) — `|| :` is not a discard for an `-> int` function

Implementing DD#74's boolean contexts in full surfaced a collision with this
DD. DD#74 enumerates *"`if f`, `elif`, `while f`, `until f`, `! f`, and `f` as
an operand of `&&`/`||`"* as the positions where a function is a **predicate**
— something branches on its status. An `-> int` function's `return N` is a
**value**, and a nonzero value is a **false** status, so using one as a
predicate is a category error (SB4030).

That makes `f || :`, originally offered as the discard, a boolean context.

Two things were already true before this was noticed:

1. **The original distinguishing principle above was already partly dead.**
   It tells the author to restructure in-process logic, *"your own `-> int`
   function"* included, with *"`if` / `||` / `|| :`"*. SB4030 has rejected
   `if f` for as long as it has existed, so that advice could not be followed
   for the very case it names.
2. **The 2026-09-09 addendum already preferred the alternative.** `-` was
   added because *"`cmd || :` expresses discard as a side effect of
   short-circuit control flow; `- cmd` says it directly"*, and it explicitly
   legalised *"`- f` for an `-> int` function"*.

**DG's call (2026-09-24): take the rejection.** `f || :` is now SB4030 when
`f` is `-> int`. The replacement already existed and is the direct spelling:

    - f            # run f, discard the int
    int n=$(( f )) # run f, keep the int

**This type restriction concerns `-> int` functions.** `|| :` remains
result-kind-legal for `match` / `matchall` / `[[ ... ]]` and opaque
externals. Those produce a status, which is what `||` consumes. The
2026-11-08 statement-effect rule separately rejects known pointless
uses of either discard spelling.

The original principle's *"`if` / `||` / `|| :`"* advice is replaced by
`$(( f ))` / `-` for an `-> int` function, subject to that effect check.

Specs: `e.b70.fndisc` (the rejection), `74.47`/`74.48` (the two replacements).

DG retained non-void call-site discard on 2026-10-29/30 (DD55; F6).
Q2 in `notes/tasks/q-suppression-on-user-functions.md` is settled.

## Addendum 2026-09-25 — multiple redirects are left-to-right, stop-at-first-failure

The setup-failure contract above (`<argv0>: <path>: <reason>`, command fails
without writing) was written for *a* redirect. It said nothing about a command
carrying several, and both backends got that case wrong in the same two ways.

**Ratified (DG):**

1. **Redirects are performed left to right, and processing stops at the first
   failed `open(2)`.** A redirect after a failed one is not attempted. This
   matters beyond tidiness: an eager `open` *creates or truncates* a file bash
   never touches, so `cmd > /nonexistent/x > keep.txt` was clobbering
   `keep.txt`. The redirect list is a sequence with side effects, not a set.

2. **The reported path is the redirect that actually failed** — necessarily the
   first one, given (1). Previously the accumulator kept the first
   *open-candidate* path, so `> good.txt 2> /nonexistent_d/bad` reported
   `good.txt: No such file or directory` about a file that had opened fine.

(2) falls out of (1) by construction: once only one redirect can fail, there is
no "which path?" left to get wrong. That is why the fix is a short-circuit in
both backends rather than a smarter choice of path — native branches on the
accumulated failure flag (`emit_guarded_redirect`), C guards each call with
`?:`, which short-circuits natively.

Specs: `rd.1` (the path named is the one that failed), `rd.2` (a redirect after
a failure creates no file), `rd.3` (a first-redirect failure is still reported),
`rd.4` (two good redirects both apply).

**Not settled here:** *where* the diagnostic is written when the redirections
applied so far include stderr. bash writes it through them; the C backend
matches bash; native restores the fds first and does not. Filed as
`notes/tasks/native-redirect-diag-after-restore.md`.

## Addendum (2026-10-26, DG): `- !` is SB4019 again; an unused terminal `!` is SB4018

**`- !` (DG).** The 2026-09-09 addendum made `- ! cmd` legal by name. DG: *"I
don't see how `- ! cmd` makes any sense"*, and ruled it SB4019 everywhere. `-`
discards the status `!` inverts, so `- ! cmd` ran exactly as `- cmd`: the
reason 2026-09-09 kept `! - cmd` rejected ("`!` has nothing left to invert")
holds for its mirror. Both orders are SB4019 again, as 2026-08-31 had them, at a
statement's head and on a pipeline stage. `a | - ! b`, `- ! { … }` and
`! - a | b` in a condition used to get through or draw the wrong code. The
message ends "Write `- cmd` without the `!`". 2026-09-09's note that a negated
pipeline keeps its `-` outside no longer has a case to describe: no `-` body
starts with `!`, so `push_suppression_into_pipeline`'s negated arm is an ICE.

**SB4018's advice for `!`** said "discard with `-` or `|| :`", which spelled
`- ! cmd` (now SB4019) or `! cmd || :` (`- cmd`, said indirectly). It now says
"or write `- cmd` without the `!`". The advice for the other (B) results
still recommends both discards. The 2026-11-08 statement-effect rule
requires qualifying that advice: legal discard does not rescue a known
pointless test. `reject-pointless-statements` owns the pending message
change; the discard operators themselves remain available.

**A terminal `!` that nothing consumes is SB4018 (decide-and-report).**
2026-08-31 lets the final stage take "the same `!` a standalone simple command
takes", and a standalone `! cmd` that nothing consumes is SB4018 (2026-07-24).
A terminal `!` escaped that rule: `a | ! b` as a statement compiled, and when
`b` succeeded the program aborted with "b: command failed (exit 1)", the bug
2026-07-24 fixed for `! cmd`. It is now SB4018 at statement level, under `&`,
and as a capture's last statement. A condition and a return producer consume it
as before, and a `!` inside a `-` scope (`- { ! cmd }`) is discarded by the `-`.

Specs: errors.sh e.b70.ndash and bng.5 (now SB4019; ids kept), bng.13-bng.20;
pipe_functions.sh pf.65, pf.66 (a negated final reader); bng.7-bng.9 still
pass. Journal 537.

## Addendum (2026-10-26): `!` and fd tags no longer hide a redirect, pipe or `&`

A redirect, pipe or `&` after a `!`-negated form gets the verdict it gets
without the `!`. The `!` used to hide its operand from those checks. A redirect
on a negated `[[ … ]]`, `(( … ))`, assignment, declaration or `attest` is SB4046,
since none of them produces output (rdr.50-rdr.57: as a statement, in a
condition, as a final stage and as a return producer). A redirect on a negated
`{ … }` is SB4021, with or without an fd tag (csfx.10, csfx.20). Each of these
crashed the compiler. `&` on a negated group is DD#55's SB4024 (csfx.4-csfx.8,
DD#55 addendum 2026-10-26).

A return producer hid a comma list (DD#31) the same way, and a pipe or a
redirect on one crashed the compiler. The list holds a write, so each now gets
an assignment's answer: SB4022 for the pipe (csfx.9) and SB4046 for the
redirect (csfx.11). DD#31's addendum of 2026-10-27 explains why the list is not
a compound.

An fd tag hid a redirect from the same checks. They saw only operator tokens,
so in `{ … } 2>f` the `2` began a new statement: SB5011 "command '2' not
found" at statement level, SB4022 on a last stage, and a parser ICE after a
`-`. `2>f` on a compound now gets `>f`'s verdict, code and position wherever
the compound stands (csfx.12-csfx.18). A digit word with a blank before the
operator is not a tag, so `{ … } 2 >f` still starts a new statement at the
`2` in that historical measurement (csfx.19). The source group now
requires rejection under `retire-command-groups`. DD71's 2026-11-09
boundary rule allows a following statement after an otherwise legal
completed construct without another separator; it does not make `2`
a valid command. `q-statement-form-residuals` owns that non-group
boundary rule, not preservation of this retired-group fixture.

At this implementation's landing, SB4023 still did not see through `!`:
`! { /bin/false } && echo y` printed `y` (bng.22), where `{ /bin/false } &&
echo y` was SB4023. The accepted command-group retirement above now
requires the negated group to reject; enforcement is pending in
`retire-command-groups`. A negated loop also slips past SB4023 (c4);
DD68's no-result rule now requires its rejection too, with enforcement
pending in `q-compound-as-condition`. The former exception is not an
ongoing language permission.

Specs: errors.sh rdr.50-rdr.57, csfx.9-csfx.20. Journal 537.
