---
title: BUG — dynamic map subscript inside `[[ ]]` does an integer lookup (test-path)
status: done
priority: P2
kind: bug
depends: []
superseded_by: dd62-test-expr-ssavalue
gates:
  - "[[ ${m[$k]} = v ]] evaluates the map string key (parity with echo/assign)"
  - a types.sh test locks dynamic map subscript in a test expression
---

# `[[ ${m[$k]} ]]` does an integer lookup on a map

> **Superseded 2026-06-17** by `dd62-test-expr-ssavalue` (DD#62).
> The clean fix is to lower `[[ ]]` value operands to `SsaValue` and
> delete the duplicate test-path `lower_subscript` entirely, rather than
> threading map-type info into it. Kept for the repro/analysis below.

Found 2026-06-17 implementing dynamic subscript (journal 113). The
map-iteration idiom works in command/assignment contexts —
`echo ${m[$k]}`, `v=${m[$k]}`, `for k in $(keys m); do echo ${m[$k]}` —
but a **dynamic map subscript used directly inside `[[ ]]`** silently
misbehaves:

```
$ sash -c 'map m=([a]=1); k=a; [[ ${m[$k]} = 1 ]] && echo MATCH || echo NOMATCH'
            # (no output at all — neither MATCH nor NOMATCH; exit 0)
$ sash -c 'map m=([a]=1); k=a; v=${m[$k]}; [[ $v = 1 ]] && echo MATCH'
MATCH       # via a temp var it is correct
```

Working in `[[ ]]`: literal map key `${m[a]}`, and *indexed* dynamic
`${a[$i]}` (integer path). Only **map + dynamic key** is wrong.

## Root cause

The `[[ ]]` codegen has its **own** subscript lowering,
`codegen/values.rs::lower_subscript` (distinct from the SSA path's
`lower_subscript_value`, which threads `is_integer_key` from transform's
`assoc_arrays`). It guesses the key type from index *syntax*:

```rust
let use_string_key = match &index.kind {
    Literal(s)           => !s.chars().all(|c| c.is_ascii_digit()),
    SimpleExpansion(_) | ParameterExpansion(_) => false, // <-- assumes integer
    _ => false,
};
```

A `SimpleExpansion` index (`$k`) defaults to the **integer** path, so on
a map it does `atoi("a")=0`, builds an 8-byte key, and `table_get`
misses. (Before dynamic subscript existed, the index was the literal
`"$k"` → string-key path → also missed, but as a defined empty string;
the new failure mode produces no output.)

The heuristic cannot distinguish map from indexed for a dynamic index
because `lower_subscript` has no type info — `VarSlot` is just `Str/Int`,
and the test-path value lowering takes `vars/literals/externs`, not
`InstCtx` (which has `program: &SsaProgram`).

## Proposed fix

Thread map-awareness into the test-path lowering, one of:
1. Pass a `&HashSet<String>` of assoc-array names down from the SSA test
   instruction (transform has `state.assoc_arrays`) to `lower_subscript`,
   and set `use_string_key = assoc.contains(name)`.
2. Or add `is_integer_key` to the SSA test operand (resolve in transform,
   like `subscript_to_value` does) so codegen doesn't re-guess.

Option 1 is the smaller change; both remove the syntax heuristic.

## Proposed spec (TDD), types.sh

- `map m=([a]=1 [b]=2); k=b; [[ ${m[$k]} = 2 ]] && echo yes` -> `yes`
- `map m=([a]=1); k=a; [[ ${m[$k]} = 9 ]] || echo no` -> `no`
- keep the working indexed case: `array a=(10 20); i=1; [[ ${a[$i]} = 20 ]] && echo yes` -> `yes`

## Two adjacent pre-existing subscript-context gaps (literal *also* fails)

Not caused by the dynamic-subscript work — `${a[2]}` literal fails these
too — but worth a spec when this is picked up:

- **Arithmetic subscript**: `echo $(( ${a[2]} + 5 ))` prints `5` (the
  subscript reads as 0 inside `$(( ))`).
- **Indexed `${!a[@]}` key splat**: `for i in ${!a[@]}; do echo $i; done`
  prints blanks + a runtime "ignored null byte" warning.

## Closed 2026-09-10 (backlog prune)

Bookkeeping only — closed 2026-09-10. Successor DD#62 `dd62-test-expr-ssavalue` landed.
