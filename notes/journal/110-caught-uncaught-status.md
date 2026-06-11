---
title: Caught/uncaught $? model — fix for if/while/for/case
date: 2026-06-11
---

# Caught/uncaught $? model

DG articulated the rule while reviewing my fix proposals for a bug
I found in spec/control_flow.sh:

> `$?` is only meaningful either in an uncaught context or as a
> capture as the very next command. An uncaught error causes a sash
> program to exit with the uncaught status. In a caught case, the
> status is either immediately captured or discarded. The if,
> while, and for constructs all catch the status.

(case is in the same family.)

I had been justifying fix options with "match bash". DG's framing
makes the actual contract explicit: $? has two meaningful sinks
(uncaught → program exit, or immediately-next-command capture),
and catching constructs are sources of "discard."

## What was wrong

Two implementations were each missing one half of the model:

**Codegen** never reset `exit_status_var` on construct exit. So:
```
sash -c 'while false; do :; done'  →  exits with 1 (condition rc leaked)
sash -c 'if true; then false; fi'  →  exits with 1 (body rc leaked)
```

**Validator** SB5012 transitioned state→Undefined after If and
While, but For and Case fell through to `check_dq_leaf` and didn't
transition. So:
```
sash -c 'true; for x in a; do :; done; echo $?'  →  passed validation
                                                   (should be SB5012)
```

The two halves combined meant: catching constructs caught nothing
at the kernel-observable layer, AND silently passed `cmd; for/case;
$?` reads that the SB5012 contract was supposed to forbid.

## Discovery path

Spec coverage was at 60% line. I started a push toward the 100%
ceiling (modulo fault injection / test scaffolding / 23 ICE arms
slated for typestate refactor). The first targeted file was
`transform/control_flow.rs` because it had the largest gap.

I wrote `spec/control_flow.sh` with 32 tests across while/case/for/if.
Then 10 of them failed under native. Per AGENTS.md "Bug policy" —
stop everything. Three filed bugs:

1. while/C-style-for exit code = 1 (this fix)
2. case patterns `?` and `[...]` silently never match (separate)
3. `for x in "$@"; do echo "$x"` ICEs in CST taint pass (separate)

DG asked me to talk about (1). My first attempt framed the fix as
"match bash at program exit". DG pushed back: that doesn't make
sense given SB5012. I verified that SB5012 forbids `$?` after if/while
(but not — gap! — after for/case). DG then stated the model verbatim,
and the fix collapsed to a single principle.

## Fix

**Codegen.** At the start of any catching construct's exit block,
reset `exit_status_var` to 0. Labels recognized:

```rust
fn is_catching_construct_exit(label: &str) -> bool {
    matches!(label,
        "if.merge" | "if_bless.merge" | "while.exit"
        | "for.exit" | "for_cmd.exit" | "case.merge")
}
```

Applied in `sash/src/codegen/entry.rs::emit_all_blocks` (Cranelift)
and `sash/src/emit_c.rs::emit_block` (C oracle). The two emitters
share the same predicate by convention — comment in each cites the
other.

**Validator.** Added `check_dq_for` and `check_dq_case` next to the
existing `check_dq_if` and `check_dq_while`. All four follow the
same shape: walk inner statements with appropriate state, then
`*state = StatusState::Undefined`. Dispatcher updated.

## Why discard, not propagate

Bash propagates last-body-cmd rc out of if/while/for. Sash discards.
The divergence is intentional because:

- The validator already forbids user observation via $? after these
  constructs.
- The only remaining observation point is program exit.
- A "0 on construct exit" rule is uniformly defined: the construct's
  exit code never depends on which condition value caused the loop
  to terminate, or which arm of an if/case ran. Predictable.
- Users who want the body rc write `if cond; then body; status=$?;
  fi; exit $status` (or similar) — the immediately-next-command
  capture is exactly the second meaningful sink in DG's model.

## Verification

- `cd sash && cargo test --release --test c_tests` → 67/67 (the
  differential oracle stays green, so Cranelift and C agree).
- `spec/run.sh SPEC_MODES=native` → 320 passing, 3 failing. The
  3 are cf.c7/c8/f4, all blocked by the other two filed bugs.
- 291 baseline native tests still pass (no regressions).

## Method note for future bug investigations

I started the fix-options discussion in default "match bash" mode.
DG corrected. Once I had verified DG's principle against the actual
SB5012 implementation, the fix collapsed from three vague options
to one obvious answer. The validator gap I found mid-investigation
was bonus: if I'd gone straight to "fix the codegen" I would have
left For/Case still silently broken.

The pattern: **principles before patches.** When DG offers a model,
verify it against the code, then re-derive the fix from the model.
Don't carry forward earlier reasoning that wasn't grounded in the
model.

## References

- `notes/tasks/bug-caught-uncaught-status-leak.md` — task entry
  (status: done)
- `sash/src/validate.rs:763-...` — SB5012 state machine
- `sash/src/codegen/entry.rs::is_catching_construct_exit`
- `sash/src/emit_c.rs::is_catching_construct_exit` (mirror)
- `spec/control_flow.sh` — 32 tests covering the model
- `notes/tasks/bug-case-pattern-glob-not-validated.md` — companion
  bug (cf.c7/c8)
- `notes/tasks/bug-for-in-dollar-at-taint-ice.md` — companion bug
  (cf.f4)
