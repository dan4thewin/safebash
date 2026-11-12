title: "`check_command` and `check_subscript_in_stmt` disagree on which operand checks run"
status: next
priority: P3
kind: investigation
depends: []
gates:
  - "the difference is explained: either a documented reason, or a bug with a probe that demonstrates it"
  - "if a gap: a spec pins each operand check at each site"
  - "if deliberate: a comment at both sites naming why the sets differ"

# Two type-check entry points, two different check sets

In `sash/src/type_check.rs`:

- `check_command` (≈1356) runs **three** operand checks.
- `check_subscript_in_stmt` (≈964) runs **four** — it is the only caller of
  `check_len_operand`.

Nothing states why. Either `check_len_operand` is unnecessary at the
`check_command` site (in which case say so), or there is a command form
reaching `check_command` whose `len`/`strlen` operand is never validated — the
same shape as the DD#58 sexpr hole found in journal 383, which was exactly
"a check that only ran on one of the paths that needed it".

## How to settle it

By probe, not by reading. Enumerate the statement shapes that reach each entry
point and run the `len`/`strlen` operand matrix (scalar / array / map / int,
bare name and `$`-value, both spellings) through each. The matrix from journal
383 is reusable; a divergent row is the answer.

Do **not** conclude from call sites alone — the sexpr bug was invisible that
way, because the reconstruction that made the two paths equivalent happened in
a later pass.

## Context

Filed from journal 383. Noticed while fixing the sexpr operand slot; deferred
because it is a separate question and the fix in hand was already scoped and
approved.
