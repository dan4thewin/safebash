title: "Every bulk fill accepts a `map` target and corrupts it"
status: done
priority: P1
depends:
gates:

# Every bulk fill accepts a `map` target and corrupts it

Found 2026-10-02 by DG — *"thought read only did arrays"* — while designing the
`keys m k` out-param. He is right, DD#99 says so explicitly, and the
implementation never enforced it.

## The defect

A collection-filling out-param writes **indexed-array** keys: the raw 8-byte
native encoding of `0, 1, 2, …` (DD#33 — "integer keys hashed as raw 8-byte
i64"). Given a target already declared `map`, the auto-declare is skipped and
those integer keys are written straight into the string-keyed table.

```sash
map m
read m
attest k v in m [[ -n $k && -n $v ]]
for k in $(keys m) { echo "key=[$k]" }
```

compiles clean and prints a key of **eight NUL bytes**:

```
key=[^@^@^@^@^@^@^@^@]
```

So the program ends up holding a *string* whose bytes are NULs — a direct
breach of DD#87's string-NUL invariant, reachable from ordinary safe code with
no `--no-taint` and no retired surface. It surfaces downstream as the runtime
abort `argument contains a NUL byte and would be truncated at exec` when such a
key reaches an external command, which is the invariant catching it one layer
too late.

## Affected — all four array-filling instructions

| spelling | SSA inst | target arg | result with a `map` target |
|---|---|---|---|
| `read m` | `ReadArray` | 0 | one entry, 8-NUL key |
| `read $ln m` | `SplitValue` | 1 | one entry, 8-NUL key |
| `argv m` | `ArgvFill` | 0 | one entry, 8-NUL key |
| `argv 1 m` | `ArgvFill` | 1 | silently empty (len=0) |
| `matchall "$s" "." m \|\| :` | `MatchInto` | 2 | three entries, keys 0/1/2 as raw i64 |
| `match "$s" "." m \|\| :` | `MatchInto` | 2 | same |

`mapfile` / `readarray` are **not** affected — they are retired outright
(SB5060, DD#99 stage 6), in every position.

### Related, distinct: `readline` accepts a collection

`readline` fills a **scalar**. Given either an array or a map it compiles and
silently does nothing (`len=0`). Same family of missing target-type check, but
the opposite direction, so it wants its own diagnostic.

## Why it is a bug and not a gap

DD#99 settles it in as many words:

> **`read` always produces an array.** No declaration-dependent behaviour, no
> …

and the section **"Maps are built by assignment, not by `read`"** records that
DG was offered a one-step read-into-map form and **rejected it**, as *implicit
polymorphism*. The sanctioned route is the two-step one:

```sash
read a                      # always an array
map m = @{a:-EXTRA}         # pairwise: k1 v1 k2 v2 …
```

That route works correctly, including for taint (see below). What is missing is
purely the enforcement that the one-step form stays rejected.

## It is also what made a dead diagnostic look live

DG asked whether a map's keys must be clean. Probing `read m` suggested "no" —
the map came back tainted and SB5020 fired on its keys. But `read m` is this
bug, so it was evidence about nothing.

**DG's recollection was right and it is settled.** DD#93, "The map 2-binder
form is validation-only (2026-09-12)":

> Every route to a tainted map is closed at construction … And the decisive
> one: **`attest k v in cfg` cannot clear a tainted map even if written**,
> because the map construction is rejected before the attest is reached. There
> is no program in which the 2-binder form clears taint.

Re-measured, that holds exactly. In `read a; map m = @{a:-X}; for k in $(keys
m)` the SB5020 is at **3:9** — the `@{a:-X}` construction — not the keys read.
`m[a]=$tainted` is SB5021, likewise at construction.

So **a tainted map is unconstructible, and a map's keys are always clean.**
The `keys m k` out-param therefore yields an unconditionally **clean** array.

### Consequence: `check_collection_reader_taint`'s `keys` arm is wrong whenever it can fire

With a tainted map impossible, the arm can only ever be reached by a tainted
**array** — and `keys` on an array is already a DD#75 §3 type error. Measured:

```
array a; read a; for k in $(keys a) { … }
  → SB5020  keys of tainted map 'a' without attest — attest-all it first:
            `attest k v in a [[ … ]]`
```

It calls an indexed array a *map*, and it preempts the correct complaint. Worse,
its advice is a dead end — following it gives:

```
  → SB4030  `attest k v in a` requires a map — 'a' is an indexed array —
            use one binder: `attest k in a [[ … ]]`
  → SB4030  `keys` needs a map — `a` is an indexed array — …
```

which is the same defect DG caught in SB5073 an hour earlier: one rule
prescribing a repair another rule rejects. The real complaint, SB4030, is
correct, already specced, and says the useful thing.

**Fix: delete the `keys` arm.** DD#93 already recorded that its justification
is gone ("so it is not later mistaken for dead code"). The `join` arm stays —
arrays genuinely can be tainted and `join` is legal on them.

## Fix

The target-type check belongs in `type_check.rs`, which is where declared
collection kinds live (`env.assoc_arrays` vs `env.collections`) and where the
sibling operand checks already are (`check_keys_operand`, `check_join_operand`,
`check_len_operand`, all called from `check_subscript_in_stmt`).

An *undeclared* target must stay legal — `autodeclare_bulk_target`
(`transform/stmt_handlers.rs`) creates the array, which is the documented
behaviour (DG 2026-07-08). Only an explicitly-`map`-declared target is the
error.

Diagnostic should name the two-step route, since that is the whole point of
DD#99's rejection:

> `read` fills an indexed array — `m` is a map. Read into an array and convert:
> `read a` then `map m = @{a:-EXTRA}`

## See also

- `notes/decisions/DD99-read-family.md` — "Maps are built by assignment, not by
  `read`"
- `notes/decisions/DD33-unified-hash.md` — the raw-8-byte-i64 key encoding
- `notes/decisions/DD87-string-nul-invariant.md` — the invariant breached
- `notes/decisions/DD60-argv-trio.md` — `argv [N] a`

## Done

The check landed in `type_check.rs::register_collection_target`, which turned
out to be where `check_command` already dispatches all four fills with exactly
the arities derived above (`read`, `argv`, and `match`/`matchall` at arity 3,
target always the LAST arg). Nothing new had to learn the dispatch table.

The bug was visible in that function's shape: its only guard was against a
*scalar* target. A map is in `collections` too, so a map target sailed through
that guard and `assoc_arrays` was never consulted.

`readline` was folded in as the mirror (`check_scalar_fill_target`): it fills a
scalar, and given a collection it compiled and silently discarded the line.
Silent data loss, same missing check, opposite direction.

Specs: `types.sh` 75.117-75.127 — the six map-target spellings (which really
pin the *dispatch*: which commands fill, at which arity), two positive controls
that a declared array and an undeclared name still fill, the two `readline`
rejections, and a positive control that `readline` into a declared scalar still
works.

### The `keys` taint arm

Deleted as filed. Removing it exposed a duplicate-diagnostic defect, now filed
as `duplicate-type-error-in-for-header-cmdsub.md` — the real SB4030 had been
masked by the bogus SB5020, and it reports twice. Deliberately left for DG: the
two overlapping walkers are not equivalent, so it is a structural fork rather
than a deletion.

Specs: `types.sh` 75.128-75.131, including a control that the surviving `join`
arm still fires, so deleting both at once cannot pass.
