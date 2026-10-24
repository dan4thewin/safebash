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
plan=133
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

# ── F. dynamic `$var` indices the identifier fast path cannot take ──

# 80.31 a positional as the index. The braced-index reader shortcuts an index
# spelled like an identifier and otherwise falls to the general expansion
# path; `$1` starts with a digit, so it takes the long way. A positional is a
# string with no declared type, so it needs the same conversion any other
# string index needs — and `:?` is the honest one, since an argument that is
# not a number is a caller error rather than something to default away.
run_sash 'fn f { array arr=(p q r); echo ${arr[$(( ${1:?not a number} ))]} }
f 2'
[[ $rc = 0 && $out = r && -z $err ]]; chk 80.31

# 80.32 `$#` as the index — the same fall-through, a first byte that is
# neither a letter nor `_`.
run_sash 'fn f { : @args; array arr=(p q r); echo ${arr[$#]} }
f a b'
[[ $rc = 0 && $out = r && -z $err ]]; chk 80.32

# ── G. the non-brace `$a[$i]` read (decision B, journal 115) ──
#
# `$a[$i]` deliberately reads as `${a[$i]}` rather than bash's `${a[0]}` then
# literal `[$i]`. The lexer desugars the brace-less spelling to the braced one
# (DG 2026-10-03, "all paths must agree"), so there is one index grammar and
# the only way to decline is for the brackets not to balance.

# 80.33 the subscript must close. `$arr[foo` never does, so no subscript is
# recognised and `$arr` is left as a plain scalar read → SB5064. Bash would
# print `p[foo`. This is the ONE decline that remains.
run_sash 'array arr=(p q r)
echo $arr[foo'
[[ $rc != 0 && -z $out && $err == *SB5064* ]]; chk 80.33

# 80.34 an index that is not identifier-spelled is no longer a decline: the
# old token-level gather required `[`,`$ident`,`]` exactly, so `$arr[$1]` fell
# out as SB5064 while `${arr[$1]}` reported the real problem. Both spellings
# now reach the same rule (a positional is a string — see 80.31 for the fix).
run_sash 'fn f { array arr=(p q r); echo $arr[$1] }
f 2'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 80.34
# 80.34b the braced twin, same diagnostic — the pair is the agreement itself.
run_sash 'fn f { array arr=(p q r); echo ${arr[$1]} }
f 2'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 80.34b

# ─── H. `name[` as a bare word: the early-outs of the dynamic-subscript
#        sniffer. `looks_like_dynamic_subscript_assign` only commits to a
#        `name[…]=` parse once the *shape* is confirmed; these are the
#        inputs where it backs out and the word stays an ordinary command
#        name.

# 80.35 `a[ x ]=5` IS a subscript assignment now. It did not used to be: the
#       sniffer required the index to be a single `$`-token, so a bareword
#       index made `a[` an ordinary command name and the error was SB5011
#       ("command not found"). The LHS index is now parsed by the same
#       `parse_subscript_index` the read side uses, which trims, so the
#       whitespace is immaterial and the real fault is reported instead:
#       `x` is a string key on an array.
run_sash 'array a=(1 2 3)
a[ x ]=5'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 80.35

# 80.36 `a[$i` with nothing after it: the token where `]` would go is the end
#       of input, so the sniffer backs out rather than reading past the end.
run_sash 'array a=(1 2 3)
int i=0
a[$i'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk 80.36

# 80.37 …and the same with a statement separator where `]` belongs.
run_sash 'array a=(1 2 3)
int i=0
a[$i; echo hi'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk 80.37

# ── an assignment-LHS subscript index accepts what the read side accepts ──
# The LHS used to be a strictly narrower surface: the lexer splits `m[$k]=`
# into `m[`, `$k`, `]`, `=`, and the parser accepted only that single
# `$var`/`${var}` token, so every richer index was SB4014 — including
# `$(( … ))`, the conversion DD#80 itself names. Both surfaces now route
# through `parse_subscript_index`, so neither can drift from the other.

# 80.38 a disposition in the index is honoured, not rejected: `k` is set, so
#       the default is not taken and the key is `x`.
run_sash 'map m
str k="x"
m[${k:-z}]=1
echo "$(len m) ${m[x]}"'
[[ $rc = 0 && $out = "1 1" && -z $err ]]; chk 80.38

# 80.39 a retired construct in the index reports what it *is* (SB4038, the
#       `${#…}` retirement) rather than the blanket SB4014 the LHS used to
#       give — the same diagnostic the read side gives for the same text.
run_sash 'map m
str k="x"
m[${#k}]=1
echo $(len m)'
[[ $rc != 0 && $err == *SB4038* ]]; chk 80.39

# 80.40 the plain forms both still work and index the same cell
run_sash 'map m
str k="x"
m[$k]=1
m[${k}]=2
echo "$(len m) ${m[x]}"'
[[ $rc = 0 && $out = "1 2" && -z $err ]]; chk 80.40

# ── G. a `str`-declared scalar as an array index (DD#80 type-aware + DD#77) ──
# `var_type` ends in `unwrap_or(String)`, so "declared a string" and "never
# recorded" are one answer. Only the *declared* case is rejected; see 80.47.

# 80.41 read: a str scalar does not index an int array
run_sash 'int array a=(10 20 30)
str s=abc
echo "${a[$s]}"'
[[ $rc != 0 && -z $out && $err == *"string-typed"* ]]; chk 80.41

# 80.42 …braced `${s}` is the same expansion, same answer
run_sash 'int array a=(10 20 30)
str s=abc
echo "${a[${s}]}"'
[[ $rc != 0 && -z $out && $err == *"string-typed"* ]]; chk 80.42

# 80.43 write: enforced on assignment as on read (the surfaces share the check)
run_sash 'int array a=(10 20 30)
str s=abc
a[$s]=99'
[[ $rc != 0 && -z $out && $err == *"string-typed"* ]]; chk 80.43

# 80.44 append `a[$s]+=` is the same write surface
run_sash 'array a=(x y)
str s=abc
a[$s]+=z'
[[ $rc != 0 && -z $out && $err == *"string-typed"* ]]; chk 80.44

# 80.45 a string *array* is still int-indexed — element type is not the point
run_sash 'array a=(x y z)
str s=abc
echo "${a[$s]}"'
[[ $rc != 0 && -z $out && $err == *"string-typed"* ]]; chk 80.45

# 80.46 a BARE disposition is not the way through. It defaults, but it does
# not convert: `${s:-2}` keeps its subject's type (DD#77), so the result is
# still a string and an array index must be a number. The disposition has to
# sit inside `$(( … ))`, which is what actually crosses.
run_sash 'int array a=(10 20 30)
str s=abc
echo "${a[${s:-2}]}"'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 80.46

# 80.47 an *undeclared* `i=1` is rejected, and this is the spec that reversed.
# It used to pass, on the ground that `i=1` records String exactly like
# `str s=abc` so the two could not be told apart. That was an argument about a
# *negative* test. The rule now asks whether the index is Int, which `i=1` is
# not — sash types a bare numeric literal as a string, per bash — so the
# ambiguity stops mattering and the fix is to say `int i=1`.
run_sash 'int array a=(10 20 30)
i=1
echo "${a[$i]}"'
[[ $rc != 0 && -z $out && $err == *"not int-typed"* ]]; chk 80.47

# 80.48 a declared str is still a perfectly good *map* key (DD#80: any string)
run_sash 'map m=([abc]=1)
str s=abc
echo "${m[$s]}"'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk 80.48

# 80.49 the write diagnostic is now the SAME sentence as the read one, because
# the LHS accepts the conversion it recommends. It used to have to say
# something different — an assignment index took only a bare `$var`, so the
# read path's `$(( ${s:?m} ))` advice would have sent the author from SB4030
# straight into SB4014. This pins the parity: same input, same message.
run_sash 'int array a=(10 20 30)
str s=abc
a[$s]=99'
w_err=${err#*SB4030}
run_sash 'int array a=(10 20 30)
str s=abc
echo "${a[$s]}"'
r_err=${err#*SB4030}
[[ -n $w_err && $w_err = "$r_err" && $r_err == *'$(( ${s:?'* ]]; chk 80.49

# 80.50 …and the route it does name compiles: convert first, then index.
run_sash 'int array a=(10 20 30)
str s=abc
int i=$(( ${s:-2} ))
a[$i]=99
echo "${a[2]}"'
[[ $rc = 0 && $out = "99" && -z $err ]]; chk 80.50

# ─── J. assignment-LHS / read parity (the surfaces accept the same index) ──
# Before this, the LHS took only a bare `$var`/`${var}` token. Everything
# below read fine inside `${a[…]}` and was a parse error on the left of an
# `=`, which left DD#80's own remedy — convert the string with `$(( … ))` —
# unusable at exactly the point an author needs it.

# 80.51 `$(( … ))` as an assignment index. This was SB5011: the sniffer never
#       recognised the statement as an assignment at all, so `a[` became a
#       command name and the message named neither the array nor the index.
run_sash 'int array a=(10 20 30)
a[$((1+1))]=99
echo "${a[0]} ${a[1]} ${a[2]}"'
[[ $rc = 0 && $out = "10 20 99" && -z $err ]]; chk 80.51

# 80.52 …carrying a disposition, which is the whole point: a string that has
#       been converted is a legal index on both sides. `s` is unset, so the
#       default is taken.
run_sash 'int array a=(10 20 30)
str s
a[$(( ${s:-2} ))]=99
echo "${a[2]}"'
[[ $rc = 0 && $out = "99" && -z $err ]]; chk 80.52

# 80.53 …and brace-less inside the parens, which the read side also takes.
run_sash 'int array a=(10 20 30)
str s
a[$(( $s:-2 ))]=99
echo "${a[2]}"'
[[ $rc = 0 && $out = "99" && -z $err ]]; chk 80.53

# 80.54 a quoted map key on the LHS. This was SB4014 — `m["$i"]=v` could not
#       be written even though `${m["$i"]}` could be read.
run_sash 'map m
int i=7
m["$i"]=v
echo "$(len m) ${m["7"]}"'
[[ $rc = 0 && $out = "1 v" && -z $err ]]; chk 80.54

# 80.55 a bare disposition is rejected on the LHS exactly as on the read side
# (80.46) — the parity has to hold for the rejections too, not just for what
# is accepted.
run_sash 'int array a=(10 20 30)
str s
a[${s:-2}]=99'
[[ $rc != 0 && $err == *SB4030* ]]; chk 80.55

# 80.56 `+=` takes the widened index too — the sniffer accepts both operators,
#       so the parity is not accidentally `=`-only.
run_sash 'array a=(x y z)
a[$((1+1))]+=Q
echo "${a[2]}"'
[[ $rc = 0 && $out = "zQ" && -z $err ]]; chk 80.56

# 80.57 whitespace inside the brackets is immaterial on the LHS, as it already
#       was on the read side: `parse_subscript_index` trims.
run_sash 'array a=(p q r)
a[ 1 ]=Z
echo "${a[1]}"'
[[ $rc = 0 && $out = "Z" && -z $err ]]; chk 80.57

# 80.58 the look-ahead is bounded, and an unclosed index is still not an
#       assignment: `a[` stays a command name rather than swallowing the file.
run_sash 'array a=(1 2 3)
int i=0
a[$i=5
echo done'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk 80.58


# ─── K. the key's TYPE, not its spelling (DG: array indexes must be
#        integers, maps must have strings; both surfaces alike) ───────────

# 80.59 a bare int variable is not a map key. A map is keyed by string and
#       int→string does not happen silently (DD#105) — the same rule that
#       makes `$(strlen $n)` an error on an int.
run_sash 'map m=([7]=v)
int i=7
echo "${m[$i]}"'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 80.59

# 80.60 …and quoting it is the conversion, exactly as the message says.
run_sash 'map m=([7]=v)
int i=7
echo "${m["$i"]}"'
[[ $rc = 0 && $out = "v" && -z $err ]]; chk 80.60

# 80.61 the map rule is about the key's type, not about it being a variable:
#       an arithmetic key is int-typed too, and would otherwise cross in
#       silence while `${m[$i]}` was told to quote.
run_sash 'map m=([2]=v)
echo "${m[$((1+1))]}"'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 80.61

# 80.62 …and the same quoting works for it, so the rule leaves no form
#       without a remedy.
run_sash 'map m=([2]=v)
echo "${m["$(( 1+1 ))"]}"'
[[ $rc = 0 && $out = "v" && -z $err ]]; chk 80.62

# 80.63 a cell of an `int array` is int-typed (DD#76), so it is an array index
#       — this is the nested form DD#80 blesses, and a whitelist that named
#       only literals, `$var` and `$(( … ))` would have lost it.
run_sash 'int array a=(2 0 9)
echo "${a[${a[0]}]}"'
[[ $rc = 0 && $out = "9" && -z $err ]]; chk 80.63

# 80.64 …while a cell of a *string* array is not. Same spelling, different
#       element type, different answer.
run_sash 'array a=(2 0 9)
echo "${a[${a[0]}]}"'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 80.64

# 80.65 a disposition keeps its subject's type, so on an int subject it is
#       still an index. This is why the array rule can reject a bare
#       disposition (80.46) without rejecting dispositions as such.
run_sash 'int array a=(10 20 30)
int i=2
echo "${a[${i:-0}]}"'
[[ $rc = 0 && $out = "30" && -z $err ]]; chk 80.65

# 80.66 the map write surface takes an int key no more than the read one does.
run_sash 'map m
int i=7
m[$i]=v'
[[ $rc != 0 && $err == *SB4030* ]]; chk 80.66

# ─── N. ALL THREE SPELLINGS AGREE (DG, 2026-10-03) ───────────────────────
#
# A subscript can be written three ways: braced `${a[K]}`, brace-less
# `$a[K]` (DD#33 decision B), and interpolated `"$a[K]"`. Each used to be a
# separate parse path with its own idea of what `K` may be, and they did not
# agree: the braced one took the full DD#80 grammar, the brace-less one took
# a fixed three-token shape (`[`, `$ident`, `]`), and the in-string one was
# narrower still — only an integer literal. So `echo $a[$i]` printed an
# element while `echo "$a[$i]"` was SB5064, and `$a[$(( i ))]` was SB5064
# where `${a[$(( i ))]}` worked.
#
# DG ruled all paths must agree. The lexer now desugars the brace-less and
# interpolated spellings into the braced one, so there is exactly ONE index
# grammar and the spellings cannot drift apart again. These specs are that
# agreement: each index form is pinned in the two spellings that used to
# diverge (the braced column is sections A–M above).

# 80.70/80.71 an integer literal — the one form every path already took.
run_sash 'int array a=(10 20 30)
echo $a[1]'
[[ $rc = 0 && $out = "20" && -z $err ]]; chk 80.70
run_sash 'int array a=(10 20 30)
echo "$a[1]"'
[[ $rc = 0 && $out = "20" && -z $err ]]; chk 80.71

# 80.72/80.73 a `$var` index. The brace-less spelling took this one; the
# interpolated spelling did NOT, and was SB5064.
run_sash 'int array a=(10 20 30)
int i=1
echo $a[$i]'
[[ $rc = 0 && $out = "20" && -z $err ]]; chk 80.72
run_sash 'int array a=(10 20 30)
int i=1
echo "$a[$i]"'
[[ $rc = 0 && $out = "20" && -z $err ]]; chk 80.73

# 80.74/80.75 a `${var}` index — same story.
run_sash 'int array a=(10 20 30)
int i=1
echo $a[${i}]'
[[ $rc = 0 && $out = "20" && -z $err ]]; chk 80.74
run_sash 'int array a=(10 20 30)
int i=1
echo "$a[${i}]"'
[[ $rc = 0 && $out = "20" && -z $err ]]; chk 80.75

# 80.76/80.77 an arithmetic index. Neither spelling took this before — the
# three-token gather saw `$((` and declined, so both were SB5064 while the
# braced form (80.9) computed the index.
run_sash 'int array a=(10 20 30)
int i=1
echo $a[$(( i ))]'
[[ $rc = 0 && $out = "20" && -z $err ]]; chk 80.76
run_sash 'int array a=(10 20 30)
int i=1
echo "$a[$(( i ))]"'
[[ $rc = 0 && $out = "20" && -z $err ]]; chk 80.77

# 80.78/80.79 a DD#58 sexpr call inside the arithmetic index. This is the
# deepest nesting the grammar allows, and it reaches the brace-less spelling
# intact — the desugar hands the whole balanced `[…]` over verbatim.
run_sash 'int array a=(10 20 30)
fn g -> int { return 2 }
echo $a[$(( (g) ))]'
[[ $rc = 0 && $out = "30" && -z $err ]]; chk 80.78
run_sash 'int array a=(10 20 30)
fn g -> int { return 2 }
echo "$a[$(( (g) ))]"'
[[ $rc = 0 && $out = "30" && -z $err ]]; chk 80.79

# 80.80/80.81 a nested subscript as the index. Balanced brackets inside the
# index are what the old byte-level gather could not do at all.
run_sash 'int array a=(10 20 30)
int array b=(2 0)
echo $a[${b[0]}]'
[[ $rc = 0 && $out = "30" && -z $err ]]; chk 80.80
run_sash 'int array a=(10 20 30)
int array b=(2 0)
echo "$a[${b[0]}]"'
[[ $rc = 0 && $out = "30" && -z $err ]]; chk 80.81

# 80.82/80.83 a map read with a `$var` key, both spellings.
run_sash 'map m=([k]=v [kk]=w)
str s=kk
echo $m[$s]'
[[ $rc = 0 && $out = "w" && -z $err ]]; chk 80.82
run_sash 'map m=([k]=v [kk]=w)
str s=kk
echo "$m[$s]"'
[[ $rc = 0 && $out = "w" && -z $err ]]; chk 80.83

# ── the agreement has to hold for REJECTS too, or a spelling becomes an
#    escape hatch from a rule the other spellings enforce. ──

# 80.84/80.85 a capture as an index is SB4030 (DD#80) — not SB5064, which is
# what both spellings reported when they declined to see a subscript at all.
run_sash 'int array a=(10 20 30)
echo $a[$( echo 1 )]'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 80.84
run_sash 'int array a=(10 20 30)
echo "$a[$( echo 1 )]"'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 80.85

# 80.86/80.87 a quoted string index on an int array is the string→int
# discipline, in every spelling.
run_sash 'int array a=(10 20 30)
echo $a["1"]'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 80.86
run_sash 'int array a=(10 20 30)
echo "$a["1"]"'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 80.87

# 80.88 a subscript on a SCALAR is SB5068 — the diagnostic added after
# `${s[0]}` segfaulted. The brace-less spelling was a hole in exactly that
# fix: it only reached SB5068 for the index forms its gather understood, so
# `$s[$(( 0 ))]` compiled and printed `z[0]`.
run_sash 'str s=z
echo $s[$(( 0 ))]'
[[ $rc != 0 && -z $out && $err == *SB5068* ]]; chk 80.88
run_sash 'str s=z
echo "$s[$(( 0 ))]"'
[[ $rc != 0 && -z $out && $err == *SB5068* ]]; chk 80.89

# 80.90 the BRACED splat subscript. `$@[0]` / `$*[0]` are SB5031 (80.20,
# 80.21); the braced twins must say the same thing, since the desugar routes
# the brace-less spelling through this very path.
run_sash 'fn f { : @args; echo ${@[0]} }
f a'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk 80.90
run_sash 'fn f { : @args; echo ${*[0]} }
f a'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk 80.91

# ── O. subscripting a special parameter: five ICEs, closed by construction ──
#
# `$#[0]`, `$?[0]`, `$$[0]`, `$![0]` and `$1[0]` each built a subscript on a
# collection named `#`/`?`/`$`/`!`/`1`, which no pass rejected and codegen
# then panicked on ("ICE: undefined array"). Every braced counterpart was
# already a clean diagnostic — the brace-less path simply never reached it.
# SB5031 (80.20/80.21) was a point-fix for the `@`/`*` members of this same
# family; these are the rest of it.

# 80.92 `$#` — the retired length sugar.
run_sash 'fn f { : @args; echo $#[0] }
f a'
[[ $rc != 0 && -z $out && $err == *SB4038* ]]; chk 80.92

# 80.93 `$?` — the status parameter is not a collection.
run_sash 'echo $?[0]'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk 80.93

# 80.94 `$$` — unrecognized content, same as `${$[0]}`.
run_sash 'echo $$[0]'
[[ $rc != 0 && -z $out && $err == *SB4047* ]]; chk 80.94

# 80.95 `$!` — reads as indirect expansion, which sash rejects outright.
run_sash 'echo $![0]'
[[ $rc != 0 && -z $out && $err == *SB4003* ]]; chk 80.95

# 80.96 `$1` — a positional is a scalar, so there is nothing to subscript.
run_sash 'fn f { echo $1[0] }
f a'
[[ $rc != 0 && -z $out && $err == *SB4047* ]]; chk 80.96

# ── P. the index rule holds on the WRITE side too (SB4041) ──
#
# DD#80's "all paths must agree" applied to `a[K]=v`. Section A rejects an
# ambiguous index when it is READ; every one of those forms used to be
# accepted when it was WRITTEN, because an assignment's LHS index is a bare
# `Expr` in `Assignment.index` and is never wrapped in a `Subscript` node —
# the expression walk reached it but could not tell it was an index.
#
# This was not merely a missing error. `m[ $s x ]=9` stored the UNEXPANDED
# text `$s x` as the key (80.99), and `m[$s ] x ]=9` silently discarded the
# ` x ` entirely (80.103): the shape probe committed to the parse on a `]`
# followed by `=`, the LHS parser closed on the FIRST `]`, and the leftover
# ` x ` was then consumed as if it were the `=`.

# 80.97 whitespace in a bare write index — the canonical form
run_sash 'map m=( ); m[ a b ]="z"; echo ${m["a b"]}'
[[ $rc != 0 && -z $out && $err == *SB4041* ]]; chk 80.97

# 80.98 ... and its read twin, which has always rejected. They must agree.
run_sash 'map m=( ); m["a b"]="z"; echo ${m[ a b ]}'
[[ $rc != 0 && -z $out && $err == *SB4041* ]]; chk 80.98

# 80.99 a `$var` mixed with a bareword — this used to store the literal
# text `$s x` as the key, with no expansion and no diagnostic.
run_sash 'map m=( ); s="k"; m[ $s x ]="z"; echo ${m["$s x"]}'
[[ $rc != 0 && -z $out && $err == *SB4041* ]]; chk 80.99

# 80.100 an operator in a bare write index (the 80.3 footgun, writing)
run_sash 'map m=( ); m[a-b]="z"; echo ${m["a-b"]}'
[[ $rc != 0 && -z $out && $err == *SB4041* ]]; chk 80.100

# 80.101 any non-identifier punctuation, not just arithmetic operators
run_sash 'map m=( ); m[a.b]="z"; echo ${m["a.b"]}'
[[ $rc != 0 && -z $out && $err == *SB4041* ]]; chk 80.101

# 80.102 the read twin of 80.100 — agreement again
run_sash 'map m=( ); m["a-b"]="z"; echo ${m[a-b]}'
[[ $rc != 0 && -z $out && $err == *SB4041* ]]; chk 80.102

# 80.103 the silent-discard shape: two `]` in the index, the second one
# carrying the `=`. Before the fix this compiled clean and assigned the
# literal string `]=9` to key `zz`, dropping ` x ` without a word.
run_sash 'map m=( ); s="k"; m[$s ] x ]="z"; echo ${m[$s]}'
[[ $rc != 0 && -z $out && $err == *SB4041* ]]; chk 80.103

# 80.104 a bare `]` as the whole index
run_sash 'map m=( ); m[ ] ]="z"; echo ${m["]"]}'
[[ $rc != 0 && -z $out && $err == *SB4041* ]]; chk 80.104

# 80.105 ... and with trailing text after it
run_sash 'map m=( ); m[ ] x ]="z"; echo ${m["]"]}'
[[ $rc != 0 && -z $out && $err == *SB4041* ]]; chk 80.105

# 80.106 the check recurses into nested bodies, not just top-level statements
run_sash 'array a=( p q ); fn f { a[ x y ]="z" }; f; echo ${a[0]}'
[[ $rc != 0 && -z $out && $err == *SB4041* ]]; chk 80.106

# ── Q. the legal write forms are untouched ──
#
# The rule rejects an ambiguous index, not a computed one — every spelling
# that states its index directly still works.

# 80.107 an integer literal
run_sash 'array a=( p q ); a[0]="z"; echo ${a[0]}'
[[ $rc == 0 && $out == "z" ]]; chk 80.107

# 80.108 a plain `$var`
run_sash 'array a=( p q ); int i=1; a[$i]="z"; echo ${a[$i]}'
[[ $rc == 0 && $out == "z" ]]; chk 80.108

# 80.109 explicit arithmetic — the escape hatch the diagnostic advises
run_sash 'array a=( p q r ); int i=1; a[$(( i + 1 ))]="z"; echo ${a[2]}'
[[ $rc == 0 && $out == "z" ]]; chk 80.109

# 80.110 a clean-identifier bareword key on a map
run_sash 'map m=( ); m[key]="z"; echo ${m[key]}'
[[ $rc == 0 && $out == "z" ]]; chk 80.110

# 80.111 a quoted key — the other escape hatch, and the one that makes the
# whitespace of 80.97 expressible
run_sash 'map m=( ); m["a b"]="z"; echo ${m["a b"]}'
[[ $rc == 0 && $out == "z" ]]; chk 80.111

# 80.112 `$var` and `${var}` key a map identically
run_sash 'map m=( ); s="k"; m[$s]="1"; m[${s}]="2"; echo ${m[$s]}'
[[ $rc == 0 && $out == "2" ]]; chk 80.112

# 80.113 append through a subscript still parses its index the same way
run_sash 'array a=( p q ); a[0]+="z"; echo ${a[0]}'
[[ $rc == 0 && $out == "pz" ]]; chk 80.113

# ── R. a cell takes a brace-less disposition (DD#90 scope, DG 2026-10-04) ──
# "A string cell behaves like a string variable" settles every operator, so the
# DD#90 scope list (scalar, positional, splat) takes a fourth entry rather than
# a cell-shaped exception. Before this, `$a[9]:?m` silently was NOT a guard —
# it read empty, concatenated `:?m` as literal bytes, and continued with a
# truthy string. No new legibility limit was needed: the `:` must be tight
# against the `]` exactly as it must be tight against a scalar's name, and the
# operand ends at the same word boundary.

# 80.114 `:-` on a present cell yields the cell
run_sash 'array a=( p q ); echo $a[0]:-z'
[[ $rc == 0 && $out == "p" ]]; chk 80.114

# 80.115 `:-` on an absent cell yields the default (was `:-z` literal)
run_sash 'array a=( p q ); echo $a[9]:-z'
[[ $rc == 0 && $out == "z" ]]; chk 80.115

# 80.116 `:?` is a GUARD — the row that made this a correctness fix, not an
# ergonomic one. It used to print `:?m` and exit 0.
run_sash 'array a=( p q ); echo $a[9]:?m; echo reached'
[[ $rc != 0 && $out != *reached* ]]; chk 80.116

# 80.117 `:~` elides the argument rather than passing a literal `:~`
run_sash 'array a=( p q ); echo x $a[9]:~ y'
[[ $rc == 0 && $out == "x y" ]]; chk 80.117

# 80.118 a map cell disposes the same way
run_sash 'map m=( [k]=v ); echo $m[no]:-z'
[[ $rc == 0 && $out == "z" ]]; chk 80.118

# 80.119 brace-less and braced agree — the whole point of the desugar
run_sash 'array a=( p q ); echo $a[9]:-z ${a[9]:-z}'
[[ $rc == 0 && $out == "z z" ]]; chk 80.119

# 80.120 interpolation still requires braces (DG: "err on the side of
# clarity"), so inside `"…"` the tail stays literal
run_sash 'array a=( p q ); echo "$a[9]:-z"'
[[ $rc == 0 && $out == ":-z" ]]; chk 80.120

# 80.121 a non-disposition `:X` after a cell is still literal concatenation,
# exactly as after a scalar — only `:-`, `:?`, `:~` open a disposition
run_sash 'array a=( p q ); echo $a[0]:/bin'
[[ $rc == 0 && $out == "p:/bin" ]]; chk 80.121

# 80.122 the `:` must be tight: a space makes it a separate word, so the
# "where would the braces go" question never arises
run_sash 'array a=( p q ); echo $a[0] :-z'
[[ $rc == 0 && $out == "p :-z" ]]; chk 80.122

# ── Section R. a `${ … }` index reads the same in every spelling ──────────
# The arithmetic lexer used to end a braced body at the *first* `}`, so a
# nested `${ … }` index truncated (`${a[${i}]}` -> `a[${i`) and the trailing
# `]` lexed as a stray: SB4035. The three spellings that did not route
# through that scanner accepted it, so one read had two answers.

# 80.123 braced cell, braced index, in arithmetic
run_sash 'int array a=(2 0 9); int i=1; echo $(( ${a[${i}]} ))'
[[ $rc == 0 && $out == "0" ]]; chk 80.123

# 80.124 sigil cell, braced index, in arithmetic (the lexer desugars this to
# the 80.123 spelling, so it used to fail the same way)
run_sash 'int array a=(2 0 9); int i=1; echo $(( $a[${i}] ))'
[[ $rc == 0 && $out == "0" ]]; chk 80.124

# 80.125 bare cell, braced index — this one always worked, and is the reason
# the divergence was a bug rather than a restriction
run_sash 'int array a=(2 0 9); int i=1; echo $(( a[${i}] ))'
[[ $rc == 0 && $out == "0" ]]; chk 80.125

# 80.126 the same index outside arithmetic
run_sash 'int array a=(2 0 9); int i=1; echo ${a[${i}]}'
[[ $rc == 0 && $out == "0" ]]; chk 80.126

# 80.127 all four spellings in one program agree
run_sash 'int array a=(2 0 9); int i=2; echo $(( ${a[${i}]} )) $(( $a[${i}] )) $(( a[${i}] )) ${a[${i}]}'
[[ $rc == 0 && $out == "9 9 9 9" ]]; chk 80.127

# 80.128 a braced index that is not a plain identifier is a literal key, so
# an int-indexed array rejects it as ambiguous rather than reading it
run_sash 'int array a=(2 0 9); echo $(( ${a[${1}]} ))'
[[ $rc != 0 && $err == *"SB4041"* ]]; chk 80.128

# 80.129 a map's string key reads the same through a braced index in both
# contexts — the arithmetic one is the spelling that used to truncate
run_sash 'int map m=([a]=5); str k=a; echo $(( ${m[${k}]} + 1 )) ${m[${k}]}'
[[ $rc == 0 && $out == "6 5" ]]; chk 80.129

# 80.130 an unbalanced `${` inside arithmetic is still reported, not scanned
# to the end of the program in silence
run_sash 'int i=1; echo $(( ${i ))'
[[ $rc != 0 ]]; chk 80.130

# ── an explicit index that fails to parse reports its own error (DD#80,
# 2026-10-24 addendum) ──
#
# A braced index is re-parsed as a value expression, and when that failed the
# text fell back to a literal that the validator rejects as ambiguous (SB4041).
# For an implicit form that is right: `(1+2)` has no construct of its own to
# report (cov.125 pins that boundary with an empty index). For an explicit
# form — `$var`, an expansion, a quoted key — SB4041's advice to write an
# explicit form sends the author back to the one they wrote:
# `${a[$(( 1 + ))]}` was told to open arithmetic explicitly. Now the explicit
# form's own parse error wins.

# 80.131 the headline: an arithmetic index with a missing operand
run_sash 'array a=(1 2)
echo "${a[$(( 1 + ))]}"'
[[ $rc != 0 && -z $out && $err == *SB4020* \
   && $err == *'ends where an operand is expected'* && $err != *SB4041* ]]; chk 80.131
# 80.132 a write index shares the reader
run_sash 'array a=(1 2)
a[$(( 1 + ))]=x
echo "${a[0]}"'
[[ $rc != 0 && -z $out && $err == *SB4020* \
   && $err == *'ends where an operand is expected'* && $err != *SB4041* ]]; chk 80.132
# 80.133 a quoted key is an explicit form too
run_sash 'array a=(1 2)
echo "${a["$(( 1 + ))"]}"'
[[ $rc != 0 && -z $out && $err == *SB4020* \
   && $err == *'ends where an operand is expected'* && $err != *SB4041* ]]; chk 80.133
# 80.134 a stray `$` in the index's arithmetic (DD#72, 2026-10-24 addendum).
# It used to read `a[0]` natively and emit C that gcc rejects.
run_sash 'array a=(p q)
echo "${a[$(( $ ))]}"'
[[ $rc != 0 && -z $out && $err == *SB4035* && $err == *'stray `$` in arithmetic'* ]]; chk 80.134
# 80.135 an empty `${}` as the index names no variable (DD#65, 2026-10-24
# addendum); it used to be SB4030 advice to convert a variable called `$`
run_sash 'array a=(p q)
echo "${a[${}]}"'
[[ $rc != 0 && -z $out && $err == *SB4047* && $err == *'`${}` names no variable'* ]]; chk 80.135


set -e
tally
