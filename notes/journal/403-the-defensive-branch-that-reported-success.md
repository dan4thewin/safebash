# 403 — The defensive branch that reported success

Coverage item three started as a cleanup and turned into a bug.

## The hunt

`codegen/inst_handlers.rs::handle_close` had six cold lines. Two regions: a
`let Some(arg) = args.first() else { … }` zero-arg fallback, and a
`_ => (zero, zero)` wildcard in the abort-diagnostic construction. The
function's own comment named the shape outright:

> Validation guarantees exactly one literal-fd arg; be defensive anyway.

SB4052 does guarantee it — `spec/exec.sh` cl.5/cl.6/cl.7 already pin zero args,
a non-literal arg, and two args. So the branches were dead, and the "defense"
was worse than nothing: it set `$? = 0` and `test_result = 1`, reporting
**success** for a `close` that closed nothing, and handed the abort diagnostic a
null segment.

Searching for siblings turned up a family — the same `args.first()` shape in
`handle_cd`, `emit_cd_c`, and `emit_close_builtin`. Two builtins, two backends,
four branches. `emit_c.rs`'s doc comment advertised it as a feature: *"including
its defensive empty-args branch — the validator guarantees exactly one literal
fd, but neither backend relies on that."*

## The trap I had just documented

The coverage rows for `emit_c.rs` pointed at lines 1788-1789 — the *normal*
path, not the empty-args branch. Which would have meant `cd DIR` was never
emitted at all.

`coverage.db` was stale. Commit `57a0494` shortened `emit_c.rs` by three lines
earlier the same session, so every coverage row past ~891 was off by three:
coverage line N is current line N−3. Re-derived, the cold rows are the
empty-args branch after all, consistent with the other three sites.

This is exactly the failure journal 401 recorded — *a coverage row naming a
non-executable line means the DB is stale* — and it recurred within the session
that wrote it, because I committed to a measured file and kept using the
measurement. The rule needs a sharper form: **`coverage.db` is invalidated by
any commit that changes line numbering in a measured file**, and the symptom is
not always an impossible line. Here every row was plausible; only the *meaning*
was wrong.

## `cd` was not dead code

Probing to confirm the family, `cd` broke the pattern:

```
cd bare        COMPILES -> (rc=0)
cd /tmp /var   COMPILES -> (rc=0)
```

Both accepted. After `cd /tmp; cd`, `pwd` is still `/tmp` — the bare `cd`
succeeded and did not move. Bash goes to `$HOME`.

So `cd`'s branches were not defensive residue. They were **live, reachable,
untested behavior**, and the behavior was wrong twice over: a bare `cd`
diverged from bash by silently doing nothing, and `cd /tmp /var` silently
dropped an argument where the sibling `close 1 2` is SB4052.

The codegen comment said *"`cd` with no arg — no $HOME magic under DD#53;
succeed silently."* DD#53 does establish that there is no implicit `$HOME`. It
says nothing about `cd`, and nothing anywhere records "succeed silently" as a
decision. It was an inference, written at the point of implementation, that
nobody ever ratified — and the only `cd` specs were the DD#55 abort paths, so
nothing contradicted it.

## DG's call

Presented as a fix-approach fork (the bug itself is not optional). DG chose to
reject both forms, with a plain message and no `$(getenv HOME)` suggestion, and
to land the `close` cleanup in the same commit as one coherent family.

`cd` now takes exactly one argument; zero or more is **SB4066**. Unlike
`close`'s fd, it need not be a literal — a path is not a runtime-chosen
descriptor, so `cd $dir` stays legal. Spec cd.3 pins that, so a later reading of
"mirror `check_close_arg`" cannot quietly tighten it.

With the validator strict, all four codegen branches became genuinely dead and
were replaced by assertions naming the code that makes them unreachable —
the `handle_signal` precedent. The C backend's `_ => ("0","0")` wildcard, the
mirror of native's null diagnostic segment, went the same way.

## What the shape teaches

The first two items of this campaign found dead code behind a gate. This one
found the opposite: a branch that *looked* like dead defensive residue, sat in
the same syntactic shape as three that genuinely were, and carried live
behavior. Reading the four sites side by side made them look identical;
**only execution separated them.** The comment on the live one was the most
confident of the four.

A "be defensive anyway" branch is a claim that the validator might be wrong. If
the validator is right, the branch is dead weight that answers a misuse with a
lie. If it is wrong — as it was for `cd` — the branch is what *hides* the gap,
because the program runs and reports success. Either way the branch is the
problem. That is why the codebase's position, stated in `handle_signal`, is an
assertion naming the diagnostic rather than a silent fallback.

## Gates

Sweep **6243**, predicted 6243 (+3 specs × 2 modes), 0 failing, 0 flips against
`57a0494`. `mk test` 59/27/20, up 2. Non-vacuity confirmed by reverting the
dispatch arm: exactly cd.1 and cd.2 fail, cd.3 (the `cd $dir` guard) still
passes, as it should.

Complexity fell at every site — `handle_cd` and `handle_close` are now cc 1 /
cog 0, the branches gone entirely.

Nine cold lines closed, one bug fixed, one inference promoted to a decision.
