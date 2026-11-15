title: Enforce readonly ERR inputs and localized NUL proofs
status: open
priority: P2
kind: work
depends: [nul-predicate-and-filename-tests]
gates:
  - "ERR imports are readonly by logical binding identity, including write-only accesses, transitive same-process helpers, captures and A-E aliases"
  - "scalar and collection writes, reader/output targets, append, element mutation and unset cannot modify imports; ERR-owned working state and fresh private locals remain usable"
  - "import classification does not equate every HandlerEntry input with an import or infer exclusive ERR ownership solely from declaration location"
  - "an otherwise-unproved argv use is allowed only when the exact value is NUL-free on every reaching path: false 0?, true i?/p?/g? or true filename predicates"
  - "negation, short-circuiting, joins, loops, copies, defaults, collection selection and same-process calls preserve value identity and outcome polarity"
  - "ordinary main/EXIT mutability, existing known-safe ERR values, runtime argv guards, taint ownership and required :! reads remain intact"
  - "no blanket SB5101 exemption, compiler-wide predicate refinement, runtime freeze or public readonly builtin is introduced"
  - "focused native/C acceptance and refusal specs have before/reversion controls and the predicted ordinary gate passes"

## Contract

[DD135](../decisions/DD135-err-readonly-inputs-nul-and-mkdtemp.md) gives ERR
a readonly view of inherited bindings. Working state introduced and used
solely by ERR is not an import merely because its backing storage is global.
Existing lexical scope/no-shadowing and fresh function/capture locals remain.
A write before the first read cannot turn an import into fresh state.

Identify actual imported logical identities, including access only through
helpers and collection slots. `transform/entry.rs` currently seeds
`HandlerEntry.inputs` with **every** `ssa.globals` entry; that table is not
the import set. A declaration inside ERR is not sufficient proof of
exclusive ownership if ordinary execution also accesses the binding.

Enforcement must cover contents as well as names. `check_const_writes`
currently relies on scalar-only consts, and `ssa_taint::instruction_target`
is not a complete mutation census. Do not reuse either as a complete answer
without accounting for element/append/unset/output-target/alias channels.
Share existing canonical walks and decoded binding identities.

## NUL query

`terminal::check_err` already has `may_nul` value facts. Preserve that fast
path. Only an otherwise-unproved external argument needs a demand-driven
query at its actual instruction position in the ERR same-process graph.
There are expected to be few such uses; do not introduce a general
compiler-wide predicate-refinement pass.

The required evidence is the predicate **outcome on every reaching path**,
not source order or an occurrence of the variable name. False `0?` proves
NUL-freedom, including empty text. True `i?`/`p?`/`g?` and successful
filename predicates do too; binary filename success proves both operands.
True `0?` and the false outcome of a filename test cannot justify a NUL-free
use. Negation changes which path carries a proof, not the required outcome.
The predicate/runtime prerequisite must land before those facts are trusted.

Readonly imports supply stability. Still preserve actual value identity
through copies, phis, collection selection, lazy paths and helper lowering.
New mutable working state needs normal invalidation; changing an index or
using a default must not silently select a different value. An earlier
main-path proof cannot cross ERR's independent entry as if it dominated it.

Keep the actual runtime argv guard. Existing literal, numeric, environment
and argument byte facts remain useful independently of taint. `:!` permits
a read but supplies no NUL proof; an ordinary successful predicate supplies
no taint clearance. Imported taint cannot be repaired by attest or overwrite.
Fresh input can instead enter writable ERR-owned working state.

## Evidence and boundaries

The existing native control in the session's
`files/tour-cleanup-directory-proof-20261114.json` still receives SB5101
for a handled external argument after `-d ${work:!}` in a cleanup helper.
The runtime also truncates the filename test at NUL, so deleting that
refusal before the prerequisite repair would be unsound. No new compiler
probe was run for this task record.

Pin the direct/helper cleanup forms, both predicate polarities, wrong-value
and one-predecessor refusals, loops and collection aliases. Include writes
to an import through a helper also used legally from main, write-only
imports, ERR-only working bindings and private mutable copies.

Do not re-walk taint bodies under caller-specific contexts or change handler
dispatch. Other uncatchable failures retain
[runtime-failure-classification](runtime-failure-classification.md) and
[recoverable-file-failures](recoverable-file-failures.md) as their owners.
[mkdtemp](mkdtemp-value-intrinsic.md) is independently useful and is not a
prerequisite for these predicate proofs.
