---
title: "mapfile / read -a undeclared-target ICE fixed"
date: 2026-08-19
---

# mapfile / read -a into an undeclared array: ICE fixed

`mapfile -t a < f; echo $(len a)` with no prior `array a=()` panicked at
codegen (`ICE: undefined array: a`, `find_array_slot`). The fill instruction
inserted the plain name `a`, but `find_array_slot` resolves the versioned
`a_0` slot, which only a declaration/assignment creates.

## Fix — mirror argv's auto-declare (DD#60)

DD#60's argv-snapshot auto-declare was explicitly decided to *match
mapfile/read* — so mapfile/read auto-declaring is the intended model; this
just completes it. Extracted the argv inline auto-declare into a shared
`autodeclare_bulk_target(name, as_map, …)`: if `name` isn't already a
collection, emit the empty `IndexedArray` (or `AssocArray` for `mapfile -m`)
Assign that creates the versioned slot, then the fill. Wired into
`try_handle_argv` (refactored to use it), `try_handle_mapfile` (now takes
`state`), and `emit_read_inst` (`read -a`).

An auto-declared target now behaves **identically** to a declared one:
`$(len a)` is the count; element/splat reads (`${a[0]}`, `@a`) are correctly
tainted (mapfile reads external → DD#61) and require `bless` — a clean
SB5020, never an ICE. (The container-bless-then-read behavior is unchanged —
and was already the same for declared arrays.)

## Verification

spec taint.sh ad.1 (`mapfile -t a` undeclared → `$(len a)`=2) + ad.2
(`read -a a` undeclared). native-only (c mapfile-from-redirect reads 0, janet
lacks mapfile — both pre-existing gaps). 3-mode sweep 0 regressions, +2.
`parse_mapfile_flags` extracted from `try_handle_mapfile` (the flag-loop) to
clear the complexity gate on the touched function.

## Follow-up 2026-08-20: front-end collection registration

The transform auto-declare fixed the codegen ICE and `$(len a)`, but an
auto-declared mapfile/read array was still seen as a **scalar** by the
front-end — `bless e in a` / `${a[0]}` / `@a` on it gave SB4030 ("'a' is a
scalar"), while `argv` (which registers in `type_check`) and an explicit
`array a=()` both worked. Surfaced by DG asking "do we have bless-all for
arrays?" and testing it on an auto-declared array.

Fix: `type_check::check_command` now registers `mapfile`/`readarray`/`read -a`
targets as collections (`register_bulk_read_target`/`register_read_array_target`,
`-m` → assoc), mirroring the argv arm. The taint gate is preserved — an
unblessed element read still rejects (SB5020). Refactored `check_command`'s
`if cmd_name ==` chain to a `match` dispatcher (cog 15→2) to clear the
grandfather-on-modify gate. spec taint.sh ad.3 (bless-all works) + ad.4
(unblessed still SB5020). 0 regressions.
