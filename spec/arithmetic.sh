#!/bin/bash
# spec/arithmetic.sh — arithmetic context tests (DD#31)
set -eu
. common.sh
setup arithmetic
plan=171
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

# 25.8 native+c: overflow prints `<argv0>: arithmetic overflow` to stderr,
# which the harness now captures as $rerr. janet deferred.
if in_modes native,c; then
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
    # DD#75 §4 (amended): the `${#x}` braced-length sugar is retired; the
    # canonical arithmetic length is the `(len x)` sexpr (58.11-58.13 above).
    # 58.14 (len a) array count in arith
    run_sash 'array a=(x y z); int n=$(( (len a) + 1 )); echo $n'
    [[ $rc = 0 && $out = "4" && -z $err ]]; chk 58.14
    # 58.15 (len s) string byte length in arith
    run_sash 's=hello; int n=$(( (len s) * 2 )); echo $n'
    [[ $rc = 0 && $out = "10" && -z $err ]]; chk 58.15
    # 58.16 (len m) map entry count in arith (int-typed)
    run_sash 'map m=([a]=1 [b]=2 [c]=3); int n=$(( (len m) + 1 )); echo $n'
    [[ $rc = 0 && $out = "4" && -z $err ]]; chk 58.16
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
# sx.7 signum — native+c: both read the sash_signum global. janet still
# emits a TODO stub for the signum intrinsic (janet deferred).
if in_modes native,c; then
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

# ── arith-subscript model B Stage 2: BARE `a[i]` (int-array cell, rvalue +
# lvalue). native,c — janet's arith VALUE path doesn't read the embedded
# subscript (janet-arith-subscript-read). ──
if in_modes native,c; then
    # bsx.1 bare a[i] read, literal index
    run_sash 'int array a=(7 13 21); int x=$(( a[1] + 1 )); echo $x'
    [[ $rc = 0 && $out = "14" && -z $err ]]; chk bsx.1
    # bsx.2 bare a[i] read, index is a full arith expr
    run_sash 'int array a=(7 13 21); int i=1; int x=$(( a[i+1] )); echo $x'
    [[ $rc = 0 && $out = "21" && -z $err ]]; chk bsx.2
    # bsx.3 bare a[i]++ lvalue (the DG line, with a declared index)
    run_sash 'int array a=(7 13 21); int i=0; (( a[i]++ )); echo $a[$i]'
    [[ $rc = 0 && $out = "8" && -z $err ]]; chk bsx.3
    # bsx.4 bare a[i] += lvalue
    run_sash 'int array a=(7 13 21); (( a[2] += 5 )); echo $a[2]'
    [[ $rc = 0 && $out = "26" && -z $err ]]; chk bsx.4
    # bsx.5 bare a[i] = lvalue
    run_sash 'int array a=(7 13 21); (( a[0] = 99 )); echo $a[0]'
    [[ $rc = 0 && $out = "99" && -z $err ]]; chk bsx.5
else
    skip_chk bsx.1; skip_chk bsx.2; skip_chk bsx.3; skip_chk bsx.4; skip_chk bsx.5
fi
# bsx.6 bare `m[k]` on a MAP in arith → compile error (string key not
# expressible under arith rules; use `${m[k]}` / `m[k]=`). Literal index so the
# error is the map rejection (SB4030), not a B2 undeclared-index one.
# Mode-independent.
run_sash 'int map m=([k]=5); int x=$(( m[0] + 1 ))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk bsx.6
# bsx.7 bare `m[0]++` mutation on a map → compile error
run_sash 'int map m=([k]=5); (( m[0]++ ))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk bsx.7

# ── arith-bare-subscript-followups edge 1: `$a[i]` (dollar, non-brace) isn't a
# subscript read in arith. The `$(( ))` content is rebuilt token→string→re-lexed
# and can't preserve a `$`/variable index cleanly (it would silently read a[0]),
# so it rejects with a clear hint pointing at the bare `a[i]` / braced `${a[i]}`
# forms. Mode-independent (compile-time). ──
# dsx.1 literal index → SB4035 carrying the `a[i]`/`${a[i]}` hint
run_sash 'int array a=(7 13 21); echo $(( $a[1] ))'
[[ $rc != 0 && -z $out && $err == *SB4035* && $err == *'${a[i]}'* ]]; chk dsx.1
# dsx.2 variable index rejects too (crucially NOT a silent a[0] read)
run_sash 'int array a=(7 13 21); int i=1; echo $(( $a[$i] ))'
[[ $rc != 0 && -z $out && $err == *SB4035* ]]; chk dsx.2

# ── arith-bare-subscript-followups edge 2: a bare nested cell read `a[a[0]]`
# ends in `]]`; the lexer now tracks subscript depth so that `]]` closes two
# subscripts rather than breaking as a `[[ ]]` dispatch delimiter. native,c —
# janet's arith VALUE path doesn't read the embedded subscript
# (janet-arith-subscript-read). ──
if in_modes native,c; then
    # nsx.1 nested rvalue read: a[0]=2 → a[a[0]] = a[2] = 9
    run_sash 'int array a=(2 0 9); echo $(( a[a[0]] ))'
    [[ $rc = 0 && $out = "9" && -z $err ]]; chk nsx.1
    # nsx.2 nested lvalue write: a[0]=1 → a[1] = 5
    run_sash 'int array a=(1 0 9); (( a[a[0]] = 5 )); echo $a[1]'
    [[ $rc = 0 && $out = "5" && -z $err ]]; chk nsx.2
    # nsx.3 triple nesting: a=(1 2 0 5); a[0]=1 → a[1]=2 → a[2]=0
    run_sash 'int array a=(1 2 0 5); echo $(( a[a[a[0]]] ))'
    [[ $rc = 0 && $out = "0" && -z $err ]]; chk nsx.3
else
    skip_chk nsx.1; skip_chk nsx.2; skip_chk nsx.3
fi

# ── B2 (SB5031 extended into arithmetic): a bare name *read* in arith that is
# assigned nowhere is a typo — the same rule as `$x` outside arith. Anything you
# assign (`(( f++ ))`, `for (( i=0;… ))`) is fine. Mode-independent (compile). ──
# b2a.1 undeclared rvalue operand → SB5031
run_sash 'echo $(( y + 1 ))'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk b2a.1
# b2a.2 undeclared subscript index → SB5031 (`i` assigned nowhere)
run_sash 'int array a=(7 13 21); (( a[i]++ ))'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk b2a.2
# b2a.3 a name assigned by the arith itself (`(( f++ )`) is in scope → no
# SB5031. native,c — janet doesn't auto-declare an undeclared arith counter
# (janet-undeclared-arith-counter); the sash compile is correct in all modes.
if in_modes native,c; then
    run_sash '(( f++ )); echo $f'
    [[ $rc = 0 && $out = "1" && -z $err ]]; chk b2a.3
else
    skip_chk b2a.3
fi

# ── DD#77 D1 for string-array cells: a bare string cell / string index in arith
# is the same string→int crossing as a bare string scalar → SB4030, needing a
# disposition (`${a[i]:-N}` / `${a[i]:?msg}`). int-array cells (int) are fine.
# The reject checks are compile-time (mode-independent); the disposition escape
# hatch runs, so it's native,c (janet arith-value subscript gap). ──
# scx.1 bare string-array cell operand → SB4030
run_sash 'array a=(hi 13 21); int x=$(( a[0] + 1 ))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk scx.1
# scx.2 read-modify of a string cell (`a[0]++` reads it) → SB4030
run_sash 'array a=(7 13 21); (( a[0]++ ))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk scx.2
# scx.3 a bare STRING scalar used as an int index → SB4030 (same crossing)
run_sash 'int array a=(7 13 21); s=hi; int x=$(( a[s] ))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk scx.3
if in_modes native,c; then
    # scx.4 escape hatch — extract the cell to a scalar, then dispose: default
    # fires for a non-numeric cell
    run_sash 'array a=(hi 13 21); v=$a[0]; int x=$(( ${v:-99} )); echo $x'
    [[ $rc = 0 && $out = "99" && -z $err ]]; chk scx.4
    # scx.5 extract-to-scalar: a numeric cell passes through
    run_sash 'array a=(hi 13 21); v=$a[1]; int x=$(( ${v:-99} )); echo $x'
    [[ $rc = 0 && $out = "13" && -z $err ]]; chk scx.5
    # scx.6 extract-to-scalar with `:?` aborts on a non-numeric cell
    run_sash 'array a=(hi 13 21); v=$a[0]; int x=$(( ${v:?nan} )); echo $x'
    [[ $rc != 0 && -z $out ]]; chk scx.6
else
    skip_chk scx.4; skip_chk scx.5; skip_chk scx.6
fi
# ── Direct-form cell dispositions (DG: a string cell behaves like a string
# variable). scx.7-11 WORD context, all modes; scx.12-14 ARITH context, the
# DD#77 strict-int gate applies to the cell string (native,c — janet arith-value
# subscript gap); scx.15-16 operator parity. ──
# scx.7 `:-` returns the cell when set
run_sash 'array a=(hi 13 21); echo ${a[0]:-DEF}'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk scx.7
# scx.8 `:-` defaults on an out-of-range cell
run_sash 'array a=(hi 13 21); echo ${a[9]:-DEF}'
[[ $rc = 0 && $out = "DEF" && -z $err ]]; chk scx.8
# scx.9 `:?` aborts on an out-of-range cell (runtime abort → rc≠0, no stdout)
run_sash 'array a=(hi 13 21); echo ${a[9]:?boom}'
[[ $rc != 0 && -z $out ]]; chk scx.9
# scx.10 map cell `:-` hit
run_sash 'map m=([k]=v); echo ${m[k]:-DEF}'
[[ $rc = 0 && $out = "v" && -z $err ]]; chk scx.10
# scx.11 map cell `:-` miss → default
run_sash 'map m=([k]=v); echo ${m[x]:-DEF}'
[[ $rc = 0 && $out = "DEF" && -z $err ]]; chk scx.11
if in_modes native,c; then
    # scx.12 arith: a numeric cell string passes the strict-int gate
    run_sash 'array a=(hi 13 21); echo $(( ${a[1]:-0} + 1 ))'
    [[ $rc = 0 && $out = "14" && -z $err ]]; chk scx.12
    # scx.13 arith: a non-numeric cell string takes the `:-` default (0)
    run_sash 'array a=(hi 13 21); echo $(( ${a[0]:-0} + 1 ))'
    [[ $rc = 0 && $out = "1" && -z $err ]]; chk scx.13
    # scx.14 arith: a non-numeric cell string with `:?` aborts (strict-int)
    run_sash 'array a=(hi 13 21); echo $(( ${a[0]:?nan} + 1 ))'
    [[ $rc != 0 && -z $out ]]; chk scx.14
else
    skip_chk scx.12; skip_chk scx.13; skip_chk scx.14
fi
# scx.15 a pruned modifier on a cell rejects exactly like a scalar (SB4034)
run_sash 'array a=(hi 13 21); echo ${a[0]#h}'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk scx.15
if in_modes native,c; then
    # scx.16 substring on a cell now lowers (edge 4) — a cell behaves like a
    # string variable; ${a[0]:1} of "hi" → "i" (SsaValue::Substring reads the cell)
    run_sash 'array a=(hi 13 21); echo ${a[0]:1}'
    [[ $rc = 0 && $out = "i" && -z $err ]]; chk scx.16
    # scx.17 substring off:len on a MAP cell: ${m[k]:1:3} of "hello" → "ell"
    run_sash 'map m=([k]=hello); echo ${m[k]:1:3}'
    [[ $rc = 0 && $out = "ell" && -z $err ]]; chk scx.17
    # scx.18 sed on a cell: ${a[0]:s/h/H/} of "hi" → "Hi" (desugars to
    # $(sub "${a[0]}" h H))
    run_sash 'array a=(hi 13 21); echo ${a[0]:s/h/H/}'
    [[ $rc = 0 && $out = "Hi" && -z $err ]]; chk scx.18
    # scx.19 gsub on a cell: ${a[0]:s/l/L/g} of "hello" → "heLLo"
    run_sash 'array a=(hello 13 21); echo ${a[0]:s/l/L/g}'
    [[ $rc = 0 && $out = "heLLo" && -z $err ]]; chk scx.19
    # scx.20 sed on a MAP cell
    run_sash 'map m=([k]=hello); echo ${m[k]:s/l/L/g}'
    [[ $rc = 0 && $out = "heLLo" && -z $err ]]; chk scx.20
else
    skip_chk scx.16; skip_chk scx.17; skip_chk scx.18; skip_chk scx.19; skip_chk scx.20
fi
# scx.21 malformed sed on a cell → SB4032 (mode-independent, shared with scalar)
run_sash 'array a=(hi 13 21); echo ${a[0]:s/h}'
[[ $rc != 0 && -z $out && $err == *SB4032* ]]; chk scx.21
# scx.22 a `:`-op with no cell lowering (`:+`) still rejects explicitly → SB4039
run_sash 'array a=(hi 13 21); echo ${a[0]:+x}'
[[ $rc != 0 && -z $out && $err == *SB4039* ]]; chk scx.22

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
# ivc.8 axis stacking: `local int array` inside a function. native+c — emit_c
# now declares the function-local int-collection as a table slot (declare_vars
# "String wins" + the save/restore Var override, 2026-08-20). janet deferred.
if in_modes native,c; then
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
# walk must descend arith so the reject fires. (`:?` is now DD#77 — un-pruned;
# `##` stays pruned.) All arith positions.
run_sash 'echo $(( ${s##x} ))'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk pex.5
# pex.6 same in a condition position (the (( )) ArithTest walk)
run_sash 's=x; if (( ${s:+1} > 0 )); then echo y; fi'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk pex.6
# pex.7 DD#77: an out-of-range input string (numeric-looking but exceeds i64)
# is "not a valid integer", so a `:?` disposition catches it and aborts
# CLEANLY (`<prog>: msg`), not a Rust panic (was "sash: panic", rc 99). Bare
# `$(( s ))` is now a compile error (SB4030, D1) — the disposition is the
# sanctioned string operand. This is the out-of-range companion to dsp.4
# (non-numeric); it exercises parse_strict_i64's range check. native,c —
# message native-only (janet arith disposition gap, janet-arith-subscript-read).
if in_modes native,c; then
    run_sash 's=9223372036854775808; echo $(( ${s:?not a number} + 0 ))'
    [[ $rc != 0 && -z $out ]] \
      && { ! in_modes native || [[ $rerr == *": not a number"* ]]; }; chk pex.7
else
    skip_chk pex.7
fi
# pex.8 DD#77: out-of-range input via `:-N` yields the default (treated as
# not-a-valid-integer), not an overflow trap — the out-of-range companion to
# dsp.1 (non-numeric). native,c.
if in_modes native,c; then
    run_sash 's=9223372036854775808; echo $(( ${s:-0} + 0 ))'
    [[ $rc = 0 && $out = "0" && -z $err ]]; chk pex.8
else
    skip_chk pex.8
fi

# ── DD#77 C2: arith strict-disposition semantics for :- / :? ──
# In arithmetic the trigger is "not a valid integer" (unset/empty/non-numeric/
# out-of-range), gated by sash_is_int: `${s:-N}` → is_int(s)?s:N ; `${s:?M}` →
# is_int(s)?s:abort(M). native,c — janet's arith VALUE path doesn't apply the
# disposition (janet-arith-subscript-read, extended to :- / :?).
if in_modes native,c; then
    # dsp.1 :- default fires on a non-numeric value (was lenient atoi → 0)
    run_sash 's=hello; echo $(( ${s:-9} + 0 ))'
    [[ $rc = 0 && $out = "9" && -z $err ]]; chk dsp.1
    # dsp.2 :- passes a valid int through
    run_sash 's=5; echo $(( ${s:-9} + 0 ))'
    [[ $rc = 0 && $out = "5" && -z $err ]]; chk dsp.2
    # dsp.3 :? passes a valid int through
    run_sash 's=5; echo $(( ${s:?bad} + 0 ))'
    [[ $rc = 0 && $out = "5" && -z $err ]]; chk dsp.3
    # dsp.4 :? aborts on a non-numeric value (message native-only)
    run_sash 's=hello; echo $(( ${s:?not a number} + 0 ))'
    [[ $rc != 0 && -z $out ]] \
      && { ! in_modes native || [[ $rerr == *": not a number"* ]]; }; chk dsp.4
    # dsp.5 the primary conversion idiom: int n=$(( ${s:?M} )) validates + converts
    run_sash 's=42; int n=$(( ${s:?bad} )); echo $n'
    [[ $rc = 0 && $out = "42" && -z $err ]]; chk dsp.5
else
    skip_chk dsp.1; skip_chk dsp.2; skip_chk dsp.3; skip_chk dsp.4; skip_chk dsp.5
fi

# ── DD#77 D4 / arith-disposition-hex-octal-default: the `:-` default in
# arithmetic must be a DD#72 INTEGER LITERAL (decimal or `0x`/`0o`/`0b`).
# A non-literal (interpolation, `base#digits`, leading-zero octal) still rejects
# at compile time (SB4030, mode-independent) — it would silently atoi to 0. The
# literal is lowered as its integer value (native/c convert at the SSA level;
# janet coerces via `_num`). ──
# d4.1 a non-numeric default rejects (was a silent 0)
run_sash 's=x; echo $(( ${s:-hello} + 0 ))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk d4.1
# d4.2 a hex default is now ACCEPTED (was rejected) — compiles in every mode
run_sash 's=x; echo $(( ${s:-0xff} + 0 ))'
[[ $rc = 0 && -z $err ]]; chk d4.2
# d4.3 a decimal (incl. negative) literal default is accepted + lowered. native,c
# (janet's `:-` disposition value semantics differ — janet-arith-subscript-read).
if in_modes native,c; then
    run_sash 's=x; echo $(( ${s:--7} + 0 ))'
    [[ $rc = 0 && $out = "-7" && -z $err ]]; chk d4.3
else
    skip_chk d4.3
fi
# d4.4 hex default lowers to its value (255). native,c (janet disposition gap)
if in_modes native,c; then
    run_sash 's=x; echo $(( ${s:-0xff} + 0 ))'
    [[ $rc = 0 && $out = "255" && -z $err ]]; chk d4.4
else
    skip_chk d4.4
fi
# d4.5 octal default lowers to its value (0o17 = 15). native,c
if in_modes native,c; then
    run_sash 's=x; echo $(( ${s:-0o17} + 0 ))'
    [[ $rc = 0 && $out = "15" && -z $err ]]; chk d4.5
else
    skip_chk d4.5
fi
# d4.6 binary default lowers to its value (0b101 = 5). native,c
if in_modes native,c; then
    run_sash 's=x; echo $(( ${s:-0b101} + 0 ))'
    [[ $rc = 0 && $out = "5" && -z $err ]]; chk d4.6
else
    skip_chk d4.6
fi
# d4.7 leading-zero octal (`010`) and `base#digits` are NOT DD#72 literals → reject
run_sash 's=x; echo $(( ${s:-010} + 0 ))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk d4.7
run_sash 's=x; echo $(( ${s:-16#ff} + 0 ))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk d4.7b


# --- bug #3: a nested / parenthesized arith assignment applies its store ---
# (was silently dropped: `(( (a=2) ))` left a unchanged). Fork A: the transform
# hoists an embedded assignment in an unconditional position so the side effect
# persists and the value is the assigned value — matching bash. Forms it can't
# apply in place (`$(( … ))` value, embedded `++`/`--`, `&&`/`||`/`?:`) are a
# compile error (SB4020), never a silent wrong value.

# 31.25 a paren-wrapped assignment applies (statement (( )))
run_sash 'int a=0; (( (a=2) )); echo $a'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk 31.25

# 31.26 an assignment as a `+` operand applies; value discarded
run_sash 'int a=0; (( 1 + (a=2) )); echo $a'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk 31.26

# 31.27 two operand assignments both apply, left-to-right
run_sash 'int a=0; int b=0; (( (a=2) + (b=3) )); echo "$a-$b"'
[[ $rc = 0 && $out = "2-3" && -z $err ]]; chk 31.27

# 31.28 a comma element that is a paren-wrapped assignment applies
run_sash 'int a=0; int b=0; (( a=2, (b=3) )); echo "$a-$b"'
[[ $rc = 0 && $out = "2-3" && -z $err ]]; chk 31.28

# 31.29 an embedded assignment in an `if (( … ))` CONDITION is rejected — a
# condition is a pure test (SUBSET.md), like `if (( x=5 ))` / `if (( x++ ))`.
# The store belongs in a `(( … ))` statement (31.25-28 above).
run_sash 'int a=0; if (( (a=2) )); then echo yes; fi'
[[ $rc != 0 && -z $out && $err == *SB5007* ]]; chk 31.29

# 31.30 embedded compound-assign applies (value == new, bash-exact)
run_sash 'int a=1; (( (a+=5) )); echo $a'
[[ $rc = 0 && $out = "6" && -z $err ]]; chk 31.30

# 31.31 embedded compound-assign as an operand applies
run_sash 'int a=1; (( 10 + (a+=5) )); echo $a'
[[ $rc = 0 && $out = "6" && -z $err ]]; chk 31.31

# 31.32 an embedded assignment in a `while (( … ))` CONDITION is rejected
# (pure-test rule); mutate in the loop body or use the DD#31 comma form.
run_sash 'int i=0; while (( (i=i+1) <= 3 )); do echo $i; done'
[[ $rc != 0 && -z $out && $err == *SB5007* ]]; chk 31.32

# 31.33 doubly-parenthesized assignment applies
run_sash 'int a=0; (( ((a=7)) )); echo $a'
[[ $rc = 0 && $out = "7" && -z $err ]]; chk 31.33

# 31.34 an assignment inside a `$(( … ))` VALUE can't take effect → SB4020
# (was silent: x=3 but a stayed 0).
run_sash 'int a=0; int x=$(( (a=2) + 1 )); echo "$x $a"'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk 31.34

# 31.35 an embedded `++`/`--` can't take effect (post/pre ordering) → SB4020
run_sash 'int a=0; (( 1 + a++ ))'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk 31.35

# 31.36 an assignment under `?:` (conditionally evaluated) → SB4020
run_sash 'int a=1; int b=0; (( a ? (b=2) : 0 ))'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk 31.36


set -e
tally

