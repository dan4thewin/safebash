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
plan=37
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

# 100.9 the same collection in two slots is one object, not two copies
run_sash 'fn two { A[0] = z; echo "${B[0]}" }
array a=(k)
two =a =a'
[[ $rc = 0 && $out = "z" && -z $err ]]; chk 100.9

# 100.10 a bulk fill through a slot is an out-parameter. Unresolved, `read A`
# filled a function-local array named `A` and the caller's collection was left
# untouched — silently, which is the whole point of Part E.
run_sash 'fn split_into { read $1 A }
sep=$'"'"'\0'"'"'
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

# 100.27 inward: passing a tainted collection taints the slot
run_sash 'fn f { echo "${A[0]}" }
argv a
f =a'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 100.27

# 100.28 Part E: the attest may be deferred INTO the callee
run_sash 'fn f {
  attest v in A [[ $v =~ ^.*$ ]]
  echo "ok"
}
argv a
f =a'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 100.28

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

# 100.34 `f =a` is not itself a read: passing a tainted collection is fine so
# long as the callee attests (contrast `f @a`, which IS a read — SB5020)
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

set -e
tally
