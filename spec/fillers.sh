#!/bin/bash
# DD131 trial: complete batches append without replacing existing elements.
set -eu
. common.sh
setup fillers
plan=35
echo "1..$plan" >&3

set +e

run_sash 'array a=(old)
argv 1 a
attest v in a [[ -n $v ]]
echo "$(join a ,)"
argv 2 a
attest v in a [[ -n $v ]]
echo "$(join a ,)"' one two
[[ $rc = 0 && $out = $'old,one,two\nold,one,two,two' && -z $err && -z $rerr ]]; chk fillacc.1

run_sash 'map m=([first]=1 [second]=2); array a=(old)
keys m a
unset m[first]; m[third]=3
keys m a
echo "$(join a ,)"'
[[ $rc = 0 && $out = old,first,second,second,third && -z $err && -z $rerr ]]; chk fillacc.2

run_sash "array a=(old); read \$'one\\x00two\\x00' a; read '' a; echo \"\$(join a ,)\""
[[ $rc = 0 && $out = old,one,two && -z $err && -z $rerr ]]; chk fillacc.3

printf 'one\0two\0' > input
run_sash 'array a=(old)
if read a < input {
    attest v in a [[ -n $v ]]
    echo "data:$(join a ,)"
}
if read a < /dev/null {
    echo unexpected
} else {
    echo "eof:${a[0]:!},${a[1]:!},${a[2]:!}"
}'
[[ $rc = 0 && $out = $'data:old,one,two\neof:old,one,two' && -z $err && -z $rerr ]]; chk fillacc.4

run_sash 'array a=(old)
if match "12" "([0-9])([0-9])" a { echo "yes:$(join a ,)" }
if match no "^z" a { echo unexpected } else { echo "no:$(join a ,)" }'
[[ $rc = 0 && $out = $'yes:old,12,1,2\nno:old,12,1,2' && -z $err && -z $rerr ]]; chk fillacc.5

run_sash 'array a=(old)
if matchall ab12cd34 "[0-9]+" a { echo "yes:$(join a ,)" }
if matchall none "[0-9]+" a { echo unexpected } else { echo "no:$(join a ,)" }'
[[ $rc = 0 && $out = $'yes:old,12,34\nno:old,12,34' && -z $err && -z $rerr ]]; chk fillacc.6

run_sash 'argv 1 a; attest v in a [[ -n $v ]]
map m=([key]=value); keys m b
read "word" c
if match word "or" d { : }
if matchall none "[0-9]+" e { : }
echo "$(len a):$(join b):$(join c):$(join d):$(len e)"' argument
[[ $rc = 0 && $out = 1:key:word:or:0 && -z $err && -z $rerr ]]; chk fillacc.7

run_sash 'fn fill { read "next" A }; fn relay { fill =A }
array a=(old); relay =a; echo "$(join a ,)"'
[[ $rc = 0 && $out = old,next && -z $err && -z $rerr ]]; chk fillacc.8

run_sash 'array a=(first)
for v in @a {
    echo "$v"
    if [[ $v == first ]] { read "second" a }
}'
[[ $rc = 0 && $out = $'first\nsecond' && -z $err && -z $rerr ]]; chk fillacc.9

run_sash 'array a=(first); array _snapshot=(@a)
read "second" a
echo "$(join _snapshot ,)"; echo "$(join a ,)"'
[[ $rc = 0 && $out = $'first\nfirst,second' && -z $err && -z $rerr ]]; chk fillacc.10

run_sash "array a=(\$'a\\x00b' old); read \$'\\x00next\\x00' a
echo \"\$(strlen \${a[0]}):\$(len a):[\${a[2]}]:\${a[3]}\""
[[ $rc = 0 && $out = '3:4:[]:next' && -z $err && -z $rerr ]]; chk fillacc.11

run_sash 'array a=(one two three); unset a[1]; read "next" a
echo "${a[0]}:${a[2]}:${a[3]}:$(len a)"'
[[ $rc = 0 && $out = one:three:next:3 && -z $err && -z $rerr ]]; chk fillacc.12

run_sash 'int array n=(1 2 3); unset n[0]; n+=($((4)) 5)
echo "${n[1]}:${n[2]}:${n[3]}:${n[4]}"'
[[ $rc = 0 && $out = 2:3:4:5 && -z $err && -z $rerr ]]; chk fillacc.13

run_sash 'array a=(); a[10]=ten; read "next" a; read "" a
echo "${a[10]}:${a[11]}:$(len a)"'
[[ $rc = 0 && $out = ten:next:2 && -z $err && -z $rerr ]]; chk fillacc.14

run_sash 'array a=(); a[9223372036854775807]=last; read "" a
ERR { - echo "${a[9223372036854775807]}:$(len a)" }
read "next" a
echo unexpected'
[[ $rc = 2 && $out = last:1 && -z $err && $rerr == *"arithmetic overflow"* ]]; chk fillacc.15

run_sash 'argv 1 a
if attest v in a [[ $v == safe ]] { : } else { read "safe" a }
echo "$(join a ,)"' unsafe
[[ $rc != 0 && $err == *SB5089* ]]; chk fillacc.16

run_sash 'map m=([safe]=value); argv 1 a
if attest v in a [[ $v == safe ]] { : } else { keys m a }
echo "$(join a ,)"' unsafe
[[ $rc != 0 && $err == *SB5089* ]]; chk fillacc.17

run_sash 'argv 1 a
if attest v in a [[ $v == safe ]] { : } else { - match safe "safe" a }
echo "$(join a ,)"' unsafe
[[ $rc != 0 && $err == *SB5089* ]]; chk fillacc.18

run_sash "ERR { array a=(\$'a\\x00b'); read \"safe\" a; - /bin/echo @{a:-empty} }; fail 7"
[[ $rc != 0 && $err == *SB5101* && $err == *NUL* ]]; chk fillacc.19

run_sash 'ERR { array a=(old); read "safe" a; - /bin/echo @{a:-empty} }; fail 7'
[[ $rc = 7 && $out = 'old safe' && -z $err && -z $rerr ]]; chk fillacc.20

run_sash 'fn writer { /bin/echo dirty }
writer | read a
argv 1 a
attest v in a [[ -n $v ]]
echo "$(join a ,)"
writer | read b
echo "${b[0]:!}"' safe
[[ $rc != 0 && $err == *SB5100* ]]; chk fillacc.21

SASH_CFLAGS=--no-taint-cst run_sash 'argv 1 a
if attest v in a [[ $v == safe ]] { : } else { read "safe" a }
echo "$(join a ,)"' unsafe
[[ $rc != 0 && $err == *tainted* ]]; chk fillacc.22

run_sash "array a=(); a[9223372036854775806]=old
ERR { - echo \"\${a[9223372036854775806]}:\${a[9223372036854775807]}:\$(len a)\" }
read \$'first\\x00second' a
echo unexpected"
[[ $rc = 2 && $out = old::1 && -z $err && $rerr == *"arithmetic overflow"* ]]; chk fillacc.23

printf -v batch 'new\\x00%.0s' {1..200}
SASH_CFLAGS=--heap-size=96K run_sash "array a=()
int i=0
while (( i < 400 )) { a+=(old); (( i++ )) }
ERR { - echo \"\${a[399]}:\${a[400]}:\$(len a)\" }
echo ready
read \$'$batch' a
echo unexpected"
[[ $rc != 0 && $out = $'ready\nold::400' && -z $err && $rerr == *40960* ]]; chk fillacc.24

run_sash 'array a=(); int i=-2; a[$i]=past; read "next" a
echo "${a[$i]}:${a[0]}:$(len a)"'
[[ $rc = 0 && $out = past:next:2 && -z $err && -z $rerr ]]; chk fillacc.25

run_sash 'array a=(); a[9223372036854775806]=old; read "new" a; read "" a
echo "${a[9223372036854775806]}:${a[9223372036854775807]}:$(len a)"'
[[ $rc = 0 && $out = old:new:2 && -z $err && -z $rerr ]]; chk fillacc.26

run_sash 'fn fill { argv 1 A; attest v in A [[ -n $v ]] }
argv 1 a
fill =a
echo "$(join a ,)"' data
[[ $rc != 0 && $err == *SB5088* ]]; chk fillacc.27

run_sash 'int array n=(1); n+=(word); echo "${n[1]}"'
[[ $rc != 0 && $err == *SB4030* && $err == *"non-numeric element"* ]]; chk fillacc.28

run_sash 'array a=(one two three); unset a[0]; a+=(four five)
echo "$(join a ,)"'
[[ $rc = 0 && $out = two,three,four,five && -z $err && -z $rerr ]]; chk fillacc.29

run_sash 'int array a=(99); a=(4 $((5)))
int map m=([old]=99); m=([new]=7)
echo "${a[0]}:${a[1]}:$(len a):${m[new]}:$(len m)"'
[[ $rc = 0 && $out = 4:5:2:7:1 && -z $err && -z $rerr ]]; chk fillacc.30

SASH_CFLAGS=--no-taint-cst run_sash 'argv 1 a
if attest v in a [[ $v == safe ]] { : } else { - match safe "safe" a }
echo "$(join a ,)"' unsafe
[[ $rc != 0 && $err == *tainted* ]]; chk fillacc.31

SASH_CFLAGS=--heap-size=256K run_sash 'array a
int i=0
while (( i < 1000 )) { a+=(left right); (( i++ )) }
echo "${a[0]}:${a[1999]}:$(len a)"'
[[ $rc = 0 && $out = left:right:2000 && -z $err && -z $rerr ]]; chk appendperf.1

run_sash "fn grow { read \$'new\\x00next' A }
array a=(zero one two)
unset a[2]; grow =a
echo \"\${a[2]}:\${a[3]}:\$(len a)\"
unset a[3]; unset a[2]; a[10]=ten; grow =a
echo \"\${a[10]}:\${a[11]}:\${a[12]}:\$(len a)\"
a=(); int neg=-2; a[\$neg]=negative; a[9223372036854775807]=last
unset a[9223372036854775807]; grow =a
echo \"\${a[\$neg]}:\${a[0]}:\${a[1]}:\$(len a)\"
unset a; grow =a
echo \"\${a[0]}:\${a[1]}:\$(len a)\"
unset a[0]; unset a[1]; grow =a
echo \"\${a[0]}:\${a[1]}:\$(len a)\""
[[ $rc = 0 && $out = $'new:next:4\nten:new:next:5\nnegative:new:next:3\nnew:next:2\nnew:next:2' && -z $err && -z $rerr ]]; chk appendperf.2

run_sash "fn grow { read \$'e\\x00f\\x00g\\x00h\\x00i\\x00j\\x00k\\x00l\\x00m\\x00n\\x00o\\x00p' A }
array a=(a b c d); array _snapshot=(@a)
for v in @a {
    echo \"\$v\"
    if [[ \$v == b ]] { unset a[2]; grow =a }
}
echo \"\$(join _snapshot ,):\$(len a)\""
[[ $rc = 0 && $out = $'a\nb\nd\ne\nf\ng\nh\ni\nj\nk\nl\nm\nn\no\np\na,b,c,d:15' && -z $err && -z $rerr ]]; chk appendperf.3

# The complete batch is ready before the destination's entry reservation fails.
printf -v batch 'new\\x00%.0s' {1..200}
SASH_CFLAGS=--heap-size=64K run_sash "array a=()
int i=0
while (( i < 400 )) { a+=(old); (( i++ )) }
ERR { - echo \"\${a[399]}:\${a[400]}:\$(len a)\" }
echo ready
read \$'$batch' a
echo unexpected"
[[ $rc = 1 && $out = $'ready\nold::400' && -z $err && $rerr == *"out of memory (requested 40960 bytes)"* ]]; chk appendperf.4

tally
