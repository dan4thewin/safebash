title: BUG — caught/uncaught $? leak from if/while/for/case (FIXED)
status: done
priority: P0
kind: bug
depends: []
resolved: 2026-06-11
resolution: "Fixed in this branch. Root cause was a missing implementation\nof the caught/uncaught model for $?:\n\n- **Validator gap**: SB5012 only transitioned state→Undefined\n  after If/While; For and Case fell through to check_dq_leaf\n  and didn't transition, so `cmd; for/case; echo $?` passed\n  validation when it shouldn't.\n- **Codegen gap**: catching constructs (if/while/for/case)\n  never reset exit_status_var on construct exit, so caught\n  internal statuses (condition rc, body cmd rc) leaked to\n  program exit.\n\nFix:\n- `sash/src/validate.rs` — added check_dq_for and check_dq_case,\n  each transitions *state = Undefined after construct\n- `sash/src/codegen/entry.rs` — added is_catching_construct_exit\n  helper; at start of any matching block, def_var(exit_status_var, 0)\n- `sash/src/emit_c.rs` — mirrors the same predicate; emits\n  __exit_status = 0 at start of catching exit blocks\n- `spec/control_flow.sh` cf.w1–w7, cf.c1–c12, cf.f1–f10, cf.i1–i3\n  now pass under native and c modes (29/32 in native; remaining\n  3 are blocked by separate filed bugs cf.c7/c8/f4)\n"
gates:
  - while/if/for/case rc at program exit is 0 unless overridden by
    a subsequent uncaught command ✓
  - SB5012 fires uniformly after all four catching constructs ✓
  - 291 baseline native spec tests pass (no regressions) ✓
  - c_tests differential oracle: 67/67 (no Cranelift↔C divergence) ✓

# BUG — caught/uncaught $? leak from if/while/for/case

## Model (DG, 2026-06-11)

> `$?` is only meaningful either in an uncaught context or as a
> capture as the very next command. An uncaught error causes a
> sash program to exit with the uncaught status. In a caught case,
> the status is either immediately captured or discarded. The if,
> while, and for constructs all catch the status.

(case is in the same family.)

Implications:
- Inside if/while/for/case bodies and conditions, $? is observable
  as the rc of the previous command in scope (already handled).
- After if/while/for/case, $? is **discarded** — neither readable
  by user code (SB5012 enforces) nor by the kernel as program exit.
- Program exit reflects only uncaught statuses: the last command
  outside any catching construct, or 0 if the program ended inside
  one.

## Pre-fix repro

```bash
$ sash -c 'while false; do :; done' -o /tmp/t && /tmp/t; echo "rc=$?"
rc=1                # WRONG — caught condition rc leaked

$ sash -c 'if true; then false; fi' -o /tmp/t && /tmp/t; echo "rc=$?"
rc=1                # WRONG — caught body rc leaked

$ sash -c 'true; for x in a; do :; done; echo $?'
# WRONG — accepted, should reject (For didn't transition SB5012 state)
```

## Post-fix behavior

All four catching constructs discard their inner statuses on exit:

| script | sash rc | rule |
|---|---|---|
| `while false; do :; done` | 0 | catching construct on exit → 0 |
| `if false; then :; fi` | 0 | catching construct on exit → 0 |
| `if true; then false; fi` | 0 | body's `false` was caught, discarded |
| `for x in a b; do echo $x; done` | 0 | catching construct on exit → 0 |
| `cmd; while false; do :; done` | 0 | `cmd` rc was caught by following construct |
| `while false; do :; done; cmd` | rc of cmd | cmd is uncaught |
| `false` | 1 | uncaught |

## Validator (SB5012) post-fix

All four constructs now reject `$?` reads after them:

```bash
$ sash -c 'true; for x in a; do :; done; echo $?'
error [SB5012] $? is not available here — no preceding command  ✓

$ sash -c 'true; case x in a) :; esac; echo $?'
error [SB5012] $? is not available here — no preceding command  ✓
```

## Files touched

- `sash/src/codegen/entry.rs` — is_catching_construct_exit + def_var hook
- `sash/src/emit_c.rs` — mirror predicate + emit `__exit_status = 0`
- `sash/src/validate.rs` — check_dq_for, check_dq_case + dispatcher
- `spec/control_flow.sh` (NEW) — 32 tests covering while/case/for/if

## Divergence from bash

Bash propagates the last-body-cmd rc out of if/while/for. Sash
discards. Two user-visible consequences:

1. `if cond; then critical; fi` at EOF: bash exits with critical's
   rc; sash exits with 0. To get the bash behavior, write
   `if cond; then critical; status=$?; fi; exit $status`.
2. `false; if true; then :; fi`: both languages exit 0 (in bash
   because the if-no-body returns 0 anyway).

The divergence is deliberate per the caught/uncaught model.

## References

- AGENTS.md "Bug policy" — zero tolerance for ICE / wrong-output
- `sash/src/validate.rs:763-...` — SB5012 state machine
- `sash/src/codegen/entry.rs::is_catching_construct_exit`
- `sash/src/emit_c.rs::is_catching_construct_exit` (keep in sync)
- spec/control_flow.sh — covers cf.w*/cf.c*/cf.f*/cf.i*
