# 529 - The test has one operand

2026-10-19. DG continued coverage from `1bc9e84`: 71 cold compiler
production lines. The selected cohort was all eight remaining lines in
`transform/stmt_handlers.rs`. Full-function and producer reading separated
one source gap from unused machinery and internal contracts.

DG asked about the repeated term "API-only". It meant direct Rust tests of
compiler functions, often private ones, not a supported public API.
**Direct-unit coverage** is clearer: source-reachable behavior still owes
a CLI spec; a test-constructed internal shape must not be presented as
source coverage.

## An integer return can forward a collection

`try_int_tail_call` correctly declines calls carrying collection slots.
The tail loop rebinds scalar `args`, not slots. A call to a different
function must take the normal inlining path; a recursive self-call with
slots must receive DD#27/DD#100's SB5037, not silently reuse the old slots.

Both source controls passed before edits in native and C. `tailcoll.1`
forwards an int array through `return $(leaf =A)` and prints 8.
`tailcoll.2` refuses the collection-bearing recursive self-call with SB5037.
The actual `functions.sh`, with both checks present, produced 214 profiles
and warmed the previously cold decline at old line 500. The real file
passes all 214 checks in both modes before and after the changes.

An initial fixture used `(leaf =A)` inside arithmetic. That is SB4020 by
the explicit DD#100 **Open** clause: the arithmetic lexer owns `=`.
It was an invalid reachability probe, not a compiler bug or a reason to
change the language. The supported command-capture return is the source
form the new specs assert.

`cov-batch --spec` suppresses suite exit failures, so its profile count is
not a success assertion. An initial attempt to grep the trace for plain
`PASSED` watched the wrong stream: the harness sends that line to fd 3,
not its trace. Explicit standalone runs supplied the success evidence.

## A selected test is not a search through arbitrary arguments

The parser's real `[[` producer constructs exactly one `ExprKind::Test`
operand. Lowering nevertheless searched every argument, accepted the first
test it found, ignored the rest, and returned `None` if none existed.
After builtin dispatch has selected `[[`, that decline routes malformed
IR into general command handling.

The selected handler is now total over its input contract: exactly one
Test operand, or a loud refusal. It retains the existing capture lifting
and emits the test in the block where lifting finishes. It no longer
advertises the obsolete `test`/`[` command-head shapes.

The new direct-unit matrix failed before implementation for the specific
reason **"this shape must ICE"**: the empty selected test returned block 0.
Afterwards it pins five malformed shapes: missing, wrong-kind, leading
garbage, trailing garbage and duplicate Test operands. It calls the real
command dispatcher with the test result consumed, not just the leaf helper.

No accepted source demonstrated this malformed-IR failure. Producer
enumeration found only the parser's one-operand constructor. Quoted and
escaped head controls refuse earlier with SB5096 and SB5011 respectively;
neither is a source route to the internal guard.

## Remove the answers no caller asks for

The three-argument regex-fill path had already checked its arity before
checking it a second time. The second guard is removed.

All four callers of bulk-target autodeclaration passed `as_map = false`.
The unused map constructor and the boolean parameter are gone; `read`,
`argv`, `keys` and regex fill all use the narrowed array constructor.
This does not remove source map support: none of these calls constructed
a map through this helper.

Attest predicate dispatch now handles arithmetic conversion directly and
Test in the other exhaustive arm. `classify_attest_convert` produces only
a scalar source, so the old second arithmetic arm was impossible within
the same function. Exhaustiveness remains; the duplicate pairing guard
does not. The quoted-head comment also now names the actual SB5096 gate.

## One body lookup, one refusal

A captured function value is produced only for a registered function body.
No body removal intervenes before assignment capture. Yet that path could
decline, while argument capture already refused a missing body.

Both callers now require the shared `capture_func_into` result. Its body
lookup owns the existing missing-body panic; neither caller duplicates it
or permits an unlowered function value to escape. The existing
`unregistered_function_value_cannot_remain_unlifted` unit still pins that
same message through the argument route. No second assertion or artificial
assignment scaffold was added.

One new compiler unit brings the count **219 -> 220**. Staged formatting,
clippy, totality and live complexity gates pass without bypass. These are
internal contract simplifications and coverage additions; no source
behavior defect was demonstrated, and no language decision was needed.

## Complete-scope verification

The selected file closes **8 -> 0**. Compiler production cold lines fall
**71 -> 63 / 30714**, with a 12-line smaller production denominator.
No cold-file regressions; binding resolution, control flow and both
emitters remain zero cold. All **42** measured source blobs are current.

Raw cargo-llvm-cov: **99.25% line / 99.57% function / 94.97% branch**,
including inline test scaffolding. Exactly **9122 CLI profiles across all
32 slots** separate source execution from direct-unit reachability:

| Site | CLI | Union |
|------|----:|------:|
| Selected Test operand invariant | 0 | 5 |
| Shared missing-body refusal | 0 | 1 |
| Collection-bearing int return decline | 10 | 10 |
| Statement arithmetic conversion | 30 | 30 |
| Bulk array initialization | 286 | 286 |
| Assignment capture's shared call | 20 | 20 |
| Argument capture's shared call | 138 | 139 |

The instrumented sweep matches its prediction exactly:
**9259 = 9246 passing + 13 existing C skips**, native **4732**, C **4527**.
All **9255** baseline statuses at **6230485-6239739** are preserved,
with exactly four intended and first-ever mode IDs from `tailcoll.1` and
`tailcoll.2`. No unexpected additions, retirements or previous-row flips;
liveness remains **260**. The sweep block is **6239740-6248998**.
Independent source plans and both global ID collision checks agree,
including `coverage.sh`'s two-check variable-id expansion.

Saved the CLI/union exports, merged profiles and database before ordinary
restoration. Remaining cold files: expression handlers (17), type checking
(14), validation (32).

## Ordinary publication gates

`mk cov-reset` completed its clean ordinary rebuild. The known Go `mk`
deadlock did not recur in this reset; no retry was needed. Serial
`mk -p 1 test` passes **220 compiler + 27 converter + 20 expansion units**.
The existing runner investigation remains separate and open.

The ordinary sweep again produces exactly **9259** checks with the same
**9246 passing / 13 skips**. Its block is **6248999-6258257**. The exact
baseline gate preserves all 9255 earlier statuses and the four intended
additions, with zero first-ever IDs on this repeat, no retirements/flips,
and liveness 260.

Native and C canaries exercise assignment/argument captures, Test capture,
the forwarded integer return and bulk array autodeclaration. Both print
the required `8` / `1` with empty runtime stderr. Default and explicit
compiler/runtime profiling destinations receive no profiles; the existing
target profile pool is unchanged. Freshness still certifies all 42 source
blobs after restoration.

Journal and status record the measured outcome. There is no new
architecture, runtime or language surface, so no GUIDE/DD change is needed.
Final staged fmt, clippy, totality and live complexity gates pass without
bypass; the measured Rust sources have not changed since the union.
