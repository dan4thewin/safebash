---
title: "BUG — a `[[ ]]` test operand spliced from a literal + a variable (`run$suf`) doesn't concatenate at runtime"
status: done
priority: P3
kind: bug
depends: []
gates:
  - "`suf=ing; x=running; [[ $x == run$suf ]]` is true (RHS builds `running`)"
  - "same for `=~`: `suf=ing; x=running; [[ $x =~ run$suf ]]` matches"
  - "native + c + janet identical; spec added"
---

# BUG — literal+var test operand doesn't concatenate

> **NOT A BUG (2026-07-07, autopilot).** The repro is arithmetically
> mistaken: `run` + `ing` = `runing`, **not** `running`, so
> `x=running; [[ $x == run$suf ]]` is correctly `N` — **bash returns `N`
> too** (verified against the oracle). The splice concat works: with
> correct expectations `suf=ing; x=runing; [[ $x == run$suf ]]` → `Y`, and
> the `=~` case matches. `expr_to_value(Concat)` → `concat_to_value` builds
> the concatenation fine. Guarded by spec/operand_words.sh ow.11 (`==`
> splice), ow.12 (`=~` splice), ow.13 (negative), all modes. Closed.

Found 2026-07-04 while writing the DD#71 operand-word specs.

## Repro

```
$ sash -c 'suf=ing; x=running; [[ $x == run$suf ]] && echo Y || echo N' -o /tmp/b; /tmp/b
N        # WRONG — RHS should be `running`, so Y
```

Affects **both** `==` and `=~` (the `=~` case surfaced it, but `==` is the
same). A *whole*-variable operand works (`pat=running; [[ $x == $pat ]]` →
Y); only the **spliced** literal+expansion (`run$suf`) fails.

## Scope / root cause (unconfirmed)

This is **not** the DD#71 operand-word parsing — the `==` RHS path
(`parse_test_operand` → `parse_expr`) is untouched by DD#71 and fails
identically, so the operand parses as a `Concat[Literal "run", Expansion
"suf"]` but the **value lowering** (`expr_to_value` on a `Concat` in test-RHS
position, → `SsaTestRhs::Value`) does not build the concatenation correctly
(or the concat is dropped). Investigate `expr_to_value`/`Concat` lowering in
`transform/expressions.rs` for the test-operand path; compare with the
command-arg path where `foo$x` concatenation presumably works.

## Notes

- Independent of DD#71; predates it. The DD#71 dynamic-`=~` path
  (`expr_is_dynamic` → `Value`) rides on the same `expr_to_value`, so fixing
  this fixes spliced dynamic regexes too.
- Add a spec (an `ow.11`-style splice) once fixed.
