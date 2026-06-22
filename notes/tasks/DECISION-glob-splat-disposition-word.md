---
title: "DECISION NEEDED: @{a:-WORD} / @{a:?WORD} splat-disposition syntax + WORD semantics (glob-splat Part 2)"
status: ready
priority: P2
kind: decision
depends: [array-splat-command-arg]
---

# Decision needed: external-splat empty-disposition syntax (DD#69 Part 2)

`array-splat-command-arg` is done (`@a` expands per-element). DD#69 Part 2
(the rule that a bare `@a` into an **external** command is a compile
error and must carry an empty-disposition `@{a:-WORD}` / `@{a:?WORD}`)
needs design sign-off before implementation, because the WORD semantics
are a genuine design choice (per decision authority), not just plumbing.

## Decided (DD#69, recorded)

- Bare `@a` to an **external** command → compile error; must use a
  disposition. Builtins (echo/printf/…) and for-in are EXEMPT.
- `@{a:-WORD}` — empty array → the single arg `WORD`; bare `@{a:-}` →
  zero args. `@{a:?WORD}` — empty → runtime abort with `WORD` on stderr.
- No static-non-empty exemption; no dominating-guard recognition; no new
  keyword. Maps 1-to-1 onto bash `"${a[@]:-WORD}"` / `"${a[@]:?WORD}"`.

## Open — needs DG decision

1. **WORD interpolation.** Bash's `:-`/`:?` word undergoes expansion
   (parameter, command, arithmetic, tilde) + quote removal. What does
   sash's WORD allow?
   - (a) literal only (simplest; `@{a:-none}` → arg `none`);
   - (b) a single expansion (`@{a:-$x}`, `@{a:-$(cmd)}`);
   - (c) a full word like the DD#65 `${x:-WORD}` default (interpolation +
     concat + quoting) — keep it consistent with the scalar `:-`.
   Note: DD#65's scalar `${x:-WORD}` interpolation is itself currently
   **unimplemented** (`vet-var-modifiers`), so (c) couples the two.
   For `@{a:-WORD}`, WORD is a *single* element when substituted (not
   re-split) — confirm.

2. **`@{a:?WORD}` abort mechanism.** Runtime abort: exit code + message
   format. Mirror bash `:?` (exit 1, `sash: a: WORD` to stderr)? Or a
   sash diagnostic style?

3. **Parser surface.** `@{a:-WORD}` / `@{a:?WORD}` is new splat syntax
   (a sash spelling of `${a[@]:-/:?}`). Confirm only `:-` and `:?` are
   valid after `@{a` (no `:=`/`:+`), consistent with DD#65's scalar prune.

4. **Taint compose (Q5/DD#61).** A tainted-source array (read -a /
   mapfile / argv) splatting into an external command needs BOTH the
   cardinality disposition (here) AND its elements blessed for taint.
   Confirm they compose cleanly (disposition = cardinality-only; bless =
   value/taint-only) and the diagnostics don't collide.

## Recommendation (for discussion, not a decision)

Start with **(a) literal WORD** for `@{a:-WORD}` (dead simple, matches
DD#69's "keep it dead simple"), and align with whatever `vet-var-
modifiers` settles for the scalar `${x:-WORD}` so the two stay 1-to-1.
`@{a:?WORD}` → exit 1 + `sash: a: WORD`.

Part 1 of glob-splat (globs-are-arrays: for-in glob per-element +
inline-`*` reject) is decision-free (DD#69) and is being implemented
independently of this decision.

## Resolved (DG, 2026-06-21)

- **WORD = full word** — interpolation + concat + quoting, identical to the
  scalar `${x:-WORD}` default (option c). COUPLES to `vet-var-modifiers`:
  the scalar `:-` word-interpolation is currently unimplemented, so the
  shared word-interpolation machinery is built once and used by both.
- **`@{a:?WORD}` abort** — exit 1; stderr is **`<program name>: WORD`**
  (the running program's own name, i.e. argv[0]/basename — NOT `sash:`).
- **Parser** — only `:-` and `:?` after `@{a` (no `:=`/`:+`).
- **Taint compose** — orthogonal: the disposition is cardinality-only, the
  bless is value/taint-only; they compose without collision.

Status now: ready to implement (jointly with the scalar `${x:-WORD}`
interpolation in vet-var-modifiers).
