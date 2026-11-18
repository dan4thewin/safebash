title: Forced reads, not trusted reads
date: 2026-11-18

# Forced reads, not trusted reads

DG objected to describing `:!` reads as trusted: the `!` forces a read
past a guard; it does not establish that the data is safe. DG selected
**forced read** and **per-read taint override**, and authorized the
terminology change in DDs, maintained documentation and source comments,
leaving historical records alone.

The AI updated DD91's canonical definition and DD93's checked/unchecked
vocabulary, then reconciled the related current DD prose, guides, manuals,
outline and tour. The rule is unchanged: the override belongs to this
subject read, not independent index/operand reads, and does not validate
bytes or clear the source binding's outstanding taint. Dispositions and
other checks remain independent. Regex-pattern prohibitions still apply;
the caught-scratch explanation specifically names forced **data** reads.

The source changes are explanatory comments in ten Rust files and seven
spec files. Internal names such as `Trusted`, `taint_off` and the parser's
trust-named helpers remain; so do diagnostics, test IDs, labels and fixtures.
Actual attest language and unrelated trust-boundary descriptions are not
renamed. Existing journals, tasks, audit records, timeline and postmortem
are unchanged. Verbatim quotations and already-struck DD history retain
their original wording.

## Verification

The final preservation check covers 1,255 historical files, all 36 changed
Markdown files, 29 DD metadata blocks and 168 local links. Fenced code,
blockquotes and struck payloads are unchanged; DD statuses are preserved.
All three edited manuals render without warnings. The 17 source/spec files
retain their non-comment content and line counts. Bash's non-executing
pretty-printer independently produces identical before/after parses for
all seven specs, including their quoted fixtures.

`NEW=0 FLIPS=0 RETIRED=0 TOTAL=12573 timeout 900 mk -p 1 gate`
passes at rows **7271251-7283823**: **12,560 passing**, **13 existing
skips**, native **6,398**, C **6,175**. No failures, status flips, first-ever
IDs, retirements, ghosts or plan mismatches; liveness is 245.

The tour renderer then runs all **70 programs**, including **15 intended
failures**, with zero mismatches. Every program and recorded output is
unchanged; only explanatory text and the forced-read heading change.
After the final same-length SSA comment repair, a serialized `mk test`
also passes: Rust **241/27/20**, regex **152**, getopt **5**.
No new behavioral tests, reversion campaign, coverage measurement or
interactive-layout check is claimed for this wording-only change.

The baseline is `c12a3586` on `glob-scratch-trial`. Runtime, lockfiles,
build rules and tour machinery remain unchanged. This does not adopt
DD131/DD132 on main or change their provisional status.

Private evidence is in the session's `forced-read-wording-20261118`
directory.
