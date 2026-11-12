title: "arith bare-subscript follow-ups: $a[i] form, nested index, string-array cell"
status: done
priority: P3
kind: bug
depends: []
gates:
  - "DONE (edge 1, 2026-10-04, DD#123): `$(( $a[i] ))` **reads the cell**. The 2026-08-21 diagnostic route was an interim measure; the lexer-level fix this section asked for landed for an unrelated reason (the 2026-10-03 brace-less desugar), so the carve-out was removed rather than re-implemented"
  - "DONE (edge 2): nested index `$(( a[a[0]] ))` parses (lexer subscript-depth tracking; the `]]` closes two subscripts, not a `[[ ]]` dispatch)"
  - "DONE (edge 4): substring `${a[i]:o:l}` and sed `${a[i]:s/…/}` on a cell reach scalar parity — sed desugars to `$(sub|gsub \"${a[i]}\" …)`; substring lowers via a new source-carrying `SsaValue::Substring` (native/c/janet)"
  - "no regression"

## Context

arith-subscript model B Stage 2 shipped bare `a[i]` (int-array cell,
rvalue+lvalue) via a `BareSubscript` lexer token + `ArithExpr::Subscript` /
`SubscriptAssign`. Three edges remain:

### 1. `$a[i]` dollar form in arith → SB4035 — RESOLVED (2026-10-04, DD#123)

**`$(( $a[1] ))` now reads the cell**, identically to `${a[1]}` — including
where the braced form *rejects* (`$a[i+1]`, `$a[a[0]]` are SB4041; neither is an
lvalue). The fix this section specified — "extend `scan_dollar` to fuse
`$name[…balanced…]` into a single token" — is exactly what the 2026-10-03
brace-less desugar does, built for the word-context "all paths must agree"
amendment to DD#80. It arrived without anyone connecting it to this edge, so the
`arith_depth > 0` carve-out in `braceless_subscript` sat for a day guarding
against a defect that the same function had already fixed. Removing that guard
was the entire change; the "**Risk**" paragraph below was likewise already paid,
since `expr.rs` has split `name[…]` back out since the desugar landed.

The investigation below is kept as history — it is the record of *why* the
reject was right when it was written.

Model B's table lists `$a[i]` (non-brace) as an rvalue read "as today". Inside
`(( ))` it doesn't parse: `scan_arith_dollar` reads `$a` as `DollarVar("a")`,
then `[` hits `match_arith_op1` → unknown → `BadLit` → SB4035 (a confusing
`invalid numeric literal '['`). Bare `a[i]` and braced `${a[i]}` both work, so
there's a path; this is a convenience gap + a poor diagnostic.

**Investigated 2026-08-21 — a naive fix is UNSAFE (silent wrong answers).**
`$(( ))` content is collected by `collect_arith_content` as a *reconstructed
string* (logos `Tok` stream → `tok_to_text`, with `needs_separator_space`
inserting spaces between tokens), then re-tokenized by `tokenize_arith`. The
tried fix — suppress the space before a `[`-Word and capture `$name[…]` in
`scan_arith_named_var` → the same `ATok::Subscript` as `${a[i]}` — makes the
*literal* case `$a[1]` work, but the bare-ident lexer only fuses `a[i]` into
one Word; `$a` stops at `[`, so any `$`-bearing or multi-token index splits:
`$a[$i]` reconstructs to `"$a[ $i ]"` (spaces **inside** the brackets, because
`needs_separator_space` fires between `[`, `$i`, `]`). `scan_balanced_brackets`
then captures `" $i "` and `subscript_index_expr` mis-parses it → **reads a[0]
silently**. Verified: `$a[i]`, `$a[$i]`, `$a[i+1]` all returned 7 (index 0)
instead of 13. Reverted — worse than the SB4035 reject.

**A correct fix is lexer-level, not a reconstruction patch.** Extend
`scan_dollar` (parse/lexer.rs) to fuse `$name[…balanced…]` into a single
`Tok::Dollar("name[…]")`, mirroring how `scan_word` fuses bare `a[i]`. Then the
subscript (including any `$i`) survives as one token: arith `tok_to_text` emits
it contiguously and `tokenize_arith` threads the index exactly like `${a[i]}`.
**Risk:** word context already parses `$a[i]` via *separate* tokens (spec
bsx.3 `echo $a[$i]` → 8); fusing at the lexer changes that path too, so
`expr.rs`'s `$var` handling must split `name[…]` back out. Needs word+arith
regression coverage. This is a focused lexer/expr change, not a one-liner —
**surface to DG** (language-surface behavior; the "convenience gap" framing
undersold it). The task's fallback ("at minimum a clear error") is also
non-trivial: the `[` is a stray token in the reconstructed string with no
`$var` context at the point SB4035 fires.

### 2. Nested index `a[a[0]]` → SB4035 — RESOLVED (2026-08-21)

Root cause was **not** the arith re-parse: the core word lexer
(`lexer.rs::scan_word_chars`) broke a word on any doubled `]]` (added to make
`[[ … ]]` whitespace-optional). A bare nested cell read `a[a[0]]` **ends in
`]]`**, so it was split into `Word("a[a[0")` + `DblBracketClose`, dropping the
close and corrupting the subscript. (The `[[ ]]`-safety note assumed nested
subscripts always use the `${…}` sigil and end in `}]` — bare model-B subscripts
violate that.) Fix: track subscript-bracket depth in `scan_word_chars`; a
doubled `[[`/`]]` delimits a test/dispatch only at depth 0, while inside a
subscript (depth > 0) `]]` is two subscript closes. `a[a[0]]`, triple-nesting
`a[a[a[0]]]`, and the lvalue `(( a[a[0]] = 5 ))` all parse now; `[[ ]]` dispatch
unaffected (0 spec regressions, dispatch.sh green). Specs nsx.1-3.

**Adjacent finding (pre-existing, NOT this fix — different path):** the *braced*
nested form `${a[a[0]]}` returns the inner cell (`a[0]`) instead of the outer
lookup — a silent wrong answer. It goes through `scan_brace_expansion` +
`subscript_index_expr`, independent of `scan_word_chars`, so it predates this
change. Filed separately (see `notes/tasks/`).

### 3. string-array cell in arith — RESOLVED (2026-08-19)

`array a=(hi 13 21); $(( a[0] ))` now rejects with **SB4030** (a bare string
cell is the same string→int crossing as a bare scalar string — DD#77 D1). The
disposition escape hatch `${a[0]:-N}` / `${a[0]:?msg}` works (native,c; janet
word-context). DG: "same behavior as from a string variable." See DD#77's
"Implemented — string-array/map cells" section + journal 151.

### 4. substring / sed on a cell — RESOLVED (2026-08-21)

`${a[i]:o:l}` (substring) and `${a[i]:s/…/}` (sed) now reach scalar parity —
"a string cell behaves like a string variable" (DG).

- **Sed** (`${a[i]:s/RE/SUB/[g]}`) desugars at parse to `$(sub|gsub "${a[i]}"
  RE SUB)` — the cell read as the subject — reusing the scalar `$(sub)`/`$(gsub)`
  machinery across all three backends. `parse::expr::subscript_operator_expr`
  builds it via the shared `build_sed_or_malformed` + a new `quoted_cell_expr`
  (`"${a[i]}"`). A missing `/` delimiter still gives SB4032.
- **Substring** (`${a[i]:o:l}`) is carried as a `ParamExpansion "substring"`
  with the cell index attached; the transform (`parameter_expansion_to_value`)
  turns a substring-with-index into a new source-carrying
  `SsaValue::Substring { source, spec }` (mirrors `DefaultValue`). Native lowers
  it (`emit_substring` on the lowered source), emit_c via `disposition_source_c`
  + `emit_substring_c`, emit_janet reads the cell (`emit_expr_subscript`) as the
  slice source. The scalar `${s:o:l}` keeps the plain `ParamExpansion` path
  (untouched).
- The pruned families (`#`/`%`/`/`/`@X`) still reject at scalar parity
  (SB4034/SB4015); any other `:`-led op with no cell lowering (`:+`, `:=`) still
  rejects explicitly (SB4039, `cell_string_op`).

Specs `arithmetic.sh` scx.16-22 (substring/sed native,c — janet substring has a
pre-existing out-of-range clamping gap and its arith-value subscript gap; the
malformed/reject checks run all modes). 0 regressions.

## Provenance

Filed during arith-subscript Stage 2 + B2 (journal 150). None block the shipped
int-array feature. Edges 1/2 journal 169, edge 4 journal 170.
