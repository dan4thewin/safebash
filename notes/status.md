title: Status
modified: 2026-11-17

# Current status

## Focus

The integrated DD131/DD132 trial is underway on `glob-scratch-trial`,
branched from `6c7b899f`; main is unchanged pending DG's evaluation.
The independent const-validation prerequisite, source-identity immutable
scratch construction, atomic accumulating ordinary fillers and coarse
`glob` with ordered list sugar are complete on the trial branch, as are
the quiet actual-engine `r?` predicate, rejection of trusted pattern reads,
mandatory immutable-pattern proofs, caught-scratch read/failure semantics
and regex text-plus-Boolean results. Guarded redirect-cell indices now use
shared ordered lowering rather than reaching an unlowered-value ICE.
`rglob` and pattern-list splats are complete too, including empty-list
success without directory work. The integrated evaluation is now recorded in
[journal 654](journal/654-glob-scratch-regex-evaluation.md): useful programs
work, but unchanged sequential mapfile conversions silently retain old
entries, and repeated append has substantial scan/clone costs. Hand
adaptations are not a converter repair. Next is DG's evaluation of those
tradeoffs and adoption/revision/withdrawal decision; no migration or
optimization policy has been selected.
DG settled underscore-only construction,
initializer self-read rejection and bare-literal-only glob sugar.
This is a current handoff,
not a second journal: keep the dated account in [journal/](journal/) and
delivery boundaries in the individual [task records](tasks/).

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

- **Integrated trial evaluated, not adopted.**
  Eight practical scenarios run in both backends, with reviewed migration
  examples and 58 resource configurations across 17 workloads. Two-cell
  append at 4,000 iterations takes about 328 ms rather than main's 2.5-2.7 ms;
  its 1,000-iteration configured-heap bracket roughly doubles. Repeated
  scalar regex operations exhaust 64 MiB before 4,000 iterations on main
  too, separately from the trial's append cost. All 29 affected DD paths
  are accounted for; provisional dispositions remain pending.
  [Journal 654](journal/654-glob-scratch-regex-evaluation.md) records the
  evidence and recommendation to keep the package off main while reconciling
  converter compatibility and addressing or accepting the append cost.
- **Regex directory selection delivered on the trial.**
  `rglob` matches basenames with validated immutable regexes and shares
  sorted, atomic publication with `glob`. Both accept pattern-list splats;
  empty expansion performs no directory work but retains operand effects.
  Directory/pattern snapshots, list/default proofs, independent native/C
  calls and runtime release/failure boundaries are pinned.
  [Journal 653](journal/653-regex-directory-selection.md) records the unit.
- **Regex results and executable redirect indices delivered on the trial.**
  Scalar `match`/`sub`/`gsub` bind text plus matched/no-match truth; direct
  capture-conditions consume both, otherwise explicit discard is required.
  Substitution sugar and basename/dirname remain text projections.
  Guarded cell-index captures preserve arguments-first, ordered redirect
  evaluation and stage-child ownership. FD-only `exec` keeps changes only
  after the complete redirect list succeeds.
  [Journal 652](journal/652-regex-results-and-redirect-indices.md) records
  the two failed draft gates, diagnostic repairs and final acceptance.
- **Caught-scratch permissions enforced on the trial branch.**
  SB5107 limits ordinary reads to the caught attest's success arm, including
  negated/logical forms; failed data stays intact and can be read explicitly
  on trust. Ordinary bindings retain R2, and handlers receive actual incoming
  restrictions. Trusted-splice metadata and signal deferral through acquisition,
  but not indefinitely after failed predicates, are repaired.
  [Journal 651](journal/651-caught-scratch-permissions.md) records the unit.
- **Immutable runtime regex proofs enforced on the trial branch.**
  SB5106 requires a successful check of the same immutable value on every
  reaching path, including helper, collection and process uses. Caught
  scratch regex proofs cannot escape their lexical arm through copies.
  Five older fixtures and the tour's mutable fence pattern were migrated;
  all 56 tour examples still render identically.
  [Journal 650](journal/650-immutable-regex-proofs.md) records the unit.
  Journal 651 supplies the ordinary-read/failure follow-up, and journal 652
  supplies regex result channels.
- **Trusted regex-pattern reads rejected on the trial branch.**
  SB5105 covers every existing consumer, including substitution sugar,
  nested indices, arithmetic and capture bodies, independently of taint
  switches. Trusted subjects, replacements and `r?` candidates remain legal.
  The ERR fixture now distinguishes this earlier refusal from its original
  dynamic-regex safety rule. [Journal 649](journal/649-regex-pattern-trust-guard.md)
  records this prerequisite; journal 650 supplies the immutable-value proof.
- **Quiet regex validity implemented on the trial branch.**
  `r?` tests the complete counted candidate with the real engine; invalid
  syntax is quiet false, while allocation failure remains fatal.
  Trailing backslashes and overflowing repetition counts now reject.
  Temporary candidate/program storage is reclaimed. Later trial units supply
  proof and result handling; journal 653 supplies `rglob`.
  [Journal 648](journal/648-quiet-regex-validity.md) records the unit and
  the independently filed map-literal element-loss defect.
- **Coarse directory selectors implemented on the trial branch.**
  Explicit `glob` and bare-word array/append/loop sugar share bounded
  patterns, bytewise per-pattern sorting, absolute paths and private
  publication. Quoted/computed list words remain data; earlier word/list
  values survive later capture effects. The full gate caught an append
  allocation regression, repaired by consuming private metadata, avoiding
  redundant initialization and using allocation-before-publication for
  one-cell batches. [Journal 647](journal/647-coarse-directory-selectors.md)
  records controls, three contract migrations and the remaining costs.
- **Ordinary array fillers accumulate atomically on the trial branch.**
  `argv`, array `read`, `keys`, `match` and `matchall` append complete
  private batches, preserving aliases and the prior contents on error.
  Sparse append no longer overwrites surviving keys; integer literal
  append/replacement preserves integer-cell encoding. Old taint and
  ownership survive clean fills, including through helpers and ERR analysis.
  [Journal 646](journal/646-atomic-accumulating-fillers.md) records
  migration, genuine late-failure controls and the remaining trial costs.
- **Immutable scratch construction implemented on the trial branch.**
  Source sites create fresh scalar/collection instances; mutation and
  initializer self-reads reject through helpers and mapped slots too.
  Allocation-free resets precede fallible work, and scratch collection
  fills publish only completed private batches. The later DD132 proof and
  caught-read units are recorded above.
  [Journal 645](journal/645-immutable-scratch-construction.md) records
  the native/C delivery, failure-state controls and branch boundary.
- **Const validation separated from writes.** Otherwise-legal full/shorthand,
  standalone/caught attests and conversion sources now accept consts.
  Real writes retain SB4044 and the lowering invariant; taint, adjacency,
  ownership and ordinary failed-path obligations are unchanged.
  [Journal 644](journal/644-glob-trial-and-const-validation.md) records
  delivery and the three newly settled trial details.
- **Tour eval diagnostic corrected.** SB4001 now ends lookup for its rejected
  command head, without hiding independent missing-command errors. The existing
  dynamic-execution classifier remains the single authority.
  [Journal 643](journal/643-eval-diagnostic-cascade.md) records the repair,
  before/reversion proof and regenerated tour.
- **Plan 9 regex boundaries repaired.** `^`/`$` anchor the original
  regex subject, not internal lines or resumed suffixes; `$` has no
  trailing-newline exception. Global operations include empty/terminal
  matches without searching past the first NUL or altering its suffix.
  Capture layout, leftmost-longest selection and dot/negated-class newline
  exclusions remain unchanged. No engine dependency or flag was added.
  The still-built `sash-regex` helper follows the same cursor contract.
  [Journal 642](journal/642-regex-subject-boundaries.md) records delivery
  and closes the [P1](tasks/bug-regex-line-anchors.md).
- **Direct temporary-directory values and tour cleanup delivered.**
  `$(mkdtemp)` creates an exclusive owner-only directory under fixed `/tmp`;
  its clean, NUL-free pathname and operational status are returned directly.
  Ordinary failure aborts before assignment; handled failure binds empty text
  and failure, with fatal allocation still checked before creation.
  The tour uses the constructor and direct guarded cleanup argv, preserving
  sibling-file staging and atomic publication.
  [Journal 641](journal/641-mkdtemp-and-tour-cleanup.md) records delivery
  and completes DD135 without changing the IR schema.
- **Readonly ERR imports and local NUL proofs delivered.** SB5102 rejects
  inherited writes through same-process calls and aliases while preserving
  fresh private and ERR-only state. The localized SB5101 prover follows
  actual values, predicate outcomes and every reaching path; runtime argv
  guards and taint remain independent.
  [Journal 640](journal/640-readonly-err-and-local-nul-proofs.md) records
  before/reversion evidence, the corrected draft-gate failures and acceptance.
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
- **ERR input/path policy implemented.**
  [DD135](decisions/DD135-err-readonly-inputs-nul-and-mkdtemp.md) adopts
  readonly inherited ERR bindings, contains-NUL `0?` (false on empty),
  NUL-rejecting filename tests, local ERR byte proofs and clean fixed-`/tmp`
  `mkdtemp`. All three delivery units are recorded above.
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

- [Multiline regex syntax](tasks/regex-multiline-mode.md) is a deferred
  decision, not an implemented flag. Literal-regex caching and diagnostics,
  general dynamic-pattern failure handling and the DD131/DD132 trial remain
  separate from the completed default-anchor repair.
- DD135's three implementation owners are complete. The broader
  [failure classification](tasks/runtime-failure-classification.md) and
  [recoverable file handling](tasks/recoverable-file-failures.md) remain
  independent work; constructor-local error classification does not close them.
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

The generated block records journal 653's regex-selector gate:
12565 checks, no failures, 116 first-ever mode IDs, no flips or retirements.
All 12449 accepted regex-result baseline statuses are unchanged.
Its commit label is the pre-publication HEAD `f61aecad`, not the
implementing commit.

<!-- BEGIN: mk-status (regenerated by contrib/status-update.sh) -->
## Current branch: glob-scratch-trial

## Spec status: 12552 passing / 0 failing of 12565 (spec.db @ f61aecad)

_Regenerated 2026-11-17 by `mk status`. Edits inside this block will be
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
