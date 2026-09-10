# 142 — DD#75 implementation, part 1: `$(keys m)` splat-context (P1 memory-safety)

DG directed: implement in the recommended order (two P1 memory-safety bugs,
then `len`, then the rest of DD#75), **each part test-first**, with **spec
tests** (`spec/*.sh`) as the failing tests.

## Part 1 — `keys-splat-context-bypass` (P1)

**Bug (DG probe).** `printf ==%s==\n $(keys m)` printed `==foo bar==` then
garbage bytes: `$(keys m)` in command-arg position bypassed the array-splat
context guard, silently joining (best case) or letting a `%s` read a bad
pointer (worst case) — a memory-safety escape in a safe-subset compiler.

**Root cause.** `check_argv_splat` (`validate.rs`) enforces the DD#60/#75
splat-context rule (SB5025 — for-in sole source / snapshot only), but its
detector `is_argv_splat_expr` matched only `$(argv)`. `$(keys m)` is an
array-returning splat too, but was invisible to the guard (while `is_splat_form`
*did* know keys, so only the quoted-splat SB5026 path caught it).

**Fix.** Added `is_keys_splat_expr`; count `$(keys m)` as a valid for-in source
(so `for k in $(keys m)` still works) and emit SB5025 with a keys-specific
message everywhere else. Argv's message untouched (no risk to argv tests).

**Test-first.** `spec/types.sh` 33.17 (`printf … $(keys m)` → SB5025) and 33.18
(`x=$(keys m)` → SB5025): written first, confirmed red, then green after the
fix. Positive `for k in $(keys m)` (33.10) still passes.

**Sweep.** native 0 failing; c 13 / janet 36 failing — identical to the clean
session baseline (no regressions). 33.17/33.18 pass in all three backends.

**Bonus fix.** The sweep surfaced `dp.co6` (a dispatch test I added in `3bca6b1`)
failing in all modes: it used a dispatch form as an `if` *condition*, which
SB4028 correctly rejects. Replaced it with a valid construct — a compact
dispatch **statement** inside an `if [[ -n $x ]]` body → `no`. (janet:33.10, the
keys for-in, remains a long-standing pre-existing janet gap — 100+ commits.)
