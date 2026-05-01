title: DD#43 — Bless semantics: assign then validate
status: implemented

Formulation A: assign tainted value first, then `bless` validates.
`bless VAR [[ test ]]` — VAR explicitly named, must appear in test.
Arithmetic context `(( ))` is a read context for taint (amendment).
