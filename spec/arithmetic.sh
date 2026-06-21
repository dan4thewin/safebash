#!/bin/bash
# spec/arithmetic.sh — arithmetic context tests (DD#31)
set -eu
. common.sh
setup arithmetic
plan=25
echo "1..$plan" >&3

set +e

# 31.1 statement (( )) accepts mutation operators
run_sash 'int x=0; (( x = 5 )); echo $x'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk 31.1a

run_sash 'int x=5; (( x += 3 )); echo $x'
[[ $rc = 0 && $out = "8" && -z $err ]]; chk 31.1b

run_sash 'int x=5; (( x++ )); echo $x'
[[ $rc = 0 && $out = "6" && -z $err ]]; chk 31.1c

# 31.2 statement (( )) rejects comparisons
run_sash 'int x=5; (( x > 0 ))'
[[ $rc != 0 && -z $out && -n $err ]]; chk 31.2

# 31.3 if (( )) accepts comparisons
run_sash 'int x=5; if (( x > 0 )); then echo yes; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 31.3a

run_sash 'int x=5; if (( x == 5 )); then echo eq; fi'
[[ $rc = 0 && $out = "eq" && -z $err ]]; chk 31.3b

run_sash 'int x=5; if (( x != 3 )); then echo neq; fi'
[[ $rc = 0 && $out = "neq" && -z $err ]]; chk 31.3c

# 31.4 if (( )) rejects mutations
run_sash 'int x=5; if (( x = 3 )); then echo yes; fi'
[[ $rc != 0 && -z $out && -n $err ]]; chk 31.4a

run_sash 'int x=5; if (( x += 3 )); then echo yes; fi'
[[ $rc != 0 && -z $out && -n $err ]]; chk 31.4b

run_sash 'int x=5; if (( x++ )); then echo yes; fi'
[[ $rc != 0 && -z $out && -n $err ]]; chk 31.4c

# 31.5 $(( )) rejects mutations
run_sash 'int x=5; echo $(( x = 3 ))'
[[ $rc != 0 && -z $out && -n $err ]]; chk 31.5

# 31.6 prefix ++x/--x rejected
run_sash 'int x=5; (( ++x ))'
[[ $rc != 0 && -z $out && -n $err ]]; chk 31.6

# 31.7 postfix only valid as statement
run_sash 'int x=5; (( x++ ))'
[[ $rc = 0 && -z $err ]]; chk 31.7

# 31.8 (( x = y++ )) rejected — increment inside expression
run_sash 'int x=0; int y=5; (( x = y++ ))'
[[ $rc != 0 && -z $out && -n $err ]]; chk 31.8

# 31.9 if (( x++ )) rejected — mutation in condition
run_sash 'int x=5; if (( x++ )); then echo yes; fi'
[[ $rc != 0 && -z $out && -n $err ]]; chk 31.9

# 31.10 comma operator: non-final are mutations
run_sash 'int x=0; int y=0; (( x=1, y=2 )); echo $x $y'
[[ $rc = 0 && $out = "1 2" && -z $err ]]; chk 31.10

# 31.11 comma in condition: final expr is comparison
run_sash 'int x=0; if (( x=1, x > 0 )); then echo yes; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 31.11

# 31.13 statement (( )) exit status always 0
run_sash 'int x=0; (( x = 0 )); echo $?'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk 31.13

# 31.14 bare (( x > 0 )) as statement is compile error
run_sash 'int x=5; (( x > 0 ))'
[[ $rc != 0 && -z $out && -n $err ]]; chk 31.14

# --- DD#25 overflow ---

# 25.2 overflow aborts even with - suppress prefix
run_sash 'int x=9223372036854775807; (( x += 1 )); echo $x'
[[ $rc != 0 && -z $out ]]; chk 25.2

# 25.3 overflow aborts with nonzero exit
run_sash 'int x=9223372036854775807; (( x = x + 1 )); echo $x'
[[ $rc != 0 && -z $out ]]; chk 25.3

# --- DD#31 comparison operators (regression: <= / >= were mis-lexed) ---

# 31.15 <= is true when equal (the bash lexer splits `<=` into `<`+`=`;
# collect_arith_content must keep them adjacent so the arith lexer recombines)
run_sash 'int x=5; if (( x <= 5 )); then echo y; else echo n; fi'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 31.15

# 31.16 <= is true when strictly less
run_sash 'if (( 3 <= 7 )); then echo y; else echo n; fi'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 31.16

# 31.17 <= is false when greater. 31.18 >= mixed. The false case exercises
# the else/skip branch, which the janet backend mishandles for ALL (( ))
# comparison conditions (always takes 'then'); see bug-janet-arith-condition.
# native is the reference.
if in_modes native,c; then
    run_sash 'if (( 7 <= 3 )); then echo n; else echo y; fi'
    [[ $rc = 0 && $out = "y" && -z $err ]]; chk 31.17
    run_sash 'int x=5
if (( x >= 5 )); then echo a; fi
if (( x >= 4 )); then echo b; fi
if (( x >= 6 )); then echo BAD; fi'
    [[ $rc = 0 && $out = $'a\nb' && -z $err ]]; chk 31.18
else
    skip_chk 31.17
    skip_chk 31.18
fi

set -e
tally

