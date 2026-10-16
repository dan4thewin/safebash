# 513 - One quote grammar

2026-10-16. DG: "continue with coverage".

Started at `f15119d`, with a fresh 265-line compiler production cold cohort,
nine in `parse/lexer.rs`. Read the complete cold functions and their callers
and producers. A nine-program corpus warmed six of those nine lines, with
positive controls; it also exposed quote-boundary errors in captures.

## The boundary scanner had its own language

These valid programs were rejected:

```sash
echo "$(echo "a\"; echo b)"
echo "$(echo $'a\'b')"
echo "$(echo "a$'"'b")"
```

The first must print `a\`, then `b` on a second line. DD#64 makes the
backslash literal: it cannot escape the following double quote. The other
two must print `a'b` and `a"b`. C-string quote escapes, including C-string
interpolation inside ordinary double quotes, are already part of the language.

`skip_balanced_dquote` privately treated backslash as an escape, contrary to
the real double-quote scanner. The capture scanner also treated `$'…'` as an
ordinary single-quoted run. An escaped apostrophe therefore changed the
boundary, and a C-string containing a double quote inside a nested `"…"`
confused the private skipper again.

Removed that skipper. Capture boundaries now reuse `scan_double_quoted` and
the existing C-string consumer, not a second quote grammar. The new specs
assert output independently, including a parenthesis inside the C-string,
nested quoted captures and an ordinary literal backslash.

## Comments are not parentheses either

The same boundary scanner also rejected this valid form:

```sash
echo "$(echo ok # ) ignored
)"
```

The unquoted capture compiled and printed `ok`. The quoted scanner counted
the comment's `)` as its closer. A quote in the comment could likewise
change its private quote state.

Extracted the existing comment-start predicate from `attached_hash_word`;
both ordinary lexing and capture boundaries use it, and the latter delegates
comment consumption to the existing logos scanner. This preserves sash's
deliberate whitespace rule for comments, including `a#b` remaining a word.
No new comment or quote behavior was chosen. `dqbound.1`-`dqbound.9` cover
the quote fixes, comment fixes and controls.

## The other cold material

- A missing quote inside a brace-less operand and nested braces in that
  operand were source gaps. `lexcold.2` and `lexcold.6` assert the actual
  unterminated-quote and literal-only-default diagnostics.
- Backticks without a literal prefix inside `"…"` are deliberately text
  (DD#90). `lexcold.1` pins the output.
- Two rejected C-escapes must preserve the first diagnostic, not replace it
  with the second. `lexcold.3` and `lexcold.5` assert the octal message and
  source position, directly and through a quoted capture.
- The C-string producer always returned `Tok::AnsiCQuoted`, including on
  error: errors are recorded in scanner fields. Narrowed it to return
  `String`; the token-producing caller wraps it and the interpolation caller
  consumes the string directly. This removes an impossible tag branch by
  encoding the producer contract, not by calling narrowing a reachability
  proof.
- Two setters implemented "first unterminated construct wins". Consolidated
  them into `record_unterminated`, retaining that policy in one place rather
  than claiming the old second setter's false branch was unreachable.
  `lexcold.4` pins the inner arithmetic opener before its missing outer
  closers. Flattened the arithmetic scan loop's redundant depth check, keeping
  the touched function below the measured complexity threshold.
- `push_source_char`'s lead-byte guard is a cursor invariant, not a source
  gap. Its new direct unit test starts with valid UTF-8 and deliberately puts
  the private cursor on a continuation byte, asserting the existing panic.
  No source-spec rival was added.

Fifteen dual-mode specs: ten in `wordsplit.sh`, four in `errors.sh`, one in
`var_modifiers.sh`. Both prefixes were checked against historical IDs first.
Both collision checks are empty; changed plans match their literal counts,
and the suite's sole two-token plan difference remains the documented
variable-ID loop in `coverage.sh`. Predicted sweep: 8465 + 30 = 8495 checks,
native 4350 / C 4145, and 30 new IDs.

## Measurement must be serialized too

Discarded the initial union measurement after the instrumented compiler was
gone at the subsequent profile export. I had run the ordinary `mk test`
alongside coverage. It is not independent: `test` depends on the release
`build` target and itself runs release-mode tests. It can overwrite the
instrumented artifacts while the driver is using them.

Repeated coverage without any overlapping build. Added the explicit ordering
rule to AGENTS.md: ordinary gate before coverage, or after `mk cov-reset`,
never alongside it. The startup canary does not protect against a later
replacement.

## Measured result

Fresh serialized `mk cov-union` -> `mk cov-db` -> `mk cov-stale`:

- Lexer: **9 -> 0** production cold lines, 1182 recorded.
- Compiler: **265 -> 256** production cold lines, 29461 recorded.
- Parser and expr remain at zero.
- Raw cargo-llvm-cov: **98.79% line / 99.23% function / 93.23% branch**.

Before resetting the instrumented build, exported only the real compiler's
profile-signature cohort, excluding the unit-test executable. All recorded
lines in `skip_simple_quote`, `skip_braced_to_close`, `scan_dq_backtick`,
`record_unterminated`, `reject_escape`, `consume_ansi_c_quoted` and
`hash_starts_comment` are warm from actual CLI specs. This is not attribution
from the earlier exploratory corpus or stale pre-edit line numbers.

The UTF-8 cursor guard at `lexer.rs:1523` reads zero in those spec profiles
and exactly one in the union, matching its direct-call test classification.

Full sweeps: **8495 checks**, exactly predicted, **8482 passing / 13 existing
C skips**. The first sweep introduced exactly the 30 predicted IDs; reruns
introduced none. Zero flips, no unexplained retirements, comparison liveness
258. Ordinary units **136 + 27 + 20**. Restored the uninstrumented release
build with `mk cov-reset`; a post-reset canary emitted no profraw.

Then reran the ordinary unit gate and the full spec sweep against the restored
shipping binary, serially. Same counts, zero flips/new IDs, liveness 258,
and no plan mismatches or retirements. Kept the signature-separated coverage
export and database as session evidence; removed the owned source probes and
temporary binaries.
