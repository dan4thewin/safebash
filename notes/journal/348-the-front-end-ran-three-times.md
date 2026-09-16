# 348 — The front end ran three times

`compile_to_object` came up next in the coverage loop: 14 cold lines, cc 12 /
cog 5. All fourteen were error handling — the parse-error map, the
parse-diagnostic check, the transform-diagnostic return.

The rubric from 346 and 347 says find the caller before writing a spec. There
is exactly one, `compile_native`, and it already holds a parsed, validated,
resolved `&Program`. It builds an SSA from it for the taint gate. Then it calls
`compile_to_object(source, trace)` — passing the raw **source string**, which
re-parses it from scratch, re-runs `type_check`, re-runs `ir_to_ssa_typed`, and
hands *that* SSA to codegen.

So the cold lines were cold for a third distinct reason. Not unreachable by
type (346), not untested (347): **pre-empted**. Every error they handle was
already reported and `exit(1)`-ed by the caller, on the same source, minutes of
CPU earlier in the same process.

Counting properly, it was worse. Three full `type_check` + `transform` runs per
native compile:

| # | site | keeps | discards |
|---|---|---|---|
| 1 | `enforce_transform_diags` | the diagnostics | the SSA |
| 2 | `compile_native` | the SSA (taint gate) | the diagnostics |
| 3 | `compile_to_object` | the SSA (codegen) | — and re-parses first |

Two adjacent calls in `cli_main` computing the identical pair, each throwing
away the half the other kept.

## Chasing the correctness question first

The re-parsed program is **unresolved**. `cli_main` calls
`resolve::resolve_commands(&mut program)` at line 170 — DD#23, rewriting `cat`
to `/usr/bin/cat` — and that mutation exists only in the caller's copy. If
codegen ran on a freshly parsed program, it should have been exec'ing bare
`cat` through `PATH`, which is precisely what DD#23 exists to prevent.

It does not. `transform/resolution.rs` re-resolves through the same
`resolve::resolve_command_path`, and `resolve_command_head` short-circuits on a
name that already contains `/`, so it is idempotent. The front-end pass exists
to make an *unresolved* external fatal early; the transform funnel does the
rewriting. Correctness holds.

Worth recording that I checked, because "the compiled artifact is built from a
different program object than the one the gates ran on" is the kind of thing
that is either nothing or catastrophic, and the only way to tell is to go and
look.

## Why it still needed fixing

Two reasons beyond the wasted time, and the second is the real one.

**The taint gate and codegen ran on different SSA objects.** Same source, same
deterministic passes, so they agreed — but nothing *made* them agree. That is
the shape AGENTS.md warns about from the deleted test harness: a gate that
validates something other than what ships is not a gate, and it fails silently
by construction.

**`compile_to_object` was a `pub`, re-exported, validate-bypassing entry point
whose doc comment claimed the opposite:**

> TRUST: This is the ONLY entry point from validated source to object code.

It did not receive validated source. It received raw source and called no
validator. The caller enforced that property; the function asserted it. A `pub`
fn asserting a property it does not check is exactly how the old fail-open
harness looked defensible — `run_bash_native` called this function directly and
compiled 49 programs the real compiler rejects.

DG ruled: pass the already-built SSA in, and fix the comment.

## The change

`transform_or_exit` replaces `enforce_transform_diags`: one transform, keeps
both halves, returns the SSA. `cli_main` threads it through `compile_native`
into `compile_to_object`, whose signature is now
`(&Program, &SsaProgram, &str, bool)`.

Three runs become one. The taint gate and codegen now share a single object
rather than agreeing by coincidence. The fourteen cold lines are gone — deleted,
not covered, because the errors they handled cannot arrive.

DG's ruling was "pass the SSA in", which on its face is about call site 3. Sites
1 and 2 are the same duplication one layer up, and leaving them would have been
narrowing the principle to the instance that prompted it.

Compile time on a 3000-line program: **0.37s → 0.21s, a 43% reduction.** I had
estimated a quarter to a third from the `--emit-ir` split; the extra came from
site 1, which that estimate did not separate out.

`compile_to_object` is now cc 4 / cog 1, from 12 / 5. Its only remaining failure
mode is the `catch_unwind` ICE path, which is what it should always have been.

## Tally

Specs `cov.306-308` pin that transform-time diagnostics still gate `--emit-ir`,
`--manifest` and `--emit-c`, since collapsing the three runs moved where that
gate sits relative to `maybe_emit_alt_output`. They invoke the flags directly
rather than through `run_sash`, because the flag is the point.

**5338 passing / 13 skipped / 0 failing, predicted exactly. 83 units.**

## Three cold-line diagnoses in three commits

346 unreachable by construction → delete the variants.
347 genuinely untested → and the gap was hiding a bug.
348 pre-empted by the caller → delete the duplication, not the symptom.

Same measurement, three different meanings. The number tells you where to look
and nothing more; every one of these was settled by reading the caller.
