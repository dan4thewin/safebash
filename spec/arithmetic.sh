#!/bin/bash
# spec/arithmetic.sh — arithmetic context tests (DD#31)
set -eu
. common.sh
setup arithmetic
plan=36
echo "1..$plan" >&3

set +e

# 31.1 statement (( )) accepts mutation operators
run_sash 'int x=0; (( x = 5 )); echo $x'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk 31.1a

run_sash 'int x=5; (( x += 3 )); echo $x'
[[ $rc = 0 && $out = "8" && -z $err ]]; chk 31.1b

# 31.1d-h compound-assign ops other than += (regression:
# bug-compound-assign-non-plus — the shell lexer split `-=`/`*=`/`/=`/`%=`
# into `- =` etc., so they parsed as a comparison `(op x 0)` and never
# wrote back; only += worked). All operands must apply and write back.
run_sash 'int x=12; (( x -= 3 )); echo $x'
[[ $rc = 0 && $out = "9" && -z $err ]]; chk 31.1d

run_sash 'int x=12; (( x *= 3 )); echo $x'
[[ $rc = 0 && $out = "36" && -z $err ]]; chk 31.1e

run_sash 'int x=12; (( x /= 4 )); echo $x'
[[ $rc = 0 && $out = "3" && -z $err ]]; chk 31.1f

run_sash 'int x=13; (( x %= 5 )); echo $x'
[[ $rc = 0 && $out = "3" && -z $err ]]; chk 31.1g

# bitwise compound-assigns + chaining
run_sash 'int x=12; (( x &= 10 )); (( x |= 1 )); (( x ^= 2 )); echo $x'
[[ $rc = 0 && $out = "11" && -z $err ]]; chk 31.1h

# 31.1i chained mixed compound-assigns (was: only the first += survived)
run_sash 'int x=10; (( x += 5 )); (( x -= 3 )); (( x *= 2 )); echo $x'
[[ $rc = 0 && $out = "24" && -z $err ]]; chk 31.1i

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
# The janet backend uses float/bignum arithmetic with no i64 overflow trap; it
# previously "passed" these only via an unrelated codegen crash. native/c are
# the reference. See bug-janet-int-overflow.
#
# bug-runtime-arith-overflow-wraps: runtime (non-const) overflow must TRAP,
# not silently wrap. 25.2/25.3 are the const path; 25.4-25.6 are the genuine
# runtime path (fn-arg operands, not const-foldable) for *, +, -. 25.7 guards
# the itoa(i64::MIN) fix (echo of MIN must not panic). 25.8 asserts the
# descriptive diagnostic (native).

if in_modes native,c; then
    # 25.2 overflow aborts even with - suppress prefix
    run_sash 'int x=9223372036854775807; (( x += 1 )); echo $x'
    [[ $rc != 0 && -z $out ]]; chk 25.2

    # 25.3 overflow aborts with nonzero exit
    run_sash 'int x=9223372036854775807; (( x = x + 1 )); echo $x'
    [[ $rc != 0 && -z $out ]]; chk 25.3

    # 25.4 runtime MUL overflow (fn-arg operands) traps — the real DD#25 case
    run_sash 'fn mul { int a=$(($1)); int b=$(($2)); (( c = a * b )); echo $c; }
mul 3037000500 3037000500'
    [[ $rc != 0 && -z $out ]]; chk 25.4

    # 25.5 runtime ADD overflow traps
    run_sash 'fn add { int a=$(($1)); int b=$(($2)); (( c = a + b )); echo $c; }
add 9000000000000000000 1000000000000000000'
    [[ $rc != 0 && -z $out ]]; chk 25.5

    # 25.6 runtime SUB overflow (underflow) traps
    run_sash 'fn sub { int a=$(($1)); int b=$(($2)); (( c = a - b )); echo $c; }
sub -9000000000000000000 1000000000000000000'
    [[ $rc != 0 && -z $out ]]; chk 25.6

    # 25.7 i64::MIN echoes correctly — itoa must not panic on negation
    run_sash 'int x=-9223372036854775808; echo $x'
    [[ $rc = 0 && $out = "-9223372036854775808" ]]; chk 25.7
else
    skip_chk 25.2; skip_chk 25.3; skip_chk 25.4
    skip_chk 25.5; skip_chk 25.6; skip_chk 25.7
fi

# 25.8 native-only: overflow prints `<argv0>: arithmetic overflow` to stderr
if in_modes native; then
    run_sash 'fn mul { int a=$(($1)); int b=$(($2)); (( c = a * b )); echo $c; }
mul 3037000500 3037000500'
    [[ $rc != 0 && $rerr == *"arithmetic overflow"* ]]; chk 25.8
else
    skip_chk 25.8
fi

# --- DD#31 comparison operators (regression: <= / >= were mis-lexed) ---

# 31.15 <= is true when equal (the bash lexer splits `<=` into `<`+`=`;
# collect_arith_content must keep them adjacent so the arith lexer recombines)
run_sash 'int x=5; if (( x <= 5 )); then echo y; else echo n; fi'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 31.15

# 31.16 <= is true when strictly less
run_sash 'if (( 3 <= 7 )); then echo y; else echo n; fi'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 31.16

# 31.17 <= is false when greater. 31.18 >= mixed. The false case exercises
# the else/skip branch (janet now threads `(( ))` comparison conditions as
# 1/0 integers — bug-janet-arith-condition fixed).
run_sash 'if (( 7 <= 3 )); then echo n; else echo y; fi'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 31.17
run_sash 'int x=5
if (( x >= 5 )); then echo a; fi
if (( x >= 4 )); then echo b; fi
if (( x >= 6 )); then echo BAD; fi'
[[ $rc = 0 && $out = $'a\nb' && -z $err ]]; chk 31.18

set -e
tally

