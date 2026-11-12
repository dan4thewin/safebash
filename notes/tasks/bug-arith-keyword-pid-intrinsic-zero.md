title: "bug: pid/random keyword intrinsics as arithmetic operands silently yield 0 (only `len` is wired as an arith int-intrinsic)"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "`$(( (getpid) ))` / `$(( (getcpid) ))` / `$(( (random) ))` compute the value, not 0"
  - "a keyword pid/random intrinsic as a compound operand (`$(( (getpid) > 0 ))`) computes correctly"
  - "decision: does `$(getpid)` command-substitution work inside `$(( ))` (currently SB4020), or is the `(getpid)` sexpr the only in-arith spelling? (DD#58 sexpr vs command-sub in arith)"
  - "native + c + janet; specs pin the values; no regression"

## Symptom

Inside `$(( ))`, the DD#58 intrinsic sexpr forms `(getpid)` / `(getcpid)` /
`(random)` compile but evaluate to **0** (silent-wrong):

```
$(( (getpid) ))        → 0   (should be the pid)
$(( (getpid) > 0 ))    → 0   (should be 1)
```

Separately, the command-sub spelling `$(( $(getpid) ))` is rejected outright
with SB4020 ("unexpected token after arithmetic expression").

## Root cause

`is_arith_int_intrinsic` (transform/expr_handlers.rs) recognises only `"len"`.
Every other name — including `getpid`/`getcpid`/`random` — falls through
`funccall_to_ssa` to `SsaValue::IntFuncCall`, which `inline_int_func` resolves
as a *user* int-function; with no user body it returns `Expr("0")`. So the
sexpr silently yields 0.

## Relation to the fix that surfaced this

The sibling bug — the `$$` / `$?` **sugar** specials yielding 0 as arithmetic
operands — was fixed (native `lower_value_as_int` now computes getpid /
exit-status; commit on `hand-parser`, specs arithmetic.sh 31.20–24). That fix
covers the `DollarVar` path (`$$`, `$?`). This task is the *keyword/sexpr* path
(`(getpid)`, `(getcpid)`, `(random)`), a different mechanism (int-intrinsic
dispatch), so DD#75 §1 equivalence in arithmetic (`$$` ≡ `$(getpid)`) is not
yet complete: after the sugar fix `$(( $$ > 0 ))` → 1 but `$(( (getpid) > 0 ))`
→ 0.

## Approach (sketch)

Add `getpid`/`getcpid`/`random` to the arith int-intrinsic set and evaluate
them in the transform's `try_native_intrinsic` (or a dedicated arm) so both the
int and string codegen paths compute the runtime value (the codegen
`value_handlers` already has a `getpid` arm). Decide the `$(getpid)`
command-sub-in-arith question with DG (keep rejected → sexpr-only, or accept).
`implement-getpid-builtins` is marked done for the *word* context; this is the
arith-operand gap it did not cover.

## SUPERSEDED (2026-08-14)

Superseded by `bug-arith-sexpr-intrinsics-zero`, which is the comprehensive
version: this task named only getpid/getcpid/random, but the empirical sweep
found **every** sash-native intrinsic except `len` is broken in a sexpr — the
int-returning ones silently yield 0, the string/collection ones silently yield 0
instead of a type error, plus the `random` int-typing inconsistency, the
wrong-arity `sash: bug:` abort, and the `signum` doc bug. See that task.

## Closed 2026-09-10 (backlog prune)

Bookkeeping only — closed 2026-09-10. Successor `bug-arith-sexpr-intrinsics-zero` is the live, comprehensive task.

## Addendum 2026-09-13 — reopened and closed (journal 298)

Gate 1 named `$(( (getcpid) ))` explicitly. It was **not met**. Measured at
`47a37a1`, sixteen days after this task was marked done:

```
/bin/sleep 1 &
echo $(getcpid)          -> 3827668
int n = $(( (getcpid) )) -> 0
```

`resolve_nullary_sexpr` had `getpid` → the `$$` slot and `status` → the `$?`
slot, with a comment reading "`getcpid` (`$!`) is excluded — filed". It reuses
`$!` exactly the same way; the fix is one arm.

**Why it stayed silent.** The fallback branch under those arms treats an
unknown nullary name as a *variable* reference — `$(( (x) ))` is arithmetic
grouping, which is correct and deliberate. A misspelling is caught there by
SB5031 ("assigned nowhere, possible typo"). But `validate` exempts every
intrinsic name from that check, so `getcpid` — a name it knows perfectly well
— was waved past the typo check *and* not handled by the transform, and read
an unset slot. Two passes disagreeing about whether a name is an intrinsic, and
the disagreement landing in the one branch that cannot report it.

**And why the deferral survived.** No spec was ever written for it. The
`sx.1`–`sx.11` block covers `argc`, `getpid`, `getppid`, `epochseconds`,
`status`, `random` and `signum`. `getcpid` is in the gate text and in the
symptom table at the top of this file, and nowhere in `spec/`. A gate that is
only prose is not a gate.

Now `sx.24` / `sx.25`, native and c. `sx.25` pins the sexpr against the `$!`
punctuation sugar, which always worked here — the two spellings disagreeing is
what made it silent.

