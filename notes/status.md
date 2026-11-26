title: Status
modified: 2026-11-26

# Current status

## Focus

The [DD137 array-slice filler](decisions/DD137-array-slice.md) is implemented
provisionally on `slice-trial`, from accepted main `727fe371`.
`slice SOURCE START [COUNT] DEST` selects present values in splat order,
preserves string/int type, and publishes an independent batch through the
existing accumulating/scratch rules. Main remains at the acceptance commit;
DG's later evaluation decides adoption.
[Journal 677](journal/677-provisional-array-slices.md) records delivery;
the [slice owner](tasks/research-array-slice.md) remains open for evaluation.

`$(mkdtemp [DIR])` is also implemented on `slice-trial`: an ordinary checked
string selects the existing parent; omission retains `/tmp`. Relative parents
produce absolute paths without changing symlink/`..` interpretation.
[Journal 678](journal/678-mkdtemp-parent-directory.md) records delivery;
main and the array-slice feature's provisional status are unchanged.

DG accepted the complete `err-attempt-trial` branch and directed its main
landing on 2026-11-26. [DD136 ERR-owned call attempts](decisions/DD136-err-call-attempts.md)
are no longer provisional. ERR-owned
`: void_helper` and caught status calls can abandon a recoverably failed
helper chain; ordinary suppression and terminal guards retain their limits.
Independent `_=` sinks replace colon's operand disposal. Named-predicate
capture conditions, failed-path taint/adjacency and fatal-fork cleanup are
included. The [trial owner](tasks/err-call-attempt-trial.md) and
[journal 664](journal/664-err-owned-call-attempts.md) record the scope and
evidence. [Journal 676](journal/676-accept-err-attempts.md) records acceptance
of all twelve branch commits and the main landing. The original trial
branch is retained; the integrated owner is complete.

DD101's symmetric `arg`/`getenv` contract is implemented in native and C.
Bare lookups require explicit presence handling or source discard; defaults
return only text, applying on absence but not present-empty input.
All four forms remain tainted. Converter hoists, incidental fixtures and
the tour are migrated. The [lookup owner](tasks/require-getenv-status-handling.md)
and B47's wrapped-lookup validation gap are complete;
[journal 663](journal/663-symmetric-input-lookups.md) records delivery.
Defaults now accept general string values, evaluated eagerly in source order
before the lookup, including when the supplied input wins. Ordinary type,
taint and failure checks remain; `:-`/`:?` are still literal-only.
[Journal 674](journal/674-general-lookup-defaults.md) records that extension
on main.
The broader suppression and file-read capture work remain separately owned.

The accepted DD131/DD132 selector, immutable-scratch, accumulating-filler and
validated-regex package is on main, including the append optimization and
expanded tour ([journal 661](journal/661-accept-glob-scratch-regex.md)).
The original `glob-scratch-trial` branch is retained.

[Converter migration](tasks/convert-glob-scratch-regex-migration.md) remains
open: unchanged sequential mapfile translations can silently retain old
entries, and names/proofs/result consumption need adaptation. Adoption
does not claim that repair or select its policy.
[Regex allocation/reuse](tasks/dd39-precompile-literal-regex.md) remains
separately owned, as does the
[Bash-compatible assignment loader](tasks/sash-expand-bash-assignment-loader.md).
[Runtime-key unset](tasks/runtime-indexed-unset.md) and the
[map-iteration deletion repair](tasks/bug-map-iteration-deletion-skip.md)
are implemented on main. DG selected snapshot-at-entry for
direct key loops; body value lookups remain live. Unset evaluates and
retains ordinary runtime keys before applying any deletion or clear.
[Journal 667](journal/667-snapshot-key-loops-runtime-unset.md) records
delivery and the updated DD75/DD80 contracts.

Existing string maps can now be replaced from string arrays, including
through direct and forwarded collection slots. Well-typed whole-collection
assignment updates the actual's contents without rebinding its identity.
Conversion publishes only a complete result: failure preserves an existing
map and leaves fresh construction empty. This is delivered on
main; [journal 668](journal/668-existing-map-and-slot-assignment.md)
records the implementation and the dependent converter/fixture repairs.
The independently frontend-covered
[SSA cell-write acquisition omission](tasks/ssa-taint-cell-write-acquisition.md)
remains separate P3 hardening.

Collection keys now use ordinary operand words in reads, assignments and
unset, including dollar/braced imports inside arithmetic. Bare arithmetic
cells retain arithmetic indices and admit wholly quoted map keys. Shared
key checking and lazy, once-only arithmetic lowering repair the associated
defects on main;
[journal 670](journal/670-operand-word-collection-keys.md) records delivery.
The [broader evaluation-order task](tasks/q-capture-hoisting-order.md)
remains open.

`cpath?` now validates conservative absolute-path spelling in ordinary
tests and scalar/collection attests on main. It checks
counted ASCII components without filesystem lookup or normalization;
it does not establish authorization or downstream interpreter safety.
[Journal 671](journal/671-conservative-path-predicate.md) records delivery.

Directly tested immutable scalars now have a bounded DD119 redirect
exemption on main: one proved read, optionally with literal
neighbors added at the redirect site. Lexically nested captures can use
the enclosing proof of that same binding. Copies, named helpers and cells
do not transfer it, and capture proofs do not escape to parents or siblings.
Taint and caught-read permissions remain independent.
[Journal 672](journal/672-direct-immutable-redirect-proofs.md) records the
original implementation; [journal 673](journal/673-lexical-capture-redirect-proofs.md)
records the capture-scope repair.

This is a current handoff,
not a second journal: keep the dated account in [journal/](journal/) and
delivery boundaries in the individual [task records](tasks/).

## Outline and tour

[Language-specification outline](../docs/SPEC-outline.md): one terse nested
list, treating still-pending DD rules as complete for this draft
only. Original snapshot `2ef86c2a` remains available; the pedagogically
reordered draft and accepted-policy updates are now committed on `spec-outline`.
[Journal 630](journal/630-language-specification-outline.md) records the outline;
[journal 631](journal/631-symmetric-input-lookups.md) records the lookup change.
This is not a shipped-language reference or a completed specification.

The [GitHub tour](../docs/tour/README.md) contains recorded output from all
73 programs in the authored [presenterm tour](../docs/tour/tour.md).
Run `mk -p 1 tour` to build and launch the presentation, or
`mk -p 1 tour-doc` to regenerate the checked-in GitHub edition.
`target/tour/check` checks the same snippets without publishing.
Lookup examples now distinguish presence from validation and explain
ordinary eager defaults; recorded converter output uses explicit presence
discard.
Three examples cover the accepted checked-disposal, void-attempt and
predicate-attempt rules, including partial output.
Keyboard/reload instructions are explicitly presenterm-only; intentional
failures have terse comments on the responsible lines. Its map counter uses
keyed `+=`; the new wholly quoted arithmetic map-cell syntax is not
demonstrated there.
The forced-read slide explains `:!` as a per-read taint override, not validation:
ordinary checks or a content-independent operation such as `strlen` can justify it.
PID/status examples use named intrinsics with their punctuation aliases noted;
familiar function positionals are paired with the canonical `args` forms.
Attest examples lead with `p?`/`g?`, with a compound predicate for optional
printable input. The expanded glob section demonstrates explicit selectors,
pattern lists, dotfiles, ordering, duplicates, list sugar and failure handling.
NUL-only `read` gathers and copies actual filenames containing spaces and
newlines from `find -print0`. Scratch is introduced beside the glob loaders:
ordinary targets gather batches, whereas scratch supplies only the current
directory's result. A separate second construction site is rejected.
The numeric import counts bad records without inventing fallback values.
Regex examples cover
whole-subject anchors guarded with `! 0?` against NUL truncation, `r?`,
immutable runtime-pattern proofs, fresh captures,
`rglob` and text-plus-Boolean results before the convenience spellings.
The selector/scratch/regex features are accepted on main; the converter-review
caveat remains explicit.
[Journal 656](journal/656-tour-globs-scratch-regex.md) records the expansion.
[Journal 657](journal/657-motivated-read-and-scratch.md) records the
NUL filename and numeric-import examples.
[Journal 658](journal/658-scratch-starts-with-the-loader.md) replaces the
rollout analogy with the bulk-loader contrast.
[Journal 659](journal/659-regex-tour-nul-guard.md) pairs the regex NUL
limitation with its guard and the combined-attest guidance.

## Just completed

- **The complete ERR-attempt branch is accepted on main.**
  DD136 and its companion rules are no longer provisional. All twelve
  branch commits are included, through the microsecond epoch clock.
  The original branch is retained, and independent backlog work stays open.
  [Journal 676](journal/676-accept-err-attempts.md) records the acceptance.
- **Microsecond epoch clock delivered.**
  `int now=$(epochuseconds)` returns clean Unix wall-clock microseconds in
  native and C, with the same arithmetic and quoted-string forms as
  `epochseconds`. It is not a monotonic timer; the seconds clock is unchanged.
  [Journal 675](journal/675-epoch-microseconds.md) records the implementation
  and its focused and full-gate evidence on `err-attempt-trial`.
- **General input-lookup defaults delivered.**
  `arg` and `getenv` accept variables, cells and computed string words as
  defaults. Shared lowering retains earlier operands before later effects
  and evaluates every argument before the lookup, in native and C.
  Present-empty input, value-only results, taint and literal dispositions
  retain their existing contracts.
  [Journal 674](journal/674-general-lookup-defaults.md) records the delivery.
  The existing intermittent `-N` timestamp-spec failure now has an explicit
  [investigation owner](tasks/bug-filetest-pinned-timestamp-failures.md);
  its cause is not established.
- **Immutable redirect proofs include lexical captures.**
  A nested capture can use the enclosing proof of the same live immutable
  binding, fixing the false SB4030 after an outer attest. Copies, named
  helpers, cells and parent/sibling escape remain outside the allowance;
  taint checks and fresh lifetimes are unchanged.
  [Journal 673](journal/673-lexical-capture-redirect-proofs.md) records
  the native/C controls, three independent reversions and full gate.
- **Conservative path predicate delivered.**
  `cpath?` shares one counted-byte validator across native and C.
  Root/trailing slash are allowed; dot/dot-dot components, leading
  hyphens, repeated separators and bytes outside the allowlist reject.
  Existing attest and ordinary-test taint semantics remain unchanged.
  [Journal 671](journal/671-conservative-path-predicate.md) records the
  before-controls, four reversions and exact full gate.
- **Operand-word keys and quoted arithmetic map cells delivered.**
  The second arithmetic-import grammar and position-dependent key checker
  are removed. Key/value typing, taint and rvalue-only imports remain;
  lazy arithmetic preserves earlier values, status and once-only subjects.
  Both key defects close, while general evaluation ordering stays open.
  [Journal 670](journal/670-operand-word-collection-keys.md) records the
  native/C before-controls, ten reversions and corrected gate.
- **Unexpected capture-taint findings backlogged, not repaired.**
  The existing P3 [buffer-eligibility task](tasks/q-buffer-capture-eligibility.md)
  now includes outer-negated tests represented as one-command pipelines.
  A separate P2 [read-source lowering bug](tasks/bug-read-source-capture-unlowered.md)
  records the unmaterialized substitution behind the resolver's SSA errors.
  [Journal 669](journal/669-capture-buffer-and-read-source-backlog.md)
  records the evidence; this task made no compiler or user-script edits.
- **Existing-map conversion and whole-collection slot assignment delivered.**
  Shared type/shape checks and private conversion publication cover direct,
  forwarded and permuted slots without relaxing readonly or taint rules.
  Converter integer operands receive explicit stringification where needed.
  [Journal 668](journal/668-existing-map-and-slot-assignment.md) records the
  before-controls, eight independent reversions and corrected full gate.
- **Snapshot key loops and runtime-key unset delivered.**
  Direct key loops retain their entry sequence despite map mutation;
  explicit body lookups remain live. Runtime unset uses static collection
  identities and ordinary checked indices, retaining all keys before
  mutation. Both owners close after native/C controls and the corrected
  gate. Mutable loops allocate snapshots; immutable maps avoid the copy.
  [Journal 667](journal/667-snapshot-key-loops-runtime-unset.md) records
  the implementation, exact-version taint guard and counter-fixture proof.
  The changes remain on `err-attempt-trial`, with main unchanged.
- **Nonliteral `unset` targets diagnose instead of crashing.**
  Early alias and taint analyses share fallible target decoding; checked
  typing and lowering retain their invariants. The earlier runtime-key
  SB5063 refusal is now superseded by the extension above.
  [Journal 665](journal/665-unset-validation-ice.md) records the repair.
- **ERR-owned call attempts delivered provisionally.**
  `: void_helper` and caught status calls in ERR recover from the selected
  narrow failure set after abandoning the helper chain and restoring owned
  resources. `_=` supplies checked disposal. Named-predicate captures,
  failed-path adjacency and fatal-fork cleanup are included; DDs, GUIDE,
  outline and both tour editions agree. DD136 stays provisional on
  `err-attempt-trial`, with main unchanged.
  [Journal 664](journal/664-err-owned-call-attempts.md) records delivery.
- **Symmetric input lookups delivered.**
  Bare `arg` and `getenv` expose presence with required handling/discard;
  defaulted forms are value-only. Present-empty, taint, operand/body
  boundaries, status reads and invalid-index failures are preserved.
  B47 is repaired through the canonical wrapped-call classifier.
  Converter hoists, fixtures, current DD/backlog guidance and both tour
  editions agree. [Journal 663](journal/663-symmetric-input-lookups.md)
  records the implementation and its acceptance evidence.
- **Bash-compatible `sash-expand` rework backlogged.**
  DG's assignment-file goal and tree-sitter suggestion are captured with
  strict non-executing evaluation, conventional help/stdin handling and
  final file-owned variable output. The UTF-8 and lone-quote bug records
  are coordinated, not closed. No helper behavior has changed.
  [Journal 662](journal/662-sash-expand-assignment-loader-backlog.md)
  records the discussion.
- **DD131/DD132 accepted and brought to main.**
  DG's adoption settles the former trial boundary. Current DD rules, GUIDE,
  tour and task pointers agree; existing journals and the evaluated trial
  branch are retained. The integrated owner and both glob repairs close,
  with remaining converter migration given its own owner.
  [Journal 661](journal/661-accept-glob-scratch-regex.md) records the closeout.
- **`:!` is a forced read, not a trusted read.**
  DDs, maintained docs, both tour editions and source/spec comments now
  describe the per-read taint override without implying validation.
  Behavior, identifiers, examples and existing historical records are
  unchanged. [Journal 660](journal/660-forced-read-terminology.md)
  records the wording change.
- **Regex validation now shows the NUL guard.**
  `0?` detects NUL, so `! 0?` precedes the anchored regex. The example
  accepts a plain word and rejects newline and NUL variants; tainted
  input uses both checks in one attest.
  [Journal 659](journal/659-regex-tour-nul-guard.md) records the correction.
- **Scratch tour is grounded in bulk loading.**
  The rollout pair is removed. Per-directory log counts introduce fresh
  scratch results beside accumulating ordinary loaders, followed by a
  rejected second construction site and the ordinary gathering alternatives.
  That revision brought the tour to 70 programs; all 69 retained programs and outputs were
  unchanged. [Journal 658](journal/658-scratch-starts-with-the-loader.md)
  records the revision.
- **Tour expanded for selectors, scratch and validated regexes.**
  Fifteen new programs and a fuller glob example demonstrate the recently
  delivered features and their important contrasts. The GitHub edition is
  regenerated from the single authored deck; the native helpers and
  presenterm configuration are unchanged.
  [Journal 656](journal/656-tour-globs-scratch-regex.md) records the examples
  and verification. The later adoption does not repair the converter.
- **Append scan/clone costs removed.**
  Maintained next-index metadata and whole-batch capacity reservation permit
  allocation-free publication by transferring owned keys. ERR still sees
  the old logical destination on failure; aliases and live iteration retain
  the same header. The matched 4,000 two-cell append workload drops from
  about 328 ms to 2.7-2.9 ms. At 1,000 iterations its configured-heap passing
  endpoint drops from 324 KiB to 164 KiB in both backends, not a peak-memory
  measurement. [Journal 655](journal/655-append-reservation-and-index-cache.md)
  records costs, sparse/deletion behavior and independent failure controls.
- **Integrated trial evaluated before adoption.**
  Eight practical scenarios run in both backends, with reviewed migration
  examples and 58 resource configurations across 17 workloads. Before the
  optimization above, two-cell append at 4,000 iterations took about 328 ms
  rather than main's 2.5-2.7 ms, with a roughly doubled 1,000-iteration
  configured-heap bracket. Repeated
  scalar regex operations exhaust 64 MiB before 4,000 iterations on original main
  too, separately from the trial's append cost. All 29 affected DD paths
  are accounted for; journal 661 records DG's acceptance.
  [Journal 654](journal/654-glob-scratch-regex-evaluation.md) records the
  original evaluation and recommendation. DG subsequently commissioned
  the append optimization and accepted the native package; converter
  compatibility remains a separate follow-up.
- **Regex directory selection delivered.**
  `rglob` matches basenames with validated immutable regexes and shares
  sorted, atomic publication with `glob`. Both accept pattern-list splats;
  empty expansion performs no directory work but retains operand effects.
  Directory/pattern snapshots, list/default proofs, independent native/C
  calls and runtime release/failure boundaries are pinned.
  [Journal 653](journal/653-regex-directory-selection.md) records the unit.
- **Regex results and executable redirect indices delivered.**
  Scalar `match`/`sub`/`gsub` bind text plus matched/no-match truth; direct
  capture-conditions consume both, otherwise explicit discard is required.
  Substitution sugar and basename/dirname remain text projections.
  Guarded cell-index captures preserve arguments-first, ordered redirect
  evaluation and stage-child ownership. FD-only `exec` keeps changes only
  after the complete redirect list succeeds.
  [Journal 652](journal/652-regex-results-and-redirect-indices.md) records
  the two failed draft gates, diagnostic repairs and final acceptance.
- **Caught-scratch permissions enforced.**
  SB5107 limits ordinary reads to the caught attest's success arm, including
  negated/logical forms; failed data stays intact and permits forced data reads.
  Ordinary bindings retain R2, and handlers receive actual incoming
  restrictions. Forced-read splice metadata and signal deferral through acquisition,
  but not indefinitely after failed predicates, are repaired.
  [Journal 651](journal/651-caught-scratch-permissions.md) records the unit.
- **Immutable runtime regex proofs enforced.**
  SB5106 requires a successful check of the same immutable value on every
  reaching path, including helper, collection and process uses. Caught
  scratch regex proofs cannot escape their lexical arm through copies.
  Five older fixtures and the tour's mutable fence pattern were migrated;
  all 56 tour examples still render identically.
  [Journal 650](journal/650-immutable-regex-proofs.md) records the unit.
  Journal 651 supplies the ordinary-read/failure follow-up, and journal 652
  supplies regex result channels.
- **Forced regex-pattern reads rejected.**
  SB5105 covers every existing consumer, including substitution sugar,
  nested indices, arithmetic and capture bodies, independently of taint
  switches. Forced subject, replacement and `r?` candidate reads remain legal.
  The ERR fixture now distinguishes this earlier refusal from its original
  dynamic-regex safety rule. [Journal 649](journal/649-regex-pattern-trust-guard.md)
  records this prerequisite; journal 650 supplies the immutable-value proof.
- **Quiet regex validity implemented.**
  `r?` tests the complete counted candidate with the real engine; invalid
  syntax is quiet false, while allocation failure remains fatal.
  Trailing backslashes and overflowing repetition counts now reject.
  Temporary candidate/program storage is reclaimed. Journals 650-652 supply
  proof and result handling; journal 653 supplies `rglob`.
  [Journal 648](journal/648-quiet-regex-validity.md) records the unit and
  the independently filed map-literal element-loss defect.
- **Coarse directory selectors implemented.**
  Explicit `glob` and bare-word array/append/loop sugar share bounded
  patterns, bytewise per-pattern sorting, absolute paths and private
  publication. Quoted/computed list words remain data; earlier word/list
  values survive later capture effects. The full gate caught an append
  allocation regression, repaired by consuming private metadata, avoiding
  redundant initialization and using allocation-before-publication for
  one-cell batches. [Journal 647](journal/647-coarse-directory-selectors.md)
  records controls, three contract migrations and the remaining costs.
- **Ordinary array fillers accumulate atomically.**
  `argv`, array `read`, `keys`, `match` and `matchall` append complete
  private batches, preserving aliases and the prior contents on error.
  Sparse append no longer overwrites surviving keys; integer literal
  append/replacement preserves integer-cell encoding. Old taint and
  ownership survive clean fills, including through helpers and ERR analysis.
  [Journal 646](journal/646-atomic-accumulating-fillers.md) records
  migration, genuine late-failure controls and the then-unoptimized costs.
- **Immutable scratch construction implemented.**
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
  decision, not an implemented flag. Literal-regex allocation/reuse and
  diagnostic improvements remain separate from the completed default-anchor
  and validation-before-use work.
- [Converter migration](tasks/convert-glob-scratch-regex-migration.md) remains
  open after native DD131/DD132 acceptance; the language trial itself is complete.
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
- The [Bash-compatible `sash-expand` loader](tasks/sash-expand-bash-assignment-loader.md)
  is queued, using stock tree-sitter-bash syntax and a restricted evaluator,
  with `/etc/os-release` as the concrete use case. Its
  [UTF-8 processing](tasks/bug-expand-utf8-value-processing.md) and
  [lone-quote](tasks/bug-expand-lone-quote.md) defects remain open. Existing
  [CLI stdout-error](tasks/bug-stdout-write-failure-panics.md) and
  [converter annotation](tasks/bug-convert-annotate-midline.md) work remains.
- Accepted DDs are not necessarily implemented; task records carry delivery
  status. [DD131](decisions/DD131-directory-selectors-scratch-collections.md)
  and [DD132](decisions/DD132-validated-regex-immutable-scratch.md) are now
  both accepted and implemented; that does not close unrelated pending DD work.
- The indexed-update fix does not complete the general
  [expression-order repair](tasks/q-capture-hoisting-order.md).
- Coverage remains paused pending DG's direction. Resumption requires a
  fresh union and provenance check, not the pre-port cold-line ranking.
  DD135 does not resume that work or unrelated implementation tasks.

## Last recorded gate

The latest trial gate, recorded in
[journal 678](journal/678-mkdtemp-parent-directory.md), has 13416
checks: 13403 passing and 13 existing skips, with no failures.
Rows 7557377-7570792 preserve all 13362 retained baseline statuses, add
54 mode IDs and retire the two `mkd.13` IDs, with no flips.
The commit label `93a608b3`
is the pre-commit HEAD under which the working-tree sweep ran.

<!-- BEGIN: mk-status (regenerated by contrib/status-update.sh) -->
## Current branch: slice-trial

## Spec status: 13403 passing / 0 failing of 13416 (spec.db @ 93a608b3)

_Regenerated 2026-11-26 by `mk status`. Edits inside this block will be
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
