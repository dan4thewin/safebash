---
status: accepted
date: 2026-10-03
---

# DD#122 — a pipe binds to the reader, not to fd 0

## The rule

DG, 2026-10-03:

> `readline` or `while readline` without a leading pipe would read fd 0;
> `cmd | while readline v` would read the output of `cmd`; a `/bin/cat`
> within that `cmd | while` would read fd 0.

Three clauses, one rule: **a pipeline delivers its data to a named reader,
not to a file descriptor.**

| what | reads |
|---|---|
| `readline v` / `read a`, no pipe | fd 0 |
| `cmd \| readline v` / `cmd \| read a` | the pipe |
| `cmd \| while readline v; do … done` | the `readline` reads the pipe |
| a command *inside* that loop body | fd 0 |
| `cmd \| /bin/cat` (a forked stage) | the pipe, as fd 0 |

The last two lines are the ones that look inconsistent and are not. A
pipeline *stage* is forked, so it inherits the pipe as its stdin, exactly as
bash does. A command in a reader-loop's **body** is not a stage — the loop
runs in the parent, so the command inherits the parent's stdin. Nothing
rebinds fd 0 in the parent, ever.

## Why the loop runs in the parent

This is the whole reason the rule exists.

```sash
int n=0
/bin/printf 'a\nb\nc\n' | while readline v; do (( n = n + 1 )); done
echo "n=$n"
```

**sash prints `n=3`. bash prints `n=0`**, because bash makes every pipeline
stage a subshell and the increments die with it. That is bash's most
notorious gotcha, and safebash does not reproduce it: the loop is lowered
into the parent and the pipe is handed to the `readline` inside it.

Keeping that property is what forces the rest of the design. The loop cannot
be forked, so fd 0 cannot be redirected to the pipe — a parent-wide fd
change would leak to every later statement. The pipe therefore has to attach
to the thing that reads it.

## Consequences

### 1. A reader binds to its own pipeline and no other

`repoint_readlines_to_pipe` and `rewrite_lastpipe_consumer_fds` attach a
pipe by rewriting a reader's fd to `-1`, a sentinel codegen resolves to the
captured pipe fd. Both used to walk **every block lowered so far**, so the
rewrite reached backwards:

```sash
/bin/printf 'x\n' | while readline a; do echo "A=$a"; done
readline mid                       # no leading pipe — must read fd 0
echo "MID=[$mid]"
/bin/printf 'y\n' | while readline b; do echo "B=$b"; done
```

`MID` came out **empty**, aimed at the first loop's exhausted pipe. Delete
the first pipeline and the identical statement reads fd 0 correctly. Exit 0,
no diagnostic, both backends.

Each repoint is now scoped to the instructions its own final stage produced
(`transform::pipelines::mark_ssa` / `for_each_new_inst`). A nested
pipe-while repoints its own readers first, and the outer `fd == 0` guard
then leaves them alone — an inner reader belongs to the inner pipe.

### 2. There is one spelling of "no pipe captured"

Native codegen initialised `pipe_read_fd_var` to `0`, `--emit-c` to `-1`.
`0` is a *valid* fd, so when the sentinel leaked the native build read stdin
and looked right while the C build aborted inside rustix. Both are `-1` now.
After the scoping fix the value is unobservable — an instruction carries fd
`-1` only when the pipeline instruction defining the variable was emitted
immediately before it — but one sentinel should have one spelling.

### 3. The final stage must consume the pipe (SB4022 / SB4073)

If the pipe binds to a reader, a final stage with no reader receives
nothing. Two shapes could reach that state and both are now rejected at
parse time.

**A compound final stage — SB4022.** `cmd | { … }`, `| if`, `| for`.
These *do* read the pipe, because they are forked, but a fork is exactly
what the parent-process lowering exists to avoid:

```sash
/bin/printf 'a\n' | { readline g; }          # ran, read a line, g unset
/bin/printf 'a\n' | if readline g; then … fi # took the branch, g unset
```

The read succeeded, the branch was right, the data silently vanished. That
is the bundling **DD#38** rejects `( … )` for — `( readline g; )` is SB4014
in every position — arriving through a different syntactic door. It now gets
DD#38's answer and SB4022's existing advice: *wrap it in a function*, which
is a legal stage and is byte-for-byte equivalent in bash too.

A non-final compound was already SB4022 (the `|` parses as a postfix on the
compound). This completes the rule to both positions.

**A `while` with no reader — SB4073.** `cmd | while (( n < 1 )); do
/bin/cat; done` ran the loop in the parent, and nothing bound the pipe, so
the producer's output went nowhere. It is consistent with the rule and is
almost certainly a mistake, so it is an error rather than a silent discard.

The transform's `is_while_read` stays structural — `matches!(StmtKind::
While(_))` — because SB4073 now guarantees what it used to assume.

### 4. Legal final stages, exhaustively

- an ordinary command — forked, inherits the pipe as fd 0
- a user function — the same, inlined as a forked compound child
- `readline NAME` — DD#99 scalar lastpipe, parent
- `read ARRAY` — DD#99 NUL-split lastpipe, parent
- `while` — parent, with a `readline`/`read` in its condition or body

The reader search (`Statement::binds_a_pipe_reader`) deliberately does not
descend into a nested `Pipeline`, a `Function` definition, or a `Redirected`
body: a reader in any of those binds to something else, and the transform's
repoint scopes itself to match.

## Notes

The rule was implemented before it was written down. The pipe-attachment
defects above were all found by reading the code and inferring the design
from it, which produced the wrong model twice — once concluding that a
`/bin/cat` inside a reader loop *should* see the pipe. DG's three clauses
settled it. This DD exists so the next reader does not have to guess.
