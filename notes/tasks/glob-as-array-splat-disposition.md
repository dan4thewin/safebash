---
title: "globs are array sugar; @a splat into external commands requires an empty-disposition (@{a:-foo}/@{a:?bang})"
status: ready
priority: P2
kind: feature
depends: [array-splat-command-arg]
gates:
  - "DD#20 reframed: globs are sugar for arrays — `*` yields an array, one verbatim element per dirent, NO word splitting; star-only (DD#39)"
  - "for-in globs: `for f in *` ≡ `array tmp=(*); for f in @tmp` (per-element; 0 matches → loop body skipped). for-in is EXEMPT from the splat-disposition rule"
  - "inline `*` in command-arg position (e.g. `echo *`, `ls *.sh`) is a COMPILE ERROR directing to the array form (globs go through arrays)"
  - "a bare `@a` splat passed to an EXTERNAL command is a COMPILE ERROR; it must carry an empty-disposition: `@{a:-WORD}` (substitute WORD when empty; bare `@{a:-}` = empty ok / zero args) or `@{a:?WORD}` (abort with WORD when empty)"
  - "rule applies to external commands ONLY (sash resolves external-vs-builtin at compile time, DD#23); builtins (echo/printf/…) and for-in are exempt. NO static-non-empty exemption — even a literal array needs a disposition at an external splat. NO dominating-guard recognition — keep it dead simple"
  - "`@{a:-WORD}` / `@{a:?WORD}` map 1-to-1 onto bash `\"${a[@]:-WORD}\"` / `\"${a[@]:?WORD}\"` (DD#67 sugar)"
  - "spec tests for: for-in glob (incl. 0-match), inline-* reject, bare-@a-external reject, each disposition form (native + c at least); no net regression; complexity gate green"
---

# Globs are array sugar + external-splat empty-disposition

Design decided with DG 2026-06-20 (see also the discussion that produced
`array-splat-command-arg` and `for-loop-arith-condition`).

## Part 1 — reframe DD#20: globs ARE arrays

Today only the **array-literal** path globs (`array a=(*)` →
`try_array_single_glob`, expressions.rs:172). `for f in *` and `echo *`
leave `*` as a literal — a silent footgun that contradicts DD#20
("globs go through arrays"; "failglob on inline star globs").

Reframe DD#20 to: **a glob is sugar for an array.** `*` expands to an
array with **one verbatim element per directory entry** — no word
splitting, so a filename with spaces/newlines is exactly one element
(DD#22). Star-only, no `?`/`[]`/`**` (DD#39 unchanged). The array is the
*only* representation of a glob.

Consequences:
- **for-in:** `for f in *` ≡ `array tmp=(*); for f in @tmp`. Per-element
  iteration; 0 matches → body never runs (for-in's built-in empty
  handling — this is why for-in is exempt from Part 2). Fix the static
  for-in path (control_flow.rs `lower_for_static`, :519) to recognise a
  glob word and route it through the array/glob value instead of
  `expr_to_value`-as-literal.
- **command-arg position:** inline `*` (`echo *`, `ls *.sh`) is a
  **compile error**: "globs go through arrays — `array a=(*); … @{a:?}`".
  No inline glob expansion, no runtime failglob.
- **array literal:** `array a=(*)` / `(*.sh)` unchanged (already globs).
- **glob results are CLEAN** (a dirent is a compiler-trusted value, like
  `$(getppid)`): no taint, no SB5021, no value-bless. The only obligation
  is cardinality (Part 2).

## Part 2 — external-splat empty-disposition

The "array expands to nothing → external command silently runs with too
few/zero args" hazard is **general** (glob, `read -a`, `mapfile`,
`$(argv)`, or any mutated-to-empty array), so the rule is on the *splat
site*, not on globs specifically.

**Rule (dead simple, no flow analysis, no trait):** a bare `@a` splat
passed as an argument to an **external command** is a compile error. The
splat must state what happens when the array is empty:

- `@{a:-WORD}` — empty → the single arg `WORD`. Bare `@{a:-}` → zero args
  (empty explicitly ok). ⇔ bash `"${a[@]:-WORD}"`.
- `@{a:?WORD}` — empty → **abort** at runtime with `WORD` on stderr.
  ⇔ bash `"${a[@]:?WORD}"`. This is the explicit, local replacement for
  failglob ("I require a non-empty set").

Non-empty arrays splat per-element (per `array-splat-command-arg`).

**Scope / non-goals (DG, keep it dead simple):**
- External commands ONLY. Builtins (echo/printf/…) and for-in are exempt.
  (Compile-time external-vs-builtin is known — DD#23.)
- NO static-non-empty exemption: even `array a=(x y z); ls @a` errors —
  use `@{a:?}`. (Revisit later if friction warrants.)
- NO dominating-guard recognition (`if (( ${#a[@]} )); then … @a`). Only
  the `@{a:…}` disposition discharges. (Possible later ergonomic add.)
- No new keyword (no gate/check/qualify/bless): the disposition lives in
  the splat syntax; no persistent array "trait" to track.

## Open implementation questions (not blockers)

- **Taint compose:** a tainted-source array (read -a/mapfile/argv) splat
  into an external command needs the cardinality disposition here AND its
  elements blessed for taint (Q5/DD#61). Confirm the two compose cleanly
  (disposition is cardinality-only; bless is value/taint-only).
- **Parser:** `@{a:-WORD}` / `@{a:?WORD}` is new splat-expansion syntax
  (a sash spelling of `${a[@]:-/:?}`); WORD interpolation rules
  (literal? expansions?) to define — mirror bash `:-`/`:?` word.
- **DD docs:** DD#20 reframed (glob lines) and **DD#69** authored
  (globs-as-arrays + the `@{a:…}` splat-disposition rule) — 2026-06-20.

## Cross-refs

- depends: `array-splat-command-arg` (per-element splat — prerequisite).
- related: `for-loop-arith-condition` (size/`(( ${#a[@]} ))` lives on the
  arith path; `[[ ]]` stays value/file tests).
- decisions touched: **DD#69** (new — globs-as-arrays + splat
  disposition), DD#20 (reframe), DD#39 (star-only, unchanged),
  DD#22 (no word splitting), DD#67 (1-to-1 bash sugar), DD#23
  (compile-time command resolution).

## Progress (2026-06-21)

- **Part 1.2 DONE** — inline `*` glob in command-arg position is now a
  compile error (SB5029, `check_inline_glob` in validate.rs), directing
  to the array form. Quoted `"*"` stays literal; `array a=(*)` still
  globs. Specs wordsplit 20.1-20.4 (all three backends). No regression.
- **Part 1.1 TODO** — `for f in *` still treats `*` as a literal (route
  it through the glob→array→per-element path; decision-free per DD#69).
- **Part 2 TODO** — `@{a:-WORD}`/`@{a:?WORD}` disposition + bare-`@a`-to-
  external reject. Blocked on the WORD-interpolation decision
  (DECISION-glob-splat-disposition-word).
