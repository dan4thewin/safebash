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
  - "assignment remains replacement and resultless; ordinary strings do not silently become glob syntax, and the sugar/quoting boundary is explicitly evaluated"
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
in pattern order without deduplication. Use the existing regex
correctness owner (`bug-regex-line-anchors`), not a new selector-specific
interpretation of its known faults.

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
not implicit argument expansion. Inventory raw, quoted and computed
strings in list sugar; raise any genuine syntax fork rather than
silently treating every evaluated string containing `*` as a glob.

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
do not acquire scratch exceptions. There is no adopted `readonly`
operation or general `const array` / `const map` syntax.

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

Before coding, settle the still-open consequential details with DG:
final scalar scratch declaration spelling and its fit with DD88's
declaration restriction; any genuinely underdetermined initialization/
self-reference rule; and the proof/publication architecture where it
affects the contract. No choice may resurrect the old instance, invent
an unchecked regex route or weaken the selected immutability. Ordinary
reversible staging/internal choices follow the normal decision tree.

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
| [DD70-error-taxonomy-A-B.md](../decisions/DD70-error-taxonomy-A-B.md) | Invalid candidates as predicates; substitution boolean/discard and diagnostics. | Reconcile A/B membership, useful statements and sugar projections. |
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
| [111-value-intrinsic-sole-statement.md](../decisions/111-value-intrinsic-sole-statement.md) | Substitution's second result versus inert prefixes elsewhere. | Reconcile classification without widening sole-statement placement. |
| [DD121-attest-adjacency.md](../decisions/DD121-attest-adjacency.md) | Ordinary R2 versus caught immutable-scratch fall-through and proof scope. | Reconcile resolution/escape rules, preserving adjacency and non-fall-through distinctions. |
| [DD128-exit-err-adjacent-attest.md](../decisions/DD128-exit-err-adjacent-attest.md) | Fresh scratch lifetime, restricted unresolved state and handler/signal obligations. | Reconcile actual incoming states and proof regions, never a false clean merge. |
| [DD130-recoverable-and-fatal-failures.md](../decisions/DD130-recoverable-and-fatal-failures.md) | Array publication versus invalid-candidate falsehood and fatal faults. | Reconcile value/error state without changing the independent recoverability criterion. |
| [DD131-directory-selectors-scratch-collections.md](../decisions/DD131-directory-selectors-scratch-collections.md) | Fresh immutable scalar/collection model, ordinary-only retention and coordinated trial. | Reconcile all constructors/aliases; do not reinstate the AI's stale-scratch extrapolation automatically. |

### New DD added by the combined recording

| Exact DD file | Contract | Revisit if that part fails |
|---|---|---|
| [DD132-validated-regex-immutable-scratch.md](../decisions/DD132-validated-regex-immutable-scratch.md) | r?, mandatory immutable-value proofs, success-region permissions and regex text/boolean results. | Record which parts are retained, revised or withdrawn and update every dependent row above. |

### Using the inventory during evaluation

Before the implementation branch changes more DDs, add each file and its
pre-change commit/section here. For each contract evaluated, record the
trial commit, concrete evidence/cost, DG's retained/revised/withdrawn
disposition and the reconciliation commit. Currently every trial outcome
is **pending**; the table records policy edits, not implementation results.

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

## Current mechanism (source inspection, not new runtime evidence)

At `bad3ff56`, `try_array_single_glob` in
`sash/src/transform/expressions.rs:452-467` requires exactly one lowered
value; `try_dispatch_glob` in `transform/control_flow.rs:614-646` requires
exactly one source word. `handle_append_assign` in
`transform/stmt_handlers.rs:1434-1456` handles `IndexedArray` elements,
but sends other values to scalar append. The single-glob special value
therefore needs proper collection composition, not another scalar case.
The singleton lowering is not an authority for the still-to-be-evaluated
sugar/quoting boundary.

Both backends still pass `"."` as the directory (`value_handlers.rs:71-94`,
`emit_c.rs:3026-3037`). `sash_glob` (`sash-rt/src/lib.rs:3721-3842`)
returns an empty table on any failed open and breaks on both EOF and
negative `getdents64` results. It appends matches in enumeration order.
The shared runtime and its callers need coherent error/result handling.

Existing fillers do not already implement unchanged-on-error:
`sash_split_into_array` clears at `sash-rt/src/lib.rs:3896`; native/C
bulk reads call `sash_read_all` before that splitter, so a real read
error aborts before clearing an existing target. `sash_regex_match_into`
and `sash_regex_matchall_into` instead clear at 2588/2660 before
compilation and allocation. `sash_argv_snapshot`/`sash_keys_snapshot`
(1099-1145) return fresh replacement tables. These inspected paths are
not a universal guarantee about every failure and were not re-probed.

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
