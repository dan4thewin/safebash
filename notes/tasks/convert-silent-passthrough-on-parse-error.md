---
status: next
priority: P2
---

# sash-convert silently passes through input tree-sitter cannot parse

Found 2026-09-26 while checking DD#118 Part D against the converter.
Pre-existing; unrelated to DD#118 (the code path below is never entered,
so no DD#118 rewrite can reach it).

## The shape

`[ a=b ]` — a glued `=` inside single brackets — is not parsed as a
`test_command` by stock tree-sitter-bash 0.25. It yields an `ERROR`:

```
program
  ERROR
    [                      '['
    command
      variable_assignment  a = b
      command_name         ']'
```

`a=b` is read as an assignment prefix and `]` as the command name. Any
`=` in an argument of `[ ]` does it: `[ ab==c ]`, `[ ab==$x ]` too.

## Why it matters

`walk_node` never sees a `test_command`, so `convert_test_command` never
runs and the text is emitted verbatim. The output is not rejected — it
**compiles**, because sash has no `[` builtin, so DD#23 resolves `[` to
the external `/usr/bin/[`:

```
$ sash --manifest z.sash
/usr/bin/[
```

So a bash test silently becomes a forked coreutils invocation rather
than a `[[ ]]` construct. The answer is bash-correct (coreutils `[` is
what bash's builtin implements), and the manifest discloses the
external, so nothing is unsound — but the user asked for a conversion
and got a downgrade with nothing said.

## The general gap

`convert.rs:289` has an `"ERROR"` arm, but it handles exactly one known
grammar hole (`${a[a[0]]}`, DD#80) and otherwise falls through
silently. Every other unparseable construct is passed through with no
`# MANUAL:` note. That is the fix: an `ERROR` node the arm does not
recognise should emit a `# MANUAL:` prefix line naming the span, the
way `NumericResult::TooComplex` already does at `convert.rs:1133`.

`[ a=b ]` would then additionally deserve its own recognition — the
text is exact, so it could be rewritten to `[[ -n a=b ]]` (bash's
reading) the way the `${a[a[0]]}` case is handled. Worth doing only if
the generic warning proves too noisy.

## Cost

Small. One arm in `walk_node`, plus specs in `convert.sh` (`57.x`).
Expect a handful of existing convert specs to gain a `# MANUAL:` line
if any of their inputs contain an unrecognised `ERROR` — check before
assuming none do.
