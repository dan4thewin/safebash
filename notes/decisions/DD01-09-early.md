title: DD#01-09 — Early Decisions
status: implemented

1. Executable code is determined statically; runtime data cannot become code.
2. Binding references are direct, not runtime-selected aliases or indirections.
3. Environment reads and per-command environments use explicit boundaries
   (DD53); runtime configuration is data loaded through DD54's pipeline.
4. Error handling and value initialization follow compiler-defined rules,
   not runtime shell options.

---

Early unnumbered decisions establishing the safebash subset:

1. Reject `eval` (SB4001) — dynamic code execution
2. Reject `source` / `.` (SB4002) — hard error. ~~Superseded by DD#54: use `sash-expand ./file | mapfile -m config` pipeline instead.~~ DD99 governs the current reader interface.
3. Namerefs — rejected (SB5003). `declare -n` / `local -n` / `typeset -n` are compile errors. Unnecessary: `$(func)` is cheap in sash, ~~structured types use globals or mapfile.~~
4. Reject `export` / `declare -x` (SB5001) — ~~use `FOO=bar cmd` instead.~~ Formalized as DD#45 export amendment. DD53 delegates per-command environments to `/usr/bin/env`.
5. ~~Reject `$(cmd)` in heredoc body (SB5002) — extract to var, bless, reference~~
6. Reject `${!var}` indirect expansion — direct reference only
7. Reject `select`, coproc — out of scope
8. ~~JIT coerce — designed, not built~~
9. Reject `set -e`/`set -u` as runtime — baked into compiler semantics

## Amendment (2026-05-24): Heredocs removed entirely

Item 5 originally said "Reject $(cmd) in heredoc body." The parser
migration removed heredocs entirely from the language (SB5002).
Heredocs (`<<` / `<<-`) are rejected at the scanner level. ~~Use
herestrings (`<<<`) or variables instead.~~ DD83 retires here-strings too.
