title: "BUG — janet backend: indexed assignment a[i]=v / a[i]+=v ignores the index (sets the whole variable)"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "janet: `array a=(x y z); a[1]=NEW; echo ${a[1]}` prints NEW (and the map form)"
  - "janet: `a[i]+=v` appends onto the current element"
  - "drop the in_modes native,c guard on spec types.sh 33.11-33.13 (subscript append)"

# BUG — janet indexed assignment ignores the index

## Resolution (2026-06-21)

`emit_assignment` now routes a `Some(index)` assignment to
`emit_indexed_assignment`, which emits `(put <name> <index> <value>)`.
The index is typed by collection kind — integer for an indexed array,
string key for a map — tracked via a new `EmitCtx::map_vars`
(`declare_map`, populated when a declaration's value is an
`ExprKind::AssocArray`); janet rejects a string key on an array. Append
(`+=`) concatenates onto the current element via
`(string (get <name> <index>) <value>)`.

Verified vs native: `a[1]=NEW`→NEW, `a[0]+=9`→19, `a[$i]+=z`→yz,
`m[k]+=cd`→abcd, `m[a]=V`→V. Specs types.sh 33.11-33.13 fully un-guarded
(all modes). Spec: janet 109→108 (33.8 dynamic map-key write recovered;
33.11-33.13 skipped→passing), zero regressions native/c/janet; janet rust
suite 24/11 unchanged.

NB: array *dynamic read* `${a[$i]}` is still a separate janet gap
(bug-janet-array-dynamic-subscript-read); the write side fixed here is
independent. (A pre-existing duplicate `chk 33.7` id in types.sh makes
the test_status row for janet:33.7 ambiguous — unrelated spec hygiene.)

## Symptom (janet backend only)

```
array a=(x y z); a[1]=NEW; echo ${a[1]}    # janet: 69   (native/c: NEW)
array a=(x y z); a[1]+=APP; echo ${a[1]}   # janet: 80   (native/c: yAPP)
```

The emitted janet sets the whole array variable to the scalar value and
drops the index:

```
(var a @["x" "y" "z"])
(set a "NEW")          # WRONG — should be (put a 1 "NEW")
(print (get a 1))      # (get "NEW" 1) -> 69 ('E')
```

## Root cause

`emit_janet.rs::emit_assignment` uses only `a.name`, `a.value` (and
int-ness) and ignores `Assignment.index: Option<Expr>` and
`Assignment.op` (AssignOp::Append). So `a[i]=v` and `a[i]+=v` both emit a
plain `(set a v)`.

## Fix direction

When `index` is `Some`, emit `(put <name> <index> <value>)`; for
`op == Append`, value becomes `(string (get <name> <index>) <value>)`.
The index type matters: janet arrays use **integer** indices, maps use
**string** keys — so the emitter must know whether `name` is an indexed
array or a map (ctx type info) to emit the index as a number vs a string,
mirroring native's `is_integer_key`. (Indexed-array reads already work in
janet for the map case but the array case has a related gap — see
bug-janet-array-dynamic-subscript-read.)

## Discovery

2026-06-21 fixing bug-c-janet-concat-in-value (the c Concat-in-value gap).
The concat itself works in janet; the subscript-append spec rows
(types.sh 33.11-33.13) stay janet-excluded because of THIS separate
indexed-assignment gap. c was un-guarded (now native,c).
