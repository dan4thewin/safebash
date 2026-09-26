# 414 — the key is a type, not a spelling

DG's directive, once the LHS could say every index the read side could
(journal 413): **enforce the strictest forms.** An array index must be
int-typed; a map key must be string-typed; and both rules apply to
assignment and read alike, because after 413 the two surfaces parse the
same grammar and a rule enforced on one is enforceable on the other.

Two things came out of the work that were not in the directive. One is a
generalisation of the map rule, which I took on DG's stated principle
rather than asking again. The other is a live bug in `attest`, found
while looking for a spec's migration spelling.

## The predicate was asking the wrong question

The rule that stood there asked "is this a **string** key?" — a negative
test, `is_string_key`, that could only name the forms it happened to know
were strings and accepted everything else by default. That is not a
fixable list. `TypeEnv::var_type` ends in `unwrap_or(VarType::String)`,
so **"declared a string" and "never recorded at all" are the same
answer**, and no negative test can separate them. The visible symptom was
that a bare `i=1` indexed an array while `str s=abc` did not, though
neither is int-typed.

Asking for `Int` is a fact rather than an absence, so the ambiguity stops
mattering:

```rust
fn is_int_index(expr: &Expr, env: &TypeEnv) -> bool {
    is_integer_literal(expr) || expr_type(expr, env) == VarType::Int
}
```

I first wrote this as a hand-rolled whitelist — literal, `$var`,
`$(( … ))` — and the sweep found 13 failures per mode. The decisive one
was **80.12**, `${a[${a[0]}]}` on an `int array`: a cell of an int array
is int-typed by DD#76, and a whitelist that enumerates spellings loses
it. Delegating to `expr_type` inherits every rule already written there —
`Arithmetic => Int`, DD#76's cell rule, and
`parameter_expansion_type`'s rule that a `:-`/`:?` disposition keeps its
**subject's** type. That last one is exactly DG's "a bare disposition is
not the conversion": `${a[${s:-2}]}` on a string `s` is rejected,
`${a[${i:-0}]}` on an int `i` is not, and neither case had to be written
down here.

Only the bare integer literal needs saying, because `expr_type` types
`Literal("2")` as String — bash does, and nothing else in the language
wants `2` to be an int by itself.

## The map rule, generalised past the table

The approved table said: a map rejects a bare **int variable** as a key,
and quoting it is the conversion. Implementing exactly that left a
residual I found by probing — `${m[$((1+1))]}` is int-typed and went
through in silence while `${m[$i]}` was told to quote. Same type, same
crossing, opposite answer.

DG's principle here is DD#105: int→string does not happen silently. That
settles every int-typed key, not the one spelling the table named. I
checked first that the remedy generalises — `m["$(( 1+1 ))"]`,
`m["$i"]`, `m["${n[0]}"]` all compile, on both surfaces — so the rule
leaves no form without a way to say what it means. Then I applied it to
all its consequences, with a generic "quote the expansion" message for
the cases where there is no variable to name.

**This is wider than the approved table and DG should veto it if it is
not what he meant.** The reasoning is the decision-tree's step 1: a
stated principle applies to all its consequences, and narrowing it back
to the one asked-about case would have been the postmortem's failure
mode.

## The ICE closed itself

The subscript int sink was the one place lacking a DD#77 D4
default-literal check, and `${a[${s:-hello}]}` panicked at
`codegen/values.rs:1432`. The strict rule rejects every bare disposition
on a string subject, so that sink is now unreachable — all ten rows of
the ICE table reject cleanly and none panics. I checked whether
`check_int_value_defaults` was owed a widening and concluded it is not:
inside `$(( … ))` the arithmetic checks already apply, and DD#72
literals parse correctly in every arithmetic sink (`0x11`→17,
`0o17`→15, `0b11`→3).

(Noted in passing, not investigated: `int n=${s:-17}` is SB4030 even
with a decimal default, because the *subject* is str-typed. That is the
same rule, and it may or may not be what we want there.)

## The bug: `attest` never declared its target

Looking for 60.70's migration spelling I wrote `attest i (( n = i ))` and
then, out of habit, `$(strlen $n)`. It was **accepted**. So was the
inverse failure: `int q=$n` was rejected.

`--emit-ir` gave the discriminator immediately. A plain `(( n = 2 ))`
lowers to an `:assignment`, so `check_assignment` declares the target.
Wrapped in `attest`, the same text stays a `:command` named `attest`
whose argument holds the `:arith-assign` — and **nothing ever entered
`n` in the type environment**. It defaulted to String, which let a
string intrinsic run on an int: the silent int→string render DD#105
exists to reject, reachable through the one construct whose whole job is
to make the conversion accountable. DD#93 Part D calls the target "the
int name bound clean"; the compiler did not know that.

One line, in `check_attest_form` (which now takes `&mut TypeEnv`):

```rust
if let AttestPredicate::ArithConvert { target, .. } = pred {
    env.declare(target, VarType::Int);
}
```

This was adjacent to the directive, not part of it. Under the bug policy
finding it is a stop-everything event and fixing it is not optional, so
I fixed it here rather than filing it; whether it should have been its
own commit is DG's call.

## Gates

Specs migrated where the strict rule changed a program's meaning:
`types.sh` ×7 (`i=N` → `int i=N`), `taint.sh` 60.70 (now the attested
form), `subscript.sh` 80.31/80.46/80.47/80.49/80.55. Nine new specs —
80.59–80.66 for the type rules and both remedies, 60.72 for the attest
fix.

Predicted 6440 before sweeping; the sweep returned exactly 6440, zero
failing, **zero flips against `0e23874`**.

Non-vacuity proved by neutering, and it took **two** passes, which is
itself the finding: `is_int_index` feeds only the array path, because
`map_key_error` types the key itself. Neutering `is_int_index` failed
the array cohort (80.6–80.8, 80.35, 80.41–80.47, 80.49, 80.55, 80.64)
and left every map spec passing. Neutering the map rule back to its
narrow literal-only form failed exactly 80.59, 80.61 and 80.66.
Neutering the `env.declare` failed 60.72. One neuter would have reported
a clean proof over half the work.

The four accept-side specs — 80.60, 80.62, 80.63, 80.65 — pass both ways
by design. They are regression guards on the remedies the diagnostics
recommend: a rule that rejects a form without a compiling alternative is
worse than no rule, and nothing else asserts that the advice is true.
