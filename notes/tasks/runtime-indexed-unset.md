title: Support runtime keys and indices in unset targets
status: done
priority: P3
kind: work
depends: []
gates:
  - "unset on a statically named collection accepts ordinary runtime string keys or integer indices, including unset choices[$pre]"
  - "Targets retain lexical collection identity; key expressions use the shared subscript grammar, type checks and ordered single evaluation"
  - "Taint, readonly/scratch, collection-slot and non-aliasing rules remain enforced, including inside uncalled functions"
  - "Focused native/C specs prove the new behavior and preserve literal deletion, whole-collection clearing and ordinary invalid-target diagnostics"
  - "Original-binary controls, independent reversions and the predicted ordinary gate pass; current DD and specification-outline wording is aligned with delivery"

## Request

DG requested this backlog item on 2026-11-22 after the
[nonliteral-unset ICE repair](../journal/665-unset-validation-ice.md).
The motivating form is:

```sh
unset choices[$pre]
```

`choices` remains a statically known map; only its key is computed at
runtime. Cover integer-array indices as well. This does not request
indirection through a runtime collection name, scalar unset, or a new
result/status on mutation.

Use the ordinary [DD80 key/index rules](../decisions/DD80-braced-key-discipline.md),
not a separate miniature expression language or a variable-only carve-out.
Preserve [DD86 evaluation and binding rules](../decisions/DD86-variable-scope.md),
[DD100 collection identities](../decisions/DD100-collection-parameters.md)
and [DD128 taint resolution](../decisions/DD128-exit-err-adjacent-attest.md).
Deleting an element must not silently validate the remaining collection.
The initial backlog left the internal representation open.

## Delivered, 2026-11-22

Implemented on `err-attempt-trial` using the existing subscript AST,
balanced scanner, key parser and typed operand saver. Shared target
decoding distinguishes storage identity from evaluated key reads; all
keys are retained before deletion or clearing begins. Lexical, taint,
type, scratch, args, alias and handler checks remain in force.

`unsetkey.1`-`.30` cover the extension and its boundaries in native and C.
The old `noalias.18` refusal is retired in favor of fresh `unsetkey.27`;
quoted/indirect target controls remain. Original-binary controls,
independent reversions and the corrected 12931-check gate pass.
[Journal 667](../journal/667-snapshot-key-loops-runtime-unset.md) records
the implementation, coupled snapshot-taint repair and complete accounting.
DD80, GUIDE and the specification outline are aligned. DD136 remains
provisional and main is unchanged.

## Pre-change implementation and verification plan

At `6e0414c6`, `transform/cmd_handlers.rs::unset_targets` decodes literal
words, and `unset_element` lowers the extracted key as `SsaValue::Literal`.
Both native and C deletion emitters already lower general key values,
and `sash_table_delete` accepts key bytes. The main work is retaining a
collection identity plus a real index expression through parsing, binding,
validation, typing and lowering, rather than simply removing SB5063.

Reuse shared walkers and subscript machinery. Key reads and nested calls
must participate in ordinary type, taint, alias and evaluation-order checks;
the target is a collection identity, not a scalar value read. Early analyses
must still tolerate malformed targets until the canonical diagnostic gate.
Test called and uncalled bodies, slots, multiple targets, wrong key types,
missing entries, and expressions whose effects expose repeated or reordered
evaluation.

Review `noalias.18` when changing the runtime-key refusal; retain the
indirect/quoted-target controls in `.19` and `.20` unless a separately
approved rule changes them. Changed contracts need fresh historical spec
IDs, not a silent assertion rewrite. Keep the separately approved
snapshot-at-entry repair distinct from key-expression support; deletion
during a loop needs its own evidence.

The [live-iteration deletion skip](bug-map-iteration-deletion-skip.md) is
already reproducible with literal unset targets. It is a separate defect,
not something fixed merely by accepting runtime keys.

The motivating form received SB5063 at this baseline. The delivered
extension supersedes that refusal; the earlier ICE repair remains a
separate diagnostic milestone.
