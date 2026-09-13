# 281 — The backstop that never ran

DG's directive was three items: **fix p1 and plan, audit 26.** The first two
went as filed. The third found a taint bypass.

## 1. The P1: fail-open TODO arms in emit_c

The task offered three options and recommended (A) *hard compile error* plus
(C) *exhaustive match*. I started implementing (A) and hit the fact that
`emit_c` returns a bare `String` with no error channel. Before plumbing one,
DG asked the question that killed the option:

> why would there be unsupported in C backend?

There is no DD sanctioning a c-backend subset. The c backend is a full
backend held to byte-identical parity, and it is also the differential
oracle. So a fallback arm can only mean one of two things: a **real gap**,
which is a bug to implement, or an **impossible state**, which is a compiler
bug. There is no third category, and a user-facing "unsupported" diagnostic
would have manufactured one — telling a user their program is unsupported
when the truth is that the compiler has a hole.

So (C) alone, with ICE terminals. `emit_command_body` is now exhaustive over
all 41 `BuiltinCmd` variants: 7 direct arms, 3 delegating group arms, and 3
ICE arms covering the 24 that cannot arrive, each with provenance. Rather
than guess at those 24 I traced every one: `RedirectPush`/`Pop` are
intercepted by `emit_compound_redirect`; the 12 value intrinsics are resolved
in `transform/expr_handlers.rs`; **`BuiltinCmd::Write` has zero matches
anywhere in the codebase** because `rewrite_write_as_echo` rewrites it before
codegen. `emit_inst` got the same treatment over `SsaInst`.

The three `-> bool` helpers became `()` with ICE `_ =>` arms — the boolean
chain was the mechanism that let an unhandled variant fall through silently.

I proved the gate actually gates by adding a dummy variant to `BuiltinCmd`:
`error[E0004]: non-exhaustive patterns`. Reverted it. Grouped `|` patterns
count as one decision, so a 24-variant arm costs cc+1 and the complexity gate
never noticed.

## 2. The plan drift

Two files, `wordsplit.sh` (118 vs 123) and `collparam.sh` (34 vs 37). Fixed.

## 3. The audit — and the bypass

Of the 26 c-mode skips, **16 were stale**. Ten of them (`cov.26`–`cov.75`)
measured **byte-identical in both backends**; their guards said nothing more
than "native." — they were never a claim about c at all, just the mode the
author happened to run.

`89.8` cited a bug fixed in `29b0b43`. `55.31` cited a message-format gap it
does not have: c matches native exactly. Only its neighbour `55.30` is
genuinely native-only, and only for the *second* stderr line.

Then `sw.4/5/8/9`, which run under `--no-taint-cst` to exercise the SSA taint
backstop in isolation. Native rejects all four, rc=1. In c:

```
sw.4  rc=0  out=[/home/dg]
sw.8  rc=0  out=[/HOME/DG]
sw.9  rc=0  out=[dangood-ubuntu-dev]
```

**The c backend printed the tainted values.** The cause is five lines long:

```rust
fn emit_c_to_stdout(program: &crate::ir::Program) {
    let type_map = type_check::infer_types(program);
    let (ssa, _) = transform::ir_to_ssa_typed(program, type_map);
    print!("{}", emit_c::emit_c(&ssa));
}
```

The native path calls `run_ssa_taint_gate` between the transform and codegen.
The c path never did. `run_ssa_taint_gate` had exactly one call site, and its
doc comment explains why it exists: *"Kept as its own function so
`compile_native` stays flat (complexity gate)."* It was extracted for the
native path's benefit and the c path was never given it.

Under default flags layer 1 (the CST pass) still rejects, so this was not
exploitable in normal use. But a backstop exists precisely for when layer 1
has a gap — `validate.rs:2569` documents a case where the two layers cover
for each other — and the c backend is the oracle. An oracle that runs a
different set of gates is not comparing the same compiler.

Fixed by calling the gate on the c path. Fail-first: reverting `lib.rs` fails
exactly those four in c; restoring passes them.

## What the audit is really telling us

That is **seven** stale native-only guards across three sessions, every one
of them hiding something. The pattern is now unambiguous, and it is
structural rather than careless:

> A skip is a bug report that suppresses its own evidence. It never fails,
> so it never expires, and the assertion that would refute it is the exact
> assertion it prevents from running.

Worse, the ten `cov.*` guards show the failure mode does not even require a
bug to start it — "I ran this in native" silently becomes "this does not work
in c" the moment it is written down as a guard, and nothing ever revisits it.

The 10 survivors are now labelled with what was measured and when, and the
one real gap points at its task (`c-dd55-abort-channel`) rather than standing
in for it.

## A note on my own measurement

My first probe of `55.30/55.31` reported rc=125 and no stderr in c, which
looked like a behavioural divergence far worse than the filed gap. It was my
probe: it did not export `SASH_RT_LIB`, and `common.sh` returns a **125
sentinel** when it cannot find the runtime archive. The identical rc for two
different programs is what gave it away.

Re-measuring with the variable set showed near-parity. Had I trusted the
first result I would have filed a bug that did not exist — the same class of
error as trusting a stale guard, just in the opposite direction.

## Coverage

c **2041 → 2057 passing**, skips **26 → 10**, 0 failing. 16 flips, all
skipped→passing. The gap reconciles exactly: 2158 − 2057 = 101 = 91
native-only ids (convert.sh 88, trace.sh 3) + 10 c-skips.

## Filed, not fixed

`value-intrinsics-exec-path-twin` (P1). `join`, `basename` and `dirname` are
in neither `BUILTINS` nor `VALUE_ONLY_INTRINSICS`, so in statement position
they PATH-resolve and exec the system binary. The other value intrinsics
escape only because no PATH twin happens to exist — the safe set is
**host-dependent**. DG chose scope B: add all 11 to both lists.
