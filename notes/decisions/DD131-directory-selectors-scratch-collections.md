title: DD#131 - Directory selectors and immutable scratch bindings
status: provisional

1. **Trial only:** evaluate the combined DD131/DD132 model on a separate
   branch; DG reviews evidence and revisions before adoption on main.
2. `glob ARRAY PATTERN...` and `rglob ARRAY DIRECTORY REGEX...` produce
   string arrays from ordinary operands; runtime regexes require DD132 proof.
3. Each glob pattern permits at most one star, in the final component, and
   literal directory prefixes. Glob needs a leading dot for dotfiles;
   both selectors exclude `.` and `..`. Regex selection tests basenames
   without an extra dotfile filter.
4. Concatenate bytewise-sorted per-pattern batches in source order, retaining
   duplicates. Results are clean, lexically normalized absolute paths.
5. Completed enumeration succeeds even with no matches; errors are not empty
   success. Preserve DD130 failure classes and handling.
6. Array fillers append complete batches to ordinary destinations.
   Leading-underscore introductions construct fresh immutable scratch;
   existing bindings cannot be reused or shadowed. Bare `_` remains discard.
7. Re-executing a construction site starts a new instance without old values,
   taint or proof. Skipping it changes nothing; block scope is unchanged.
8. Immutability survives aliases and collection slots. Validation may refine
   checked properties, not mutate the value.
9. Failed array fills leave existing ordinary targets unchanged and fresh
   scratch empty; publish no partial batch. Preserve each producer's own
   EOF/no-match result, independently of destination contents.
10. Assignment replaces, append adds and iteration supplies items through
    checked fresh results; none gains a status or implicit argument globbing.

---

DG adopted this package provisionally on 2026-11-09 (journal 603):

> let's adopt glob, rglob, and scratch collections as a provisional
> decision; when implementing we'll use a branch and evaluate the utility
> and ramifications

This is a decision to try the model, not a claim that it ships or a final
language commitment. Implement and evaluate it on a separate branch.
Bring the evidence and any proposed revisions back to DG before landing
the behavior on main. `q-glob-semantics` owns that integrated trial.

Later the same day DG provisionally adopted the aligned immutable-scratch,
`r?` and boolean-regex model (journal 604, DD132). That owner now covers
the combined experiment and contains the **DD revision inventory**, with
the pre-revision commit and the contracts to revisit if part of the trial
proves untenable.

The earlier text retaining a scratch batch after a failed repeat was the
AI's extrapolation, not a separately selected DG rule. It is corrected
below: unchanged-on-error applies to ordinary existing destinations;
scratch starts a new instance. Rejecting some future part of the experiment
must not silently reinstate that extrapolation.

## Directory selectors

The working command forms are:

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

DD132 supplies the provisional regex prerequisite: invalid candidates
produce quiet false at `r?`, not a bad-pattern result from a later regex
consumer. `rglob` still has directory-operation failures; successful
enumeration with no matches is not false. Glob-bound errors, directory
errors and fatal faults retain their distinct handling. Neither selector
may publish partial output or conceal failure as successful emptiness.

## Fill destinations: accumulation or an immutable scratch result

The trial applies one destination model to the array-filling forms of
`read`, `argv`, `keys`, `match`, `matchall`, `glob` and `rglob`:

| Target at the fill site | Working rule |
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
an exception only for regex variables. Leading-underscore binding
introductions are the working notation. The exact scalar declaration
spelling (`_re=...` or an explicit `const _re=...`) remains a surface detail
to settle before implementation; examples here and in DD132 illustrate
semantics, not a shipped grammar. This does not add a `readonly` operation
or generally legalize `const array` / `const map` declarations.

### Scope, repeated execution and parameters

Keep DD86's positional visibility, no shadowing and one source declaration.
Re-executing the **same construction site** starts a genuinely new scratch
instance, rather than writing the preceding frozen instance. Its old value,
unresolved state and regex proof do not become the new instance's state.
Neither construction nor its failure path may recover the preceding batch.
This is a new lifetime rule, not permission for ordinary writes to clear
taint or for an arbitrary second site to reinitialize a constant.

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
requiring `r?`; elsewhere only explicit trusted data reads are allowed,
never a trusted regex-pattern operand. The failed instance may remain
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
patterns just because their bytes contain `*`. The exact sugar/quoting
boundary must be made explicit in the trial; no new quote-dependent
syntax rule or internal representation was selected in this discussion.

## Evaluate before adoption

The branch must assess the package, not just demonstrate two matchers:

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

Current DD60/DD63/DD75/DD99 replacement contracts describe the shipped
baseline. They are not silently rewritten as if this experiment already
passed. Maintain `q-glob-semantics`' DD revision inventory through the
trial. Record which parts DG adopts, revises or drops, and reconcile
every affected contract in place rather than leaving contradictory
addenda. No implementation branch, compiler change or new behavior
measurement accompanies this provisional recording.
