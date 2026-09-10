## 211 — no-ambiguity parameter fallthrough (SB4047)

**On `hand-parser`.** DG: "need to prevent ambiguity — if the compiler doesn't
know what something does, that's very bad." A direct follow-on to journal 210
(non-colon family), generalising the principle.

### The hole

`parse_parameter_expansion` ended with `Expr::param(name, None, None, span)` —
a bare `${name}` — whenever the trailing content after the name matched **no**
operator parser. That silently **dropped** the content. After the non-colon
fix (210) the common cases were caught, but *arbitrary* unrecognised content
still degraded silently: `${x&y}`, `${x:foo}`, `${x.y}`, `${x:}`, `${x y}` all
compiled to `$x`. The compiler was emitting code for input it did not
understand — exactly what DG flagged as "very bad".

### Fix

Reaching the fallthrough means `name_end < len` (the bare `${name}` case returns
earlier), so `rest` is always non-empty and genuinely unrecognised. Carry it on
a sentinel `unrecognized` operator; `rejected_param_operator_diag` rejects it
with **SB4047**: *"unrecognized content `…` after `x` in `${…}` — the compiler
does not recognise this parameter operator and will not silently ignore it."*

Two-layer reject model, both fatal at parse:
- **SB4034** — a *known* dropped operator (`#` `%` `-` `+` `=` `?` `^` `,` `:=`
  `:+` glob-replace): specific guidance ("use `:-` / string builtins").
- **SB4047** — *genuinely unknown* trailing content: "the compiler doesn't know
  this."

Everything sash *does* understand is recognised before the fallthrough — the
kept `:-`/`:?`/`:off:len`/`:s/…` operators, `[…]` subscripts, `@`-transform
(SB4015), `!`-indirection (SB4003), `${#…}` strlen — so only real garbage
reaches SB4047.

### Verification

Specs `spec/var_modifiers.sh` vm.un1-4 (`&`, `:foo`, `.y`, bare `:`). Full
3-mode sweep vs HEAD `e4269d1`: **0 regressions, 0 gone** native/c/janet —
nothing in the suite relied on the silent-accept. fmt + clippy clean. DD#65
"no-ambiguity fallthrough" note recorded.
