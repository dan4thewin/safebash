#!/bin/bash
# spec/collparam.sh — DD#100 collection parameters.
#
# A collection reaches a user function through a distinct set of slots
# (`A`..`E`), bound at the call site with the `=name` sigil:
#
#     fn show { echo "$1 has $(len A) items: $(join A ,)" }
#     array nums=(3 1 4)
#     show "nums" =nums
#
# The slot IS the caller's collection (Part E) — not a copy — so mutation is
# visible to the caller, and taint crosses the boundary in both directions.
#
# Runtime slot tests exercise the transform's slot binding; compile-time
# diagnostics stay front-end checks.
set -eu
. common.sh
setup collparam
set +e
plan=158
echo "1..$plan" >&3

# ── Part A: binding and reading ───────────────────────────────────

# 100.1 the DD's own example
run_sash 'fn show {
  echo "$1 has $(len A) items: $(join A ,)"
}
array nums=(3 1 4)
show "nums" =nums'
[[ $rc = 0 && $out = "nums has 3 items: 3,1,4" && -z $err ]]; chk 100.1

# 100.2 subscript read through a slot
run_sash 'fn first { echo "${A[0]}" }
array a=(p q r)
first =a'
[[ $rc = 0 && $out = "p" && -z $err ]]; chk 100.2

# 100.3 splat a slot in for-in
run_sash 'fn each { for v in @A; do echo "$v"; done }
array a=(x y)
each =a'
[[ $rc = 0 && $out = "x
y" && -z $err ]]; chk 100.3

# 100.4 two slots bind in order
run_sash 'fn both { echo "$(len A)/$(len B)" }
array a=(1 2 3)
array b=(9)
both =a =b'
[[ $rc = 0 && $out = "3/1" && -z $err ]]; chk 100.4

# 100.5 scalars and collections together — scalars keep their `$1` positions
run_sash 'fn mix { echo "$1 $2 $(len A)" }
array a=(1 2)
mix one two =a'
[[ $rc = 0 && $out = "one two 2" && -z $err ]]; chk 100.5

# 100.6 forwarding: a slot may be passed on as a collection argument
run_sash 'fn inner { echo "i=$(len A)" }
fn outer { inner =A }
array a=(1 2 3)
outer =a'
[[ $rc = 0 && $out = "i=3" && -z $err ]]; chk 100.6

# 100.7 five slots is the maximum, and all five bind
run_sash 'fn five { echo "$(len A)$(len B)$(len C)$(len D)$(len E)" }
array a=(1)
array b=(1 2)
array c=(1 2 3)
array d=(1 2 3 4)
array e=(1 2 3 4 5)
five =a =b =c =d =e'
[[ $rc = 0 && $out = "12345" && -z $err ]]; chk 100.7

# ── Part E: the slot is the caller's collection ───────────────────

# 100.8 a write through a slot is visible to the caller
run_sash 'fn setit { A[0] = z }
array a=(k)
setit =a
echo "${a[0]}"'
[[ $rc = 0 && $out = "z" && -z $err ]]; chk 100.8

# 100.9 DD#100's non-aliasing contract rejects one identity in two slots.
run_sash 'fn two { A[0] = z; echo "${B[0]}" }
array a=(k)
two =a =a'
[[ $rc != 0 && $err == *SB5099* && $err == *'distinct identities'* ]]; chk 100.9

# 100.10 a bulk fill through a slot is an out-parameter. Unresolved, `read A`
# filled a function-local array named `A` and the caller's collection was left
# untouched — silently, which is the whole point of Part E.
run_sash 'fn split_into { read $1 A }
sep=$'"'"'\x00'"'"'
str s=a:b:c
s =~ s/:/$sep/g
array f=()
split_into $s =f
echo "$(len f):${f[0]}:${f[2]}"'
[[ $rc = 0 && $out = "3:a:c" && -z $err ]]; chk 100.10

# ── Part B: a slot's kind comes from its call sites ───────────────

# 100.11 a map slot indexes by string key
run_sash 'fn get { echo "${A[x]}" }
map m=([x]=hi)
get =m'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 100.11

# 100.12 `keys` works on a map slot
run_sash 'fn ks { for k in $(keys A); do echo "k=$k"; done }
map m=([x]=1)
ks =m'
[[ $rc = 0 && $out = "k=x" && -z $err ]]; chk 100.12

# 100.13 `keys` on an ARRAY slot is still the DD#75 §3 error — the permissive
# reading (skip kind checks on a slot) printed garbage here
run_sash 'fn ks { for k in $(keys A); do echo "k=$k"; done }
array a=(p q)
ks =a'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 100.13

# 100.14 a string key on an ARRAY slot is still the DD#80 error — permissive,
# this silently returned element 0
run_sash 'fn get { echo "${A[x]}" }
array a=(p q)
get =a'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 100.14

# 100.15 call sites that disagree on a slot's kind are rejected
run_sash 'fn n { echo "$(len A)" }
array a=(1)
map m=([x]=1)
n =a
n =m'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 100.15

# ── SB5065: `A`..`E` are reserved ─────────────────────────────────

# 100.16 a slot name means nothing at the top level
run_sash 'array A=(1 2)
echo "$(len A)"'
[[ $rc != 0 && -z $out && $err == *SB5065* ]]; chk 100.16

# 100.17 a slot may not be re-declared inside a function
run_sash 'fn f {
  array A=(1)
  echo "$(len A)"
}
f'
[[ $rc != 0 && -z $out && $err == *SB5065* ]]; chk 100.17

# 100.18 a slot may not be assigned as a whole name
run_sash 'fn f { A = 5 }
f'
[[ $rc != 0 && -z $out && $err == *SB5065* ]]; chk 100.18

# 100.19 a top-level read of a slot name is SB5065, not "undeclared"
run_sash 'echo "$A"'
[[ $rc != 0 && -z $out && $err == *SB5065* ]]; chk 100.19

# ── SB5066: the call-site rules ───────────────────────────────────

# 100.20 `=name` is only a sigil at a user-function call; `echo` still prints it
run_sash 'echo =a'
[[ $rc = 0 && $out = "=a" && -z $err ]]; chk 100.20

# 100.21 quoting is the escape hatch — `f "=nums"` passes the literal word
run_sash 'fn f { echo ${args[0]} }
f "=nums"'
[[ $rc = 0 && $out = "=nums" && -z $err ]]; chk 100.21

# 100.22 Part C: every scalar argument comes before every collection one
run_sash 'fn f { echo "$1 $(len A)" }
array a=(1)
f =a hi'
[[ $rc != 0 && -z $out && $err == *SB5066* ]]; chk 100.22

# 100.23 a sixth collection has no slot
run_sash 'fn f { echo "$(len A)$(len B)$(len C)$(len D)$(len E)" }
array a=(1)
f =a =a =a =a =a =a'
[[ $rc != 0 && -z $out && $err == *SB5066* ]]; chk 100.23

# 100.24 `=s` on a scalar has nothing to bind
run_sash 'fn f { echo "$(len A)" }
str s=hi
f =s'
[[ $rc != 0 && -z $out && $err == *SB5066* ]]; chk 100.24

# 100.25 `=name` on a name introduced nowhere
run_sash 'fn f { echo "$(len A)" }
f =nope'
[[ $rc != 0 && -z $out && $err == *SB5066* ]]; chk 100.25

# 100.26 a slot the body uses must actually be filled — this silently read as
# an empty collection and printed `n=0`
run_sash 'fn f { echo "n=$(len A)" }
f'
[[ $rc != 0 && -z $out && $err == *SB5066* ]]; chk 100.26

# ── Taint across the call boundary ────────────────────────────────

# 100.27 inward: the body's read is diagnosed at the tainted call.
run_sash 'fn f { echo "${A[0]}" }
argv a
f =a'
[[ $rc != 0 && -z $out && $err == *SB5020* \
   && $err == *'`f`, called here with `a` tainted'* ]] \
   && grep -qE ':3: *1: error \[SB5020\]' <<< "$err"; chk 100.27

# 100.28 DD#127: a body cannot attest taint it received.
run_sash 'fn f {
  attest v in A [[ $v =~ ^.*$ ]]
  echo "ok"
}
argv a
f =a'
[[ $rc != 0 && -z $out && $err == *SB5088* \
   && $err == *'`f`, called here with `a` tainted'* ]] \
   && grep -qE ':6: *1: error \[SB5088\]' <<< "$err"; chk 100.28

# 100.29 attesting BEFORE the call also clears the slot
run_sash 'fn f { echo "ok${A[9]}" }
argv a
attest v in a [[ $v =~ ^.*$ ]]
f =a'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 100.29

# 100.30 outward: a callee that bulk-fills a slot taints the CALLER's
# collection — this leaked unattested argv to the caller's `echo`
run_sash 'fn fill { argv A }
array a=()
fill =a
echo "${a[1]}"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 100.30

# 100.31 the caller may attest what the callee filled
run_sash 'fn fill { argv A }
array a=()
fill =a
attest v in a [[ $v =~ ^.*$ ]]
echo "ok"'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 100.31

# 100.32 outward taint propagates through a forwarding call too
run_sash 'fn fill { argv A }
fn outer { fill =A }
array a=()
outer =a
echo "${a[1]}"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 100.32

# 100.33 a clean collection stays clean through a slot (no false positive)
run_sash 'fn f { echo "${A[0]}" }
array a=(x y)
f =a'
[[ $rc = 0 && $out = "x" && -z $err ]]; chk 100.33

# 100.34 `f =a` passes identity, not a value; the body cannot read it unescaped.
# In contrast, `f @a` IS a value read at the call — SB5020.
run_sash 'fn f { for v in @A; do echo "$v"; done }
argv a
f @a'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 100.34

# ── a bare name to a user function is a STRING (DG ruling, 2026-09-11) ──
# `bare-collection-name-operand-asymmetry` asked whether a bare collection
# name at a user-function call should be rejected, directing to `=a` or `"a"`.
# DG ruled no: `len`/`join`/`keys` are language surface whose operand slot is
# DECLARED to name a collection, while a user function takes values and
# creates no such expectation. Bare word to a user function = string, always.
# Marked "live with it" -- DG may reconsider with use -- so these pin the
# behaviour rather than bless it. If they start failing, read the DD#100
# clarification before "fixing" anything.

# 100.35 the asymmetry itself: one bare word, two meanings, no diagnostic.
# `show a` gets the STRING "a"; `len a` gets the COLLECTION.
run_sash 'array a=(x y z)
show() { echo "arg1=${args[0]}"; }
show a
echo "len=$(len a)"'
[[ $rc = 0 && $out = $'arg1=a\nlen=3' && -z $err ]]; chk 100.35

# 100.36 `=a` is the one spelling that binds the collection
run_sash 'array a=(x y z)
show() { echo "slot=${A[0]} n=$(len A)"; }
show =a'
[[ $rc = 0 && $out = "slot=x n=3" && -z $err ]]; chk 100.36

# 100.37 a bare word that names NOTHING is still just a string -- the rule
# does not depend on a variable existing, which is what makes it regular
run_sash 'show() { echo "arg1=${args[0]}"; }
show hello'
[[ $rc = 0 && $out = "arg1=hello" && -z $err ]]; chk 100.37

# ── A call is a call, wherever it appears (2026-09-28) ──────────────
# `split_call_args` had exactly ONE call site — the plain-statement path — so
# `=name` was treated as an ordinary scalar argument everywhere else: the slot
# was never bound and argc was off by one. The symptoms were not subtle: an
# `-> int` callee ICEd in codegen ("undefined arrlen: A") and a string callee
# compiled to a binary that SEGFAULTED.

# 100.38 a pipeline stage binds the slot (was: len A = 0, argc = 1)
run_sash 'fn f { : @args; echo "len=$(len A) n=$(len args)" }
array q=(x y z)
f =q | /bin/cat'
[[ $rc = 0 && $out = "len=3 n=0" && -z $err ]]; chk 100.38

# 100.39 a declaration capture binds the slot
run_sash 'fn f { : @args; echo "$(join A ,)" }
array q=(3 1 4)
str t=$(f =q)
echo "t=$t"'
[[ $rc = 0 && $out = "t=3,1,4" && -z $err ]]; chk 100.39

# 100.40 a re-assignment capture binds the slot (a distinct lowering path)
run_sash 'fn f { : @args; echo "$(join A ,)" }
array q=(3 1 4)
str t=x
t=$(f =q)
echo "t=$t"'
[[ $rc = 0 && $out = "t=3,1,4" && -z $err ]]; chk 100.40

# 100.41 an argument-position capture binds the slot (was: SEGFAULT)
run_sash 'fn f { : @args; echo "$(join A ,)" }
array q=(3 1 4)
echo "[$(f =q)]"'
[[ $rc = 0 && $out = "[3,1,4]" && -z $err ]]; chk 100.41

# 100.42 an `-> int` callee captures through a slot (was: ICE in codegen)
run_sash 'fn total -> int { int s=0; for v in @A; do s=$(( s + ${v:-0} )); done; return $s }
array q=(3 1 4)
int t=$(total =q)
echo "t=$t"'
[[ $rc = 0 && $out = "t=8" && -z $err ]]; chk 100.42

# 100.43 Part C (scalars first) is enforced inside `$( )` too — this was
# silently ACCEPTED, because the checks never descended into a capture
run_sash 'fn f { : @args; echo "$(join A ,)${args[0]:-}" }
array q=(3 1 4)
str t=$(f =q "x")
echo "t=$t"'
[[ $rc != 0 && $err == *SB5066* ]]; chk 100.43

# 100.44 …and `=scalar` inside `$( )` reports SB5066, not a misleading arity
# error about an argument the author never passed
run_sash 'fn f { : @args; echo "$(join A ,)" }
str sc=z
echo "s=$sc"
str t=$(f =sc)
echo "t=$t"'
[[ $rc != 0 && $err == *SB5066* ]]; chk 100.44

# 100.45 `=foo` in capture position counts as a READ of foo (DD#100 Part F) —
# the dead-variable gate must not fire on it
run_sash 'fn f { : @args; echo "$(join A ,)" }
array q=(3 1 4)
str t=$(f =q)
echo "t=$t"'
[[ $rc = 0 && $err != *SB5050* ]]; chk 100.45

# One inferred signature includes both structure and element type.
run_sash 'fn measure { echo $(( ${A[0]} )) }
array a=(nonsense)
measure =a'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'string cell `A'* ]]; chk slotty.1

run_sash 'fn measure { echo $(( ${A[0]} )) }
int array a=(7)
measure =a'
[[ $rc = 0 && $out = 7 && -z $err ]]; chk slotty.2

run_sash 'fn measure { echo $(( ${A[word]} )) }
int map m=([word]=9)
measure =m'
[[ $rc = 0 && $out = 9 && -z $err ]]; chk slotty.3

run_sash 'fn measure { echo $(( ${A[word]} )) }
map m=([word]=nonsense)
measure =m'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'string cell `A'* ]]; chk slotty.4

run_sash 'fn fill { A[0]=nonsense }
int array a=(7)
fill =a
echo ${a[0]}'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'cannot assign string value to int variable `A`'* ]]; chk slotty.5

run_sash 'fn fill { A[0]=$(( 8 )) }
int array a=(7)
fill =a
echo ${a[0]}'
[[ $rc = 0 && $out = 8 && -z $err ]]; chk slotty.6

run_sash 'fn fill { int n=8; A[0]=$n }
array a=(word)
fill =a
echo ${a[0]}'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'cannot assign int value to string variable `A`'* ]]; chk slotty.7

run_sash 'fn size { echo $(strlen ${A[0]}) }
int array a=(17)
size =a'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'`strlen` is a string operation'* ]]; chk slotty.8

run_sash 'fn size { echo $(strlen "${A[0]}") }
int array a=(17)
size =a'
[[ $rc = 0 && $out = 2 && -z $err ]]; chk slotty.9

run_sash 'fn show { echo ${A[0]:-word} }
int array a=(0)
show =a'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'default for int `A` must be an integer literal'* ]]; chk slotty.10

run_sash 'fn show { echo ${A[0]:-word} }
array a=()
show =a'
[[ $rc = 0 && $out = word && -z $err ]]; chk slotty.11

run_sash 'fn show { echo ${A[0]:0:1} }
int array a=(17)
show =a'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'substring of int variable `A`'* ]]; chk slotty.12

run_sash 'fn inner { echo ${A[word]} }
fn outer { inner =A }
array a=(value)
outer =a'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *"non-integer index on indexed array 'A'"* ]]; chk slotty.13

run_sash 'fn inner { echo ${A[word]} }
fn outer { inner =A }
map m=([word]=hello)
outer =m'
[[ $rc = 0 && $out = hello && -z $err ]]; chk slotty.14

run_sash 'fn inner { echo $(( ${A[0]} )) }
fn middle { inner =A }
fn outer { middle =A }
int array a=(7)
outer =a'
[[ $rc = 0 && $out = 7 && -z $err ]]; chk slotty.15

run_sash 'fn inner { echo $(( ${A[0]} )) }
fn outer { inner =A }
array a=(nonsense)
outer =a'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'string cell `A'* ]]; chk slotty.16

run_sash 'fn count { echo $(len A) }
array words=(hello)
int array numbers=(10 20)
count =words
count =numbers'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'one inferred signature'* \
   && $err == *'string array'* && $err == *'int array'* ]]; chk slotty.17

run_sash 'fn count { echo $(len A) }
map words=([word]=hello)
int map numbers=([word]=10)
count =words
count =numbers'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'one inferred signature'* \
   && $err == *'string map'* && $err == *'int map'* ]]; chk slotty.18

run_sash 'fn both { echo $(( ${A[0]} )) ${B[word]} }
int array a=(7)
map m=([word]=hello)
both =a =m'
[[ $rc = 0 && $out = "7 hello" && -z $err ]]; chk slotty.19

run_sash 'fn integer { echo $(( ${A[0]} )) }
fn string { echo $(strlen ${A[0]}) }
int array a=(7)
array b=(hello)
integer =a
string =b'
[[ $rc = 0 && $out = $'7\n5' && -z $err ]]; chk slotty.20

run_sash 'fn inner { echo $(len A) }
fn outer { inner =A }
int array numbers=(7)
array words=(hello)
inner =numbers
outer =words'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'function `inner`'* \
   && $err == *'one inferred signature'* ]]; chk slotty.21

# Anonymous bodies and sibling functions have distinct local bindings.
run_sash 'fn count { echo $(len A) }
str first=$(array items=(word); count =items)
str second=$(int array items=(7); count =items)
echo $first $second'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'one inferred signature'* ]]; chk slotty.22

run_sash 'fn count { echo $(len A) }
fn first { array items=(word); count =items }
fn second { int array items=(7); count =items }
first
second'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'one inferred signature'* ]]; chk slotty.23

run_sash 'fn measure { echo $(( ${A[0]} )) }
read "nonsense" generated
measure =generated'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'string cell `A'* ]]; chk slotty.24

run_sash 'fn size { echo $(strlen ${A[0]}) }
fn outer { str text=$(size =A); echo $text }
int array a=(7)
outer =a'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'`strlen` is a string operation'* ]]; chk slotty.25

run_sash 'fn size { echo $(strlen ${A[0]}) }
int array a=(7)
size =a | /bin/cat'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'`strlen` is a string operation'* ]]; chk slotty.26

# Collection slots are fixed, not variadic; scalar variadics retain DD#81.
run_sash 'fn count { echo $(len A) }
array a=(word)
array b=(other)
count =a
count =a =b
echo $(len b)'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'both 1 and 2 collection arguments'* ]]; chk slotty.27

run_sash 'fn nothing { : }
array a=(word)
nothing
nothing =a
echo $(len a)'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'both 0 and 1 collection arguments'* ]]; chk slotty.28

run_sash 'fn zero -> int { return 0 }
array a=(word)
int first=$(zero =a)
int second=$(( (zero) ))
echo $first $second $(len a)'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'one inferred signature'* \
   && $err == *'collection arguments'* ]]; chk slotty.29

run_sash 'fn fill { read "nonsense" A }
int array a=()
fill =a
echo $(len a)'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'cannot fill int collection `A`'* ]]; chk slotty.30

run_sash 'fn fill { keys B A }
map target=()
map source=([word]=value)
fill =target =source
echo $(len target)'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'cannot fill the map `A`'* ]]; chk slotty.31

run_sash 'fn fill { match abc "(a)" A }
int array a=()
fill =a
echo $(len a)'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'cannot fill int collection `A`'* ]]; chk slotty.32

run_sash 'int array a=()
read "nonsense" a
echo $(len a)'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'cannot fill int collection `a`'* ]]; chk slotty.33

run_sash 'fn setit { (( A[0] = 1 )) }
map m=([word]=value)
setit =m
echo $(len m)'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'bare'* && $err == *'map'* ]]; chk slotty.34

run_sash 'fn dormant { echo $(( ${A[0]} )); echo $(( A[0] )) }
echo unused'
[[ $rc = 0 && $out = unused && -z $err ]]; chk slotty.35

run_sash 'fn count { echo $(len A) }
array a=(word)
array b=(one two)
count =a
count =b'
[[ $rc = 0 && $out = $'1\n2' && -z $err ]]; chk slotty.36

run_sash 'fn string { echo ${A[0]} }
fn integer { echo $(( ${A[0]} )) }
str first=$(array items=(word); string =items)
str second=$(int array items=(7); integer =items)
echo $first $second'
[[ $rc = 0 && $out = "word 7" && -z $err ]]; chk slotty.37

run_sash 'str text=$(array items=(one); array items=(two); echo $(len items))
echo $text'
[[ $rc != 0 && -z $out && $err == *SB5049* && $err == *'already declared in this scope'* ]]; chk slotty.38

run_sash 'array items=(outer)
str text=$(array items=(inner); echo $(len items))
echo $text $(len items)'
[[ $rc != 0 && -z $out && $err == *SB5049* && $err == *'already declared in this scope'* ]]; chk slotty.39

run_sash 'fn counts { : @args; echo "$1:$(len args):$(len A)" }
array a=(word)
counts first =a
counts second extra =a'
[[ $rc = 0 && $out = $'first:1:1\nsecond:2:1' && -z $err ]]; chk slotty.40

# DD#86: internal identities belong to bindings, not their source spelling.
run_sash 'fn string { array items=(word); echo ${items[0]} }
fn integer { int array items=(7); echo ${items[0]} }
string
integer'
[[ $rc = 0 && $out = $'word\n7' && -z $err ]]; chk bindscope.1

run_sash 'fn integer { int array items=(7); echo ${items[0]} }
fn string { array items=(word); echo ${items[0]} }
integer
string'
[[ $rc = 0 && $out = $'7\nword' && -z $err ]]; chk bindscope.2

run_sash 'fn indexed { array items=(word); echo ${items[0]} }
fn keyed { int map items=([key]=7); echo ${items[key]} }
indexed
keyed'
[[ $rc = 0 && $out = $'word\n7' && -z $err ]]; chk bindscope.3

run_sash 'fn scalar { str items=word; echo $items }
fn indexed { int array items=(7); echo ${items[0]} }
scalar
indexed'
[[ $rc = 0 && $out = $'word\n7' && -z $err ]]; chk bindscope.4

run_sash 'fn inner { int array items=(7); echo ${items[0]} }
fn outer { array items=(word); echo ${items[0]}; inner; echo ${items[0]} }
outer'
[[ $rc = 0 && $out = $'word\n7\nword' && -z $err ]]; chk bindscope.5

run_sash 'fn inner { int item=7; echo $item }
fn outer { str item=word; echo $item; inner; echo $item }
outer'
[[ $rc = 0 && $out = $'word\n7\nword' && -z $err ]]; chk bindscope.6

run_sash 'fn outer {
str item=before
str result=$(item=after; echo $item)
echo $item $result
}
outer'
[[ $rc = 0 && $out = "after after" && -z $err ]]; chk bindscope.7

run_sash 'fn outer {
str first=$(str middle=$(array items=(word); echo ${items[0]}); echo $middle)
str second=$(int array items=(7); echo ${items[0]})
echo $first $second
}
outer'
[[ $rc = 0 && $out = "word 7" && -z $err ]]; chk bindscope.8

run_sash 'str state=before
fn update { str result=$(state=after; echo changed); echo $result }
update
echo $state'
[[ $rc = 0 && $out = $'changed\nafter' && -z $err ]]; chk bindscope.9

run_sash 'fn word { str item=word; echo $(toupper $item) }
fn number { int item=7; echo $(( item + 1 )) }
number
word'
[[ $rc = 0 && $out = $'8\nWORD' && -z $err ]]; chk bindscope.10

run_sash 'fn string { array items=(word); echo items =items "${items[0]}" }
string'
[[ $rc = 0 && $out = "items =items word" && -z $err ]]; chk bindscope.11

run_sash 'str __string_items=keep
str __string_items_1=also
fn string { array items=(word); echo ${items[0]} }
string
echo $__string_items $__string_items_1'
[[ $rc = 0 && $out = $'word\nkeep also' && -z $err ]]; chk bindscope.12

run_sash 'fn show { echo "$1:$(len A)" }
fn outer { array items=(word); show items =items }
outer'
[[ $rc = 0 && $out = "items:1" && -z $err ]]; chk bindscope.13

run_sash 'fn first { map items=([key]=word); keys items names; echo $(join names ,) }
fn second { int array items=(7); echo ${items[0]} }
first
second'
[[ $rc = 0 && $out = $'key\n7' && -z $err ]]; chk bindscope.14

run_sash 'fn first { array items=(one two); unset items[0]; echo "$(len items):${items[1]}" }
fn second { int array items=(7); echo ${items[0]} }
first
second'
[[ $rc = 0 && $out = $'1:two\n7' && -z $err ]]; chk bindscope.15

run_sash 'fn first { str item=word; echo $(toupper item) $(strlen item) $item }
first'
[[ $rc = 0 && $out = "ITEM 4 word" && -z $err ]]; chk bindscope.16

run_sash 'fn first { for item in one two { echo $item } }
fn second { int item=7; if (( item > 0 )) { echo $(( item + 1 )) } }
first
second'
[[ $rc = 0 && $out = $'one\ntwo\n8' && -z $err ]]; chk bindscope.17

run_sash 'fn first { int array items=(7); int index=0; echo $(( items[index] + ${items[$index]} )) }
fn second { array items=(word); echo ${items[0]} }
first
second'
[[ $rc = 0 && $out = $'14\nword' && -z $err ]]; chk bindscope.18

run_sash 'fn first { str item=key; : $item; map entries=([item]=word); echo ${entries[item]}; unset entries[item]; echo $(len entries) }
first'
[[ $rc = 0 && $out = $'word\n0' && -z $err ]]; chk bindscope.19

run_sash 'fn first { str path=scope-output; echo word >${path:?missing}; str text=$(<${path:?missing}); attest text [[ -n $text ]]; echo $text }
fn second { int path=7; echo $path }
first
second'
[[ $rc = 0 && $out = $'word\n7' && -z $err ]]; chk bindscope.20

run_sash 'fn first { int item=7; echo $(strlen $item) }
first'
[[ $rc != 0 && $err == *SB4030* && $err == *'`strlen` is a string operation'* \
   && $err == *'`item`'* && $err != *'__first_item'* ]]; chk bindscope.21

run_sash 'fn first { str item=word; attest item [[ $item == no ]]; echo $item }
first'
[[ $rc = 1 && $rerr == *"attest: item = 'word' failed the gate"* \
   && $rerr != *'__first_item'* && -z $err ]]; chk bindscope.22

run_sash 'fn first { array items=(word); attest element in items [[ $element == no ]]; echo $(len items) }
first'
[[ $rc = 1 && $rerr == *"attest: items"* && $rerr == *"failed the gate"* \
   && $rerr != *'__first_items'* && -z $err ]]; chk bindscope.23

printf '%s\n' 'fn first { array items=(word); echo ${items[0]} }; first' > bindings.sash
"${SASH:-sash}" --emit-ir bindings.sash > bindings.ir 2>bindings.err; flag_rc=$?
"${SASH:-sash}" bindings.sash -o bindings-bin >/dev/null 2>>bindings.err; native_rc=$?
embedded=$("${SASH:-sash}" --extract bindings-bin ir 2>>bindings.err); extract_rc=$?
source=$("${SASH:-sash}" --extract bindings-bin source 2>>bindings.err); source_rc=$?
[[ $flag_rc = 0 && $native_rc = 0 && $extract_rc = 0 && $source_rc = 0 \
   && $(<bindings.ir) = "$embedded" && $embedded == *'"items"'* \
   && $embedded != *'__first_items'* && $source = "$(<bindings.sash)" ]]; chk bindscope.24

# A subscript does not turn an unknown name or a scalar into a collection.
run_sash 'unset missing[0]; echo done'
[[ $rc != 0 && $err == *SB4030* && $err == *"'missing'"* ]]; chk bindscope.25

run_sash 'str item=word; unset item[0]; echo $item'
[[ $rc != 0 && $err == *SB4030* && $err == *"'item'"* ]]; chk bindscope.26

run_sash 'int item=7; unset item[0]; echo $item'
[[ $rc != 0 && $err == *SB4030* && $err == *"'item'"* ]]; chk bindscope.27

run_sash 'fn first { unset missing[0]; echo done }; first'
[[ $rc != 0 && $err == *SB4030* && $err == *"'missing'"* ]]; chk bindscope.28

run_sash 'echo $(unset missing[0]; echo done)'
[[ $rc != 0 && $err == *SB4030* && $err == *"'missing'"* ]]; chk bindscope.29

run_sash 'fn remove { unset A[0]; echo $(join A ,) }
array items=(red green)
remove =items
echo $(join items ,)'
[[ $rc = 0 && $out = $'green\ngreen' && -z $err ]]; chk bindscope.30

run_sash 'fn remove { unset A[0]; echo $(join A ,) }
int array items=(7 9)
remove =items
echo $(join items ,)'
[[ $rc = 0 && $out = $'9\n9' && -z $err ]]; chk bindscope.31

run_sash 'fn remove { unset A[first]; echo ${A[second]} }
map items=([first]=red [second]=green)
remove =items
echo ${items[second]}'
[[ $rc = 0 && $out = $'green\ngreen' && -z $err ]]; chk bindscope.32

run_sash 'fn remove { unset A[first]; echo ${A[second]} }
int map items=([first]=7 [second]=9)
remove =items
echo ${items[second]}'
[[ $rc = 0 && $out = $'9\n9' && -z $err ]]; chk bindscope.33

run_sash 'fn first { array words=(red green); int array numbers=(7 9)
unset words[0] numbers[1]
echo $(join words ,); echo $(join numbers ,) }
first'
[[ $rc = 0 && $out = $'green\n7' && -z $err ]]; chk bindscope.34

run_sash 'fn first { array items=(word); unset items[5]; echo $(join items ,) }
fn second { map items=([key]=value); unset items[missing]; echo ${items[key]} }
first; second'
[[ $rc = 0 && $out = $'word\nvalue' && -z $err ]]; chk bindscope.35

run_sash 'fn empty { unset A; echo $(len A) }
array items=(red green)
empty =items
echo $(len items)'
[[ $rc = 0 && $out = $'0\n0' && -z $err ]]; chk bindscope.36

run_sash 'fn remove { unset A[0] }
fn forward { remove =A; echo $(join A ,) }
array items=(red green)
forward =items
echo $(join items ,)'
[[ $rc = 0 && $out = $'green\ngreen' && -z $err ]]; chk bindscope.37

SASH_CFLAGS="${SASH_CFLAGS:-} --no-taint-cst" \
run_sash '/bin/echo $(str token=$(getenv HOME); echo $token)'
[[ $rc != 0 && -z $out && $err == *'tainted variable token_1 used'* \
   && $err == *'SSA taint backstop'* && $err != *'__top_capture'* ]]; chk bindscope.38

# A live SSA version can equal another binding's qualified base name.
# Version 1 belongs to the unexecuted template; the real calls define version 2.
SASH_CFLAGS="${SASH_CFLAGS:-} --no-taint-cst" \
run_sash 'fn a { str b_t=$(getenv HOME); echo $b_t }
fn a_b { str t_2=$(getenv HOME); echo $t_2 }
a
a_b'
[[ $rc != 0 && -z $out && $err == *'tainted variable b_t_2 used'* \
   && $err == *'tainted variable t_2_2 used'* && $err == *'SSA taint backstop'* \
   && $err != *'__a_b_t'* ]]; chk bindscope.40

# DD#100: strict non-aliasing, including read-only and taint-disabled calls.
SASH_CFLAGS="${SASH_CFLAGS:-} --no-taint" \
run_sash 'fn pair { echo "$(len A) $(len B)" }
array a=(x)
pair =a =a'
[[ $rc != 0 && $err == *SB5099* && $err == *'distinct identities'* ]]; chk noalias.1

run_sash 'fn pair { echo "$(len A) $(len B)" }
fn forward { pair =A =A }
array a=(x)
forward =a'
[[ $rc != 0 && $err == *SB5099* && $err == *'collection `A`'* \
   && $err == *'distinct identities'* ]]; chk noalias.2

run_sash 'array a=(x)
fn f {
echo "${A[0]} ${a[0]}"
}
f =a'
[[ $rc != 0 ]] && grep -qE ':5: *1: error \[SB5099\]' <<< "$err" \
   && [[ $err == *'global `a` in `f` at 3:1'* ]]; chk noalias.3

run_sash 'array a=(old)
fn f {
a[0]=changed
echo "${A[0]}"
}
f =a'
[[ $rc != 0 && $err == *SB5099* \
   && $err == *'global `a` in `f` at 3:1'* ]]; chk noalias.4

run_sash 'array a=(x)
fn f { unset a; echo "$(len A)" }
f =a'
[[ $rc != 0 && $err == *SB5099* && $err == *'global `a` in `f`'* ]]; chk noalias.5

SASH_CFLAGS="${SASH_CFLAGS:-} --no-taint" \
run_sash 'array a=(x)
fn f { attest v in a [[ -n $v ]]; echo "$(len A)" }
f =a'
[[ $rc != 0 && $err == *SB5099* && $err == *'global `a` in `f`'* ]]; chk noalias.6

# A global passed to a helper is an identity use, even without a value read.
run_sash 'array a=(x)
fn inspect { echo "$(len A)" }
fn f { inspect =a; echo "$(len A)" }
f =a'
[[ $rc != 0 && $err == *SB5099* && $err == *'global `a` in `f`'* ]]; chk noalias.7

run_sash 'array a=(x)
fn leaf { echo "${a[0]}" }
fn middle { leaf }
fn f { middle; echo "$(len A)" }
f =a'
[[ $rc != 0 && $err == *SB5099* && $err == *'`f` is passed collection `a`'* \
   && $err == *'global `a` in `leaf`'* ]]; chk noalias.8

run_sash 'map m=([k]=value)
fn leaf { echo "${m[k]}" }
fn inner { leaf; echo "$(len A)" }
fn outer { inner =A }
outer =m'
[[ $rc != 0 && $err == *SB5099* && $err == *'`outer` is passed collection `m`'* \
   && $err == *'global `m` in `leaf`'* ]]; chk noalias.9

run_sash 'array a=(x)
fn f {
str size=$(echo "$(len a)")
echo "$size $(len A)"
}
f =a'
[[ $rc != 0 && $err == *SB5099* && $err == *'global `a` in `f`'* ]]; chk noalias.10

run_sash 'int array nums=(7)
fn get -> int { return $(( nums[0] )) }
fn f { echo "$(( get )) $(len A)" }
f =nums'
[[ $rc != 0 && $err == *SB5099* && $err == *'global `nums` in `get`'* ]]; chk noalias.11

run_sash 'array a=(x)
fn f {
str choose=no
if [[ $choose == yes ]] { echo "${a[0]}" }
echo "$(len A)"
}
f =a'
[[ $rc != 0 && $err == *SB5099* && $err == *'global `a` in `f`'* ]]; chk noalias.12

run_sash 'array a=(x)
fn f { echo "${a[0]} $(len A)" }
END {
f =a
}'
[[ $rc != 0 && $err == *SB5099* && $err == *'global `a` in `f`'* ]]; chk noalias.17

run_sash 'fn key { : "$(len A)" "$(len B)"; echo k }
map m=([k]=yes)
str value="${m[$(key =m =m)]}"
echo "$value"'
[[ $rc != 0 && $err == *SB5099* && $err == *'distinct identities'* ]]; chk noalias.14

# Unrelated globals and ordinary literal arguments remain legal.
run_sash 'array a=(one)
array b=(two)
fn helper { echo "${a[0]}" }
fn f { helper; A[0]=changed }
f =b
echo "${b[0]}"'
[[ $rc = 0 && $out = $'one\nchanged' && -z $err ]]; chk noalias.15

run_sash 'array a=(x)
fn f { /bin/echo =a; echo "$(len A)" }
f =a'
[[ $rc = 0 && $out = $'=a\n1' && -z $err ]]; chk noalias.16

# DD#127: slots use the same independent summaries as globals.
run_sash 'fn clean {
argv 1 A
attest v in A [[ -n $v ]] || exit 3
}
argv 1 a
clean =a
echo "${a[0]}"' safe
[[ $rc = 0 && $out = safe && -z $err && -z $rerr ]]; chk sflow.1

run_sash 'fn clean {
argv 1 A
attest v in A [[ -n $v ]]
}
array a=()
array b=()
clean =a
clean =b
echo "${a[0]} ${b[0]}"' safe
[[ $rc = 0 && $out = 'safe safe' && -z $err && -z $rerr ]]; chk sflow.2

# Forwarding permutes the input and output identities, not just their names.
run_sash 'fn clean {
argv 1 B
attest v in B [[ -n $v ]]
echo "${A[0]}"
}
fn outer { clean =B =A }
argv 1 a
array b=(keep)
outer =a =b
echo "${a[0]}"' safe
[[ $rc = 0 && $out = $'keep\nsafe' && -z $err && -z $rerr ]]; chk sflow.3

run_sash 'fn show { echo "${A[0]}" }
argv 1 a
show =a
argv 1 a
attest v in a [[ -n $v ]]
echo "${a[0]}"' dirty
[[ $rc != 0 && $err == *'`show`, called here with `a` tainted'* ]] \
   && grep -qE ':3: *1: error \[SB5020\]' <<< "$err"; chk sflow.4

run_sash 'fn show { echo "${A[0]}" }
array a=(clean)
show =a
argv 1 a
echo "${a[0]:!}"' dirty
# The validator accepts the earlier clean call; the SSA backstop still reads
# the future array version (bug-ssa-taint-array-read-uses-future-version).
[[ $rc != 0 && $err != *SB5020* \
   && $err == *'tainted array a[...] used'* \
   && $err == *'SSA taint backstop'* ]]; chk sflow.5

run_sash 'fn check { attest v in A [[ -n $v ]]; echo ok }
fn outer { check =A }
argv 1 a
outer =a' safe
[[ $rc != 0 && $err == *SB5088* \
   && $err == *'`outer`, called here with `a` tainted'* \
   && $err == *'`check`, called here with `A` tainted'* ]]; chk sflow.6

# A read before a kill remains an obligation, but only for the dirty call.
run_sash 'fn use {
echo "${A[0]}"
argv 1 A
attest v in A [[ -n $v ]]
}
array clean=(safe)
argv 1 dirty
use =clean
use =dirty' value
[[ $rc != 0 && $err == *'`use`, called here with `dirty` tainted'* \
   && $err != *'called here with `clean` tainted'* \
   && $(grep -cE 'error \[SB5020\]' <<< "$err") = 1 ]] \
   && grep -qE ':9: *1: error \[SB5020\]' <<< "$err"; chk sflow.7

run_sash 'fn fill { argv 1 A }
array a=()
array b=()
fill =a
fill =b
echo "$(len a) $(len b)"' x
[[ $rc != 0 && $err == *SB5067* && $err == *'collection slot `A`'* \
   && $err == *'`fill` is shared'* \
   && $(grep -cE 'error \[SB5067\]' <<< "$err") = 1 ]]; chk sflow.8

run_sash 'fn fill { argv 1 A }
argv 1 a
argv 1 b
fill =a
fill =b
echo "$(len a) $(len b)"' x
[[ $rc != 0 && $err == *SB5067* && $err == *'collection slot `A`'* \
   && $err == *'`fill` is shared'* ]]; chk sflow.9

# Received taint survives a shared metadata-only call without becoming its own.
run_sash 'fn size { : "$(len A)" }
argv 1 a
size =a
size =a
echo "${a[0]}"' dirty
[[ $rc != 0 && $err == *SB5020* && $err != *SB5067* ]] \
   && grep -qE ':5: *6: error \[SB5020\]' <<< "$err"; chk sflow.10

run_sash 'fn size { : "$(len A)" }
argv 1 a
size =a
attest v in a [[ -n $v ]]
echo "${a[0]}"' dirty
[[ $rc != 0 && $err == *SB5088* ]] \
   && grep -qE ':4: *1: error \[SB5088\]' <<< "$err"; chk sflow.11

run_sash 'fn count -> int { : "${A[0]}"; return 7 }
argv 1 a
int n=$(count =a)
echo "$n"' dirty
[[ $rc != 0 && $err == *SB5020* \
   && $err == *'`count`, called here with `a` tainted'* ]]; chk sflow.12

run_sash 'fn clean {
argv 1 A
attest v in A [[ -n $v ]]
echo ok
}
array a=()
str result=$(clean =a)
echo "$result ${a[0]}"' safe
[[ $rc = 0 && $out = 'ok safe' && -z $err && -z $rerr ]]; chk sflow.13

run_sash 'fn show { echo "${A[0]}" }
argv 1 a
show =a &
- wait' dirty
[[ $rc != 0 && $err == *SB5020* \
   && $err == *'`show`, called here with `a` tainted'* ]] \
   && grep -qE ':3: *1: error \[SB5020\]' <<< "$err"; chk sflow.14

run_sash 'fn clean {
argv 1 E
attest v in E [[ -n $v ]]
echo "$(len A) $(len B) $(len C) $(len D)"
}
array a=()
array b=()
array c=()
array d=()
argv 1 e
clean =a =b =c =d =e
echo "${e[0]}"' safe
[[ $rc = 0 && $out = $'0 0 0 0\nsafe' && -z $err && -z $rerr ]]; chk sflow.15

run_sash 'fn maybe {
if [[ $1 == yes ]] { argv 1 A; attest v in A [[ -n $v ]] }
}
argv 1 a
maybe no =a
echo "${a[0]}"' dirty
[[ $rc != 0 && $err == *SB5020* ]] \
   && grep -qE ':6: *6: error \[SB5020\]' <<< "$err"; chk sflow.16

run_sash 'fn clean { argv 1 A; attest v in A [[ -n $v ]] }
fn outer {
array buf=()
clean =buf
echo "${buf[0]}"
}
outer' safe
[[ $rc = 0 && $out = safe && -z $err && -z $rerr ]]; chk sflow.17

# A tainted array does not taint the clean map in another, forwarded slot.
run_sash 'fn inspect {
if attest k v in B [[ -n $k && -n $v ]] { echo "${B[k]} $(len A)" } else { exit 3 }
}
fn outer { map m=([k]=safe); inspect =A =m }
argv 1 a
outer =a' value
[[ $rc = 0 && $out = 'safe 1' && -z $err && -z $rerr ]]; chk sflow.18

set -e
tally
