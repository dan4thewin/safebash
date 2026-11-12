title: "EFFORT: janet structural parse-map (--janet-map) — staged rewrite"
status: deferred
priority: P2
kind: effort
depends: []
supersedes-strategy: janet-rt-primitive-library

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

## Stage 2 — DONE (2026-08-20)

Function surface + typed bindings. Verified as a correct oracle: 7 programs
(functions, positionals, `$@` splice, return, function-in-condition, elif-in-
function, negative int) render maps whose `janet` output is **byte-identical to
native**.

- **Typed declarations**: `int n=5` → `(set n 5)` — a janet **number**; untyped
  `x=5` → `(set x "5")` — a **string**. The type shows through the *value
  representation*, not an annotation (DG, 2026-08-20). `render_decl_value` +
  `parse_int_literal` (decimal/±/0x/0o/0b). Note: `int h=0xff` is rejected by
  the front-end (SB4030); prefixed ints arrive via `$((0xff))` (S4).
- **Functions**: `f(){…}` → `(sash/defn f body…)` (a `with-dyns` positional
  frame + `sash-ret` label; structure still supplied by emit).
- **User-function calls are distinct from externals**: a call to a defined
  function → direct `(f args…)` (bare) / `(sash/true? (f args…))` (condition —
  the trailing `?` marks the boolean; DG renamed from `succeeded`); an external
  stays `sash/run`/`sash/ok?`. `Env.fns` collected up front. A bare nonzero-
  return call does NOT abort (matches native).
- **Function parameters vs process argv are kept distinct** (DG, 2026-08-20):
  `$1` is a function's own argument (sugar for `$args[1]`), NEVER an argv
  element. Function params → the `param` family: `$N` → `(sash/param N)`,
  `$#` → `(sash/nparams)`, `$@`/`$*` → `;(sash/params)` splice (function-local;
  SB5024 bars them at top level). The process command line → the argv family:
  `$(arg N)` → `(sash/arg N)` (0-based, tainted), `$(arg N DEFAULT)` →
  `(sash/arg N DEFAULT)`, `$(argc)` → `(sash/argc)`, `$(argv)` → `(sash/argv)`.
  (Before, `$(arg N)`/`$(argc)` mis-mapped as external `sash/capture`.)
- **return**: `return N` → `(sash/return N)` (numeric via `sash/-code`); bare
  `return` → `(sash/return ($ ?))`.
- **`(sash/TODO …)` now compiles**: defined as a preamble macro that names the
  unmapped construct and aborts at runtime if reached — a partial program runs
  up to the first gap instead of failing janet compilation.

**Known gap — other `$(…)` value-function intrinsics** (`len`, `toupper`,
`tolower`, `getenv`, `getpid`, `random`, `signum`, `epochseconds`, `basename`,
`dirname`, `getcwd`, `join`, `keys`, …) still route through `sash/capture`
(mis-shown as an external spawn). Only the argv trio (`arg`/`argc`/`argv`) is
mapped so far — the rest are the S4 value surface.

**Known S2 gap (flat scope):** declarations/assignments — including inside
functions — pre-declare one flat top-level `(var … nil)`; `local`-shadowing is
not modelled (the existing `emit_janet` has the same flat model). Proper per-
scope `(var)`/`(set)` deferred (candidate S3/S5).

## Next stages (rough, refine per DG's read)

- **S3 pipelines** (`sash/pipeline` structure kept in emit), `&` background /
  `$!` / `wait`, redirects (`sash/redirect`). Consider proper local scope here.
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

## Deferred to post-beta (DD#104, 2026-09-30)

The in-tree janet backend is removed; `--emit-ir` replaces it with an
inert IR capture that an out-of-tree oracle can consume. This gap is
**not fixed** -- it is recorded here as part of the diagnosis a restart
should read. The code it describes lives at tag `janet-babel`.

This file was still marked live (`open`/`pending`/`active`) long after the
backend it targets was deleted; the other 28 janet tasks were deferred at the
time and these four were missed. Deferred 2026-09-30 for consistency -- no
assessment of the underlying gap has changed.
