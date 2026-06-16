---
title: Array taint construction restriction + bless-all (DD#61)
date: 2026-06-15
---

# Array taint construction restriction + bless-all

DG reported that this compiled and ran when it shouldn't:

```bash
array a=$(argv); bless a [[ -n $a[0] ]]; echo $a[0]
```

and the sharper repro:

```bash
array a=( $(getenv HOME) $(getenv PATH) )
bless a [[ $a[0] =~ ^\S*$ ]]
echo $a[1]
```

## The bug — two blind spots

1. **CST/IR taint pass** (`validate.rs`): `is_taint_source` and
   `expr_uses_tainted` didn't recurse into `Array` / `AssocArray` /
   `Concat` / command-substitution children, so a tainted element in an
   array literal never tainted the array. `+=( … )` append leaked the
   same way. (`a[i]=$(getenv X)` already tainted — its RHS is a bare
   cmdsub `is_taint_source` caught.)
2. **SSA taint backstop** (`ssa_taint.rs`): the `Subscript` use-check
   inspected only the index, never the array name; and `value_is_tainted`
   classified `Subscript`/`TableValue`/`TableKey` as always-clean.

## The design (DD#61, locked with DG over a long discussion)

- **Restrict construction.** A tainted value may not enter an array via
  program syntax — literal, `+=` append, or `a[i]=` subscript are all
  rejected (SB5021). Tainted arrays come only from bulk external reads
  (`read -a`, `mapfile`, planned `array a=$(argv)`). This makes every
  tainted array dynamic-arity and homogeneous, so **whole-array taint is
  exact**, not a compromise — DD#24's "each element carries its own
  taint" stays literally true (all elements tainted together).
- **bless-all syntax (Go/for-style, DG's pick).** `bless x in a [[ $x … ]]`
  for indexed arrays; `bless k v in m [[ $k … && $v … ]]` for maps
  (both key and value must be validated — a key escaping unvalidated is
  unsound). Cross-language intuition: Go/Rust/Python all use an explicit
  per-element binder for "∀ element". Scalar `bless` on a collection is
  a type error.
- **Semantics.** Statement form: a failing element exits 1. `if` form:
  false branch, collection stays tainted. Empty collection: vacuous pass.
  Binder is fresh, test-scoped, shadows any outer same-named var.

## Implementation

Pipeline: parser (`bless BINDER… in ARRAY`) → `classify_bless` in
`ir.rs` (arity-keyed: 1 lead = scalar, 2 = indexed, 3 = map) → transform
emits `BlessAll` (loop, sets test-result var) + `BlessRetag` (copies the
clean version) → codegen `emit_bless_all` builds the Cranelift loop over
the table (binds element via a shadowing slot versioned above any
existing var), `emit_bless_retag` copies the table pointer to the clean
version. Validation: `taint_bless` clears the array name + enforces the
binder-on-LHS rule; `type_check::check_bless_form` rejects form/kind
mismatches.

## Two coupled pre-existing bugs found & fixed

- **`mapfile`/`read -a` `< file` silently dropped the redirect.**
  `attach_redirects_to_last_inst` handled Command/ReadLine/Pipeline but
  not Mapfile/ReadArray (no redirect field). Fixed by routing bulk-read
  commands through the existing `__redirect_push/pop` bracket path —
  no SSA/codegen/backend change. Scalar `read < file` already worked;
  this aligns mapfile with it.
- **`map m=()` created an indexed table.** `()` parses as an empty
  *indexed* `Array` regardless of declaration, so only bare `map m` hit
  the assoc path. `compute_declaration_value` now steers an empty `()`
  init to the declared kind.

## Cross-mode handling

`validate` + `type_check` now run for **all** backends (moved the
type-check error gate ahead of `maybe_emit_alt_output` in `cli_main`),
so the construction restriction (SB5021) and bless-form errors (SB4030)
fire identically under native, `--janet`, and `--emit-c`. The bless-all
**runtime** tests are guarded `in_modes native` + `skip_chk`: the janet
(IR) and c (SSA) oracles pre-existingly elide `bless` and don't support
`mapfile` (existing scalar-bless tests 24.9–24.13 already diverge there).

## Test migrations

Scalar-bless-on-collection was the old (unsound) idiom; DD#61 makes it a
type error. Migrated the existing users to bless-all: `misc.sh` 54.1
(map config) and `wordsplit.sh` 22.6/22.8/22.9/22.10 (mapfile arrays/maps).

## Status

- 31 new spec tests (`taint.sh` §DD#61, `60.*`): 31 pass native; 17 pass
  / 14 skip in janet & c. Zero new failures in any mode.
- Rust suites: no regression vs baseline; taint_spec_tests +2.
- DD#61 created; the argv-trio plan's `array a=$(argv)` reuses these
  array-bless rules.
