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
plan=21
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

# ── C. explicit array indices accepted (native,c — janet subscript-value gap) ──

if in_modes native,c; then
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
else
    skip_chk 80.9; skip_chk 80.10; skip_chk 80.11; skip_chk 80.12
fi

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

# ── E. nesting unrolls (native,c) ──

if in_modes native,c; then
    # 80.18 a map key that is itself a map read: inner[k]=foo → outer[foo]=win
    run_sash 'map inner=([k]=foo); map outer=([foo]=win); echo ${outer[${inner[k]}]}'
    [[ $rc = 0 && $out = "win" && -z $err ]]; chk 80.18
    # 80.19 a double-quoted key interpolates (string-building, not arithmetic)
    run_sash 'map m=([pX]=hit); s=X; echo ${m["p$s"]}'
    [[ $rc = 0 && $out = "hit" && -z $err ]]; chk 80.19
else
    skip_chk 80.18; skip_chk 80.19
fi

# ── F. the positional splat `$@` / `$*` isn't subscriptable (was a codegen ICE
# `undefined array: @`; now a clean reject). Mode-independent (validate). ──

# 80.20 `$@[i]` rejects → use `$1`..`$N` / `${args[i]}`
run_sash 'fn f { echo $@[0]; }; f a b c'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk 80.20

# 80.21 `$*[i]` likewise
run_sash 'fn f { echo $*[0]; }; f a b c'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk 80.21

set -e
tally
