# 504 — The arms the callers take apart first

Journal 503 established that no program reaches the `ExprKind::AssocArray`
arm of `is_taint_source`, `expr_uses_tainted` or `check_value_cmdsub_reads`,
and said the artefact was a direct-call unit test. This is that test, plus
a fourth walker found in the same family.

## Why these are answers and not ICEs

`validate.rs` already has one worked example of the opposite choice:
`expr_uses_tainted`'s `ExprKind::Test` arm is an `unreachable!()`, because
`ExprKind::Test` is built only by `parse_test_command` and `parse_attest`,
both of which yield a *statement*. Nothing can hand that walker a test, so
a correct-looking recursion there would be an unverified claim.

The `AssocArray` arms are a different case and it matters. Nothing
*forbids* a map literal in those positions — it simply does not arrive,
because `reject_tainted_into_array` is handed each element **value** and
the literal is decomposed one level up. If the caller ever stopped
decomposing, an `unreachable!()` would turn a working program into a
crash, where the correct recursion would just keep working.

There is a second reason, specific to this pair:
`reject_tainted_into_array` ORs `is_taint_source` and `expr_uses_tainted`
together. The invariant lives *between* them, not in either one. An arm
that disagreed with its twin is a taint hole, not a panic, so the thing
worth pinning is the answer.

Each test therefore asserts a positive **and** a negative. Without the
negative, an arm hard-wired to `true` passes — which is exactly the trap
`atop.3` was added to avoid in the spec suite last session.

The third test goes further and asserts the *pair*: a map literal whose
element reads a tainted variable `uses` taint but is not itself a
`source`. That is one assertion neither function can satisfy alone.

## A fourth walker

Ranking by function hid this one, because `first_expansion` has its two
cold lines in different arms. It is the same family: a fourth
`AssocArray` arm, cold for a fourth caller's reason —
`flag_arg_builtin` asks it about an `arg` default, and a default is
parsed as a word. Its `Test` arm is cold for the *statement-only* reason
instead.

Both must answer "substituted". The rule `first_expansion` serves is
"can any of this change at runtime", and its own doc already says a
wildcard "would silently answer *no* for it and reopen the hole this
rule closes" — so a cold arm answering `None` is the same hole with
extra steps.

`mk test` 119 → 123.

## The shape of this part of the queue

Four walkers, one variant, four different reasons for being cold, and
one shared reason for existing (the no-wildcard convention). Ranking
cold *lines by function* is what nearly missed the fourth — it had two
cold lines in two unrelated arms and sorted below functions with three
in one. Worth remembering when the queue is mined by count.
