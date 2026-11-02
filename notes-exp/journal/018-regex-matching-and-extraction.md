# Regex matching and owned extraction

**2026-11-02.** DG authorized matching/extraction after byte-string helpers.
DG selected full counted subjects, including NUL, and standard C++ repeated
iteration. The modified ECMAScript standard-library engine was already
selected pragmatically in DD#1. DG delegated the output-exception choice
to simplicity; the AI chose local-array construction plus swap and the
reversible `match_text` naming/bool overloads.
[DD#11](../decisions/DD11-regex-matching-and-extraction.md) separates these
choices, their reasons, and their observable consequences.

The AI implemented the shared helpers, four focused fixture groups, and
the harmless [C++](../../sash-cpp/examples/regex_report.cpp)/
[sash](../../sash-cpp/examples/regex_report.sash) report. Replacement
templates were explicitly outside this unit.

## Implementation and boundaries

`match(subject, regex)` searches rather than requiring a full-subject match.
`match_text` owns the whole match and returns empty on miss. The out-array
`match` stores dense `[whole, g1..gN]`; `matchall` flattens each match's
groups, or whole matches when no groups exist. Empty and nonparticipating
groups keep their slots, and every reported group is retained.

All subjects are counted views and every output string owns its bytes.
An empty match is success; the scalar empty value alone cannot distinguish
it from no-match. A completed miss clears old output. Exceptions preserve
the previous array, and a subject borrowed from that target remains safe
until extraction completes. Standard regex errors use the existing
`CommandError(2)` / `regex: ` renderer; allocation failure escapes.
No helper adds checkpoints or silently checks an ordinary false result.

The report retains a real function, conditions, capture arrays, pairwise
map construction, explicit key snapshots, and key traversal. Unchanged
`08` converts strictly to 8; an empty value defaults to zero. The first
source draft incorrectly nested `keys` inside `join`, which SB5062
rejects: `join` takes a named collection. The corrected source uses
`keys options saved_keys`; C++ mirrors that explicit snapshot.

## Deliberate differences and the pattern finding

Executed probes pin the differences rather than treating native behavior
as automatically wrong. For `a<NUL>b`, C++ finds the final `b` and repeated
`.` yields three bytes; native DD#63 stops at the first NUL and yields one.
For `ab` / `^`, C++ emits one empty span, while native fresh-suffix searches
emit three. Under the same ECMAScript engine, `a` / `(|a)` produces
`["", "a", ""]` with standard iteration rather than the two empty spans
an immediate byte skip would produce. That last comparison is not a claim
about Plan 9's alternative ordering.

A separate direct libstdc++ probe accepted the counted pattern
`a<NUL>b` but failed to match it; regex escape `a\x00b` and a dot across
the subject NUL worked. This is measured host behavior, not a claim about
every standard library or a settled standards violation. `compile_regex`
now explicitly rejects actual zero pattern bytes with
`regex: literal NUL in pattern; use \x00`; it neither truncates subjects
nor rewrites patterns. Externally supplied `std::regex` values have no
inspectable original pattern.

DG requested the native
[literal-NUL pattern/source backlog](../../notes/tasks/reject-literal-nul-patterns-and-source.md).
That is a pending native task, not a native implementation change here.
The broader raw-source ban still needs a native decision; escaped NUL
string values and native's deliberate subject boundary are separate.

An actual native invalid-regex probe returned 2, emitted its regexp error,
and produced no continuation output: `if`/`||` cannot resume failed
`regcomp9` in that process. C++ can catch its existing status-2 exception.
Neither failure is an ordinary false match.

## Evidence and the libc++ fixture correction

Predictions were four additional groups (**51 to 55**), one golden
(**ten to eleven**), and three source-only project files (**50 to 53**).
GCC, Clang, supported Linux ASan, real Apple Clang/libc++, and Apple ASan
produced those totals. Both Linux compilers retained their fault gates
and unchanged **169-case** catalog. Formatting passed.

The first Apple run reached 54 passing groups, then failed the allocation
fixture's search positive control: its broad `>=512` fault also blocked
libc++ executor storage before any owned copy. The test-only injector now
supports an exact size, derived from the host string's capacity plus one.
Positive controls require scalar search and the complete standard
iterator walk over the small and large matches to succeed under that
same fault. Owned whole/single/later-repeated copies must then fail.
The separate minimum-size search fault still verifies that an executor
allocation failure escapes without clearing output. No production code,
sanitizer category, or resource case was changed to repair this fixture.

Before the first transfer, all 50 published string-baseline hashes matched
on DG's isolated Mac. The failed run's 53-file snapshot was verified
unchanged before replacement. The final 53-file snapshot produced
**110** Apple/Apple-ASan group results and **106** matching before/after
hash lines. All eleven goldens passed in each gate; the new report also
had explicitly checked empty stderr on both Apple builds.

The four-group `--regex` selector and new report passed complete-category
UBSan. An independent signed-overflow canary returned 1 with the intended
UBSan diagnostic under the same flags. This is **regex-only UBSan**, not
a full-runtime claim or resolution of the existing sanitizer backlog.

Nine isolated mutations compiled and returned 1 for their intended
assertions, not a compile failure, timeout, or unrelated trap:

| Mutation | Assertion that failed |
|---|---|
| Require a whole-subject match | Match is a search |
| Truncate the search subject at NUL | Counted NUL and high-byte extraction |
| Omit the single whole match | Dense whole-plus-group layout |
| Include whole matches with repeated groups | Groups-only flattening |
| Retain stale output after a single miss | Normal miss clears single output |
| Retain stale output after a repeated miss | Normal miss clears repeated output |
| Accept literal-NUL patterns | Explicit pattern diagnostic |
| Clear output while rethrowing allocation failure | Later copy publishes no partial replacement |
| Reanchor and byte-skip fresh suffixes | Original beginning is retained |

Production controls passed before and after; all 53 local source hashes
remained unchanged. Sources/logs are retained as session artifacts.
The 25 specific temporary executables, objects, and local transfer archive,
plus the Mac transfer archive, were removed.

## Real-CLI reference adaptations

All eleven source pairs pass using fresh dereferenced, checksum-pinned
copies of DG's supplied CLI and companion archive:

- compiler: `3437761ea4c93f6119a6a497e65c5254a41c4a23e9e38fc2728268ed5f6d34cd`
- archive: `d26debdce3f691f0880d5a2745bdf2e1f7c875815c25a8e4fc676bba48f10dfa`

This compiler now applies DD#127 to older external-stdout captures.
Job/stream/pipeline worker labels and the lifecycle summary needed adjacent
own-taint attests after their single-use captures. The shared
`worker_summary` instead captures and attests its pipeline inside its
definition, then emits clean builtin output. `framed_summary` captures
that cleaned value before adding brackets, keeping the builtin's newline
out of the frame. All previous golden bytes remain unchanged; no C++
runtime taint checks or native changes were added.

Independent extra inputs exercise ordinary misses, the positive `NEVER`
sentinel, invalid numeric text defaulting without repair, signed binary
conversion, and empty-input attestation failure. Both implementations meet
those independent output/status expectations.

## Scope

This completes matching/extraction, not `sub`/`gsub`, bulk reads/splitting,
remaining formatting, general redirects, public wait, or forked-C++/
background ownership. There is still no IR reader/emitter or producer
implementation. The protected producer proposal remains unchanged,
untracked, and excluded from every transfer and commit.
