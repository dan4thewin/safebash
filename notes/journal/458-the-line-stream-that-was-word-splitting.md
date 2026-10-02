# 458 — the line stream that was word splitting

Coverage mining, continued from 457. The ranking put `merge_attest_branches` on
top and I specced it; the next lead was `detect_array_iter`'s `DoubleQuoted`
arm, cold on **both** branches — never called with a quoted word at all.

That looked like dead code. Every quoted for-in source I tried was rejected
before transform: `"@a"` is SB5026, `"${a[@]}"` is SB4038, `"$(keys m)"` is
SB5026, and neither `--no-taint` nor `--no-taint-cst` bypasses any of them. The
arm dated from a 2026-06-01 refactor; SB5026 arrived 2026-09-30 with the for-in
scalar fix. Leftover, surely.

It was not. `is_dynamic_single_word` counts `DoubleQuoted` as dynamic, so
`for v in "hello"` reaches the arm — compiles, prints `hello`. A live arm with
no spec, not a dead one. **A cold line is a hypothesis**, and I had spent four
probes building a conclusion on top of one instead of testing it.

## What the next probe found

Looking for a spelling that would reach the sibling cold branch, I tried a
multi-statement capture and it printed the wrong answer:

```
for k in $(echo a; echo b) { echo "[$k]" }
bash: [a] [b]        sash: [b]
```

The capture itself was fine — `str s=$(echo a; echo b)` gives `a\nb`, matching
bash. Only the loop was wrong. `try_dispatch_cmdsubst` rewrites
`for x in $( … )` into `cmd | while readline x`, and it builds **one pipeline
stage per statement in the body**. A `;` sequence became a `|` pipeline, so
`echo a | echo b` discarded `a`.

Two more fell out of the same site:

- `$(echo a | cat)` — a real pipeline is a `StmtKind::Pipeline`, not a
  `StmtKind::Command`, so the stage builder returned `None` and
  `build_pipeline_stages_from_cmdsubst` **silently skipped it**. Zero stages,
  zero iterations, no diagnostic. bash prints `a`.
- `$(g)` on a user function — `expr_to_string(&c.name)` with no DD#23
  resolution, so the bare name reached `sash_exec` and the compiled program
  aborted, rc=127. Same class as `bug-fn-capture-in-test-operand-ice`: a third
  position that never upheld the DD#81 lifting invariant. The site's own
  comment describes that exact failure happening once before, to value
  intrinsics, and the fix then was to add one name to a rejection list.

Both the sequence and the user-function cases reproduce in plain, flag-free,
fully-taint-checked code.

## The polarity, again

`for_source_scalar_cmdsubst` **allowed by default**. It rejected
`is_scalar_value_intrinsic` and let everything else fall out of the bottom as a
collection — every external command, builtin, pipeline, sequence and user
function. That is the third allow-by-default guard in recent memory to be
hiding something, and the shape is always the same: a named list of rejections
with the interesting cases underneath it.

I reported it with three fix options — route the capture through the normal
capture path, fix the streaming lowering in place, or reject the broken shapes.
All three took for granted that `for x in $(cmd)` should iterate *something*.

DG did not:

> capturing the whole `$( )` first as though it were assigned to a variable is
> to me the least surprising — if I wanted streaming I'd pipe to
> `while readline`; I don't think this iterates lines — sash has no word
> splitting; read to array and match are the mechanisms for that. Given that,
> this looks like a for loop over a single value, which we already have an
> error for, yes?

Yes. And the evidence had been in the spec file the whole time. `cov.61` pinned
`for x in $(echo a b c)` producing `out = "a b c"` — **one iteration** — and its
comment called the divergence from bash deliberate. `fs.8` pinned the same
thing and justified it: "a BUILTIN command writes to a pipe, and its output is
iterated by line". The construct had never been a loop. It was a one-iteration
wrapper around a value, which is precisely what SB5009 exists to reject, and the
only thing that made it *look* like a loop was splitting the capture on
newlines — word splitting on a different delimiter, in a language whose
defining rule is that it has none.

Three of us had read that comment. Nobody had read it as a contradiction.

## The change

The rule is now stated in the positive. `is_scalar_value_intrinsic` becomes
`is_collection_intrinsic` — `argv` and `keys`, the two that denote a collection
rather than running anything — and everything else is a capture. Deny by
default, exception list of two.

`$(< file)` went with it. It had the same exemption, worded the same way ("a
stream of lines, like an external command"), and already bound the entire file
in one iteration.

104 lines of lowering deleted: `try_dispatch_cmdsubst`,
`build_pipeline_stages_from_cmdsubst`, `build_cmdsubst_stage`,
`emit_cmdsubst_pipe_while_read`. The dead-code warnings then named two unused
imports, which is the `pub(crate)` discipline paying out.

One thing I had to not break. `$(argv; CMD)` is already rejected by **SB5025**,
whose complaint — the other statements would be silently discarded — is more
specific than the source-shape rule's, and DG settled that form on 2026-09-29.
SB5009 preempted it and six specs went red. The fix is a deferral: if any
statement in the body is an argv splat, this rule stays quiet. `keys` has no
sibling rule, so `$(keys m; …)` takes the general case.

## Gate

Sweep **7293**, predicted two independent ways before running (7275 + 12 for
the `sw.*` specs carried in from 457 + 8 for `fs.10`–`fs.13` − 2 for the
retired `cov.61`; and the source-derived `plan=` sum). Zero failing. `mk test`
rc=0. The retirement the purge announced was exactly the one I expected.

## What I want to remember

The dead-code claim was wrong and I nearly shipped it as a finding. Three
rejected spellings felt like proof; the fourth compiled. Failed probes are
evidence about the probes.

And the comment that justified the behaviour was the best available evidence
that the behaviour was wrong. "A deliberate divergence from bash" in a project
whose stated goal is byte-identical output is not an explanation, it is an
unanswered question someone wrote down and moved past.
