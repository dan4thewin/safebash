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

### Addendum — wider RHS, and predicate reads (DG, 2026-09-07)

DG hit `attest x (( y = x*2 ))` and it ICE'd. Two rules came out of it.

**The RHS may be any expression that REFERENCES the attested name.** Computing
a wider RHS still requires the conversion to succeed, so the `i?` gate proves
exactly what it proves for the plain `(( t = s ))`; the target simply binds the
computed value instead of the bare conversion:

    attest n (( bytes = n * 1024 ))

An RHS that never mentions the attested name proves nothing about it and is
**SB5054**, as is a `(( … ))` that is not an assignment at all.

**A predicate may not read another TAINTED name.** The attested name is tainted
by definition — vouching for it is the point — but every *other* read must be
clean. This matters most here because an arith predicate yields a **value**
that is bound to the clean target, so an unchecked read would launder tainted
data outright:

    z=$(getenv PATH)
    attest x (( y = 0*x + z ))    # SB5020 — z would be laundered into clean y

DG extended the same rule to the `[[ ]]` forms (2026-09-07), where only a
boolean escapes and the leak is bounded to influence over the gate: *even one
bit of influence over a declassification should not be free.* Previously
`attest x [[ $x == $z ]]` with a tainted `z` was accepted.

The SB4030 exemption narrows accordingly — it covers the attested name, not
every bare string in the predicate.

Constraints (DG, 2026-09-06):
- **Both clean** (source and target).
- **Distinct names** required: `myint` ≠ `numstr` (same-name in-place retype is
  deferred) — **SB5053** otherwise.
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

## Considered and not pursued (2026-09-12)

Four extensions were noted while the four stages landed and carried on the
tracker as follow-ups. DG closed all four rather than hold them as backlog:

- **A UTF-8-aware shape test.** Would need its own spelling, and the existing
  shape tests are deliberately byte-oriented ASCII classes.
- **The remaining POSIX-class shape tests** (`a?` alnum, `d?` digit, `l?`
  alpha, `s?` space). Mechanical — the `?`-suffix namespace is reserved in
  `is_unary_test_op` and each class is one `all_bytes_match` predicate plus a
  wrapper and three dispatch arms — but not wanted absent a real use.
- **An array analogue of the arith-assign form**, `attest e in a (( … ))`.
- **Same-name in-place retype**, `attest s (( s = s ))`. S4 deliberately
  requires distinct names (SB5053); this would relax that.

The reserved `?` namespace and the `all_bytes_match` template stay in place,
so any of these remains cheap if a use appears. Nothing is filed.

## The map 2-binder form is validation-only (2026-09-12)

`attest k v in m [[ … ]]` **no longer has a taint role**. DG kept the form
(2026-09-10) on its own merits; this records why its original justification
is gone, so it is not later mistaken for dead code.

Every route to a tainted map is closed at construction, measured against the
release binary:

| route | result |
|---|---|
| `cfg[b] = $(getenv X)` — tainted value into a clean map | SB5021 |
| `map cfg = @{arr:~}` from a tainted `read` array | SB5020 |
| `matchall` on an unattested tainted scalar, then `@{p:~}` | SB5020 |

And the decisive one: **`attest k v in cfg` cannot clear a tainted map even
if written**, because the map construction is rejected before the attest is
reached — the same SB5020. There is no program in which the 2-binder form
clears taint.

It remains a live **runtime gate** on a clean map:

```sh
attest k v in cfg [[ $k =~ ^[a-z]+$ && $v =~ ^[0-9]+$ ]]
```

An entry failing the predicate aborts with `attest: cfg failed the gate`.
That is worth keeping because **`attest` is the only assert-with-diagnostic
in the language**: DD#70 makes a bare `[[ ]]` a must-consume compile error
(SB4018), and `[[ … ]] || exit 1` aborts silently. Removing the map form
would delete the only way to validate a map's contents with a message, for
no safety gain.

For **scalars and arrays** the taint role is still live. The asymmetry is
deliberate.

### Naming note

"attest" reads as vouching for untrusted data, which is precisely what the
map form no longer does. No rename is proposed — recorded so the mismatch is
understood rather than rediscovered.

## Addendum (2026-09-26) — Part D's target is an int, and now the compiler knows it

Part D says the target is "**bound clean** to the i64 value of `numstr`". The
*clean* half was implemented. The **int** half was not: nothing ever entered
the target in the type environment at all.

`--emit-ir` shows why. A plain `(( n = s ))` lowers to an `:assignment`, so
`check_assignment` declares the target. Wrapped in `attest`, the identical
text stays a `:command` named `attest` whose argument holds the
`:arith-assign` — and no pass declared `n`. It therefore fell to
`TypeEnv::var_type`'s `unwrap_or(String)` default:

```sh
i=$(printf 5)
attest i (( n = i ))
echo $(strlen $n)     # ACCEPTED — a string intrinsic on an int
int q=$n              # REJECTED — SB4030, though n is an int
```

Both answers are backwards, and the first is a **DD#105 hole in the one
construct whose entire job is to make the string↔int crossing accountable**.
An author who did the conversion properly got a target the compiler believed
was a string.

Fixed in `761b3f4`: `check_attest_form` declares `AttestPredicate::ArithConvert`'s
target as `VarType::Int`. No rule in Part D changes — this addendum records
that the stated rule is now enforced, and that it was not before.

The `if` form and the `[[ ]]` predicates are unaffected; they bind no value.

Spec `60.72`. Journal 414.

## Addendum (2026-10-31): a predicate's `&&` and `||` short-circuit

A predicate's junctions are lazy like any other `[[ ]]`'s (DD#95
addendum; journal 550, finding B88a). A decided left operand skips the
right one. So `attest x [[ $x == ok || ${y:?} == z ]]` passes without
evaluating `${y:?}`, and a failing left operand of `&&` fails the gate
before its right operand runs. That holds for the statement form and for
the caught `if attest` form.

Specs: atlazy.2, .3, .4 and .7. The attest-all ids (atlazy.5/.6, B88d)
and C's attest-all failure text (atlazy.8/.9, B91) land with F10.
atlazy.1 does too, because its `$(( ))` operand raises a separate F10
question.

## Addendum (2026-10-31): two reads the predicate-reads rule missed

The 2026-09-07 rule says every read in a predicate other than the attested
name must be clean. Two reads escaped it (findings B89 and B36):

- **An inline taint source.** The check returned early whenever no *other
  name* was tainted, so a predicate that read a source directly was never
  examined. `attest x [[ $x == "$(getenv WANT)" ]]` and
  `[[ $x == "$(<allow)" ]]` compiled, in the statement form and the `if
  attest` form alike, and the gate compared the attacker's input against
  data nobody had vouched for. There is no early return now: each is
  SB5020 at the source.
- **A Part D value's `${…}` import.** Only the bare names in the value were
  checked, so `attest s (( n = ${t:-0} + s ))` with a tainted `t` passed
  the front end. Only the SSA backstop caught it, naming an SSA version and
  no source location. The import is now a read at its own site, SB5020 like
  any other.

Neither changes a rule; both record that the stated rule is now enforced.
Since the same date the inline sources are also SB5098, because a predicate
runs no code (DD#24's 2026-10-31 addendum), so both diagnostics are
reported. A Part D value's own operands keep their reading: they are bare
names, reported at the attest.

Specs taint.sh atpred.13–16. atpred.13–15 assert the SB5020, and SB5098 is
reported beside it. Journal 552.

The F8 addendum above said atlazy.1 would land with F10. It will not: its
predicate holds `$(( 1 / z ))`, which is SB5098 now (spec atpred.17).

## Addendum (2026-10-31): a caught attest evaluates its predicate once

A caught scalar attest is `if attest x [[ … ]] { … }`, or `attest x [[ … ]] ||
cmd`, which lowers through the same path. It tested the predicate in its
condition, and then its pass branch ran the statement form's gate on the same
value: the predicate a second time, through the aborting check (batch-531
B88c). A predicate that reads the outside world can answer differently the
second time. A file test stat'd its file twice, and when the second stat
failed, the caught form aborted at the statement form's gate instead of
choosing a branch.

The pass branch now only retags the value clean, so the predicate runs once.
The Part D convert form and the collection `if` form already only retagged.
All three now share one retag helper.

Specs taint.sh atonce.1 (the `if` form) and atonce.2 (the `||` form). strace
fails the tested file's second `newfstatat`. Each spec asserts that the pass
path ran and that the file was stat'd exactly once.

**C's attest-all text (batch-531 B91).** C's statement form of attest-all
printed a value clause, `attest: a = '123' failed the gate`, where native
prints `attest: a failed the gate`. The map section above already gives
native's text ("aborts with `attest: cfg failed the gate`"), and it is the
right one. The gate belongs to the collection, no single value failed it, and
for a map the key may be what failed. C now prints the same text. The runtime
needed no change, because it already omits the clause when no value is
passed. Specs atlazy.8 (an array) and atlazy.9 (a map).

The F8 addendum said atlazy.5/.6/.8/.9 would land with F10, and they have.
atlazy.5/.6 pin a collection rule, so they are recorded in DD#61's 2026-10-31
addendum. Journal 553.
