---
title: "EFFORT: janet structural parse-map (--janet-map) — staged rewrite"
status: active
priority: P2
kind: effort
depends: []
supersedes-strategy: janet-rt-primitive-library
---

# janet structural parse-map (DD#15 amendment, 2026-08-20)

DG re-aimed the janet backend at **reviewability**: a 1-to-1 map of how the
compiler parsed the sash, readable side-by-side. Rendered via a `sash/*`
vocabulary over a `sash-rt.janet` primitive library; semantics in the library,
the emitted map stays structural. Built in **stages** behind `--janet-map`
(self-contained, preamble prepended), to eventually replace `--janet`.

This reframes `janet-rt-primitive-library` (same "thin lowering + sash-rt.janet"
foundation) but extends the naming all the way up (control + sugar, not just
leaf primitives) and prioritizes the readable map.

## Stage 1 — DONE (2026-08-20)

`sash/src/emit_janet_map.rs` + `sash/janet/sash-rt.janet` + `--janet-map`.
Covers: assignments, `echo`, `:`, external commands (bare `sash/run` /
condition `sash/ok?` — position visible), `if`/`elif`/`else`/`while`/`until`,
`[[ ]]` string tests (`= == != -n -z`), `$(single-external)`
capture, `bless` (functional gate: `sash/bless-checked` aborts, `sash/bless`
branches), `- cmd` suppression (`sash/dash`), `$?`. Unhandled nodes render as a
visible `(sash/TODO stmt:… / expr:…)` naming the construct. Runs; validates
through the real front-end (SB5031 etc. still reject before a map is produced).

**No numerics in `[[ ]]`** (DG, 2026-08-20): sash `[[ ]]` is string/file tests
only; numeric comparison is `(( ))` (`ArithTest`) — the front-end rejects
`[[ $x -eq 5 ]]` with SB5019. `binary_test_form` maps string ops only; there
are NO `sash/-eq…-ge` runtime forms. **S4 must model `(( ))` numeric compare
as arithmetic (`sash/arith`), never as a `[[ ]]` test form.** Same rule bars
the pruned `${x%%…}`/`${x:OP}` bashisms — model only what the front-end
accepts (DD#65).

## Next stages (rough, refine per DG's read)

- **S2 functions + return** (`sash/fn`, `sash/return`, predicate returns),
  positional args (`$1`/`$@`/`$#`/argv trio), local scope.
- **S3 pipelines** (`sash/pipeline` structure kept in emit), `&` background /
  `$!` / `wait`, redirects (`sash/redirect`).
- **S4 the value surface**: `${…}` parameter modifiers, arithmetic `$(( ))`
  and `(( ))` (`sash/arith` — this is where numeric compare `-eq…-ge` lives,
  NOT `[[ ]]`), arrays/maps + subscripts, splats `@a`, glob.
- **S5 the rest of taint/abort/signals**: DD#55 abort channel through the map,
  DD#56 signals, `case`/dispatch, multi-statement/`$(f)` capture, regex tests.
- **S6 oracle re-green + swap**: bring `--janet-map` to parity with native on
  the spec suite, then retire the old `emit_janet` and point `--janet` at it.

## Open refinements (for DG to weigh in via experimenting)

- preamble prepended (self-contained) vs an `(import)` so the map reads clean
  on its own — currently prepended.
- `$name` as `($ x)` vs bare `x` — currently `($ x)` (explicit sigil).
- assignment as `(set x …)` with pre-declared `(var x nil)` vs `(var/set)` —
  currently pre-declare + `set`.
