# Design Decision #21: `-` prefix for error suppression

**Date:** 2025-04-15

**Problem:** With pipefail always on (DD#20) and implicit fatal default for
commands, the programmer needs a way to suppress failure for specific commands.
The `|| :` idiom is syntactically broken inside pipelines (`|| :` binds tighter
than `|`), and modifying the command name (`cmd?`, `cmd!`) feels wrong — the
command is a program, the modifier is about *our response* to its result.

**Resolution:** Borrow Make's `-` prefix. A `-` before a command means
"ignore this command's nonzero exit."

```bash
mktemp                          # fatal on failure (default)
- grep pattern file             # no matches is ok
- diff a b                      # difference found is ok
- grep pattern file | wc -l    # grep tolerates failure, wc is fatal
sort file | - head -1           # head gets SIGPIPE, that's ok
```

**Rules:**
- `-` always applies to the single command immediately following it
- `-` never applies to a whole pipeline — annotate each segment individually
- No ambiguity: `- cmd1 | cmd2` means cmd1 tolerates failure, cmd2 is fatal
- `-` at statement position is not valid bash syntax, so no collision

**Rationale:**
- Same semantics as Make's `-` prefix, decades of precedent
- The modifier is on the *statement*, not the command — the command is just
  a program, we're modifying how we respond to its result
- Reads naturally: "don't care about this one"
- The coerce tool (DD#8) inserts `-` where needed during migration

**Considered and rejected:**
- `cmd?` — reads as a predicate/question, opposite of "ignore"; also modifies
  the command name rather than the statement
- `|| :` inside pipelines — syntactically broken, needs ugly `{ cmd || true; }`
- `try cmd` — implies `catch`, feels incomplete without it
- `|:` as pipe variant — only works in pipe context, not standalone

