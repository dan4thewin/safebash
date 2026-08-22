# 170 — arith bare-subscript edge 4: substring / sed on a cell

`arith-bare-subscript-followups` edge 4 — the last edge. Substring
(`${a[i]:o:l}`) and sed (`${a[i]:s/…/}`) on an array/map cell now reach scalar
parity ("a string cell behaves like a string variable" — DG), completing the
task. Previously both rejected with SB4039 (`cell_string_op`, a staged reject).

## Sed on a cell — parse-time desugar (all three backends free)

The scalar `${s:s/RE/SUB/}` already desugars to `$(sub|gsub "$s" RE SUB)`. For a
cell I build the same shape with the cell read as the subject:
`${a[i]:s/RE/SUB/}` → `$(sub|gsub "${a[i]}" RE SUB)`.

`parse::expr` now shares the sed builder: `try_sed_op` (scalar) and
`subscript_operator_expr` (cell) both call `build_sed_or_malformed`, differing
only in the subject — `quoted_var_expr` (`"$s"`) vs the new `quoted_cell_expr`
(`"${a[i]}"`, a `DoubleQuoted([Subscript])`). Because it lowers to the existing
`$(sub)`/`$(gsub)` path, native/c/janet all work with no backend change. A
missing `/` delimiter still yields the `s_malformed` → SB4032 sentinel.

## Substring on a cell — a source-carrying SSA node

Substring has no builtin form (it's a codegen operator that reads a var slot),
so it needs a value source. New `SsaValue::Substring { source, spec }` mirrors
`DefaultValue`'s source-carrying shape:

- **parse**: `${a[i]:o:l}` becomes a `ParamExpansion "substring"` with the cell
  index attached (was `cell_string_op`/SB4039).
- **transform** (`parameter_expansion_to_value`): a `substring` op *with an
  index* becomes `Substring { source: Subscript(a,i), spec }`; the scalar
  `${s:o:l}` (no index) keeps the plain `ParamExpansion "substring"` path
  untouched (zero risk to the working scalar form).
- **native** (`codegen/values.rs`): lower the source to `(ptr,len)`, then
  `emit_substring` (made `pub(crate)`).
- **emit_c**: `disposition_source_c` (handles the cell read) + `emit_substring_c`.
- **emit_janet** (IR-based, doesn't see the SSA node): the `substring` arm now
  reads the cell via `emit_expr_subscript` when `pe.index` is set, instead of
  slicing the array variable (which had been silently emitting
  `(string/slice a …)` — a latent landmine now closed).
- taint: `Substring`'s taint is its source's (a substring of tainted is
  tainted) — folded into the `RequireValue`/`IntCell` passthrough arm.

## Modes / specs

`arithmetic.sh` scx.16-22. Substring/sed value checks are `native,c` (scx.16-20)
— janet's `string/slice` doesn't clamp an out-of-range end (a pre-existing
scalar-substring gap) and rides the arith-value subscript gap; the malformed
(SB4032) and unlowered-op (SB4039) rejects run all modes (scx.21-22). janet emit
was still corrected (reads the cell) even though not spec-gated there.

Verified end-to-end: native + c `${a[0]:1:3}`→`ell`, `${a[1]:2}`→`rld`,
`${a[0]:s/l/L/g}`→`heLLo`, map-cell sed/substring; janet run `${a[0]:1:3}`→`ell`.
Full `SPEC_MODES="native c janet"` sweep, latest-run-per-test diff vs HEAD
(`f31d2c9`): 0 passing→failing, 0 gone, 14 new passing.

## Task complete

All four edges of `arith-bare-subscript-followups` are done: edge 1 (diagnostic,
journal 169), edge 2 (nested lexer depth, 169), edge 3 (string-cell SB4030,
journal 151), edge 4 (this). Adjacent `bug-braced-nested-subscript-index` filed
(pre-existing, separate path).
