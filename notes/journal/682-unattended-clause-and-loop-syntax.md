title: Unattended backlog session - clauses and loop headers
date: 2026-11-28

# Unattended backlog session - clauses and loop headers

The second cohort implements DG's settled next-line-clause and word-list
header decisions. Seven of the twenty selected owners are now complete
on `unattended-backlog-20261128`; main remains at `4e5090c9`.
The AI implemented the existing contracts, without new syntax policy.

## Clause boundaries and loop sources

Brace-form `else`/`elif` attaches across newlines, blank lines and comments.
Lookahead consumes those newline tokens only when a clause follows.
Semicolons, intervening statements and enclosing closers remain barriers.
Completed `else` bodies cannot acquire another clause. SB4072's advice
now describes that rule rather than requiring the same line.

Word-list `for` requires `in` and at least one written source item.
Located SB4014 distinguishes an omitted `in`, a semicolon before it and
an explicitly empty list. Newlines before `in`, populated keyword/brace
forms and C-style loops retain their existing grammar. An empty string
still supplies one item; empty runtime arrays/argument tails remain legal.
No AST, lowering, backend or runtime change was needed.

The missing-body spec previously used `for x`, now correctly rejected at
the header. Fresh `pcold.3` uses `for x in a`, retaining the actual body
diagnostic instead of weakening its assertion. A malformed next-line
`elif {` retains the existing SB4027 at its brace, not an invented
empty-condition diagnostic at the clause keyword.

## Bash conversion

Omitted-list loops become `in @args` inside functions and `in $(argv 1)`
at top level. The latter reuses the existing argument-loop attest helper;
its guidance is placed before the loop, not in a one-line body where it
would comment out code. Explicit empty lists are retained, not converted
to arguments or erased.

Stock tree-sitter-bash fails to parse Bash's valid `for x do ...; done`
normally. The AI chose narrow CST-token normalization: an error-owned
`for`, binder and adjacent `do` identify the missing separator. Insert
the separator and reparse; no source-text regex or newlines are inserted.
Nested loops demonstrated that these tokens can cross CST node boundaries.
Collecting adjacent leaf tokens, while retaining the error-parent guard
on `for`, handles that recovery. An ERROR child containing explicit `in`
must not be mistaken for an omitted list.

Normal and `--annotate` converter output are covered. Historical references
to a separate canonical-output mode do not describe the current tool.
Generated programs run with multiple/zero arguments in both native and C;
function cases also preserve an empty argument and the nested Cartesian loop.

## Evidence

Fresh `clausenl.1`-`.7`, `forhead.1`-`.5` and `pcold.3` are dual-mode;
`convfor.1`-`.5` use the native-only converter harness. Both modes of
`skw.20` and `pcold.2` retire rather than reuse their changed contracts.

The frozen post-naming tools fail 23 final assertions: 19 new mode IDs
and four retained SB4072 wording assertions. Twelve new preservation
mode IDs pass before. Final focused files pass: control flow 145 and
errors 729 per mode, converter 235.

Six independent final-tree reversions match their predicted red sets:

| Reversion | Failures |
| --- | --- |
| Disable clause newline attachment | `clausenl.1`, `.2`, `.3`, `.6` per mode |
| Consume post-body newlines unconditionally | `clausenl.7` per mode |
| Restore optional `in` and empty lists | `forhead.1`-`.3` per mode |
| Disable implicit-list conversion | `convfor.1`, `.2`, `.5` |
| Remove explicit-empty ERROR guard | `convfor.3` |
| Require recovery tokens to share one CST parent | `convfor.5` |

All stubs are restored. Source, spec, compiler, converter, runtime and
lockfile hashes match the saved positive tree before and after the gate.
The private trace archive contains 29 exact run blocks, with all 14,176
ordered check/status records matched to their TSVs and archive bytes verified.

```sh
NEW=31 FLIPS=0 RETIRED=4 TOTAL=13494 timeout 900 mk -p 1 gate
```

The gate passes at rows 7624518-7638011: native 6,867 plus C 6,627;
13,481 passing and 13 existing skips. The sweep takes 337 seconds.
All 13,463 retained naming-baseline IDs keep their statuses; 31 are added
and four retire. Plans, liveness (246) and ordinary unit/static gates pass.
Evidence is retained under `files/batch20-20261128/`.

General parser-error recovery, binding-list boundaries, resultless
control flow and the broader pointless-work analysis keep their separate
owners. This cohort does not claim those repairs, fresh coverage or any
dependency change. The unattended converter/diagnostic/hook queue continues.
