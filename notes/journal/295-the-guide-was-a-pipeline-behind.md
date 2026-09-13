# 295 — The guide was a pipeline behind

`guide-architecture-stale-cst` was filed with three specific line numbers and
a note at the end I nearly skipped:

> Worth a single pass over the whole document rather than more spot fixes: a
> reader currently cannot tell which statements are current.

That note was the task. The three sites were a sample.

## What "stale" turned out to mean

The filing said `docs/GUIDE.md` mentions `lower.rs`, which does not exist. It
does. It also lists `transform.rs`, `codegen.rs`, `ssa_printer.rs`,
`printer.rs`, `emit_source.rs`, `taint.rs`, `validator.rs`, `rule.rs`,
`rules/` and `known_externals.rs` — ten names, of which one (`known_externals.rs`)
moved to another crate and nine are gone. `transform` and `codegen` are
directories now, of thirteen and twelve files.

Running the other direction is worse. `parse/` — the entire front end, six
files, the thing that replaced the deleted pipeline — was not in the crate
tour. Neither were `resolve.rs`, `ssa_taint.rs`, `regex_validate.rs`, or
`emit_c.rs`.

I checked both directions mechanically at the end rather than by reading,
because I had already learned in this document that reading is how it got
this way.

## The C backend was not in the Backends section

This is the one that bothers me. `docs/GUIDE.md` has a section called
"Backends". It documented the native backend, the IR capture, and source
emission. It did not mention `--emit-c`.

Half the spec suite runs through the C backend. Every sweep in this session's
journals reports two lines, `c|passing` and `native|passing`, because the
suite runs twice. It is the reason a codegen regression usually surfaces as
one failing spec in one mode instead of as a wrong answer in both — the two
backends share `sash-rt` and differ only in how SSA becomes machine code, so
a divergence localises the fault without any reference implementation needing
to exist.

A new contributor reading the Backends section would not have known it was
there. They would have found out by running the suite and wondering what `c|`
meant.

I wrote it up as a differential oracle rather than as a portability feature,
because that is what it is for. "We also emit C" undersells it to the point
of being misleading.

## Deleting a phase versus explaining where it went

"Phase 4: Lower" described `lower.rs` converting a CST to IR, and listed five
coercions it performed. The easy edit is to delete the section, since neither
the file nor the pass exists.

That would have been wrong. Someone reading this guide wants to know *where
coercions happen*, and the answer is no longer one place: the parser handles
the ones with a single IR form (`declare`, `let`, backticks, `[ ]`), and the
transform handles the ones that depend on types or resolution (glob→regex,
int/string, DD#23 command resolution). The section now says that, and says
the phase numbering is kept so that names in the journal still line up.

A deleted section teaches nothing. A section that says "this used to be a
pass, here is where its work went" answers the question the reader actually
has.

## The onboarding list was actively harmful

```
1. Read `JOURNAL.md` prologue (first 40 lines).
...
3. Run `mk test` to verify everything passes.
4. Pick a task from `TODO.md`.
```

`JOURNAL.md` does not exist. `TODO.md` does not exist. And `mk test` does not
pass at HEAD — `cargo test --test codegen_tests` hangs forever, because the
harness bypasses `validate` and fixtures using surfaces DD#99 retired compile
to binaries that never terminate.

So a new contributor following the guide's own instructions would: fail to
find the first file, hang on step 3, and fail to find the file in step 4. Two
of the five steps pointed at nothing and one was a trap.

It now points at `notes/journal/001-prologue.md`, `notes/status.md`, the spec
suite as the authoritative gate, `notes/decisions/` before changing anything
that might be deliberate — and, explicitly, **re-measure a task filing against
the binary before acting on it**. That last one is not general advice. It is
the single most repeated finding in this session: of the task files I have
picked up, most were materially wrong about something, and two had repros that
no longer compiled.

## AGENTS.md had the same disease

Since I was checking the crate tour anyway, I checked the one in `AGENTS.md`,
which is the file every AI contributor reads first. It listed `emit_janet.rs`,
`emit_source.rs` and `oracle.rs` as key source files, and `oracle_tests.rs`
and `janet_tests.rs` as test files. None exist. The test counts for the files
that do exist were all wrong. It said "57 design decisions … numbered up to
DD#69"; there are 95, up to DD#107.

The worst one: it hard-codes the complexity means as **cc 12.46 / cog 6.58**.
Measured today they are **4.04 / 2.05**. The gate is 3× the mean, recalculated
per commit, so the real bar is 12.1 / 6.2 — while the document tells you it is
37 / 20. Anyone sizing a refactor against the quoted numbers would be roughly
three times too generous with themselves, and would find out at the commit
hook. (I found out at the commit hook, in journal 291.)

The fix there is not better numbers. It is *no* numbers: the document now
carries the SQL query that produces them and says explicitly not to quote
them. A ratcheting threshold cannot be documented by value.

## Why documents rot in exactly this shape

Every stale statement in both files was true when written. Nothing here was
ever wrong. They rotted because the tree moved and a prose document has no
gate — there is no `cargo check` for "this file still exists", no spec that
fails when a section describes a deleted pass.

The two mechanical checks I ran at the end took about thirty seconds to write
and would have caught roughly two-thirds of what I fixed by reading:

- every `*.rs` named in the structure block exists somewhere in the tree;
- every file in `sash/src` is named in the structure block.

That is not a general solution to documentation rot. But it is the difference
between a crate tour that is *checkable* and one that is merely *checked, once,
by whoever last cared*.
