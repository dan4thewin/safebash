---
title: "array subscripts in arithmetic — bare `a[i]` (int-index lvalue+rvalue), `${…}` as rvalue value-import; DG design"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "READ: `${a[i]}` / `${m[k]}` / `$a[i]` / `${a[$i]}` in `(( ))`/`$(( ))` read the cell (fixes the silent-0 bug); `${…}` is a normal-parse rvalue value-import"
  - "READ+LVALUE: bare `a[i]` in arith (int index expr) — `$(( a[i]+1 ))`, `(( a[i]++ ))`, `(( a[i]=expr ))`, `(( a[i] op= expr ))`"
  - "bare `m[k]` in `(( ))` → compile error (a string key is not expressible under arith rules; use `${m[k]}` to read, `m[k]=` to write)"
  - "`${…}` is NEVER an lvalue (DG chose model B): `(( ${m[k]}++ ))` is rejected; map arith-mutation is `m[k]=$(( ${m[k]} + 1 ))`"
  - "index is a full arith expression: `a[i]`, `a[i+1]`, `a[0]`, `a[$i]`"
  - "native + c + janet (or per-mode guards + filed); spec tests pin reads, bare lvalue mutation, the map-bare error, and the map-via-statement path"
  - "no spec regression"
---

# Model (DG, 2026-08-16)

Two suspected bugs reported. Investigation + design conversation resolved them:

## BUG 1 — real bug (this task)

Array/map subscripts silently evaluate to **0** in every arithmetic context
(`$(( ${a[0]}+1 ))`→1 not 11; `(( x=${a[1]} ))`→0; `if (( ${a[0]}>5 ))`→false;
`$(( ${m[k]}+1 ))`→1). Root cause: the arith parser reads `${a[0]}` as a
variable *literally named* `"a[0]"` (it never parses the subscript) → nonexistent
var → 0. Outside `(( ))`, `${a[0]}` reads correctly.

## BUG 2 — NOT a bug (zero-value, decided)

Out-of-range array index / absent map key reads **empty/0**. Arrays and maps are
both **table-backed** (sparse), so an out-of-range index and a missing map key
are the same thing — a missing key — reading the DD#53 zero-value, uniform with
an unset variable (`int u; $(( u+1 ))`→1). **DG decided: keep zero-value, no
bounds check** (table-consistent, and missing-map-key-empty is relied upon).
Documented so it is not re-reported.

# The arithmetic-subscript model (option B)

Unifying rule DG settled on: **inside `(( ))`, a bare identifier/subscript uses
arith rules (int-valued); `${…}` is a normal-parse rvalue that *imports a value*
into arith. `${…}` NEVER denotes a location** — it means "the value of"
everywhere, no exceptions (the invariant that made model A — `${…}` as an lvalue
— too surprising).

| form | in `(( ))` | notes |
|------|-----------|-------|
| bare `a[i]` | int-array cell, **rvalue + lvalue** | index `i` is an int arith expr; `(( a[i]++ ))`, `(( a[i] += 5 ))`, `(( x = a[i] ))` |
| bare `m[k]` | **error** | string key not expressible under arith rules |
| `${a[i]}`, `${m[k]}` | **rvalue only** | `${…}` contents parsed by NORMAL expansion rules (so `m[k]` = string key); value-import |
| `$a[i]` (non-brace), `$f` | rvalue read | as today |
| bare scalar `f` | rvalue + lvalue | unchanged |

- Premise (verified): in `(( ))` there are no string literals — a bare
  undefined name reads 0, not its text — so bare names are unambiguously
  variable/array/function refs. `$` is optional for scalars (`f` ≡ `$f`); the
  same now holds for int-array cells (`a[i]` ≡ `${a[i]}` for reads).
- The lvalue is ALWAYS bare-arith (`a[i]`) or the existing statement assign
  (`a[i]=`, `m[k]=` — both already work). `${…}` lvalue is redundant AND breaks
  the invariant → rejected. Map arith-mutation: `m[k]=$(( ${m[k]} + 1 ))`
  (`${m[k]}` reads, `m[k]=` writes, `$(( ))` computes — every piece consistent).
- The array/map asymmetry (bare `a[i]` mutable in arith, maps only via statement
  form) is principled: `(( ))` is the integer world; maps are string-keyed
  (DD#33) so they're not native to bare arith — they enter only as `${…}`
  rvalue imports.

# Implementation (staged)

- **Stage 1 — reads.** arith lexer handles `[` (was SB4035) + `${a[i]}`
  normal-parse; `ArithExpr::Subscript { name, index: Box<ArithExpr> }`; transform
  → `SsaValue::Subscript`; codegen int-context read (atoi of the cell — the
  existing non-arith subscript read path). Covers BUG 1 + bare-`a[i]` read.
- **Stage 2 — bare lvalue mutation.** subscript as an arith assignment target
  (`++`/`--`/`=`/`op=`): read-modify-write the cell (table_get→atoi→arith→itoa→
  table_set). Reuses the statement subscript-assign write path.
- bare `m[k]` → type error (arith rules can't carry a string key).

May warrant a DD (the `${…}`-is-always-rvalue invariant + arith-subscript
model) — flag to DG.
