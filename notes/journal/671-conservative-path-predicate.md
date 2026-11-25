title: Conservative path predicate
date: 2026-11-25

# Conservative path predicate

DG requested a conservative absolute-path predicate for ordinary tests and
attests, then chose the name: "`cp?` says copy to me... use `cpath?`
implement this". The AI implemented it on `err-attempt-trial`. This does
not adopt DD136 or move main.

## Spelling, not filesystem safety

`cpath?` accepts exactly one leading slash and nonempty components made
of ASCII letters, digits, dot, underscore and hyphen. Complete `.` and
`..` components, component-leading hyphens and repeated separators reject.
Root `/` and one trailing slash are valid. NUL, controls, non-ASCII bytes,
whitespace and every other punctuation byte reject.

This is a counted-byte predicate: it neither truncates at NUL nor repairs
invalid input. There is no filesystem lookup, cwd query, normalization or
path-length policy. `/dev/null/child` passes despite being impossible to
resolve; `/etc/passwd` passes without becoming an authorized operand.
Existence, file type, symlink containment and races remain separate.
Arbitrary downstream interpretation is not made safe, and downstream
scripts still need correct quoting.

```sash
path=$(- arg 1)
attest path cpath?
echo $path
```

Full `[[ cpath? $path ]]` predicates and existing array/map shorthand
attests use the same operator. Map shorthand tests both keys and values.
An ordinary condition does not attest its input. Existing string typing,
predicate purity, taint, ownership, adjacency and caught-failure rules stay.
No path type, automatic glob filtering or external-argument mandate was
introduced. DD93 records the contract; DD135's NUL-property lists and the
specification outline agree.

## Shared implementation

The parser already recognizes multi-letter question-mark operators.
Registering `cpath?` in its shared unary list supplies both ordinary tests
and attest shorthand without another lexer token, AST/SSA variant or
attest lowering path.

Both native and C emission call `sash_is_cpath(ptr, len)`. Its allocation-free
runtime helper works on the complete slice and reuses the existing nonempty
byte-class helper for components. Root is recognized before stripping the
optional trailing slash, so `//` cannot become an accepted empty root.
Operand evaluation and lazy test composition remain the existing shared
behavior. No taint exception or static ERR proof was added.

Seven Rust files change: the runtime helper/export, parser registration,
native extern declaration/field/test dispatch, and C declaration/dispatch.
No architecture change or GUIDE expansion was needed.

## Focused evidence

`spec/filetest.sh` grows from 57 to 74 checks per mode, adding
`cpath.1` through `.17`. The final file passes all 74 in native and C.
The cases cover valid and invalid components, every possible byte,
quiet false, negation/laziness, once-only capture evaluation, acquired
scalar/array attests, both map binders, caught failure replacement, and
preserved SB5020/SB4030/SB5098 boundaries. A syscall trace also verifies
that the valid-spelling program makes no filesystem or cwd query.

The exhaustive byte fixture reads a file containing bytes 0 through 255
and peels one counted byte at a time. Initial fixtures used unsupported
high-byte source escapes and computed substring offsets; actual xtraces
identified SB4063 and SB4047. Other draft fixtures used an invalid array
capture initializer, an echo splat, and tainted map insertion. Those were
corrected to existing argv out-parameters, explicit joins and clean map
values. These were fixture errors, not compiler repairs or new allowances.

The frozen original compiler and runtime fail exactly the seventeen
final new checks in each backend; all 57 retained checks still pass.
A session-only linker shim selects the frozen archive without replacing
live build artifacts. Actual archive-open traces verify that both modes
really link the original runtime. Standalone evidence uses `SPEC_TSV`
and does not import results into `spec.db`.

Four independent reversions ran the complete 74-check file in both modes.
The numbers below are suffixes of `cpath.`; every unlisted check passed.

| Reversion | Native failures | C failures |
|---|---|---|
| Permit empty, dot/dot-dot and dash-leading components | 3, 4, 10, 12 | 3, 4, 10, 12 |
| Truncate counted input at its first NUL | 2, 5 | 2, 5 |
| Route native dispatch to the graphical-byte predicate | 2, 3, 4, 5, 9, 10, 12, 14 | none |
| Route C dispatch to the graphical-byte predicate | none | 2, 3, 4, 5, 9, 10, 12, 14 |

All predictions matched. Every mutation is restored, and the final
source, spec, compiler, runtime archive, local lockfiles and protected
`notes/dat` match the saved positive identities.

## Complete gate

```sh
NEW=34 RETIRED=0 FLIPS=0 TOTAL=13120 timeout 900 mk -p 1 gate
```

The first full gate passes at rows 7438379-7451498: 6676 native and
6444 C checks, 13107 passing and 13 existing skips. The sweep took 335
seconds. There are no failures, flips, retirements, ghosts or plan
mismatches; liveness is 246. The gate includes the ordinary build/unit
checks and both source-ID and diagnostic censuses.

An independent comparison against the preceding accepted block,
7425293-7438378, preserves all 13086 statuses and adds exactly the
34 expected `native:cpath.*` / `c:cpath.*` IDs, with no missing or
unexpected additions. Source/compiler/runtime identities still match
after the gate. Root and separate-runtime formatting checks pass.

Evidence is retained under session `files/cpath-20261125/`: frozen before
and positive artifacts, authoritative `final-before-*` and
`final-positive-*` logs/TSVs/xtraces, archive-open traces, four reversion
campaigns, restoration hashes, and the gate log/comparison. Earlier
invalid-fixture runs remain labeled separately. No fresh coverage
measurement or unrelated backlog implementation was performed.
