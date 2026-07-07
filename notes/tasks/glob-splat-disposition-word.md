---
title: "glob-splat Part 2: `@{a:-WORD}` / `@{a:?WORD}` external-splat empty-disposition (DD#69)"
status: ready
priority: P2
kind: feature
depends: [array-splat-command-arg]
gates:
  - "bare `@a` into an EXTERNAL command is a compile error directing to a disposition; builtins (echo/printf/…) and for-in are EXEMPT"
  - "`@{a:-WORD}` — empty array → the single arg WORD; bare `@{a:-}` → zero args. WORD is a full word (interpolation + concat + quoting), identical to the scalar `${x:-WORD}` default; it substitutes as a SINGLE element (not re-split)"
  - "`@{a:?WORD}` — empty array → runtime abort, exit 1, stderr `<program-name>: WORD` (the program's own argv[0]/basename, NOT `sash:`)"
  - "only `:-` and `:?` valid after `@{a` (no `:=`/`:+`), consistent with the DD#65 scalar prune"
  - "taint composes: disposition = cardinality-only, bless = value/taint-only; no diagnostic collision"
  - "native + c + janet; spec tests"
---

# glob-splat Part 2: external-splat empty-disposition (`@{a:-WORD}` / `@{a:?WORD}`)

`array-splat-command-arg` is done (`@a` expands per-element). DD#69 Part 2
is the rule that a bare `@a` into an **external** command is a compile
error and must carry an empty-disposition `@{a:-WORD}` / `@{a:?WORD}`.
The WORD semantics were a genuine design choice; **DG resolved them on
2026-06-21** (below), so this is now a ready implementation task.

## Decided (DD#69, recorded)

- Bare `@a` to an **external** command → compile error; must use a
  disposition. Builtins (echo/printf/…) and for-in are EXEMPT.
- `@{a:-WORD}` — empty array → the single arg `WORD`; bare `@{a:-}` →
  zero args. `@{a:?WORD}` — empty → runtime abort with `WORD` on stderr.
- No static-non-empty exemption; no dominating-guard recognition; no new
  keyword. Maps 1-to-1 onto bash `"${a[@]:-WORD}"` / `"${a[@]:?WORD}"`.

## Resolved (DG, 2026-06-21)

- **WORD = full word** — interpolation + concat + quoting, identical to the
  scalar `${x:-WORD}` default (option c). When substituted it is a *single*
  element (not re-split).
- **`@{a:?WORD}` abort** — exit 1; stderr is **`<program name>: WORD`** (the
  running program's own name, i.e. argv[0]/basename — NOT `sash:`).
- **Parser** — only `:-` and `:?` after `@{a` (no `:=`/`:+`).
- **Taint compose** — orthogonal: the disposition is cardinality-only, the
  bless is value/taint-only; they compose without collision.

## Ready to implement

The shared **word machinery** the resolution couples to is **already
built**: the scalar `${x:-WORD}` default now interpolates fully (commit
`6ffe982`, DD#65 stage). So `@{a:-WORD}` reuses that same word lowering
(interpolation + concat + quoting), substituting the result as one element.
No blocker remains.

Remaining implementation surface:
1. **Parser** — accept `@{a:-WORD}` / `@{a:?WORD}` (only `:-`/`:?`), reusing
   the scalar `:-` word body for WORD.
2. **Validator** — bare `@a` in an external-command arg position → compile
   error directing to a disposition (builtins/for-in exempt, per DD#69).
3. **Codegen/runtime** — empty-array → `@{a:-}` yields zero args, `@{a:-WORD}`
   yields the one WORD element; `@{a:?WORD}` aborts exit 1 with
   `<progname>: WORD` on stderr.
4. **c/janet** parity + spec tests.

Part 1 of glob-splat (globs-are-arrays: for-in glob per-element +
inline-`*` reject) is tracked in `glob-as-array-splat-disposition.md`.
