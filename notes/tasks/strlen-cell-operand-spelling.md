---
title: "How is a cell spelled as a `strlen` operand?"
status: question
priority: P2
kind: decision
depends: []
gates:
  - "DG picks a spelling"
  - "`$(strlen <cell>)` returns the element's byte length in all three backends"
  - "`bug-convert-hash-array-conflation` can emit it"
---

# The question

`bug-convert-hash-array-conflation` needs the converter to emit a byte
length of one array element, because bash's `${#a}` means "bytes of element
0" and `${#a[i]}` means "bytes of element i". The
`dd75-len-collection-only-strlen` filing writes that mapping as
`strlen a[0]` and `strlen a[i]`.

**That spelling does not parse as a cell today, and making it do so is a
language-surface change.** A bare `a[0]` in argument position is an ordinary
literal word — `echo a[0]` prints `a[0]`, exactly as bash does. So `strlen`
would have to read `[`…`]` in its operand *contextually*, which no other
command does.

Measured on `e66339a`, all three candidate spellings are one argument (no
arity error) and all three are rejected SB5062:

    $(strlen a[0])      literal word `a[0]`
    $(strlen a[$i])     literal word, lexer keeps it whole
    $(strlen ${a[0]})   already an ExprKind::Subscript

That last line is the interesting one: **`${a[0]}` already parses as a
cell.** It is rejected only because `classify_operand` buckets it as
"not a name", which is the correct call for `len`/`join`/`keys` and an open
question for `strlen`.

# Why this is not a free choice

Every *other* cell operation in the language is spelled with the braces:
`${a[i]:-x}`, `${a[i]:?m}`, `${a[i]:o:l}`, `${a[i]:~}`. The SSA has a
matching value-carrying node for each (`DefaultValue`, `RequireValue`,
`Substring`, `ElideEmpty`), all documented with the same phrase — *a cell
behaves like a string variable*. `strlen` is the next one in that series.

But `strlen`/`len`/`join`/`keys` are also the only builtins that resolve an
operand as a **name**, and SB5062 exists precisely to stop them accepting
value spellings, because every value spelling they used to accept was
silently wrong. Letting `strlen` take `${a[i]}` puts one foot in each camp.

# Options

**A. `strlen ${a[i]}` — value spelling, brace-delimited.**
No parser change at all. Carve `Cell` out of SB5062 for `strlen` only.
Consistent with every other cell operator in the language. Costs: `strlen`
becomes the one name-resolving builtin that also accepts a value form, so
the SB5062 message has to explain a narrower rule; and it invites "why not
`strlen "$s"` too?" (answer: because that one is genuinely ambiguous with a
name, and a cell is not).

**B. `strlen a[i]` — bare spelling, as the DD#75 filing writes it.**
Matches the filing and reads well next to `strlen s`. Costs: contextual
parsing. The operand word has to be re-read for `NAME[INDEX]` and rebuilt
via the parser's own `subscript_expr` helper. Nothing else in the language
treats `[` specially in an argument, so this is a new precedent, and it is
the kind of source-text re-reading that SB5062's own doc comment identifies
as the original sin.

**C. Both.** A is the general rule, B is sugar for it. Costs: two spellings
for one thing, and B's parsing cost is still paid.

**D. Neither — the converter emits something else.** `${#a[i]}` could map to
an `arr_elem_len`-style operator instead of going through `strlen`. Costs: a
second way to ask the same question, which is what DD#75 §2 was splitting
apart in the first place.

# Recommendation

**A.** It is the only option with no new parsing precedent, it slots into a
series the codebase has already built four times, and the cell form is a
*value* read — which is what the braces mean everywhere else in sash.

# State

The backend half is already written and parked at
`session-state/…/files/strlen-cell-ssa-node.patch` (84 lines): an
`SsaValue::StrLen { source }` node plus its native (`emit_int_to_str` on the
cell's `_len`), C (`emit_itoa_temp`), taint (Clean — a byte count exposes a
length, not the data, matching the existing `strlen`/`arrlen` carve-out) and
`child_values` arms. It is spelling-agnostic: any of A/B/C reuses it
unchanged. Janet lowers from the AST rather than the SSA, so it needs a
`cmdsub_length_intrinsic` arm instead.

Until this is settled, `$(strlen <any cell spelling>)` is rejected SB5062
(spec `types.sh` 75.65), which is correct-but-unbuilt rather than silently
wrong.
