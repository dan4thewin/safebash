#!/bin/bash
# DD#89 — `exit` accepts any int value/variable/return. The value is a full i64
# internally (readable in END via `$?`) and truncated to 8 bits only at the
# process boundary (POSIX `exit_group`). Codegen behavior → native,c; the
# full-value-to-END case is native (the c backend's main-body `exit N` skips
# END — bug-emit-c-exit-skips-end).
. common.sh
plan=8
setup exit_arg

set +e

# 89.1 an int VARIABLE is a valid exit arg (was rejected pre-DD#89)
if in_modes native,c; then
    run_sash 'int code=7
exit $code'
    [[ $rc = 7 && -z $err ]]; chk 89.1
else skip_chk 89.1; fi

# 89.2 an ARITHMETIC result is a valid exit arg
if in_modes native,c; then
    run_sash 'exit $((3+4))'
    [[ $rc = 7 && -z $err ]]; chk 89.2
else skip_chk 89.2; fi

# 89.3 an int-returning FUNCTION result is a valid exit arg
if in_modes native,c; then
    run_sash 'fn f -> int { return 9 }
exit $(f)'
    [[ $rc = 9 && -z $err ]]; chk 89.3
else skip_chk 89.3; fi

# 89.4 an out-of-range LITERAL truncates to 8 bits at the process boundary
# (300 & 0xFF = 44) — same as bash
if in_modes native,c; then
    run_sash 'exit 300'
    [[ $rc = 44 && -z $err ]]; chk 89.4
else skip_chk 89.4; fi

# 89.5 a negative literal truncates too (-1 & 0xFF = 255)
if in_modes native,c; then
    run_sash 'exit -1'
    [[ $rc = 255 && -z $err ]]; chk 89.5
else skip_chk 89.5; fi

# 89.6 an out-of-range VARIABLE truncates the same way (300 → 44)
if in_modes native,c; then
    run_sash 'int code=300
exit $code'
    [[ $rc = 44 && -z $err ]]; chk 89.6
else skip_chk 89.6; fi

# 89.7 no argument → exit 0
if in_modes native,c; then
    run_sash 'exit'
    [[ $rc = 0 && -z $err ]]; chk 89.7
else skip_chk 89.7; fi

# 89.8 the FULL i64 reaches the END block via `$?` (before the 8-bit boundary
# truncation): END sees 300, the process exits 44. native only — the c backend's
# main-body `exit N` skips END (bug-emit-c-exit-skips-end).
if in_modes native; then
    run_sash 'int code=300
END { int c=$?
/bin/echo "END c=$c" }
exit $code'
    [[ $rc = 44 && $out == *"END c=300"* && -z $err ]]; chk 89.8
else skip_chk 89.8; fi
