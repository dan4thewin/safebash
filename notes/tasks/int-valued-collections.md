---
title: "DD#76 — int-valued collections (int array / int map) + bare a[i] arith lvalue"
status: done
priority: P2
kind: feature
depends: []
resolved: "2026-08-20 — Stage C (bare a[i] in arith, rvalue + lvalue) shipped via DD#79 this session (commit 38bb25c): (( a[i]++ ))/(( a[i] += e ))/(( a[i] = e )) on an int array work; bare m[k] in arith → error. Stages A/B (decl + codegen store/read) done earlier. Backend gaps are separate filed tasks (janet-int-valued-collections, c-local-int-collection-decl)."
gates:
  - "`int array a=(10 20 30)` / `int map m=([k]=5)` declare; non-numeric element → type error"
  - "int cell stored as {val_ptr:null, val_len:i64} (DD#76); string collections unchanged"
  - "`${a[i]}` / `${m[k]}` on an int collection types Int: `int x=${a[0]}` OK; arith read is raw val_len (no atoi)"
  - "string-context read of an int cell → itoa(val_len) (`echo ${a[0]}` → decimal)"
  - "`a[i]=$(( … ))` on an int collection stores int WITHOUT DD#30 quotes; string collection still needs `\"…\"`"
  - "bare `a[i]` in `(( ))` — int index expr, rvalue + lvalue (`(( a[i]++ ))`, `(( a[i] = … ))`, `(( a[i] op= … ))`); bare `m[k]` → error"
  - "missing key / int 0 both read zero-value (DD#53) — no bounds error (DD#76)"
  - "native + c + janet (or per-mode guards filed); spec tests; no regression"
---

# Staging

Design: DD#76 (int-valued collections, val_len double-duty) + arith-subscript-
values (the `${…}` rvalue import, stage 1 done; bare `a[i]` lvalue here).

## Progress

- [x] **Stage A (parser + type)** — top-level `int array`/`int map` declaration
  (surgical add in parse_declaration; `local int array` already stacked via
  parse_local). type_check: `${a[i]}`/`${m[k]}` on an int collection types Int
  (expr_type Subscript arm keyed on var_type(name)==Int); int-collection element
  literals must be numeric (check_int_collection_elements, SB4030 otherwise).
- [x] **Stage B (codegen store/read, native + c)** — new `SsaValue::IntCell`
  (write path → `(null, i64)`) wraps int-collection literal elements
  (wrap_int_cells) and int-cell assign RHS (build_subscript_rhs);
  `is_integer_value` on `SsaValue::Subscript` (read path: string ctx → itoa,
  int ctx → raw val_len, no atoi). Collection var slotted as a table (Str) even
  when element type is int (setup.rs declare_inst_bases; emit_c
  collect_inst_var_types + emit_assign). Runtime unchanged (stores (val_ptr,
  val_len) verbatim). spec arithmetic.sh ivc.1-11; native+c 0 regressions.
- [ ] **Stage C** — bare `a[i]` in arithmetic (arith-subscript-values stage 2):
  lexer `[`/`]`, `ArithExpr::Subscript{name,index}`, rvalue + lvalue
  (`(( a[i]++ ))`, `(( a[i]=… ))`, `(( a[i] op= … ))`); bare `m[k]` → error.
- [ ] **Backends** — janet int collections (janet-int-valued-collections);
  emit_c function-local int collection (c-local-int-collection-decl). ivc.8
  native-only pending the c fix; janet ivc runtime tests skipped pending janet.

## Remaining stage detail


- `parse_declaration`: allow the `int` type keyword to stack onto `array`/`map`
  → an int-valued collection declaration (DD#35 order: `int array`/`int map`).
  Today `array`/`map` hardwire `VarType::String`; thread an element type.
- IR/`Declaration`: carry the element type (int vs string) for a collection.
- type_check: track int collections (a set, or reuse `collections` + an
  `int_collections` set); `${a[i]}`/`${m[k]}` on an int collection → `VarType::
  Int`; element initializers must be numeric.

## Stage B — codegen int-cell store/read

- store (array-literal init + `a[i]=`): for an int collection, `table_set` with
  `val_ptr = null`, `val_len = <the i64>` (lower the element/rhs as int).
- read (`lower_subscript_value`): `is_integer_value` branch — arith/int context
  returns `val_len` directly; string context `itoa(val_len)`.
- the `is_integer_value` flag comes from the collection's static type (type map),
  mirroring `is_integer_key`.

## Stage C — bare `a[i]` in arithmetic (arith-subscript-values stage 2)

- arith lexer: handle `[` / `]` (was SB4035).
- `ArithExpr::Subscript { name, index: Box<ArithExpr> }` — index is an int arith
  expr; rvalue read (table_get → int).
- subscript as arith **lvalue**: `(( a[i]++ ))`, `(( a[i] = e ))`, `(( a[i] op= e ))`
  → read-modify-write (reuse the statement subscript-assign write path). For an
  int array the cell is a raw i64 (no coercion); a string array cell round-trips
  through itoa on store.
- bare `m[k]` in arith → error (string key not expressible under arith rules;
  use `${m[k]}` to read, `m[k]=` to write — arith-subscript-values).

## Notes

- `${…}` stays rvalue-only in arith (arith-subscript-values / model B).
- backends: emit_c + emit_janet need the int-cell store/read; guard + file per
  backend if a stage lands native-first (janet already has an arith-subscript
  value-read gap: janet-arith-subscript-read).
