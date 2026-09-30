---
title: "`:!` is silently ignored on a for-in splat — `for v in @{a:!}` is rejected SB5020 despite DD#91"
status: open
priority: P2
kind: bug
depends: []
gates: []
---

# `reject_tainted_array_iter` never reads `pe.taint_off`

Found 2026-09-30, while measuring the SB5067 question (whether a function may
taint a global). DG's framing named "a global only accessed by `:!`" as the
escape under discussion; measuring it showed that for a **for-in read of a
collection the escape does not work at all**.

## The defect

`sash/src/validate.rs:5411` `reject_tainted_array_iter` resolves the splat with
`splat_array_name`, which matches `pe.operator == Some("arrvals")` and returns
the name. It **never consults `pe.taint_off`**, so a trusted splat is rejected
exactly like a bare one.

```sh
array g=()
argv g
for v in @{g:!} { /bin/echo $v }   # SB5020 — the `:!` is ignored
```

On a **clean** array the same word compiles and runs, silently honouring
nothing, so the modifier is a no-op in this position either way.

## Why this is a bug and not the intended restriction

Two independent sources of truth say a for-in trusted splat is legal:

- **`sash/src/ir.rs:1287-1295`** — the DD#91 2026-09-09 addendum made `!` a
  **modifier** rather than a disposition, in a separate `taint_off` field,
  explicitly so "the two cannot be conflated again — the retired
  `arrvals_trust` operator meant 'elide AND trust', which forced the trust to
  inherit the elide's position restriction." Rejecting `@{g:!}` in a for-in
  source re-imposes exactly the position restriction that addendum removed.
- **`notes/decisions/DD91-taint-off-operator.md`** — "`@a:!` bypasses the
  bless-all requirement on a tainted array."

The parser is already correct: in a for-in source `@{g:!}` lowers to operator
`arrvals` with `taint_off: true` — a *bare* splat (the form SB5034's message
requires for a for-in source) carrying the trust flag. Only the validator half
is missing.

The position handling is otherwise right, which is what makes `:!` the anomaly:

| word in a for-in source | result |
| --- | --- |
| `@{g:-X}` / `@{g:?X}` | SB5034 (disposition is command-argument-only) |
| `@{g:~}` | SB5034 |
| scalar `${x:~}` | SB5046 (non-variadic position) |
| **`@{g:!}`** | **accepted, flag dropped** |

And `:!` demonstrably works in the other collection positions, so this is
positional, not a wholesale gap:

| position | tainted `g` | result |
| --- | --- | --- |
| array literal `array h=(@{g:!} lit)` | yes | compiles, runs |
| varargs user fn `f @{g:!}` | yes | compiles, runs |
| external command `/bin/echo @{g:!}` | yes | SB5032 (DD#69 — correct: an external command must state the empty case) |
| `echo @{g:!}` | yes | SB5030 (correct: echo flattens) |
| **for-in `for v in @{g:!}`** | yes | **SB5020 — wrong** |

## Severity

**Fail-safe** — it rejects a program that should compile; it never admits a
tainted read. So this is a usability/consistency defect, not a taint hole. It
is P2 rather than P3 because the diagnostic actively misleads: SB5020 says
"attest-all it first" and never mentions that the author's `:!` was discarded,
so the author's correct use of the sanctioned bypass looks like it simply does
not exist.

## Fix sketch

Honour the flag in the guard — but the read side is only half of it:

1. `reject_tainted_array_iter` (or `splat_array_name`) must return `None` when
   `pe.taint_off` is set.
2. **Check the loop binding.** The point of `:!` is that the value is treated
   as clean, so the bound `v` must come out **clean**, not tainted — otherwise
   the loop body's first read trips SB5020 again on `v` and the bypass is still
   unusable. `taint_for`/`taint_block` (`validate.rs:5390`) sets `lead` from
   `from_source`; that predicate needs the same flag.
3. Confirm the SSA backstop agrees (`ssa_taint.rs` classifies a trusting
   `ArraySplat` as Clean per DD#91) so the two passes do not disagree.

## Spec

No spec covers `:!` on a for-in source — every `:!` spec in `spec/taint.sh`
(91.1-91.3) and `spec/optional_args.sh` (84.33-84.36) is scalar or a
command-argument splat. Pin both the tainted for-in read **and** a read of the
loop variable inside the body, or half the fix will look complete.

## Note for DD#91

DD#91's "Implementation" section still describes the retired `arrvals_trust`
operator and its position restriction. It predates the 2026-09-09 addendum
recorded in `ir.rs` and should be corrected when this is fixed.
