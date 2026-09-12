---
title: "janet: join intrinsic gaps — bare-name join a, and separator join @a SEP"
status: deferred
deferred-by: DD104
priority: P3
kind: bug
depends: []
effort: effort-janet-backend-parity
gates:
  - "janet `$(join a)` (bare-name array) space-joins, instead of exec'ing /usr/bin/join"
  - "janet `$(join @a SEP)` / `$(join a SEP)` joins with SEP, instead of erroring (`bad slot, expected string, got array`)"
  - "un-guard spec/types.sh 33.19–33.21 to include janet once both work"
---

# janet join intrinsic gaps

Surfaced 2026-08-11 while implementing DD#75 join (`join arr SEP`, array-first).
janet's join only handles the `@a` **space**-join (`$(join @a)` → `(join a " ")`,
works). Two gaps:

- **bare-name `join a`** → janet emits an external-command exec (`join a`),
  running `/usr/bin/join` instead of the intrinsic. native/c resolve bare `a`
  to the array; janet's emit doesn't.
- **separator `join @a SEP`** → janet emits a call that passes the array to a
  janet builtin expecting a string → `error: bad slot #1, expected string, got
  <array>`. The separator isn't wired through janet's join emission.

Guarded `spec/types.sh` 33.19–33.21 to `native,c` meanwhile. The arg-order and
the native non-array reject (SB4030) are backend-independent and already land.

## Addendum (2026-08-14): keys intrinsic also missing in janet

Same class: janet execs `$(keys m)` as an external command (`("keys" "m"):
No such file or directory`) instead of emitting the map-keys intrinsic.
Pre-existing (cf. `janet:33.10`). Surfaced again by DD#75 §3 spec 75.7
(guarded to native,c). Gate: janet `for k in $(keys m)` iterates the map's
string keys. Fold into the janet collection-intrinsics work.

## Addendum (2026-08-14): `len` in arith-operand position missing in janet

`emit_janet` lowers a `len`/`arrlen`/`strlen` arith operand to `(len …)`, but
janet's builtin is `length` — so `$(( (len a) + 1 ))` and `$(( ${#a} + 1 ))`
emit an undefined `len` symbol (janet compile error). native/c fine. Guarded
`spec/arithmetic.sh` 58.10–58.16 to native,c. Gate: janet evaluates the len
intrinsic (array/map count, string byte length) as an arith operand — emit
`(length …)` or a table-len helper. Same collection-intrinsic class as join/keys.

## Addendum (2026-09-10): `len $x` / `len @x` in command form missing in janet

Surfaced by the SB5062 control spec (`types.sh` 75.38). The three addenda above
cover `len` in *arith-operand* position; the `$(len …)` **command** form has the
same class of gap for the sigil spellings:

- `$(len $s)`, `$(len $a)` → janet emits the operand verbatim as a symbol,
  giving `compile error: unknown symbol $s`.
- `$(len @a)` → same, as `unknown symbol ?`.

Bare `$(len a)` is fine. native/c accept all four. Guarded 75.38 to native,c.

Root cause is the one this whole file circles: `emit_janet` renders from the
**AST**, so it never sees the transform's operand resolution that native and c
inherit from SSA — cf. `emit-janet-from-ssa`. Fixing that retires the class
rather than patching spelling-by-spelling; DG deferred the janet overhaul
(including the `/usr/bin/join` fallthrough guard) to a single later pass.

## Addendum (2026-09-11) — `args` is unbound in janet, so *every* user
## function argument is broken

Same root cause, wider blast radius than the `len`/`join`/`keys` operands
above. Janet emits a user function as **zero-arity** and then calls it with
arguments:

```
(defn f [] (label _ret (print (get args 0)) ...))
(f "nums")
```

`args` is never bound, so `janet` reports `compile error: unknown symbol
args`. This is not sigil-specific — `f nums` and `f =nums` fail identically;
any `${args[N]}` in a called function does. native/c are correct.

Found while pinning the `=`-leading-word fix (spec `86.23`, guarded
native,c). Note this is the *function-call* surface, not an intrinsic, so
`emit-janet-from-ssa` has to cover parameter binding too — worth checking
that the overhaul's scope includes it.


## Deferred to post-beta (DD#104, 2026-09-12)

The in-tree janet backend is removed; `--emit-ir` replaces it with an
inert IR capture that an out-of-tree oracle can consume. This gap is
**not fixed** — it is recorded here as part of the diagnosis a restart
should read. The code it describes lives at tag `janet-babel`.
