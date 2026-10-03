---
status: next
priority: P1
depends:
gates:
---

# `cmd | while` attaches the pipe by rewriting `ReadLine` fds, not by redirecting fd 0

Found 2026-10-03 running DD#116's "prove it before acting" clause against the
arms journal 466 called validate-shadowed. The probe for
`lower_one_pipeline_stage`'s compound branch **hit** — `cmd | { … }` reaches
it — so that classification was wrong. Chasing why `| while` did *not* hit it
found this. Superseded the first filing, which named only the third defect.

## Root cause

`cmd | while readline v; do … done` is **deliberately not a subshell**. The
loop runs in the **parent process**, so variables it sets survive:

```
int n=0
/bin/printf 'a\nb\nc\n' | while readline v; do (( n = n + 1 )); done
echo "n=$n"
```

sash prints `n=3`; bash prints `n=0`. That is sash fixing bash's single most
notorious pipeline gotcha, and it is worth keeping.

The defect is in **how the pipe is attached**. Instead of redirecting the
parent's fd 0 for the duration of the loop, `lower_pipe_while_read`
(`transform/pipelines.rs:325`) lowers the loop normally and then calls
`repoint_readlines_to_pipe`, which **statically rewrites every `ReadLine`
instruction's `fd` from 0 to -1**, the "use the captured pipe fd" sentinel.

Two things follow, and a third is a separate oversight in the same path.

## D1 — the rewrite is positional, not scoped

`repoint_readlines_to_pipe` walks `ssa.blocks` — every block **lowered so
far**, which is not the loop. Its own comment admits it ("Walks every block,
not just the loop body, because the header/body split is not known here"). A
`readline` that lexically precedes a later pipe-while is captured by it.

```
/bin/printf 'x\n' | while readline a; do echo "A=$a"; done
readline mid
echo "MID=[$mid]"
/bin/printf 'y\n' | while readline b; do echo "B=$b"; done
```

with stdin `PARENT1\nPARENT2\n`:

| | output |
|---|---|
| bash | `A=x` / `MID=[PARENT1]` / `B=y` |
| sash | `A=x` / **`MID=[]`** / `B=y` |

`readline mid` belongs to neither loop. The second pipeline's repoint reached
back and aimed it at the **first** loop's exhausted pipe. Exit 0, no
diagnostic, **both backends**.

With only *one* pipeline the preceding `readline` gets the bare sentinel -1,
and the backends diverge:

- **native** — `pipe_read_fd_var` is initialised to **0**
  (`codegen/setup.rs:1364-1378`), so it reads stdin and is accidentally
  correct.
- **C** — `--emit-c` writes `i64 __pipe_read_fd = -1;`, so it calls
  `sash_read_line(-1)` and **aborts**:
  `assertion failed: fd != u32::MAX as RawFd`, rc=99.

Two initialisers for one SSA variable. The differential oracle catches it.

## D2 — only `ReadLine` is repointed

Everything else in the loop that reads stdin reads the **parent's** stdin,
because the parent's fd 0 was never touched.

```
/bin/printf 'a\nb\n' | while readline v; do echo "l:$v"; /bin/cat; done
```

| | output |
|---|---|
| bash | `l:a` / `b`  — `cat` drains the rest of the pipe |
| sash | `l:a` / `l:b` — `cat` produced nothing |

Proved with `./u1.bin < par.txt`: `cat` printed the contents of `par.txt`,
i.e. the parent's stdin. This hits the **legitimate** `while readline` form,
not just the degenerate one.

## D3 — `is_while_read` is structural

`transform/stmt_handlers.rs:637`:

```rust
let is_while_read = matches!(&last.kind, StmtKind::While(_));
```

The name claims the condition reads; the test is syntax only. So a
`while`/`until` with no reader **anywhere** is routed here too, and since
there is no `ReadLine` to repoint, the producer writes to a pipe nobody
drains:

```
int n=0
/bin/printf 'a\n' | while (( n < 1 )); do (( n = n + 1 )); /bin/cat; done
```

bash prints `a`; sash prints nothing. `until` is identical. This is D2's
worst case rather than a separate mechanism.

Fourth instance of a predicate keyed on syntax rather than on the property it
names (journals 430, 432, 440, 456).

## Measured behaviour of the neighbouring forms

Everything below verified against the live binary and bash, 2026-10-03.

| form | runs where | reads the pipe | keeps assignments | vs bash |
|---|---|---|---|---|
| `\| read arr` | parent (capture) | yes | **yes** | sash-only |
| `\| while readline v; do …` | **parent** | only `ReadLine`s (D2) | **yes** | better, then D2 |
| `\| { … }` | forked child | yes — fd 0 **is** the pipe | **no** | agrees |
| `\| if …; fi` | forked child | yes | **no** | agrees |
| `\| for …; done` | forked child | yes | **no** | agrees |
| `\| while COND; do …` | **parent** | **nothing** (D3) | yes | **broken** |

The three forked forms are correct for free: they are the generic stage
mechanism, the same one `\| /bin/cat` uses. They are also **implicit
subshells** — see the open question below.

## Fix — DG's call

**(a) Redirect fd 0 for the duration of the loop.** Save fd 0, `dup2` the
pipe read end onto it, run the loop in the parent, restore. Kills all three
defects at once: `ReadLine` needs no rewriting (so D1's whole-program walk and
the -1 sentinel both disappear, along with the backend divergence), body
commands inherit the pipe (D2), and a non-reading `while` drains it correctly
(D3). Loop variables still survive. Matches bash everywhere except the
variable scope, where sash is deliberately better. **Recommended.**

**(b) Scope the repoint to the loop's own blocks and keep the sentinel.**
Fixes D1 only. D2 and D3 remain, and the two initialisers remain.

**(c) Fix D3 alone** by making `is_while_read` honest, as the superseded
filing proposed. Now known to be the smallest of the three; leaves the
`while readline` form broken.

## Specs needed

`spec/pipe_functions.sh` has no coverage of a terminal compound stage at all —
the cold-line report is what surfaced that. Needs, at minimum: `| { … }`,
`| if`, `| for`, `| while readline` with a body command that reads, a
`readline` between two pipe-whiles (D1), and whichever disposition `| while
COND` ends up with.
