title: BUG — subscript append `a[i]+=v` discards the existing element value
status: done
priority: P2
kind: bug
depends: []
gates:
  - a[i]+=v appends to the existing element (bash parity)
  - a types.sh/spec test locks indexed and map subscript append

# subscript `+=` append discards the current value

> **DONE (2026-06-20).** Native: `handle_subscript_assign`
> (transform/stmt_handlers.rs) now honours `AssignOp::Append` by setting
> the value to `Concat([${a[i]}, rhs])` (read-modify-write);
> `subscript_to_value` made `pub(super)`. Spec types.sh 33.11–33.13
> (guarded `in_modes native` — c/janet don't lower a Concat value in
> non-echo position, filed bug-c-janet-concat-in-value). Zero regression.

Found 2026-06-17 while implementing dynamic subscript (the map-iteration
work). **Pre-existing** — not introduced by that change; literal and
dynamic indices behave identically.

```
$ sash -c 'array a=(1 2 3); a[0]+=9; echo ${a[0]}'
9            # bash: 19 — append should concatenate onto the existing "1"
```

Scalar append works (`x=1; x+=9` -> `19`), so the defect is specific to
the subscript-assignment path: `AssignOp::Append` with `a.index.is_some()`
writes the RHS as if setting an empty slot instead of reading-modifying.

## Where

`validate.rs::taint_assignment` routes `a.index.is_some()` to
`reject_tainted_into_array` (taint only); the codegen subscript-assign
path (`codegen/inst_handlers.rs::handle_subscript_assign` /
`emit_c.rs::emit_subscript_assign`) needs to honour `AssignOp::Append`
by loading the current element and concatenating.

## Proposed spec (TDD)

- `array a=(1 2 3); a[0]+=9; echo ${a[0]}` -> `19`
- `array a=(x y); i=1; a[$i]+=z; echo ${a[1]}` -> `yz` (dynamic index)
- `map m=([k]=ab); m[k]+=cd; echo ${m[k]}` -> `abcd`

## Notes

Out of scope for the map-iteration fix (those specs use `=`, all green).
Append is a distinct read-modify-write path.
