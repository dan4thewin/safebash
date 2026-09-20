title: DD#25 — Arithmetic Overflow Traps
status: implemented

Integer overflow traps at runtime (not silent wraparound). Call depth
limited to prevent stack exhaustion.

## Implementation (corrected 2026-06-26)

`+`, `-`, `*` trap on runtime overflow via cranelift `sadd_overflow` /
`ssub_overflow` / `smul_overflow` (native) and `__builtin_*_overflow` (C),
both calling `sash_arith_overflow` → `<argv0>: arithmetic overflow`,
exit 2. The trap is unconditional — a `-` (suppress) prefix does **not**
suppress it. Constant folding (`fold_arith_op`) uses checked arithmetic
and defers a compile-time-known overflow to the runtime trap rather than
wrapping. See `notes/journal/120` and `bug-runtime-arith-overflow-wraps`.

## Amendment (2026-09-14): every arithmetic fault aborts

A zero divisor now aborts like the rest: `sash_arith_divzero` →
`<argv0>: division by zero`, exit 2, in both backends. It previously
wrote bash's `division by 0 (error token is "0")` to stderr, yielded
`0` and kept running — the only arithmetic fault in the language that
produced a value and continued. `i64::MIN / -1` (the other x86 `#DE`
cause) was already `sash_arith_overflow`, so the same hardware fault
was disposed of two opposite ways fifteen lines apart in one function.

The explicit `rhs == 0` check is **deliberate, not a fallback.**
Letting the hardware `#DE` fire instead would report `Floating point
exception` — no `argv0`, and "floating point" for an *integer* divide —
and would re-split `i64::MIN / -1` back out of the message path.

Measured, for reference: C never wraps and never yields a value either.
gcc emits `ud2` as the *entire body* of a function containing a literal
`1/0` (SIGILL, rc 132) and a raw `idiv` for a runtime zero (SIGFPE, rc
136). sash was the outlier, and in the opposite direction — its literal
case was the silent one.

### A folder-proven zero divisor is a compile-time error

**SB5077.** When the constant folder can prove the divisor is zero, the
compiler already knows the program cannot run, so it says so instead of
emitting a binary that aborts. Reach is exactly what the folder proves —
literals and operators over literals, through parens (`$(( 1 / (2-2) ))`)
— because `check_arith_div_by_zero` in `validate.rs` calls the
transform's own `fold_arith_op`, not a second copy of the semantics.

There is **no const-propagation through variables**, so `int z = 0;
$(( 1 / z ))` is not provable and still aborts at runtime. The two paths
are therefore complementary, not redundant, and neither can be removed.

The check is wired into both the `$(( … ))` value path
(`check_arith_sexpr_arity`) and the `(( … ))` statement/condition path
(`validate_arith_context`) — a rule answering to only one of the two
spellings would be the same spelling-dependent inconsistency the
amendment exists to remove.

It also descends into ternary arms, so `$(( 1 ? 5 : 10/0 ))` is
rejected even though that arm is never evaluated (bash prints 5). A
literal fault written in dead code is still a fault; compare SB5050,
which already rejects a dead variable.

Keeping a literal `/0` out of the emitted C matters independently:
see the gcc `ud2` behaviour above.

**Note this is the first compile-time arithmetic error.** The
constant-folding rule above defers a proven *overflow* to the runtime;
a proven zero divisor does not follow it. The difference is that the
folder proves overflow only for the operands it was given, whereas a
zero divisor is a property of the expression as written.

#### Addendum (2026-09-18): the folder's reach now includes unary operators

The reach stated above — "literals and operators over literals, through
parens" — was not what the code did. `const_eval_arith` had arms for
`Literal`, `Paren` and `BinOp` and fell through to `None` for everything
else, so `ArithExpr::UnaryOp` was unevaluable and **any divisor containing
a unary operator was not provable**. Measured over 18 spellings: every
binary form was caught (`2 - 2`, `2 * 0`, `1 % 1`, `0 << 2`, …) and every
unary form escaped (`1 + -1`, `-1 + 1`, `-0`). The same fault, reported at
compile time or at runtime, decided by whether a minus sign appeared —
exactly the spelling-dependent inconsistency this amendment exists to
remove, reintroduced one level down.

This was an oversight, not a scope decision: `const_eval_arith`'s own doc
declines "variables, expansions, calls", and a negated literal is none of
those. Folding a `UnaryOp` is safe because it carries no side effect —
`x++` and `++x` are the separate `PostIncDec`/`PreIncDec` variants and stay
unevaluable.

The fix follows the rule the section above already states. The binary
semantics were shared with the transform precisely so the two could not
drift; the *unary* semantics were not — `try_fold_unop` worked on
`SsaValue` strings while `const_eval_arith` worked on `i64`, which is how
they drifted in the first place. The i64 core is now `fold_unary_op`, and
both call it. `Neg` uses `checked_neg`, so `-i64::MIN` declines to fold and
defers to the runtime trap, consistent with the constant-folding rule for
overflow above.

Specs `cov.309`-`cov.313`; the non-zero unary divisor (`5 / -1`) is pinned
alongside so the wider reach cannot start over-rejecting.

#### Addendum (2026-09-20): the gap list was stale; only `Shl` is left

The paragraph here used to read "`Pow` saturates rather than traps; `Shl`
(shift ≥ 64) and unary `Neg` (i64::MIN) are not yet trap-with-message."
Measured against the live binary, two of those three were already fixed, and
the specs recording the fixes say so in their own comments — `25.9` is
annotated *"(was: saturated to i64::MAX silently)"* and `25.11` *"(regression:
-x → 0 - x → ssub_overflow)"*. The DD was never updated to match. A
"follow-up" that is already done is worse than no note: it is a work item
someone picks up and finds finished.

Current state, all verified by execution:

| op | behaviour | spec |
|---|---|---|
| `+` `-` `*` | trap, exit 2 | `25.8` |
| `/` (i64::MIN / -1) | trap, exit 2 | `25.10` |
| `/` `%` (zero divisor) | trap, exit 2 | see amendment above |
| `**` | **traps**, exit 2 | `25.9` |
| unary `-` (i64::MIN) | **traps**, exit 2 | `25.11` |
| `%` (i64::MIN % -1) | `0` — not a fault | `25.12` |
| `<<` | masks the count, **matches bash** | `25.13` |

**`Shl` was the only one left — settled below.**

The janet sentence that used to close this section is also gone: the janet
backend was removed by DD#104, which takes `bug-janet-int-overflow` and
`janet-arith-divzero` with it. The overflow specs no longer need
`in_modes native,c` and do not carry it.

#### Addendum (2026-09-20): shifts take an unbounded, non-negative count

**DG's ruling.** The shift table above listed `Shl` as the last open item.
DG's framing settled it: a shift is *dependent on the internal representation
and thus not truly mathematical*, and **shifting in zeroes is what is
desirable in practice**. Measuring against that framing showed the single word
"shift" was hiding three separate questions, only two of which were really
about representation.

**1. Count masking — changed.** The hardware masks the count to 6 bits, a
property of the register width and not of the arithmetic. It made `1 << 64`
yield `1`, `1 << 200` yield `256`, and `8 >> 64` yield `8`. Those are wrong
answers that look right, which is the same class of silent-wrong this DD
already rejects for overflow — a dynamically computed mask width of 64
silently returned the unshifted value. The count is now unbounded: **a count
>= 64 shifts every bit out.**

**2. Negative count — changed, and it was the worst of the three.** Masking
reinterpreted a negative count as a large positive one, so `8 >> -1` was `0`
and `8 >> -64` was **`8`, an identity**. A negative shift count is meaningless
in any model. It now aborts: `sash_arith_negshift` → `<argv0>: negative shift
count`, exit 2, consistent with "every arithmetic fault aborts".

**3. Sign fill on `>>` — deliberately NOT changed.** This is the one that
looks like it should follow "zeros in" and does not. On a *signed* type the
sign fill is the **less** representation-dependent choice: `-8 >> 1` is `-4`,
which is `floor(-8 / 2)`. A zero fill would give `9223372036854775804`, which
is meaningful only if the sign bit is reinterpreted as a value bit — it
*increases* the dependence on the representation rather than removing it.
Zeros-on-right is the right rule for an unsigned type; sash's `int` is `i64`.
So a count >= 64 on `>>` **saturates to the sign**: `0` for a non-negative
value, `-1` for a negative one.

This is exactly Go's rule, arrived at independently: unbounded non-negative
counts, large counts shift everything out, negative counts are a runtime
fault.

| expression | was (bash) | now |
|---|---|---|
| `1 << 64` | `1` | `0` |
| `1 << 200` | `256` | `0` |
| `8 >> 64` | `8` | `0` |
| `-1 >> 64` | `-1` | `-1` (sign fill, unchanged) |
| `8 >> -1` | `0` | **abort** |
| `8 >> -64` | `8` | **abort** |
| `1 << 3`, `-8 >> 1`, `-9 >> 1` | `8`, `-4`, `-5` | unchanged |

**Deliberate divergence from bash**, on the same grounds as the overflow trap:
a silent wrong answer loses to a visible one. Ordinary counts — the only ones
a correct program uses — are untouched and still match bash exactly.

**Four sites had to agree** and are cross-checked by spec `25.17`:
`codegen/arithmetic.rs::emit_shift` (native), `emit_c.rs::emit_shift_c` (C),
and `transform/expr_handlers.rs::fold_arith_op`, which `validate.rs`'s
`const_eval_arith` shares. The folder **declines to fold a negative count**
and defers to the runtime trap rather than producing a value, following the
constant-folding rule for overflow stated at the top of this file.

The C guard is not only semantic: in C a shift by >= 64 or by a negative count
is *undefined behaviour*, so clamping before the shift is also what keeps the
emitted program well-defined.

Specs `25.13`-`25.17`.

**Open (not decided):** whether a *folder-proven* negative count should be a
compile-time error, the way a proven zero divisor is SB5077. The argument
applies — a literal `x << -1` is "a property of the expression as written" by
this file's own distinction — but it adds a diagnostic and was not part of the
ruling, so it is deferred rather than assumed.
