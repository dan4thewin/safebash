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

**Known gap (follow-up):** `Pow` saturates rather than traps; `Shl`
(shift ≥ 64) and unary `Neg` (i64::MIN) are not yet
trap-with-message. `Div` (i64::MIN / -1) is **no longer a gap** — it
traps with a message, verified. The janet backend uses float/bignum and
does not trap (`bug-janet-int-overflow`, wontfix), so overflow specs are
`in_modes native,c`; janet's disposition for the zero divisor is
deferred (`janet-arith-divzero`, DG: "TBD much later").
