#!/bin/bash
# spec/subscript.sh — DD#80 braced subscript key/index discipline.
#
# `${a[K]}` / `${m[K]}` take a DIRECT, unambiguous key. `K` is one of: an
# integer literal, a `$var`, an expansion (`${…}`/`$(…)`/`$(( … ))`), a quoted
# string, or a clean bareword (identifier — maps only). Implicit / ambiguous
# bare forms reject at parse (SB4041); a string key on an int-indexed array
# rejects at type-check (SB4030, the string→int discipline). Computation uses an EXPLICIT nested context —
# bare arithmetic lives in `(( ))` (DD#79), never implicitly inside `${ }`.
set -eu
. common.sh
setup subscript
plan=30
echo "1..$plan" >&3

set +e

# ── A. implicit / ambiguous bare index → SB4041 (parse, mode-independent) ──

# 80.1 a bare nested subscript (brackets) — the canonical footgun
run_sash 'int array a=(2 0 9); echo ${a[a[0]]}'
[[ $rc != 0 && -z $out && $err == *SB4041* ]]; chk 80.1

# 80.2 a bare nested subscript on a map rejects too (quote it, or use $var)
run_sash 'map m=([k]=v); echo ${m[a[0]]}'
[[ $rc != 0 && -z $out && $err == *SB4041* ]]; chk 80.2

# 80.3 an arithmetic operator in a bare index (no implicit arithmetic)
run_sash 'int array a=(2 0 9); echo ${a[i+1]}'
[[ $rc != 0 && -z $out && $err == *SB4041* ]]; chk 80.3

# 80.4 a mixed `$var` + operator is not a pure `$var`
run_sash 'int array a=(2 0 9); i=0; echo ${a[$i+1]}'
[[ $rc != 0 && -z $out && $err == *SB4041* ]]; chk 80.4

# 80.5 a `$var` carrying its own subscript is ambiguous → precompute it
run_sash 'int array a=(2 0 9); echo ${a[$b[0]]}'
[[ $rc != 0 && -z $out && $err == *SB4041* ]]; chk 80.5

# ── B. string key on an int-indexed array → SB4030 (type-check, string→int) ──

# 80.6 a clean bareword is a string key — invalid for an int array
run_sash 'int array a=(2 0 9); echo ${a[foo]}'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 80.6

# 80.7 bare `i` is the literal key "i", NOT the variable i → SB4030 (use $i)
run_sash 'int array a=(2 0 9); echo ${a[i]}'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 80.7

# 80.8 a quoted numeric is a string literal, not an int literal → use ${a[5]}
run_sash 'int array a=(2 0 9); echo ${a["5"]}'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 80.8

# ── C. explicit array indices accepted ──

# 80.9 integer literal
run_sash 'int array a=(2 0 9); echo ${a[2]}'
[[ $rc = 0 && $out = "9" && -z $err ]]; chk 80.9
# 80.10 a `$var` index
run_sash 'int array a=(2 0 9); int i=2; echo ${a[$i]}'
[[ $rc = 0 && $out = "9" && -z $err ]]; chk 80.10
# 80.11 an EXPLICIT arithmetic index (the sanctioned computed form)
run_sash 'int array a=(2 0 9); int i=1; echo ${a[$(( i + 1 ))]}'
[[ $rc = 0 && $out = "9" && -z $err ]]; chk 80.11
# 80.12 an explicit nested expansion index: a[0]=2 → a[${a[0]}] = a[2] = 9
run_sash 'int array a=(2 0 9); echo ${a[${a[0]}]}'
[[ $rc = 0 && $out = "9" && -z $err ]]; chk 80.12

# ── D. map keys accepted (bareword / $var / quoted / integer / expansion) ──

# 80.13 a clean bareword literal key
run_sash 'map m=([foo]=hi [bar]=bye); echo ${m[foo]}'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 80.13

# 80.14 a `$var` dynamic key
run_sash 'map m=([foo]=hi); k=foo; echo ${m[$k]}'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 80.14

# 80.15 a quoted string parses as a literal key even with brackets (absent key →
# empty, but crucially NOT SB4041 — the quotes disambiguate)
run_sash 'map m=([foo]=hi); echo ${m["a[0]"]}'
[[ $rc = 0 && -z $out && -z $err ]]; chk 80.15

# 80.16 a numeric-looking map key must be quoted for the READ — DD#33 rejects a
# bare integer key `${m[5]}` (it looks like an array index); `${m["5"]}` reads it
run_sash 'map m=([5]=five); echo ${m["5"]}'
[[ $rc = 0 && $out = "five" && -z $err ]]; chk 80.16

# 80.17 a computed (expansion) key from another collection read: src[k]=2 →
# m[${src[k]}] = m["2"]
run_sash 'map src=([k]=2); map m=([2]=two); echo ${m[${src[k]}]}'
[[ $rc = 0 && $out = "two" && -z $err ]]; chk 80.17

# ── E. nesting unrolls ──

# 80.18 a map key that is itself a map read: inner[k]=foo → outer[foo]=win
run_sash 'map inner=([k]=foo); map outer=([foo]=win); echo ${outer[${inner[k]}]}'
[[ $rc = 0 && $out = "win" && -z $err ]]; chk 80.18
# 80.19 a double-quoted key interpolates (string-building, not arithmetic)
run_sash 'map m=([pX]=hit); s=X; echo ${m["p$s"]}'
[[ $rc = 0 && $out = "hit" && -z $err ]]; chk 80.19

# ── F. the positional splat `$@` / `$*` isn't subscriptable (was a codegen ICE
# `undefined array: @`; now a clean reject). Mode-independent (validate). ──

# 80.20 `$@[i]` rejects → use `$1`..`$N` / `${args[i]}`
run_sash 'fn f { echo $@[0]; }; f a b c'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk 80.20

# 80.21 `$*[i]` likewise
run_sash 'fn f { echo $*[0]; }; f a b c'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk 80.21

# ── G. reading a key after `unset m[K]`. The open-addressed table marks a
# removed slot with a DELETED sentinel distinct from EMPTY, and the probe
# returns the first DELETED slot it crossed when the key is absent. Both
# `sash_table_get` and `sash_table_set` tested only for EMPTY, so the -2
# sentinel was used as a live entry index: get dereferenced
# `entries.add(-2 as usize)` (garbage ptr/len → EFAULT on the next write) and
# set *wrote* through it (heap corruption → SIGSEGV under churn). Runtime, so
# both backends were affected identically. ──

# 80.22 a deleted key reads as the empty string, like bash
run_sash 'map m=([a]=1 [b]=2)
unset m[a]
echo "[${m[a]}]"'
[[ $rc = 0 && $out = '[]' && -z $err ]]; chk 80.22

# 80.23 re-setting a deleted key reclaims the slot: new value, live count
# back to 2. Before the fix `set` took the update branch on the sentinel and
# live_count stayed 1.
run_sash 'map m=([a]=1 [b]=2)
unset m[a]
m[a]=9
echo "[${m[a]}] n=$(( (len m) ))"'
[[ $rc = 0 && $out = '[9] n=2' && -z $err ]]; chk 80.23

# 80.24 deleting every key then adding a fresh one — the new key is readable
# and an absent key still reads empty (the probe crosses two DELETED slots)
run_sash 'map m=([a]=1 [b]=2)
unset m[a]
unset m[b]
m[c]=3
echo "n=$(( (len m) )) c=${m[c]} a=[${m[a]}]"'
[[ $rc = 0 && $out = 'n=1 c=3 a=[]' && -z $err ]]; chk 80.24

# 80.25 set/unset the same key in a loop. Repeated slot reclamation; before
# the fix this returned the last-written value from the dead entry.
run_sash 'map m=()
int i=0
while (( i < 40 )); do
  m[k]="$i"
  unset m[k]
  (( i = i + 1 ))
done
echo "n=$(( (len m) )) k=[${m[k]}]"'
[[ $rc = 0 && $out = 'n=0 k=[]' && -z $err ]]; chk 80.25

# 80.26 churn with a live neighbour, enough iterations to force table_grow
# compaction. This is the case that segfaulted on the wild write.
run_sash 'map m=()
int i=0
while (( i < 30 )); do
  m[x]="$i"
  m[y]="$i"
  unset m[x]
  (( i = i + 1 ))
done
echo "n=$(( (len m) )) y=${m[y]} x=[${m[x]}]"'
[[ $rc = 0 && $out = 'n=1 y=29 x=[]' && -z $err ]]; chk 80.26

# 80.27 `keys` and `len` agree with the reads after a delete (the c backend
# emitted no `__table_delete` at all, so `unset m[K]` was a silent no-op there
# and this reported n=3 with all three keys)
run_sash 'map m=([a]=1 [b]=2 [c]=3)
unset m[b]
echo "n=$(( (len m) ))"
for k in $(keys m); do echo "k=$k"; done'
[[ $rc = 0 && $out = $'n=2\nk=a\nk=c' && -z $err ]]; chk 80.27

# 80.28 multiple subscripted targets in one `unset`. The args were joined
# into a single string, so `unset a[0] a[1]` became one target named `a` with
# the key `0]a[1` — every target after the first was silently discarded.
run_sash 'array a=(x y z)
unset a[0] a[1]
echo "n=$(( (len a) )) 2=${a[2]}"'
[[ $rc = 0 && $out = 'n=1 2=z' && -z $err ]]; chk 80.28

# 80.29 the same for a map, where the joined key matched nothing at all and
# the whole statement was a no-op
run_sash 'map m=([a]=1 [b]=2 [c]=3)
unset m[a] m[b]
echo "n=$(( (len m) )) c=${m[c]}"'
[[ $rc = 0 && $out = 'n=1 c=3' && -z $err ]]; chk 80.29

# 80.30 targets of different collection kinds in one `unset` — each is keyed
# by its own encoding (i64 bytes for the array, the string for the map)
run_sash 'array a=(x y z)
map m=([k]=v [j]=w)
unset a[0] m[k]
echo "na=$(( (len a) )) nm=$(( (len m) ))"'
[[ $rc = 0 && $out = 'na=2 nm=1' && -z $err ]]; chk 80.30

set -e
tally
