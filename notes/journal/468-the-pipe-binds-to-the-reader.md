# 468 — the pipe binds to the reader

DG, reading my filing:

> your `readline mid` is not part of a pipe at all, so should
> `repoint_readlines_to_pipe` run in that case? shouldn't `readline` without a
> pipe just default to fd 0?

Yes to both, and that is the entire fix. I had filed three defects; one of
them was not a defect, one was the real bug stated the wrong way round, and
the third was a missing diagnostic rather than a lowering fault. DG's two
questions sorted them in one line.

## The model I got wrong

I had reported this as a bug:

```sash
/bin/printf 'a\nb\n' | while readline v; do echo "l:$v"; /bin/cat; done
```

bash prints `l:a` then `b` — the `cat` drains the pipe. sash prints `l:a`
then `l:b` — the `cat` reads the *parent's* stdin and the loop reads both
lines. I called that a divergence. DG:

> `readline` or `while readline` without a leading pipe would read fd 0;
> `cmd | while readline v` would read the output of `cmd`; a `/bin/cat`
> within that `cmd | while` would read fd 0. That seems as it should be, are
> you saying it is not?

He is right, and it is better than bash. The rule is **a pipe binds to the
reader, not to fd 0** — now DD#122. Everything follows from it, including
why the bug I *did* find is a bug.

It follows from a property that is the whole reason the shape exists:

```sash
int n=0
/bin/printf 'a\nb\nc\n' | while readline v; do (( n = n + 1 )); done
echo "n=$n"
```

sash prints `n=3`. **bash prints `n=0`** — its most notorious pipeline
gotcha, the loop being a subshell. safebash runs the loop in the parent on
purpose. And once the loop is in the parent, the pipe *cannot* be delivered
as fd 0: redirecting the parent's stdin would leak to every later statement.
So it attaches to the reader instead. A command in the loop body is not a
stage, so it inherits the parent's stdin, which is exactly what DG said.

I derived the opposite model by reading `lower_pipe_while_read` and
reasoning from the mechanism. The mechanism is right; my inference from it
was wrong, twice. **DD#122 exists so the next reader does not have to
guess** — the rule was implemented and never written down.

## The bug, stated correctly

Not "the loop fails to redirect fd 0". It is:

```sash
/bin/printf 'x\n' | while readline a; do echo "A=$a"; done
readline mid                       # no leading pipe — must read fd 0
echo "MID=[$mid]"
/bin/printf 'y\n' | while readline b; do echo "B=$b"; done
```

`MID=[]`. Delete the *first* line and the identical `readline mid` reads fd 0
correctly. Exit 0, no diagnostic, both backends.

`repoint_readlines_to_pipe` walked **every block lowered so far** and
rewrote every fd-0 `ReadLine` to the sentinel. Its own comment admitted it:
*"Walks every block, not just the loop body, because the header/body split is
not known here."* So the second pipeline's repoint reached backwards past its
own stage and captured a `readline` belonging to no pipeline at all.

The fix does not need the header/body split. It needs to know which
instructions this lowering produced — record every block's instruction count
before lowering (`mark_ssa`), then visit only what is past the mark
(`for_each_new_inst`). Nested pipe-whiles fall out for free: the inner loop
repoints its own readers first, and the outer `fd == 0` guard then skips
them, because an inner reader belongs to the inner pipe.

**The array half had the identical defect**, in a different function.
`rewrite_lastpipe_consumer_fds` has the same whole-program walk for
`read arr`. I nearly shipped a spec that did not catch it: my first `pf.25`
had no `cmd | read arr` pipeline in it, so the buggy function never ran and
the spec passed against the unfixed compiler. Checking that a new spec
*fails* before the fix is what caught that — this is the second time in this
file's history that a spec I believed in turned out to assert nothing.

## The initialiser that made native accidentally right

With only one pipeline the stray sentinel reaches a `readline` before any
pipe is captured, and the two backends disagreed:

| | init | stray `fd = -1` does |
|---|---|---|
| native | `pipe_read_fd_var = 0` | reads **stdin** — looks correct |
| `--emit-c` | `__pipe_read_fd = -1` | **aborts**, rc=99, rustix `fd != u32::MAX` |

Native was not right, it was lucky: `0` is a valid fd. One sentinel should
have one spelling, so both are `-1` now. After the scoping fix the value is
unobservable — an instruction carries `-1` only when the pipeline instruction
that defines the variable was emitted immediately before it — but the
divergence was the thing that made the bug invisible on the backend everyone
runs.

## The two new rejections

If a pipe binds to a reader, a final stage with no reader gets nothing. Two
shapes could reach that state, and both now fail at parse time.

**SB4022, extended to the final position.** `cmd | { … }`, `| if`, `| for`.
These are forked, so they *do* read the pipe — and then throw away everything
they assigned:

```sash
/bin/printf 'a\n' | { readline g; }           # ran, read a line, g unset
/bin/printf 'a\n' | if readline g; then … fi  # took the branch, g unset
```

The read succeeded, the branch was right, the data vanished. That is the
bundling **DD#38** rejects `( … )` for — `( readline g; )` is SB4014 in every
position — arriving through a different syntactic door, and getting the same
answer. SB4022 already rejected a compound in a non-*final* stage, where the
`|` parses as a postfix on the compound; this completes it to both positions,
with SB4022's existing advice. *Wrap it in a function* is not a consolation
prize: `fn f { /bin/head -n1; /bin/cat }` as a stage is byte-identical to
bash's `{ /bin/head -n1; /bin/cat; }`, verified both ways.

**SB4073, new.** `cmd | while (( n < 1 )); do /bin/cat; done` ran the loop in
the parent with nothing bound to the pipe, so the producer's output went
nowhere. Consistent with the rule, and almost certainly a mistake.

The reader search is `Statement::binds_a_pipe_reader`, and it deliberately
does not descend into a nested `Pipeline`, a `Function` definition or a
`Redirected` body — a reader in any of those binds to something else. It is
scoped to match the repoint, which is the same question asked of the SSA.

`is_while_read` in the transform stays structural, `matches!(While(_))`. It
was wrong before because it was the only thing deciding; it is right now
because SB4073 guarantees what it assumes. That is the DD#116 distinction
exactly — it was a rival, and it is a backstop.

## DD#116, the other half of the session

DG: *"delete the rivals; finish the rubric."* Three deleted, four kept with
the property named.

Deleted:

- `detect_array_iter`'s `DoubleQuoted` arm — it recursed, so `for v in "@a"`
  would have iterated. SB5026 rejects `"@a"`. The arm stated the **opposite**
  of the spec.
- the same function's `Redirected` arm — `$(keys m < f)` would have iterated;
  SB5073 rejects it.
- `attest_arith_near_miss`'s `if classify_attest(…).is_some() { return None }`
  — the sole caller reaches it only on the `else` of its own
  `classify_attest`, so re-asking could only answer `None`. Confirmed by
  running all three attest-all-with-arith shapes: each reaches SB5054 through
  the arm below it.

Kept, each with a comment naming which of DD#116's three properties it holds:

| site | property |
|---|---|
| `detect_array_iter`'s `return None` guards | specification fidelity — "only `$(keys NAME)` iterates" *is* the rule |
| `build_function_pipeline_stage`'s recursion check | specification fidelity — inlining's termination is stated where it is relied on |
| `attest_arith_near_miss`'s `[] => None` | loud-vs-silent — else a zero-name lead gets the attest-all message |
| `attest_scalar_form_error`'s map arm | loud-vs-silent — else a scalar attest on a map advises `attest x in m`, itself an error |

## Numbers

`mk test` rc=0. Sweep **7543 → 7565**, the predicted +22 (11 specs,
dual-mode), zero failures. Four spec-integrity checks clean.

## A stale todo outlived its decision

Listing what was still open, I named SB5067's re-decision as blocked on DG,
sourced from a session todo saying *"BLOCKED ON DG. Four choices presented
2026-09-30."* DG: *"I think the SB5067 question was already decided and
implemented."* He is right. DD#121 carries an addendum **dated that same
day** — *"SB5067 re-decided: a net-effect rule"* — and `validate.rs:8449`
implements it, crediting `(DG, 2026-09-30)` in the comment.

So the todo was written the day the question was answered, and never
updated. I then compounded it by filing a task whose stated rationale was
that the rule's comment *"argues from a property the compiler no longer
has"* — when that comment is precisely the record of the new decision. I had
grepped for `SB5067` in `notes/` and taken the absence of a task file as
evidence the question was open, without reading the one DD the grep did hit.

The check that would have caught it is the one AGENTS.md already prescribes:
**read `notes/decisions/` before believing anything about what is
intentional.** A todo is a note-to-self, not a source of truth; the DDs are.

## What I'd take from this

Three times now I have inferred a design rule from the code that implements
it and got it wrong. The mechanism tells you *what* happens, never *what was
intended*, and the gap between them is exactly where a bug hides — here, the
fd-0 behaviour I called a defect and the whole-program walk I nearly
justified as "it needs to find readers in the body". DG settled both with one
sentence about what reads what. The lesson is not "ask more"; it is that an
unwritten load-bearing rule costs more than the DD would have — and the
SB5067 slip is the same failure from the other side: a rule that *was*
written down, in the right place, that I did not read.
