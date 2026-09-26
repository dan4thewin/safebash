---
status: done
priority: P3
depends: DD80-braced-key-discipline
gates: closed by the DD#80 amendment (2026-09-26)
---

# Array indices that are provably strings but not *declared* `str`

**Resolved 2026-09-26 (`761b3f4`) — but not by the route this task proposed.**
The question it asks no longer arises.

## What the task was for

Journal 412 had closed only the *declared* case: `str s=abc; ${a[$s]}` was
SB4030, but a bare `i=1` was not, because `TypeEnv::var_type` ends in
`unwrap_or(VarType::String)` and "declared a string" and "never recorded" are
the same answer. The proposal was to widen the **negative** test — infer
"string by evidence" from a non-numeric literal initializer, a capture, or a
string intrinsic.

## Why that route was abandoned

The negative test cannot be made complete. It can only ever name the forms it
happens to know are strings and must default the rest to accepted, and no
amount of evidence-gathering changes that: the default *is* the ambiguity.
Every case added is a case, not a rule.

DG's directive settled it from the other side — **"array indexes must be
int-typed"** — which is the positive question. `is_int_index` asks
`expr_type(expr, env) == VarType::Int`, a fact rather than an absence, and the
`unwrap_or(String)` conflation stops mattering because nothing depends on
telling the two apart any more. See the 2026-09-26 amendment to DD#80.

So "the larger question" this task closes with — *whether an array index
should require `int` at all* — is answered yes, and the value-flow machinery
it was scoping became unnecessary rather than implemented.

## The blocker dissolved differently too

The task named 60.70 as the blocker: `i=$(printf 1); attest i i?` is a
capture, and closing the capture case was said to require `attest x i?` to
retype `x` as Int.

That is **not** what happened. `attest x i?` still validates in place without
retyping; 60.70 migrated to the arith-convert form `attest i (( n = i ))`,
which binds a *separate* int name — and which, it turned out, had never
declared its target as an int at all. See DD#93's 2026-09-26 addendum; that
was a live DD#105 hole, fixed in the same commit.

The other cost the task cites — "8 specs in both modes" — was paid: the
affected specs were migrated (`types.sh` x7, `taint.sh` 60.70,
`subscript.sh` 80.31/46/47/49/55), each one a program whose meaning the
strict rule genuinely changes.

## What replaced spec 80.47

80.47 was "the backstop pinning the deliberate silence" — it asserted that a
bare `i=1` *does* index an array. It now asserts the opposite, because the
silence is gone.

## See also

- DD#80, amendment 2026-09-26 — the rule that closed this
- `notes/journal/414-the-key-is-a-type-not-a-spelling.md`
- `notes/tasks/undispositioned-string-to-int-sites.md` — the sibling
  inventory, still open for `exit` / `wait` / `[[ -t ]]`
