---
title: "DD#75 §4 — retire the [@]/[*] bracket-subscript collection forms; @a is the sole splat"
status: ready
priority: P3
kind: task
depends: []
gates:
  - "${a[@]} / $a[@] (at-bracket splat) → compile error pointing at @a (retires the DD#60 input-alias convenience)"
  - "${a[*]} / $a[*] (star) → compile error pointing at @a (splat) / $(join a) (join); $* removed (see dd75-args-model)"
  - "${#a[@]} / ${#a[*]} → compile error pointing at ${#a}/$(len a); ${!a[@]} / ${!a[*]} → pointing at $(keys m)"
  - "unaffected: ${a[$i]}/${a[1]}/a[$i]= indexed access; @a splat + @{a:-W}/@{a:?W} disposition; * as glob and arithmetic multiply"
  - "sash-convert rewrites the retired bash forms to the canonical spellings (§6)"
  - "0 native/c/janet spec regressions; specs pin each reject + the canonical replacement"

## Scoping (2026-08-14 — investigated, not yet implemented)

**Reject must be at PARSE time**, not validate: `${a[@]}` and the canonical
`@a` both lower to `ParameterExpansion{arrvals}` (semantically identical — that
is why they are aliases), so they are indistinguishable downstream. The
syntactic distinction only exists in `parse/expr.rs` where `${…[@]}` / `$a[@]`
are parsed into the arrvals expansion. Emit a diagnostic there instead (mirror
the DD#65 `${var#pat}` SB4034 reject; a new code, e.g. SB40xx, "use `@a`").
`@a` is a different parse path (the `@`-sigil) and is unaffected.

**Test migration (these currently PASS and must flip to reject or `@a`):**
- `spec/wordsplit.sh` — deliberately tests the alias: `${a[@]}` (ws ~139),
  `$a[@]` (~135), quoted `"$a[@]"` (~127); `"${a[@]}"` (~123) is already SB5026.
  Flip the bare-bracket ones to expect the new reject; keep an `@a` positive.
- `sash/tests/codegen_tests.rs:2332` `for x in "${arr[@]}"` → `@arr`.
- `sash/tests/brush_compat_tests.rs:339` `${NONEXISTENT[@]}` → reject/`@x`.
- `${arr[@]OP}` per-element forms already reject (DD#65 SB4034) — leave.

**Converter (follow-up, DG 2026-08-14):** sash-convert rewrites `${a[@]}`/
`${a[*]}`→`@a`, `"${a[*]}"`→`$(join a)`. File separately; not a compiler gate.

Larger than the keys/join validator adds (parser-level + flips passing tests);
best as a focused effort. Unblocked by DG (unreleased → no migration risk).

---

# DD#75 §4 — one splat spelling; retire bracket subscripts

Implements DD#75 §4. `@a` becomes the sole array/map splat. Every bracket-`[@]`/
`[*]` collection role has a canonical home already: splat `@a`, join `$(join a)`,
length `${#a}`, keys `$(keys m)`. After this, `@` means only "splat" and `*` has
no array/positional meaning anywhere (only glob + arithmetic multiply).

This retires DD#60's "parser convenience" that accepted `${a[@]}` on input:
one-true-spelling over paste-from-bash (the converter handles bash input, §6).

See DD#75 §4.
