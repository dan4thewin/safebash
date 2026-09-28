---
title: "SB5042/SB5043 count compile-time SsaValues, so a splat or elide at a call site is miscounted in BOTH directions"
status: blocked
priority: P1
kind: bug
depends: []
blocked_on: "DG — (1) whether a runtime arity shortfall stays unchecked; (2) separately, whether observing argc (`$(len args)`/`$#`) should keep implying variadic (DD#81 Part B says yes)"
gates:
  - "`f @q` where `q` has enough elements for a fixed-arity `f` is not rejected"
  - "`f one ${x:~}` cannot silently deliver fewer arguments than the callee's `min`"
  - "no diagnostic states an argument count that is not the count"
  - "specs for both directions, in both modes"
---

# The bug

`transform/cmd_handlers.rs:282`:

```rust
check_callee_arity(name, body, arg_values.len(), state);
```

`arg_values.len()` is the number of **SsaValues**, and both
`SsaValue::ArraySplat` and `SsaValue::ElideEmpty` are *one* SsaValue at
compile time that expands to 0..n tokens at runtime. So the number handed
to the arity check is neither the runtime count nor a bound on it, and the
check is wrong in both directions.

This is the third instance this session of one class: **a compile-time count
taken over a value that expands at runtime.** The other two were the `exec`
argv builder and the DD#84 elidability gate (both fixed, journal 441).

Both reproducers below use a callee that is **fixed-arity by every reading** —
it reads `args[0]` and `args[1]` and nothing else. No `len`, no `$#`, no
splat: `analyze_arity` returns `Fixed{min:2,max:2}`, and the diagnostics quoted
are the *fixed* ones ("expects 2 arguments"), not the variadic "at least".

## Direction A — accepts a call it has itself proven invalid

```sash
fn f {
  str a=${args[0]}
  str b=${args[1]}
  echo "a=[$a] b=[$b]"
}
str x=""
f one ${x:~}
```

compiles, and prints `a=[one] b=[]` — one argument delivered to a callee the
compiler has typed `Fixed{2,2}`. Writing the *same runtime shape* literally is
rejected:

```
f one
→ error [SB5042] function 'f' expects 2 arguments but was called with 1
```

So elision reaches exactly the state SB5042 exists to prevent, silently, and
`${args[1]}` reads out of range as empty.

## Direction B — rejects a valid call, with a false count

```sash
fn f {
  str a=${args[0]}
  str b=${args[1]}
  echo "[$a][$b]"
}
array q=(p d)
f @{q:-}
```

```
→ error [SB5042] function 'f' expects 2 arguments but was called with 1
```

`q` has two elements, so the call delivers exactly the two the callee wants.
It is rejected anyway, and the message asserts "called with 1", which is not
true of any execution. Bare `f @q` behaves identically, and a *three*-element
`q` — genuinely too many — gets the same "called with 1" rather than SB5043.
The count is not merely imprecise; it is the constant 1 for any splat.

The splat does expand at runtime — a variadic callee sees all three:

```sash
fn f { for x in @args; do echo "[$x]"; done }
array q=(p d e)
f @{q:-}        # → [p] [d] [e]
```

(`@args` is what makes that callee variadic here — deliberately not `$(len
args)`, so the evidence does not lean on the rule questioned below.)

# Why option "reject dynamic counts into fixed-arity callees" is wrong

It is the obvious fail-closed move and it kills DD#84's own motivating case.
A function with an optional trailing parameter is `Fixed{1,2}`, and

```sash
f a ${maybe:~}
```

— "pass a second argument if I have one" — has runtime range `[1,2]`, which
is entirely inside `[1,2]`. That is the feature DD#84 §(A) exists for. Any
fix has to keep it.

# The honest check

Compute the runtime range of the call's argument count:

- a plain value contributes exactly 1;
- `${x:~}` contributes 0 or 1;
- `@{a:~}` contributes 0..∞ (the array length is not known at compile time);
- `@{a:-}` contributes 1..∞ (DD#84 addendum: an empty array still yields one
  empty element);
- `@a` bare is rejected earlier (SB5030/SB5032 — DD#69 requires a
  disposition), so it does not arise.

giving `[lo, hi]` with `hi` possibly unbounded. Then diagnose only when
`[lo,hi]` and the callee's `[min,max]` are **disjoint** — that alone fixes
direction B and makes every emitted count true.

# The decision — what happens when the ranges OVERLAP but the runtime lands outside

Direction A is still reachable after the range check: `[1,2]` overlaps
`[2,2]`, so `f one ${x:~}` compiles, and with `x` empty it delivers 1.

- **A — accept, and let `${args[K]}` read empty.** Zero runtime cost. This
  is today's behaviour for the accept case, just with B's false rejection
  gone. Cost: SB5042 becomes a check that a program can walk past, and "the
  callee's `args` array is materialized short" (DD#84 §(A)) becomes a
  *runtime* fact where the DD describes it as a compile-time one.
- **B — accept, plus emit a runtime arity check** on a fixed-arity callee
  whenever the range is not a single point; abort per DD#55 on a shortfall.
  Fail-closed. Costs one compare per such call, and only for calls that
  actually have a dynamic count.
- **C — accept, and define the short read.** Say explicitly that a call may
  deliver fewer than `min` and that `${args[K]}` out of range is the empty
  string, making SB5042 a *statically-decidable-cases-only* check rather
  than an invariant. Honest, zero cost, and the largest language change.

I lean **B**: `min` is derived from what the body reads *unconditionally*, so
a shortfall means the callee is about to read a positional that was never
passed — which is the failure SB5042 names, and DD#55 already has an abort
for "a thing the program cannot proceed past". A is the cheapest and would
not be unreasonable if the runtime check proves noisy in practice.

This is DG's call: it decides whether a compile-time arity guarantee survives
contact with dynamic argument counts, and B changes generated code.

# Adjacent, and NOT part of this bug — "observing argc ⇒ variadic"

The first draft of this file used `int n=$(len args)` in both callees. DG
caught it (2026-09-28): *"the example functions in the task file are not
variadic (the use of len alone shouldn't qualify)."*

He is right that it spoiled the reproducers — the examples above are rewritten
without it — and the draft compounded the error by calling that callee
`Fixed{min:2}` while quoting an error that said "expects **at least** 2",
which is the *variadic* message. The quoted evidence contradicted the
sentence it was supporting.

The underlying rule is real and measurable. `$(len args)` alone erases the
arity ceiling:

```sash
fn f { str a=${args[0]}; str b=${args[1]}; echo "$(len args) [$a][$b]" }
f p d x y      # → "4 [p][d]"        (accepted; two arguments discarded)
```

Delete the `$(len args)` and the same call is `SB5043 ... takes 2 arguments
but was called with 4`. `$#` behaves identically — the rule is *observing the
count*, not anything about `len`.

**This is deliberate.** DD#81 Part B states it three times — under "Signature
inference" (whole-args access is "`@args`, `$(len args)` (the count; ≡ sugar
`${#args}` / `$#`), or a dynamic index"), again under "Edge rules", and again
in its "Implemented" section. Accepted, settled with DG 2026-08-25.

Measured cost of changing it (experimental two-line patch to
`cmdsub_reads_whole_args` + `sig_record_name`, built and run, then reverted —
nothing committed): **17 native spec failures across 8 files**, roughly 34 rows
dual-mode. `types.sh` 120/125/235/240/241/243/244, `functions.sh` 91/98,
`optional_args.sh` 50/51, `argv.sh` 48/50, `arithmetic.sh` 76,
`control_flow.sh` 35, `subscript.sh` 32, `coverage.sh` 255. The specified
idiom that breaks is the plain arg-counter — `fn f { echo $(len args) }`
called with arguments (75.23, 75.94, 75.98) becomes `Fixed{0,0}`, so `len` can
only ever report 0.

Changing it is a **DD#81 amendment and DG's call** — not folded into this bug,
which reproduces without it.

# Where

- `sash/src/transform/cmd_handlers.rs:282` — `inline_function_call`
- `sash/src/transform/stmt_handlers.rs:897` — `build_function_pipeline_stage`
- `sash/src/transform/stmt_handlers.rs:1260` — `capture_func_into`

All three pass a `.len()`. `check_callee_arity`
(`cmd_handlers.rs:367`) takes `call_argc: usize` and would take a range
instead; `analyze_arity` already yields the callee side as
`Fixed{min,max}` / `Variadic{floor}`.

`arity_too_few_msg` / `arity_too_many_msg` print the count, so they need the
range too — "called with 1" must not be said about a call that may pass 3.
