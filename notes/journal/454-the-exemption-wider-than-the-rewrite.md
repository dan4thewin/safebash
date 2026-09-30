# 454 — The exemption that was wider than the rewrite

Coverage triage found a P1. That is the whole shape of this session.

## The cold lines were telling the truth about the wrong thing

`coverage.db` was fresh at `0784f56`, and the two coldest functions in the
ranking were `try_nullary_intrinsic` (15 cold lines) and its C twin
`try_process_intrinsic_c` (11). Reading the cold lines, the pattern was clean
enough to be suspicious: in **both** backends the cold arms were exactly
`getpid` and `getcwd`, while `getppid`, `random`, `argc`, `signum` and
`epochseconds` sat warm beside them. One gap, two backends, 26 lines.

The obvious hypothesis was a spec gap. I wrote `p=$(getpid)`, ran it, got a
correct pid, and would happily have filed "add a spec" — except that
`cov-probe` said the line was **still cold**. The script's own header warns
about this and I nearly skipped it.

The reason took a while to find: there are **two** nullary-intrinsic tables in
native codegen, `try_nullary_intrinsic` (string) and `int_intrinsic_i64`
(i64), and `getpid` is cold in both. Tracing upward,
`transform/expr_handlers.rs::magic_intrinsic_var` maps `"getpid" => "$"`.
Both spellings are intercepted in the transform, so those arms **cannot** be
reached. `value_handlers.rs:689` already says so in a comment.

`getcwd` probed at count 1 — genuinely just unexercised.

So: one dead-by-construction arm and one real spec gap. Neither is a bug. I
went to widen the probe set across syntactic positions to be sure, and that is
where the session actually started.

## Eight positions, one of which killed the program

`for v in $(getpid)` came back rejected — but by the **SSA taint backstop**,
not the validator. Two layers disagreeing about the same program is worth a
minute, so I took the taint out of it:

```sh
for v in $(getcwd) { attest v … ; echo "$v" }
```

```
sash: bug: bare command passed to sash_exec (expected absolute path)
$ echo $?
127
```

Compiles clean. Dies at runtime. All eight scalar intrinsics, both backends —
`--emit-c` emits `const char* __t1_argv[] = { "getcwd", NULL }` in plain
sight. `getenv` is the same shape, masked by SB5020; `--no-taint` confirms it.

Two rules, each locally correct, disagreeing about one name.
`try_dispatch_cmdsubst` rewrote **any** `$( )` for-in source into a pipeline
without looking inside. `resolve.rs::INTRINSICS` exempts intrinsic names from
DD#23 path resolution — right, because an intrinsic has no path — but the
exemption presumes a downstream intrinsic lowering that this path never
provides. The bare name rode all the way to `sash_exec`.

The runtime guard is the hero here. Without it this is a silent `exec` of
whatever `getcwd` happens to resolve to.

**DD#111 documents this exact failure** for `$(echo hi; status)`, down to the
message and the 127. The for-in source is the second instance of the same
class, and it was still open. The exemption is wider than the rewrite that
justifies it.

## DG corrected two of my three conclusions

I reported the bug with two fix options and two quieter coverage findings. The
reply:

> probably any for loop on a single scalar, quoted or not, should be an error;
> you said `cmd | while read -r` above, and that seems wrong becase of the
> switch and read vs. readline. Later you say transform rewrites getpid /
> status to `$` / `?` but we have a no-magic-variable rule - any desugar must
> go the other direction

Three things, and I had two of them wrong.

**The comment.** I had quoted `while read -r x` out of
`try_dispatch_cmdsubst` as if it described the code. It describes neither
half of the code: the path emits `SsaInst::ReadLine`, and per DD#99
`readline` binds a scalar per line while `read` is NUL-fields-to-array — and
**neither takes flags**. The comment named a flag sash does not have, attached
to the wrong member of the pair. I repeated it uncritically because it was
already in the file.

**The desugar.** I had called `magic_intrinsic_var`'s `"getpid" => "$"`
mapping *dead by construction* and proposed ICE-ing or deleting the codegen
arms it starves. DD#111 says the opposite in as many words: *"Variables are
not magic in sash… `$?` is punctuation sugar for the `status` intrinsic… The
intrinsic is the real thing."* `nullary_magic_var`'s own doc even says "DD#111
reversed that direction". The parser is right to fold `$$` into `getpid`; the
transform unfolds it **back**. Deleting the starved arms would have entrenched
the violation and destroyed the evidence. The cold lines were reporting a real
defect — just not the one I read them as.

**The scalar for-in.** I measured the matrix and found what DG was pointing
at. `for v in $s` was SB5009, but `for v in "$s"` and `for v in "$(getcwd)"`
were **accepted**. Quoting a scalar *granted* it iteration. And `ws.q1`–`ws.q3`
already reject quoted *collections* — so the family was consistent everywhere
except this one place.

## The rule that landed

Not "reject intrinsics in for-in". The general one: **a for-in source must
denote a collection.** A single scalar could only ever run one iteration.

`check_for_word` delegates to `for_source_scalar`, total over `ExprKind` and
carrying the `do not add a wildcard` marker. That is the point, not decoration:
the bug *was* a guard keyed on one variant, and `contrib/totality-check.py`
now makes the same regression a pre-commit failure.

Three distinctions the classifier has to hold:

- **Quoting is not the discriminator.** `"$v"` is the same scalar as `$v`.
- **A literal word is not a scalar in this sense.** `for v in a b` is a list;
  `for v in a` is its degenerate case. Both stay legal.
- **`$( )` splits on its contents.** A command — external or a `BuiltinCmd`
  like `echo` — is a line stream, legal by DD#28/#40. A value intrinsic is a
  scalar, SB5009. `is_scalar_value_intrinsic` is *derived* from `INTRINSICS`
  minus `argv`/`keys`, so a name added to the exemption list cannot quietly
  acquire the wrong shape.

## The sweep found the spec I did not predict

Predicted three casualties (`cov.226`–`cov.228`). Got four. `cov.173` asserted
that `for v in pre$s*post` globbed — and `try_dispatch_glob` accepts only a
bare `Literal` containing `*`, never a `Concat`. It really is one scalar word.
The rule was right; the spec had been pinning a shape the code never had.

I only saw it because I ran the sweep to enumerate the fallout instead of
trusting my grep for it.

7129 → 7147, +9 specs × 2 modes, predicted and matched. Zero failures.

## The complexity gate did its job too

`for_source_scalar` landed at cc 18 / cog 7 — rejected: not a dispatcher by
the delta test, above 3× the mean on cc. Pushing every conditional down into
`for_source_scalar_simple` / `_param` / `_parts` / `_cmdsubst` left the parent
a flat 14-arm dispatch at **cc 11 / cog 1**. It reads better than what I first
wrote, which is the argument for the gate.

## What I am carrying forward

The two coverage findings survive, reclassified:

- `getcwd` in a for-in was never a spec gap at all.
- The 26 cold `getpid`/`status` lines are **not dead code to delete**. They are
  the DD#111 violation, visible as coverage. Reversing the transform desugar —
  keeping the intrinsic canonical so codegen's existing arms lower it — closes
  them by making them live. That is the next piece of work, and it is DG's
  correction, not my plan.

The thing I want to remember: the cold lines were a true signal read the wrong
way round. I took "this arm never runs" to mean "this arm is unnecessary",
when it meant "something upstream is doing the wrong thing". A coverage hole
is a question, not an answer.
