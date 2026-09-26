# 412 — the type that was only half known

*2026-09-25*

## The claim I made, and why it was wrong

Last session I reported a "bug against DD#80": `str s=abc; ${a[$s]}` silently
indexes 0, while the bareword `${a[s]}`, the quoted `${a["abc"]}` and the
arithmetic `(( a[s] ))` all reject. Same array, same variable, three spellings,
two answers. I cited DD#80's See-also:

> A string *key* for an int-array index is the same crossing, here rejected at
> the braced surface (SB4030) rather than coerced.

DG approved the fix. The fix was right; **the framing was an over-read**, and
the measurement is what showed it.

DD#80 uses "string key" as a term of art. Its type-aware section defines it
structurally:

> A clean bareword or a quoted string is a *string* key and does not index an
> int array.

Two forms, both *syntactic*. A `$var` is a different enumerated form (case 2),
listed with no type proviso at all. So "string key" never meant "a variable
whose type is string" — I had generalised a structural term into a type one.

DD#80 is not silent on the case, though. Its SB4030 bullet reads:

> a string key (clean bareword, quoted, **or a non-numeric expansion result**)

"a non-numeric expansion result" is a *runtime* property of a `$var`. That is
the tension: the enumeration permits `$var` unconditionally, and the diagnostic
bullet wants non-numeric results rejected. The implementation had resolved it
conservatively, with a comment that turns out to be the whole story:

```rust
ExprKind::SimpleExpansion(_) => false, // variable — could be either
```

## What the aggressive reading cost

I implemented "reject when the index variable is string-typed" — `env` is in
scope, `var_type` answers, done. The sweep came back with **8 specs failing in
both modes** (16 of 6386). Not a rounding error: a cohort.

Every one was an *undeclared* variable holding an integer:

| spec | program | why it broke |
|---|---|---|
| 33.5 / 33.7 / 33.12 / 33.14 / 33.41 / 75.16 | `i=1; ${a[$i]}` | bare assignment |
| 60.70 | `i=$(printf 1); attest i i?` | DD#93's *documented way through* |
| 80.31 | `${arr[$1]}` | a positional; comment says "Bash agrees" |

The mechanism is one line:

```rust
self.vars.get(name).copied().unwrap_or(VarType::String)
```

**`var_type` cannot distinguish "declared a string" from "never recorded".**
Both are `String`. And a bare `i=1` genuinely *is* recorded `String`, because
sash follows bash in typing a bare numeric literal as a string — only `int` or
`$(( ))` makes a value int (`expr_type`'s own comment says so).

So "the type *is* known here", which is what I wrote in the fix's doc comment,
was false. The type is known in exactly one direction. `Int` is a fact;
`String` is a fact *or* an absence, and nothing in `TypeEnv` told them apart.

The two DD-blessed casualties are the ones that settle it. `attest i i?` is
DD#93's sanctioned route for a tainted index — 60.70's comment calls it "the
documented way through" — and `i?` is the *integer* predicate, so the value is
known to be an integer at exactly the moment the aggressive reading rejects it.
A reading that rejects the idiom a DD prescribes is the wrong reading.

## What went in

A third state, but only the one that carries information:

```rust
/// Scalars the author explicitly declared `str`. Distinct from
/// `vars[name] == String`, which is also what an unrecorded name returns.
declared_str: HashSet<String>,
```

`is_string_key` consults `is_declared_str` rather than `var_type`. The rule:
**reject a positively-declared string, stay silent where the type is genuinely
unknown.** That is the original comment's intent, kept — "could be either" is
still honoured for the cases where it is true, and dropped for the one case
where it is not.

`type_check` and `infer_types` build *separate* envs (the former populates
incrementally through `check_declaration`; the latter never runs in
`type_check` at all — I lost a build cycle to that), so the condition lives in
one `note_declared_str` method both call rather than being restated twice.

Measured, on both surfaces and both backends:

| | before | after |
|---|---|---|
| `str s=abc; ${a[$s]}` | `10` (index 0) | SB4030 |
| `str s=abc; ${a[${s}]}` | `10` | SB4030 |
| `str s=abc; a[$s]=99` | wrote index 0 | SB4030 (write-specific text) |
| `str s=abc; a[$s]+=z` | appended index 0 | SB4030 |
| `str s=abc; $a[$s]` | `x` | SB4030 |
| `array a=(x y z)` (string array) | index 0 | SB4030 |
| `i=1; ${a[$i]}` | `y` | `y` |
| `attest i i?` | `y` | `y` |
| `${arr[$1]}` | `r` | `r` |
| `${a[${s:-2}]}` | `30` | `30` |
| `int i=1; ${a[$i]}` | `20` | `20` |
| `map m; str s; ${m[$s]}` | `1` | `1` |

The write path needed no separate work: `check_assignment` and
`check_subscript_types` both funnel through `check_subscript_key_kind`, so
read, write and append closed together. That is DG's "no string subscript for
arrays", enforced once.

Sweep: **6402, zero failing, zero regressions** (6386 + 8 new specs × 2 modes,
predicted before running).

## The message

The old SB4030 text recommended `${a[$i]}`. For someone who just wrote
`${a[$s]}` that is the *same spelling* — it sends them from one SB4030 to the
same SB4030, because the spelling was never the problem. `conversion_hint`
learned this already; the subscript message hadn't. A variable index now gets:

> `$s` is string-typed, and an int-indexed array is indexed by number (DD#80).
> A string→int crossing must carry a disposition: `${a[${s:-0}]}` (0 if it
> isn't an integer) or `${a[${s:?msg}]}` (abort if it isn't); declare `map a`
> for string keys

Both spellings work today, braced and brace-less (`${a[$s:-2]}`), and 80.46
pins the disposition route with a `:-2` default — a `:-0` default would print
`10` whether the fix worked or not, since a lenient atoi lands on index 0 too.

## The map side is not a mirror bug

`${m[0]}` rejects but `int i=0; ${m[$i]}` does not — the same
literal-only shape as the array fault. I nearly "fixed" it. DD#80 says not to:

> **map** `${m[K]}` — `K` may be a clean bareword, a quoted string, a `$var`,
> or an expansion. **Any string is a valid key.**

The only map rejection DD#80 names is a bare integer *literal*, and DD#33's
reason is that it "reads like an array index" — an **anti-ambiguity** rule, not
a type rule. `$i` carries no such ambiguity. The asymmetry is deliberate:
arrays are typed by their index, maps accept any string. Checking
`notes/decisions/` first is what stopped that one.

## The fix that pointed into a wall

Read and write share `check_subscript_key_kind`, so `a[$s]=99` started
rejecting for free. I wrote in the follow-up task that the accountable spelling
"already parsed on both sides" — and then, before committing, checked. It does
not:

| spelling | result |
|---|---|
| `a[${s:?m}]=Q` | **SB4014** |
| `a[${s:-2}]=Q` | **SB4014** |
| `a[$s:-2]=Q` | **SB5011** |
| `a[$((1+1))]=Q` | **SB5011** |

DD#33's `parse_split_subscript_index` accepts a bare `$var`/`${var}` index and
nothing else. So the shared check gave the write path a diagnostic whose advice
**cannot compile** — SB4030 telling the author to write `a[${s:?msg}]=…`, which
is SB4014. A rejection is only as good as its exit, and I had just built one
that led into a wall.

Hence `KeySurface`. The read message keeps the disposition route; the write
message names what actually parses — an integer literal, an `int` variable, or
converting first:

```sh
int i=$(( ${s:-2} )); a[$i]=99
```

80.49 asserts the write text is surface-specific, 80.50 that the route it names
compiles. Making the two surfaces symmetric means lifting the DD#33 LHS
restriction, which is filed, not done.

The general lesson is the one this session keeps producing: **the claim and the
measurement are different things.** I asserted the spelling parsed because it
parsed on the *read* side and the check was shared. Two of the three findings
here came from checking a sentence I had already written.

## 80.47

The backstop for the silence. `i=1; ${a[$i]}` must keep working, and the spec
says why in the comment — not "this passes" but "this is deliberately not
rejected, and here is the conflation that makes it undecidable". Neutering
`is_declared_str` fails exactly 80.41–80.45 and leaves 46/47/48 green, so the
cohort is non-vacuous in both directions.

## What is still uncaught

`str s=abc` is the only index the compiler can *prove* is a string. These still
index 0 silently, and no declaration exists to catch them:

- `s=abc; ${a[$s]}` — bare assignment of a non-numeric literal
- `s=$(some-cmd); ${a[$s]}` — a capture
- `${arr[$1]}` with a non-numeric argument

The first two are decidable by value-flow (`expr_type` already sees the
initializer) and are filed. The third is genuinely runtime. Whether sash should
require `int` for an array index at all — making the index surface fully
declared, and typing `attest i i?` as int so the idiom survives — is a language
question for DG, not something to settle inside a type-check helper.
