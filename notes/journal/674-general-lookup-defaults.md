title: General input-lookup defaults
date: 2026-11-26

# General input-lookup defaults

DG distinguished an ordinary argument from a disposition operand and requested:
**"implement arg and getenv supporting general values for the default"**.
The AI implemented the extension on `err-attempt-trial`. Main and DD136's
provisional adoption boundary are unchanged.

`$(arg N DEFAULT)` and `$(getenv NAME DEFAULT)` now accept ordinary string
values for DEFAULT: variables, cells, composed words and calls. Integers
require explicit stringification, just as at other string operands.
Evaluate and retain the name/index, then DEFAULT, then perform the lookup.
The default runs once even when the supplied input wins; its effects and
failures are not conditional on absence. A bare lookup with `||` remains the
lazy alternative.

Only absence selects the default. Present-empty remains empty; an explicitly
empty default still selects the value-only form. All four lookup forms remain
tainted acquisitions. The bare forms still require presence handling or
discard, and the defaulted forms still have no separate status to consume.
Negative indices, ordinary type/taint checks and operand/body boundaries are
unchanged. `:-` and `:?` remain literal-only.

## Shared implementation

The validator's lookup-only SB4070 restriction, its private `first_expansion`
walker and the obsolete walker unit are removed. The surviving check is named
`check_arg_calls` and retains the existing index and arity diagnostics.
The parser's independent SB4070 disposition check is unchanged.

The string-operand checker now selects positional ranges rather than a leading
count. `arg`'s second operand joins the existing string check while its first
stays in the integer-sink check. Its error hint quotes the default, not the
index.

Both emitters already accept general SSA lookup operands. Their local emission
order, however, performs the lookup before evaluating the default. Simply
removing the refusal would therefore allow an invalid-index failure to precede
the default's own required-value failure.

Shared lowering materializes defaulted lookups with the existing exhaustive
value lifter, unsuppressed operand boundary and scalar-retention helper.
Earlier values survive later mutations; the completed lookup is retained
before subsequent operands. The stage classifier also recognizes a defaulted
lookup without needing a nested capture to trigger lifting. There is no runtime,
ABI or backend-emitter change, and no new temporary-allocation mechanism.
General argument-list ordering and attached-redirect timing remain with their
existing owners.

DD101/DD60 define the lookup contract, DD105 its typing, and DD65 the separate
literal disposition rule. Companion DDs, the outline and authored tour are
reconciled in place, with superseded restrictions marked as historical.

## Focused evidence

Nineteen fresh assertions per mode replace seven old-contract assertions:
`lookup.66`-`.83` and `capwalk.12` are new; `av.d4`, `av.d7`,
`lookup.22`-`.24`, `lookup.63` and `capwalk.11` retire.
The prefix high-water marks came from spec history, not only source grep.

| Claims | Specs |
|---|---|
| General intrinsics, stateful defaults, variables and present-empty selection | `lookup.66`-`.68` |
| Composed words, cells, nested calls and once-only eager effects | `lookup.69`-`.70` |
| Value-only result and source-discard rejection | `lookup.71` |
| Name/index retention before a mutating default | `lookup.72` |
| Ordinary body failures and defaults-before-lookup failure order | `lookup.73`-`.75` |
| Integer refusal, correct hint and tainted-default reads | `lookup.76`-`.78`, `capwalk.12` |
| Independent SSA taint and counted NUL bytes | `lookup.79`-`.81` |
| Completed lookup before later effects, including simple stage operands | `lookup.82`-`.83` |

The frozen original compiler fails exactly nineteen assertions per mode;
every retained assertion passes. The positive compiler passes `argv.sh`
81/81, `capture_cond.sh` 129/129 and `taint.sh` 556/556 in native and C.
The final hint refinement was checked by the strengthened `lookup.76` in
another positive 129/129 pair.

Four independent reversions run the complete capture-condition file in both
modes. Every observed failure set matches its prediction:

| Reversion | Failing `lookup` suffixes in each mode |
|---|---|
| Omit the new ordered lookup path | 72, 75, 82, 83 |
| Retain operands but not the lookup result | 82 |
| Omit defaulted lookups from stage-lifting classification | 83 |
| Omit `arg`'s string-default slot | 76 |

All stubs are restored. Source, specs and the rebuilt compiler match the saved
positive hashes; the runtime archive, both local locks and protected user files
remain unchanged.

## Full gate and existing timestamp failures

The first gate used `NEW=38 FLIPS=0 RETIRED=14 TOTAL=13226`.
It recorded rows 7491081-7504306, with the exact 6729 native / 6497 C counts,
38 first-ever IDs and 14 retirements. The included unit suites pass
240/27/20 tests. Static checks, plan agreement, ghosts and liveness pass.

The sweep was not accepted: only `native:ft.2`, `native:ft.3` and
`native:ft.5` failed, the same timestamp cases previously observed before this
change. Their actual xtrace shows successful compilation and `no` results,
not lookup diagnostics. Frozen-original/new-compiler controls produce
byte-identical executables, preserve the pinned timestamps and return `yes`.
The original full file passes; runtime-stat tracing on the new full file also
passes and records the expected kernel fields.

The failed run's cause remains unestablished. It is recorded separately in
[the timestamp failure task](../tasks/bug-filetest-pinned-timestamp-failures.md),
without changing the predicate or its assertions.

The unchanged replacement gate passes with
`NEW=0 FLIPS=3 RETIRED=0 TOTAL=13226`: rows 7504307-7517532, 336 seconds,
13213 passing and 13 existing skips. The three predicted corrections are
exactly the timestamp cases. There are no failures, ghosts or plan
mismatches; liveness is 246. The original failed sweep remains part of
the record, not evidence of a timestamp repair.

An independent comparison against the pre-task block ending at row 7491080
finds all 13188 retained statuses unchanged, exactly the 38 intended additions
and exactly the 14 intended retirements. No commit-hash baseline or comparison
against the just-finished sweep is used.

`mk -p 1 tour-doc` checks all 73 existing snippets with zero mismatches.
The generated GitHub edition changes only the lookup-default explanation.
All 19 changed Markdown documents render; 137 local links resolve and
supersession markup balances. DD statuses and dependencies stay unchanged;
DD101's title drops the superseded word "literal".

The positive source/compiler/runtime/lock/protected-file manifest still
matches after the gate. No coverage run or dependency refresh was performed.
Private evidence is retained in `lookup-defaults-20261126`, including the
before/positive pairs, four reversion pairs, failed and accepted gates,
timestamp investigation and row-boundary comparison.
