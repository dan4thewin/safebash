title: DD#93 — attest: declassification ergonomics (rename + shorthands + shape tests)
status: accepted

Settled with DG (2026-09-06). A bundle of ergonomic improvements to the taint
**declassification** primitive (formerly `bless`), keeping its defining
property — every declassification stays **explicit, named, and greppable**.

## Motivation

DG opened on bless ergonomics: "assigning an int var rather than using a `-i`
test could perform the same check and save a step, e.g., `bless numstr (( myint
= numstr ))`." The thread that followed (2026-09-05/06) surfaced four cohesive
changes. The unifying constraint is the one DG defended when he **rejected**
"make every arithmetic result clean" (an earlier Option A): declassification
must never become *silent*. An auditor grepping for where tainted became clean
must still find a single keyword at every site. Each change below preserves that
anchor while cutting ceremony.

DG on the keyword itself: an early reviewer "balked at the word 'bless' (not at
taint testing)." `bless` carries Perl-taint/whimsical baggage that reads as
unserious in a security primitive.

## The rule

### Part A — rename `bless` → `attest` (hard rename, no alias)

The keyword is **`attest`**. `bless` is removed entirely (pre-1.0, single
project; self-consistency beats a compatibility alias). "Attest" names the
author's speech act — *certifying a checkable property*, with the predicate as
the evidence — and sits in current security vocabulary (SLSA / sigstore /
in-toto attestations), directly answering the reviewer's objection.

Vocabulary split this resolves: the **unchecked** bypasses stay "trust"
(`--no-taint` DD#82; `:!` / `arrvals_trust` DD#91 — declassify with *no*
predicate); the **checked** primitive is "attest" (declassify *with* a
predicate). Using "trust" for the keyword would have blurred that line.

Diagnostics follow the rename ("attest condition must validate $x"). SB codes
are unchanged (SB5020 etc.).

### Part B — unary shorthand: `attest VAR OP` ≡ `attest VAR [[ OP $VAR ]]`

For any unary test operator `OP`, the bare-operator form desugars to the full
predicate with the operator applied to the attested variable:

    attest x i?            ≡  attest x [[ i? $x ]]
    attest x -n            ≡  attest x [[ -n $x ]]

It extends to the attest-all (array) form, the operator binding the element
binder:

    attest e in a i?       ≡  attest e in a [[ i? $e ]]

The two-binder **map** form applies the operator to **both** binders
(DG, 2026-09-07):

    attest k v in m i?     ≡  attest k v in m [[ i? $k && i? $v ]]

which follows from the existing rule that a map predicate must reference both
the key and the value (SB5020, `taint_attest`) — binding only one would make
the shorthand unusable for maps, and declassify a name the predicate never
checked.

Because the operator binds the attested var/binder *by construction*, the
shorthand makes a wrong-var predicate (SB5020) impossible to write — strictly
safer than the full form, and still greppable (`attest` remains). Sugar only:
it lowers to the existing `TestExpr::Unary`, so all validation/taint flows
unchanged.

### Part C — content-shape tests `i?` / `p?` / `g?`

Three sash-native unary tests that validate the **shape of a value's bytes**,
spelled with a `?` suffix to distinguish them from bash's `-X` file/emptiness
tests (whose one-letter space is saturated). They are **general test
operators**, usable anywhere a unary test is (`if`, `while`, bare `[[ ]]`) —
exactly as `-i` is today — not only inside `attest`.

| op   | true iff the value is …                                    |
|------|------------------------------------------------------------|
| `i?` | a strict, i64-representable integer (**renamed from `-i`**) |
| `p?` | non-empty and every byte is ASCII **printable** (0x20–0x7E) |
| `g?` | non-empty and every byte is ASCII **graph** (0x21–0x7E, i.e. printable non-space) |

- **`-i` → `i?` (hard rename).** `-i` was sash-native (bash has no `-i`, DD#77);
  folding it into the `?`-suffix family pulls it out of bash's `-X` namespace.
  Same strict, range-checked grammar (`parse_strict_i64`) and semantics.
- **Non-empty required.** `p?`/`g?` on the empty string are **false** (`i?` is
  already false on empty). These are one-token sanitizers: "non-empty and all
  bytes in the class."
- **ASCII-only.** High bytes (≥ 0x80) are rejected; ambiguous non-ASCII is part
  of what a shape test screens out. A UTF-8-aware variant may come later (a
  distinct spelling), not now.
- **Reserved space.** The `?`-suffix opens a clean namespace for the remaining
  POSIX classes later (`a?` alnum, `d?` digit, `l?` alpha, `s?` space, …); the
  operator token + runtime dispatch are built so adding one is a one-liner. Not
  implemented now.

### Part D — arith-assign attest: `attest numstr (( myint = numstr ))`

A new attest predicate form where an **int-assignment is the predicate**. The
conversion *is* the validation: `numstr` must be a valid i64 integer (the `i?`
grammar); if not, attest-fail **aborts** at runtime (like a failed `[[ ]]`
predicate). On success it declassifies **both** names —

- `numstr` (the source string) becomes **clean** (matching attest's clear-the-
  named-var model), and
- `myint` is **bound clean** to the i64 value of `numstr`.

Constraints (DG, 2026-09-06):
- **Both clean** (source and target).
- **Distinct names** required: `myint` ≠ `numstr` (same-name in-place retype is
  deferred) — a compile error otherwise.
- **Scalar only** for now (the array analogue `attest e in a (( … ))` is a
  follow-up; the existing `attest e in a [[ … ]]` covers arrays meanwhile).
- The **`if` form is supported** (DG, 2026-09-07): `if attest numstr
  (( myint = numstr ))` branches instead of aborting, and `myint` is
  *declared but not assigned* on the else path — it reads as `0`, which is
  what a declared-but-unassigned int reads as everywhere else in sash.
- Same-name (`attest s (( s = s ))`) is **SB5053**.

The bare `numstr` inside the `(( ))` is **exempt from SB4030** (DD#77 D1's
"bare string in arith needs a disposition") precisely because the `attest`
supplies the accountability — the whole point of the form. It is parsed as an
attest predicate, never lowered as a free arithmetic statement, so SB4030 never
applies. Two sibling rules written for `$(( … ))` *rvalues* needed the same
narrow exemption once the form existed: **SB4020** (an assignment inside
`$(( … ))` can't take effect — here it is an assignment by design, and the
transform emits the store) and **SB5031** (assigned nowhere — the attest
*introduces* the target). This *completes* DD#77's `-i`/disposition split: `i?` validates-and-
branches (keeps the string); the disposition converts inline; and now
`attest … (( n = s ))` validates-converts-declassifies in one accountable step.

## What does NOT change

- The security posture. Every declassification still requires the `attest`
  keyword; there is **no** silent path (Option A was rejected for exactly this).
- SB codes and the taint model (definition-based, DD#24). The shorthand and the
  arith form desugar/lower onto existing taint-clearing machinery.
- `--no-taint` (DD#82) and `:!` (DD#91) — the unchecked bypasses, unaffected.
- DD#77 dispositions and the strict-int grammar (`parse_strict_i64`) — `i?`
  reuses it verbatim; `p?`/`g?` add sibling `is_print`/`is_graph` validators.
- `-o` / `-v` / `-R` stay SB5028-rejected in every form (full and shorthand).
  Implementing Stage 3 exposed that this was **not** in fact true: SB5028 was
  only applied to `[[ ]]` *commands*, so an `attest` predicate carrying one of
  them (`attest x [[ -v $x ]]`) escaped validation and reached codegen, which
  has no lowering for it and panicked — a user-reachable ICE. `attest` now
  goes through the same `check_test_constructs` gate as `[[ ]]` (which also
  brings the DD#39/#63 regex-literal checks to attest predicates).

## Staging

Each stage: test-first, build, 3-mode sweep (native/c/janet), 0 native+c
regressions, commit.

1. **Rename** `bless` → `attest` (lexer keyword, all identifiers/strings,
   diagnostics, converter emit, specs, Rust tests). Mechanical; establishes the
   vocabulary for the rest.
2. **Shape tests** `i?`/`p?`/`g?`: runtime `is_print`/`is_graph` + `-i`→`i?`
   rename; parser op set + `?`-suffix recognizer; native/c/janet dispatch;
   migrate `types.sh` 77.x, add p?/g? specs. General-operator scope.
3. **Unary shorthand** `attest VAR OP` (scalar + attest-all): parser desugar
   after the binder(s); specs.
4. **Arith-assign form** `attest numstr (( myint = numstr ))`: new predicate
   form (IR/parse/validate/transform/codegen ×3), distinct-name check, both-
   clean taint clear, SB4030 exemption; specs.

See notes/tasks/dd93-attest-ergonomics.md for the live tracker.
