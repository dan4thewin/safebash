# 358 — The guards validate already keeps

Resumed the coverage queue at `check_len_operand`, whose three cold
branches I had been probing when the last session ended. The probes had
all come back SB5066 — which I had read as "my test programs are
malformed", and they were: the DD#100 call-site sigil is `=name`, not
`@name`. `@name` is the *splat*. Copying the form from `spec/collparam.sh`
instead of inventing it made every probe work first try.

With the syntax right, the answer arrived immediately, and it was not the
one I expected:

```
for A in @A   →  SB5080
len A         →  SB5066   (A not passed)
str A="x"     →  SB5065
```

SB5080 is a check I shipped three commits ago. The `env.is_slot(&name)`
disjunct in `check_len_operand` was guarding against a slot letter
arriving as a loop variable — and my own bug fix had closed that route.
The branch did not go cold because it was always dead; it went cold
because we fixed the thing that reached it.

## The family, not the function

The lesson from journals 352–353 is to check the whole family in one
pass, so before touching anything I looked at the other three `is_slot`
call sites. Two of them were live. The third was not, and neither were
two more disjuncts next to it. Branch-level counts, not line counts, are
what showed this — line 999 has a count of 26 and looks perfectly warm:

```
999|0|0|0     name.is_empty()              never true
999|1|2|20    env.assoc_arrays.contains()  live
999|1|3|6
999|2|4|0     env.is_slot()                never true
```

A line covered 26 times, holding two branches that never fire. Line
coverage cannot see this. It is the argument for `mk cov-branch` in one
table.

Every one of the five turned out to be blocked by a validate rule that
runs before type check, and I proved each by execution rather than by
reading the call sites — the error I was corrected for in journal 356:

| guard | what actually rejects it first |
|---|---|
| `len` / `keys` with no args | SB5073 (statement), SB4045 (arity) |
| `keys` non-name operand | SB5062 — every form tried: `${m[0]}`, `"$x"`, `$((1+1))`, `$(echo m)`, `'m'`, `@a` |
| `keys` operand is a slot | SB5066 unpassed; a passed slot is in `assoc_arrays`/`collections`, so `is_slot` is false by construction |
| `len` operand is a slot | SB5066 / SB5080 / SB5065 |
| subscript on a non-collection | SB5068, on the read path *and* the write path |

These type-check functions were each re-deriving a precondition that
validate already enforces. That is not defensive depth, it is a second
copy of a rule that can drift from the first.

## What came out, and one that did not

Three purges, all of which left every probe's diagnostic byte-identical:

- `check_len_operand` lost `env.is_slot(&name)` and `name.is_empty()`.
  The second was redundant *twice over*: `""` is never a key in `vars`
  or `loop_vars`, so the following disjunct returns anyway, with no
  dependence on validate at all.
- `check_keys_operand` lost the same two. Here `name.is_empty()` was
  **not** redundant — it was suppressing a nonsense message (``keys`
  needs a map — `` is not a map``). Worth recording that the same
  expression in two sibling functions had two different jobs.
- `check_subscript_key_kind` lost `!env.collections.contains(name)`.
  This one is a different kind: not a redundant disjunct but the
  function's stated precondition. Purging it moves the contract to the
  caller. I judged it reversible and low-consequence — one line to
  restore, and the failure mode on a validate regression is a wrong
  diagnostic, which is loud — so I decided and am reporting rather than
  asking. Veto is cheap if you disagree.

Complexity fell on all three: `check_len_operand` 8/4 → 6/4,
`check_keys_operand` 6/4 → 4/4, `check_subscript_key_kind` 8/4 → 6/4.

No ICE guards this time. The standing exception is for a regression that
would otherwise produce a *silent wrong answer*, which is what justified
the guards in journal 356. None of these five qualify: each would produce
a misleading error message, not a wrong result.

## An irreducible class

Two cold branches survive deliberately, and they are the same shape in
both functions:

```rust
let Some(arg) = c.args.first() else {
    return;
};
```

Validate guarantees the list is non-empty, so the `else` never runs.
Neither purging nor an ICE guard removes the branch — an `Option`
destructure has two arms however it is written. The only fix that would
actually raise the number is restructuring both functions to take the
argument from the caller, which is churn for two branches. Left alone,
recorded here so the next pass does not re-derive it.

Sweep 5480, zero failing — the total predicted before running it, and no
spec changes. Units 92.
