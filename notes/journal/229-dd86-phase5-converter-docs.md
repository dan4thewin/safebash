# 229 — DD#86 Phase 5: converter drops `local`; docs

The migration/cleanup phase of DD#86. The language no longer has `local`
(Phase 2); this makes the converter and docs consistent with that.

## Converter (`sash-convert`)

The converter emitted `local` in its output (`local -i x=5` → `local int
x=5`), and left a bare `local x=5` **unconverted** (it assumed sash accepted
`local`). Both now produce uncompilable sash (`local` → SB5011). Fixed:

- Dropped the `parts.push("local")` — `local` is never emitted.
- Made a bare `local x=5` (no type flag) convert too (previously
  `needs_conversion` required a type flag for `local`), so the keyword is
  stripped: `local x=5` → `x=5` (auto-local), `local -i x=5` → `int x=5`,
  `local -r x=5` → `const x=5`, etc.

Verified round-trip: `f() { local x=5; echo $x; }` → `f() { x=5; echo $x; }`
compiles and runs. Converter test `coerce_local_i_to_int` updated (was
`…_to_local_int`, expected `local int`); all 18 sash-convert tests pass.

**Not done (filed `converter-shadowing-local-rename`):** DD#86 also wants a
bash `local` that *shadowed* a global to be renamed/flagged (sash forbids
shadowing). The common non-colliding case is handled; shadow detection is a
follow-up.

## Docs

- `docs/GUIDE.md` — dropped the `local int x` example from the int-variable
  list (now `int x=5` / `declare int x=5`).
- `notes/decisions/DD35-type-keywords.md` — amended: the **lifetime axis
  (`local`) is retired** (scope is positional, not a keyword); `str` added to
  the type axis. Axes are now mutability (`const`) / type (`int`/`str`) /
  structure (`array`/`map`).

## DD#86 complete

Phases 1–5 shipped:
1. `str` keyword (journal 223)
2. drop `local` + `local fn`, structural scope (journal 226)
3. no-shadowing SB5049 (journal 227)
4. dead-variable rejection SB5050 (journal 228)
5. converter + docs (this)

The whole DD#86 model — no `local`, no shadowing, structural scope,
use-before-and-after (SB5031 + SB5050) — is in place.
