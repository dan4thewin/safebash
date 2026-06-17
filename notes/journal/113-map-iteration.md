---
title: 113 — map iteration: dynamic subscript, flat-literal reject, keys taint
date: 2026-06-17
---

# 113 — map iteration fixes (dynamic subscript + flat literal + keys taint)

DG: "try to fix the map iteration bugs." The filed task
(`map-iteration-builtins-broken`) claimed keys emits garbage / values
ICEs / keys leaks a tainted map. Investigation showed **that diagnosis
was wrong**; the real bugs were elsewhere. Discussion with DG corrected
the picture before any code changed (decision authority held).

## Corrected diagnosis

- **`values` is not a builtin** (DD#41 = len/keys/join only). The
  "values ICE" was just the generic for-in-over-external-cmdsub ICE
  (`$(values m)` calls a nonexistent command). Out of scope; no `values`
  added — values are read via `${m[$k]}`.
- **`keys` was never broken.** On a correctly-built map it yields keys
  in insertion order. The "garbage" came from a *different* bug:

The three real bugs, all fixed:

### (A) Flat map literal silently mis-stored → SB4031

`map m=(a 1 b 2)` parsed as an indexed `Array` (len=4, `${m[a]}`→`a`),
not a 2-entry map, because `parse_array_literal` only sets `is_assoc`
when it sees `[k]=v`. DG chose **reject** (over bash-style pair
coercion). `validate.rs::reject_flat_map_literal` emits SB4031 for a
`-A` declaration whose initializer is a non-empty plain `Array`; empty
`map m=()` and the `[k]=v` (`AssocArray`) form are fine. (Type errors
all hardcode SB4030 in lib.rs, so this lives in the validator, which
also runs first.) Spec types.sh 33.9.

### (B) Dynamic subscript was unimplemented → now works

The parser literalised the bracket index at every site, so:
- `${m[$k]}` read → key `"$k"` → table miss → empty
- `${a[$i]}` read → `is_string_key("$i")` true → SB4030 false-reject
- `a[$i]=v` / `m[$k]=v` write → lexer splits on `$`, assignment
  detection fails → parsed as a command → `sash: bug:` runtime abort

Fixes:
- **Read** (`expr.rs`): new `subscript_index_expr` converts a `$var` /
  `${var}` index to a `SimpleExpansion` (else `Literal`), routed through
  all three subscript-construction sites (`try_subscript_arm`,
  `subscript_expr`, `try_subscript`). Everything downstream already
  handled `SimpleExpansion` correctly (`is_string_key`/`is_integer_literal`
  return false for it; transform `subscript_to_value` already expands the
  index; codegen `lower_subscript_value` already lowers it). So the read
  side was a one-helper parser fix.
- **Write** (`parser.rs`): `a[$i]=v` lexes as `Word("a[")`, `Dollar(i)`,
  `Word("]")`, `Eq`, … . `looks_like_dynamic_subscript_assign` detects
  that token shape and `parse_split_subscript_index` gathers the
  `$var`/`${var}` index + closing `]`. Low-risk: this shape previously
  produced a runtime `sash: bug:`, so no working behaviour changed.

Bare-identifier indices keep their DD#33 meaning: `${m[a]}` literal key
(works), `${a[foo]}`/`${a[i]}` string-on-indexed → SB4030 (spec 33.3,
intended — *not* bash arith). Only the `$var` expansion forms changed.
Specs types.sh 33.5–33.8, plus the full idiom 33.10
(`for k in $(keys m); do echo "$k=${m[$k]}"; done`).

### (C) `keys` of a tainted unblessed map escaped taint → SB5020

`keys m` takes a *bare name* arg, so the `$(join @a)` cmdsub-arg
recursion (journal 112) didn't see it. `check_collection_reader_taint`
now treats `keys`'s collection-name arg as a use site; `taint_for` also
runs `check_cmdsub_use_taint` on `$(…)` for-words so
`for k in $(keys m)` is covered. `len` stays exempt (cardinality only).
Spec taint.sh 60.35 (reject), 60.36 (blessed positive).

## Verification

- Native spec **414 passing / 2 failing** of 416 (only pre-existing cf.c7/cf.c8
  case-glob). 8 new tests green.
- Rust suite: identical 167-failing set vs baseline — **zero net
  regression** (stash-diff verified).
- clippy clean; complexity gate green (all touched fns < 3× mean;
  `validate_declaration` kept lean by extracting `reject_flat_map_literal`).

## Filed (pre-existing, out of scope, discovered here)

- `dynamic-map-subscript-in-test` (P2): `[[ ${m[$k]} = v ]]` does an
  integer lookup on a map — the `[[ ]]` codegen has its own
  `lower_subscript` that guesses key-type from index syntax and lacks the
  map type info the SSA path threads. Works in echo/assign/for; only the
  direct `[[ ]]` map+dynamic-key path is wrong (indexed-dynamic and all
  literal forms are fine). No spec exercised it. Repro + fix in the task.
- `subscript-append-broken` (P2): `a[i]+=v` discards the existing
  element (literal *and* dynamic; scalar `+=` is fine). Distinct
  read-modify-write path.
- `nonbrace-dynamic-subscript` (P3): `$a[$i]` (non-brace read) not
  recognised — same lexer-split root as the write side; brace form
  `${a[$i]}` works and is idiomatic.
