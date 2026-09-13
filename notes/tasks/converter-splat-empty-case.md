---
title: "sash-convert — `cmd \"$@\"` has no faithful conversion; pick the empty-case default"
status: blocked
priority: P3
kind: design
depends: []
blocked_on: DG
gates:
  - "DG picks: MANUAL-only (current), `@{a:-WORD}` scaffold, or `@{a:?WORD}` scaffold"
  - "if a scaffold is chosen, conv.x7 asserts it round-trips"
---

## The fork

`converter-positional-use-needs-bless` is implemented except for this case.
DG's 2026-07-08 resolution said to emit:

```bash
argv a; attest a in e [[ -n $e ]]   # MANUAL(sash-convert): tighten predicate
cmd @a
```

That resolution **predates SB5032**, which now rejects a bare `@a` splat into
an external command:

> a bare `@a` array splat into an external command is unsafe — an empty array
> would run it with too few arguments. State the empty case: `@{a:-WORD}`
> (empty → the single arg WORD) or `@{a:?WORD}` (empty → abort with WORD)

So the scaffold as written does not compile, and the two spellings that do
both diverge from bash, which runs the command with **zero** arguments:

| spelling | empty `"$@"` behaviour | matches bash? |
|---|---|---|
| `cmd @{a:-WORD}` | runs `cmd WORD` — one argument | no |
| `cmd @{a:?WORD}` | aborts with `WORD` | no |
| (bash) | runs `cmd` with no arguments | — |

## Current behaviour (shipped, journal 289)

A MANUAL marker naming the fork, and no rewrite:

```
# MANUAL: `"$@"` as a command argument — snapshot it (`argv 1 a`;
# `attest e in a [[ -n $e ]]`) and pass `@{a:-WORD}` or `@{a:?WORD}`,
# which states what an empty argument list should do (SB5032)
/bin/echo "$@"
```

This satisfies the parent task's gate ("either compiles or carries a MANUAL
marker") and does not silently change what the script does. Pinned by
`spec/convert.sh` conv.x7.

## Options

1. **Keep MANUAL-only.** Honest; the author is told exactly what to decide.
   Costs: converted scripts using `cmd "$@"` need a hand edit before they
   build. Recommended, and what is shipped.
2. **Scaffold with `@{a:?…}`.** Compiles; an empty argument list becomes a
   runtime abort. Loud, so it will not be missed — but it is a behaviour
   change the author did not ask for, and `cmd` with no arguments is a
   perfectly ordinary thing for a script to do.
3. **Scaffold with `@{a:-…}`.** Compiles; an empty argument list silently
   grows one argument. This is the worst of the three: it changes behaviour
   quietly.
4. **A fourth sash spelling** meaning "empty → pass nothing", which would make
   the conversion faithful. This is a language change (DD#22 territory), not a
   converter change, and would need its own DD.

Option 4 is the only one that makes `cmd "$@"` mechanically convertible.
Whether that case is common enough to justify a language surface is DG's call.
