---
status: deferred
deferred-by: DD104
depends: []
gates: [janet]
---

# janet: `return <&&/|| list>` producer collapses to one operand

## Symptom

With the DD#74 `return <composite producer>` parser fix (native/c correct),
the janet backend mis-lowers an **and/or list** producer:

```bash
both() { return [[ 1 = 1 ]] && [[ 2 = 3 ]] }
if both; then echo yes; else echo no; fi   # native/c: no   janet: yes
```

`--janet` emits `(do (set __last_exit (if true 0 1)) (return _ret nil))` — it
captures a **single** operand of the `&&`/`||` list and drops the rest, so the
aggregate status is wrong. Both `&&` and `||` are affected; some cases pass by
coincidence (the collapsed operand happens to match).

`return <pipeline>` producers ARE correct in janet (spec 74.19/74.20 run in all
modes and pass); only the `&&`/`||` list form is broken.

## Scope

janet emits return-producers from the AST (`emit_janet.rs`), not SSA. The
return-producer path needs to fold a `List` (and/or short-circuit) into
`__last_exit` the way native/c do, instead of taking one operand.

## Gate

Specs `spec/functions.sh` 74.21/74.22/74.23 are currently gated `in_modes
native,c` with `skip_chk` for janet. Once fixed, drop the guard so they run in
all three modes.

## Related

- `notes/decisions/DD74-predicate-functions.md` (return-producer semantics)
- `notes/tasks/janet-optional-args-elision.md` (sibling janet AST-emit gap)


## Deferred to post-beta (DD#104, 2026-09-12)

The in-tree janet backend is removed; `--emit-ir` replaces it with an
inert IR capture that an out-of-tree oracle can consume. This gap is
**not fixed** — it is recorded here as part of the diagnosis a restart
should read. The code it describes lives at tag `janet-babel`.
