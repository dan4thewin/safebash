title: DD#131 - Directory selectors and immutable scratch bindings
status: accepted

1. `glob ARRAY PATTERN...` and `rglob ARRAY DIRECTORY REGEX...` produce
   string arrays from ordinary operands; runtime regexes require DD132 proof.
   Pattern-list splats preserve element boundaries. An empty expanded list
   succeeds without enumeration; the target and directory are fixed operands.
2. Each glob pattern permits at most one star, in the final component, and
   literal directory prefixes. Glob needs a leading dot for dotfiles;
   both selectors exclude `.` and `..`. Regex selection tests basenames
   without an extra dotfile filter.
3. Concatenate bytewise-sorted per-pattern batches in source order, retaining
   duplicates. Results are clean, lexically normalized absolute paths.
4. Completed enumeration succeeds even with no matches; errors are not empty
   success. Preserve DD130 failure classes and handling.
5. Array fillers append complete batches to ordinary destinations.
   Leading-underscore introductions construct fresh immutable scratch;
   no extra `const` is required. Existing bindings cannot be reused or
   shadowed. Bare `_` remains discard.
6. Re-executing a construction site starts a new instance without old values,
   taint or proof. Skipping it changes nothing; block scope is unchanged.
   An initializer cannot read its own target, directly or through a helper.
7. Immutability survives aliases and collection slots. Validation may refine
   checked properties, not mutate the value.
8. Failed array fills and DD99 map conversions leave existing ordinary
   targets unchanged and fresh scratch empty; publish no partial collection.
   Preserve producer EOF/no-match results independently of destination contents.
9. Assignment replaces, append adds and iteration supplies items through
   checked fresh results; none gains a status or implicit argument globbing.
   Only bare literal list words activate glob sugar. Quoted/computed strings
   are data; mixed words containing an unquoted source star are errors.

---

DG accepted the complete implemented DD131/DD132 package and directed its
landing on main on 2026-11-18
([journal 661](../journal/661-accept-glob-scratch-regex.md)).
The separate-branch trial and evaluation are complete; the remaining
[converter migration](../tasks/convert-glob-scratch-regex-migration.md)
does not change these language contracts.

DG originally adopted this package provisionally on 2026-11-09 (journal 603):

> let's adopt glob, rglob, and scratch collections as a provisional
> decision; when implementing we'll use a branch and evaluate the utility
> and ramifications

That decision required implementation and evaluation on a separate branch,
followed by DG's review before landing on main. `q-glob-semantics` records
the completed integrated trial.

Later the same day DG provisionally adopted the aligned immutable-scratch,
`r?` and boolean-regex model (journal 604, DD132). That owner now covers
the combined experiment and retains the **DD revision inventory**, with
the pre-revision commit and the contracts to revisit if part of the trial
proves untenable.

The earlier text retaining a scratch batch after a failed repeat was the
AI's extrapolation, not a separately selected DG rule. It is corrected
below: unchanged-on-error applies to ordinary existing destinations;
scratch starts a new instance. Revising any part of the accepted model
must not silently reinstate that extrapolation.

## Directory selectors

The command forms are:

```sash
glob files '.*' '*'
rglob files /var/log '^app\.[0-9]+\.log$'
```

`glob ARRAY PATTERN...` and `rglob ARRAY DIRECTORY REGEX...` fill a
string array. Patterns are ordinary string operands, not literal-only
syntax. Validate known glob bounds statically and unknown ones at runtime;
computation does not evade the same limits. For `rglob`, DD132 instead
requires every runtime regex operand to be immutable and have a successful
`r?` proof available at its use. Literal regexes retain static validation.
The directory operand of `rglob` is separate from its regexes, so the
compiler need not infer a directory by interpreting regex syntax.

DG settled pattern-list expansion during implementation on 2026-11-17:

> Allow pattern-list splats, with empty expansion succeeding without enumeration.

Both selectors accept array splats in the variadic pattern tail, preserving
each element, its position and duplicates. Normal argument dispositions apply
there; the target remains a binding name and `rglob`'s directory remains one
non-elidable scalar string. Every expanded regex still needs DD132's immutable
value proof, including a default pattern supplied by a splat disposition.
An empty expanded list succeeds without opening, enumerating or normalizing
a directory, appends nothing to an ordinary destination and leaves a new
scratch instance empty. Operand evaluation still occurs normally. At least
one pattern operand must be written in the source.

### Matching and output

- **Glob is deliberately coarse.** At most one `*`, in the final filename
  component, with a non-wildcard directory prefix. `sub/*.sh`, `./*`,
  `../x/*` and `/etc/*` remain legal. Multiple stars and wildcard directory
  components are errors, not empty successes. Zero-star exact-name
  patterns are not excluded by the one-star limit. Only `*` is a wildcard;
  this adds no `?` or bracket-class operators.
- **One directory, not traversal.** Each glob pattern opens/enumerates
  one directory. Several patterns may name different directories.
  `rglob` enumerates its one specified directory for the regex patterns.
  Multiple directory reads are allowed; this is not a one-buffer limit.
  Use `/usr/bin/find` for traversal, not recursive or per-segment globbing.
- **Dots follow the pattern language.** A glob's filename component needs
  a literal leading `.` to select a leading-dot entry; wildcards never
  select `.` or `..`. Literal dot/dot-dot directory-prefix components
  remain legal. `rglob` applies ordinary DD63 regex matching to each
  **basename**, excluding the directory entries `.` and `..`; there is
  no additional hidden-file filter. Regex matching is unanchored unless
  the author supplies anchors.
- **Each pattern contributes a bytewise-sorted batch.** Concatenate
  batches in pattern order, retaining duplicates across overlapping
  patterns. Do not globally sort the combined destination or reorder
  entries already there.
- **Results are clean, normalized absolute paths.** Retain DD69/DD73:
  one element per name, no word splitting, lexical normalization rather
  than symlink canonicalization. Matching a basename and returning a path
  are explicit parts of the interface, not the hidden distinction DD108
  rejected.

This does not restore `arr=(GLOB =~ RE)`, a second pattern language inside
an array literal, or a regex filter over arbitrary arrays. DD108's loop
remains available for refinement of existing data.

### Result and failure

The commands have an operational status independent of their output
array. Successful enumeration, including **zero matches**, succeeds.
Ordinary implicit checking, otherwise-legal conditions and command-local
`-` apply under DD55/DD130. A failed open/read/required `getcwd` is not
an empty success; preserve the operation and cause. Missing directories
do not acquire an ENOENT-as-empty exception, and a read error is not EOF.
Fatal faults remain fatal.

DD132 supplies the regex prerequisite: invalid candidates
produce quiet false at `r?`, not a bad-pattern result from a later regex
consumer. `rglob` still has directory-operation failures; successful
enumeration with no matches is not false. Glob-bound errors, directory
errors and fatal faults retain their distinct handling. Neither selector
may publish partial output or conceal failure as successful emptiness.

## Fill destinations: accumulation or an immutable scratch result

One destination model applies to the array-filling forms of
`read`, `argv`, `keys`, `match`, `matchall`, `glob` and `rglob`:

| Target at the fill site | Rule |
|---|---|
| Ordinary name | Append the operation's complete batch; an undeclared target auto-declares empty first. |
| Leading-underscore name such as `_batch` | Introduce a fresh immutable scratch binding and populate it with the operation's batch. An already-bound target is an error. |
| Bare `_` | Still DD98's write-only discard binder, not a readable scratch collection. |

The scratch name retains its underscore. Freshness is a source binding
rule, not a test of whether an existing array happens to be empty.
An existing local, global or visible enclosing binding cannot be reused
or shadowed by a scratch fill. A second source binding site for the same
scratch name in the same scope is a redeclaration:

```sash
glob _batch '*.log'
glob _batch '*.txt'       # error: _batch is already bound
```

This is **fresh-result introduction versus accumulation**, not permission
to overwrite any existing underscore-prefixed array. A scratch instance
is immutable after construction: no assignment, append, cell write, unset
or mutation through an alias. Validation may change its checked properties,
not its value. Bare `_`'s discard/unused-variable exemptions do not apply.

The model is uniform across scalar and collection scratch results, not
an exception only for regex variables. DG selected the leading underscore
itself as the construction notation on 2026-11-17: `_re=...`,
`str _re=...`, readers and loop binders need no additional `const`.
`if _out=$(...)` remains an untyped capture-condition; typed declarations
still do not acquire a result under DD88. This does not add a `readonly` operation or
generally legalize `const array` / `const map` declarations.

### Scope, repeated execution and parameters

Keep DD86's positional visibility, no shadowing and one source declaration.
Re-executing the **same construction site** starts a genuinely new scratch
instance, rather than writing the preceding frozen instance. Its old value,
unresolved state and regex proof do not become the new instance's state.
Neither construction nor its failure path may recover the preceding batch.
This is a new lifetime rule, not permission for ordinary writes to clear
taint or for an arbitrary second site to reinitialize a constant.

An initializer may not read the scratch binding it is constructing, directly
or through a helper (DG, 2026-11-17). It gets neither the previous instance
nor the new instance's meaningful zero as an initializer input. Meaningful
zeros outside that construction remain as specified below.

Freshness is tied to execution of the construction site. It does not create
block scope, reset every scratch name at every loop edge, or destroy a value
merely because a loop exits. A skipped site does not reconstruct anything.
DD62's meaningful zeros remain; a zero-trip or failed construction does
not invent a successful regex-validation proof. Function/capture entries
retain their existing fresh-private-binding behavior.

DD100's A-E slots preserve the caller's actual collection identity **and
immutability**. A helper may read an immutable collection but cannot fill,
append, unset or mutate it, directly or through forwarding. Fills through
mutable slots still accumulate. There is no hidden append/replace mode
chosen by the caller's spelling, and a slot is not a fresh-binding request.
Well-typed whole-collection assignments through mutable slots replace
contents, not identity; they cannot restart an immutable actual's lifetime.
The existing non-aliasing and one-signature contracts remain.

Interpret the source binding, not a qualified internal name or compiler
temporary prefix. DD125 still permits source `__` names and requires
collision-free, properly typed synthetic identities. Function names do
not gain scratch semantics.

The named producers above yield string arrays. Existing array/map and
element-type checks remain: this does not make `read` a map filler or
invent map merge/collision semantics. `readline` remains a scalar binding,
with a fresh immutable result when its target is a scratch introduction.

### Construction cases to cover

| Construction | Scratch instance |
|---|---|
| Scalar initializer or capture | One immutable value; a repeated site constructs another instance. |
| `readline _line` | A fresh scalar on each execution, including a loop condition; ordinary scalar targets still replace, not append. |
| `for _item in ...` | A fresh immutable item per binding; no new block scope or implicit validation. |
| `argv`, array `read`, `keys`, `glob`, `rglob` | One immutable batch; input taint and producer-specific results remain. |
| `match` / `matchall` out-array | This operation's captures/extraction, not earlier iterations' groups; no match gives false and a fresh empty array. |
| Array/map initializer or conversion | An immutable collection, preserving existing type, construction and odd-pair rules. |
| `attest s (( _n = s ))` | A fresh conversion target; failure cannot reveal an older integer. The source's attest obligations are separate. |

Use ordinary mutable storage for incremental building, then construct an
immutable snapshot when wanted. A snapshot cannot be a frozen name for
storage that another live alias can still modify. This is a semantic
requirement, not a selected copy, allocation or ownership implementation.

## Publication and empty results

For array fills, an operation error leaves an **ordinary existing**
destination unchanged. A new ordinary target starts empty. A scratch
attempt instead starts fresh and remains empty on operation error; it
cannot expose an earlier instance or a partly constructed batch. This
applies across all patterns of one selector invocation. The failure
status remains a failure, not an invented successful empty result.

DD99 map conversion follows the same publication guarantee for declarations
and existing-map assignments, including collection-slot destinations.
Its successful result **replaces** the map rather than accumulating.
An empty source therefore clears an existing map; an odd-tail `:?` failure
preserves it. A new map or repeated scratch construction remains empty on
failure. The source array is not consumed or changed.

This needs an all-or-nothing **collection update**, not just removal of
the current clear-on-entry calls. Account for allocation while preparing
or publishing the result, including what ERR can observe after a fatal
fault. Do not weaken DD130 or treat heap failure as recoverable.
No ABI, buffer representation or publication algorithm is chosen here.

An empty result is not necessarily an operation error. Normal destinations
append zero elements; scratch destinations receive an empty batch after a
successful computation. `match`/`matchall` can validly report no match and
stdin `read` can reach EOF without an I/O error. Preserve each producer's
own result kind and EOF/no-match rules; never derive its verdict from
old entries already present in the destination.

The guarantee is not rollback of input consumption, argument effects,
earlier commands, file creation or an enclosing pipeline. It also does
not turn every scalar producer into an atomic array fill: DD88 still
permits this attempt's partial stdout on an external capture failure.
DD132's substitution no-match result is valid original-subject text,
not a failed construction. A failed attest leaves its candidate intact.

Acquisition, adjacency and ownership still apply. The selected exception
for a **caught attest on immutable scratch** is DD132/DD128: normal reads
are confined to the attest-success arm, with regex use additionally
requiring `r?`; elsewhere only explicit forced data reads are allowed,
never a forced regex-pattern read. The failed instance may remain
unresolved. It is not automatically abandoned, replaced or declassified.

## Assignment and iteration sugar

Retain the selected forms:

```sash
array f=(.* *)
array f=(.*); f+=(*)
for f in .* * { echo "$f" }
```

These are independent examples. `=` still replaces, `+=` appends, and a
`for` list supplies iteration data. Use the checked selector operation
with a fresh result where necessary; do not desugar replacement into
accumulation on the existing destination. Several expansions and other
legal elements retain source order, element boundaries and empty
contributions.

These mutable `f` examples do not permit `_f+=(...)` on an immutable
scratch result. A scratch initializer constructs once per execution of
its site; subsequent mutations remain errors.

Bindings and loops remain resultless. There is no `- array ...`,
assignment condition or new suppression reach into an operand/body.
Ordinary command arguments do not gain implicit glob expansion; the
explicit selectors interpret their pattern operands themselves.
Computed strings in ordinary array lists must not silently become
patterns just because their bytes contain `*`.

DG selected **bare literal words only** for sugar on 2026-11-17.
`sub/*.sb` expands; `'*'`, `"*"`, `$'*'` and `$pattern` are ordinary data.
A mixed word with an unquoted source star, such as `$dir/*.sb` or
`"sub/"*.sb`, is a compile-time error rather than a partial glob language.
Use explicit `glob files "$dir/*.sb"` for computed patterns. Explicit
selectors continue to interpret their ordinary string operands as patterns
regardless of quoting; list sugar does not.

## Evaluation and delivery

The branch evaluation assessed the package, not just two matchers:

- usefulness of accumulating results versus refreshing snapshots, fields
  and capture groups, including the loss of fixed latest-match indices
  when an ordinary destination already contains data;
- whether fresh immutable construction remains clear across scalar
  readers, for binders, conversions, collections and parameter aliases;
- failure atomicity, absence of stale previous-loop data, skipped sites,
  memory cost and DD132's branch-limited caught-attest permissions;
- regex proof propagation, boolean/text results, explicit discard and
  substitution sugar, without a taint-bypass escape for patterns;
- migration of existing fillers, underscore names and definitions named
  `glob`/`rglob` under DD125, plus sugar/quoting and converter impact;
- exact-name/empty-pattern edge cases and final arity details, without
  inventing a traversal or filesystem-snapshot promise;
- agreement of native/C behavior, real refusal/failure-path evidence,
  before/reversion controls and the normal predicted gate.

DD60/DD63/DD75/DD99 now state the accepted accumulation/scratch contract
in place of their former replacement rules. `q-glob-semantics` retains the
complete DD revision inventory, evidence and DG's acceptance dispositions.
The original recording made no compiler change. DG authorized implementation
on 2026-11-17; `glob-scratch-trial` started from `6c7b899f` and was evaluated
before his 2026-11-18 acceptance.

The selector implementation is complete (journals 647 and
653): explicit `glob`/`rglob`, ordered list sugar, pattern-list splats,
empty-list success, immutable regex proofs and atomic publication use shared
lowering and runtime traversal. Scratch and accumulating fillers are recorded
in journals 645-646; DD132's proof/read/result units are in journals 648-652.
Journal 654 records the integrated utility, migration and memory/time
evaluation. DG subsequently authorized the append optimization recorded in
[journal 655](../journal/655-append-reservation-and-index-cache.md):
maintained next-index metadata, complete reservation and allocation-free
owned-key transfer replace repeated scans and destination cloning. The
old-logical-contents guarantee on fatal failure and stable collection identity
remain intact. Sequential converted mapfile fills still need reconciliation;
hand adaptations are not a converter repair. That remaining work is tracked
separately in
[convert-glob-scratch-regex-migration](../tasks/convert-glob-scratch-regex-migration.md).
