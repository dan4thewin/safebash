# 199 — ANSI-C octal + grouped-test specs; trap correction

Continuing the coverage grind toward DG's accounting goal (notes/coverage-
accounting.md).

## Added (bucket A)

- cov.63 — ANSI-C `$'\ooo'` octal escapes (`$'\061\062\063'` → "123"),
  covering lexer `scan_ansi_c_octal`. All backends.
- cov.64 — parenthesized `[[ ( … ) ]]` grouped test (`parse_test_group`). All
  backends.

## Correction: `trap` is not a native form (DG)

I had added cov.60 (`trap … EXIT`) and cov.65/66 (`trap ""`/`trap -` signals),
treating `trap` as a first-class sash builtin. DG flagged it, and DD#56 is
clear: the native signal model is **`signal N {IGN,DFL,FLAG,EXIT}` + `END {}`**;
`trap` is a **bash form the converter rewrites** to those (signals.sh 57.14 is a
`run_convert` test — the converter emits `signal`, never `trap`). So direct
`trap` in sash source isn't a supported native form.

Removed cov.60/65/66. Withdrew the `signal-trap-command-handler-bare-exec` task
(a signal-command-handler `trap` misbehaving isn't a bug in a supported feature).

This puts `trap` in the same bucket as `exec` (accounting §"accepted-in-source
bash forms"): `"trap" => BuiltinCmd::Trap` + `handle_trap` accept & lower direct
`trap`, but the converter emits `signal`/`END`, so that machinery is reachable
only via hand-written `trap` that DD#56 says shouldn't exist. DG decision
pending: reject direct `trap`/`exec` (→ dead machinery, like `command`) or keep
as bash-compat.
