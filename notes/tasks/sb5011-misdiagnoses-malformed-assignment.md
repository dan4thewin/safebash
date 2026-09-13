---
title: "SB5011 blames a variable/expansion for a word that has neither"
status: done
priority: P4
kind: diagnostic
depends: []
gates:
  - "`a-b=1` and `9x=1` report a message that names the actual cause (a malformed assignment target / an unrunnable word), not \"a variable or expansion can't be resolved\""
  - "the genuine dynamic-head case (`$x foo`) keeps its current wording"
---

## What

A word that looks like an assignment but whose target is not a valid
identifier falls through to command position, where `check_cmd_in_stmt`
reports:

```
error [SB5011] a command name must be a literal path — a variable or
expansion can't be resolved at compile time; use the literal path, or
`/usr/bin/env` for a runtime tool
```

for input that contains **no variable and no expansion**:

```sh
a-b=1
9x=1
a]b[=1
```

The diagnosis names a cause the source does not have. The author is told to
"use the literal path" for a line they never intended as a command.

## Why it says that

`check_cmd_in_stmt` branches on `!matches!(&c.name.kind, ExprKind::Literal(_))`.
The malformed assignment word reaches it as a non-`Literal` expression (the `=`
splits it), so it takes the dynamic-head arm. The arm's message is correct for
its intended subject — `$x foo`, the DD#23 runtime-chosen command — and wrong
for everything else that happens to arrive non-literal.

## Not caused by the identifier fix

Measured both directions: `a-b=1` produced this identical message **before**
`dup-is-identifier-leading-digit` landed. That change only routed `9x=1` into
the same pre-existing path, which is the uniform behaviour DG asked for when
the removed-builtin table was dropped. The wording predates it.

## Suggested shape

Distinguish the two arrivals rather than widening the message: if the
non-literal name came from a word containing an unquoted `=`, say the target
is not a valid variable name. Otherwise keep the current text.

## Found

2026-09-12, while measuring reachability for
`dup-is-identifier-leading-digit`. Pinned indirectly by `spec/assign_words.sh`
`aw.11`–`aw.15` and `aw.20`, which assert the code but deliberately not the
wording.

## Resolution (2026-09-13, commit pending)

Fixed in `sash/src/validate.rs`: `bad_command_head_diag` chooses between two
messages using `malformed_assignment_target`, which accumulates literal parts
of the `Concat` head up to the first `Literal("=")` and returns `None` if any
non-literal appears in that prefix.

Both gates met. Gate 1: `a-b=1`, `9x=1`, `0=1`, `a]b[=1` now report
`` `<target>` is not a valid variable name, so this is neither an assignment
nor a runnable command — a name is letters, digits and `_`, and cannot start
with a digit`` (specs `aw.21`-`aw.23`). Gate 2: `$x foo` keeps the
literal-path wording (`aw.24`), as does `9x[$i]=1`, whose head really does
contain an expansion (`aw.25`). `aw.26` pins that an `=` in an argument
(`echo a=b`) is untouched.

Code remains SB5011; `aw.11`-`aw.15` and `aw.20` assert the code only.
Journal 288.
