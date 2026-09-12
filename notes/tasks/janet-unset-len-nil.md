---
title: "janet backend: $(len a) after unset crashes on nil"
status: deferred
deferred-by: DD104
priority: P3
kind: bug
depends: []
gates:
  - "janet `array a=(...); unset a; echo $(len a)` prints 0 (matches native/c), not a `(length nil)` crash"
  - "spec types.sh 30.11 un-guarded back to all three modes (drop the native,c guard)"
---

## Symptom

In the janet backend, `array a=(one two three); unset a; echo $(len a)`
crashes at runtime:

```
error: expected string, symbol, keyword, array, tuple, table, struct or
buffer, got nil
```

Native and c both print `0`. Exposed by the DD#75 §4 `${#…}` retirement
(journal 191): spec `types.sh` 30.11 switched from `${#a}` to `$(len a)`
and is now guarded `in_modes native,c`. `$(len a)` itself predates that
change — this janet gap is pre-existing, just newly surfaced.

## Cause (hypothesis)

`emit_janet.rs` lowers `arrlen`/`strlen` to `(string (length (or {} "")))`.
`unset` leaves the janet binding as `nil`, and `(or a "")` evidently does
not coerce it (the var may be a different binding form, or `unset` stores a
sentinel `(length)` rejects). Confirm what `unset` emits in janet and make
the length path treat an unset/nil collection as empty (length 0), matching
the native/c "unset clears the collection" semantics (DD#75).

## Fix

- Make the janet `arrlen`/`strlen` emit nil-safe (e.g. `(length (or a @[]))`
  for the array case, or normalize `unset` to store an empty collection).
- Re-run spec/types.sh janet; un-guard 30.11.


## Deferred to post-beta (DD#104, 2026-09-12)

The in-tree janet backend is removed; `--emit-ir` replaces it with an
inert IR capture that an out-of-tree oracle can consume. This gap is
**not fixed** — it is recorded here as part of the diagnosis a restart
should read. The code it describes lives at tag `janet-babel`.
