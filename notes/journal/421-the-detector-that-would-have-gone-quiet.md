# 421 — The detector that would have gone quiet

`$?` → `$(status)`. The third and last of DD#111's magic-variable desugars,
and the only one that carried a diagnostic with it.

## What landed

`magic_sugar_expansion` gains `"?" => "status"`, so the punctuation is gone by
the time the parser finishes. `parse_status_word` returns
`intrinsic_expansion("status", span)` instead of `SimpleExpansion("?")`.
Downstream, `$?` and `$(status)` are now literally the same node.

Three things fell out as dead and were deleted:

- **`bareword_shortcut_expansion`** — the whole function. It existed to fold
  `$(status)` *into* `$?`, which is the direction DD#111 reverses. Its last
  arm was `status`; removing that emptied it.
- **`intrinsic_int`'s `?` seed** in `type_check.rs`, by the same structural
  argument that retired `!` in journal 420: `var_type` reaches a magic name
  only through `SimpleExpansion`, `assert_no_magic_sugar` forbids those
  surviving parse, and the only other spelling — `${?}` — is rejected by
  SB4034. Verified by running it, not by failing to find callers.
- The two stale comments that named `bareword_shortcut_expansion` as the live
  mechanism (`spec/commands.sh`, `validate.rs::check_cmdsub_heads`).

## The trap, and why it was worth the caution

`validate.rs` had two identical walks — `find_dollar_question` and
`stmt_uses_status` — both matching `ExprKind::SimpleExpansion("?")` and
nothing else. They are the AST half of SB5012, the rule that says a status
read needs a preceding command to read the status *of*.

Their doc comment claimed to cover "`$?` / `$(status)`". That was true only
because `bareword_shortcut_expansion` folded the intrinsic into the node they
matched. Reverse the fold without moving them and both walks find **nothing** —
SB5012 stops firing, forever, and every spec stays green, because a rule that
never fires breaks no test that does not expect it to fire.

That is the same shape as the `test_status` ghosts and the `sx.25` non-proof:
the failure mode is silence. So the change moved both to
`ir::intrinsic_call_name(expr) == Some("status")` and collapsed
`stmt_uses_status` into `!spans.is_empty()` over the span collector — one
walk, one encoding, which is the `magic_intrinsic_var` lesson for the third
time.

`intrinsic_call_name` lives in `ir.rs` and its constructor lives in
`parse/expr.rs`, so they can drift. `intrinsic_expansion_round_trips` pins
them.

## Neuters

Three, because three separate things could have gone quiet.

| neuter | failures | verdict |
|---|---|---|
| detector matches `"NEUTERED"` instead of `"status"` | 6 (`coverage.sh` 3, `errors.sh` 2, `signals.sh` 1) | SB5012 genuinely pinned |
| `"?" => "status"` arm removed | 5 (`coverage.sh` 2, `errors.sh` 2, `signals.sh` 1) | the desugar is pinned |
| `status` removed from `is_int_returning_intrinsic` | **0** | **gap** |

## The gap the third neuter found

Deleting the `?` seed moves `$?`-as-int onto `is_int_returning_intrinsic`.
Neutering `status` out of that set breaks `int x=$?`, `exit $?` and `wait $?`
with SB4030 — and the **entire sweep stayed green**. The int-ness of the
most-used magic variable, at exactly the sinks DG scoped to int-only, was
pinned by nothing.

`isink.11` guards `$!`/`wait` and reads as though it covers the family. It
does not. Added `isink.13` (`int n=$?`), `isink.14` (`exit $?`), and
`isink.15` (`int n=$(status)` — the spelling-agreement invariant, which is
DD#111 itself). `errors.sh` is dual-mode, so +6.

The proof of the new specs was accidental and complete: they failed on a
binary that still had the neuter compiled in, and passed once it was rebuilt.

## A false signal worth writing down

A standalone `convert.sh` reported **62 failures** against a tree whose only
change was in `sash`. `common.sh` *appends* `target/release` to `PATH`, so
`sash-convert` resolved to an installed copy twelve days old — `SASH=` does
not override it, because it is a different binary. The same 62 appeared under
a neuter, where they read as signal and nearly sent the whole investigation
sideways.

AGENTS.md now says to prepend, always, and that a standalone failure count is
not a finding until it survives that.

## Gate

Predicted 6524 (6518 + 3 specs × 2 modes); swept exactly 6524, zero failing.
Zero flips; exactly the six new ids; non-vacuity probe 231. `mk test` clean,
clippy clean. Every touched function well inside the complexity gate.

DD#111 is now complete: no magic variable survives the parser.
