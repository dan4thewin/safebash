---
title: DD#89 — `exit` accepts any int value/variable/return; full i64 to END, 8-bit at the process boundary
status: accepted
---

Settled with DG (2026-09-03). `exit`'s argument is an ordinary integer sink:
**any int — a literal, a variable, an arithmetic result, or an int-returning
function — is a valid `exit` argument**, with no compile-time range restriction.

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
  "every other int context" claim is false; see the amendment below.** `exit
  $s` does still coerce today, but that is a measured fact about the
  implementation, not a rule this DD settled.

The prior type-check reject of `exit $intvar` ("value may exceed 0-255; use
`exit $((v<256?v:1))`") is **removed** — clamping is the programmer's choice,
not a compiler mandate.

## Value semantics

- **Internally the value is a full i64.** `exit N` stores the untruncated value
  in the exit-status channel, so an END block reads the exact value via `$?`
  (e.g. `int code=$?`) — `exit 300` → END sees `300`, `exit -1` → END sees `-1`.
- **At the process boundary it is 8-bit (POSIX).** The final `exit_group(status)`
  passes only the low byte to the parent's `wait()`: `exit 300` → parent sees
  `300 & 0xFF = 44`; `exit -1` → `255`; `exit 256` → `0`. Same as bash.

So an out-of-range value is faithful *inside* the program (and to END) and
truncated only when it actually leaves the process.

## What this enables (but does NOT mandate)

Because the full value reaches END, a program *may* use an out-of-range exit
code as a **tag** for "which `exit` preceded END", dispatch on it in END
(`int code=$?; if (( code > 255 )) { … }`), and re-exit with a real 0-255 code.
DG (2026-09-03): this is **not** prescribed over just using a global variable —
"it's up to them." The DD blesses the *mechanism* (any int works, value is
faithful to END), not a particular idiom. A plain global read in END is an
equally valid way to convey an exit-site marker (it keeps the exit code a clean
0-255 but only covers explicit exits, not aborts).

## Implementation

- **Native codegen** (`codegen/dispatch.rs::emit_exit`): evaluate the argument
  with `lower_value_as_int` (was literal-only, defaulting any non-literal to 0).
- **c backend** (`emit_c.rs::emit_exit`): already used `lower_value_as_int` — no
  change.
- **type_check**: `check_exit_arg` (the `exit $intvar` reject) removed.
- **Process exit**: `sash_rt::sash_exit(i64)` → `exit_group(status as i32)`; the
  kernel truncates to 8 bits for the parent. (No change — this is the POSIX
  boundary that makes >255/<0 "internal only".)

## SB codes

None. This REMOVES a type-check diagnostic (the `exit $intvar` reject); no new
codes.

## Related / open

- DD#56 — `$?` in the END block is the triggering status (the channel that
  carries an out-of-range tag into END). journal 237.
- `bug-emit-c-exit-skips-end` — the c backend's main-body `exit N` skips END
  (native routes through it); orthogonal to this DD but on the same path.
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
a valid `exit` argument, no compile-time range restriction, full i64 to END,
8-bit at the process boundary. None of that depended on the struck clause.

**Left open:** whether `exit $s` / `return $s` *should* reject. That is a
behaviour change, not a correction, and it is DG's call. The direction is
already set — DG (2026-09-25): *"Rather than prioritize bash consistency, I
want to prioritize sash self-consistency"* — which means bash agreeing with
the coercion at `wait` and `[[ -t ]]` is not by itself a reason to keep it.
The open sites are tracked in
`notes/tasks/undispositioned-string-to-int-sites.md`; `${a[$s]}` and `a[$s]=`
were closed by DD#80's 2026-09-26 amendment, leaving `exit`, `return`, `wait`
and `[[ -t ]]`. (`$(arg $s)` is masked by SB5020 — taint fires first — so it
is untested rather than settled.)

DG has also raised whether **`-t` should stop being a test operator at all**
and become a distinct predicate (`if $(terminal? $fd)`), following SB5019
having already moved the `-eq` family into `(( ))`. That would retire one of
the four sites by removing the surface rather than tightening it. Not decided.

### Related

Journal 411 records the casualty this clause's thinking produced: `return`
lowered its value **as a string** and ran the lenient `sash_atoi`, never
consulting the type, so `return ${s:?bad}` — the spelling SB4030 itself
recommends — compiled to nothing and yielded 0 on native while the c backend
aborted correctly. An explicit "abort if this is not an integer" guard that
does nothing is worse than the coercion it was meant to replace. Fixed there;
the surface question above is what remains.
