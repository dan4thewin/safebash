# 205 — Fix P1 bug: intrinsic-arity fallthrough (SB4045, fork C)

Second of the two bugs the coverage grind surfaced. A value intrinsic called
with the wrong arg count — `$(sub a)`, `$(toupper)` — compiled clean then
aborted at **runtime** with `sash: bug: bare command passed to sash_exec`
(rc 127). DG approved **fork C**.

## Cause

`sub`/`toupper`/… are `BuiltinCmd`s, so `resolve.rs` skips resolving them
(they're expected to lower as intrinsics). But each intrinsic (`regex_replace_
intrinsic`, `case_convert_intrinsic`, …) returns `None` on a wrong arg count,
and `resolve_single_cmdsubst` then treats the name as an **external command** —
building a bare `sub` argv that reaches `sash_exec` and hits the runtime
invariant (sash-rt/lib.rs:363). `$(len a b)` was the sibling inconsistency:
`try_len_intrinsic` uses `args.first()` so it silently ignored the extra arg.

## Fix (SB4045, defense-in-depth)

Shared arity table `value_intrinsic_arity(name)` in ssa.rs (len/keys/toupper/
tolower/basename/dirname=1, match=2, sub/gsub=3; `None` = variadic join/echo).

- **A — validate.rs `check_cmdsub_arity`**: walk every `$( )` (`walk_all_exprs`
  visits the `CommandSubstitution` node); a single-command body headed by a
  fixed-arity intrinsic with a mismatched arg count → **spanned** SB4045. Makes
  `len` strict too (uniform arity).
- **B — transform `resolve_single_cmdsubst` backstop**: a fixed-arity intrinsic
  that fell through no longer becomes a bare external command — it yields an
  empty capture, so nothing bare can reach `sash_exec` even under `--bypass`
  (where validate doesn't run). This is why fork B lives in transform, not
  validate.

## Verified

`$(sub a)`/`$(sub a b)`/`$(toupper)`/`$(len a b)` → spanned SB4045 (was runtime
`bug:`); `$(sub l L s)`, `$(toupper s)`, `$(len s)` still compile+run. Specs
errors.sh 55.42-45. Full 3-mode sweep: 0 regressions, 12 new passing.

## Both grind bugs now fixed

const-reassign panic (journal 204, SB4044) + intrinsic-arity fallthrough
(SB4045). Both were reachable panic/`bug:` invariants on ordinary input,
surfaced precisely because the coverage audit probed unexamined paths — the
concrete payoff of the "account for every reachable line" directive.

## Noted, separate

`$(toupper s)` uppercases the literal `s` → `S`, while `$(len s)` reads the
variable `s` (→ its value's length). That literal-vs-variable subject
inconsistency is pre-existing and unrelated to arity — not chased here.
