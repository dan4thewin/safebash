---
title: "`:!` is silently ignored on a for-in splat — `for v in @{a:!}` is rejected SB5020 despite DD#91"
status: done
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

## Fixed 2026-09-30

Two layers, not the one the filing named.

1. **validate** — `reject_tainted_array_iter` now early-returns on
   `splat_vouched(w)`, a new helper kept deliberately separate from
   `splat_array_name` (structural question vs policy question; conflating them
   is what `arrvals_trust` did).
2. **transform** — fixing validate alone moved the rejection to the SSA
   backstop, because the loop variable binds `SsaValue::TableValue { array, .. }`
   and its taint is `taint_of_name(array)`. `taint_off` was never plumbed past
   the CST. `detect_array_iter` now returns an `ArrayIter` struct carrying
   `vouched`, and `loop_binding_value` wraps the binding in `SsaValue::Trusted`
   — the node that exists for exactly this, lowered by codegen as its source
   (emitted C verified byte-identical).

Measured: the modifier already worked in every other splat position. The
command-arg case reads as a failure but is SB5032 (an empty splat needs a
disposition), which only fires *after* the taint check passes — `@{g:!-none}`
is accepted while `@{g:-none}` is SB5020.

Controls: `:!` stays per-use (a later bare read is still SB5020); nested
vouched loops work; the `keys` path cannot have a tainted map (SB5021 rejects
building one), so `vouched: false` there is unreachable by construction.

Specs `91.34`-`91.38` in `spec/taint.sh`. Sweep 7093, zero failing. Journal 452.

A control probe here led me to file `bug-quoted-array-splat-not-a-read`, which
was **wrong and has been retracted** — `"@g"` is literal text, not a splat, so
the SB5050 it reports is correct. See `q-quoted-at-splat-is-silently-literal`
for the real (and much narrower) question that remains. The fix recorded above
is unaffected: it was verified by running the binaries.
