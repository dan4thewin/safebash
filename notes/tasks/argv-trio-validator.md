---
title: argv/arg/argc trio — validator phase
status: done
priority: P0
kind: feature
depends: [argv-trio-design]
gates:
  - top-level $@/$*/$N/$#/$0 → SB-error with directed text (cf.dd46.1..5 pass)
  - inside-function $@/$*/$N/$# accepted as clean (cf.f11..f14 pass)
  - $(arg N) recognized as scalar tainted builtin (av.t3..t7 pass)
  - $(argv) and $(argv N) recognized as array-context tainted splats
  - quoted array splats rejected per DD22 addendum (ws.q1..q3 pass)
  - mixed-taint for-in rejected (av.m1..m3 pass)
  - SB5020 fires for all bare uses of $(arg)/$(argv) (av.x* pass)
  - no regressions in existing 291 native spec tests
---

# argv/arg/argc trio — validator phase

Implements all validator-side gates of [DD60](../decisions/DD60-argv-trio.md)
(to be written by [argv-trio-design](argv-trio-design.md)), the
DD22 quote-strengthening addendum, and DD46 top-level enforcement.

Codegen stays unchanged in this phase — the validator either rejects
or hands well-formed IR to the codegen layer for T3 to extend.

## Scope

### A. Top-level positional rejection (DD46 enforcement)

Today: `sash/src/validate.rs::check_dq_*` family checks `$?` (SB5012)
but does not check positionals. `sash/src/codegen/dispatch.rs:1097
emit_exec_with_dollar_at` happily lowers top-level `$@` via
`splice_in_program_argv` — bypassing DD46 entirely.

Required: a new rule (assigning an SB code from the next free slot)
that walks every top-level expression and rejects positional reads
(`$@`, `$*`, `$1`..`$N`, `$#`, `$0`) with a directed-error message
naming the replacement builtin. Inside function bodies, the same
forms are accepted (current behavior already supports this for the
function-local case; the existing `lower_for_dollar_at` path and
function-arg lowering remain).

**Decision boundary**: "top-level" = any statement outside a `fn`
body. Function bodies see clean positionals; sourced files at top
level do not (sourced files run with the program's argv).

### B. New builtin recognition

Today: `sash/src/validate.rs:994` lists `argv` as a known builtin.
`sash/src/value_handlers.rs:946` lowers `argv` with N as scalar.
`sash/src/ssa_taint.rs:364` marks `argv` (and `getenv`) tainted.

Required:

  - Add `arg` to the known-builtin list; mark tainted in `ssa_taint.rs`.
  - Re-type `argv`: in scalar-context substitution `$(argv)` /
    `$(argv N)`, recognize as array-returning. Add validator rejection
    for scalar-context uses (av.x7, av.x8) directing user to `$(arg N)`
    or array snapshot.
  - `argc` already returns scalar (clean? or tainted? — argc itself
    is a count, not a user-supplied string; today it's clean. Keep
    clean.)

### C. Splat context enforcement

`$(argv)` and `$(argv N)` are legal only in:
  - for-in headers (single sole source)
  - RHS of `array a=$(argv ...)` (snapshot — full mechanics in T4
    but validator-side check lands here)

Other positions (scalar assign, command arg, test expression,
arithmetic operand, quoted interpolation) → SB-error.

### D. Quote rejection (DD22 addendum)

Today: `sash/src/transform/control_flow.rs:339` unwraps decorative
quotes around `${arr[@]}` via `DoubleQuoted(parts) if parts.len() == 1`.

Required: validator-side reject any `DoubleQuoted` that contains an
array-splat form as its sole content. Remove the unwrap at
`control_flow.rs:339` once the validator rejects upstream (handled
in T3, but validator-side check lands here).

Forms to reject: `"${a[@]}"`, `"$a[@]"`, `"$(argv)"`, `"$(argv N)"`.

### E. Mixed-taint for-in rejection

For-in header validation: enumerate the source items (after
splat-form recognition). If any item is tainted, every item must
come from the same tainted source. Otherwise SB-error.

Simplest formulation: in for-in headers, exactly one of these holds:
  1. Zero tainted sources (clean literals, clean named array splat —
     today's accepted shape)
  2. Exactly one tainted source as the sole item (no other items
     present)

## Sites likely affected

  - `sash/src/validate.rs` — new rule(s); extend known-builtin table;
    add for-in source enumeration helper
  - `sash/src/rules/*` — add new rule files following the existing
    rule-per-file convention (e.g., `r_dd46_toplevel.rs`,
    `r_array_splat_unquoted.rs`, `r_for_in_taint_uniform.rs`,
    `r_argv_splat_context.rs`)
  - `sash/src/ssa_taint.rs:364` — extend the taint-source match arm
    for `arg`
  - `sash/src/value_handlers.rs` — recognize `arg` and `argv`-without-N

## Test gates

All spec tests enumerated in T1 fall into one of:
  - **Pass under T2 alone**: the SB-error gates (av.x*, av.m*,
    cf.dd46.*, ws.q*) — pure validator behavior
  - **Pass after T3**: the runtime semantics gates (av.t*, av.s*,
    av.n*, cf.f11..f14) — need codegen wired
  - **Pass after T7 closure**: cf.f4 (was ICE, now SB-error redirect
    once T2 lands; positive case once T3 + bless plumbing lands)

T2 should land all SB-error tests as passing and all positive-case
tests as **failing with the expected codegen-missing error** (not
ICE — they should produce a clean "not yet implemented" diagnostic
or compile through to a runtime stub that's then wired in T3). The
boundary between "validator accepts and codegen barfs" and "validator
defers to codegen" needs deciding in T2's first commit.

## Risks

  - Existing tests using `"${arr[@]}"` will start failing after the
    unwrap is removed → must coordinate with T6 (test migration)
    landing before/with T2's final commit. Suggest T2 lands the
    validator with the unwrap intact + a feature-flag gate, then T6
    migrates tests, then T2 follow-up commit removes the unwrap.
  - `splice_in_program_argv` still needs to exist for inside-function
    `$@` codegen → T3 keeps the machinery; only the top-level dispatch
    site is gated off by the new validator rule.

## References

  - [argv-trio-design](argv-trio-design.md) — DDs and spec contract
  - DD22, DD24, DD46
  - `sash/src/validate.rs` — existing rule framework
  - `sash/src/rules/` — one-rule-per-file convention
  - AGENTS.md "Bug policy" — ICE is always a bug; the validator
    closing the top-level $@ ICE is the headline win here
