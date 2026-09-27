---
title: A downgrade nobody was told about
date: 2026-09-27
---

# A downgrade nobody was told about

`sash-convert` had an `ERROR` arm that recognised exactly one grammar hole
and silently passed everything else through. This session gave it a voice.

## The shape

tree-sitter-bash 0.25 cannot parse a glued `=` inside single brackets.
`[ a=b ]` is read as an assignment prefix followed by a command named `]`:

```
program
  ERROR
    [                      '['
    command
      variable_assignment  a = b
      command_name         ']'
```

So no `test_command` node ever exists, `convert_test_command` never runs, and
the text is emitted verbatim. That output is not rejected — it **compiles**,
because sash has no `[` builtin and DD#23 resolves `[` to the external
`/usr/bin/[`.

Nothing here is unsound. coreutils `[` is what bash's builtin implements, and
the manifest discloses the external. But the user asked for a conversion and
got a fork, with nothing said. The gap was general: the `ERROR` arm handled
`${a[a[0]]}` (DD#80) and fell through silently for everything else.

## The fix, and the two things that made it honest

One `prefix_lines.push` in the fallthrough, matching the `# MANUAL:` form
already used for `NumericResult::TooComplex` and `select` and process
substitution. Two details mattered more than the push itself.

**Only the outermost ERROR is reported.** tree-sitter fragments a malformed
region into nested ERROR nodes: `if [ a=b ]; then echo hi; fi` yields one for
the whole statement and another for the stray `; then`. The first version
emitted both, and the second note is an artefact of the failed parse rather
than a second thing to fix. A `has_ancestor_kind(node, "ERROR")` guard
suppresses it.

**The guard suppresses the note, not the recursion.** My first attempt
returned early, which would have skipped the children — and a recognisable
construct nested deeper inside a bad span would then stop being rewritten.
Worth catching: the whole point of the change is disclosure, and silently
losing a conversion to buy a warning would have been a net loss.

I checked whether the generic note swallowed the specific handler, and it does
not: the DD#80 recognition runs first and returns before the fallthrough. That
is spec `57.176`, and it is the control that matters most, because a careless
edit to this arm would break exactly that ordering.

## A pre-existing limitation the note now discloses

`${a[a[0]]}` converts correctly on its own, but *inside* a larger unparseable
span it does not — the nested expansion is not its own ERROR node there, so
the DD#80 arm never matches its text. I confirmed against a stashed tree that
this predates the change, and left it alone.

It is a good illustration of what the warning buys. Before, that input
produced a silently half-converted file. Now it produces a half-converted file
that says so.

## Sixty-two failures that were not failures

Running `convert.sh` standalone reported **62 failures**. They were entirely
an artefact: `run_convert` invokes bare `sash-convert`, and `common.sh`
*appends* `target/release` to `PATH`, so the helper resolved to an installed
binary two weeks old. With the path prepended, 197/197 passed.

AGENTS.md documents this precisely — including the same 62-failure count, from
2026-09-26, the day before. I hit it anyway, because I reached for the
standalone run as a quick check and did not re-read the harness section first.

The costly part is not the detour. It is that the same 62 would have appeared
under a neuter, where a large failure count reads as *signal* — proof the
neuter worked. AGENTS.md already says it: a standalone failure count is not a
finding until it survives a prepended PATH. I have stored it as a memory, since
reading it in a file I had already read was evidently not enough.

## Gate

Predicted 6581, swept 6581, zero failing, zero flips, 5 new ids — native-only,
correctly, since `convert.sh` does not run in c mode.

Two disjoint neuters: removing the note fails `57.173`/`57.175`/`57.177` and
leaves both controls passing; removing the nested-ERROR guard fails `57.175`
alone. `57.175` counts the notes rather than asserting presence, which is what
makes it able to detect the duplicate at all.

## Noted, not filed as blocking

`walk_node` is cc 39 / cog 44 — about twenty times the `sash/src` cognitive
mean. The complexity gate scopes to `sash/src/`, so `sash-convert` is exempt
by construction rather than by merit, and my change added one `if` to an arm
of an already very large `match`. Filed as P4.
