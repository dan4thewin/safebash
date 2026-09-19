# 369 — the only other possibility

`attest x in zz [[ -n $x ]]`, with `zz` declared nowhere, said:

```
error [SB4030] `attest x in zz` requires an array or map — 'zz' is a scalar;
              use scalar attest `attest zz [[ … ]]`
```

Every clause after the dash is false. `zz` is not a scalar. The prescribed
repair does not work either — `attest zz [[ … ]]` on a nowhere-introduced
name is rejected in turn, by a *different* rule with a *different* code. A
user following the advice literally gets a second error and no path forward.

The check asked one question and reported the answer to another:

```rust
if !env.collections.contains(array) { "… is a scalar" }
```

`collections` holds arrays and maps. Absent from it are two populations:
declared scalars, and names that do not exist at all. The code named the
first and swallowed the second, because when you have two buckets and test
one, the other looks like a single thing.

## Already solved, twelve hundred lines up

`check_len_operand` carries this comment:

> Fire only on a name known to be a scalar. "Absent from `collections`" is
> not the same question: an undeclared name is SB5039's or SB5055's to
> report, and answering it here masked the real diagnostic with a
> misleading one ("`f` is a scalar" when `f` was never declared at all).

Same file, same `TypeEnv`, same mistake, already diagnosed in prose. The
attest checks were written against the same environment and did not inherit
the lesson. A comment is not a mechanism.

## Where the fix had to differ

`len` resolves it by **staying silent** and deferring to SB5039, which owns
the question. Attest cannot: nothing downstream rejects an attest-all over
an undeclared name. Silence there does not defer, it *admits* — the program
compiles and iterates a collection that was never introduced. So this one
reports, in SB5039's own words:

```
`attest … in zz` — `zz` is introduced nowhere (not a variable, array, or
map); declare it, or fix the typo
```

Two call sites, one helper (`attest_unknown_name_error`), asked before the
kind question rather than folded into it — the undeclared case is not a
refinement of "wrong kind", it is a different fault.

## The scalar form was already safe, by accident

`attest zz [[ … ]]` with `zz` undeclared reports SB5031 correctly, and it
turns out not to be the kind check doing it: SB5020 requires the condition
to validate the attested variable, so `$zz` is always read, and the read is
what SB5031 catches. The scalar form is covered by a rule about something
else. It holds, but nothing makes it hold — worth knowing before anyone
relaxes SB5020.

## Specs

`ud.30`–`ud.37` in `undeclared.sh` (plan 29 → 37), which is where DD#62's
typo footgun already lives. Four pin the new reject (both binder forms, the
wording, the named word); four pin that the *declared* cases still get the
kind advice they always got — because the cheapest way to "fix" a
misleading message is to make it misleading in the other direction.

Sweep **5661** (5645 + 16), predicted exactly, 0 failing, 0 flips.
