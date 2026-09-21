#!/bin/bash
# DD#89 — `exit` accepts any int value/variable/return. The value is a full i64
# internally (readable in END via `$?`) and truncated to 8 bits only at the
# process boundary (POSIX `exit_group`). The full-value-to-END case is native
# (the c backend's main-body `exit N` skips
# END — bug-emit-c-exit-skips-end).
. common.sh
plan=11
setup exit_arg

set +e

# 89.1 an int VARIABLE is a valid exit arg (was rejected pre-DD#89)
run_sash 'int code=7
exit $code'
[[ $rc = 7 && -z $err ]]; chk 89.1

# 89.2 an ARITHMETIC result is a valid exit arg
run_sash 'exit $((3+4))'
[[ $rc = 7 && -z $err ]]; chk 89.2

# 89.3 an int-returning FUNCTION result is a valid exit arg
run_sash 'fn f -> int { return 9 }
exit $(f)'
[[ $rc = 9 && -z $err ]]; chk 89.3

# 89.4 an out-of-range LITERAL truncates to 8 bits at the process boundary
# (300 & 0xFF = 44) — same as bash
run_sash 'exit 300'
[[ $rc = 44 && -z $err ]]; chk 89.4

# 89.5 a negative literal truncates too (-1 & 0xFF = 255)
run_sash 'exit -1'
[[ $rc = 255 && -z $err ]]; chk 89.5

# 89.6 an out-of-range VARIABLE truncates the same way (300 → 44)
run_sash 'int code=300
exit $code'
[[ $rc = 44 && -z $err ]]; chk 89.6

# 89.7 no argument → exit 0
run_sash 'exit'
[[ $rc = 0 && -z $err ]]; chk 89.7

# 89.8 the FULL i64 reaches the END block via `$?` (before the 8-bit boundary
# truncation): END sees 300, the process exits 44.
run_sash 'int code=300
END { int c=$?
/bin/echo "END c=$c" }
exit $code'
[[ $rc = 44 && $out == *"END c=300"* && -z $err ]]; chk 89.8

# 89.9 a non-numeric exit status is rejected (SB4030). It was lowered as an
# integer constant with a fallback of 0, so `exit 1x` reported SUCCESS for a
# script that meant to fail. bash refuses with "numeric argument required".
run_sash 'exit 1x'
[[ $rc != 0 && $err == *SB4030* ]]; chk 89.9

# 89.10 ... and a wholly non-numeric one
run_sash 'exit abc'
[[ $rc != 0 && $err == *SB4030* ]]; chk 89.10

# 89.11 a negative literal still works: bash masks -1 to 255
run_sash 'exit -1'
[[ $rc = 255 && -z $err ]]; chk 89.11

set -e
tally
