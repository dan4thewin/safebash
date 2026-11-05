#!/bin/bash
# DD#128 — checked EXIT and explicitly handled ERR.
. common.sh
plan=81
setup terminal

set +e

run_sash 'exit 1; ERR { /bin/false }'
[[ $err == *SB5101* && $err == *"uncaught command failure"* ]]; chk errfail.1

run_sash '
fn bad {
    /bin/false
}
exit 1
ERR { bad }'
[[ $err == *SB5101* && $err == *":6:7:"* && $err == *"at 3:5"* ]]; chk errfail.2

run_sash 'exit 1; ERR { /bin/false || /bin/false }'
[[ $err == *SB5101* && $err == *"uncaught command failure"* ]]; chk errfail.3

run_sash '
fn bad { cd /sash-dd128-nonexistent-directory }
exit 1
ERR { - echo "$(bad)" }'
[[ $err == *SB5101* && $err == *'uncaught `cd` failure'* ]]; chk errfail.4

run_sash 'exit 1; ERR { if /bin/true >out { : } }'
[[ $err == *SB5101* && $err == *"descriptor backup/restore"* ]]; chk errfail.5

run_sash "exit 1; ERR { - /bin/echo \$'x\\x00y' }"
[[ $err == *SB5101* && $err == *"NUL-byte argv guard"* ]]; chk errfail.6

# A stage's argv guard is in the child; the parent handles its process status.
run_sash "exit 1; ERR {
    if /bin/echo \$'x\\x00y' | /bin/cat { - echo unexpected } else { - echo caught }
}"
[[ $rc = 1 && $out = caught && $err != *SB5101* ]]; chk errfail.7

DD128_VALUE=allowed run_sash '
v=$(getenv DD128_VALUE)
attest v [[ $v == allowed ]]
exit 1
ERR { - /bin/echo "${v:!}" }'
[[ $rc = 1 && $out = allowed && -z $err ]]; chk errfail.8

DD128_VALUE=allowed run_sash '
exit 1
ERR {
    v=$(getenv DD128_VALUE)
    attest v [[ $v == allowed ]] || v=replaced
    - /bin/echo "$v"
}'
[[ $rc = 1 && $out = allowed && -z $err ]]; chk errfail.9

DD128_VALUE=allowed run_sash '
exit 1
ERR {
    v=$(getenv DD128_VALUE)
    attest v [[ $v == allowed ]]
    - echo "$v"
}'
[[ $err == *SB5101* && $err == *"uncaught attest failure"* ]]; chk errfail.10

printf allowed >in
run_sash '
exit 1
ERR {
    v=$(<in)
    attest v [[ $v == allowed ]] || v=replaced
    - echo "$v"
}'
[[ $err == *SB5101* && $err == *"slurp"* ]]; chk errfail.11

# A second fatal fault terminates; arithmetic itself is not banned in ERR.
run_sash '
exit 1
ERR {
    - echo attempted
    int n=1
    int zero=0
    n=$(( n / zero ))
    - echo unseen
}'
[[ $rc = 2 && $out = attempted && -z $err && $rerr == *"division by zero"* ]]; chk errfail.12

run_sash 'exit 1; ERR { str v=good; - echo "${v:?stop}" }'
[[ $rc = 1 && $out = good && -z $err ]]; chk errfail.13

run_sash 'v=good; exit 1; ERR { - echo "${v:?stop}" }'
[[ $err == *SB5101* && $err == *'unhandled `:?` failure'* ]]; chk errfail.14

run_sash 'exit 1; ERR { - /bin/true | - /bin/true }'
[[ $err == *SB5101* && $err == *"pipeline construction"* ]]; chk errfail.15

run_sash '
exit 1
ERR {
    array pairs=(key value other)
    map values = @{pairs:?odd}
    - echo "${values[key]}"
}'
[[ $err == *SB5101* && $err == *"odd-length map conversion"* ]]; chk errfail.16

# A single-use call owns the acquisition through the caller's adjacent attest.
DD128_VALUE=bad run_sash '
str v=initial
fn acquire {
    v=$(getenv DD128_VALUE)
    /bin/kill -USR1 $$
}
signal 10 EXIT
acquire
attest v [[ $v == allowed ]] || v=clean
/bin/true
echo unseen
EXIT { int code=$?; echo "$code:$v" }'
[[ $rc = 138 && $out = 138:clean && -z $err ]]; chk sigguard.1

run_sash '
fn acquire { /bin/kill -USR1 $$; echo bad }
signal 10 EXIT
v=$(acquire)
attest v [[ $v == allowed ]] || v=clean
/bin/true
echo unseen
EXIT { int code=$?; echo "$code:$v" }'
[[ $rc = 138 && $out = 138:clean && -z $err ]]; chk sigguard.2

DD128_VALUE=bad run_sash '
signal 10 EXIT
v=$(getenv DD128_VALUE)
if attest v [[ $v == allowed ]] {
    :
} else {
    /bin/kill -USR1 $$
    v=clean
}
/bin/true
echo unseen
EXIT { int code=$?; echo "$code:$v" }'
[[ $rc = 138 && $out = 138:clean && -z $err ]]; chk sigguard.3

# EOF skips the body's attest; producer completion is a real signal entry.
run_sash '
signal 10 EXIT
if /bin/true | readline v {
    attest v [[ -n $v ]]
    exit 0
} else {
    /bin/true
    exit 1
}
EXIT { echo "$v" }'
[[ $err == *"signal checkpoint"* && $err == *"EXIT requires it"* ]]; chk sigguard.4

SASH_CFLAGS=--no-taint-cst run_sash '
signal 10 EXIT
if /bin/true | readline v {
    attest v [[ -n $v ]]
    exit 0
} else {
    /bin/true
    exit 1
}
EXIT { echo "$v" }'
[[ $err == *"tainted variable"* && $err == *"SSA taint backstop"* ]]; chk sigguard.5

DD128_VALUE=bad run_sash '
str v=initial
signal 10 EXIT
signal 10 IGN
/bin/true
v=$(getenv DD128_VALUE)
exit 1
EXIT { echo "$v" }'
[[ $rc = 1 && -z $out && -z $err ]]; chk sigguard.6

# Changing disposition does not erase a previously received EXIT request.
run_sash '
str v=initial
signal 10 EXIT
signal 10 IGN
v=$(getenv DD128_VALUE)
/bin/true
exit 1
EXIT { echo "$v" }'
[[ $err == *"signal checkpoint"* && $err == *"EXIT requires it"* ]]; chk sigguard.7

run_sash '
int parent=$$
str v=initial
fn producer { /bin/kill -USR1 $parent; echo bad; return 7 }
signal 10 EXIT
producer | readline v
attest v [[ $v == allowed ]] || v=clean
EXIT { echo "exit:$v" }
ERR { - echo "err:${v:!}" }'
[[ $rc = 7 && $out = err:bad && -z $err ]]; chk sigguard.8

# Injection at the parent read is after the pipeline's pre-reader polls.
run_sash_under '
signal 10 EXIT
/bin/echo bad | readline v
echo "${v:!}"
exit 1
EXIT { int code=$?; echo "$code:completed" }
' strace -qq -o signal-read.trace -e trace=read -e inject=read:signal=SIGUSR1:when=1
[[ $rc = 138 && $out = 138:completed && -z $err ]] &&
    grep -q SIGUSR1 signal-read.trace; chk sigguard.9

run_sash_under '
signal 10 EXIT
if /bin/true | readline v {
    attest v [[ -n $v ]]
    exit 0
} else {
    exit 1
}
EXIT { echo "$v" }
ERR { - echo explicit }
' strace -qq -o signal-transfer.trace -e trace=read -e inject=read:signal=SIGUSR1:when=1
[[ $rc = 1 && $out = explicit && -z $err ]] &&
    grep -q SIGUSR1 signal-transfer.trace; chk sigguard.10

# The independent SSA gate must not allow a late attest of inherited input.
SASH_CFLAGS=--no-taint-cst run_sash '
v=$(getenv DD128_VALUE)
exit 1
ERR {
    attest v [[ $v == allowed ]] || v=clean
    - echo "$v"
}'
[[ $err == *"cannot be attested without a fresh acquisition"* ]]; chk errentry.1

SASH_CFLAGS=--no-taint-cst run_sash '
v=$(getenv DD128_VALUE)
exit 1
ERR {
    v=allowed
    attest v [[ $v == allowed ]] || v=clean
    - echo "$v"
}'
[[ $err == *"cannot be attested without a fresh acquisition"* ]]; chk errentry.2

DD128_VALUE=allowed SASH_CFLAGS=--no-taint-cst run_sash '
v=$(getenv DD128_VALUE)
exit 1
ERR {
    v=$(getenv DD128_VALUE)
    attest v [[ $v == allowed ]] || v=clean
    - echo "$v"
}'
[[ $rc = 1 && $out = allowed && -z $err ]]; chk errentry.3

SASH_CFLAGS=--no-taint-cst run_sash '
v=$(getenv DD128_VALUE)
exit 1
ERR {
    if [[ "$$" == impossible ]] { v=$(getenv DD128_VALUE) }
    attest v [[ $v == allowed ]] || v=clean
    - echo "$v"
}'
[[ $err == *"cannot be attested without a fresh acquisition"* ]]; chk errentry.4

SASH_CFLAGS=--no-taint-cst run_sash '
argv a
exit 1
ERR {
    if attest item in a [[ -n $item ]] { - echo done } else { unset a }
}'
[[ $err == *"cannot be attested without a fresh acquisition"* ]]; chk errentry.5

SASH_CFLAGS=--no-taint-cst run_sash '
fn check { if attest item in A [[ -n $item ]] { - echo done } else { unset A } }
argv a
exit 1
ERR { check =a }'
[[ $err == *"cannot be attested without a fresh acquisition"* ]]; chk errentry.6

SASH_CFLAGS=--no-taint-cst run_sash '
str v=initial
signal 10 EXIT
v=$(getenv DD128_VALUE)
/bin/true
exit 1
EXIT {
    attest v [[ $v == allowed ]] || v=clean
    echo "$v"
}'
[[ $err == *"cannot be attested without a fresh acquisition"* ]]; chk errentry.7

SASH_CFLAGS=--no-taint-cst run_sash '
v=$(getenv DD128_VALUE)
exit 1
ERR { v=clean; - echo "$v" }'
[[ $err == *"tainted variable"* && $err == *"SSA taint backstop"* ]]; chk errentry.8

# Collection mutations invalidate count and nonempty proofs in ERR.
run_sash '
exit 1
ERR {
   array a=(key value)
   unset a[1]
   map m = @{a:?odd}
   - echo "${m[key]}"
}'
[[ $err == *SB5101* && $err == *"odd-length map conversion"* ]]; chk errproof.1

run_sash '
exit 1
ERR {
   array a=(x)
   unset a[0]
   - /bin/echo @{a:?empty}
}'
[[ $err == *SB5101* && $err == *"empty-splat"* ]]; chk errproof.2

run_sash '
exit 1
ERR {
   array a=(x)
   - match none "^x$" a
   - /bin/echo @{a:?empty}
}'
[[ $err == *SB5101* && $err == *"empty-splat"* ]]; chk errproof.3

run_sash '
exit 1
ERR {
   v=$(arg 999 good)
   attest v [[ $v == good ]] || v=good
   - /bin/echo "$v"
}'
[[ $rc = 1 && $out = good && -z $err ]]; chk errproof.4

SASH_CFLAGS=--no-taint run_sash "exit 1; ERR {
   v=\$(arg 999 \$'x\\x00y')
   - /bin/echo \"\$v\"
}"
[[ $err == *SB5101* && $err == *"NUL-byte argv guard"* ]]; chk errproof.5

# Implicit argv iteration is bounded by the compiler's loop guard.
run_sash '
exit 1
ERR {
   for v in $(argv 1) {
       attest v [[ $v == good ]] || v=good
       - /bin/echo "$v"
   }
}' good
[[ $rc = 1 && $out = good && -z $err ]]; chk errproof.6

# Failed capture setup never executes the body or completes the assignment.
run_sash_under '
str v=before
fn capture { /bin/echo unseen }
v=$(capture)
ERR { - echo "err:${v:!}" }
' strace -qq -o capture-memfd.trace -e trace=memfd_create \
   -e inject=memfd_create:error=EIO:when=1
[[ $rc = 1 && $out = err:before && -z $err &&
  $rerr == *"capture memfd_create"* ]] &&
   grep -q 'EIO.*INJECTED' capture-memfd.trace; chk errresource.1

run_sash_under '
str v=before
fn capture { /bin/echo unseen }
v=$(capture)
ERR { - echo "err:${v:!}" }
' strace -qq -o capture-dup.trace -e trace=dup2 \
   -e inject=dup2:error=EIO:when=1
[[ $rc = 1 && $out = err:before && -z $err &&
  $rerr == *"capture dup2"* ]] &&
   grep -q 'EIO.*INJECTED' capture-dup.trace; chk errresource.2

# A failed rewind preserves completed body writes, not a fabricated empty result.
run_sash_under '
str phase=before
str v=before
fn capture { phase=body; /bin/echo captured }
v=$(capture)
ERR { - echo "err:$phase:${v:!}" }
' strace -qq -o capture-seek.trace -e trace=lseek \
   -e inject=lseek:error=EIO:when=1
[[ $rc = 1 && $out = err:body:before && -z $err &&
  $rerr == *"capture seek"* ]] &&
   grep -q 'EIO.*INJECTED' capture-seek.trace; chk errresource.3

run_sash '
exit 1
ERR {
   array a=(good)
   a+=(also)
   - /bin/echo @{a:?empty}
}'
[[ $rc = 1 && $out = "good also" && -z $err ]]; chk errproof.7

DD128_VALUE=bad run_sash '
signal 10 EXIT
for v in $(argv 1) {
    if attest v [[ $v == allowed ]] {
        :
    } else {
        /bin/kill -USR1 $$
        v=clean
    }
    /bin/true
}
EXIT { echo "$v" }
' bad
[[ $rc = 138 && $out = clean && -z $err ]]; chk sigguard.11

DD128_VALUE=bad run_sash '
str v=initial
int n=0
signal 10 EXIT
while (( n < 1 )) {
    v=$(getenv DD128_VALUE)
    (( n++ ))
    /bin/kill -USR1 $$
}
attest v [[ $v == allowed ]] || v=clean
/bin/true
EXIT { echo "$v" }
'
[[ $rc = 138 && $out = clean && -z $err ]]; chk sigguard.12

run_sash '
signal 10 EXIT
exit 0
EXIT { echo first; /bin/kill -USR1 $$; echo "$(signum):last" }
ERR { - echo unexpected }
'
[[ $rc = 0 && $out = $'first\n10:last' && -z $err ]]; chk sigguard.13

run_sash '
signal 10 EXIT
exit 7
EXIT { echo unexpected }
ERR { - echo first; - /bin/kill -USR1 $$; - echo "$(signum):last" }
'
[[ $rc = 7 && $out = $'first\n10:last' && -z $err ]]; chk sigguard.14

# Startup dispatch needs neither a working heap nor an initialized main frame.
run_sash_under '
echo unseen
EXIT { echo wrong }
ERR { - echo startup }
' strace -qq -o startup-heap.trace -e trace=mmap \
    -e inject=mmap:error=ENOMEM:when=1
[[ $rc = 1 && $out = startup && -z $err && $rerr == *"heap mmap"* ]] &&
    grep -q 'ENOMEM.*INJECTED' startup-heap.trace; chk errresource.4

run_sash '
str phase=written
int n=9223372036854775807
(( n++ ))
EXIT { echo wrong }
ERR { int code=$?; - echo "$code:$phase:$n" }
'
[[ $rc = 2 && $out = 2:written:9223372036854775807 && -z $err &&
   $rerr == *"overflow"* ]]; chk errresource.5

# EXIT may fault into ERR; fresh buffered captures in either handler are private.
run_sash '
str phase=main
EXIT {
    phase=$(echo exit; :)
    int n=0
    int zero=0
    n=$(( n / zero ))
}
ERR {
    int code=$?
    str report=$(- echo err; :)
    - echo "$code:$phase:$report"
}
'
[[ $rc = 2 && $out = 2:exit:err && -z $err &&
   $rerr == *"division by zero"* ]]; chk errresource.6

SASH_CFLAGS=--heap-size=64K run_sash '
str v=before
fn fill {
    int n=0
    while (( n < 4096 )) {
        - echo 0123456789012345678901234567890123456789012345678901234567890123456789
        (( n++ ))
    }
}
v=$(fill)
EXIT { echo wrong }
ERR { str report=$(- echo cleanup; :); - echo "$v:$report" }
'
[[ $rc = 1 && $out = before:cleanup && -z $err &&
   $rerr == *"out of memory"* ]]; chk errresource.7

SASH_CFLAGS=--heap-size=64K run_sash '
fn fill {
    int n=0
    while (( n < 4096 )) {
        - echo 0123456789012345678901234567890123456789012345678901234567890123456789
        (( n++ ))
    }
}
exit 7
EXIT { echo wrong }
ERR { - echo attempted; str report=$(fill); - echo "$report" }
'
[[ $rc = 1 && $out = attempted && -z $err &&
   $rerr == *"out of memory"* ]]; chk errresource.8

# Declaration diagnostics must name the actual placement or migration rule.
run_sash 'END { echo old }'
[[ $rc != 0 && $err == *SB4054* && $err == *'`END` is removed'* &&
   $err == *'not automatically equivalent'* ]]; chk handler128.1

run_sash 'ERR { : }; ERR { : }'
[[ $rc != 0 && $err == *SB4055* && $err == *ERR* ]]; chk handler128.2

run_sash 'fn f { ERR { : } }; f'
[[ $rc != 0 && $err == *SB4054* && $err == *ERR* ]]; chk handler128.3

run_sash 'ERR {
fn cleanup { : }
cleanup
}'
[[ $rc != 0 && $err == *':2:1: error [SB5071]'* &&
   $err == *'Move `fn cleanup` to the top level.'* ]]; chk handler128.4

run_sash 'ERR { int n=5; - echo "${n:0:1}" }'
[[ $rc != 0 && $err == *SB4030* && $err == *'substring of int variable'* ]]; chk handler128.5

# Selection uses the full integer and the cause, not the final exit byte.
run_sash 'exit 256
EXIT { echo wrong }
ERR { int code=$?; - echo "$code" }'
[[ $rc = 0 && $out = 256 && -z $err && -z $rerr ]]; chk handler128.6

run_sash 'exit 3
EXIT { echo wrong }'
[[ $rc = 3 && -z $out && -z $err && -z $rerr ]]; chk handler128.7

run_sash '- /bin/false
ERR { - echo wrong }'
[[ $rc = 1 && -z $out && -z $err && -z $rerr ]]; chk handler128.8

run_sash 'exit 3
EXIT { echo wrong }
ERR { - echo err; exit 0; - echo wrong }'
[[ $rc = 0 && $out = err && -z $err && -z $rerr ]]; chk handler128.9

run_sash 'EXIT { echo exit; exit 0; echo wrong }
ERR { - echo wrong }'
[[ $rc = 0 && $out = exit && -z $err && -z $rerr ]]; chk handler128.10

run_sash 'EXIT { echo exit; exit 9; echo wrong }
ERR { int code=$?; - echo "err:$code" }'
[[ $rc = 9 && $out = $'exit\nerr:9' && -z $err && -z $rerr ]]; chk handler128.11

DD128_VALUE=bad run_sash '
v=$(getenv DD128_VALUE)
if attest v [[ $v == allowed ]] { : } else { exit 0 }
EXIT { echo "$v" }'
[[ $rc != 0 && $err == *'enters EXIT with unresolved `v`'* &&
   $err == *'EXIT requires it'* ]]; chk handler128.12

DD128_VALUE=allowed run_sash '
v=$(getenv DD128_VALUE)
attest v [[ $v == allowed ]]
exit 1
ERR { - echo "$v" }'
[[ $rc != 0 && $err == *'ERR incoming `v` may be tainted'* &&
   $err == *'inherited data requires `:!`'* ]]; chk handler128.13

DD128_VALUE=allowed run_sash '
str v=initial
EXIT { v=$(getenv DD128_VALUE); attest v [[ $v == allowed ]] }
ERR { - echo "$v" }'
[[ $rc != 0 && $err == *'ERR incoming `v` may be tainted'* ]]; chk handler128.14

run_sash '
str v=clean
fn unused { v=$(getenv DD128_VALUE); attest v [[ $v == allowed ]] }
exit 1
ERR { - echo "$v" }'
[[ $rc = 1 && $out = clean && -z $err && -z $rerr ]]; chk handler128.15

# A false refresh path must not permit a late attest of inherited input.
run_sash '
v=$(getenv DD128_VALUE)
exit 1
ERR {
    if [[ "$$" == impossible ]] { v=$(getenv DD128_VALUE) }
    attest v [[ $v == allowed ]] || v=clean
    - echo "$v"
}'
[[ $rc != 0 && $err == *'ERR incoming `v` may be tainted'* ]]; chk handler128.16

# Heap-backed capture frames have no unrelated fixed nesting limit.
_captures='fn cap0 { - echo value; : }'
for (( _depth=1; _depth<=65; _depth++ )); do
    _captures+=$'\n'"fn cap$_depth { str v=\$(cap$((_depth-1))); - echo \"\$v\"; : }"
done
run_sash "$_captures
exit 1
ERR { str result=\$(cap65); - echo \"\$result\" }"
[[ $rc = 1 && $out = value && -z $err && -z $rerr ]]; chk errresource.9

# A second allocation fault after failed startup terminates ERR once.
run_sash_under '
ERR { - echo attempted; str report=$(- echo captured; :); - echo "$report" }
' strace -qq -o startup-second.trace -e trace=mmap \
    -e inject=mmap:error=ENOMEM:when=1
[[ $rc = 1 && $out = attempted && -z $err &&
   $rerr == *"heap mmap"* && $rerr == *"out of memory"* ]] &&
    grep -q 'ENOMEM.*INJECTED' startup-second.trace; chk errresource.10

# Faulting with both main and EXIT captures live must not divert ERR's output.
run_sash '
fn finish { exit 0; - echo unseen }
str pending=$(finish)
EXIT {
    str completed=$(int n=1; int zero=0; n=$((n / zero)); echo "$n")
    echo "$completed"
}
ERR { str report=$(- echo cleanup; :); - echo "${pending:!}:$report" }
'
[[ $rc = 2 && $out = :cleanup && -z $err &&
   $rerr == *"division by zero"* ]]; chk errresource.11

run_sash 'pattern=$(getenv DD128_PATTERN)
exit 1
ERR { - match x "${pattern:!}" }'
[[ $err == *SB5101* && $err == *"dynamic-regex"* ]]; chk errfail.17

run_sash 'exit 1; ERR {
    v=$(arg 999)
    attest v [[ $v == allowed ]] || v=clean
    - echo "$v"
}'
[[ $err == *SB5101* && $err == *"argv-bounds"* ]]; chk errfail.18

run_sash 'exit 1; ERR {
    array a
    read a
    attest item in a [[ -n $item ]] || unset a
}'
[[ $err == *SB5101* && $err == *"bulk read"* ]]; chk errfail.19

run_sash 'fn acquire { - /bin/true }
exit 1
ERR {
    v=$(acquire)
    attest v [[ $v == allowed ]] || v=clean
}'
[[ $err == *SB5101* && $err == *"descriptor capture"* ]]; chk errfail.20

run_sash 'exit 1; ERR { exec /bin/true }'
[[ $err == *SB5101* && $err == *"replacing-exec"* ]]; chk errfail.21

run_sash 'exit 1; ERR {
    array pairs=(key value)
    map values = @{pairs:?empty}
    if match x "^x$" { - echo "${values[key]}" }
}'
[[ $rc = 1 && $out = value && -z $err && -z $rerr ]]; chk errproof.8

run_sash '
fn check {
    if [[ "$$" == impossible ]] { argv A }
    if attest item in A [[ -n $item ]] { : } else { unset A }
}
argv a
exit 1
ERR { check =a }'
[[ $rc != 0 && $err == *'ERR incoming `a` may be tainted'* ]]; chk handler128.17

# Deliver SIGUSR1 during the next registration, before the acquisition.
DD128_VALUE=bad run_sash_under '
str v=initial
signal 10 EXIT
signal 12 FLAG
v=$(getenv DD128_VALUE)
attest v [[ $v == allowed ]] || v=clean
/bin/true
EXIT { echo "$v" }
' strace -qq -o pre-acquire.trace -e trace=rt_sigaction \
    -e inject=rt_sigaction:signal=SIGUSR1:when=2
[[ $rc = 138 && $out = initial && -z $err && -z $rerr ]] &&
    grep -q 'rt_sigaction(SIGUSR2' pre-acquire.trace; chk sigguard.15

run_sash '
fn fault { int zero=0; int n=$((1 / zero)); - echo "$n" }
if fault | /bin/cat { : } else { : }
echo parent
EXIT { echo orderly }
ERR { - echo wrong }
'
[[ $rc = 0 && $out = $'parent\norderly' && -z $err &&
    $rerr == *"division by zero"* ]]; chk errresource.12

tally
