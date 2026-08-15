---
title: "sash-native intrinsics silently yield 0 in arithmetic sexpr `$(( (f …) ))` — only `len` works"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "int-returning intrinsics compute their value in a sexpr: `$(( (argc) ))`, `(getpid)`, `(getppid)`, `(getcpid)`, `(epochseconds)`, `(random)`, `(status)`, `(signum)` — all currently 0"
  - "string/collection intrinsics in a sexpr are a TYPE ERROR (not silent 0): `(toupper s)`, `(tolower s)`, `(join a)`, `(keys m)`, `(basename p)`, `(dirname p)`, `(sub …)`, `(gsub …)`, `(match …)`, `(getcwd)`, `(getenv N)`, `(arg N)`, `(itod …)`"
  - "wrong-arity intrinsics never reach `sash: bug: bare command passed to sash_exec` — a clean compile-time error instead"
  - "`random` is consistently int-typed: `int n=$(random)` (word form) no longer rejects SB4030 (it returns an int)"
  - "native + c + janet parity (or per-mode guards with the janet gap filed); spec tests pin every func's sexpr behaviour"
  - "no spec regression"
---

# What

In an arithmetic sexpr — the DD#58 `(name args…)` call shape inside `$(( ))` —
**only `len` works**. Every other sash-native intrinsic **silently evaluates to
0** (int-returning ones) or **silently evaluates to 0 instead of being a type
error** (string/collection ones). Verified 2026-08-14 against the word form
`$(f …)` as ground truth:

| intrinsic       | word `$(f)` (truth) | sexpr `$(( (f) ))` | category |
|-----------------|---------------------|--------------------|----------|
| `len`           | 2                   | **2** ✅           | works    |
| `argc`          | 1                   | **0** ❌           | A int    |
| `getpid`        | 1040861             | **0** ❌           | A int    |
| `getppid`       | 1040875             | **0** ❌           | A int    |
| `getcpid`       | (empty/0)           | **0** ❌           | A int    |
| `epochseconds`  | 1786765926          | **0** ❌           | A int    |
| `random`        | (big int)           | **0** ❌           | A int    |
| `status`        | 1                   | **0** ❌           | A int    |
| `signum`        | 0 (DD#56 sig no.)   | **0** ❌ (masked)  | A int    |
| `toupper s`     | S                   | **0** ❌           | B string |
| `tolower s`     | s                   | **0** ❌           | B string |
| `join a`        | x y                 | **0** ❌           | B string |
| `keys m`        | (splat)             | **0** ❌           | B string |
| `basename p`    | (base)              | **0** ❌           | B string |
| `dirname p`     | (dir)               | **0** ❌           | B string |
| `sub`/`gsub`    | (string)            | **0** ❌           | B string |
| `match`         | (bool)              | **0** ❌           | B string |
| `getcwd`        | /home/…             | **0** ❌           | B string |
| `getenv N`      | SB5020 (tainted)    | **0** ❌           | B string |
| `arg N`         | SB5020 (tainted)    | **0** ❌           | B string |
| `itod N V`      | 3.14                | **0** ❌           | B string |
| `dtoi N "s"`    | 314                 | **SB4035** ⚠️      | C mixed  |

(`signum` returns 0 with no signal received, so its sexpr breakage is masked in
a naive test — but it goes through the same broken path. It is **DD#56**: the
most-recent signal number for FLAG-based, non-trap signal handling, polled via
`case $(signum) in …`; default 0, cleared with `(( signum = 0 ))`. It is *not* a
numeric-sign function — the `type_check.rs:116` comment calling `signum`/`dtoi`
"numeric conversions" is wrong and should be corrected as part of this fix.)

# Root cause

Two structural gaps in the transform's arith-sexpr path:

1. **The sexpr gate admits only `len`.** `is_arith_int_intrinsic`
   (`sash/src/transform/expr_handlers.rs:464`) returns true for `"len"` only.
   Anything else in `funccall_to_ssa` (expr_handlers.rs:440) falls through to
   `SsaValue::IntFuncCall`, which `inline_int_func`
   (`transform/functions.rs:93`) resolves as a *user* int-function; with no user
   body it returns `Expr("0")`. Hence silent 0.

2. **`try_native_intrinsic` has no arm for the int intrinsics.**
   `try_native_intrinsic` (`transform/expr_handlers.rs:85`) matches
   `BuiltinCmd::from_name` and handles only Len/Keys/Join/Sub/Gsub/Match/
   Toupper/Tolower/Basename/Dirname/Echo. The int intrinsics (argc, getpid,
   getppid, getcpid, epochseconds, random, signum, status, dtoi, itod) have **no
   `BuiltinCmd` variant** — `from_name` returns `BuiltinCmd::Other` — so even if
   the gate admitted them there is nothing to lower them to. Their word-form
   `$(f)` works only because codegen's command-substitution path
   (`sash/src/codegen/value_handlers.rs:640+`) lowers them by **string name**
   (`"getpid"`, `"argc"`, `"signum"`, `"dtoi"`, `"itod"`, …). The sexpr path
   never reaches that codegen path.

So the int set is known in one place (`is_int_returning_intrinsic`,
`type_check.rs`: len, argc, signum, dtoi, epochseconds, status, getpid, getppid,
getcpid — note `random` is **missing** here too) but the sexpr transform path
does not use it.

# Categories / intended fix

- **A. Int-returning → compute.** argc, getpid, getppid, getcpid, epochseconds,
  random, status, signum should produce their runtime value in a sexpr, matching
  the word form. Fix = route the sexpr `FuncCall` for these names to the same
  value-producing lowering the word form uses (see design fork below).
- **B. String/collection → type error.** toupper, tolower, join, keys, basename,
  dirname, sub, gsub, match, getcwd, getenv, arg, itod are not int-typed, so
  using them in `$(( ))` should be a **compile-time type error** (the same class
  as `int n=$(toupper s)` → SB4030), *not* silent 0.
  - **⚠️ Validator-bypass sub-case (higher severity).** For the *tainted-source*
    intrinsics `getenv` and `arg`, the word form is rejected **SB5020** (tainted
    source needs bless), but the sexpr `$(( (getenv HOME) ))` silently compiles
    (producing 0) — **the SB5020 reject never fires**. Confirmed 2026-08-14:
    `echo $(getenv HOME)` → SB5020; `echo $(( (getenv HOME) ))` → compiles, runs,
    prints 0; same for `(arg 1)`. It is a *reject*-bypass (a construct that
    should be a compile error silently compiles to 0), not a data-exfiltration
    (the tainted value is not propagated — the result is 0). Still, per AGENTS.md
    ("validator bypass testing is the highest priority") this raises the whole
    sexpr-intrinsic gap to at least P2 and warrants an adversarial look at
    whether other validator rejects are similarly bypassable through the sexpr
    `(f …)` shape.
- **C. dtoi (and itod).** dtoi returns an int but takes a string arg; in a sexpr
  the string literal currently trips the arith tokenizer (SB4035). Decide
  whether dtoi is allowed in a sexpr at all (and if so, how its args tokenize) or
  is cleanly rejected. itod returns a string → category B.

# Adjacent bugs surfaced (fix or file separately — flagged for DG)

- **`random` int-typing inconsistency.** `random` returns an int, but
  `int n=$(random)` rejects **SB4030** because `random` is absent from
  `is_int_returning_intrinsic` (`type_check.rs`). Either it is int-typed
  everywhere (word capture + arith) or nowhere — currently neither.
- **Wrong-arity → internal abort.** `$(dtoi x)` / `$(itod 5)` (wrong arg count)
  reach `sash: bug: bare command passed to sash_exec (expected absolute path)` —
  an internal-error abort at runtime, not a clean compile error. A validation
  gap (arity check for these intrinsics) independent of the sexpr issue. **Bug
  policy: this "bug:" abort is a stop-everything marker — investigate whether it
  can be hit by other wrong-arity intrinsic calls.**
- **`type_check.rs:116` doc bug.** Comment mis-describes `signum`/`dtoi` as
  "numeric conversions"; `signum` is the DD#56 signal-number reader.

# Resolved decisions (2026-08-15, DG) — governed by Option C

**Governing model: Option C** (`0a6d690`; DD#58/DD#17 amended in place). `$(...)`
is return-type-polymorphic — it captures the callee's declared result (int
`return` for `-> int` fns / int intrinsics, stdout string for string fns). An
**int producer** (user `-> int` fn OR int intrinsic) works in **both** `$(f)`
capture and the `$(( (f) … ))` sexpr. DG's principle (turn 226): "len shouldn't
be different from any other integer intrinsic" — so every int intrinsic must
work in both surfaces exactly like `len` and `get_port` already do. This fix
**completes that uniformity** for the sexpr surface; it does **not** retire or
restrict `$()` (an earlier "full-symmetry / retire the word form" idea was
raised and **rejected** — it contradicted Option C).

1. **Q1 — string producer in a sexpr → type error.** `(toupper s)` etc. sit in
   int-producer position; a string function there is a compile error pointing at
   capturing it first (`s2=$(toupper s)`). Not silent 0, not capture+atoi. The
   string function still works normally via `$()`. (This is DD#30 string-in-arith,
   not a `$()` restriction.) Covers the getenv/arg validator-bypass sub-case too.
2. **Q2 — dtoi/itod rejected in a sexpr.** Use `$()`: `int n=$(dtoi 2 "3.14")`
   already works under Option C. Do NOT complicate the arith tokenizer for a
   quoted string inside `(dtoi …)`. (itod returns a string → Q1 anyway.)
3. **Q3 — `random` int-typed everywhere.** Add `random` to
   `is_int_returning_intrinsic` so `int n=$(random)` captures the int (Option C
   requires it) and `$(( (random) ))` computes.
4. **Q4 — int intrinsics compute directly as i64 in a sexpr.** Extract the
   i64-producing step out of codegen `value_handlers` (each int-intrinsic arm
   computes an i64 then `emit_int_to_str`s it) into a shared helper; the sexpr
   path lowers to the direct i64 (no string round-trip), the word form keeps
   wrapping the same i64 with itoa for stdout capture. One source of truth; an
   int intrinsic yields an int, not a string. (DG: "why would an int-type
   intrinsic return a string?")

# History

Supersedes `bug-arith-keyword-pid-intrinsic-zero` (which named only getpid/
getcpid/random). The `$$`/`$?` *punctuation-sugar* siblings of this bug were
already fixed (commit 598d228: native `lower_value_as_int` computes the computed
specials). This task is the *keyword/sexpr* surface — a different mechanism
(int-intrinsic dispatch in the transform), and the broader of the two. Found
2026-08-14 while assessing DD#75 §1. `bug-janet-special-var-arith` tracks the
janet side of `$$` in arith and the janet getpid-exec / arith-status gaps.

Decisions taken 2026-08-15. During that discussion the AI briefly
mis-recommended retiring `$()` for int producers — it was reading the **stale**
DD#58/DD#17 "type error" wording, unaware Option C (`0a6d690`, THIS session) had
already superseded it and was never written into the DDs. Fixed as part of this
work: DD#58 + DD#17 amended in place with Option C banners; the done task
`dd58-sexpr-int-functions.md` point 2 noted superseded.

## DONE (2026-08-15)

Part 1 (9f85526): category A int intrinsics compute in a sexpr (native direct-i64
via `int_intrinsic_i64`, c via atoi fallback); `(getpid)`→`$$`, `(status)`→`$?`,
`(getppid)/(argc)/(random)/(signum)/(epochseconds)`→CmdSubst. Also fixed the
`$(( (x) ))` grouping bug (→0) and Q3 (`random` int-typed). signum native-only
(c/janet TODO stub → c-janet-signum-intrinsic).

Part 2: category B + Q2 — type_check `check_sexpr_string_producers` rejects a
string producer in a sexpr (SB4030); closes the getenv/arg validator-bypass and
the user-string-fn side-effect run. specs arithmetic.sh sx.1–17.

Deferred (small): getcpid-in-sexpr (`$!` slot).
