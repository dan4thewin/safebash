title: Prototype glob, rglob and scratch collections on a separate branch
status: open
priority: P2
kind: work
depends: [bug-glob-directory-portion, bug-glob-multi-star, bug-regex-line-anchors]
gates:
  - "implementation and evaluation occur on a separate branch; DG reviews utility and ramifications before behavior lands on main"
  - "glob ARRAY PATTERN... and rglob ARRAY DIRECTORY REGEX... accept ordinary strings with consistent static/runtime validation"
  - "DD39's at-most-one-star/final-component rule is enforced in explicit glob calls and every glob list position; literal prefixes and zero-star exact-name patterns remain legal"
  - "dotfiles require a literal leading dot in the filename pattern; wildcards never yield dot or dot-dot"
  - "rglob matches basenames with ordinary regex semantics, not glob limits or a hidden-file prefilter; dot/dot-dot are excluded and outputs are normalized absolute paths"
  - "each expansion is bytewise sorted, without locale dependence, a whole-list sort or cross-pattern deduplication"
  - "array f=(.* *), array f=(.*); f+=(*), and for f in .* * all expand per element in both backends"
  - "multiple globs and other legal list elements preserve source order, boundaries and empty contributions; append preserves prior elements"
  - "each glob pattern enumerates one directory and rglob enumerates its specified directory; repeated reads are allowed, traversal is not"
  - "selectors succeed on zero matches; normal DD130 handling distinguishes failed open/read/getcwd and pattern validation without empty or partial success or new suppression reach"
  - "ordinary array-fill targets accumulate; leading-underscore scratch targets require fresh source bindings; bare discard _ remains distinct"
  - "one scratch site can re-execute, replacing its batch on successful computation and retaining it on operation error; there is no new loop scope or taint lifetime"
  - "array fills through A-E preserve identity and accumulate independently of caller spelling; scalar readline, type/kind checks, non-aliasing and taint ownership remain intact"
  - "a failed operation publishes no partial collection update, including later-pattern and allocation failures; prior input/argument effects are not falsely rolled back"
  - "assignment remains replacement and resultless; ordinary strings do not silently become glob syntax, and the sugar/quoting boundary is explicitly evaluated"
  - "migration impact on existing fills, underscore names, reserved command names, converter output and capture-group indexing is measured rather than assumed"
  - "focused real-CLI native/C specs have before/reversion controls; changed contracts use fresh ids and the predicted ordinary gate passes"
  - "DG's post-trial adoption, revision or rejection is recorded and affected DDs are reconciled; a prototype is not automatically a shipped contract"

## Provisional decision and branch boundary

DG adopted `glob`, `rglob` and scratch collections **provisionally** on
2026-11-09, explicitly requiring a branch for implementation and an
evaluation of utility and ramifications. DD131 is the working model;
DD39/DD69/DD73 supply the matching, composition and path contracts.
Retain this P2 owner and slug rather than create competing owners for
the same experiment. No implementation branch or compiler change was
made when recording the decision (journal 603).

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

## Scratch collections and existing fillers

The trial applies DD131's destination model to the array-filling forms
of `read`, `argv`, `keys`, `match`, `matchall` and the new selectors.
Normal destinations append, auto-declaring empty when needed.
`_batch`-style destinations request a fresh source binding; reject
already-bound names even when empty. Bare `_` stays the DD98 discard
binder, without making readable scratch names taint/dead-variable exempt.

Preserve DD86 scope: a second source declaration is an error; a loop
can re-execute the same site, replacing its batch on successful
computation, including an empty batch. Actual operation failure retains
the prior value, so a failed later iteration can expose the previous
batch. Measure that usability cost rather than silently introducing
per-iteration lifetimes or allowing ordinary writes to resolve taint.

The marker belongs to source introduction, not internal names or a
permanent array type. Existing A-E slots accumulate regardless of the
caller's original spelling. Preserve identity through forwarding,
non-aliasing, signatures, scalar `readline` and array/map/element kinds.
The current producers yield string arrays; no map merge policy is
invented by calling the feature scratch collections.

Measure repeated `read`/`keys` snapshots and `match` capture indexing:
an ordinary accumulated array's index 1 need not be the latest group 1.
Also census existing underscore targets and user definitions named
`glob`/`rglob`; adding public builtins changes the reserved-name set.
Converter consequences and literal/quoting behavior are part of the
trial, not assumed compatibility.

## Errors and shared ownership

The new commands own an operational status: successful enumeration,
including zero matches, succeeds. Apply DD130 to actual failures.
Directory open/read/getcwd failure retains its cause and is handled
according to recoverability. Do not adopt F13's old
ENOENT/ENOTDIR-as-empty recommendation or its blanket abort for every
other errno. A read error is not EOF or a successful partial result.

Stage and publish each operation's result without clearing or partly
mutating its existing destination on error. Test failures after earlier
patterns or records produced data, plus allocation failures and the
state visible to ERR. Merely removing clear-on-entry is insufficient.
Do not claim rollback of consumed input, argument effects or an enclosing
pipeline. Preserve each existing filler's result kind and EOF/no-match
semantics; old array length cannot supply the latest operation's verdict.

`runtime-failure-classification` and `recoverable-file-failures` retain
their shared classification/interface ownership. Coordinate the
affected operations with them rather than duplicate runtime or backend
error policy; the trial need not wait for unrelated file-API work.
`q-dynamic-regcomp-levers` retains the broader regex-consumer question.
The new `rglob` must not settle that question by an accidental shared
runtime change or pretend an invalid dynamic pattern succeeded.

Bindings and loops have no status. Do not invent `- array ...`/
`- f+=(...)`, extend suppression into a body/operand, change existing
result kinds or call a recoverable error fatal for lack of implemented
handling. `=` needs a fresh result before replacement; a direct
accumulating call into its old destination is not equivalent sugar.

`bug-glob-directory-portion` owns the literal-prefix repair.
`bug-glob-multi-star` owns the now-selected rejection rather than a
general matcher. The dependencies are correctness obligations, not a
required commit sequence. They may be implemented with the trial;
coordinate shared work and retain evidence for each owner.

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
