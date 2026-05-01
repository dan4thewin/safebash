title: DD#39 — missing expression form simplification and non-colon amendments
status: ready
source: claude-opus-4.6
severity: accuracy

DD#39 note omits amendment 2 (expression form simplified from
`${$var =~ s/...}` to `${var:s/...}`) and amendment 3 (non-colon
parameter expansions coerced to colon forms). Both are in the journal.
