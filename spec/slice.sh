#!/bin/bash
# DD137 provisional: typed, independent slices through the ordinary filler path.
set -eu
. common.sh
setup slice
plan=62
echo "1..$plan" >&3

set +e

run_sash 'array items=(a b c d)
slice items 1 _tail
slice items 1 2 _middle
echo "$(join _tail ,)"
echo "$(join _middle ,)"'
[[ $rc = 0 && $out = $'b,c,d\nb,c' && -z $err && -z $rerr ]]; chk slice.1

run_sash 'array items=(a b c); array empty
slice empty 0 _empty
slice items 3 _end
slice items 9223372036854775807 _past
slice items 1 0 _zero
slice items 1 9223372036854775807 _huge
echo "$(len _empty):$(len _end):$(len _past):$(len _zero):$(join _huge ,)"'
[[ $rc = 0 && $out = '0:0:0:0:b,c' && -z $err && -z $rerr ]]; chk slice.2

# Positions are live-entry ordinals, not numeric keys or a numeric-key sort.
run_sash 'array items
items[9]=nine; items[2]=two; items[8]=eight; items[1]=one
unset items[2]
slice items 1 2 _part
echo "$(join _part ,):${_part[0]}:${_part[1]}:$(len _part)"'
[[ $rc = 0 && $out = 'eight,one:eight:one:2' && -z $err && -z $rerr ]]; chk slice.3

run_sash 'array items=(a b c); items[7]=last
slice items 1 2 items
echo "$(join items ,):${items[8]}:${items[9]}:$(len items)"'
[[ $rc = 0 && $out = 'a,b,c,last,b,c:b:c:6' && -z $err && -z $rerr ]]; chk slice.4

run_sash 'array original=(a b c)
slice original 0 copy
original[0]=changed; unset original[1]; copy[2]=own
echo "$(join original ,):$(join copy ,)"'
[[ $rc = 0 && $out = 'changed,c:a,b,own' && -z $err && -z $rerr ]]; chk slice.5

run_sash 'int array numbers=(4 -5 9223372036854775807)
slice numbers 1 copy
int value=${copy[0]}; (( value += 2 ))
echo "$value:$(join copy ,)"'
[[ $rc = 0 && $out = '-3:-5,9223372036854775807' && -z $err && -z $rerr ]]; chk slice.6

run_sash 'int array source=(4 5); int array dest=(7)
dest[8]=9
slice source 0 dest
(( dest[9] += 2 ))
echo "$(join dest ,):${dest[10]}"'
[[ $rc = 0 && $out = '7,9,6,5:5' && -z $err && -z $rerr ]]; chk slice.7

run_sash "array source=(\$'a\\x00b' '' \$'x\\ny')
slice source 0 copy
write \${copy[0]} > bytes
echo \"\$(len copy):\$(strlen \${copy[1]}):\$(strlen \${copy[2]})\""
[[ $rc = 0 && $out = '3:0:3' && -z $err && -z $rerr \
   && $(od -An -tx1 bytes | tr -d ' \n') = 610062 ]]; chk slice.8

run_sash 'array items=(a b c); int i=0
while (( i < 3 )) {
    slice items $i 1 _part
    echo "$(join _part ,):$(len _part)"
    (( i += 1 ))
}'
[[ $rc = 0 && $out = $'a:1\nb:1\nc:1' && -z $err && -z $rerr ]]; chk slice.9

run_sash 'fn remaining {
    slice args 1 _rest
    echo "[$(join _rest ,)]"
}
remaining
remaining first
remaining first second third'
[[ $rc = 0 && $out = $'[]\n[]\n[second,third]' && -z $err && -z $rerr ]]; chk slice.10

run_sash 'fn remaining { echo $2; slice args 2 _rest; echo "$(join _rest ,)" }; remaining first'
[[ $rc != 0 && $err == *SB5042* ]]; chk slice.11

run_sash 'fn remaining {
    echo "${args[0]:-fallback}"
    slice args 1 _rest
    echo "[$(join _rest ,)]"
}
remaining
remaining first second'
[[ $rc = 0 && $out = $'fallback\n[]\nfirst\n[second]' && -z $err && -z $rerr ]]; chk slice.12

# A derived array retains an inferred dependency on its caller's slot type.
run_sash 'fn show { int n=${A[0]}; (( n += 1 )); echo $n }
fn select { slice A 1 _part; show =_part }
fn relay { select =A }
int array values=(3 8 13)
relay =values'
[[ $rc = 0 && $out = 9 && -z $err && -z $rerr ]]; chk slice.13

run_sash 'fn copy { slice A 1 B }
int array source=(2 3 4); int array target=(9)
copy =source =target
echo "$(join target ,)"'
[[ $rc = 0 && $out = '9,3,4' && -z $err && -z $rerr ]]; chk slice.14

run_sash 'fn numbers { slice A 0 _part; int n=${_part[0]}; (( n += 1 )); echo $n }
fn strings { slice A 0 _part; echo "$(toupper ${_part[0]})" }
int array ints=(3); array words=(word)
numbers =ints; strings =words'
[[ $rc = 0 && $out = $'4\nWORD' && -z $err && -z $rerr ]]; chk slice.15

run_sash 'slice numbers 0 first
int array numbers=(4 5)
slice numbers 0 second
int n=${second[0]}; (( n += 1 ))
echo "$(len first):$n"'
[[ $rc = 0 && $out = '0:5' && -z $err && -z $rerr ]]; chk slice.16

# Retain START before COUNT changes it; snapshot SOURCE after COUNT's effects.
run_sash 'int start=1; array items=(a b c d)
fn limit -> int { (( start = 0 )); items[1]=B; return 2 }
slice items $start $(limit) _part
echo "$start:$(join _part ,)"'
[[ $rc = 0 && $out = '0:B,c' && -z $err && -z $rerr ]]; chk slice.17

run_sash 'int calls=0
fn begin -> int { (( calls += 1 )); return 1 }
fn count -> int { (( calls += 10 )); return 2 }
array items=(a b c d)
slice items $(begin) $(( (count) )) _part
echo "$calls:$(join _part ,)"'
[[ $rc = 0 && $out = '11:b,c' && -z $err && -z $rerr ]]; chk slice.18

run_sash 'array items=(a b); str start=1; slice items $start copy; echo "$(join copy ,)"'
[[ $rc != 0 && $err == *SB4030* && $err == *'nonnegative integers'* ]]; chk slice.19

run_sash 'array items=(a b); slice items 0 "1" copy; echo "$(join copy ,)"'
[[ $rc != 0 && $err == *SB4030* && $err == *'nonnegative integers'* ]]; chk slice.20

run_sash 'array items=(a b); slice items -1 copy; echo "$(join copy ,)"'
[[ $rc != 0 && $err == *SB4030* && $err == *'nonnegative integers'* ]]; chk slice.21

run_sash 'array items=(a b); slice items 0 -1 copy; echo "$(join copy ,)"'
[[ $rc != 0 && $err == *SB4030* && $err == *'nonnegative integers'* ]]; chk slice.22

run_sash 'array dest=(old)
ERR { echo "ERR:$(join dest ,)" }
int start=-1; array items=(a b)
slice items $start dest
echo after'
[[ $rc = 1 && $out = 'ERR:old' && -z $err \
   && $rerr == *'slice start and count must be nonnegative'* ]]; chk slice.23

run_sash 'array dest=(old)
ERR { echo "ERR:$(join dest ,)" }
int count=-1; array items
slice items 0 $count dest
echo after'
[[ $rc = 1 && $out = 'ERR:old' && -z $err \
   && $rerr == *'slice start and count must be nonnegative'* ]]; chk slice.24

run_sash 'map source=([key]=value); slice source 0 copy; echo "$(join copy ,)"'
[[ $rc != 0 && $err == *SB4030* && $err == *'indexed array source'* ]]; chk slice.25

run_sash 'str source=value; slice source 0 copy; echo "$(join copy ,)"'
[[ $rc != 0 && $err == *SB4030* && $err == *'indexed array source'* ]]; chk slice.26

run_sash 'array source=(a); int array target; slice source 0 target; echo "$(join target ,)"'
[[ $rc != 0 && $err == *SB4030* && $err == *'produces string elements'* ]]; chk slice.27

run_sash 'int array source=(3); array target; slice source 0 target; echo "$(join target ,)"'
[[ $rc != 0 && $err == *SB4030* && $err == *'produces int elements'* ]]; chk slice.28

run_sash 'fn copy { slice A 0 B }
int array source=(3); array target
copy =source =target; echo "$(join target ,)"'
[[ $rc != 0 && $err == *SB4030* && $err == *'produces int elements'* ]]; chk slice.29

run_sash 'array source=(a); slice $source 0 copy; echo "$(join copy ,)"'
[[ $rc != 0 && $err == *SB5062* && $err == *'bare indexed-array source name'* ]]; chk slice.30

run_sash 'array source=(a); slice "source" 0 copy; echo "$(join copy ,)"'
[[ $rc != 0 && $err == *SB5062* && $err == *'bare indexed-array source name'* ]]; chk slice.31

run_sash 'array source=(a); slice source 0 $copy'
[[ $rc != 0 && $err == *SB5063* ]]; chk slice.32

run_sash 'array source=(a); slice source copy'
[[ $rc != 0 && $err == *SB4045* && $err == *'SOURCE START [COUNT] DEST'* ]]; chk slice.33

run_sash 'array source=(a); slice source 0 1 copy extra'
[[ $rc != 0 && $err == *SB4045* && $err == *'SOURCE START [COUNT] DEST'* ]]; chk slice.34

run_sash 'array source=(a); array _part=(old); slice source 0 _part; echo "$(join _part ,)"'
[[ $rc != 0 && $err == *SB5103* && $err == *'`_part`'* ]]; chk slice.35

run_sash 'slice _part 0 _part; echo "$(join _part ,)"'
[[ $rc != 0 && $err == *SB5104* && $err == *'`_part`'* ]]; chk slice.36

run_sash 'fn count -> int { return $(len _part) }
array source=(a); slice source 0 $(count) _part; echo "$(join _part ,)"'
[[ $rc != 0 && $err == *SB5104* && $err == *'`_part`'* ]]; chk slice.37

run_sash 'fn copy { slice args 0 args; echo @args }; copy a b'
[[ $rc != 0 && $err == *SB5052* ]]; chk slice.38

run_sash 'argv 1 source; slice source 0 copy; echo "$(join copy ,)"' value
[[ $rc != 0 && $err == *SB5020* && $err == *'slice of tainted collection'* ]]; chk slice.39

run_sash 'argv 1 dest; array source=(clean); slice source 0 dest; echo "$(join dest ,)"' value
[[ $rc != 0 && $err == *SB5020* && $err == *'dest'* ]]; chk slice.40

SASH_CFLAGS=--no-taint-cst run_sash 'argv 1 source; slice source 0 copy; echo "$(join copy ,)"' value
[[ $rc != 0 && $err == *tainted* && $err == *'slice source'* ]]; chk slice.41

run_sash 'fn copy { slice source 0 A }
array source=(value); copy =source; echo "$(join source ,)"'
[[ $rc != 0 && $err == *SB5099* && $err == *source* ]]; chk slice.42

run_sash 'fn copy { slice A 1 _part; echo "$(join _part ,)" }
array _source=(a b c); copy =_source'
[[ $rc = 0 && $out = 'b,c' && -z $err && -z $rerr ]]; chk slice.43

SASH_CFLAGS=--no-taint run_sash 'fn copy { slice A 0 B }
array source=(value); array _dest=(old); copy =source =_dest; echo "$(join _dest ,)"'
[[ $rc != 0 && $err == *SB5103* && $err == *'`_dest`'* ]]; chk slice.44

run_sash 'array source=(a); if slice source 0 copy { echo "$(join copy ,)" }'
[[ $rc != 0 && $err == *SB4025* && $err == *'no result'* ]]; chk slice.45

run_sash 'array source=(a); - slice source 0 copy; echo "$(join copy ,)"'
[[ $rc != 0 && $err == *SB4076* && $err == *'no result'* ]]; chk slice.46

run_sash 'fn copy { return slice A 0 B }
array source=(a); array dest; copy =source =dest; echo "$(join dest ,)"'
[[ $rc != 0 && $err == *SB4025* && $err == *'no result'* ]]; chk slice.47

run_sash 'fn remaining { slice args 1 _rest; /bin/echo @_rest }; remaining first second'
[[ $rc != 0 && $err == *SB5032* ]]; chk slice.48

run_sash 'array source=(a b c)
str text=$(slice source 1 _part; echo "$(join _part ,)")
echo "$text"'
[[ $rc = 0 && $out = 'b,c' && -z $err && -z $rerr ]]; chk slice.49

run_sash 'array source=(clean)
fn count -> int { argv 1 source; return 1 }
slice source 0 $(count) _part
echo "$(join _part ,)"' dirty
[[ $rc != 0 && $err == *SB5020* && $err == *'slice of tainted collection'* ]]; chk slice.50

run_sash 'array source=(a b); int count=1; int i=0
ERR { echo "ERR:$(len _part)" }
while (( i < 2 )) {
    slice source 0 $count _part
    echo "$(join _part ,)"
    (( count = -1 )); (( i += 1 ))
}'
[[ $rc = 1 && $out = $'a\nERR:0' && -z $err \
   && $rerr == *'slice start and count must be nonnegative'* ]]; chk slice.51

run_sash 'array source=(a b c)
ERR { slice source 1 _part; echo "$(join _part ,)" }
fail 3'
[[ $rc = 3 && $out = 'b,c' && -z $err && -z $rerr ]]; chk slice.52

run_sash 'array source=(a); array dest=(old)
ERR { slice source 0 dest; echo "$(join dest ,)" }
fail 3'
[[ $rc != 0 && $err == *SB5102* && $err == *'inherited binding `dest`'* ]]; chk slice.53

run_sash 'array _patterns=(a b); attest pattern in _patterns r?
slice _patterns 0 _copy
if match ab "${_copy[0]:?missing}" { echo matched }'
[[ $rc != 0 && $err == *SB5106* && $err == *'validated by r?'* ]]; chk slice.54

run_sash 'array source=(a); map dest=([old]=value); slice source 0 dest; echo "${dest[old]}"'
[[ $rc != 0 && $err == *SB4030* && $err == *'cannot fill the map'* ]]; chk slice.55

run_sash 'array source=(a b); array dest; dest[8]=old
slice source 0 0 dest
slice source 9 dest
echo "$(len dest):${dest[8]}"'
[[ $rc = 0 && $out = '1:old' && -z $err && -z $rerr ]]; chk slice.56

# A capture body can collect a bound's stdout without inventing a slice value.
run_sash 'fn begin -> int { echo begin; return 1 }
array source=(a b); array dest
str output=$(slice source $(begin) dest)
echo "$output:$(join dest ,)"'
[[ $rc = 0 && $out = 'begin:b' && -z $err && -z $rerr ]]; chk slice.57

run_sash 'array source=(a b)
/bin/true && slice source 1 _part
echo "$(join _part ,)"'
[[ $rc = 0 && $out = b && -z $err && -z $rerr ]]; chk slice.58

run_sash 'array source=(a b)
if ! slice source 1 _part { echo "$(join _part ,)" }'
[[ $rc != 0 && $err == *SB4025* && $err == *'no result'* ]]; chk slice.59

run_sash 'array source=(a)
fn invalid { int start=-1; slice source $start _part; echo "$(join _part ,)" }
ERR { : invalid; echo after }
fail 7'
[[ $rc = 1 && -z $out && -z $err \
   && $rerr == *'slice start and count must be nonnegative'* ]]; chk slice.60

SASH_CFLAGS=--heap-size=16K run_sash 'array source; array dest=(old); int i=0
ERR { echo "ERR:$(join dest ,)" }
while (( i < 128 )) { source[$i]=cell; (( i += 1 )) }
echo ready
slice source 0 dest
echo "done:$(len dest)"'
[[ $rc = 1 && $out = $'ready\nERR:old' && -z $err \
   && $rerr == *'out of memory'* ]]; chk slice.61

SASH_CFLAGS=--heap-size=64K run_sash 'array source; array dest=(old); int i=0
ERR { echo "ERR:$(join dest ,)" }
while (( i < 128 )) { source[$i]=cell; (( i += 1 )) }
echo ready
slice source 0 dest
echo "done:$(len dest)"'
[[ $rc = 0 && $out = $'ready\ndone:129' && -z $err && -z $rerr ]]; chk slice.62

tally
