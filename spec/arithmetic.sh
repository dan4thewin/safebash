#!/bin/bash
# spec/arithmetic.sh — arithmetic context tests (DD#31)
set -eu
. common.sh
setup arithmetic
plan=54
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

# 31.1j-l left-shift `<<` / `<<=` (regression: the shell lexer scanned
# `<<` as a heredoc sentinel, so `x << n` lost the operator and gave the
# wrong result; `>>` was unaffected). bug-arith-left-shift-heredoc.
run_sash 'int x=3; (( y = x << 2 )); echo $y'
[[ $rc = 0 && $out = "12" && -z $err ]]; chk 31.1j

run_sash 'int x=3; echo $(( x << 4 ))'
[[ $rc = 0 && $out = "48" && -z $err ]]; chk 31.1k

run_sash 'int x=3; (( x <<= 2 )); echo $x'
[[ $rc = 0 && $out = "12" && -z $err ]]; chk 31.1l

# 31.1m `>>` right-shift unaffected (regression guard)
run_sash 'int x=48; (( x >>= 2 )); echo $x'
[[ $rc = 0 && $out = "12" && -z $err ]]; chk 31.1m

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

    # 25.9 Pow overflow traps (was: saturated to i64::MAX silently)
    run_sash 'fn p { int a=$(($1)); int b=$(($2)); (( c = a ** b )); echo $c; }
p 2 70'
    [[ $rc != 0 && -z $out ]]; chk 25.9

    # 25.10 Div i64::MIN / -1 traps cleanly (was: SIGFPE core dump, rc 136).
    # Assert the clean trap exit (sash_arith_overflow exits 2), not a crash.
    run_sash 'int x=-9223372036854775808; int y=-1; (( c = x / y )); echo $c'
    [[ $rc = 2 && -z $out ]]; chk 25.10

    # 25.11 unary Neg of i64::MIN traps (regression: -x → 0 - x → ssub_overflow)
    run_sash 'int x=-9223372036854775808; (( c = -x )); echo $c'
    [[ $rc != 0 && -z $out ]]; chk 25.11

    # 25.12 Mod i64::MIN % -1 is 0 (NOT an overflow — srem doesn't trap)
    run_sash 'int x=-9223372036854775808; int y=-1; (( c = x % y )); echo $c'
    [[ $rc = 0 && $out = "0" ]]; chk 25.12
else
    skip_chk 25.2; skip_chk 25.3; skip_chk 25.4
    skip_chk 25.5; skip_chk 25.6; skip_chk 25.7
    skip_chk 25.9; skip_chk 25.10; skip_chk 25.11; skip_chk 25.12
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

# --- 26 integer literal range & bit-patterns (bug-arith-literal-out-of-range) ---
# Policy A (DG 2026-06-27, bash-faithful): a DECIMAL literal whose magnitude
# exceeds i64::MAX is a COMPILE ERROR (SB4026); hex/octal literals are 64-bit
# BIT-PATTERNS — reinterpreted u64->i64 at scan time, so
# 0xFFFFFFFFFFFFFFFF == -1 — and only error when wider than 64 bits; i64::MIN
# is expressible as an arith literal via the unary-neg peephole. Compile-error
# cases are front-end (all modes); the i64::MAX/MIN value cases are guarded
# native,c (janet's double rounds the boundary values).
# NB: based literals (base#digits) are excluded here — they are separately
# broken (the `#` is eaten as a comment before the scanner), see
# bug-arith-based-literal-hash-comment.

# 26.1 decimal i64::MAX+1 -> compile error (was: silent 0)
run_sash 'echo $(( 9223372036854775808 ))'
[[ $rc != 0 && -z $out && -n $err ]]; chk 26.1

# 26.2 decimal far out of range -> compile error
run_sash 'echo $(( 99999999999999999999999 ))'
[[ $rc != 0 && -z $out && -n $err ]]; chk 26.2

# 26.3 non-arith int literal out of range -> compile error (was: silent 0)
run_sash 'int x=99999999999999999999999
echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 26.3

# 26.4 non-arith negative out of range -> compile error
run_sash 'int x=-99999999999999999999999
echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 26.4

# 26.5 hex wider than 64 bits -> compile error
run_sash 'echo $(( 0x1FFFFFFFFFFFFFFFF ))'
[[ $rc != 0 && -z $out && -n $err ]]; chk 26.5

# 26.6 hex all-ones is the bit-pattern -1 (bash parity); all modes
run_sash 'echo $(( 0xFFFFFFFFFFFFFFFF ))'
[[ $rc = 0 && $out = "-1" && -z $err ]]; chk 26.6

# 26.7 octal still works (regression guard)
run_sash 'echo $(( 017 ))'
[[ $rc = 0 && $out = "15" && -z $err ]]; chk 26.7

# 26.8-26.10 i64::MAX / i64::MIN magnitudes — guarded native,c
if in_modes native,c; then
    # 26.8 decimal i64::MAX is in range
    run_sash 'echo $(( 9223372036854775807 ))'
    [[ $rc = 0 && $out = "9223372036854775807" && -z $err ]]; chk 26.8

    # 26.9 arith i64::MIN via unary-neg peephole (was: silent 0)
    run_sash 'echo $(( -9223372036854775808 ))'
    [[ $rc = 0 && $out = "-9223372036854775808" && -z $err ]]; chk 26.9

    # 26.10 hex sign-bit pattern == i64::MIN (bash parity)
    run_sash 'echo $(( 0x8000000000000000 ))'
    [[ $rc = 0 && $out = "-9223372036854775808" && -z $err ]]; chk 26.10
else
    skip_chk 26.8; skip_chk 26.9; skip_chk 26.10
fi

set -e
tally

