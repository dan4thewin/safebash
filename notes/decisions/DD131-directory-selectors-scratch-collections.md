title: DD#131 - Directory selectors and scratch collections
status: provisional

DG adopted this package provisionally on 2026-11-09 (journal 603):

> let's adopt glob, rglob, and scratch collections as a provisional
> decision; when implementing we'll use a branch and evaluate the utility
> and ramifications

This is a decision to try the model, not a claim that it ships or a final
language commitment. Implement and evaluate it on a separate branch.
Bring the evidence and any proposed revisions back to DG before landing
the behavior on main. `q-glob-semantics` owns that integrated trial.

## Directory selectors

The working command forms are:

```sash
glob files '.*' '*'
rglob files /var/log '^app\.[0-9]+\.log$'
```

`glob ARRAY PATTERN...` and `rglob ARRAY DIRECTORY REGEX...` fill a
string array. Patterns are ordinary string operands, not literal-only
syntax. Validate known patterns statically and
unknown patterns at runtime; computation does not evade the same limits.
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

Dynamic pattern validation and regex compilation must have explicit
failure behavior, not partial output or a success-shaped fallback.
Coordinate the shared regex compiler with `q-dynamic-regcomp-levers`;
this provisional addition is not a ruling on every existing `match`,
`sub`, test or capture error interface. Any remaining consequential
classification/interface choice must be raised before implementing it.

## Fill destinations: accumulation or a fresh scratch binding

The trial applies one destination model to the array-filling forms of
`read`, `argv`, `keys`, `match`, `matchall`, `glob` and `rglob`:

| Target at the fill site | Working rule |
|---|---|
| Ordinary name | Append the operation's complete batch; an undeclared target auto-declares empty first. |
| Leading-underscore name such as `_batch` | Introduce a fresh scratch collection binding and populate it with the operation's batch. An already-bound target is an error. |
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
to overwrite any existing underscore-prefixed array. Reading the result
uses the ordinary name. Scratch does not imply const, discard, an unused-
variable exemption or a taint exemption.

### Scope, repeated execution and parameters

Keep DD86's positional scope and ordinary mutation rules, including
immutable-target restrictions. An underscore does not create a new block
scope or private lifetime. Re-executing the
**same** scratch binding site in a loop is distinct from a second source
declaration: each successfully computed batch replaces that site's prior
value, including an empty batch. On an operation error, the previous
value remains. A function/capture's genuinely fresh body entry still
starts its private bindings empty as DD86 already requires.

Thus a failed fill on a later loop iteration can leave the preceding
iteration's batch visible. That is a deliberate consequence of the
working scope and unchanged-on-error rules, and a central evaluation
case, not an accidental claim of per-iteration lifetime.

Scratch is an introduction request at the fill site, **not a permanent
refill-mode property of the collection**. DD100's A-E slots already name
existing caller collections; fills through them accumulate, regardless
of whether the caller originally named the collection `_batch`.
Forwarding must preserve identity, not make a callee's mode depend on a
hidden caller spelling. A fresh-binding operation cannot target an
already-bound slot. Normal non-aliasing and the one-signature rule stay.

Interpret the source binding, not a qualified internal name or compiler
temporary prefix. DD125 still permits source `__` names and requires
collision-free, properly typed synthetic identities. Scalar variables
and function names do not gain scratch semantics.

The named producers above yield string arrays. Existing array/map and
element-type checks remain: this does not make `read` a map filler or
invent map merge/collision semantics. `readline` remains a scalar binding.

## Publication and empty results

An operation error leaves an existing destination unchanged: no cleared
target, partly appended batch or partly replaced scratch result. This
applies across all patterns of one selector invocation. A newly
introduced target starts empty; failure supplies no invented successful
value.

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
earlier commands, file creation or an enclosing pipeline. All existing
taint acquisition, adjacency and ownership rules remain. Replacement in
one live binding is not a fresh private lifetime or declassification.

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
- whether the scratch naming/declaration rule remains clear in loops,
  conditions, functions and forwarded collection parameters;
- failure atomicity, retained previous-loop data, memory cost and
  interaction with DD128's strict taint/lifetime rules;
- migration of existing fillers, underscore names and definitions named
  `glob`/`rglob` under DD125, plus sugar/quoting and converter impact;
- exact-name/empty-pattern edge cases and final arity details, without
  inventing a traversal or filesystem-snapshot promise;
- agreement of native/C behavior, real refusal/failure-path evidence,
  before/reversion controls and the normal predicted gate.

Current DD60/DD63/DD75/DD99 replacement contracts describe the shipped
baseline. They are not silently rewritten as if this experiment already
passed. After evaluation, record which parts DG adopts, revises or drops,
and reconcile their authoritative contracts in place. No implementation
branch, compiler change or new behavior measurement accompanies this
provisional recording.
