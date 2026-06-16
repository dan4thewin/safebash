---
title: argv/arg/argc trio — DD updates and spec test contract
status: done
priority: P0
kind: decision
depends: []
gates:
  - DD60 created (supersedes DD42)
  - DD22 addendum (quotes always yield a single string)
  - DD24 updated (rename callsites, $0 → $(arg 0))
  - DD46 updated (point at new builtins in error redirect)
  - spec/argv.sh created with av.t*/av.s*/av.n*/av.x*/av.m* cases
  - spec/control_flow.sh extended with cf.dd46.*, cf.f11..f14
  - spec/wordsplit.sh extended with ws.q*
  - spec/errors.sh extended with e.dd46.*
  - sub-decisions Q1..Q5 resolved with DG
---

# argv/arg/argc trio — DD updates and spec test contract

## Background

Session 2026-06-13 (notes/journal/TBD — write up when this lands)
locked the surface design for program-argument access in sash-native.
This task captures the contract: DDs as the design source of truth,
spec tests as the executable contract. Implementation phases T2..T7
work toward both.

The motivation was [`bug-for-in-dollar-at-taint-ice.md`](bug-for-in-dollar-at-taint-ice.md)
— ICE when iterating `$@` at top level. The investigation revealed
three latent gaps:

1. DD46 (positional params function-local) not enforced for top-level
   `$@` codegen — bypasses validator into a working-but-DD-violating
   splat path
2. DD42 `$(argv N)` shape was scalar-only; there was no clean way
   to express "iterate over program args"
3. DD22 (no word splitting) tolerated decorative quotes around array
   splats, creating an exception to "quotes yield one string"

DG-locked design fixes all three.

## Locked decisions (session 2026-06-13)

1. **C semantics across the trio.** `$(argc)` = real C argc (1 +
   user-arg count, program name always present at index 0). `$(arg 0)`
   = program name. `$(arg 1)` = first user arg.
2. **`$(arg N)`** scalar-returning, tainted, aborts on N ≥ argc.
3. **`$(argv)`** = whole array, tainted, splat-context only.
4. **`$(argv N)`** = slice from index N, tainted, splat-context only,
   yields empty array (not abort) when N ≥ argc.
5. **`array a=$(argv)`** allowed — snapshot to named tainted array
   for reuse; DD#61 array-bless rules apply (`bless x in a [[…]]`).
6. **Quotes yield a single string — no exceptions.** `"${a[@]}"`,
   `"$a[@]"`, `"$(argv)"`, `"$(argv 1)"` all rejected. Removes the
   decorative-quote-unwrap exception at
   `sash/src/transform/control_flow.rs:339`.
7. **Canonical array-splat spelling is `@a`** (= `@arr` per DD#41:
   bare, no braces, no quotes). This is the form the converter emits
   and the spec tests use. The brace/`$a[@]` forms are accepted on
   input as a parser convenience (Q1) but are never the canonical
   output.
8. **DD46 enforced at top level.** `$@`/`$*`/`$1..$N`/`$#`/`$0` at
   top level → SB-error directing to the appropriate builtin.
9. **Inside-function `$@`/`$*`/`$1..$N` stay clean** — call sites
   are visible, so the caller's args are statically known. sash-convert
   must stop rewriting inside-function forms.
10. **No taint-mixing in for-in headers.** `for x in head $(argv) tail`
    rejected. `for x in $(argv) $(argv 1)` rejected (two tainted
    sources also disallowed for the same reason). `for x in @a
    @b` (two named arrays) is a separate question — flag if it
    arises.

## DD changes

### DD60 (new): argv / arg / argc builtins

```
title: DD#60 — argv / arg / argc builtins (C semantics)
status: done
supersedes: DD42

`$(argc)` returns the program's argc following C semantics: 1 plus
the count of user-supplied arguments. The program name (argv[0]) is
always present, so `$(argc) >= 1` always holds.

`$(arg N)` returns the Nth program argument as a single string,
0-indexed:
  - $(arg 0)  → argv[0]  = program name
  - $(arg 1)  → argv[1]  = first user arg
  - $(arg N)  for N >= $(argc) aborts at runtime with a clean error

`$(argv)` returns the entire argv array, including the program name
at index 0. Equivalent to `$(argv 0)`.

`$(argv N)` returns the argv slice starting at index N (Python-style
argv[N:]). N >= $(argc) yields the empty array (zero-iteration), NOT
an abort. The slice form is the natural "iterate user args" idiom —
but see "Consumption + taint" below: iterating a tainted splat must
go through a snapshot + bless-all, not a body-bless.

`$(arg N)`, `$(argv)`, and `$(argv N)` are **tainted** (DD24): each
carries external input. `$(argc)` is **clean** — it is a count
(process metadata, like `$#`), not external data; this keeps the
converter's `$# → $(($(argc) - 1))` clean arithmetic.

## Consumption + taint

The governing rule (DD24): **assigning *from a source* is allowed (the
target becomes tainted); *reading a tainted variable* is rejected except
inside `bless`.** `getenv`/`argv`/`read`/`$(extern)` are sources; `$a`,
`${a[i]}`, `@a` are reads of a variable. For-in iteration is therefore a
**bind** or a **read** depending on the iterand — two distinct cases:

- **Direct `$(argv)` / `$(argv N)` for-in** (no named array): each
  iteration binds one argv element to the loop variable *from the
  source*. This is assignment-from-source, exactly parallel to the
  scalar `x=$(arg 1); bless x; use x` (av.t4) — so per-iteration
  body-bless is valid:

      for x in $(argv 1); do bless x [[ $x =~ … ]]; echo $x; done

  The read analog is `while read x; do bless x; done` (streaming,
  body-bless). Both are allowed because nothing reads a stored variable.

- **Named tainted array** (a snapshot, or any `read -a`/`mapfile`
  array): iterating it — `for x in @a` — *reads* its stored tainted
  elements (the `b=$a` case), which DD24/DD#61 reject. Materializing a
  stream into an array is what turns it into a variable; once stored,
  bless-all first, then iterate the clean array:

      array a=$(argv 1)              # tainted snapshot (DD#61 bulk-read)
      bless x in a [[ $x =~ … ]]     # DD#61 bless-all clears the array
      for x in @a; do echo $x; done  # @a is the canonical splat spelling

  Once blessed, the DD#41 collection builtins apply: `len a`, `keys a`,
  `join @a`. There is no direct `join $(argv)` — join operates on a
  named array, so snapshot first.

You can body-bless a read *stream* (`while read` / `for x in $(argv)`),
but not a read *array* — because the array is a stored value. (Q5,
resolved: strict source-vs-variable line.)

## Context restrictions

`$(argv)` and `$(argv N)` are array-returning substitutions. They
are legal only in:
  - for-in headers, as the sole source (no mixing with clean or
    other tainted sources — see "no-mixing" below)
  - RHS of `array a=$(argv)` / `array a=$(argv N)` (snapshot)

Any other appearance is a compile error. In particular:
  - `x=$(argv)`           rejected (scalar-context array splat)
  - `cmd $(argv)`         rejected (command-arg-position splat banned;
                                    use snapshot+iterate instead)
  - `echo "$(argv)"`      rejected (quoting an array splat per DD22)
  - `[[ -n $(argv) ]]`    rejected
  - `$(($(argv)))`        rejected

`$(arg N)` and `$(argc)` are scalar-returning, legal anywhere a
scalar substitution is, subject to DD24 taint rules.

## No-mixing in for-in headers

A for-in header must have exactly one taint-uniform source. These
are rejected:

  for x in head $(argv) tail; do ...; done    # clean + tainted
  for x in $(argv) $(argv 1); do ...; done    # two tainted sources

This is a corollary of DD24 (use-site rejection of tainted data) —
the body's bless on the loop variable can only certify uniform-source
input.

## Migration from DD42

  - $(argv N) old-scalar semantics  →  $(arg N)
  - $(argv N) new slice semantics   →  same syntax, different shape
  - $(argc) was equivalent to $#    →  now equals 1 + user-arg count
                                        (C semantics)

## See also
  - DD22 (quotes yield one string — array splats unquoted)
  - DD24 (taint quarantine — argv/arg sources are tainted)
  - DD46 (positionals function-local — top-level redirects to here)
  - DD42 (superseded)
```

### DD22 addendum

Append to existing DD22 body:

```
## Addendum 2026-06-13: quotes always yield a single string

Quoting an array splat is a compile error — no exceptions:

  "${a[@]}"   rejected
  "$a[@]"    rejected
  "$(argv)"   rejected
  "$(argv 1)" rejected

The previous tolerance for decorative quotes (the DoubleQuoted-unwrap
path in `transform/control_flow.rs::detect_array_iter`) is removed.
Quotes exist for string concatenation, interpolation, and int-to-string
conversion only. Array-context splat is a separate syntactic category
expressed by `@arr` / `$(argv)` / `$(argv N)`.

Bash-mode source uses the bash-style quoted spelling; sash-convert
coerces it to the sash-native unquoted form.
```

### DD24 update

In the "Taint sources" list, change:

```
  - `x=$(argv N)` — program positional params (DD#46)
```

to:

```
  - `x=$(arg N)`      — single program positional param (DD#60)
  - `array a=$(argv)`   — whole-argv snapshot (DD#60)
  - `array a=$(argv N)` — argv slice snapshot (DD#60)
```

(`$(argv)`/`$(argv N)` are splat-only — scalar `x=$(argv)` is rejected;
the snapshot RHS is an `array` declaration.)

In the closing line of "Clean sources":

```
Top-level positionals are program argv (tainted, use `$(argv N)`).
```

change `$(argv N)` to `$(arg N)`.

In the "$0 is not supported" line:

```
`$0` is not supported by the compiler — the converter rewrites it
to `$(argv 0)`.
```

change `$(argv 0)` to `$(arg 0)`.

Add a "Reads (rejected unless inside bless)" clarification for for-in:

```
For-in iteration is a bind or a read by iterand. Iterating a *source*
(`for x in $(argv)`, `while read x`) binds each element to the loop
var from the source — assignment-from-source, so per-iteration
body-bless is valid. Iterating a stored tainted *variable*
(`for x in @a` where `a` is a tainted named array) reads its elements
(the `b=$a` case) and is rejected; bless-all the array first (DD#61).
```

### DD46 update

Top-level position errors. Replace the existing "guidance to use
`argv N`" with explicit builtin names per form:

  - `$@` / `$*` (top-level)  → SB-error: "use `$(argv)` for iteration
    or `$(arg N)` for individual args"
  - `$1`..`$N` (top-level)   → SB-error: "use `$(arg N)`"
  - `$#` (top-level)         → SB-error: "use `$(argc)` (note: this
    is C argc, includes the program name; subtract 1 for user-arg
    count)"
  - `$0` (top-level)         → SB-error: "use `$(arg 0)`" (sash-native);
    sash-convert coerces bash-mode source

## Spec test enumeration

The complete set of tests that would cover every decision listed
above. IDs use the existing `<prefix>.<letter><num>` convention.

### New file: `spec/argv.sh`

**Trio scalar semantics**

| ID    | Script | Expected |
|-------|--------|----------|
| av.t1 | `echo $(argc)` invoked as `./bin a b c` | `4` (C argc) |
| av.t2 | `echo $(argc)` invoked as `./bin` (no args) | `1` |
| av.t3 | `x=$(arg 0); bless x [[ -n "$x" ]]; echo "$x"` invoked as `./bin` | program path / argv[0] |
| av.t4 | `x=$(arg 1); bless x; echo "$x"` invoked as `./bin alpha` | `alpha` |
| av.t5 | `x=$(arg 2); bless x; echo "$x"` invoked as `./bin alpha beta` | `beta` |
| av.t6 | `x=$(arg 5)` with argc < 5 → runtime abort with clean error | abort |
| av.t7 | `x=$(arg -1)` → compile-time error (no negative index) | SB-error |

**Slice / splat semantics**

| ID    | Script | Expected |
|-------|--------|----------|
| av.s1 | `for x in $(argv); do bless x; echo "$x"; done` invoked as `./bin a b c` | program-path\na\nb\nc |
| av.s2 | `for x in $(argv 1); do bless x; echo "$x"; done` invoked as `./bin a b c` | a\nb\nc |
| av.s3 | `for x in $(argv 99); do echo iter; done` with argc < 99 | (no output, rc=0) |
| av.s4 | `for x in $(argv 0); do bless x; echo "$x"; done` ≡ `for x in $(argv); ...` | identical to av.s1 |

**Snapshot to named array**

| ID    | Script | Expected |
|-------|--------|----------|
| av.n1 | `array a=$(argv); for x in @a; do bless x; echo "$x"; done` invoked `./bin a b c` | program-path\na\nb\nc |
| av.n2 | `array a=$(argv 1); echo $(len a)` invoked as `./bin a b c` | `3` |
| av.n3 | `array a=$(argv 1); array b=$(argv); echo $(len a) $(len b)` invoked as `./bin a b` | `2 3` (independent snapshots) |

**Taint rules**

| ID    | Script | Expected |
|-------|--------|----------|
| av.x1 | `echo $(arg 0)` | SB5020 |
| av.x2 | `echo $(arg 1)` | SB5020 |
| av.x3 | `cmd $(argv)` | SB-error (splat in command-arg position) |
| av.x4 | `x=$(arg 1); bless x [[ -n "$x" ]]; echo "$x"` | accepted, runs |
| av.x5 | `array a=$(argv); bless a [[ -n "$a[0]" ]]; echo $a[0]` | accepted, runs |
| av.x6 | `for x in $(argv); do bless x; cmd "$x"; done` | accepted, runs |
| av.x7 | `x=$(argv)` (scalar assign of array form) | SB-error |
| av.x8 | `[[ -n $(argv) ]]` | SB-error |

**Mixed-taint rejection**

| ID    | Script | Expected |
|-------|--------|----------|
| av.m1 | `for x in head $(argv) tail; do :; done` | SB-error (clean+tainted) |
| av.m2 | `for x in $(argv) $(argv 1); do :; done` | SB-error (two tainted sources) |
| av.m3 | `array a=(x y); for x in @a $(argv); do :; done` | SB-error (named array + argv) |

### Extension: `spec/control_flow.sh`

**DD46 top-level enforcement** (new test IDs)

| ID         | Script | Expected |
|------------|--------|----------|
| cf.dd46.3  | `echo "$*"` at top level | SB-error → `$(argv)` |
| cf.dd46.4  | `echo "$#"` at top level | SB-error → `$(argc)` |
| cf.dd46.5  | `echo "$0"` at top level | SB-error → `$(arg 0)` |
| cf.dd46.6  | `for x in "$@"; do :; done` at top level | SB-error (was ICE — bug 2 closure) |

**Inside-function positionals stay clean** (new test IDs)

| ID     | Script | Expected |
|--------|--------|----------|
| cf.f11 | `fn greet { echo "$@"; }; greet alpha beta gamma` | `alpha beta gamma` |
| cf.f12 | `fn each { for x in $@; do echo "$x"; done }; each a b c` | a\nb\nc |
| cf.f13 | `fn n { echo "$#"; }; n a b` | `2` |
| cf.f14 | `fn one { echo "$1"; }; one alpha` | `alpha` |

**Existing failing test (becomes bug 2 closure)**

| ID    | Action |
|-------|--------|
| cf.f4 | Rewrite from `for x in "$@"; do echo "$x"; done` (top-level, was ICE) to `for x in $(argv 1); do bless x [[ $x =~ . ]]; echo "$x"; done`. Expected: `alpha\nbeta\ngamma` when invoked as `./bin alpha beta gamma`. |

### Extension: `spec/wordsplit.sh`

**Quote-rejection (DD22 strengthening)**

| ID    | Script | Expected |
|-------|--------|----------|
| ws.q1 | `array a=(x y); for x in "${a[@]}"; do :; done` | SB-error |
| ws.q2 | `array a=(x y); for x in "$a[@]"; do :; done`  | SB-error |
| ws.q3 | `for x in "$(argv)"; do :; done` | SB-error |
| ws.q4 | `array a=(x y); for x in $a[@]; do echo "$x"; done` | x\ny (bare splat, quoted scalar) |
| ws.q5 | `array a=(x y); for x in ${a[@]}; do echo "$x"; done` | x\ny |

**Migration of existing tests** (update in place, retain IDs)

  - 28.1, 28.2 (existing): rewrite `"${arr[@]}"` → `@arr`

### Extension: `spec/errors.sh`

**Error message text snapshots for DD46 top-level forms**

| ID         | Script | Expected error text fragment |
|------------|--------|------------------------------|
| e.dd46.t1  | top-level `"$@"` | `use $(argv)` |
| e.dd46.t2  | top-level `"$1"` | `use $(arg 1)` |
| e.dd46.t3  | top-level `"$*"` | `use $(argv)` |
| e.dd46.t4  | top-level `"$#"` | `use $(argc)` |
| e.dd46.t5  | top-level `"$0"` | `use $(arg 0)` |

## Sub-decisions Q1..Q4

- **Q1**: Reject `${a[@]}` (brace form) outright in sash-native, or
  keep accepting? **`@a` (= `@arr`, DD#41) is the canonical sash-native
  spelling** and the only form the converter emits. The brace `${a[@]}`
  and `$a[@]` forms are accepted on input (purely a parser convenience;
  semantically identical) but never produced.
  - decision: accept braces/`$a[@]` on input; canonical/output is `@arr`
- **Q2**: `$(arg -1)` and negative slice (e.g., `$(argv -1)` ≡
  "last element only") — reject at compile time, or accept with
  Python-style semantics? Recommend: reject (no negative indexing
  in v1; can add later if real demand). Closes av.t7 as SB-error.
  - decision: reject negative index
- **Q3**: `$(argc) - 1` is the user-arg count idiom. Worth introducing
  `$(uargc)` shorthand to avoid `$(($(argc) - 1))` in converter
  output, or keep surface lean? Recommend: keep lean. Converter
  produces `$(($(argc) - 1))`.
  - decision: no uargc
- **Q4**: Two named-array sources in for-in (`for x in @a @b`)
  — allowed (both clean) or rejected for uniformity? Not directly
  covered by DG's "no taint-mixing" lock. Recommend: allowed (both
  clean → uniform → no taint concern); flag if found to be hard to
  implement. Independent of this trio.
  - decision: allow
- **Q5**: Direct `for x in $(argv); do bless x; … done` — is the
  loop binding "assignment-from-source" (so per-iteration body-bless
  is valid, parallel to `x=$(arg 1); bless x`), or must every tainted
  splat snapshot to a named array and use DD#61 bless-all?
  - decision: **strict source-vs-variable line.** Iterating a *source*
    (`$(argv)`, `while read`) binds from the source → body-bless valid
    (av.s1–s4, cf.f4 stand). Iterating a *stored variable* (a named
    `read -a`/`mapfile` array, `for x in @a`) reads it → DD#61 bless-all
    required first. You can body-bless a read stream, not a read array.
    (DD24 should absorb this for-in clarification.)

## References

- `notes/decisions/DD41-array-syntax.md` (`@arr` canonical spelling;
  `len`/`keys`/`join` builtins)
- `notes/decisions/DD61-array-taint-bless-all.md` (array construction
  restriction + bless-all; `array a=$(argv)` is a bulk-read source)

- `notes/decisions/DD42-getenv-argv.md` (to be superseded)
- `notes/decisions/DD22-no-word-splitting.md` (addendum)
- `notes/decisions/DD24-taint-quarantine.md` (rename callsites)
- `notes/decisions/DD46-positional-local.md` (clarify redirect)
- [`bug-for-in-dollar-at-taint-ice.md`](bug-for-in-dollar-at-taint-ice.md) (closed by T7)
- session 2026-06-13/14 (write journal entry when this lands)
