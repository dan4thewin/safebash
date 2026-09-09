---
title: "bug: `$(status)` inside a double-quoted string is not desugared"
status: done
priority: P1
kind: bug
depends: []
gates:
  - "`echo \"s=$(status)\"` prints the status, exactly as `echo \"s=$?\"` does"
  - "no spelling of `status` reaches `sash_exec` as a bare command name"
---

## What

`$?` is sugar for `$(status)` (DG), so the two must agree everywhere. Inside a
double-quoted string they do not: `$?` works and `$(status)` emits a runtime
error string **into the output** and exits 0.

```sh
/bin/false || { echo "$(status)"; }
# -> sash: bug: bare command passed to sash_exec (expected absolute path)

/bin/false || { echo "$?"; }        # -> 1        (correct)
/bin/false || { echo $(status); }   # -> 1        (correct, unquoted)
/bin/false || { s=$(status); … }    # -> 1        (correct, assignment)
```

It is a **silent wrong answer**, not a crash: the diagnostic text is
substituted as the value, the program continues, and the exit status is 0.

## Scope

Specific to `status`, and specific to the quoted context. `$(len a)` inside
`"…"` is fine, so the double-quoted command-substitution path is not broken in
general — only this desugaring is missing from it.

## Cause

`$(status)` → `$?` is a **parse-time** rewrite: `try_special_word`
(`sash/src/parse/expr.rs:1466`) matches the bareword `status` and returns
`SpecialWord::Returned($?)`. That probe sits on the unquoted `$( )` path. The
`$( )` scanner used inside a double-quoted string does not call it, so the
substitution survives as a `Command { name: "status" }`, which no backend
recognises as a builtin, so it is dispatched as an external — hence
`sash_exec`'s "bare command passed" complaint.

`status` is registered as a builtin in `validate.rs:1820` and handled in
`transform/expr_handlers.rs:602` and `codegen/value_handlers.rs:762`, so the
compile-time checks all pass and nothing flags it.

## Note

The related first-expression rule (DD#55: `$(status)` must be the first
expression in a handling branch) IS enforced — `cmd || { /bin/echo x;
s=$(status); }` is correctly rejected (spec `errors.sh` 55.17b). So this is
not a hole in that rule; it is the desugaring itself going missing in one
context.

Found while comparing `-` and `|| :` discard idioms.

## Resolved 2026-09-09

`command_sub_to_expr` (the `$( )`-inside-a-string path) now applies the same
bareword shortcut the unquoted path gets from `try_special_word`, via
`bareword_shortcut_expansion`.

The rewrite is applied to the **parsed result** — one argument-less `Command`
named `status`/`getcpid` — rather than to the raw content text, so the two
paths agree on what counts as the bare form (whitespace, `;`, a trailing
newline) instead of re-deciding it with a string compare.

`$(getcpid)` (sugar for `$!`) had the identical bug and is fixed by the same
change; it was found by checking whether the defect was specific to `status`.

Specs: `errors.sh` 55.51-55.56. 55.56 pins the *shape* of the old failure
(no "bare command" / "sash: bug" text in the output) so a regression cannot be
silent again.
