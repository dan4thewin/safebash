# 515 - One arithmetic boundary

2026-10-16. DG: "continue with coverage".

Started at `1d55bf7`, with current coverage provenance and 256 compiler
production cold lines. Took the whole nine-line `parse/arith.rs` cohort.
A seven-program corpus warmed seven of them, including a known positive
control. The remaining two are the deliberate empty comment/EOF renderings.

## A paired token is not necessarily a paired delimiter

Both of these valid expressions rejected:

```sash
echo $(((1+2)))
echo "$(((1+2)))"
```

The unquoted collector subtracted two for a `DblParenClose` token and emitted
both bytes whenever the remaining depth was positive. With one inner group
still open, the token straddled that group's close and the first outer close.
It put an extra `)` into the expression text.

The quoted boundary scanner had a different mistake: it counted `((`/`))`
pairs while treating a single parenthesis as an ordinary byte. It stopped at
the inner close plus the first outer close, leaving the group unbalanced.

Removed the paired closing token and its outside-arithmetic splitting shim.
All closing parentheses now occupy individual tokens. The collector closes
inner groups one byte at a time, then consumes the adjacent outer `))`
without emitting either byte. A spare unmatched single close remains text
for the existing operand/trailing-token diagnostic; spaced outer closes
still cannot substitute for the delimiter.

This is an internal token representation correction, not new arithmetic
syntax. Grouping, calls, mutation statements, conditions, for-headers,
surrounding captures, trailing literal text, and missing/extra closes are
covered in both backends.

## Quoted arithmetic had a second grammar

Its raw body also bypassed DD#94 comments:

```sash
echo "$(( 1 + # ) ignored
2 ))"
```

And the raw arithmetic re-lexer silently supplied a missing brace:

```sash
int x=5
echo "$(( ${x ))"
```

That malformed program compiled and printed `5`. `scan_arith_braced_var`
classified everything remaining after `${` as though it were a closed body.

Quoted arithmetic now reuses the real token collector instead of a private
byte counter. That shares comment handling, balanced expansion scanning and
lexical errors. Its nested scan isolates the lookahead queue and restores
`last_end`: `peek_n` can be constructing this quoted token while older tokens
are still queued. Extended the existing direct-next versus peek/next
invariant across three sources and lookahead depths 0, 1 and 3, including
token spans and adjacency.

The first full sweep found an introduced diagnostic regression in
`lexcold.4`: without an arithmetic close, the enclosing double quote could
be read as an unclosed inner string. Preserved the arithmetic opener rather
than reporting that consequence. Earlier errors and other nested delimiter
errors retain the existing first-error policy. Repeated full gates are green.

## The raw API test is not a rival

The re-lexer now produces an explicit unterminated-brace token, and the raw
arithmetic API rejects it with a distinguishing SB4057 message. This API
still executes for the malformed source, but the scanner's deferred lexical
error masks its result at the CLI.

Proved the distinction rather than inferring it from coverage: temporarily
restored the old valid-variable fallback. All **466 arithmetic CLI specs
still passed**, while the direct raw-API unit failed on
`Ok(DollarVar("x"))`. Restored the fix before final measurement. The unit
pins a result the CLI specs cannot see, not a second assertion of their
observable diagnostic.

The other new unit checks the correct empty Comment/EOF renderer mappings:
neither reaches that renderer through the source pipeline. Compiler-only
profiles show both at zero; the union shows each at one. They remain correct
totality answers, not new ICEs.

The other source gaps are missing/invalid radix digits in an authored
arithmetic default, an unbalanced bare subscript (two cold lines), postfix
increment/decrement on a non-assignable operand, and negative overflow
beyond i64::MIN. Their specs assert distinguishing diagnostic text.

Flattened the atom dispatcher and reused its cloned token, removing nested
optional dispatch and an extra clone. Its live complexity is 12/2; the
enforced dispatcher exemption applies without a suppression or gate bypass.

## Accounting and measurement

Added **28 dual-mode specs**, arithmetic plan 438 -> 466:
`aclose.1`-`aclose.20` and `arithcold.1`-`arithcold.8`. Both ID collision
checks are clean, including the variable-ID check. Historical reservations
were empty. Predicted **8555** checks, native 4380 / C 4175.

The full sweep matches exactly: **8542 passing / 13 existing C skips**.
The first-sweep history contains exactly **56 new IDs**; reruns contain zero
new IDs and zero flips, with comparison liveness **258**. A first historical
lookup incorrectly used the full SHA against stored short hashes and matched
no rows; discarded that vacuous result. The corrected first-at-hash query
matches all 8555 IDs and identifies exactly the reserved 56.

Units **138 sash + 27 convert + 20 expand**. Formatting, clippy, totality and
grandfather-on-modify complexity gates pass without bypasses. Ordinary builds
and coverage runs were serialized.

Fresh final union and database, with current provenance:

| Measure | Before | After |
|---|---:|---:|
| Arithmetic production cold lines | 9 | **0** |
| Compiler production cold lines | 256 | **247** |
| Compiler production recorded lines | 29454 | 29430 |
| Compiler production cold branches | 250 | 245 |

Lexer, parser and expr remain at zero cold production lines. Raw headline
coverage is **98.82% line / 99.23% function / 93.32% branch**.

Exported compiler-only spec profiles separately from the unit binary, using
the compiler's own canary profile signature. The source-gap sites are warm;
the comment/EOF renderer mappings are not. The raw brace error executes four
times in spec profiles and three more in its direct unit, with its independent
assertion established by the neuter above.

Restored uninstrumented release binaries with `mk cov-reset`, verified a
compiler canary produces no profile, and repeated ordinary units and full
specs. Updated DD#94's enforcement notes and status. No new architecture,
language decision, deferred work, or SOP change.
