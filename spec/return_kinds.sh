#!/bin/bash
# DD55/DD58/DD74/DD86: result kinds, not numeric ceremony.
. common.sh
plan=56
setup return_kinds
set +e

run_sash 'fn f { return 0 }; if f { echo yes } else { echo no }'
[[ $rc = 0 && $out = yes && -z $err ]]; chk retkind.1

run_sash 'fn f { return 0 }; f'
[[ $err == *SB4016* && $err == *"returns a status"* ]]; chk retkind.2

run_sash 'fn f { return 0 }; - f; echo done'
[[ $rc = 0 && $out = done && -z $err ]]; chk retkind.3

run_sash 'fn f -> int { return 0 }; int n=$(f); echo "$n"'
[[ $rc = 0 && $out = 0 && -z $err ]]; chk retkind.4

run_sash 'fn f -> int { if (( 1 )) { return 7 } else { return 7 } }; int n=$(f); echo "$n"'
[[ $rc = 0 && $out = 7 && -z $err ]]; chk retkind.5

run_sash 'fn f { if (( 1 )) { return 3 } else { return 3 } }; if f { echo wrong } else { echo status:$? }'
[[ $rc = 0 && $out = status:3 && -z $err ]]; chk retkind.6

run_sash 'fn f { if (( 1 )) { return } else { return 0 } }; - f'
[[ $err == *SB4037* && $err == *"bare return and fall-through supply no result"* ]]; chk retkind.7

run_sash 'fn f { if (( 1 )) { return 0 } }; - f'
[[ $err == *SB4037* && $err == *"every returning path"* ]]; chk retkind.8

run_sash 'fn f -> int { return }; - f'
[[ $err == *SB4037* && $err == *"integer result"* ]]; chk retkind.9

run_sash 'fn f -> int { : }; - f'
[[ $err == *SB4037* && $err == *"integer result"* ]]; chk retkind.10

run_sash 'fn f -> int { if (( 1 )) { return 3 } }; int n=$(f); echo "$n"'
[[ $err == *SB4037* && $err == *"every returning path"* ]]; chk retkind.11

run_sash 'fn f { if (( 1 )) { return }; echo done }; f; echo after'
[[ $rc = 0 && $out = after && -z $err ]]; chk retkind.12

run_sash 'fn g { echo good }; fn f { return g }; f'
[[ $rc = 0 && $out = good && -z $err ]]; chk retkind.13

run_sash 'fn g { echo good }; fn f { return g >forwarded }; f; /bin/cat forwarded'
[[ $rc = 0 && $out = good && -z $err ]]; chk retkind.14

run_sash 'fn g { return 0 }; fn f { return g >forwarded }; if f { echo yes }'
[[ $rc = 0 && $out = yes && -z $err ]]; chk retkind.15

run_sash 'fn g -> int { echo good; return 300 }; fn f -> int { return g >forwarded }; int n=$(f); echo "$n"; /bin/cat forwarded'
[[ $rc = 0 && $out = $'300\ngood' && -z $err ]]; chk retkind.16

run_sash 'fn g -> int { return 0 }; fn f { return g }; - f'
[[ $err == *SB4030* && $err == *"return producer supplies integer"* ]]; chk retkind.17

run_sash 'fn g -> int { return 7 }; fn f { return g >forwarded }; - f'
[[ $err == *SB4030* && $err == *"return producer supplies integer"* ]]; chk retkind.18

run_sash 'fn g { : }; fn f -> int { return g }; - f'
[[ $err == *SB4030* && $err == *"return producer supplies void"* ]]; chk retkind.19

run_sash 'fn g { return 0 }; fn f -> int { return g >forwarded }; - f'
[[ $err == *SB4030* && $err == *"return producer supplies status"* ]]; chk retkind.20

run_sash 'fn g { : }; fn f { if (( 1 )) { return 0 }; return g }; - f'
[[ $err == *SB4030* && $err == *"return producer supplies void"* ]]; chk retkind.21

run_sash 'fn f -> int { return "" }; - f'
[[ $err == *SB4030* && $err == *'`return` takes an integer result'* ]]; chk retkind.22

run_sash 'fn f { return "" }; - f'
[[ $err == *SB4030* && $err == *'`return` takes an integer result'* ]]; chk retkind.23

run_sash 'fn f -> int { int n=$(($1)); if (( n == 0 )) { return 0 }; return f $((n-1)) }; int answer=$(f 3); echo "$answer"'
[[ $rc = 0 && $out = 0 && -z $err ]]; chk retkind.24

run_sash 'fn f { int n=$(($1)); if (( n == 0 )) { return 0 }; return f $((n-1)) }; if f 3 { echo yes }'
[[ $rc = 0 && $out = yes && -z $err ]]; chk retkind.25

run_sash 'fn f -> int { if (( 1 )) { exit 7 }; return 0 }; - f; EXIT { echo exit }'
[[ $rc = 7 && $out = exit && -z $err ]]; chk retkind.26

run_sash 'fn f -> int { fail 0 }; - f; ERR { - echo err }'
[[ $rc = 0 && $out = err && -z $err ]]; chk retkind.27

run_sash 'str s=$(if (( 1 )) { return 0 }); echo "$s"'
[[ $err == *SB4037* && $err == *"anonymous capture"* ]]; chk retkind.28

run_sash 'str s=$(if (( 1 )) { return }; return 0); echo "$s"'
[[ $err == *SB4037* && $err == *"anonymous capture"* ]]; chk retkind.29

run_sash 'str s=$(if (( 1 )) { echo good; return }; echo other); echo "$s"'
[[ $rc = 0 && $out = good && -z $err ]]; chk retkind.30

run_sash 'str s=$(echo good; return 0); echo "$s"'
[[ $rc = 0 && $out = good && -z $err ]]; chk retkind.31

run_sash 'fn f -> int { return 3 }; f | /bin/cat'
[[ $err == *SB4030* && $err == *"pipeline stage"* ]]; chk retkind.32

run_sash 'fn f -> int { return 256 }; /bin/echo x | f'
[[ $err == *SB4030* && $err == *"pipeline stage"* ]]; chk retkind.33

run_sash 'fn f -> int { return 3 }; /bin/echo x | - f | /bin/cat'
[[ $err == *SB4030* && $err == *"pipeline stage"* ]]; chk retkind.34

run_sash 'fn f -> int { return 256 }; if - f | /bin/cat { echo wrong }'
[[ $err == *SB4030* && $err == *"pipeline stage"* ]]; chk retkind.35

run_sash 'fn f -> int { return 3 }; /bin/echo x | ! f'
[[ $err == *SB4030* && $err == *"pipeline stage"* ]]; chk retkind.36

run_sash 'fn f -> int { return 256 }; - f | readline s; echo "${s:!}"'
[[ $err == *SB4030* && $err == *"pipeline stage"* ]]; chk retkind.37

run_sash 'fn f -> int { return 0 }; - f; echo done'
[[ $rc = 0 && $out = done && -z $err ]]; chk retkind.38

run_sash 'fn f -> int { return 300 }; fn wrapper { - f; echo good }; wrapper | /bin/cat'
[[ $rc = 0 && $out = good && -z $err ]]; chk retkind.39

run_sash 'fn f -> int { return 300 }; fn wrapper { int n=$(f); echo "$n"; return (( n == 300 )) }; wrapper | /bin/cat'
[[ $rc = 0 && $out = 300 && -z $err ]]; chk retkind.40

run_sash 'fn g { echo good }; fn f { return g >forwarded }; - f'
[[ $err == *SB4076* && $err == *"returns nothing"* ]]; chk retkind.41

run_sash 'fn f -> int { exit 7; echo unreachable }; - f; EXIT { echo exit }'
[[ $rc = 7 && $out = exit && -z $err ]]; chk retkind.42

run_sash 'fn f -> int { if (( 1 )) { exit 7 } else { fail 0 } }; - f; EXIT { echo exit }; ERR { - echo wrong }'
[[ $rc = 7 && $out = exit && -z $err ]]; chk retkind.43

run_sash 'fn f -> int { if exit 7 { : } }; - f; EXIT { echo exit }'
[[ $rc = 7 && $out = exit && -z $err ]]; chk retkind.44

run_sash 'fn f -> int { fail 0 || echo wrong }; - f; ERR { - echo err }'
[[ $rc = 0 && $out = err && -z $err ]]; chk retkind.45

run_sash 'fn f { return 0; return }; if f { echo yes }'
[[ $rc = 0 && $out = yes && -z $err ]]; chk retkind.46

run_sash 'fn f -> int { return exit 7 }; - f; EXIT { echo exit }'
[[ $rc = 7 && $out = exit && -z $err ]]; chk retkind.47

run_sash 'fn f -> int { return fail 0 }; - f; ERR { - echo err }'
[[ $rc = 0 && $out = err && -z $err ]]; chk retkind.48

run_sash 'fn f { return "5" }; - f'
[[ $err == *SB4030* && $err == *'`return` takes an integer result'* ]]; chk retkind.49

run_sash 'fn f -> int { if (( 1 )) { return $(( (f) )) } }; - f'
[[ $err == *SB4037* && $err == *"every returning path"* ]]; chk retkind.50

run_sash 'ERR { - echo err }; fn g -> int { return 7 }; fn f -> int { return g >retkind-missing/out }; - f; echo bad'
[[ $rc = 1 && $out = err && $rerr == *'retkind-missing/out'* && -z $err ]]; chk retkind.51

run_sash 'ERR { - echo err }; fn g { echo good }; fn f { return g >retkind-missing/out }; f; echo bad'
[[ $rc = 1 && $out = err && $rerr == *'retkind-missing/out'* && -z $err ]]; chk retkind.52

run_sash 'fn f -> int { while exit 7 { return } }; - f; EXIT { echo exit }'
[[ $rc = 7 && $out = exit && -z $err ]]; chk retkind.53

run_sash 'fn f -> int { until fail 0 { return } }; - f; ERR { - echo err }'
[[ $rc = 0 && $out = err && -z $err ]]; chk retkind.54

run_sash 'fn f -> int { if (( 0 )) { return 3 } elif exit 7 { return } }; - f; EXIT { echo exit }'
[[ $rc = 7 && $out = exit && -z $err ]]; chk retkind.55

run_sash 'fn f -> int { fail 0 || return }; - f; ERR { - echo err }'
[[ $rc = 0 && $out = err && -z $err ]]; chk retkind.56

set -e
tally
