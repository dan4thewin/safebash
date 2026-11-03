# Owned NUL record reads

**2026-11-03.** DG authorized the proposed bulk-read/string-splitting unit,
retaining native DD#99's NUL-only, no-flags model. The AI implemented the
shared value helpers, four fixture groups, and the thirteenth harmless
[C++](../../sash-cpp/examples/record_report.cpp)/
[sash](../../sash-cpp/examples/record_report.sash) report.
[DD#13](../decisions/DD13-nul-record-reads.md) records the contract and the
AI's reversible API/collector choices. No IR reader, emitter, or native
implementation changed.

## One splitter, buffered stream handoff

`read(std::string_view)` returns owned string records in a dense array
beginning at index zero. `read(int fd = 0)` borrows its descriptor through
EOF, defaulting to stdin. An existing command/pipeline callback calls its
`LineReader::read()` so that bytes prefetched by `readline` are consumed
before any further syscall.

NUL alone terminates records. Real leading/interior empty records remain;
the final NUL adds no synthetic field, and a nonempty unterminated tail
survives. Empty input/EOF returns an empty array. Assignment replaces all
old cells, without a stale suffix. Scalar strings and string-cell values
share the operation; newline, CR, whitespace, and high bytes stay data.
Maps remain separate array-to-map conversions.

The simple first collector gathers remaining buffered/stream bytes into a
local string before invoking that same splitter. It temporarily holds the
whole input plus owned fields instead of adding an incremental parser or
stream framework. Dense indices reuse the checked collection-count helper.
A private refill method shares the existing EINTR retry; bulk errors are
labeled `read NUL records`, while earlier read diagnostics stay unchanged.
Borrowed descriptors are neither closed nor reflagged.

I/O, allocation, and length errors escape before a partial result reaches
the destination, including when the source is borrowed from that destination.
Consumed bytes are not rewound, and retry has no transaction guarantee.
DD#3's producer outcome is still separate: failure after a completed
consumer assignment does not roll back that assignment. Command/pipeline
owners close before waiting on ordinary and exceptional paths.

## Report and targeted failures

The report retains a real function, caller-owned array, and ordinary field
traversal. `Ada||Grace|Linus|` is converted upstream to counted NUL bytes,
giving four fields, including one empty field. External printf produces
two NUL records; the read replaces a deliberately stale cell, joins the
saved records, and clears them with empty scalar input. The independent
ten-line golden was specified before execution.

The fresh actual CLI rejected the initial `$'\0'` spelling with SB4063:
octal byte escapes are retired. The source now uses its required
`$'\x00'`; external printf's ordinary quoted `%s\0` format is unchanged.
All thirteen source pairs then passed. Own external records are attested
inside the producing function, retaining DD#127 and passed-slot access.

Fixtures pin counted partial views, scalar/cell parity, owned results,
replacement, binary data larger than pipe capacity, repeated EOF,
buffered line-to-bulk handoff, and borrowed stdin/fd flags. A real
nonblocking pipe keeps its writer open after a short prefix: the next
read raises EAGAIN, preserves the old array, and a raw read proves the
prefix was consumed. Closing the writer then produces real EOF.

The exact field-copy allocation fault derives from a healthy long field's
capacity plus one. A complete small-field positive control passes under
that same fault; a later long-field copy must fail without publishing
earlier records. The collector fault starts inside the callback, after
launch. It uses the inspectable producer form so a mutated collector cannot
hide a swallowed allocation failure behind a later SIGPIPE verdict.
The original allocation exception escapes, the old array remains, and
`waitpid` confirms the producer was reaped.

## Executed evidence

The predictions were four additional groups (**58 to 62**), one golden
(**twelve to thirteen**), and three project files (**56 to 59**).
GCC, Clang, supported Linux ASan, real Apple Clang/libc++, and Apple ASan
produced those totals. Both Linux compilers retained the fault gates and
unchanged **169-case** catalog. Formatting passed.

The Mac's published 56-file baseline matched before transfer and again
before extraction. The frozen 59-file snapshot produced **124** combined
Apple/Apple-ASan group results, thirteen goldens per gate, and **118**
matching before/after source-hash lines. The new report's stderr was
explicitly checked empty on every compiler/sanitizer build.

The four-group `--nul-read` selector and record report passed
complete-category UBSan. An independent signed-overflow canary returned 1
with the intended UBSan diagnostic under the same flags. This is
**NUL-read-only UBSan**, not a full-runtime claim or resolution of the
existing sanitizer backlog.

Ten isolated mutations compiled and returned 1 for their intended assertions:

| Mutation | Pinned failure |
|---|---|
| Drop empty records | Real empty NUL records retain their slots |
| Add a final synthetic record | Terminators do not add another record |
| Drop the unterminated tail | Nonempty final data survives |
| Also split on newline | Newline/CR/high bytes remain data |
| Truncate the counted source at NUL | Counted NUL record boundaries survive |
| Discard prefetched bytes | Line-to-bulk handoff retains the buffered tail |
| Treat post-prefix EAGAIN as EOF | I/O failure cannot publish partial records |
| Swallow field allocation failure | Old caller array remains unchanged |
| Swallow collector allocation failure | Consumer failure escapes the inspectable form |
| Close the borrowed fd | Stdin/fd ownership and flags remain with the caller |

Production controls passed before and after. The control runner's initial
link failure was a binary/directory path collision; distinct `.bin` paths
fixed the session runner, not production code. No build failure counted
as a rejected mutation. The extra-input runner likewise required Python 3
rather than the host's legacy `python`.

Four independently specified extra inputs pass in both implementations:
one terminator, an unterminated tail, repeated empty boundaries, and spaces
as ordinary field data. All thirteen source pairs use pinned copies of
the actual compiler and companion archive:

- compiler: `5fd4fbcdf0ab5ef76e3cd5cd86bca1eed77559b4c6295867623b5a77e2a5f04d`
- archive: `d26debdce3f691f0880d5a2745bdf2e1f7c875815c25a8e4fc676bba48f10dfa`

All 59 source hashes, protected-draft bytes, and pinned reference inputs
remained unchanged. Thirty-eight exact local temporary control/build/archive
files and the exact Mac transfer archive were removed; sources/logs remain
session artifacts.

## Scope

This completes NUL bulk reads and string splitting, not remaining
formatting, general redirects, public wait, forked-C++/background ownership,
or source-to-C++ emission. Source statement/condition disposition remains
consumer call-site work. No flags, taint/admission machinery, hidden
checkpoints, cancellation, timeout, or automatic END policy were added.
The protected producer proposal remains unchanged, untracked, and excluded
from transfer and publication.
