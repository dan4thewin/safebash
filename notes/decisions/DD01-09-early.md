title: DD#01-09 — Early Decisions
status: implemented

Early unnumbered decisions establishing the safebash subset:

1. Reject `eval` (SB4001) — dynamic code execution
2. Reject `source` / `.` (SB4002) — hard error. Superseded by DD#54: use `sash-expand ./file | mapfile -m config` pipeline instead.
3. Namerefs — rejected (SB5003). `declare -n` / `local -n` / `typeset -n` are compile errors. Unnecessary: `$(func)` is cheap in sash, structured types use globals or mapfile.
4. Reject `export` / `declare -x` (SB5001) — use `FOO=bar cmd` instead. Formalized as DD#45 export amendment.
5. Reject `$(cmd)` in heredoc body (SB5002) — extract to var, bless, reference
6. Reject `${!var}` indirect expansion — direct reference only
7. Reject `select`, coproc — out of scope
8. JIT coerce — designed, not built
9. Reject `set -e`/`set -u` as runtime — baked into compiler semantics
