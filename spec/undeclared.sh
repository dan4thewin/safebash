#!/bin/bash
# spec/undeclared.sh — DD#62 Model B: reject a scalar read of a name
# introduced NOWHERE in scope (the typo footgun). Scalar zero values are
# meaningful (untyped → "", int → 0), so a KNOWN-but-maybe-unset read is
# allowed and yields the zero value — only a nowhere-introduced name (a
# typo) is a compile error (SB5031). Front-end check → all backends.
set -eu
. common.sh
setup undeclared
plan=37
echo "1..$plan" >&3

set +e

# ud.1 a bare read of a name assigned nowhere is a typo → SB5031
run_sash 'echo $foo'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk ud.1

# ud.2 the classic typo (msg vs mgs) is caught, and the message names it
run_sash 'msg=hi; echo $mgs'
[[ $rc != 0 && -z $out && $err == *SB5031* && $err == *mgs* ]]; chk ud.2

# ud.3 an assigned name reads fine
run_sash 'x=1; echo $x'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk ud.3

# ud.4 ${x:-DEFAULT} always compiles (explicit opt-out, even nowhere-introduced)
run_sash 'echo ${x:-def}'
[[ $rc = 0 && $out = "def" && -z $err ]]; chk ud.4

# ud.5 Model B: a KNOWN but maybe-unassigned read COMPILES (meaningful zero
#      on the else path) — NOT rejected. `x` is introduced by the then-branch.
run_sash 'v=zzz; if [[ -z $v ]]; then x=1; fi; echo [$x]'
[[ $rc = 0 && $out = "[]" && -z $err ]]; chk ud.5

# ud.6 a typed declaration introduces the name (and zero-inits).
run_sash 'int n; echo $n'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk ud.6

# ud.7 a for-loop variable is introduced
run_sash 'for i in a b c; do echo $i; done'
[[ $rc = 0 && $out = $'a\nb\nc' && -z $err ]]; chk ud.7

# ud.8 a function sees an enclosing (global) name
run_sash 'g=hi; fn f { echo $g; }; f'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk ud.8

# ud.9 a name introduced ONLY inside another function is unknown here → typo
run_sash 'fn f { helper=1; }; echo $helper'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk ud.9

# ud.10 a typo inside a function body is caught
run_sash 'fn f { val=1; echo $vla; }; f'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk ud.10

# ud.11 order-insensitive: a read before the assignment in the same scope is
#       fine (the name is introduced in scope; reads the zero value first).
run_sash 'echo [$x]; x=5; echo [$x]'
[[ $rc = 0 && $out = $'[]\n[5]' && -z $err ]]; chk ud.11

# ud.12 a typo inside a double-quoted interpolation is caught
run_sash 'name=world; echo "hello $naem"'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk ud.12

# ud.13 a read inside a ${x:-$y} default is still checked (y is a typo here)
run_sash 'echo ${x:-$undef_y}'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk ud.13

# ud.14 specials are not flagged ($? after a command)
run_sash '/bin/true; echo $?'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk ud.14

# ud.15 a typo inside a command substitution is caught (subshell reads see
#       the enclosing scope, so an unknown name there is still a typo)
run_sash 'echo $(echo $cs_typo)'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk ud.15

# ud.16 a command-substitution-local assignment is a valid introduction —
#       the validator must NOT flag the read of `csx`. Asserts only that
#       validation is clean (no SB5031); the multi-statement cmdsub runtime
#       path is a separate known gap (bug-multistatement-cmdsub-placeholder).
run_sash 'echo $(csx=1; echo $csx)'
[[ $rc = 0 && $err != *SB5031* ]]; chk ud.16

# ud.17 `$LINENO` gets no special handling (DG: no magic vars) — an unassigned
#       magic var is rejected as an undefined variable, like any other name
run_sash 'echo $LINENO'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk ud.17
# ud.18 the braced form `${LINENO}` is rejected the same way
run_sash 'echo ${LINENO}'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk ud.18

# ud.19 reading an UNDECLARED array `${z[i]}` is SB5031, not a codegen ICE
# (`undefined array` panic). The collection name is a read too (DD#80 era fix).
run_sash 'echo ${z[0]}'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk ud.19
# ud.20 the map-style `${z[k]}` read likewise
run_sash 'echo ${z[k]}'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk ud.20
# ud.21 the bare dollar-subscript cell read `$z[0]` likewise
run_sash 'echo $z[0]'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk ud.21

# ---------------------------------------------------------------- DD#97
# A parenthesized (collection) value requires an `array`/`map` declaration.
# A bare `f=(...)` used to compile and half-work: element reads and `for-in`
# were fine, but the name was never registered as a collection, so `len` read
# a table as a string (0), `join`/`unset` raised SB4030, and `f+=(...)` on an
# undeclared name was a codegen ICE.

# ud.22 a bare array literal is rejected — declare it `array`
run_sash 'f=(a b c); echo $(len f)'
[[ $rc != 0 && -z $out && $err == *SB5055* ]]; chk ud.22

# ud.23 the glob form likewise (a glob IS an array literal, DD#69)
run_sash 'f=(*); echo $(len f)'
[[ $rc != 0 && -z $out && $err == *SB5055* ]]; chk ud.23

# ud.24 the empty literal likewise
run_sash 'f=(); echo $(len f)'
[[ $rc != 0 && -z $out && $err == *SB5055* ]]; chk ud.24

# ud.25 `f+=(d)` on a nowhere-declared name is SB5055, NOT a codegen ICE
# ("ICE: undefined array" panic) — the ud.19 lesson, on the write side.
run_sash 'f+=(d); echo $(len f)'
[[ $rc != 0 && -z $out && $err == *SB5055* && $err != *ICE* ]]; chk ud.25

# ud.26 the diagnostic names the variable and prescribes the keyword form
run_sash 'items=(a b c)'
[[ $rc != 0 && $err == *SB5055* && $err == *items* && $err == *array* ]]; chk ud.26

# ud.27 appending a literal to a declared SCALAR is the same reject
run_sash 'x=hi; x+=(a)'
[[ $rc != 0 && $err == *SB5055* ]]; chk ud.27

# ud.28 re-assigning a DECLARED collection stays legal (the reject is about
# introducing a collection without the keyword, not about the `=(...)` form)
run_sash 'array f=(a b c); f=(x y); echo $(len f)'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk ud.28

# ud.29 …and so does appending to one.
run_sash 'array f=(a b c); f+=(d); echo $(len f)'
[[ $rc = 0 && $out = "4" && -z $err ]]; chk ud.29

# ── attest-all over a name introduced nowhere (DD#61 × DD#62) ──────────
# The kind check asked "is this in `collections`?" and reported the sole
# other possibility — "is a scalar" — for a name that was never introduced
# at all, then advised the scalar form, which fails too. Unlike `len`
# (SB5039) this cannot defer: nothing else rejects an attest-all over an
# undeclared name, so the program compiled and iterated a missing slot.

# ud.30 one-binder attest-all over a nowhere-introduced name is rejected
run_sash 'attest x in zz [[ -n $x ]]; echo ok'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk ud.30

# ud.31 …and says "introduced nowhere", not "is a scalar"
run_sash 'attest x in zz [[ -n $x ]]; echo ok'
[[ $err == *"introduced nowhere"* && $err != *"is a scalar"* ]]; chk ud.31

# ud.32 two-binder attest-all over a nowhere-introduced name, same answer
run_sash 'attest k v in zz [[ -n $k && -n $v ]]; echo ok'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *"introduced nowhere"* ]]; chk ud.32

# ud.33 the diagnostic names the offending word (so a typo is findable)
run_sash 'array items=(a b); attest x in itmes [[ -n $x ]]; echo ok'
[[ $rc != 0 && $err == *itmes* && $err == *"introduced nowhere"* ]]; chk ud.33

# ud.34 a DECLARED scalar still gets the scalar-form advice, not the typo one
run_sash 'str s = "hi"; attest x in s [[ -n $x ]]; echo ok'
[[ $rc != 0 && $err == *SB4030* && $err == *"is a scalar"* \
   && $err != *"introduced nowhere"* ]]; chk ud.34

# ud.35 …and so does a declared scalar under the two-binder form
run_sash 'str s = "hi"; attest k v in s [[ -n $k && -n $v ]]; echo ok'
[[ $rc != 0 && $err == *"is a scalar"* && $err != *"introduced nowhere"* ]]; chk ud.35

# ud.36 an indexed array under the two-binder form is the array advice
run_sash 'array a=(x y); attest k v in a [[ -n $k && -n $v ]]; echo ok'
[[ $rc != 0 && $err == *"indexed array"* && $err != *"introduced nowhere"* ]]; chk ud.36

# ud.37 a declared collection attests normally — the new check is inert
run_sash 'array a=(x y); attest e in a [[ -n $e ]]; echo "n=$(len a)"'
[[ $rc = 0 && $out = "n=2" && -z $err ]]; chk ud.37

set -e
tally
