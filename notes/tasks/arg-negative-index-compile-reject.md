---
title: "$(arg -N) negative index is not a compile error (DD#60 says it should be)"
status: done
priority: P3
kind: bug
depends: []
gates:
  - "`x=$(arg -1)` is a compile error (SB code), not a compile-success that only aborts at runtime"
  - "`$(arg -1 DEFAULT)` — decide: reject the negative literal at compile time too, OR treat negative as out-of-range → default (currently the runtime returns the default for negative)"
  - "argv.sh av.t7 rejects for the *negative-index* reason, not (only) the downstream unblessed-read taint check"
---

## Symptom

DD#60: "Negative indices (`$(arg -1)`, `$(argv -1)`) are rejected at compile
time — no negative indexing in v1." But they are **not** enforced:

    x=$(arg -1)          # COMPILES (should be a compile error)

`spec/argv.sh` av.t7 (`x=$(arg -1); echo "$x"`) passes only because the
unblessed `echo "$x"` trips the taint check (SB5020) — not because the negative
index is rejected. Bare `$(arg -1)` does abort at *runtime* (`sash_argv_get`
panics on `idx < 0`), so it's not a silent-wrong-value bug, but the DD#60
compile-time guarantee is missing.

## Also decide for the addendum

`$(arg -1 DEFAULT)` — the runtime (`sash_argv_get_or`) currently treats any
invalid index (negative or `>= argc`) as out-of-range → returns the default. If
negative literals become a compile error, decide whether `$(arg -1 D)` is a
compile error (consistent) or the default applies (lenient). Lean: compile-error
the *literal* negative (a typo guard), consistent with DD#60.

## Fix sketch

In `validate.rs`, where `$(arg …)` arity is checked (`flag_arg_builtin`), also
reject a negative *literal* index (`c.args[0]` is a `Literal` starting `-` with
digits). A dynamic negative index stays a runtime concern.

## Provenance

Found during DD#60 `$(arg N DEFAULT)` / DD#78 work (journal 148). Orthogonal to
that feature; the negative case was pre-existing.
