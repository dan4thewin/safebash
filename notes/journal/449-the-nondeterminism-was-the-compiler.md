# 449 — The nondeterminism was the compiler

2026-09-30. Fixing `bug-caught-attest-all-fail-path-undefined-handle`, the P1
SIGSEGV left open by DD#121.

## The filing was confidently wrong about one thing

It recorded the crash as layout-sensitive:

> The compiled binary is byte-identical across compiles (same sha256), but
> whether it faults depends on process memory layout. […] **Always reproduce
> under `env`**, and treat a clean direct run as meaningless.

That advice worked, which is the problem — it worked for the wrong reason and
so it stayed unexamined.

The first thing I noticed was that my transcription of the reproducer, using
`{ }` braces, did not crash, while the filing's `then/fi` spelling crashed
10/10. Those parse to the same AST. A syntax that cannot matter, mattering, is
not a curiosity to route around; it is the finding.

Compiling one unchanged source six times:

```
compile 1: sha=7c6a03c943fe rc=139
compile 2: sha=131fb478a1f9 rc=0
compile 3: sha=7c6a03c943fe rc=139
compile 4: sha=131fb478a1f9 rc=0
```

Two binaries from one source. Each is perfectly deterministic; which one you
get was not. The `env P=x` ritual never perturbed the runtime — it just rode
along with whichever binary that compile had produced. The brace/`then` "syntax
sensitivity" was two compiles wearing a disguise.

So the headline defect was never "a crash under some memory layouts". It was
**the compiler does not produce the same output twice**, and the crash was one
of its two outputs.

## Root cause

`codegen::types::find_array_slot` resolves a collection by *name*. Absent
`a_0`, it took `vars.keys().find(|k| k.starts_with("a_"))` — an arbitrary
`HashMap` key, and `HashMap` ordering is seeded per process.

The only thing that ever puts a second `a_*` key in that map is
`emit_attest_retag`, which implemented DD#61's clean-retag by
`declare_var`/`def_var`-ing a fresh pointer pair. It runs in the **pass block
only** — that is where `control_flow.rs` emits `AttestRetag`. So on the failing
path `a_2` was a key with no definition, and a coin-flip decided whether
`sash_table_len` received it.

The differential oracle localised it in one step: `--emit-c` never faulted,
because `emit_c.rs` has always treated `AttestRetag` as a no-op — "versions
collapse to one slot". Native was the only backend that gave the retag a
runtime existence. That is the oracle doing exactly the job it was built for.

The filing's "undefined phi incoming edge" hypothesis was wrong in mechanism:
there is no phi. `lower_if_attest` restores the pre-state at the merge, and
that is *correct* — the retag is a compile-time clean-tag valid only on the
passing path, and DD#121's SB5089 now requires the subject to be resolved on
every falling-through path anyway. What was there instead was a `let _then_snap
= state.snapshot();` feeding nothing: a breadcrumb from an abandoned attempt at
the phi the filing went looking for.

## Fix

Two changes, each sufficient alone, both landed because they close different
halves:

1. `emit_attest_retag` **aliases** — the target version maps to the same
   `VarSlot`. A relabelling that changes nothing at runtime must not create a
   runtime value. Now identical to c.
2. `find_array_slot`'s fallback is **deterministic**: the lowest version
   present. This generalises the `_0`-first rule already above it — the base
   version is defined on every path by construction, so a version introduced on
   only *some* path cannot shadow it.

I kept both because they fail differently. (1) stops a one-sided definition
from existing; (2) stops any future one-sided definition from being *selected*,
and restores reproducible output, which is worth having on its own terms.

## The pin took three tries, and measurement caught each one

"A gate that cannot fail is not a gate" is in `contrib/coverage.sh` for the
coverage canary. It applies just as well to a regression spec, and I nearly
shipped one that could not fail.

My first spec body reused `join a` — 0/12 failures against the reverted fix.
`join` and `len` do not route through `find_array_slot`; only the splat's
`arrlen` lowering does. A spec asserting the right *outcome* through the wrong
*path* is a spec that will pass forever.

The second problem was the probability. Even with the splat, one compile caught
it about 60% of the time, because it is one `HashMap` draw. `sw.12` therefore
compiles **six times** — under 1% escape. The comment in the spec says so, and
says why re-running one binary is worthless.

Verified both directions: the pin fails 3/3 sweeps against the original code
and passes deterministically with the fix.

## Cost

`lower_if_attest` was a pre-existing complexity violator (cc 7 / cog 6 against a
5.46 gate) that my comment edit un-grandfathered. Extracting `push_attest_retag`
took it to 5/5. The ratchet worked as designed — I touched the function, so it
came down to spec.

Sweep: native 3626 / c 3408 / 13 skipped, zero failing. `mk test` green.

## What I'd carry forward

A reproducer whose behaviour depends on how you *spell* it is reporting
compile-time nondeterminism, whatever the filing says it is. And a ritual that
reliably reproduces a bug ("always run it under `env`") earns no trust from
working — it has to be explained before it can be believed.
