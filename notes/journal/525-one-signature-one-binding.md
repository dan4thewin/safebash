# 525 - One signature, one binding

2026-10-18. DG continued the coverage campaign from `2bf0550`: 122 compiler
production lines cold. Twelve type-check/helper sites exposed bugs that
could not be closed by deleting one measured exemption.

## A slot must carry its full type

A string cell containing `nonsense` evaluated to zero in arithmetic through
slot `A`, in both backends. A string assignment through an int collection
slot reached native's non-integer-literal ICE; `strlen` accepted an int
cell; forwarding `=A` lost collection kind. Direct ordinary-cell reads
already rejected these operations.

Signatures recorded only array/map, ignored forwarded slots, and several
auxiliary type-checking visitors entered function bodies without their slot
environment. DG settled the policy:

> "while we do infer the signature of a function, I don't think we can allow
> a function to have more than one signature"

Every called function now has one fixed collection count and one complete
type per slot: string array, int array, string map or int map. Forwarding
propagates through a monotone fixpoint before disagreements report SB4030.
Every relevant visitor enters the same inferred function environment.
String-producing bulk fills cannot write int collections. DD#81's scalar
fixed/variadic rules are unchanged. Forty dual-mode `slotty` regressions pin
the complete contract; DD#100 records DG's ruling.

## The spelling was not the identity

A valid sibling-capture control first hit false redeclaration: one capture's
locals escaped into the next. Cloning inherited declaration state admits
sibling locals while preserving no-shadow.

That exposed the deeper defect. Sibling functions could each declare
`items`, one a string array and the other an int array, yet both backends
printed **`4` then `7`**, not `word` then `7`. Type inference, SSA versioning
and collection lookup all keyed the same unqualified spelling. Isolating
one checker would leave the other consumers wrong.

DG chose centralized lexical binding resolution:

> "do 1; could prepend function name to locals..."

`bindings.rs` resolves a compiler copy once, after structural/path checks
and before semantic checks, typing and SSA. Named locals use function-name
prefixes; captures get separate owners. Allocation reserves source tokens
and disambiguates collisions deterministically. Globals retain their
identity; named functions inherit globals rather than caller locals.
Captures inherit enclosing bindings without exporting their introductions.
Attest-all binders are predicate-local.

The resolver rewrites variable/name operands, including builtin targets and
actual user-function collection markers, not literal values, literal map
keys or external-command arguments. Its walkers and the shared declaration
collector are explicitly total. All semantic consumers share the resolved
AST. C and manifests reuse the checked SSA instead of rebuilding from the
original AST. Diagnostics and runtime attest labels recover source names;
raw IR and embedded artifacts retain original source/AST. DD#86 and GUIDE
describe the seam.

The final diagnostic control caught another coupled failure: formatting an
SSA version before restoring its base name leaked qualified identifiers.
Worse, a formatted version could equal another binding's base and be
replaced by the wrong variable. `a`'s local `b_t` and `a_b`'s local `t`
demonstrate both outcomes. The SSA backstop now receives `SourceNames` and
restores structured names before appending versions, not by guessing where
an underscore-delimited name ends. Two source regressions pin capture names
and the real alias/version collision; no duplicate unit assertion.

## Brackets did not make an unknown name a collection

Fresh coverage probing found `unset missing[0]; echo done` compiled and
segfaulted natively. `check_unset_args` skipped all indexed operands without
checking their base. Inspection also found whole and indexed slot deletion
failed to resolve `A` to the caller's collection.

Both checks and lowering now reuse the canonical `unset_targets`
normalizer, including split bracket fragments and multiple operands.
Unknown/scalar indexed bases report SB4030. Both deletion paths resolve
collection aliases before lookup and mutate the actual caller collection.
Missing elements of a declared collection remain harmless no-ops.

The old whole-unset scalar-assignment fallback is an invariant instead.
Its first assertion was too strong: dead function templates legitimately
retain reserved `A`-`E` before real calls inline. The invariant permits those
template slots as well as known collections; its direct API test pins a
genuinely malformed target. Thirteen source cases cover unknown/scalar
refusal, all four slot types, multiple targets, whole clearing and forwarded
writeback. DD#30 records the contract.

## The original cold contracts still mattered

Direct API tests pin source-inaccessible name/default/substring answers,
constant proof, operator rendering, conversion hints, reconstruction decline,
unknown signature metadata and the binding mutation invariant. The latter
was initially nested accidentally inside a function; the unit count caught
it, and the final same-file module actually executes it.

Attribution showed the absent arithmetic-default answer is source-warm:
`cov83` reaches it. Removed the rival unit assertion. Consolidated the
private join-name accessor into canonical `expr_to_name`.

Removed arithmetic-signature support for rejected bare `args[k]` rather than
documenting unsupported syntax in a dead branch. `args` is a string array;
DD#77's disposition rule applies to its cells too. Updated the two old
expectations, not the language policy. The existing redirect-backstop
fixture now proves nonempty with `:?` so DD#119 does not mask its taint
assertion.

## Measurement and publication gates

Fresh complete-scope union: **106 / 30434** production lines remain cold,
down **122 -> 106**, with no cold-file regressions. The new binding resolver,
native emission and C emission are zero cold. All **41** measured source
blobs are current. Type checking falls **25 -> 14**, command handlers
**4 -> 2**, function lowering **9 -> 7**, validation **33 -> 32**.

Raw cargo-llvm-cov: **99.16% line / 99.56% function / 94.50% branch**,
including inline test scaffolding. Exactly **8938 CLI profiles across all
32 slots** distinguish source execution from direct API contracts. Saved
CLI/union exports, merged profiles and the coverage database before
restoring ordinary tools.

Final prediction and result: **9075 = 9062 passing + 13 existing C skips**,
native **4640**, C **4435**. Collection specs are **124 per mode**:
45 original, 40 signature cases, 39 binding cases. Independent plans and
both global ID-collision checks agree. Historical high-water checks prevent
reuse of retired IDs.

All **8917** baseline statuses at **6093590-6102506** are preserved.
Exactly **158** intended mode-qualified additions, no unexpected additions,
retirements or previous-row flips; first-versus-current liveness **260**.
The final instrumented block is **6147808-6156882**; four first-ever IDs are
the two last diagnostic specs in both modes. The final ordinary block is
**6156883-6165957**, with zero first-ever IDs. Comparisons use earlier rowids,
never the dirty tree's shared commit hash.

Ordinary tooling restored with `mk cov-reset`. Final units:
**202 sash + 27 convert + 20 expand**. Native/C compiler and scoped/deletion
runtime canaries give the required output and produce no profiles with
default or configured profiling destinations. Staged fmt, clippy, totality
and live complexity gates pass without bypass; the refactor satisfied the
actual gate rather than retaining exemptions.

The existing run-3 adversarial-review task now names the new binding,
signature, deletion and taint seams. No coupled bug is deferred.
