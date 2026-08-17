#!/bin/bash
# spec/arithmetic.sh — arithmetic context tests (DD#31)
set -eu
. common.sh
setup arithmetic
plan=107
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

# 26.7 DD#72: C-octal `017` dropped — leading-zero rejects (SB4035); the
# modern `0o17` octal prefix is the replacement and still yields 15.
run_sash 'echo $(( 017 ))'
[[ $rc != 0 && -z $out && $err == *SB4035* ]]; chk 26.7
# 26.7b the 0o octal prefix works (regression guard)
run_sash 'echo $(( 0o17 ))'
[[ $rc = 0 && $out = "15" && -z $err ]]; chk 26.7b

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

# --- DD#58/#75: int-returning intrinsics evaluate in $(( )) sexpr calls ---
# DG demo: `$(( (len a) + 1 ))` must yield count+1 (was len→0 → 1).
# (janet lacks the `len` intrinsic in arith-operand position — it emits an
# undefined `(len …)` symbol; pre-existing class, cf. janet join/keys gaps.
# Guard the runtime to native,c; the transform/parse work is backend-shared.)
if in_modes native,c; then
    # 58.10 (len a) in arith yields the element count
    run_sash 'array a=(x y z); int n=$(( (len a) + 1 )); echo $n'
    [[ $rc = 0 && $out = "4" && -z $err ]]; chk 58.10
    # 58.11 bare (len a) in arith (no +1)
    run_sash 'array a=(a b c d); echo $(( (len a) ))'
    [[ $rc = 0 && $out = "4" && -z $err ]]; chk 58.11
    # 58.12 (len s) on a string in arith → byte length
    run_sash 's=hello; int n=$(( (len s) * 2 )); echo $n'
    [[ $rc = 0 && $out = "10" && -z $err ]]; chk 58.12
    # 58.13 (len m) on a map in arith → entry count
    run_sash 'map m=([a]=1 [b]=2 [c]=3); echo $(( (len m) ))'
    [[ $rc = 0 && $out = "3" && -z $err ]]; chk 58.13
    # DD#75 §2: `${#x}` braced-length is int-valued inside $(( )) too (the
    # canonical length form; parses to the same len intrinsic as (len x)).
    # 58.14 ${#a} array count in arith
    run_sash 'array a=(x y z); int n=$(( ${#a} + 1 )); echo $n'
    [[ $rc = 0 && $out = "4" && -z $err ]]; chk 58.14
    # 58.15 ${#s} string byte length in arith
    run_sash 's=hello; int n=$(( ${#s} * 2 )); echo $n'
    [[ $rc = 0 && $out = "10" && -z $err ]]; chk 58.15
    # 58.16 ${#m} map entry count in arith
    run_sash 'map m=([a]=1 [b]=2 [c]=3); echo $(( ${#m} ))'
    [[ $rc = 0 && $out = "3" && -z $err ]]; chk 58.16
else
    skip_chk 58.10; skip_chk 58.11; skip_chk 58.12; skip_chk 58.13
    skip_chk 58.14; skip_chk 58.15; skip_chk 58.16
fi

# DD#24/DD#31 — the clean special vars ($$ pid, $? status, $# argc) are valid
# `$(( ))` operands, including inside a *compound* expression (not just
# standalone). Native regression: `$$`/`$?` read 0 as a compound-arith operand
# because the int-lowering path missed the getpid / exit-status specials that
# the string path already handles ($# worked — it is a materialized slot). c
# handles all three; janet lacks `$$` in arith (filed janet-special-var-arith).
if in_modes native,c; then
    # 31.20 $$ (pid) as a comparison operand — a pid is always positive
    run_sash 'echo $(( $$ > 0 ))'
    [[ $rc = 0 && $out = "1" && -z $err ]]; chk 31.20
    # 31.21 $$ inside a compound arithmetic operand (pid*2 is positive)
    run_sash 'echo $(( $$ * 2 > 0 ))'
    [[ $rc = 0 && $out = "1" && -z $err ]]; chk 31.21
    # 31.22 $? (=1 after arith-false) as a compound operand → 1 + 7 = 8
    run_sash '(( 0 )); echo $(( $? + 7 ))'
    [[ $rc = 0 && $out = "8" && -z $err ]]; chk 31.22
    # 31.23 $? (=0 after arith-true) as a compound operand → 0 + 7 = 7
    run_sash '(( 1 )); echo $(( $? + 7 ))'
    [[ $rc = 0 && $out = "7" && -z $err ]]; chk 31.23
    # 31.24 $# (arg count) as a compound operand (already worked — pin it)
    run_sash 'f() { echo $(( $# * 2 )); }; f a b c'
    [[ $rc = 0 && $out = "6" && -z $err ]]; chk 31.24
else
    skip_chk 31.20; skip_chk 31.21; skip_chk 31.22; skip_chk 31.23; skip_chk 31.24
fi

# ── DD#58/Option C — sash-native intrinsics in an arithmetic sexpr $(( (f) )) ──
# An int producer (user `-> int` fn OR int intrinsic) works in BOTH $(f) capture
# and the $(( (f) )) sexpr. Previously ONLY `len` worked in the sexpr; every
# other intrinsic silently evaluated to 0. See bug-arith-sexpr-intrinsics-zero.

# Category A — int intrinsics compute (not silent 0). native+c (janet execs
# the intrinsics / emits invalid sexpr — filed, same gap as len 58.10-16).
if in_modes native,c; then
    # sx.1 argc in a sexpr (C argc = 1: program name only, no extra args)
    run_sash 'echo $(( (argc) ))'
    [[ $rc = 0 && $out = "1" && -z $err ]]; chk sx.1
    # sx.2 getpid in a sexpr — a pid is positive
    run_sash 'echo $(( (getpid) > 0 ))'
    [[ $rc = 0 && $out = "1" && -z $err ]]; chk sx.2
    # sx.3 getppid in a sexpr — positive
    run_sash 'echo $(( (getppid) > 0 ))'
    [[ $rc = 0 && $out = "1" && -z $err ]]; chk sx.3
    # sx.4 epochseconds in a sexpr — unix time is large/positive
    run_sash 'echo $(( (epochseconds) > 0 ))'
    [[ $rc = 0 && $out = "1" && -z $err ]]; chk sx.4
    # sx.5 status in a sexpr — arith-false sets status 1
    run_sash '(( 0 )); echo $(( (status) ))'
    [[ $rc = 0 && $out = "1" && -z $err ]]; chk sx.5
    # sx.6 int intrinsic composes in a larger arith expression
    run_sash 'echo $(( (argc) + 10 ))'
    [[ $rc = 0 && $out = "11" && -z $err ]]; chk sx.6
    # sx.8 random in a sexpr, captured to int — two draws differ (buggy → 0,0)
    run_sash 'int a=$(( (random) )); int b=$(( (random) )); echo $(( a != b ))'
    [[ $rc = 0 && $out = "1" && -z $err ]]; chk sx.8
else
    skip_chk sx.1; skip_chk sx.2; skip_chk sx.3; skip_chk sx.4
    skip_chk sx.5; skip_chk sx.6; skip_chk sx.8
fi
# sx.7 signum — native only: c/janet emit a TODO stub for the signum intrinsic
# (filed c-janet-signum-intrinsic), so it can't be tested there yet.
if in_modes native; then
    # signum with no signal received → 0 (pins it computes in a sexpr)
    run_sash 'echo $(( (signum) ))'
    [[ $rc = 0 && $out = "0" && -z $err ]]; chk sx.7
else
    skip_chk sx.7
fi

# Grouping — `(x)` on a plain variable is arithmetic grouping, not a nullary
# call. The arith parser emits FuncCall for every `(name)` and defers the
# decision; a name that is neither an intrinsic nor a known function is a
# variable. Regression: `$(( (x) ))` silently returned 0. native,c (janet
# sexpr-resolution parity filed).
if in_modes native,c; then
    # sx.9 grouped int variable
    run_sash 'int x=5; echo $(( (x) ))'
    [[ $rc = 0 && $out = "5" && -z $err ]]; chk sx.9
    # sx.10 grouped variable composes with an operator
    run_sash 'int x=5; echo $(( (x) + 1 ))'
    [[ $rc = 0 && $out = "6" && -z $err ]]; chk sx.10
    # sx.11 Q3: random is int-typed — int n=$(random) captures (was SB4030)
    run_sash 'int n=$(random); echo ok'
    [[ $rc = 0 && $out = "ok" && -z $err ]]; chk sx.11
else
    skip_chk sx.9; skip_chk sx.10; skip_chk sx.11
fi

# Category B — a STRING producer in a sexpr is a type error (SB4030), not silent
# 0: it sits in int-producer position; capture it first with $(). Compile-time →
# all backends. sx.14 also closes the getenv validator-bypass ($(( (getenv X) ))
# silently compiled to 0, sidestepping the word form's SB5020 taint reject).
# sx.12 toupper (string) in a sexpr → reject
run_sash 's=hi; echo $(( (toupper s) ))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sx.12
# sx.13 join (string) in a sexpr → reject
run_sash 'array a=(x y); echo $(( (join a) ))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sx.13
# sx.14 getenv (tainted string) in a sexpr → reject (was a validator bypass)
run_sash 'echo $(( (getenv HOME) ))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sx.14
# sx.15 keys (splat) in a sexpr → reject
run_sash 'map m=([a]=1); echo $(( (keys m) ))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sx.15
# sx.16 a user string function in a sexpr → reject (also stops side-effect run)
run_sash 'fn g { echo hi; }; echo $(( (g) ))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sx.16

# Q2 — dtoi/itod rejected in a sexpr (capture via $(): `int n=$(dtoi 2 "3.14")`).
# sx.17 itod (string result) in a sexpr → reject
run_sash 'echo $(( (itod 2 5) ))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sx.17

# ── array/map subscripts as an rvalue in arithmetic (`${…}` value-import) ──
# BUG: `${a[i]}` in `(( ))` read a bogus variable named "a[i]" → 0. `${…}` is a
# normal-parse rvalue value-import into arith; fixes the silent-0. native,c —
# janet's arith VALUE path doesn't read the embedded subscript (filed
# janet-arith-subscript-read; its condition path does — see asx.5).
if in_modes native,c; then
    # asx.1 ${a[0]} literal index read
    run_sash 'array a=(10 20 30); echo $(( ${a[0]} + 1 ))'
    [[ $rc = 0 && $out = "11" && -z $err ]]; chk asx.1
    # asx.2 ${a[$i]} dynamic index read
    run_sash 'array a=(10 20 30); i=1; echo $(( ${a[$i]} + 1 ))'
    [[ $rc = 0 && $out = "21" && -z $err ]]; chk asx.2
    # asx.3 ${m[k]} map read (string key)
    run_sash 'map m=([k]=5); echo $(( ${m[k]} + 1 ))'
    [[ $rc = 0 && $out = "6" && -z $err ]]; chk asx.3
    # asx.4 subscript rvalue in a (( )) statement
    run_sash 'array a=(10 20 30); (( x = ${a[1]} + 5 )); echo $x'
    [[ $rc = 0 && $out = "25" && -z $err ]]; chk asx.4
    # asx.6 map arith-mutation via the statement form (${m[k]} read + m[k]= write);
    # the cell is string-typed so the int result needs DD#30 `"…"` conversion.
    run_sash 'map m=([k]=5); m[k]="$(( ${m[k]} + 1 ))"; echo ${m[k]}'
    [[ $rc = 0 && $out = "6" && -z $err ]]; chk asx.6
else
    skip_chk asx.1; skip_chk asx.2; skip_chk asx.3; skip_chk asx.4; skip_chk asx.6
fi
# asx.5 subscript rvalue in a condition (janet's condition path handles it)
run_sash 'array a=(10 20 30); if (( ${a[0]} > 5 )); then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk asx.5

# ── DD#76 int-valued collections (`int array` / `int map`) ──
# An int collection stores each element as an int cell (val_len double-duty);
# `${a[i]}` types Int (feeds `int x=${a[i]}` and arithmetic with no atoi),
# renders as decimal in string context. native,c — janet treats the collection
# variable as a scalar int (filed janet-int-valued-collections; its is_int model
# conflates the collection var with its element type). Compile-rejects (ivc.9,
# ivc.10) are backend-independent and run in every mode.
if in_modes native,c; then
    # ivc.1 int array element read in string context → decimal
    run_sash 'int array a=(10 20 30); echo ${a[0]}'
    [[ $rc = 0 && $out = "10" && -z $err ]]; chk ivc.1
    # ivc.2 int array element read in arithmetic (raw i64, no atoi)
    run_sash 'int array a=(10 20 30); echo $(( ${a[0]} + 5 ))'
    [[ $rc = 0 && $out = "15" && -z $err ]]; chk ivc.2
    # ivc.3 int array element assign (arith RHS, no DD#30 quotes)
    run_sash 'int array a=(10 20 30); a[1]=$(( 99 )); echo ${a[1]}'
    [[ $rc = 0 && $out = "99" && -z $err ]]; chk ivc.3
    # ivc.4 int map read (two string keys)
    run_sash 'int map m=([k]=5 [j]=7); echo ${m[k]}${m[j]}'
    [[ $rc = 0 && $out = "57" && -z $err ]]; chk ivc.4
    # ivc.5 int map element read in arithmetic
    run_sash 'int map m=([k]=5); echo $(( ${m[k]} * 2 ))'
    [[ $rc = 0 && $out = "10" && -z $err ]]; chk ivc.5
    # ivc.6 int x=${a[i]} — an int cell feeds an int decl (no SB4030)
    run_sash 'int array a=(10 20 30); int x=${a[0]}; echo $x'
    [[ $rc = 0 && $out = "10" && -z $err ]]; chk ivc.6
    # ivc.7 int map arith-mutation without DD#30 quotes (cell is int)
    run_sash 'int map m=([k]=1); m[k]=$(( ${m[k]} + 4 )); echo ${m[k]}'
    [[ $rc = 0 && $out = "5" && -z $err ]]; chk ivc.7
    # ivc.11 int-cell arith RHS reads another int cell via ${…}
    run_sash 'int array a=(10 20 30); a[1]=$(( ${a[0]} * 2 )); echo ${a[1]}'
    [[ $rc = 0 && $out = "20" && -z $err ]]; chk ivc.11
else
    skip_chk ivc.1; skip_chk ivc.2; skip_chk ivc.3; skip_chk ivc.4; skip_chk ivc.5
    skip_chk ivc.6; skip_chk ivc.7; skip_chk ivc.11
fi
# ivc.8 axis stacking: `local int array` inside a function. native only — emit_c
# leaves the function-local collection var (`a_ptr`) undeclared (filed
# c-local-int-collection-decl); native handles the save/restore + table slot.
if in_modes native; then
    run_sash 'fn f { local int array a=(7 8); echo ${a[1]}; }; f'
    [[ $rc = 0 && $out = "8" && -z $err ]]; chk ivc.8
else
    skip_chk ivc.8
fi
# ivc.9 a non-numeric element in an int collection → SB4030 (compile-reject)
run_sash 'int array a=(hi 2); echo ${a[0]}'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk ivc.9
# ivc.10 bare `a[i]=99` (numeric literal) on an int collection stores the int
# cell — DD#77/DD#30-refine: a bare numeric literal to a known-int LHS aligns
# with `int x=5` (see types.sh 30.5/30.8). native,c (janet int-collection gap).
if in_modes native,c; then
    run_sash 'int array a=(10 20 30); a[1]=99; echo ${a[1]}'
    [[ $rc = 0 && $out = "99" && -z $err ]]; chk ivc.10
else
    skip_chk ivc.10
fi

# ── DD#77 A: ${var:OP…} routed through the real param-expansion parser ──
# BUG (bug-arith-param-expansion-operator-zero): `${s:-N}`/`${s:?…}`/etc inside
# `(( ))`/`$(( ))` swallowed the operator into a bogus var name → silent 0. Now
# the body goes through parse_parameter_expansion. These assert the fix with
# stable cases (a valid-int read + an unset default — unchanged by DD#77 C).
# pex.1 ${s:-N} reads a set valid-int operand (was silently 0)
# native,c — janet's arith VALUE path doesn't read an embedded ${…} expansion
# (reads 0); its condition path does (pex.4). Same gap as asx.* subscripts —
# janet-arith-subscript-read (extended to ${var:OP}).
if in_modes native,c; then
    run_sash 's=5; echo $(( ${s:-9} ))'
    [[ $rc = 0 && $out = "5" && -z $err ]]; chk pex.1
    # pex.2 ${s:-N} default fires on unset
    run_sash 'echo $(( ${s:-9} ))'
    [[ $rc = 0 && $out = "9" && -z $err ]]; chk pex.2
    # pex.3 composes with an operator (was 1 via bogus-var 0)
    run_sash 's=5; echo $(( ${s:-9} + 1 ))'
    [[ $rc = 0 && $out = "6" && -z $err ]]; chk pex.3
else
    skip_chk pex.1; skip_chk pex.2; skip_chk pex.3
fi
# pex.4 in a (( )) condition (was reading 0 → wrong branch); janet's condition
# path reads the embedded expansion, so this runs in every mode.
run_sash 's=5; if (( ${s:-9} > 3 )); then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk pex.4
# pex.5 a DD#65-pruned operator embedded in arith must still SB4034-reject (not
# ICE) — Stage A routed ${…} through the real parser, which surfaced these; the
# walk must descend arith so the reject fires. All arith positions.
run_sash 'echo $(( ${s:?bad} ))'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk pex.5
# pex.6 same in a condition position (the (( )) ArithTest walk)
run_sash 's=x; if (( ${s:+1} > 0 )); then echo y; fi'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk pex.6

set -e
tally

