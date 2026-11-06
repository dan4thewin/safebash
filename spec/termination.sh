#!/bin/bash
# DD128: cause selects cleanup independently of the full integer status.
. common.sh
plan=38
setup termination
set +e

run_sash 'exit 300; EXIT { int c=$?; echo "exit:$c" }; ERR { - echo wrong }'
[[ $rc = 44 && $out = exit:300 && -z $err && -z $rerr ]]; chk exfail.1

run_sash 'exit 256; EXIT { int c=$?; echo "$c" }; ERR { - echo wrong }'
[[ $rc = 0 && $out = 256 && -z $err && -z $rerr ]]; chk exfail.2

run_sash 'exit -9223372036854775808; EXIT { int c=$?; echo "$c" }'
[[ $rc = 0 && $out = -9223372036854775808 && -z $err ]]; chk exfail.3

run_sash 'fail; EXIT { echo wrong }; ERR { int c=$?; - echo "$c" }'
[[ $rc = 1 && $out = 1 && -z $err && -z $rerr ]]; chk exfail.4

run_sash 'fail 0; EXIT { echo wrong }; ERR { int c=$?; - echo "$c" }'
[[ $rc = 0 && $out = 0 && -z $err && -z $rerr ]]; chk exfail.5

run_sash 'fail 9223372036854775807; ERR { int c=$?; - echo "$c" }'
[[ $rc = 255 && $out = 9223372036854775807 && -z $err && -z $rerr ]]; chk exfail.6

run_sash 'fail -9223372036854775808; ERR { int c=$?; - echo "$c" }'
[[ $rc = 0 && $out = -9223372036854775808 && -z $err && -z $rerr ]]; chk exfail.7

run_sash 'int n=300; fail $((n+1)); ERR { int c=$?; - echo "$c" }'
[[ $rc = 45 && $out = 301 && -z $err ]]; chk exfail.8

run_sash 'fn code -> int { return 256 }; fail $(code); ERR { int c=$?; - echo "$c" }'
[[ $rc = 0 && $out = 256 && -z $err ]]; chk exfail.9

run_sash 'exit 7; ERR { - echo wrong }'
[[ $rc = 7 && -z $out && -z $err && -z $rerr ]]; chk exfail.10

run_sash 'fail 0; EXIT { echo wrong }'
[[ $rc = 0 && -z $out && -z $err && -z $rerr ]]; chk exfail.11

run_sash 'if fail 0 { echo wrong }; echo resumed; ERR { - echo err }'
[[ $rc = 0 && $out = err && -z $err ]]; chk exfail.12

run_sash 'fail 7 || echo resumed; ERR { - echo err }'
[[ $rc = 7 && $out = err && -z $err ]]; chk exfail.13

run_sash '- fail; echo resumed; ERR { - echo err }'
[[ $rc = 1 && $out = err && -z $err ]]; chk exfail.14

run_sash 'if exit 7 { echo wrong }; echo resumed; EXIT { echo exit }'
[[ $rc = 7 && $out = exit && -z $err ]]; chk exfail.15

run_sash 'exit 7 || echo resumed; EXIT { echo exit }'
[[ $rc = 7 && $out = exit && -z $err ]]; chk exfail.16

run_sash '- exit 7; echo resumed; EXIT { echo exit }'
[[ $rc = 7 && $out = exit && -z $err ]]; chk exfail.17

run_sash 'exit 9; EXIT { int c=$?; echo "$c"; exit 11 }; ERR { - echo wrong }'
[[ $rc = 11 && $out = 9 && -z $err && -z $rerr ]]; chk exfail.18

run_sash 'fail 9; EXIT { echo wrong }; ERR { - echo err; exit }'
[[ $rc = 0 && $out = err && -z $err && -z $rerr ]]; chk exfail.19

run_sash 'exit 9; EXIT { echo exit; fail 0 }; ERR { int c=$?; - echo "err:$c" }'
[[ $rc = 0 && $out = $'exit\nerr:0' && -z $err && -z $rerr ]]; chk exfail.20

run_sash 'fail 9; ERR { - echo once; fail 7; - echo wrong }'
[[ $rc = 7 && $out = once && -z $err && -z $rerr ]]; chk exfail.21

run_sash 'fail "7"'
[[ $err == *SB4030* && $err == *'`fail` takes an exit status'* ]]; chk exfail.22

run_sash 'str n=7; fail $n'
[[ $err == *SB4030* && $err == *'`fail` takes an exit status'* ]]; chk exfail.23

run_sash 'fail 1 2'
[[ $err == *SB4045* && $err == *'`fail` expects at most one integer status'* ]]; chk exfail.24

run_sash 'exit 1 2'
[[ $err == *SB4045* && $err == *'`exit` expects at most one integer status'* ]]; chk exfail.25

run_sash 'fail 1 | /bin/cat'
[[ $err == *SB4022* ]]; chk exfail.26

run_sash 'fail 1 &'
[[ $err == *SB4024* ]]; chk exfail.27

run_sash 'fn die { fail 7 }; die &; - wait; echo main; EXIT { echo exit }; ERR { - echo wrong }'
[[ $rc = 0 && $out = $'main\nexit' && -z $err ]]; chk exfail.28

run_sash 'fn die { echo child; fail 7 }; if die | /bin/cat { echo wrong } else { echo caught }; EXIT { echo exit }; ERR { - echo wrong }'
[[ $rc = 0 && $out = $'child\ncaught\nexit' && -z $err ]]; chk exfail.29

run_sash 'fn code -> int { /bin/false; return 3 }; fail $(code); EXIT { echo wrong }; ERR { - echo err }'
[[ $rc = 1 && $out = err && -z $err && $rerr == *"command failed"* ]]; chk exfail.30

run_sash 'exit 7; EXIT { /bin/false }; ERR { int c=$?; - echo "err:$c" }'
[[ $rc = 1 && $out = err:1 && -z $err && $rerr == *"command failed"* ]]; chk exfail.31

run_sash 'str v=clean; v=$(getenv HOME); exit 7; EXIT { echo "$v" }'
[[ $err == *SB5020* && $err == *"explicit exit enters EXIT"* ]]; chk exfail.32

SASH_CFLAGS=--no-taint-cst run_sash 'str v=clean; v=$(getenv HOME); exit 7; EXIT { echo "$v" }'
[[ $err == *"tainted variable"* && $err == *"SSA taint backstop"* ]]; chk exfail.33

run_sash 'str v=clean; fn die { v=$(getenv HOME); exit 7 }; die; EXIT { echo "$v" }'
[[ $err == *SB5020* && $err == *"EXIT requires it"* ]]; chk exfail.34

run_sash 'fn die { read A; exit 7 }; array a; die =a; EXIT { echo "${a[0]}" }'
[[ $err == *SB5020* && $err == *"EXIT requires it"* ]]; chk exfail.35

run_sash 'v=$(getenv HOME); fail 0; ERR { - echo "$v" }'
[[ $err == *SB5020* && $err == *'ERR incoming `v` may be tainted'* ]]; chk exfail.36

run_sash 'str v=clean; v=$(getenv HOME); attest v [[ $v == allowed ]] || fail 0; echo "$v"; ERR { - echo cleanup }'
[[ $rc = 0 && $out = cleanup && -z $err ]]; chk exfail.37

run_sash 'str pending=$(fail 0); echo "$pending"; ERR { - echo cleanup }'
[[ $rc = 0 && $out = cleanup && -z $err && -z $rerr ]]; chk exfail.38

set -e
tally
