title: Refine unset semantics — scalar vs collection
status: ready
depends: []
severity: correctness
gates:
  - tests
  - commit

Current implementation treats all `unset var` as assign-empty-string
in transform.rs (~line 567). This is wrong for collections and should
not exist for scalars in sash-native.

### Changes

**Sash-native:**
- `unset var` (scalar) → compile error. Use `var=""`.
- `unset arr[N]` → remove element (keep, already works via __table_delete)
- `unset map[key]` → remove key (keep, already works via __table_delete)
- `unset arr` / `unset map` → clear collection (assign empty table,
  not empty string — current implementation is wrong)

**Bash-mode coercion:**
- `unset var` (scalar) → coerce to `var=""` in lowerer

### Implementation

1. Validator: new rule — `unset` on scalar variable in sash-native
   is a compile error with suggestion to use `var=""`.
2. Lowerer (bash-mode): detect scalar `unset`, rewrite to assignment.
3. Transform: fix `unset arr` / `unset map` to emit empty IndexedArray
   or AssocArray instead of empty string. Need to check whether the
   variable is a collection (state.assoc_arrays or type info).

### Ref

transform.rs ~line 536
SUBSET.md builtins section
