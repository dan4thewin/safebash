#!/bin/bash
# DD98 / provisional DD136: each write-only _ binding is an independent sink.
. common.sh
plan=22
setup discard

set +e

run_sash '_=one
_=2
int _=3
str _=four
array _=(five six)
map _=([seven]=eight)
echo kept'
[[ $rc = 0 && $out = kept && -z $err && -z $rerr ]]; chk discardv.1

run_sash 'int n=0
fn value -> int { (( n+=1 )); return $n }
_=$(value)
_=$(( (value) + 1 ))
echo "$n"'
[[ $rc = 0 && $out = 2 && -z $err && -z $rerr ]]; chk discardv.2

run_sash 'str s=""
_=${s:?empty}
echo lost'
[[ $rc = 1 && -z $out && $rerr == *": empty"* ]]; chk discardv.3

run_sash 'array a
array _=(@{a:?empty})
echo lost'
[[ $rc = 1 && -z $out && $rerr == *": empty"* ]]; chk discardv.4

run_sash 'int zero=0
_=$(( 1 / zero ))
echo lost'
[[ $rc = 2 && -z $out && $rerr == *"division by zero"* ]]; chk discardv.5

run_sash 'str s=""
_="$s"
_=$'\''a\x00b'\''
array a=($'\''c\x00d'\'' "")
array _=(@a)
echo kept'
[[ $rc = 0 && $out = kept && -z $err && -z $rerr ]]; chk discardv.6

run_sash 'int n=0
fn text { (( n+=1 )); echo discarded }
_=$(text)
_=$(text)
echo "$n"'
[[ $rc = 0 && $out = 2 && -z $err && -z $rerr ]]; chk discardv.7

run_sash '_=one; echo $_'
[[ $err == *SB5056* ]]; chk discardv.8

run_sash '_=one; echo ${_:!}'
[[ $err == *SB5056* ]]; chk discardv.9

run_sash '_=one; _+=two'
[[ $err == *SB5056* ]]; chk discardv.10

run_sash 'int _=1; (( _+=1 ))'
[[ $err == *SB5056* ]]; chk discardv.11

run_sash 'array _=(one); echo $(len _)'
[[ $err == *SB5056* ]]; chk discardv.12

run_sash 'fn use { echo $(len A) }; array _=(one); use =_'
[[ $err == *SB5056* ]]; chk discardv.13

run_sash 'array _=(one); echo @_'
[[ $err == *SB5056* ]]; chk discardv.14

run_sash 'int count=0
for _ in one two { (( count+=1 )) }
for _ in three { (( count+=1 )) }
echo "$count"'
[[ $rc = 0 && $out = 3 && -z $err && -z $rerr ]]; chk discardv.15

printf 'one\ntwo\n' >input
run_sash 'readline _ <input
readline _ <input
echo kept'
[[ $rc = 0 && $out = kept && -z $err && -z $rerr ]]; chk discardv.16

run_sash 'str n=12
attest n (( _=n ))
str m=3
attest m (( _=m ))
echo "$n:$m"'
[[ $rc = 0 && $out = 12:3 && -z $err && -z $rerr ]]; chk discardv.17

run_sash 'fn use { _=$1; int _=2; array _=(@args); echo kept }
use one
use two'
[[ $rc = 0 && $out = $'kept\nkept' && -z $err && -z $rerr ]]; chk discardv.18

run_sash '_=one
ERR { str _=two; int _=3; echo kept }
fail 9'
[[ $rc = 9 && $out = kept && -z $err && -z $rerr ]]; chk discardv.19

run_sash 'fn predicate { str s=""; _=${s:?empty}; return 0 }
if predicate { echo lost }
echo after'
[[ $rc = 1 && -z $out && -z $err && $rerr == *": empty"* ]]; chk discardv.20

run_sash 'array a=(present)
_=${a[9]:?missing cell}
echo lost'
[[ $rc = 1 && -z $out && -z $err && $rerr == *": missing cell"* ]]; chk discardv.21

run_sash 'fn consume { str value=""; _=${value:?child guard}; echo lost }
consume | /bin/cat
echo after'
[[ $rc = 1 && -z $out && -z $err && $rerr == *": child guard"* ]]; chk discardv.22

set -e
tally
