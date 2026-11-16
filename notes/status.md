title: Status
modified: 2026-11-16

# Current status

## Focus

Implementing DD135's full package, directed by DG. The counted-NUL predicate
and filename guards are complete; readonly ERR imports/local proofs and
`mkdtemp` follow. This is a current handoff, not a second journal: keep the
dated account in [journal/](journal/) and delivery boundaries in the
individual [task records](tasks/).

## Outline and tour

[Language-specification outline](../docs/SPEC-outline.md): one terse nested
list, treating provisional and pending DD rules as complete for this draft
only. Original snapshot `2ef86c2a` remains available; the pedagogically
reordered draft and accepted-policy updates are now committed on `spec-outline`.
[Journal 630](journal/630-language-specification-outline.md) records the outline;
[journal 631](journal/631-symmetric-input-lookups.md) records the lookup change.
This is not a shipped-language reference or a completed specification.

The [GitHub tour](../docs/tour/README.md) contains recorded output from all
56 programs in the authored [presenterm tour](../docs/tour/tour.md).
Run `mk -p 1 tour` to build and launch the presentation, or
`mk -p 1 tour-doc` to regenerate the checked-in GitHub edition.
`target/tour/check` checks the same snippets without publishing.
Keyboard/reload instructions are explicitly presenterm-only; intentional
failures have terse comments on the responsible lines. Its map counter uses the
supported keyed `+=`, not arithmetic map-cell `++`.
The trust slide explains justified per-read `:!` uses:
validation in ordinary code or a content-independent operation such as `strlen`.
PID/status examples use named intrinsics with their punctuation aliases noted;
familiar function positionals are paired with the canonical `args` forms.
Attest examples lead with `p?`/`g?`, with a compound predicate for optional
printable input. The expanded regex section teaches `match`/`sub`/`gsub`
before their convenience spellings, with a regex attest, chained edits,
and the distinction between literal word backslashes and necessary quoting.

## Just completed

- **Counted-NUL predicate and filename guards delivered.** `0?` tests NUL
  presence, false on empty; existing full and shorthand attests use it without
  new taint exemptions. Every filename predicate rejects a NUL-bearing
  operand before lookup, including either side of `-nt`/`-ot`/`-ef`.
  [Journal 639](journal/639-counted-nul-and-filename-guards.md) records
  the native/C implementation, before/reversion controls and ordinary gate.
- **Backlog reconciled.** Removed 32 deferred Janet records; closed five
  completed and three superseded items. The cleanup left 148 non-terminal
  records, not 148 independent ready jobs. The empty-splat converter task now
  owns its still-obsolete guidance, not an unchosen language extension.
  [Journal 638](journal/638-backlog-retirement-and-closeouts.md) records
  evidence, historical retrieval and three proposed workstreams.
- **Single-source interactive and static tour delivered.**
  The root README highlights the GitHub edition and interactive launch command
  before its small example.
  The native checker now renders a GitHub edition with literal combined
  output, hidden setup omitted, and disclosed stable sample values.
  Publication replaces the old edition only after every expected outcome
  matches. The converter example uses the same native runner as the rest.
  [Journal 637](journal/637-static-tour-generation.md) records the generator,
  publication controls and ordinary gate.
- **Review set captured in seven coherent commits on `spec-outline`.**
  Completed compiler/runtime repairs, three policy packages, the reordered
  outline, tour deck and native helpers have distinct publication records.
  [Journal 636](journal/636-publishing-outline-tour-and-policy-work.md)
  records the grouping, current evidence and remaining boundaries.
- **ERR input/path policy recorded; implementation underway.**
  [DD135](decisions/DD135-err-readonly-inputs-nul-and-mkdtemp.md) adopts
  readonly inherited ERR bindings, contains-NUL `0?` (false on empty),
  NUL-rejecting filename tests, local ERR byte proofs and clean fixed-`/tmp`
  `mkdtemp`. Predicate/filename delivery is recorded above; the other two
  implementation units remain.
  [Journal 635](journal/635-err-inputs-nul-and-mkdtemp.md) records the decision
  and accounts for the earlier uncommitted code.
- **Native tour helpers delivered.** The `.sb` sources compile into
  `target/tour/` and use the presenterm configuration's native runner.
  A coupled runtime repair stops `readline` splitting long records at
  4,096 bytes. [Journal 634](journal/634-tour-helpers-in-safebash.md) records
  the original side-by-side port and its evidence; journal 636 records
  the current helper versions and publication.
- **Indexed updates repaired.** A write evaluates its key
  once; a compound update retains the old cell value before RHS effects.
  Arithmetic updates and ordinary indexed `+=` share the lowering. The inert
  IR is schema 3. [Journal 633](journal/633-indexed-updates-evaluate-once.md)
  records the repair and its evidence.
- **Loop-binder typing policy accepted.** DD134: `for v in @a` over an
  int collection binds integers, following ordinary assignment from a cell.
  Implementation is queued in
  [loop-binder-element-type](tasks/loop-binder-element-type.md);
  [journal 632](journal/632-loop-binder-element-type.md) records the ruling.
- **Symmetric input lookups accepted.** DD101 gives bare `arg`/`getenv`
  text plus presence status requiring handling or discard. Both literal-default
  forms return only text; defaults apply on absence, never present-empty.
  All four remain tainted. Implementation is queued in the expanded
  [require-getenv-status-handling](tasks/require-getenv-status-handling.md);
  [journal 631](journal/631-symmetric-input-lookups.md) records the ruling.
- **Named predicate capture policy accepted.** DD88 now permits an
  otherwise-legal predicate call to bind stdout and consume its explicit
  returned status, without weakening body checks, taint or same-process
  effects. Compiler implementation is queued in
  [predicate-capture-conditions](tasks/predicate-capture-conditions.md);
  [journal 628](journal/628-predicate-capture-conditions.md) records the ruling.
- **Decision rules made readable.** All 121 decision documents now begin with
  a concise numbered list of current requirements; superseded body claims are
  struck, with history and metadata retained. DG confirmed that capturing a pipeline
  does not exclude otherwise-legal user-function filters.
  [Journal 627](journal/627-current-decision-principles.md) records the pass.
  The earlier [metadata/naming repair](journal/626-note-record-conventions.md)
  and [top-level pruning](journal/625-top-level-notes-pruning.md) remain intact.
- **Public references refreshed.** README, architecture/review guides and
  `sash(1)` now describe the shipped system. Added `sash-getopt(1)`,
  `sash-expand(1)` and `sash-convert(1)`. The legacy grep sample/harness,
  Goose runners and external suites were retired; current tests remain.
  See [journal 622](journal/622-architecture-and-review-guides.md),
  [623](journal/623-compiler-command-manual.md) and
  [624](journal/624-companion-command-manuals.md).
- **Fresh-clone runtime build repaired.** Public rustix APIs and the shared
  Linux Rust syscall shim replace private runtime APIs. This is not a claim
  of full AArch64 compiler/runtime or cross-compilation delivery; those
  remain with the platform task below.

## Deferred work and boundaries

- DD135's [predicate/file-test prerequisite](tasks/nul-predicate-and-filename-tests.md)
  is complete. Its [readonly ERR/local-proof work](tasks/err-readonly-inputs-and-nul-proofs.md)
  and independent [mkdtemp unit](tasks/mkdtemp-value-intrinsic.md) remain in
  the authorized implementation package; none completes the broader
  recoverable-file-failure backlog.
- The [public language specification](tasks/specification-document.md) and
  [source license notices/Cargo metadata](tasks/apply-source-license-notices.md)
  are separate follow-ups, not completed by the documentation refresh.
- [Tracking/enforcing both Cargo lockfiles](tasks/track-cargo-lockfiles.md)
  and the [native host/cross-target matrix](tasks/cross-compile-and-setjmp-on-mac.md)
  remain implementation work.
- The companion review filed `sash-expand`
  [UTF-8 processing](tasks/bug-expand-utf8-value-processing.md) and
  [lone-quote](tasks/bug-expand-lone-quote.md) defects. Existing
  [CLI stdout-error](tasks/bug-stdout-write-failure-panics.md) and
  [converter annotation](tasks/bug-convert-annotate-midline.md) work remains.
- Accepted DDs are not necessarily implemented; task records carry delivery
  status. [DD131](decisions/DD131-directory-selectors-scratch-collections.md)
  and [DD132](decisions/DD132-validated-regex-immutable-scratch.md) describe
  the provisional selectors/scratch/regex package, not shipped features.
- The indexed-update fix does not complete the general
  [expression-order repair](tasks/q-capture-hoisting-order.md).
- Coverage remains paused pending DG's direction. Resumption requires a
  fresh union and provenance check, not the pre-port cold-line ranking.
  DD135 does not resume that work or unrelated implementation tasks.

## Last recorded gate

The generated block records journal 639's predicate/filename gate: 20 new
mode IDs, no flips or retirements. Its commit label is the pre-publication
HEAD `1aee5543`, not the commit containing the implementation.

<!-- BEGIN: mk-status (regenerated by contrib/status-update.sh) -->
## Current branch: main

## Spec status: 11394 passing / 0 failing of 11407 (spec.db @ 1aee5543)

_Regenerated 2026-11-16 by `mk status`. Edits inside this block will be
overwritten; edit outside the BEGIN/END markers._
<!-- END: mk-status -->

## Reading map

[README](../README.md) introduces the language and build.
[GUIDE](../docs/GUIDE.md) describes the shipped architecture;
[REVIEW](../docs/REVIEW.md) is the invariant/evidence checklist.
[AGENTS](../AGENTS.md) holds contributor procedure.
[Decisions](decisions/) are language authority; [tasks](tasks/) own remaining
work; [journals](journal/) preserve the narrative; [audit/](../audit/)
holds the audit records.

The [parser-migration postmortem](parser-migration-postmortem.md) is retained
for its decision-authority lessons, not as current technical guidance.
The full former status is available at
[`f02477dd`](https://github.com/dan4thewin/safebash/blob/f02477dd31e67a0fe6fe3c4a359935569c4715d8/notes/status.md).
