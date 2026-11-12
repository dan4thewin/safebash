title: "bug: subscripting a scalar (`${s[0]}`) segfaults"
status: done
priority: P1
kind: bug
depends: []
gates:
  - "`s=hello; echo ${s[0]}` is a COMPILE error naming the type mistake, not a segfault"
  - "the undefined case stays SB5031 (it already is)"

## What

```sh
s=hello
echo "[${s[0]}]"     # Segmentation fault (core dumped), rc 139
echo "[${s[9]}]"     # likewise
```

Compiles clean, then dumps core. An **undefined** name is correctly rejected
(SB5031), so the guard exists — it just does not cover "declared, but not a
collection".

The subscript lowers to a table read; a scalar's slot holds a string pointer,
so the read dereferences a `char*` as a `SashTable*`.

## Type context

Subscript is **collection-polymorphic** by design (array by position, map by
key — both work, DD#80). The defect is that the third case, a scalar subject,
is neither supported nor rejected.

Sibling operations reject it properly: `keys s` and `join s` are both SB4030
("`s` is a scalar"). The same check applied to the subscript path is likely
all this needs.

## Found

2026-09-10, while surveying which operations in the language are
type-polymorphic (`len`, `join`, subscript, for-in) for the `len` review.

## Resolution (2026-09-12)

**SB5068** in `validate.rs`, adjacent to its dual SB5064. Probing expanded the
filed reproducer into **seven** faces of one defect — six segfaults (read,
dynamic index, `[[ ]]` operand, arithmetic operand, loop variable, subscript
assignment) and a compiler **ICE** for an `int` subject. All seven are now
compile errors.

The hole was `check_subscript_key_kind` (`type_check.rs`), which returns early
for a subject with no collection kind. The fix does **not** live there: a
`for` variable is never declared into `TypeEnv.vars`, so a check written at
that site would have missed the loop-variable face. `validate.rs` has
`collect_scope_names`, which does include loop variables.

The allow-set is declarations ∪ bulk-fill targets (`read`/`mapfile`/`matchall`/
`argv` introduce a collection with no declaration) ∪ DD#100 slots `A`..`E` ∪
`args`. An undefined name stays SB5031, per the second gate.

`walk_arith_subscripts` moved from `type_check.rs` to `ir.rs`; both passes
share it.

Specs: `spec/types.sh` 75.44–75.54. Journal: `notes/journal/263-scalar-subscript.md`.
