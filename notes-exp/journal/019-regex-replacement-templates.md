# Standard C++ regex replacement

**2026-11-03.** DG selected the remaining replacement-template policy:

> use C++ replacement notation; when the IR consumer is written we can translate; go

[DD#12](../decisions/DD12-regex-replacement-templates.md) records that
decision. The AI implemented owned first/global replacements, three fixture
groups, and a harmless [C++](../../sash-cpp/examples/rewrite_report.cpp)/
[sash](../../sash-cpp/examples/rewrite_report.sash) report. Native-template
translation remains deferred to the future IR consumer; no adapter,
producer change, or native implementation belongs in this unit.

## Operations and the counted-format correction

`sub(subject, regex, replacement)` returns an owned first replacement;
`gsub` replaces every standard-iterator match. Unmatched input is preserved,
and a normal miss returns an owned unchanged subject. The existing
two-integer arithmetic `sub` remains unchanged.

Templates use standard C++ default formatting: whole match `$&`, numbered
groups `$1` through `$99`, literal dollar `$$`, and per-match prefix/suffix
context. Backslashes are ordinary bytes, not native group escapes.
Computed templates use the same rules as literals. DD#11's original
anchor context, nonempty retry after an empty match, and final-boundary
termination apply; `sub` stops at its first match, including an empty one.

Subjects and formats are counted through NUL/high bytes and partial views.
The first implementation used the standard output-iterator
`regex_replace` overload with a counted format string. Its binary fixture
failed. A direct probe isolated the convenience overload: replacing `(b)`
in `a<NUL>b<FF>` with `X<NUL>$1Y` returned four bytes instead of the expected
seven, even outside the runtime. The standard explicit-range match
formatter retained all four formatted bytes.

The corrected shared helper uses standard `regex_iterator` and
`match_results::format(begin, end)`, copying unmatched spans into its local
result. The same direct probe then returned the expected seven bytes.
This is an observed convenience-overload boundary, not a universal
standard-library defect claim. There is no bespoke template parser,
progression algorithm, or subject/format snapshot.

Regex errors retain the existing status-2 `regex: ` exception.
Allocation/length failures escape without publishing partial, empty,
or unchanged success. Assignment happens only after successful evaluation,
including destination-borrowed subject/template views. No helper adds
checkpoints or automatic END handling.

## Report and failure fixtures

The report keeps a real function, match branch, standard first/global
rewrites, literal dollar/backslash output, and live string-cell traversal.
`COUNT=08;RATE=7` matches an independent ten-line golden.
The sash companion uses equivalent native `\1:[\2]`/`<\0>` templates;
that does not make its notation interchangeable with C++ `$1:[$2]`/`<$&>`.
Two `=`-bearing sample values were quoted for the current source grammar.

Fixtures pin owned match/miss results, scalar/cell parity, non-inserting
missing reads, empty deletion, counted views, binary subjects/formats,
computed templates, literal dollars/backslashes, missing/unmatched groups,
two-digit group references, prefix/suffix context, and empty progression.

Exact allocation faults derive their size from a healthy completed output's
capacity plus one. Positive controls require the entire 1024-match standard
iterator walk to succeed under that same fault before failing first/global
output construction. A long-format case likewise targets owned output,
not a template snapshot that no longer exists. A separate minimum-size
search fault must escape without returning unchanged input.

The isolated control runner initially assumed the healthy selector wrote
its `ok` line to stdout; the fixture protocol writes it to stderr.
Inspecting both channels resolved that false control failure. Only the
session artifact's channel assertion changed.

## Executed evidence

Predictions were three additional groups (**55 to 58**), one golden
(**eleven to twelve**), and three source-only project files (**53 to 56**).
GCC, Clang, supported Linux ASan, real Apple Clang/libc++, and Apple ASan
produced those totals. Both Linux compilers retained the fault gates and
unchanged **169-case** catalog. Formatting passed.

The Mac's 53-file published baseline matched before transfer and again
before extraction. The frozen 56-file snapshot produced **116** combined
Apple/Apple-ASan group results, twelve goldens per gate, and **112**
matching before/after hash lines. The new report's stderr was explicitly
checked empty on both Apple builds.

The three-group `--replacement` selector and new report passed
complete-category UBSan. An independent signed-overflow canary returned 1
with the intended UBSan diagnostic under the same flags. This is
**replacement-only UBSan**, not a full-runtime claim or resolution of the
existing sanitizer backlog.

Eight isolated mutations compiled and returned 1 for their exact intended
assertions, rather than passing through a compile failure, timeout,
or unrelated trap:

| Mutation | Assertion that failed |
|---|---|
| Make `sub` global | First/global scope and preserved surrounding text |
| Make `gsub` first-only | First/global scope and preserved surrounding text |
| Select sed formatting | Expected C++ group expansions |
| Omit unmatched spans | Preserved surrounding text |
| Truncate the subject at NUL | Counted binary subject/format results |
| Truncate the format at NUL | Counted binary subject/format results |
| Skip the nonempty retry | Standard empty/nonempty progression |
| Swallow allocation failure into empty output | Caller assignment preserved on failure |

Production controls passed before and after; all 56 source hashes,
the protected draft, and pinned reference inputs remained unchanged.
Control sources/logs remain session artifacts. Twenty specific local
temporary executables, objects, and transfer-archive files, plus the Mac
transfer archive, were removed.

All twelve source pairs pass through checksum-pinned copies of the actual
CLI and its companion archive:

- compiler: `4992d51d4e37c9cf90ae3b3313c89e3e9712b6110d6163115dbc673aa9a10b22`
- archive: `d26debdce3f691f0880d5a2745bdf2e1f7c875815c25a8e4fc676bba48f10dfa`

Four additional independent input cases pass in both implementations:
an ordinary miss with literal dollar/backslash bytes, three assignments,
the positive `NEVER` replacement sentinel, and empty-input attestation
failure. The report comparison does not impose native's deliberate
first-NUL subject boundary on C++.

## Scope

This completes first/global replacement, not bulk reads/splitting,
remaining formatting, general redirects, public wait, forked-C++/
background ownership, or source-to-C++ emission. Native-template translation
must eventually cover evaluated strings and literal dollars/backslashes,
not merely C++ source-literal escaping. The protected producer proposal
remains unchanged, untracked, and excluded from transfer and publication.
