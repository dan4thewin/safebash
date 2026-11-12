title: DD#89 — `exit` accepts any int; full i64 to the handler, 8-bit at the process boundary
status: accepted

Settled with DG (2026-09-03). `exit`'s argument is an ordinary integer sink:
**any int — a literal, a variable, an arithmetic result, or an int-returning
function — is a valid `exit` argument**, with no compile-time range restriction.

DG extended the same integer domain to `fail [N]` on 2026-11-06 (DD128,
journal 575). `fail` defaults to 1 and permits zero. The explicit-termination
revision is implemented in journal 576 (`explicit-exit-and-fail`).

## The rule

- `exit 5`, `exit 300`, `exit -1` (literals, including out-of-range).
- `exit $code` where `int code = …` (an int variable).
- `exit $(( expr ))` (an arithmetic result).
- `exit $(f)` where `f` is an `-> int` function (an int return).
- `exit $?` (the current status — subject to the usual `$?` handling-position
  rule).
- No argument → `exit 0`.
- ~~A string argument coerces to int (atoi), as in every other int context;
  `exit` imposes no extra type restriction.~~ **Struck 2026-09-26 — the
  "every other int context" claim is false, and the coercion it described is
  now gone: `exit` takes int-type only. See the amendment and its resolution
  below.**
- **A string argument is rejected** (SB4030). `exit $(( ${s:?msg} ))` is the
  accountable spelling. Quoting is the int→string conversion everywhere else
  in sash, so `exit "1"` is rejected too.

## Explicit conversion at integer sinks (2026-11-06)

DG chose **"require explicit"** for the remaining return-disposition
question (journal 579). An integer destination does not itself establish
an arithmetic context. A string-valued disposition must enter explicit
arithmetic to produce an integer; `return` has no exception.

For a string `s` and an integer `n`, the accepted contract is:

| form | contract |
|---|---|
| `return ${s:-7}` / `return ${s:?bad}` | SB4030: a string-valued operand |
| `return $(( ${s:-7} ))` | explicit DD77 conversion, with default 7 |
| `return $(( ${s:?bad} ))` | explicit DD77 conversion, aborting on invalid input |
| `return ${n:-7}` / `return ${n:?bad}` | already integer-valued; no conversion required |

This applies to numeric status returns and `-> int` results alike, and
keeps `return` consistent with `exit`, `fail`, `wait`, `arg` and integer
assignment. It does not broaden string conversion at those other sinks or
change string-context disposition semantics. Already-integer expressions
retain their type; a disposition alone is not a request to stringify them.

**Implementation is pending.** Journal 577 implemented the undispositioned
return operand guard but preserved the old disposition exception.
`bug-return-status-string-operand` owns its removal and spec migration,
depending on `bug-expr-type-int-disposition` so `${n:?bad}` is not falsely
rejected. Existing arithmetic conversion behavior and assertions must be
preserved under the explicit spellings.

The prior type-check reject of `exit $intvar` ("value may exceed 0-255; use
`exit $((v<256?v:1))`") is **removed** — clamping is the programmer's choice,
not a compiler mandate.

## Value semantics

- **Internally the value is a full i64.** `exit N` or `fail N` passes the
  untruncated value to the selected handler, readable via `$?`
  (e.g. `int code=$?`).
  DD#128 selects by the operation, not by the integer:
  `exit 300` -> EXIT sees `300`, `exit -1` -> EXIT sees `-1`, and
  `exit 256` -> EXIT sees `256`. The corresponding `fail` forms supply
  those same values to ERR; `fail 0` selects ERR with zero.
- **At the process boundary it is 8-bit (POSIX).** The final `exit_group(status)`
  passes only the low byte to the parent's `wait()`: `exit 300` → parent sees
  `300 & 0xFF = 44`; `exit -1` → `255`; `exit 256` → `0`. Same as bash.

So an out-of-range value is faithful *inside* the program and truncated only
when it actually leaves the process. Signal and fall-through termination
select EXIT by cause, not by whether their saved status happens to be zero.

## What this enables (but does NOT mandate)

Because the full value reaches the selected handler, a program *may* use an
out-of-range `fail` code as a **tag** for "which `fail` preceded ERR",
dispatch on it in ERR (`int code=$?; if (( code > 255 )) { … }`), and re-exit
with a real 0-255 code.
DG (2026-09-03): this is **not** prescribed over just using a global variable —
"it's up to them." The DD blesses the *mechanism* (any int works, value is
faithful to the handler), not a particular idiom. A global can instead record
the phase reached before an exit or failure. DD#128's incoming-data rule still
applies to any potentially tainted global read in ERR.

## Implementation

Both backends evaluate the integer argument and pass it to
the runtime without narrowing. The existing runtime has separate orderly
and failure entries; the accepted DD128 revision routes `exit` to the former
and `fail` to the latter, calling the selected generated handler with the
full value. Final shutdown calls
`exit_group(status as i32)`; the kernel exposes only the low byte to the
parent. A handler's explicit exit may replace the value before that boundary.

The original DD89 change removed native codegen's literal-only evaluation
and the type check that rejected integer variables merely for their possible
range. DD#128 adds common terminal routing without narrowing that domain.

## SB codes

No range diagnostic. String operands are rejected by SB4030; integer
variables and out-of-range integer values remain valid.

## Related / open

- DD#56 / DD#128 — handler entry preserves the triggering status and
  selection is cause-based. Both backends now use generated callbacks.
- Whether to add a dedicated "exit-site" channel (like `$(signum)`) is left
  open; a plain global already suffices, so no new surface is planned.

## Amendment (2026-09-26) — the "coerces, as in every other int context" clause was false

The rule list above ends with:

> A string argument coerces to int (atoi), as in every other int context;
> `exit` imposes no extra type restriction.

DG, on re-reading it: **"plain wrong."** He is right, and the interesting part
is *how* it got here. This DD was settled to answer a **range** question —
may `exit` take a value outside 0–255? The answer was yes, and the type remark
rode along in the same sentence, unmeasured. **A DD that argues "consistent
with X" is asserting a fact about the implementation, and nothing re-checks
it.**

Measured 2026-09-26 against `761b3f4`, with `str s=abc`:

| sink | result |
|---|---|
| `int q=$s` | **SB4030** |
| `int q=abc` | **SB4030** |
| `$(( s ))` | **SB4030** |
| `${a[$s]}` | **SB4030** (since 2026-09-26) |
| `a[$s]=v` | **SB4030** (since 2026-09-26) |
| `exit abc` | **SB4030** |
| `exit $s` | accepted, atoi → 0 |
| `return $s` | accepted, atoi → 0 |
| `wait $s` | accepted |
| `[[ -t $s ]]` | accepted |

So the clause has it backwards: **nearly every other int context rejects**,
and the coercion is the exception rather than the rule. Note also the
inconsistency *within this DD's own sink* — `exit abc` is rejected as a
string literal while `exit $s` is accepted, so `exit` is stricter about a
value written in the source than about the same value in a variable.

### What this amendment does and does not settle

**Struck:** the factual claim "as in every other int context". It was not true
when written and is less true now.

**Unchanged:** everything this DD was actually settled to decide — any int is
a valid `exit` argument, no compile-time range restriction, full i64 internally,
8-bit at the process boundary. None of that depended on the struck clause.

**~~Left open:~~ Resolved 2026-09-26 — they reject.** See below.

### Related

Journal 411 fixed a lowering bug in the then-accepted
`return ${s:?bad}` spelling: `return` lowered its value **as a string** and
ran the lenient `sash_atoi`, so native ignored the disposition and yielded
0 while the C backend aborted correctly. That repaired execution, not the
consistency of the conversion surface. The earlier claim here that SB4030
recommended the bare disposition was wrong: its advice includes `$(( ))`.
The 2026-11-06 ruling above requires `return $(( ${s:?bad} ))`.

## Resolution (2026-09-26) — every site closed, none by coercion

DG settled the question the amendment left open, and settled it the same way
in every case: **by rejection**. Nothing was made to coerce.

| sink | was | now | closed by |
|---|---|---|---|
| `exit $s` | atoi → 0 | **SB4030** | int-only sink (`a875d58`) |
| `return $s` | atoi → 0 | **SB4030** | return operand guard (journal 577) |
| `wait $s` | accepted | **SB4030** | int-only sink (`a875d58`) |
| `$(arg $s)` | **argv[0]**, silently | **SB4030** | int-only sink (`a875d58`) |
| `[[ -t $s ]]` | accepted | **SB5028** | the operator is retired (journal 418) |

**Return-row correction (2026-11-06):** the earlier attribution to journal
411 was wrong; it fixed disposition lowering, not undispositioned operand
checking. Journal 577 supplies that guard, including empty and quoted
strings. It left `return ${s:-N}` / `${s:?message}` conversions unchanged.
Their policy is now settled by the explicit-conversion rule above; removing
the exception and the remaining numeral/producer work stay in
`bug-return-status-string-operand`.

Three things are worth keeping from how this closed.

**`$(arg $s)` was the worst of them, and the amendment could not see it.**
The table above recorded it as "masked by SB5020 — untested rather than
settled", which was the honest reading at the time. Measured properly it did
not coerce to 0 like its neighbours; it silently yielded **argv[0]** — a real
value, plausibly correct-looking, with no diagnostic. Declining to record it
as closed is what left it open to be measured later.

**`-t` needed no rule at all.** It was the sole int-typed unary test in a
`[[ ]]` family that is otherwise entirely string and file predicates, and the
only unary arm whose codegen had to `atoi` its operand first. DG retired the
operator, which removed the sink along with it — the surface-removal option
this section had recorded as "not decided". The `terminal?` predicate
replacement was considered and **dropped as YAGNI**: sash arithmetic has no
unary predicates at all, so it would have been the first of its kind, in a
language whose predicate context (`[[ ]]`) it would have been declining to
use.

**A traversal bug had to be fixed first.** `check_command` was reachable only
from `check_stmt`, so operand checks ran on a command substitution in a
command's *arguments* but never on an *assignment RHS*: `echo $(arg $s)` was
checked, `str t=$(arg $s)` was not. Any rule written here before `6013848`
would have been vacuous in half its cases.

The direction DG set — *"prioritize sash self-consistency"* — is what the
outcome reflects: bash agrees with the coercion at `wait`, and that was
correctly treated as no reason to keep it.
