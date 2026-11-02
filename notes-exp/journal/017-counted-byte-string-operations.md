# Counted byte strings and the tenth pair

**2026-11-02.** DG authorized byte length/slicing, ASCII case conversion,
and the existing `p?`/`g?` predicates after numeric ingress. DG clarified
the example: slice `COUNT=08`, lowercase only the key, and strictly convert
unchanged `08`. No trimming, Unicode normalization, or malformed-number
repair was authorized. Native DD#12, DD#93, DD#102, DD#123 and the existing
substring/case implementations settle the operations.
[DD#10](../decisions/DD10-counted-byte-string-operations.md) records those
contracts and the AI's reversible representation choices. The AI
implemented the standalone helpers and harmless
[C++](../../sash-cpp/examples/string_report.cpp)/
[sash](../../sash-cpp/examples/string_report.sash) report.

## Implementation and boundaries

Six shared helpers use ordinary strings and counted views: `strlen`,
`substring`, `toupper`, `tolower`, `is_print`, and `is_graph`. Scalar
values and owned string-cell reads use the same operations.

Length counts bytes with the existing signed-i64 capacity check. Slices
own their bytes, including partial views and interior NUL. Positive offsets
clamp to the end; negative offsets count from the end and clamp to zero.
An optional signed length distinguishes omission (the remainder) from
explicit zero (empty). Positive length clamps to the remainder; negative
length trims its end. Unsigned bounded magnitudes handle both signed-i64
extremes without negating the minimum. Slices may split UTF-8 bytes.

Upper/lowercase copies change only ASCII letters and preserve every other
byte. Print requires nonempty `0x20..0x7e`; graph requires nonempty
`0x21..0x7e`. Ordinary false is a result, not an exception. These predicates
and byte length allocate nothing. Slice/case allocation failure escapes
without publishing an empty replacement. No helper adds a checkpoint.

The report retains a real function, explicit caller checkpoints, ordinary
conditions and live string-cell traversal. It demonstrates omitted versus
zero length, case conversion, byte counts, and print/graph results.
`COUNT=08` produces an independent seven-line golden.

## Evidence

Predictions were three additional groups (48 to **51**), one additional
golden (nine to **ten**), and three additional project files (47 to **50**).
GCC, Clang, supported Linux ASan, actual Apple Clang/libc++, and Apple ASan
produced those group/golden totals. Both Linux compilers retained the fault
gates and unchanged **169-case** command/capture catalog. Formatting passed.

Before transfer, the isolated Mac workspace matched all 47 numeric-baseline
hashes. Only the 50 source-only project files and their manifest were
transferred. Apple Clang 21/libc++ on arm64 Darwin and Apple ASan produced
102 group results; all 50 hashes matched before and after the two gates.
Exact report stdout and empty runtime stderr were checked on both hosts
and their sanitizer builds.

The three-group text selector and report also passed UBSan, including
signed-minimum slice bounds. An independent deliberately overflowing
signed-add canary returned status 1 with its intended diagnostic under the
same flags. This is **text-only UBSan**, not a full-runtime claim; the
existing sanitizer compatibility task remains open. No sanitizer category
or resource fixture was disabled.

Eight isolated mutations compiled, ran, and returned status 1 for the
intended assertion, not a compile failure, timeout, or unrelated trap:

| Mutation | Assertion that failed |
|---|---|
| Treat explicit zero length as omitted | Signed/clamped slices distinguish omission from zero |
| Interpret negative offset from the start | Signed/clamped substring bounds |
| Interpret negative length as positive | Negative length trims the remainder |
| Stop case conversion at NUL | Counted NUL/UTF-8/high bytes survive ASCII-only mapping |
| Accept an empty shape | Print/graph require nonempty input |
| Admit space as graph | Print/graph have distinct inclusive ASCII endpoints |
| Swallow slice allocation failure | Failure escapes without replacing the destination |
| Count bytes only up to NUL | Byte length and slices preserve interior NUL/high bytes |

Production controls passed before and after; all 50 local source hashes
remained unchanged. Mutation sources/logs remain session artifacts.
Temporary mutation/canary executables and both transfer archives were
removed.

## Real-CLI reference

All ten sash pairs pass using fresh dereferenced, checksum-pinned copies
of DG's supplied real compiler and companion archive:

- compiler: `5a1de01a24dd0444ca073791c0b17ffdb26a8af4424e3ff27e3744d9596825f8`
- archive: `d26debdce3f691f0880d5a2745bdf2e1f7c875815c25a8e4fc676bba48f10dfa`

Independent extra inputs confirm that mixed-case key `CoUnT` becomes
`count` while value `0XfF` stays unchanged and converts to 255. Valid zero
stays zero. Malformed numeric text, whitespace rejected by the example's
graph attestation, and an unexpected key all fail without repaired input
or partial report output.

The extra failure probe exposed an existing, recorded boundary rather
than a text-helper regression: DD#9's C++ required conversion raises
`CommandError(2)` with the established renderer, whereas the supplied
reference returns 1 for the same invalid required numeric text. The probe
initially expected 2 from both. The policies were checked and retained,
and the final probe records both statuses explicitly. No native behavior,
previous decision, or renderer was silently changed.

## Scope

This completes basic byte-string operations, not regex replacement, bulk
reads/splitting, remaining formatting, redirects, public wait, or
forked-C++/background ownership. There is still no IR reader/emitter or
producer implementation. No native compiler code changed. The protected
producer proposal remains unchanged, untracked, and excluded from transfers.
