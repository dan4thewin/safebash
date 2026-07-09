---
title: "cleanup: retire the now-unreachable legacy `:s`/`=~` regex_replace machinery"
status: ready
priority: P3
kind: cleanup
depends: []
gates:
  - "no runtime behavior change (the paths below are already unreachable); all specs stay green"
  - "remove the dead transform + codegen + IR encodings once confirmed no producer remains"
---

# Retire the legacy sed/`regex_replace` machinery

## Context

`regex-interpolation-delimiters` (DD#63) re-routed **both** substitution
sugars to the `sub`/`gsub` builtin path at parse time:

- `${var:s/RE/SUB/[g]}` — `parse/expr.rs::try_sed_op` now builds
  `$(sub|gsub "$var" RE SUB)` (interpolating), instead of the old
  `ParameterExpansion { operator: "/" | "//", operand: Literal("pat\0rep") }`.
- `var =~ s/RE/SUB/[g]` — `parse/parser.rs::parse_regex_mutation` now
  builds `var = $(sub|gsub "$var" RE SUB)`, instead of the old
  `Command { name: var, args: [Literal("s/…")] }`.

Nothing else produces those two shapes, so the transform/codegen that
consumed them is **runtime-unreachable** but still compiled/called (so it
does NOT warn). Left in place to keep the DD#63 change focused and
low-risk. This task removes it.

## Unreachable code to remove (verify no producer first)

- `transform/stmt_handlers.rs::try_regex_substitution` (+ its call in
  `handle_command`) — consumed the `Command{var,[s/…]}` shape; always
  returns `None` now.
- The `SsaValue::ParamExpansion { operator: "regex_replace", … }`
  encoding it produced, and its codegen:
  - `codegen/value_handlers.rs` `"regex_replace"` arm (~line 45)
  - `codegen/value_handlers.rs` `"/" | "//" => emit_pattern_replace` arm
    (~line 461) and `emit_pattern_replace`
  - `codegen/values.rs::lower_regex_replace_value` if only reachable via
    the above (CHECK: `$(sub)`/`RegexReplace` uses a different lowering —
    `values.rs:98` `lower_regex_replace_value` may be shared; keep the
    `SsaValue::RegexReplace` path, remove only the `ParamExpansion`
    `regex_replace`/`"/"`/`"//"` path).
  - `codegen/setup.rs:914` `operator == "/" || "//" || "/#" || "/%"`
    guard, if now dead.
- **Keep** `sash-rt::sash_regex_replace` — it is the runtime for the live
  `$(sub)`/`$(gsub)` path.

## Method

1. Grep for any remaining producer of `ParameterExpansion` operator `"/"`
   / `"//"` and of `Command{name:Literal(var), args:[Literal("s/…")]}`.
   Confirm zero (the parser no longer emits them).
2. Remove the consumers above, compile (dead-code warnings will now point
   at anything genuinely orphaned), run the full spec suite (0 regressions
   expected — the code was already unreachable).
3. The `/#`, `/%` operators in setup.rs:914 — confirm whether any live
   construct still uses them before removing that arm.

## Note

DG has ruled dead code should not be kept (decision 2026-07-07). This is
the scoped follow-up to honor that for the DD#63 re-route without bloating
the feature commit.
