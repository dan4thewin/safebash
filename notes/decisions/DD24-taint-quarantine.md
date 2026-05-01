title: DD#24 — Taint Quarantine and Bless (with DD#43, DD#44)
status: implemented

All external input is tainted: `$(cmd)`, `$(getenv VAR)`, `$(argv N)`,
`read`. Tainted variables cannot be used at any site — not just dangerous
ones — until blessed (DD#44: all use sites, not just sinks).

Bless syntax: `bless var [[ condition ]]` — assign first, validate
second (DD#43: assign-then-validate semantics).

`(( ))` and `$(( ))` are read contexts — tainted vars in arithmetic
are compile errors.

Auto-bless via `known_externals` for 26 env vars and 25 commands with
conservative regex patterns.
