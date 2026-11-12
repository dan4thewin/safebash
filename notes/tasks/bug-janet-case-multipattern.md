title: BUG — Janet backend miscompiles multi-pattern case arms (a|b) → catch-all
status: done
priority: P1
kind: bug
depends: []
gates:
  - "[DONE] janet multi-pattern case matches ONLY the listed keys (a|b matches a or b, not everything)"
  - "[DONE] spec control_flow cf.c5b covers a multi-pattern arm with a non-matching subject reaching a later arm (native+c+janet green). Rust janet_tests is benched (spec is source of truth)."

# RESOLVED 2026-07-03

`emit_case_item` (emit_janet.rs) emitted `_` (janet catch-all) for any arm
with `patterns.len() > 1`, so `a|b` matched every subject. Fixed: a
multi-pattern arm now emits a janet `match` guard alternation binding a
fresh `_casev` — `(_casev (or (= _casev "a") (= _casev "b") …))`. A `*`
alternative still collapses the whole arm to `_`. Regression gate:
spec/control_flow.sh cf.c5b (`x=q; case … a|b|c) … *) …` → other),
green in native+c+janet.

NB: single-pattern GLOBS (`h*`) are still emitted as exact literals — a
SEPARATE pre-existing gap, filed as `bug-janet-case-glob` (cf.c6).

---

# (original) BUG — Janet backend emits multi-pattern case arms as `_` (catch-all)


## Discovery

Found 2026-06-18 while auditing `case` for the exact-match redesign.
Native and C are correct; only the **Janet backend** is wrong.

## Repro

```
$ sash -c       'x=c; case $x in a|b) echo first ;; *) echo other ;; esac' -o /tmp/t && /tmp/t
other            # native — correct
$ bash -c       'x=c; case $x in a|b) echo first ;; *) echo other ;; esac'
other            # bash   — correct
$ sash --janet -c 'x=c; case $x in a|b) echo first ;; *) echo other ;; esac' | janet
first            # janet  — WRONG (c is neither a nor b)
```

The emitted Janet is:

```
(match x
  _ (do (print "first"))   # the a|b arm → _  (matches EVERYTHING)
  _ (do (print "other"))   # the * arm  → _
)
```

The first `_` always wins, so any subject runs the first arm.

## Root cause

`emit_janet.rs::emit_case_item` (lines 263–281): only single-pattern
arms emit their literal; **any arm with `patterns.len() > 1` emits `_`**
(Janet's catch-all) instead of the alternation:

```rust
if item.patterns.len() == 1 {
    ... write the pattern ...
} else {
    write!(out, "_ ").unwrap();   // <-- multi-pattern → catch-all
}
```

(Same function also ignores `item.fallthrough`/`item.resume`, i.e.
`;&`/`;;&` ≡ `;;` in Janet — but those are being **abandoned**
per the 2026-06-18 case redesign, so that half is mooted.)

## Fix

Emit a real alternation for multi-pattern arms — Janet `match` accepts
`(@ or …)`-style guards, or emit one `match` clause per key sharing the
body. Add janet_tests coverage (a multi-key arm + a subject that must
fall to a later arm).

**Subsumed by DD#66** (`dd66-cond-form-dispatch`). The exact-match `case`
rework — `case` becomes the `[[ … ]]` dispatch form — rewrites the Janet
case emitter entirely; this repro is a **regression gate** for that work.
Under DD#66 `==` has no alternation (so `a|b` literal multi-key goes away)
and `=~` alternation is a single regex `^(a|b)$` — either way the
`patterns.len() > 1 → _` bug cannot recur. If DD#66 lands first, fix it
there; otherwise fix `emit_case_item` directly.

## References

- AGENTS.md "Bug policy" — zero tolerance
- `sash/src/emit_janet.rs:263` — emit_case_item (the `_` fallback)
- `sash/src/transform/control_flow.rs:723` — emit_multi_pattern_chain
  (the correct native behaviour: one `=glob`/exact test per key)
