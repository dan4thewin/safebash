# 637 - One tour, interactive and recorded

DG requested a tour usable both in presenterm and as a static GitHub page.
He approved a generator, asked that the second slide clearly limit its
keyboard instructions to presenterm, and requested terse end-of-line
comments on the lines intended to fail.

The AI extended the existing native checker rather than adding another
runner. `docs/tour/tour.md` remains the single authored source;
`docs/tour/README.md` is its generated, checked-in GitHub edition.
No compiler or runtime behavior changed.

## Rendering and publication

`check --render` shares the ordinary checker's snippet extraction and
execution. Visible source and prose remain, hidden `/// ` setup executes
without being displayed, and each program gains a literal block containing
combined stdout/stderr. Expected failures are labeled. Output containing
backticks gets a sufficiently long fence; empty output is stated explicitly.
Presentation-only control comments are omitted and slide titles become
ordinary Markdown headings.

This is deliberately a renderer for the current tour format, not a general
Markdown or YAML implementation. Unsupported executable fences, unfinished
frontmatter/fences, missing titles and a tour without runnable programs
refuse publication. The checker accepts no option, `-v`, or `--render`;
unknown or excess options return usage with status 2.

The final converter example now uses `+exec:sash` too. Its safebash program
creates Bash input and launches `sash-convert`; the launcher does not need
a separate Bash executor. All **56** examples therefore use the same runner,
including the example omitted by the previous sash-only checker.

Render mode fixes `USER=tour`, `LC_ALL=C` and `NO_COLOR=1`; both modes retain
the existing `world 42` program arguments. Four slide-specific output fields
use disclosed placeholders: glob scratch directories, job PIDs, kernel
release and hostname. Ordinary numbers and text elsewhere are preserved.
Each affected slide must produce a recognizable field rather than silently
skipping normalization. Glob paths are compared literally using their
directory, so regex metacharacters in `TMPDIR` are not pattern syntax.
Ordinary checking and presenterm keep their inherited environment.

The complete candidate is written to a sibling `.README.md.XXXXXXXXXX`.
Only a successful complete render sets its public permissions and renames
it over `README.md`. `mv -T` refuses a destination directory rather than
silently placing the candidate inside it. An unexpected failure, unexpected
success, malformed tour or failed publication leaves the previous edition
untouched. EXIT/ERR remove owned staging and working files.

Cleanup retains the existing NUL-delimited `write | xargs -0` route.
DD135's direct ERR argument proofs and `mkdtemp` are still pending; this
generator does not implement or pretend to use them.

The mk targets share the fail-fast `tour-helpers` prerequisite:

```sh
mk -p 1 tour       # build helpers and launch presenterm
mk -p 1 tour-doc   # build helpers and publish the GitHub edition
```

All 13 expected failures now mark their responsible source line. The second
slide and the later FizzBuzz instruction qualify `Ctrl+E` as a presenterm
operation; static readers are directed to regeneration instead.

The first implementation attempted a dynamic substring offset and received
SB4047. The compiler's existing constant-offset contract was correct; the
generator now uses `dirname`/`basename` for that variable-length prefix.
No compiler relaxation or unrelated fix was made.

## Focused evidence

Six `tourgen` specs extend `spec/harness.sh` from plan 3 to 9. They compile
the actual `.sb` helpers through the real CLI in native and C modes, then
exercise a fixture with the same executable-relative directory layout.
The C run compiles the helpers through C; snippet execution still uses the
ordinary native compiler, matching the shipped helper.

The cases cover visible/hidden content, combined output and safe fences;
repeatable normalization with a metacharacter-containing temporary path;
both expectation mismatches; malformed/empty input; unsupported executors
and options; and refusal to publish into a directory. Output assertions
include their output fence so displayed source cannot satisfy them.

Original-helper and independent reversion controls ran in isolated copies,
never by stubbing shared product sources or binaries. The final red sets
are identical in both modes:

| Control | Red `tourgen` IDs |
| --- | --- |
| Original checker | 1, 2, 5, 6 |
| Publish despite a mismatch | 3 |
| Remove the four normalizations | 2 |
| Fixed triple-backtick output fence | 1 |
| Remove `mv -T` | 6 |
| Remove candidate-file cleanup | 3, 4, 5, 6 |

The first unconditional-publication control also failed cases 4 and 5:
they reused the sentinel overwritten by case 3. The recorded xtrace exposed
that fixture dependency. Each now reinstalls its own sentinel before its
case; the final controls produce exactly the table above.

The actual `mk -p 1 tour-doc` completes with **56 snippets, 0 mismatched**.
Its README is byte-identical to the preceding full render. Ordinary `-v`
checking also completes with 56/0. GFM parsing finds 56 Bash source blocks
and 56 text output blocks; all 13 expected-failure outputs were reviewed
against their marked lines.

Seven isolated recipe cases cover successful document generation and
presentation launch, failure at each of the three helper compilations,
mkdir failure, and a failing generator. No failed prerequisite runs a later
compiler or launcher; the document target calls only `check --render`.
A separate live TERM control observes both a staged candidate and a running
snippet before interruption, then confirms the old README is unchanged and
both staging and working directories are gone.

These checks validate rendering and expected success/failure, not the
semantic correctness of arbitrary output. The checker retains its existing
capture newline behavior and has no new per-snippet timeout; bounded checks
run it under `timeout`.

## Ordinary gate

```sh
NEW=12 FLIPS=0 RETIRED=0 TOTAL=11387 timeout 900 mk -p 1 gate
```

The first full gate passes. Rows **6936251-6947637** contain 11,374 passing
checks and 13 existing skips: native 5,805, C 5,582. Exactly the twelve new
`tourgen` mode IDs are first-ever; there are no failures, flips, retirements,
ghosts or plan mismatches. Liveness remains 251. The included unit runs pass
239 compiler, 27 converter and 20 expansion tests; the C regex suite passes
152 checks. Static ID/accounting/SB checks, shell syntax and whitespace pass.

The compiler, runtime archive, both local lockfiles, `sash-snippet.sb`,
`tour.sb` and presenterm configuration retain their baseline bytes.
No coverage measurement, compiler policy implementation or backlog batch
was started. `notes/status.md` points at both tour editions and records this
gate. Private logs, fixtures, control outcomes and input identities are
retained under the session's `tour-static-20261115` evidence directory.

## README entry point, 2026-11-15

After DG's requested fast-forward of `main` to `55270359`, he asked to
highlight the tour in the main README. A prominent "Take the tour" section
now precedes the small example, linking directly to the recorded GitHub
edition and giving `mk -p 1 tour` for the interactive presenterm version.
The existing introduction, example, build instructions and licensing text
are unchanged, as are the tour source and generated edition.

This documentation-only follow-up reuses the implementation gate above;
no new spec sweep or coverage measurement was needed.
