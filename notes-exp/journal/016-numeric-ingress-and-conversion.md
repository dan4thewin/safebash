# Program inputs, strict conversion, and the ninth pair

**2026-11-02.** DG authorized the numeric-ingress/conversion unit after
checked arithmetic. Native DD#60, DD#72, DD#77, DD#93, and DD#101 already
settle its behavior. [DD#9](../decisions/DD09-numeric-ingress-and-conversion.md)
records those contracts and the AI's reversible implementation choices.
The AI implemented the standalone helpers and harmless
[C++](../../sash-cpp/examples/numeric_report.cpp)/
[sash](../../sash-cpp/examples/numeric_report.sash) report.

## Implementation and boundaries

`arg` and `argv` receive ordinary C argc/argv explicitly, without hidden
program state. Lookups and snapshots own their strings; whole snapshots
include argv0, and slices reindex from zero without dropping empty arguments.
Required absence fails, optional defaults are eager and test presence,
wide slices are empty, and negative/malformed metadata fails explicitly.
A low-level empty C++ frame is supported; source programs still have argv0.
Callers must provide an actual C frame of the stated extent and lifetime.

`getenv` returns an owned optional string. Missing is an ordinary lookup
result, not an automatic abort; set-empty stays present (DD#101).
Counted names are honored, NUL names are rejected rather than truncated,
and allocation failure does not become absence. Host environment reads
remain live under the selected single-threaded model, without mirroring.
Environment fixtures change only their private child's environment.

One allocation-free, locale-independent `from_chars` parser implements
`parse_int`, `is_int`, `int_or`, and `int_required`. Complete optional-sign
decimal/modern-prefixed text must fit i64, including its asymmetric minimum.
Leading-zero runtime decimal is accepted; partial, malformed, whitespace,
NUL, and out-of-range input is invalid.

Default/required conversion works identically on scalar strings and string
cells. Valid zero stays zero. Required messages run only on failure,
exactly once, using the existing `required value: ` renderer and
`CommandError(2)`, not ArithmeticError. Failed conversion preserves the
destination and existing body/owner-END unwinding. Invalid unhandled END
requirements escape the caller; no automatic END guard was added.

The report retains a real charge function, checked multiplication,
validity branching, and argv traversal. Inputs `08 0` with a hex rate
produce the independent eight-line golden. Input validation and traversal
loops have explicit caller checkpoints; no ingress helper hides one.

## Evidence

Predictions were four additional groups (44 to **48**), one additional
golden (eight to **nine**), and three additional project files (44 to **47**).
GCC, Clang, supported Linux ASan, actual Apple Clang/libc++, and Apple ASan
produced those group/golden totals. Both Linux compilers retained the
fault gates and unchanged **169-case** command/capture catalog.
The Mac's two complete gates produced 96 group results; all 47 transferred
source hashes matched before and after. Exact report stdout and empty
runtime stderr were checked on both hosts and their sanitizer builds.

The four-group numeric selector and report also passed UBSan. An independent
deliberately overflowing signed-add canary returned status 1 with its
intended diagnostic under the same flags. This is **numeric-only UBSan**,
not a full-runtime claim; the existing host-compatibility task remains open.
No sanitizer category or resource fixture was disabled.

Seven isolated mutations compiled, ran, and returned status 1 for their
intended assertions, not a compile failure, timeout, or unrelated trap:

| Mutation | Assertion that failed |
|---|---|
| Ignore unconsumed trailing bytes | Reject partial/malformed/out-of-range input |
| Use the positive magnitude limit for negative text | Preserve complete signed/prefixed values including MIN |
| Default valid zero | Numeric defaults distinguish invalid text from valid zero |
| Treat set-empty environment as missing | A present empty value is a successful lookup |
| Keep original indices in argv slices | Slices reindex without dropping empty arguments |
| Return empty for a missing required argument | Required absence cannot silently yield empty text |
| Change the requested requirement message | Invalid/out-of-range conversion uses its exact requirement |

Production controls passed before and after, with unchanged project hashes.
An initial post-mutation hash command used the subproject cwd with a
repo-relative manifest; rerunning from the repo root verified all 47 files.
The final example-only checkpoint placement was rechecked on GCC, Clang,
Linux ASan, UBSan, Apple Clang, and Apple ASan, with refreshed 47-file parity.
Mutation sources/logs remain session artifacts; temporary executables,
objects, canary executable, and transfer archives were removed.

## Real-CLI reference

All nine sash pairs pass against dereferenced, checksum-pinned copies of
DG's supplied compiler and companion archive:

- compiler: `be2626e41cddc0a6914efa5782c5829031f8eb175df877557564d79d0fdb5a9e`
- archive: `d26debdce3f691f0880d5a2745bdf2e1f7c875815c25a8e4fc676bba48f10dfa`

The first source draft used retired `-i`, an unescaped literal class hyphen,
and a typed declaration as a lookup condition. DD#93's `i?`, DD#39's `\-`,
and DD#88's untyped capture-condition rule corrected these example forms.
No native behavior or compiler code changed.

## Scope

This completes the bounded program-input/strict-conversion unit, not the
remaining text, regex, read, redirect, public-wait, or forked-C++ families.
Taint/admission and default-literal constraints remain producer work.
There is still no IR reader/emitter or numeric wrapper. The protected
producer proposal remains unchanged, untracked, and excluded from transfers.
