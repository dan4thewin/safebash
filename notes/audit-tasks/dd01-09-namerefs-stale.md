title: DD#01-09 namerefs — note says designed, should say rejected (SB5003)
status: resolved
source: claude-opus-4.6, gpt-5.4
severity: accuracy

DD#01-09 item 3 says namerefs are "compile-time substitution in lowerer
(designed, not built)." But audit run-2 added SB5003 which rejects
`declare -n` / `local -n`. The note should reflect rejection, not
designed substitution.
