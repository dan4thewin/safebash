title: Prototype selectors, immutable scratch and validated regexes on a separate branch
status: open
priority: P2
kind: work
depends: [bug-glob-directory-portion, bug-glob-multi-star, bug-regex-line-anchors, q-attest-retag-of-const]
gates:
  - "implementation and evaluation occur on a separate branch; DG reviews utility and ramifications before behavior lands on main"
  - "glob ARRAY PATTERN... accepts ordinary strings with consistent static/runtime bound checks; rglob ARRAY DIRECTORY REGEX... requires DD132 proof for runtime regex operands"
  - "DD39's at-most-one-star/final-component rule is enforced in explicit glob calls and every glob list position; literal prefixes and zero-star exact-name patterns remain legal"
  - "dotfiles require a literal leading dot in the filename pattern; wildcards never yield dot or dot-dot"
  - "rglob matches basenames with ordinary regex semantics, not glob limits or a hidden-file prefilter; dot/dot-dot are excluded and outputs are normalized absolute paths"
  - "each expansion is bytewise sorted, without locale dependence, a whole-list sort or cross-pattern deduplication"
  - "array f=(.* *), array f=(.*); f+=(*), and for f in .* * all expand per element in both backends"
  - "multiple globs and other legal list elements preserve source order, boundaries and empty contributions; append preserves prior elements"
  - "each glob pattern enumerates one directory and rglob enumerates its specified directory; repeated reads are allowed, traversal is not"
  - "selectors succeed on zero matches; normal DD130 handling distinguishes failed open/read/getcwd from no matches or invalid regex candidates, without empty success or new suppression reach"
  - "ordinary array-fill targets accumulate and remain unchanged on operation error; scratch construction starts fresh and leaves an empty batch on error, never a preceding or partial batch"
  - "scratch scalar/collection constructions, readline, for binders and conversion targets create fresh immutable instances; repeated execution is not reassignment or a second source declaration"
  - "leading underscore alone requests scratch construction, without extra const; initializers reject direct or helper-mediated reads of their own target; typed declarations remain excluded from capture-conditions"
  - "positional visibility, no shadowing, meaningful zeros and bare discard _ remain; skipped sites do not reset values and loop exit is not automatic destruction"
  - "scratch mutation rejects through assignments, append, cells, unset and transitive aliases; mutable A-E fills accumulate while immutable actuals remain read-only"
  - "ordinary scalar readline, type/kind checks, one inferred signature per function, strict non-aliasing, source ownership and ordinary const lifetime/write rules remain intact"
  - "a failed operation publishes no partial collection update, including later-pattern and allocation failures; prior input/argument effects are not falsely rolled back"
  - "external capture failures preserve this attempt's partial stdout under DD88, not an earlier scratch instance; array publication is not a general capture transaction"
  - "r? tests the full candidate with the actual accepted regex rules; invalid data is quiet false, literal regex use remains statically validated, and allocation faults stay fatal"
  - "every runtime regex consumer requires successful proof for its exact immutable pattern; cleanliness, constness, trusted copies and taint-disable switches cannot supply proof"
  - "final composition and transformed patterns need their own checks; quoted whole values and proved present collection elements retain identity, while missing-cell defaults cannot borrow proof"
  - "caught scratch attest permits normal reads only in its success arm; failure and post-construct data reads require explicit trust, including no-else fall-through"
  - "negated and logical caught forms retain the same success-region rule; a failed arm that exits/continues does not export permission; standalone attest still permits subsequent use"
  - "regex-pattern operands reject :! including combined dispositions, even inside success arms and with taint disabled; ordinary subject/replacement trust remains legal"
  - "calls, captures, globals, A-E forwarding and EXIT/ERR neither fabricate proof nor export caught-scratch permission; preserve adjacency, received-taint ownership and correct signal planning"
  - "sub/gsub produce text plus matched/no-match boolean; no-match text is the original subject and an identity replacement can still be true"
  - "existing regex value forms consume their boolean through capture-conditions or explicitly discard it for text-only use; no general assignment status or arbitrary intrinsic/function condition is introduced"
  - "substitution value sugar explicitly projects text per edit; mutation sugar remains resultless and rejects immutable targets; no chain-wide boolean is invented"
  - "other status-less captures still reject inert -; boolean consumption, pointless-work, no-reach and fatal-fault contracts remain"
  - "assignment remains replacement and resultless; only bare literal list words activate glob sugar; quoted/computed strings are data, and mixed words with an unquoted source star reject with explicit-glob guidance"
  - "migration impact on existing fills, underscore names, reserved command names, converter output and capture-group indexing is measured rather than assumed"
  - "focused real-CLI native/C specs have before/reversion controls; changed contracts use fresh ids and the predicted ordinary gate passes"
  - "the DD revision inventory exactly covers changed/new decision files, records per-contract trial evidence and DG dispositions, and reconciles every affected DD before landing or withdrawal"

## Provisional decision and branch boundary

DG adopted `glob`, `rglob` and scratch collections **provisionally** on
2026-11-09, explicitly requiring a branch for implementation and an
evaluation of utility and ramifications. Later that day he provisionally
adopted fresh immutable scratch, required `r?` validation, boolean regex
results and restricted caught-scratch attests (journal 604). DD131 and
DD132 are the combined working model; DD39/DD69/DD73 supply the matching,
composition and path contracts.
Retain this P2 owner and slug rather than create competing owners for
the same experiment. No implementation branch or compiler change was
made when recording either decision (journals 603-604).

DG authorized implementation on 2026-11-17. The trial branch is
`glob-scratch-trial`, based on `6c7b899ff9f8e1bfe18a5746efd23d8c549003ea`.
He accepted leading-underscore construction without extra `const`,
rejection of initializer self-reads (including through helpers), and
bare-literal-only glob sugar. These settle the previously deferred source
choices; they do not adopt the package on main.

The independently accepted const-validation prerequisite is implemented
and gated on this branch (journal 644). Source-identity immutable scratch,
fresh lifetime starts and private scratch collection publication are also
implemented and gated (journal 645). Ordinary accumulating fillers and their
atomic publication are implemented and gated too (journal 646). Explicit
coarse `glob` and source-provenance-aware list sugar are now implemented and
gated (journal 647), including the coupled private-batch allocation repair.
The quiet actual-engine `r?` predicate is implemented and gated
(journal 648), with trailing-escape and repetition-count overflow repairs.
The independent structural refusal of trusted pattern reads is implemented
and gated with SB5105 (journal 649), including with taint checking disabled.
Mandatory immutable-value proofs now cover every existing consumer with
SB5106 (journal 650), including collection presence, helper/process flow
and the lexical boundary of currently caught scratch regex uses.
Caught-scratch ordinary reads and failed-path relaxation are now enforced
with SB5107 (journal 651), including negated/logical forms and handler/signal
boundaries. Scalar regex text-plus-Boolean results and their projections are
implemented and gated (journal 652), together with guarded redirect-index
lifting and its ordered scopes. `rglob` remains implementation work;
this owner stays open through the integrated trial and DG's evaluation.

This is the **single integrated implementation/evaluation owner**, including
the regex work formerly posed by `q-dynamic-regcomp-levers`. That record
is closed as a provisionally answered question, not as an implementation
claim. Keep its historical evidence and this task's exact DD revision
inventory rather than maintaining competing trial inventories.

The branch is not permission to land the package automatically. Report
what worked, compatibility and memory costs, awkward cases and any
proposed changes to DG before adoption. Do not mark the independent
directory/regex bugs fixed merely because a prototype exists.

## Matching and composition

`glob ARRAY PATTERN...` accepts ordinary string patterns and selects
from **one directory per pattern**, with at most one `*` in the final
filename component. Non-wildcard prefixes such as `sub/`, `./`, `../x/`
and `/etc/` remain legal. Reject wildcard directory components and
multiple stars statically when known, dynamically otherwise. No rule
requires an explicit pattern to contain a star.

Leading-dot entries require a literal leading dot in the filename
pattern. `.*` never includes `.` or `..`. Sort each expansion bytewise.
Several globs may occur in one list: the directory limit is per glob,
not per statement, and it does not cap `getdents` reads.

`rglob ARRAY DIRECTORY REGEX...` names the directory separately.
Regexes match basenames using DD63 semantics, including leading-dot
names when they match, but never the `.`/`..` entries. Do not split a
directory out of regex syntax or impose the glob star limit. Its
directory enumeration serves the pattern batches, which concatenate
in pattern order without deduplication. The shared whole-subject anchor
repair (`bug-regex-line-anchors`, journal 642) is complete. Use that
DD39/DD63 contract, not a selector-specific interpretation of the old faults.

Every runtime regex operand needs DD132's successful validity proof for
its immutable final value. Literal use retains static validation.
`r?` returning false is not `rglob` returning a directory failure, and
neither is successful enumeration with zero matches.

DG explicitly included these independent sugar forms:

- `array f=(.* *)`
- `array f=(.*); f+=(*)`
- `for f in .* * { ... }`

Expand and concatenate in source order, each name as one element. Do not
sort the entire list, reorder existing append contents, deduplicate
overlapping patterns or word-split names. Successful zero-match globs
contribute nothing, not an empty-string placeholder. Ordinary type/scope
rules remain; the example declarations and loop are independent snippets.
The same glob rules apply to other legal initializer/append/list mixtures
and both brace and keyword loop bodies, not just these exact spellings.
This does not broaden named-splat/keys placement or ordinary command-
argument globs. Explicit selector operands are interpreted patterns,
not implicit argument expansion. Only bare literal words such as
`sub/*.sb` activate sugar. Quoted stars, including C-quoted ones, and
computed strings stay data. Mixed words with an unquoted source star
(`$dir/*.sb`, `"sub/"*.sb`) reject with guidance to use explicit `glob`.
Preserve source provenance instead of testing evaluated strings for `*`.

## Immutable scratch construction and existing fillers

The trial applies DD131's destination model to the array-filling forms
of `read`, `argv`, `keys`, `match`, `matchall` and the new selectors.
Normal destinations append, auto-declaring empty when needed.
`_batch`-style destinations request a fresh immutable source binding; reject
already-bound names even when empty. Bare `_` stays the DD98 discard
binder, without making readable scratch names taint/dead-variable exempt.

Preserve DD86 positional visibility and no shadowing: a second source
declaration is an error. Re-executing the **same construction site**
starts a new instance without the old value, unresolved state or proof.
It is not a write to the preceding constant. Failed scratch array
construction leaves the fresh empty result, not a preceding or partial
batch. The earlier retained-scratch interpretation was the AI's
extrapolation, corrected by DG, not a separately selected rule to preserve.
An initializer may not read its own target, directly or through a helper;
it cannot use either the old instance or the new meaningful zero.

The model also covers scalar initializers/captures, `readline _line`,
`for _item`, array/map construction and scratch arithmetic-conversion
targets. A failed conversion cannot expose the preceding integer; the
source's attest duties remain separate. Do not convert ordinary scalar
readline into append, revive general definite assignment, or permit
map `read`. Skipped sites do not reconstruct, and loop exit alone does
not destroy a binding. Meaningful zeros do not automatically carry proof.

Freeze contents as well as names: reject assignment, append, element
writes and unset, directly or through helper calls/forwarded A-E aliases.
Mutable slot fills accumulate; immutable actuals admit only read-only
uses. Retain identity, one inferred signature and strict non-aliasing,
including conditional/transitive accesses. Interpret source construction,
not internal qualified names or a compiler temporary prefix.

Ordinary const validation remains independently accepted and owned by
`q-attest-retag-of-const`; ordinary const loop/declaration restrictions
do not acquire scratch exceptions. There is no adopted public `readonly`
operation or general `const array` / `const map` syntax. DD135's implicit
readonly ERR imports are separately accepted and owned by
[err-readonly-inputs-and-nul-proofs](err-readonly-inputs-and-nul-proofs.md);
they neither adopt general freezing nor belong to this provisional trial.

Use ordinary mutable builders followed by independent immutable
snapshots where staged construction is needed. Measure copying/memory
cost and prove that later builder or alias mutation cannot change the
snapshot; do not select a storage architecture by documenting the name
as frozen.

Measure repeated `read`/`keys` snapshots and `match` capture indexing:
an ordinary accumulated array's index 1 need not be the latest group 1.
Also census existing underscore targets and user definitions named
`glob`/`rglob`; adding public builtins changes the reserved-name set.
Converter consequences and literal/quoting behavior are part of the
trial, not assumed compatibility.

## Regex proof, caught attests and results

Implement `r?` against the actual accepted regex rules, not a second
approximation. Test invalid literal candidates as data (quiet false)
separately from invalid literal **use** (static refusal). Exercise empty,
NUL, escape and dialect-boundary inputs without reopening the distinct
subject-interior-NUL decision. Fault controls must prove that resource
exhaustion remains fatal. A clean runtime candidate still needs its check.

Cover every consumer: match variants, substitutions, tests, dispatch,
every sugar edit and `rglob`. Validate final composition, not merely
pieces; whole-value quoting does not change bytes. Attest-all proves
present immutable elements, not missing-index defaults. Retain existing
attest eligibility, predicate restrictions and body ownership.

For **caught immutable-scratch** attests, cover successful and failed
arms, negation, logical forms, no-else fall-through and code after the
construct. Only the successful region permits normal reads and any
proved regex use. Elsewhere require explicit trusted **data** reads.
Even an exiting/continuing failed arm does not export permission;
standalone attest does permit subsequent successful uses. DD121 already
exempts non-fall-through paths for ordinary R2, so do not claim that
allowing `continue` is itself this experiment's new permission.

Reject trusted pattern reads, including combined modifiers and sugar,
independently of taint switches. Preserve trusted subjects/replacements.
A trusted copy requires its own successful check; calls and copies may
not launder a caught proof out of its region. Test globals, functions,
collection aliases, captures, children and independent EXIT/ERR roots.
Do not treat the restricted failed instance as destroyed or clean, and
do not defer signal dispatch for its entire lifetime.

Pin `sub`/`gsub`'s text plus boolean: matched, unmatched, empty result,
identity replacement and repeated construction. Capturing text must
consume the boolean through the selected capture-condition or discard
it explicitly; assignment remains resultless. Preserve `match` capture
shape and `matchall`'s mandatory array, and derive each verdict from
this invocation, not accumulated array length. Substitution sugar
explicitly projects each text result; mutation sugar cannot write
scratch and has no status or chain-wide boolean.

Coordinate the canonical result classifier with
`reject-statusless-intrinsic-suppression` and
`reject-discarded-status-consumers`; do not maintain a private name list
or let the new second result legalize inert prefixes elsewhere.
Preserve useful-statement checks, no-reach and existing capture/body
boundaries. Inventory condition, argument/interpolation, return/value
and sugar positions without silently widening their legal placement.

The scalar spelling and self-reference choices were settled on 2026-11-17:
leading underscore alone requests construction, and initializer self-reads
reject. `if _out=$(...)` remains untyped; DD88's declaration restriction
does not change. Raise any remaining consequential proof/publication
architecture choice only where it affects the contract. No choice may
resurrect the old instance, invent an unchecked regex route or weaken
immutability. Ordinary reversible internal choices follow the decision tree.

## Errors and shared ownership

The new commands own an operational status: successful enumeration,
including zero matches, succeeds. Apply DD130 to actual failures.
Directory open/read/getcwd failure retains its cause and is handled
according to recoverability. Do not adopt F13's old
ENOENT/ENOTDIR-as-empty recommendation or its blanket abort for every
other errno. A read error is not EOF or a successful partial result.

Publish each array operation without partly mutating an ordinary
destination or exposing a partial scratch batch. Ordinary existing
targets retain their value on error; scratch attempts remain fresh and
empty. Test failures after earlier patterns or records produced data,
plus allocation failures and the state visible to ERR. Merely removing
clear-on-entry is insufficient.
Do not claim rollback of consumed input, argument effects or an enclosing
pipeline. Preserve each existing filler's result kind and EOF/no-match
semantics; old array length cannot supply the latest operation's verdict.
External stdout captures retain this attempt's partial bytes under DD88;
the array guarantee does not make every scalar producer transactional.

`runtime-failure-classification` and `recoverable-file-failures` retain
their shared classification/interface ownership. Coordinate the
affected operations with them rather than duplicate runtime or backend
error policy; the trial need not wait for unrelated file-API work.
DD132 now supplies the provisional regex-consumer policy. This owner
implements it together with scratch, rather than letting an `rglob`
runtime change settle the old question accidentally. Coordinate with
`cross-compile-and-setjmp-on-mac`, which owns DD129's now-selected
regex-local jump support for x86_64 and AArch64 (journal 613), not an
ordinary-return rewrite. Quiet invalid-candidate falsehood still rules
out a blanket terminal `regerror9` path for validation; preserving the
jump does not supply that reporting change. Resource faults stay fatal.
This accepted native recovery choice is independent of the provisional
trial and is not a trial-only DD revision to withdraw.

Bindings and loops have no status. Do not invent `- array ...`/
`- f+=(...)`, extend suppression into a body/operand, change result kinds
beyond DD132's explicit second result, or call a recoverable error fatal
for lack of implemented handling. `=` needs a fresh result before
replacement; a direct accumulating call into its old destination is not
equivalent sugar.

`bug-glob-directory-portion` owns the literal-prefix repair.
`bug-glob-multi-star` owns the now-selected rejection rather than a
general matcher. The dependencies are correctness obligations, not a
required commit sequence. They may be implemented with the trial;
coordinate shared work and retain evidence for each owner.

## DD revision inventory

DG requested this inventory so that an untenable part of the trial can
be revised or withdrawn coherently. **Baseline for every modified file
below: `2a70e7ddf17d211c20b09ee961e1f3075072986c`**, immediately before
the combined scratch/regex revision. Each link names the exact tracked
file. That commit is provenance, not a claim that every earlier working
interpretation was independently ratified.

### Earlier selector recording carried into the trial

The original selector recording is commit
`2a70e7ddf17d211c20b09ee961e1f3075072986c`, with pre-change baseline
`bad3ff56f86a8f3e410f8c618d6601acfbdcb513`. It modified DD39, DD60,
DD63, DD68, DD69, DD73, DD75, DD86, DD98, DD99, DD100, DD108 and DD130,
and introduced DD131. All except DD60 receive additional revisions in
the tables below.

| Exact prior-only DD file | Carried-forward contract | Revisit if that part fails |
|---|---|---|
| [DD60-argv-trio.md](../decisions/DD60-argv-trio.md) | Provisional inclusion of argv's array fill in the accumulation/scratch trial; unchanged in this recording. | Reconcile its trial pointer and replacement/accumulation contract without undoing argv's independent typing, taint or bounds rules. |

Use **both** recordings at trial closeout. The tables below inventory this
combined-model revision exactly; the prior-only row prevents withdrawal
from overlooking an earlier companion edit.

### Existing DDs modified by the combined recording

| Exact DD file | Changed contract / section | Revisit if that part fails |
|---|---|---|
| [DD24-taint-quarantine.md](../decisions/DD24-taint-quarantine.md) | Acquisition/lifetime overview and restricted unresolved scratch reads. | Reconcile quarantine and actual remaining taint permissions. |
| [DD35-type-keywords.md](../decisions/DD35-type-keywords.md) | Immutable scratch on scalar/collection values, distinct from ordinary const. | Reconcile mutability axes without silently adding general const collections. |
| [DD39-glob-regex.md](../decisions/DD39-glob-regex.md) | Dynamic regex use, shipped abort history and F15 question disposition. | Restate the selected invalid-pattern interface; do not restore blanket abort by accident. |
| [DD55-error-model.md](../decisions/DD55-error-model.md) | Regcomp result ownership and substitution's real second result. | Reconcile checked failures, discard eligibility and captures. |
| [DD61-array-taint-bless-all.md](../decisions/DD61-array-taint-bless-all.md) | Immutable attest-all, present-element proof and caught scope. | Preserve quantification while revisiting proof/read permissions. |
| [DD63-regex-quoting-interpolation.md](../decisions/DD63-regex-quoting-interpolation.md) | Runtime proofs, text-plus-boolean results, sugar and out-array publication. | Reconcile the whole family, including transformed patterns and every sugar edit. |
| [DD65-var-modifier-prune.md](../decisions/DD65-var-modifier-prune.md) | Substitution sugar's explicit text projection. | Keep sugar and explicit-call result handling consistent. |
| [DD68-brace-control-flow.md](../decisions/DD68-brace-control-flow.md) | Fresh scratch loop/read binders without block scope. | Reconcile per-site lifetimes, skipped sites and ordinary binders. |
| [DD69-globs-as-arrays-splat-disposition.md](../decisions/DD69-globs-as-arrays-splat-disposition.md) | Immutable targets and ordinary/scratch failure publication. | Preserve assignment/append meaning and selected empty-result behavior. |
| [DD70-error-taxonomy-a-b.md](../decisions/DD70-error-taxonomy-a-b.md) | Invalid candidates as predicates; substitution boolean/discard and diagnostics. | Reconcile A/B membership, useful statements and sugar projections. |
| [DD71-operand-words-value-bindings.md](../decisions/DD71-operand-words-value-bindings.md) | Shipped dynamic-regcomp description versus required proof. | Keep ordinary operand parsing separate from pattern-use eligibility. |
| [DD73-glob-absolute-paths.md](../decisions/DD73-glob-absolute-paths.md) | Fresh-empty scratch errors and rglob's validated operands. | Reconcile publication without withdrawing independent absolute-path guarantees. |
| [DD75-canonical-keyword-forms-uniform-collections.md](../decisions/DD75-canonical-keyword-forms-uniform-collections.md) | Independent immutable keys snapshots. | Reconcile snapshot/alias behavior and normal accumulation. |
| [DD86-variable-scope.md](../decisions/DD86-variable-scope.md) | Per-construction immutable lifetimes and their caught-attest exception. | Reconcile scope/const restrictions without undoing independently accepted const validation. |
| [DD88-capture-condition.md](../decisions/DD88-capture-condition.md) | Regex value-plus-boolean conditions and fresh external capture instances. | Reconcile permitted producer shapes; preserve this attempt's partial stdout. |
| [DD91-taint-off-operator.md](../decisions/DD91-taint-off-operator.md) | Pattern-role trust prohibition and restricted scratch data reads. | Reconcile trust positions and prevent proof laundering. |
| [DD93-attest-ergonomics.md](../decisions/DD93-attest-ergonomics.md) | r?, immutable attest-all and scratch conversion targets. | Reconcile predicate/target semantics without broadening ordinary writes. |
| [DD98-discard-binder.md](../decisions/DD98-discard-binder.md) | Readable immutable scratch versus bare discard _. | Reconcile the prefix surface without losing discard's independent meaning. |
| [DD99-read-family.md](../decisions/DD99-read-family.md) | Fresh immutable scalar readline as well as array scratch. | Reconcile repeated/EOF attempts while keeping ordinary scalar replacement and NUL splitting. |
| [DD100-collection-parameters.md](../decisions/DD100-collection-parameters.md) | Immutable actuals through A-E and regex proof at received uses. | Reconcile alias capabilities without weakening identity, non-aliasing or the one-signature rule. |
| [DD108-retire-array-regex-filter.md](../decisions/DD108-retire-array-regex-filter.md) | Validated rglob patterns and operational, not no-match, status. | Keep the selector separate from the retired combined shorthand. |
| [DD111-value-intrinsic-sole-statement.md](../decisions/DD111-value-intrinsic-sole-statement.md) | Substitution's second result versus inert prefixes elsewhere. | Reconcile classification without widening sole-statement placement. |
| [DD121-attest-adjacency.md](../decisions/DD121-attest-adjacency.md) | Ordinary R2 versus caught immutable-scratch fall-through and proof scope. | Reconcile resolution/escape rules, preserving adjacency and non-fall-through distinctions. |
| [DD128-exit-err-adjacent-attest.md](../decisions/DD128-exit-err-adjacent-attest.md) | Fresh scratch lifetime, restricted unresolved state and handler/signal obligations. | Reconcile actual incoming states and proof regions, never a false clean merge. |
| [DD130-recoverable-and-fatal-failures.md](../decisions/DD130-recoverable-and-fatal-failures.md) | Array publication versus invalid-candidate falsehood and fatal faults. | Reconcile value/error state without changing the independent recoverability criterion. |
| [DD131-directory-selectors-scratch-collections.md](../decisions/DD131-directory-selectors-scratch-collections.md) | Fresh immutable scalar/collection model, ordinary-only retention and coordinated trial. | Reconcile all constructors/aliases; do not reinstate the AI's stale-scratch extrapolation automatically. |

### New DD added by the combined recording

| Exact DD file | Contract | Revisit if that part fails |
|---|---|---|
| [DD132-validated-regex-immutable-scratch.md](../decisions/DD132-validated-regex-immutable-scratch.md) | r?, mandatory immutable-value proofs, success-region permissions and regex text/boolean results. | Record which parts are retained, revised or withdrawn and update every dependent row above. |

### Using the inventory during evaluation

The implementation-start clarification has baseline
`6c7b899ff9f8e1bfe18a5746efd23d8c549003ea`:

| Exact DD file | Changed section / contract | Disposition |
|---|---|---|
| [DD131-directory-selectors-scratch-collections.md](../decisions/DD131-directory-selectors-scratch-collections.md) | Opening rules, construction, initializer self-reads and list sugar. | DG accepted these trial details on 2026-11-17; implementation/evaluation pending. |
| [DD86-variable-scope.md](../decisions/DD86-variable-scope.md) | Provisional scratch notation, initializer restriction and unchanged capture-condition grammar. | Same trial ruling; ordinary const validation remains independently accepted. |
| [DD35-type-keywords.md](../decisions/DD35-type-keywords.md) | Provisional scratch notation without extra const. | Same trial ruling. |
| [DD98-discard-binder.md](../decisions/DD98-discard-binder.md) | Provisional prefix spelling, preserving bare discard. | Same trial ruling. |

The same unit updates the ordinary const-validation delivery notices in
DD24, DD35, DD86 and DD93. Those implement the independently accepted
2026-11-09 rule, not a provisional contract to withdraw with this trial.
Keep that distinction when reconciling individual sections.

The predicate unit updates delivery notices, not policy, from baseline
`3ddaab5536b17f0091e790d1974844892ea653e8`: DD39's engine/vendor sections,
DD93's provisional `r?` section and DD132's implementation boundary.
No new DD or additional language contract is introduced.

The pattern-trust unit updates DD39's delivery/validator-name references and
DD132's implementation boundary from
`516944b539bcf9cf5a2d4c06a23fa43517f30679`. It enforces the existing
provisional trust prohibition, without selecting a proof representation.

The immutable-proof unit updates DD39's delivery paragraph and DD132's
implementation boundary from
`f5371e631421bb50d5d1dd9babe86dcde86c9f39`. It implements the existing
pattern contract with a shared SSA analysis; no language rule or runtime
representation is newly selected.

The caught-scratch unit updates DD132's implementation boundary from
`ebdf18bfac6f065f47b86b1f784f695918c0da0a`. It implements the recorded
read/failure/signal contract without a new language rule or runtime ABI.

The regex-result/redirect-index unit has baseline
`a4f29d6528c3ae05f29fdfea34ee11a791858bb4`:

| Exact DD file | Changed section / contract | Disposition |
|---|---|---|
| [DD119-redirect-target-provably-non-empty.md](../decisions/DD119-redirect-target-provably-non-empty.md) | Opening rule 3 and guarded-index clarification. | DG chose captures in guarded cell indices while retaining the direct-capture-target ban; the coupled repair follows existing DD86 ordering. |
| [DD132-validated-regex-immutable-scratch.md](../decisions/DD132-validated-regex-immutable-scratch.md) | Implementation boundary. | Scalar regex text/Boolean results and projections are delivered on the trial; the new scalar ABI is explicit, without a cache/handle. |
| [DD39-glob-regex.md](../decisions/DD39-glob-regex.md) | Engine delivery paragraph. | Reconcile caught-scratch and result delivery; `rglob` and integrated evaluation remain. |
| [DD65-var-modifier-prune.md](../decisions/DD65-var-modifier-prune.md) | Kept substitution-sugar entry. | Record the implemented per-edit text projection on the trial, not main adoption. |

Before the implementation branch changes more DDs, add each file and its
pre-change commit/section here. For each contract evaluated, record the
trial commit, concrete evidence/cost, DG's retained/revised/withdrawn
disposition and the reconciliation commit. DG's adoption dispositions
remain **pending**; the table records policy edits. Implementation evidence
is tracked separately below.

### Trial delivery evidence

The scratch unit follows `e053b852a96428dca076c30d9d036e3a91fb68e0`;
[journal 645](../journal/645-immutable-scratch-construction.md) records its
implementation, controls and publication boundary. Its 51 dual-mode cases
cover scalar/collection construction, helper/slot immutability,
initializer self-reads, repeated/skipped sites, reader and conversion
failure state, independent snapshots and private-body lifetimes.
The final gate has 11765 checks, no failures or status flips, and 102 added
mode IDs over the const baseline. Ten independent reversions pin the source
and SSA guards, shared read traversal, both backend resets and private fills.

The implementation reuses valid empty table headers and publishes completed
private tables without a runtime ABI change. It deliberately adds private
batch storage; its repeated-use memory cost remains to be measured with the
ordinary accumulating fillers and useful-program evaluation. These results
do not establish DD132's caught-read/proof rules or complete either selector.
No DD was revised by the scratch implementation unit, and no part of the
trial has been adopted on main.

The accumulation unit follows `67e99309674c73f687782f3dac252af3edf1e5dd`;
[journal 646](../journal/646-atomic-accumulating-fillers.md) records 31
dual-mode filler cases and 20 migrated contracts at fresh IDs. Ordinary
`argv`, array `read`, `keys` and regex out-arrays stage complete batches.
The shared runtime clones table metadata, appends privately and publishes
into the original header, retaining alias/live-iteration identity.
Late index overflow and late entry-growth OOM leave ERR the original
contents. The corrected OOM control proves failure after private insertions,
not merely during the initial clone.

Thirteen independent reversion campaigns pin publication, sparse indices,
both backends, source/SSA taint, inherited attest ownership, integer cells,
terminal facts and the separate ERR NUL prover. The final gate has 11827
checks, no failures or flips. Against the accepted scratch baseline,
11725 retained statuses are unchanged, with 102 added and 40 retired
mode IDs. No source/artifact mutation remains active.

Sparse append's next-index implementation uses one past the greatest live
nonnegative key, or zero. This is the AI's reversible trial choice to avoid
overwriting sparse entries, not a separate DG indexing ruling. Private
batches and cloned metadata still needed repeated-use memory/time evaluation;
that unit did not consume batches. No DD was revised by it.

The coarse-selector unit follows `3a15ddce1e9fc2ad775ea29b65ab453e73f8801f`;
[journal 647](../journal/647-coarse-directory-selectors.md) records the
shared bounded-pattern authority, explicit command, ordered mixed-list
sugar, quoting provenance, actual syscall failures and completed-batch
publication. The final glob file has 79 cases per mode. Eleven selector
reversions, three allocation reversions and a capture-bound reversion
reprise are restored.

The first full selector gate exposed three intentional contract migrations
and an introduced append-allocation regression. The repair removes redundant
literal initialization, consumes private-batch metadata without freeing
borrowed values, and avoids a full clone for a single allocation-before-
publication insertion. The existing late-OOM fixture is unchanged and again
reaches its intended later growth. Other snapshot/value retention remains
part of the integrated evaluation, not a solved general reclamation problem.

The repaired gate has 11953 checks, zero failures, ten first-ever mode IDs,
six retirements and two expected failure repairs. Against the accepted
accumulation baseline, all 11821 retained statuses are unchanged, with
132 added and six retired IDs. The independent directory-prefix and
multi-star owners record this verified trial implementation but remain
open pending adoption. No DD was revised by this unit.

The predicate unit follows `3ddaab5536b17f0091e790d1974844892ea653e8`;
[journal 648](../journal/648-quiet-regex-validity.md) records 23 new
dual-mode assertions, all red on the matched frozen compiler/runtime and
green after implementation. Nine independent reversions pin quiet syntax
failure, counted NUL and slice boundaries, both temporary allocations,
fatal engine OOM, trailing escapes and both repetition-count bounds.
Every reversion is restored. The ordinary gate has 11999 checks,
11986 passing and 13 existing skips, with 46 first-ever mode IDs and no
flips or retirements. All 11953 selector-baseline statuses are unchanged.

The predicate compiles and frees each candidate; it retains no handle or
cache. A short engine program still requests roughly 34 KB, while 500
valid/invalid/empty cycles fit a 64K heap with reclamation. These bounded
controls are not the integrated allocation/time evaluation. Consumer
proofs, caught permissions, result channels and `rglob` remain pending.
The independent pre-existing map-literal element-loss defect discovered
while correcting a fixture has its own backlog owner.

The pattern-trust unit follows `516944b539bcf9cf5a2d4c06a23fa43517f30679`;
[journal 649](../journal/649-regex-pattern-trust-guard.md) records SB5105
across every existing regex consumer and substitution spelling, including
nested capture/arithmetic/index reads. Subjects, replacements, literal
modifier-like text and `r?` candidates retain their existing behavior.
Eighteen regex refusals fail on the frozen compiler in each mode; a separate
ERR migration fails because it formerly received SB5101. A trusted ordinary
data copy retains coverage of that independent ERR dynamic-regex rule.

Two independent reversions pin the common guard and nested capture walk.
All mutations are restored, with source/compiler/runtime/lock identities
matching the positive snapshot. The gate has 12043 checks: 12030 passing
and 13 existing skips, 46 first-ever mode IDs, two expected retirements and
no flips. All 11997 retained predicate-baseline statuses are unchanged.
No runtime, ABI or retained allocation was added. Exact immutable-value
proofs were the next unit; that delivery follows below.

The immutable-proof unit follows `f5371e631421bb50d5d1dd9babe86dcde86c9f39`;
[journal 650](../journal/650-immutable-regex-proofs.md) records SB5106,
all-reaching-path and fresh-instance facts, scoped copies, helper returns,
collection presence and independent process roots. The original compiler
fails exactly 55 assertions in the final 262-case regex file per mode.
Twenty-two independent source reversions and two SSA API reversions are
restored. A separately preserved, mis-aimed status mutation stayed green;
the corrected unconditional-edge mutation independently pins helper returns.

The first full gate exposed five older unchecked-pattern fixtures and the
tour renderer's mutable fence pattern. These are migrated without weakening
the proof requirement; the six tour publication cases return in both modes,
and all 56 actual snippets render to the same published bytes. The accepted
gate has 12211 checks, 12198 passing and 13 existing skips. Relative to the
accepted trust-guard baseline, all 12003 retained statuses are unchanged,
with 208 added and 40 deliberately retired mode IDs.

The compiler/runtime/lock identities match the restored positive. This unit
adds no runtime allocation or ABI, but a mutable builder now needs a fresh
immutable candidate and validation before regex use, as the tour migration
demonstrates. The caught ordinary-read/failure and result-channel follow-ups are recorded
below; `rglob` and integrated migration/memory/time evaluation remain open.
No main adoption or fresh coverage run is included.

The caught-scratch unit follows `ebdf18bfac6f065f47b86b1f784f695918c0da0a`;
[journal 651](../journal/651-caught-scratch-permissions.md) records SB5107,
scratch-only unresolved fall-through, shared logical-attest lowering and
actual EXIT/ERR inputs. Trusted bare splices retain their trust metadata.
Fresh zero initialization no longer ends signal protection before acquisition;
the caught predicate releases it on either outcome without cleaning failure.

The finalized before compiler fails 48 new scratch assertions in native mode,
49 in C, and both new signal assertions in each. All original 51 scratch and
126 signal checks stay passing. Thirteen independent reversions are restored,
with all 26 mode runs accounted for; the trusted-index mutation also reaches
redirect targets through their normal scalar spills.

The gate adds exactly 120 mode IDs, with no flips or retirements:
12331 checks, 12318 passing and 13 existing skips, native 6277 and C 6054.
All 12211 accepted proof-baseline statuses are unchanged. The unit adds no
runtime allocation, retained state or dependency. The result-channel follow-up
is recorded below; `rglob` and the integrated
useful-program/helper/converter/memory evaluation remain, followed by DG's
adoption decision.

The regex-result unit follows `a4f29d6528c3ae05f29fdfea34ee11a791858bb4`;
[journal 652](../journal/652-regex-results-and-redirect-indices.md) records
scalar text-plus-Boolean results, explicit discard, text-only sugar and the
optional status-pointer ABI in both backends. DG's guarded-cell-index ruling
also closes the redirect ICE through shared ordered scopes. Argument retention,
stage-child ownership and whole-list FD-only `exec` completion are pinned;
the broader ordering and file-recovery owners remain open.

The regex file has 321 passing checks per backend. Original and intermediate
before controls and ten independent reversions pin the result, projection,
ordering and descriptor mechanisms. The first two full sweeps exposed omitted
tour migration and diagnostic regressions; neither missing checks nor
independent diagnostics were discarded to get a green gate. The final gate
at rows 7221215-7233663 has 12449 checks, 12436 passing and 13 existing skips,
with no final-retry additions, flips or retirements. Against the accepted
caught-scratch baseline, all 12255 retained statuses are unchanged:
194 added mode IDs and 76 intentional retirements.

Final source/compiler/runtime/lock identities match the separate post-refactor
snapshot. The ABI changes, but this unit adds no retained regex handle or cache;
existing per-use compilation and integrated memory/time costs still need
evaluation. All 56 tour snippets match their regenerated edition. `rglob`
and the integrated evaluation remain before DG's adoption decision.

Reconcile affected text **in place**. Do not blindly restore whole files
over independent later changes, or leave contradictory amendments.
Ordinary const validation, strict non-aliasing, resultless assignment,
DD130's failure criterion and the coarse glob choices have independent
authority. Withdrawal of an experiment is not permission to reverse them.
In particular, the pre-revision scratch-retention text was an AI
extrapolation; the old regex-consumer abort also did not implement
DD55/DD70's promised handling levers.

Check this inventory mechanically against actual changed/new
`notes/decisions/*.md` paths at recording and at trial closeout. Coordinate
GUIDE, status, this owner, companion tasks and journal 604 as well.
Do not mark this task done merely because the policy was recorded or a
prototype compiled: implementation evidence and DG's evaluation remain.

## Pre-trial mechanism (historical source inspection)

At `bad3ff56`, `try_array_single_glob` in
`sash/src/transform/expressions.rs:452-467` requires exactly one lowered
value; `try_dispatch_glob` in `transform/control_flow.rs:614-646` requires
exactly one source word. `handle_append_assign` in
`transform/stmt_handlers.rs:1434-1456` handles `IndexedArray` elements,
but sends other values to scalar append. The single-glob special value
therefore needs proper collection composition, not another scalar case.
The singleton lowering is not an authority for the sugar/quoting boundary
DG selected on 2026-11-17.

Both backends still pass `"."` as the directory (`value_handlers.rs:71-94`,
`emit_c.rs:3026-3037`). `sash_glob` (`sash-rt/src/lib.rs:3721-3842`)
returns an empty table on any failed open and breaks on both EOF and
negative `getdents64` results. It appends matches in enumeration order.
The shared runtime and its callers need coherent error/result handling.

Before the accumulation unit, fillers did not uniformly implement
unchanged-on-error: `sash_split_into_array` cleared at
`sash-rt/src/lib.rs:3896`; native/C bulk reads called `sash_read_all`
before that splitter, so a real read error aborted before clearing an
existing target. `sash_regex_match_into` and `sash_regex_matchall_into`
instead cleared at 2588/2660 before compilation and allocation.
`sash_argv_snapshot`/`sash_keys_snapshot` (1099-1145) returned fresh
replacement tables. These historical inspected paths were not a universal
failure guarantee. Journal 646 records the replacement of that model.

## Historical F13 evidence

These are the original observations, not rerun on 2026-11-09. In a
directory holding `.hid`, `a*c`, `ab.c`, `abc`, `axbxc`, `sub1/f1` and
`sub2/f2`, both backends:

- included `.hid` in `*`;
- returned `sub1 abc axbxc a*c sub2 .hid ab.c` in filesystem order;
- treated every directory-open error as no matches;
- accepted `*/f1` and `sub*/f*` but yielded nothing.

The original AI recommendation to support per-segment traversal is
superseded by DG's coarse-tool rule. Historical unsupported-pattern
acceptance is now a refusal target, not a matcher implementation target.
Measure current spec impact and database high-water marks before selecting
ids/counts; none were predicted during this policy-only recording.
