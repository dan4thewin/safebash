title: Immutable regex values need reaching proofs
date: 2026-11-17

# Immutable regex values need reaching proofs

DG's DD132 trial requires every runtime regex operand to be immutable and
successfully validated for the same value before use. The AI implemented
that prerequisite on `glob-scratch-trial`, after `f5371e63`. Main remains
`6c7b899f`; the complete trial and DG's adoption decision are still pending.

## What is proved

SB5106 covers existing match forms, out-array extraction, sub/gsub, logical
tests, dispatch and every substitution-sugar edit. It runs unconditionally
on the shared checked SSA before terminal analysis and inspection exits;
neither taint switch disables it. Literal patterns retain static validation.
Source consts and scratch, managed argument arrays and compiler-held copies
provide immutable storage, not automatic validity. Testing mutable source
storage does not freeze it.

The new pass follows logical binding identities and stable cell selections.
It normalizes SSA versions through a canonical mutable variable walker,
recording every index dependency for invalidation. The existing instruction
write classifier kills facts on writes and fresh lifetime starts. Scalar
quoting preserves identity; byte-changing expressions need a separately
constructed and checked final candidate.

Facts retain equality between unchanged values, present cells, known array
sizes and validity certificates. A certificate carries its required lexical
scopes; independent alternatives survive only where their requirements hold.
An all-path merge requires both predecessors' proof. Caught scratch has a
separate permission restriction: an earlier certificate cannot overcome the
current failed/expired caught permission. Trusted copies need a fresh check,
and ordinary copies cannot export scoped proof.

The worklist has no arbitrary iteration cap. It converges before a second,
diagnostic-only traversal, avoiding transient refusals while inputs settle.
True/false alternatives survive unconditional edges as well as being selected
at branches. Inlined functions and captures use that same graph. Main, EXIT
and ERR start independently; background and pipeline children inherit facts
at their actual spawn point and never write them back into the parent.

Attest-all certifies the present values or keys whose binder checks prove
`r?`, without borrowing arbitrary predicate facts from an empty collection.
Literal indices, known-size forwarded splats, required present values and
live map-key iteration preserve relevant membership. A missing-cell fallback
is not a checked element. Unstable indices cannot acquire a reusable witness
or copy-equality relation.

The shared SSA API now has mutable-child traversal and variable mapping.
Two direct tests assert read-only/mutable traversal order, actual storage
replacement and all nested storage references. The terminal NUL prover
shares the extracted edge-outcome classifier. The older terminal-only
dynamic-regex refusal is removed: mandatory proof owns invalid-pattern
eligibility before ERR checking, rather than a second exact-text heuristic.
No runtime, ABI, compiled handle, cache or dependency changes.

## Controls and one corrected prediction

The final regex file has 262 cases. The frozen original compiler fails
exactly 55 per backend, with no unexpected failures; the positive passes
all 262. The source-reversion campaigns are independent and fully restored.
Numbers below are `regexproof` suffixes; every listed full red set was the
same in native and C.

| Disabled mechanism | Full observed regex red set |
|---|---|
| Proof diagnostics | Original 55-red set except `.40` |
| Immutable eligibility | 10, 53, 57, 80 |
| All-path join | 15, 16, 31, 56, 81, 93 |
| Lexical scope expiry | 17, 18, 58, 96, 98 |
| Subject permissions | 17, 18, 38, 58, 96, 98 |
| Copy equivalence | 70, 73, 74, 76 |
| Write invalidation | 64, 65, 92, 93 |
| Stable proof storage | 88 |
| Stable copy aliases | 89 |
| Independent handler roots | 19, 78 |
| Fork inheritance / visitation | 60 / 37 |
| Pipeline inheritance / visitation | 61 / 62 |
| Aggregate witnesses | 27, 28, 35, 66, 77, 91 |
| Cell membership | 99 |
| Live-key membership | 66 |
| Forwarded splats | 77 |
| Conditional outcomes | 22, 23, 32, 40, 52, 63, 73, 74, 79, 90, 97 |
| Embedded conjunction | 55, 66 |
| Fresh result invalidation | 81 |
| Unconditional-edge outcome transport | 73, 74 |

These are 22 source campaigns. Diagnostic removal also turns the terminal,
errors and operand-word proof checks red. Two further independent campaigns
break one SSA API test each while leaving its companion green: reverse the
mutable RegexMatch child order, or omit TableValue/TableKey's index reference.
Thus 24 successful independent campaigns, not 24 source regressions.

The matrix declared selected red sets and separately retained additional
reds; not every member of each full set was predicted. The `.73/.74`
helper-return control initially mutated `SetTestFromStatus` and stayed
entirely green. Its xtraces genuinely showed successful matching. Producer
enumeration found that instruction belongs to capture-conditions, not these
direct returned predicates. A separately declared mutation of `Flow::edge`
on Goto edges turns exactly `.73/.74` red. Both results and the mistaken
prediction are preserved; the first is not counted as a successful control.

An earlier apparent escape through a multi-statement condition was also
withdrawn after actual invalid-candidate controls: that attest currently
lowers as an ordinary aborting instruction, not a caught predicate. It aborts
before the apparent failure arm in both backends. No behavior was changed
to fit the mistaken diagnosis; the remaining caught-form work must address
lowering, taint, outcomes and signals together.

## The first full gate found real caller migrations

The initial predictions were NEW198/RETIRED30/FLIPS0/TOTAL12211. Rows
7147139-7159337 instead contain 12199 checks and ten failures. Five old
programs, in both modes, still supplied unchecked patterns:

- `cov.223/.224`: identifier scanning through interpolated sed patterns.
- `dp.mp10`: an unspaced dispatch with an assembled pattern.
- `vm.tu7`: case-converted pattern and subject.
- `96.7`: a runtime pattern in the first chained edit.

Those assertions now use fresh `regexproof.100`-`.104` IDs, preserving the
output/scanner purpose while validating immutable final candidates. The
dispatch case constructs its multipart candidate before the unspaced use;
unchecked multipart pattern operands are no longer a legal success case.

The missing twelve mode IDs were not intended retirements. The tour helper
failed compilation before `tourgen.1`-`.6`: its output-fence loop used a
mutable string directly as a regex. The retained xtrace named SB5106 at
that use. It now snapshots and validates a fresh `_fence` per attempt, then
grows the mutable builder only after a match. The existing publication and
backtick-fence assertions remain unchanged.

The purge cap correctly refused all 42 apparent retirements rather than
silently deleting the tour cases. It was not raised to accommodate them.
Both original and positive compilers pass the complete coverage, dispatch,
var-modifier and harness files in both modes after migration, with exact
plans 374/73/176/9. The actual tour also renders all 56 snippets with zero
mismatches, producing byte-identical `docs/tour/README.md`.

## Accepted ordinary gate

```text
NEW=10 FLIPS=0 RETIRED=40 TOTAL=12211
rows 7159338-7171548
native 6217 + C 5994
12198 passing + 13 existing skips
```

The corrected sweep took 285 seconds. It has no failures, flips, ghosts or
plan mismatches; liveness is 248. Compiler/convert/expand units are 241/27/20.
Relative to the last accepted trust-guard block ending 7147138, all 12003
retained statuses are unchanged, with 208 added and 40 retired mode IDs.
The first sweep had already recorded 198 of those additions; the retry's
ten are the five newly discovered migrations.

The twenty retired source IDs are `63.b1`, `63.d7`, `39.27`, `39.46`,
`39.60`, `39.53`-`.56`, `regextrust.23`, `e.a70.regcomp`, `ow.8`-`.10`,
`ow.12`, `cov.223/.224`, `dp.mp10`, `vm.tu7` and `96.7`. Their replacements
use new contracts rather than silently reusing historical identities.
All `regexproof.1`-`.104` IDs are new in this unit, across seven spec files.

All source stubs are restored. The source/compiler/runtime/local-lock
identities match the positive manifest after the gate; caller migrations
are recorded separately. Formatting, Clippy, totality and touched complexity
use the normal hooks without a bypass. No new coverage measurement is made.
Evidence remains under the session's
`files/glob-trial-20261117/regex-proof/immutable/`, including failed-gate
xtraces, original/positive controls, the reversion matrix and both gate logs.

## Still part of the same trial

Regex-only caught permission is not the whole DD132 caught-attest contract.
Ordinary scratch data reads outside the success arm still need enforcement,
failed unresolved scratch fall-through still needs its taint relaxation,
and the remaining caught forms need coherent handling. Substitution's
text-plus-Boolean result and `rglob` follow. Then measure useful programs,
helper/slot/converter migration and memory/time costs before DG evaluates
adoption. This unit adds no runtime storage, but its tour migration already
shows the extra candidate/check needed around a mutable pattern builder.
