#!/bin/bash
# spec/arithmetic.sh — arithmetic context tests (DD#31)
set -eu
. common.sh
setup arithmetic
plan=330
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
run_sash 'int x=0; (( x = 0 )); : $x; echo $?'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk 31.13

# 31.14 bare (( x > 0 )) as statement is compile error
run_sash 'int x=5; (( x > 0 ))'
[[ $rc != 0 && -z $out && -n $err ]]; chk 31.14

# --- DD#25 overflow ---
# bug-runtime-arith-overflow-wraps: runtime (non-const) overflow must TRAP,
# not silently wrap. 25.2/25.3 are the const path; 25.4-25.6 are the genuine
# runtime path (fn-arg operands, not const-foldable) for *, +, -. 25.7 guards
# the itoa(i64::MIN) fix (echo of MIN must not panic). 25.8 asserts the
# descriptive diagnostic (native).

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

# 25.13-25.17 DD#25 amended (DG 2026-09-20): shifts take an UNBOUNDED,
# NON-NEGATIVE count. The hardware masks the count to 6 bits, which is a
# property of the register width rather than of the arithmetic — it made
# `1 << 64` yield `1` and `8 >> -64` yield `8`, wrong answers that look right,
# the same silent-wrong class DD#25 already rejects for overflow. These
# DIVERGE from bash deliberately; the ordinary-count cases (25.16) do not.
# 25.13 a count >= 64 shifts every bit out: `<<` is 0, not a masked re-shift
run_sash 'int x=1; echo $(( x << 63 )); echo $(( x << 64 )); echo $(( x << 200 ))'
[[ $rc = 0 && $out = "-9223372036854775808
0
0" && -z $err ]]; chk 25.13
# 25.14 `>>` fills with the SIGN, so a count >= 64 saturates rather than
# zeroing: 0 for a non-negative value, -1 for a negative one
run_sash 'int a=8; int b=-1; echo $(( a >> 64 )); echo $(( b >> 64 )); echo $(( b >> 200 ))'
[[ $rc = 0 && $out = "0
-1
-1" && -z $err ]]; chk 25.14
# 25.15 a NEGATIVE count aborts. Under masking `8 >> -64` was `8` — an
# identity — and `8 >> -1` was 0; both are meaningless in any model. The count
# here is a VARIABLE, so the folder cannot prove it and this is the runtime
# trap; a count it can prove is SB5084 at compile time instead (25.18-25.21).
run_sash 'int x=8; int n=-1; (( c = x >> n )); echo $c'
[[ $rc = 2 && -z $out && $rerr == *"negative shift count"* ]]; chk 25.15
# 25.16 ordinary counts are UNCHANGED and still match bash, including the
# sign fill on `>>`: -8>>1 is -4 (= floor(-8/2)), not a huge positive number.
# A zero fill here would be the MORE representation-dependent choice on a
# signed type, so `>>` deliberately keeps filling with the sign.
run_sash 'int a=1; int b=-8; int c=-9; echo $(( a << 3 )); echo $(( b >> 1 )); echo $(( c >> 1 ))'
[[ $rc = 0 && $out = "8
-4
-5" && -z $err ]]; chk 25.16
# 25.17 the constant folder agrees with the runtime. A literal `1 << 64` must
# fold to the same 0 the variable path computes.
run_sash 'echo $(( 1 << 64 )); echo $(( -1 >> 70 ))'
[[ $rc = 0 && $out = "0
-1" && -z $err ]]; chk 25.17
# 25.18-25.21 DD#25 amended (DG 2026-09-21): a negative shift count the FOLDER
# can PROVE is a compile-time error (SB5084), not a runtime abort — exactly the
# SB5077 rule for a proven zero divisor, and for the same reason: the compiler
# already knows the program cannot run, so it says so before emitting a binary.
# 25.18 a literal negative count, both directions
run_sash 'echo $(( 8 >> -1 ))'
[[ $rc != 0 && -z $out && $err == *SB5084* ]]; chk 25.18
run_sash 'echo $(( 8 << -1 ))'
[[ $rc != 0 && -z $out && $err == *SB5084* ]]; chk 25.19
# 25.20 reach is what the folder proves: operators over literals, through
# parens — and a COMPOUND assignment, which the parser desugars to an ordinary
# BinOp (`x <<= -1` becomes `x = x << -1`), so it needs no rule of its own.
run_sash 'int x=8; (( x >>= (1 - 2) )); echo $x'
[[ $rc != 0 && -z $out && $err == *SB5084* ]]; chk 25.20
# 25.21 the fault is a property of the expression AS WRITTEN, not reachability:
# a proven negative count in a ternary branch that is never taken is still an
# error. This is inherited from SB5077 (same walk), not chosen separately --
# `(( x ? 1 : 5 / 0 ))` has always been rejected the same way.
run_sash 'int x=1; echo $(( x ? 1 : 8 >> -1 ))'
[[ $rc != 0 && -z $out && $err == *SB5084* ]]; chk 25.21

# 25.8 overflow prints `<argv0>: arithmetic overflow` to stderr, which the
# harness now captures as $rerr.
run_sash 'fn mul { int a=$(($1)); int b=$(($2)); (( c = a * b )); echo $c; }
mul 3037000500 3037000500'
[[ $rc != 0 && $rerr == *"arithmetic overflow"* ]]; chk 25.8

# --- DD#31 comparison operators (regression: <= / >= were mis-lexed) ---

# 31.15 <= is true when equal (the bash lexer splits `<=` into `<`+`=`;
# collect_arith_content must keep them adjacent so the arith lexer recombines)
run_sash 'int x=5; if (( x <= 5 )); then echo y; else echo n; fi'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 31.15

# 31.16 <= is true when strictly less
run_sash 'if (( 3 <= 7 )); then echo y; else echo n; fi'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 31.16

# 31.17 <= is false when greater. 31.18 >= mixed. The false case exercises
# the else/skip branch.
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
# cases are front-end; the i64::MAX/MIN value cases check runtime lowering.
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

# 26.8-26.10 i64::MAX / i64::MIN magnitudes
# 26.8 decimal i64::MAX is in range
run_sash 'echo $(( 9223372036854775807 ))'
[[ $rc = 0 && $out = "9223372036854775807" && -z $err ]]; chk 26.8

# 26.9 arith i64::MIN via unary-neg peephole (was: silent 0)
run_sash 'echo $(( -9223372036854775808 ))'
[[ $rc = 0 && $out = "-9223372036854775808" && -z $err ]]; chk 26.9

# 26.10 hex sign-bit pattern == i64::MIN (bash parity)
run_sash 'echo $(( 0x8000000000000000 ))'
[[ $rc = 0 && $out = "-9223372036854775808" && -z $err ]]; chk 26.10

# --- DD#58/#75: int-returning intrinsics evaluate in $(( )) sexpr calls ---
# DG demo: `$(( (len a) + 1 ))` must yield count+1 (was len→0 → 1).
# 58.10 (len a) in arith yields the element count
run_sash 'array a=(x y z); int n=$(( (len a) + 1 )); echo $n'
[[ $rc = 0 && $out = "4" && -z $err ]]; chk 58.10
# 58.11 bare (len a) in arith (no +1)
run_sash 'array a=(a b c d); echo $(( (len a) ))'
[[ $rc = 0 && $out = "4" && -z $err ]]; chk 58.11
# 58.12 byte length in arith is `(strlen $s)`. This used to read `(len s)`,
# pinning the polymorphic `len` that DD#75 stage 2 retired — the command form
# `$(len s)` had reported SB4030 since then, but the sexpr operand slot was
# never type-checked, so the two spellings disagreed. See 58.14-58.16.
run_sash 's=hello; int n=$(( (strlen $s) * 2 )); echo $n'
[[ $rc = 0 && $out = "10" && -z $err ]]; chk 58.12
# 58.13 (len m) on a map in arith → entry count
run_sash 'map m=([a]=1 [b]=2 [c]=3); echo $(( (len m) ))'
[[ $rc = 0 && $out = "3" && -z $err ]]; chk 58.13
# DD#75 §4 (amended): the `${#x}` braced-length sugar is retired; the
# canonical arithmetic length is the `(len x)` sexpr (58.11-58.13 above).
# 58.14-58.16 were byte-identical restatements of 58.10/58.12/58.13. Repurposed
# (DG 2026-09-21) to pin the rules the sexpr operand slot did not enforce: the
# transform reconstructs a `Command` for `(len …)` and reuses the ordinary
# intrinsic builder, but that runs AFTER type_check, so every operand rule the
# `$(len …)` form applies was absent inside `$(( … ))`.
# 58.14 `(len <scalar>)` is SB4030, exactly as `$(len s)` is (DD#75 stage 2).
# It used to return the byte length — the polymorphic `len` that stage 2 retired.
run_sash 'str s=hello; int n=$(( (len s) * 2 )); echo $n'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 58.14
# 58.15 `(strlen $<collection>)` is SB5064. It used to answer **0** silently —
# verbatim the defect the len/strlen split was made to remove.
run_sash 'array a=(x y z); echo $(( (strlen $a) ))'
[[ $rc != 0 && -z $out && $err == *SB5064* ]]; chk 58.15
# 58.16 `(len <int>)` was an ICE ("expected string var, got int") in the native
# backend and uncompilable C in the other — a crash, not a diagnostic.
run_sash 'int n=42; echo $(( (len n) ))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 58.16

# DD#24/DD#31 — the clean special vars ($$ pid, $? status, $# argc) are valid
# `$(( ))` operands, including inside a *compound* expression (not just
# standalone). Native regression: `$$`/`$?` read 0 as a compound-arith operand
# because the int-lowering path missed the getpid / exit-status specials that
# the string path already handles ($# worked — it is a materialized slot).
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

# ── DD#58/Option C — sash-native intrinsics in an arithmetic sexpr $(( (f) )) ──
# An int producer (user `-> int` fn OR int intrinsic) works in BOTH $(f) capture
# and the $(( (f) )) sexpr. Previously ONLY `len` worked in the sexpr; every
# other intrinsic silently evaluated to 0. See bug-arith-sexpr-intrinsics-zero.

# Category A — int intrinsics compute (not silent 0).
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
# sx.24 getcpid in a sexpr — it was the one name left out of the DD#58 fix,
# and because `validate` exempts every intrinsic name from the SB5031 typo
# check the omission did not read as "unknown name": it fell through to the
# variable branch, read an unset slot, and was a silent 0.
run_sash '/bin/sleep 0 & echo $(( (getcpid) > 0 ))'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk sx.24
# sx.25 …and the sexpr agrees with the `$!` punctuation sugar, which always
# worked here — the two spellings disagreeing is what made it silent
run_sash '/bin/sleep 0 & int a=$(( (getcpid) )); int b=$(( $! )); echo $(( a == b ))'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk sx.25
# sx.7 signum — read the sash_signum global.
# signum with no signal received → 0 (pins it computes in a sexpr)
run_sash 'echo $(( (signum) ))'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk sx.7

# Grouping — `(x)` on a plain variable is arithmetic grouping, not a nullary
# call. The arith parser emits FuncCall for every `(name)` and defers the
# decision; a name that is neither an intrinsic nor a known function is a
# variable. Regression: `$(( (x) ))` silently returned 0.
# sx.9 grouped int variable
run_sash 'int x=5; echo $(( (x) ))'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk sx.9
# sx.10 grouped variable composes with an operator
run_sash 'int x=5; echo $(( (x) + 1 ))'
[[ $rc = 0 && $out = "6" && -z $err ]]; chk sx.10
# sx.11 Q3: random is int-typed — int n=$(random) captures (was SB4030)
run_sash 'int n=$(random); : $n; echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk sx.11

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

# ── an unrecognized symbol in arithmetic context is an error (DG 2026-09-13) ──
# The only exception is an assignment target: `(( x = 5 ))` introduces x. Every
# read spelling of an undefined name already reported SB5031 — `$(( zz ))`,
# `$zz`, `${zz}`, `zz[0]`, `a[zz]` — but the DD#58 sexpr node `(name …)` was a
# hole: validate blanket-skipped the name (it *might* be an intrinsic) while the
# transform resolved it as a variable read or an empty int-func, both yielding a
# silent 0. Found 2026-09-13 while retiring dtoi/itod, which had been masking the
# with-args half behind SB4030.
#
# The rule, applied at that one node:
#   nullary, not an intrinsic   → a variable read → SB5031 if undefined
#   with args, not an intrinsic → not callable at all → SB5011
# sx.17 an undefined name, parenthesised, is still an undefined read
run_sash 'int n=$(( (zz) )); echo $n'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk sx.17
# sx.18 ... and as a bare `(( ))` statement, not only in `$(( ))`
run_sash '(( (zz) )); echo done'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk sx.18
# sx.19 arguments make it unambiguously a call — an unknown callee is SB5011
run_sash 'int n=$(( (nosuchfn 1) )); echo $n'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk sx.19
# sx.20 a *defined* variable is still not callable
run_sash 'int zz=4
int n=$(( (zz 1) )); echo $n'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk sx.20
# sx.21 no false positive: `(x)` on a defined variable is arithmetic grouping
run_sash 'int x=7
int n=$(( (x) + 1 )); echo $n'
[[ $rc = 0 && $out = 8 && -z $err ]]; chk sx.21
# sx.22 no false positive: a nullary user int function is a legal sexpr callee
run_sash 'fn g -> int { return 7; }
int n=$(( (g) )); echo $n'
[[ $rc = 0 && $out = 7 && -z $err ]]; chk sx.22
# sx.23 retired dtoi/itod take the ordinary unknown-callee path, not SB4030
run_sash 'echo $(( (itod 2 5) ))'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk sx.23

# ── sexpr intrinsic operands and arity (silent 0 before) ──
# sx.26 a cell operand: SB4035 recommended `${a[i]}` and SB4020 then rejected
# it inside `(strlen …)`, so this spelling had nowhere to go.
run_sash 'array a=(10 200 3)
echo $(( (strlen ${a[1]}) ))'
[[ $rc = 0 && $out = 3 && -z $err ]]; chk sx.26
# sx.27 redundant grouping around an operand
run_sash 's=abcde
echo $(( (strlen ($s)) ))'
[[ $rc = 0 && $out = 5 && -z $err ]]; chk sx.27
# sx.28 `(len a)` on a cell-bearing array still counts elements
run_sash 'array a=(10 200 3)
echo $(( (len a) + (strlen ${a[1]}) ))'
[[ $rc = 0 && $out = 6 && -z $err ]]; chk sx.28
# sx.29 wrong arity is SB4045 — the same code `$(strlen a b)` reports, not a
# different one for the sexpr spelling
run_sash 's=abcde
echo $(( (strlen $s $s) ))'
[[ $rc != 0 && -z $out && $err == *SB4045* ]]; chk sx.29
# sx.30 …and zero args
run_sash 'echo $(( (strlen) ))'
[[ $rc != 0 && -z $out && $err == *SB4045* ]]; chk sx.30
# sx.31 an operand the transform cannot rebuild is rejected, not silently 0
run_sash 'echo $(( (strlen (getpid)) ))'
[[ $rc != 0 && -z $out && $err == *SB4045* ]]; chk sx.31
# sx.32-sx.35 the remaining sexpr operand rules, each the exact answer the
# `$(…)` command spelling gives. Before these the slot was unchecked and the
# two spellings disagreed on every row.
# sx.32 `(len $<collection>)` — the collection read through the scalar sigil
run_sash 'array a=(p q r); echo $(( (len $a) ))'
[[ $rc != 0 && -z $out && $err == *SB5064* ]]; chk sx.32
# sx.33 `(strlen $<int>)` — measuring an int's decimal rendering
run_sash 'int n=42; echo $(( (strlen $n) ))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sx.33
# sx.34 the rule is not limited to intrinsics: a USER int function's sexpr
# argument is an operand slot too, and `(f $a)` ran the call rather than
# reporting what `$(f $a)` reports.
run_sash 'array a=(1 2 3)
fn f -> int { return 7; }
echo $(( (f $a) ))'
[[ $rc != 0 && -z $out && $err == *SB5064* ]]; chk sx.34
# sx.35 the valid spellings are untouched: a cell operand and a `$`-scalar
run_sash 'array a=(10 200 3)
str s=abcde
echo $(( (strlen ${a[1]}) + (strlen $s) + (len a) ))'
[[ $rc = 0 && $out = 11 && -z $err ]]; chk sx.35

# ── array/map subscripts as an rvalue in arithmetic (`${…}` value-import) ──
# BUG: `${a[i]}` in `(( ))` read a bogus variable named "a[i]" → 0. `${…}` is a
# normal-parse rvalue value-import into arith; fixes the silent-0.
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
# asx.5 subscript rvalue in a condition
run_sash 'array a=(10 20 30); if (( ${a[0]} > 5 )); then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk asx.5

# ── arith-subscript model B Stage 2: BARE `a[i]` (int-array cell, rvalue +
# lvalue). ──
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
# bsx.8 bare `args[i]` (the implicit positional array) in arith inside a
# function — resolves to the per-inline __argsN slot like the word form
# ${args[i]} (arith-args-subscript-codegen; was an SB5031 false positive
# masking a codegen ICE).
run_sash 'f() { echo $(( args[0] + 1 )); }; f 5'
[[ $rc = 0 && $out = "6" && -z $err ]]; chk bsx.8
# bsx.9 contiguous multi-arg read `args[0]`+`args[1]`
run_sash 'f() { echo $(( args[0] + args[1] )); }; f 3 4'
[[ $rc = 0 && $out = "7" && -z $err ]]; chk bsx.9
# bsx.6 bare `m[k]` on a MAP in arith → compile error (string key not
# expressible under arith rules; use `${m[k]}` / `m[k]=`). Literal index so the
# error is the map rejection (SB4030), not a B2 undeclared-index one.
# Mode-independent.
run_sash 'int map m=([k]=5); int x=$(( m[0] + 1 ))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk bsx.6
# bsx.7 bare `m[0]++` mutation on a map → compile error
run_sash 'int map m=([k]=5); (( m[0]++ ))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk bsx.7
# bsx.10 bare `args[i]` in arith at TOP LEVEL (not in a function) stays rejected
# — positionals are function-only (SB5031). Mode-independent.
run_sash 'echo $(( args[0] ))'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk bsx.10

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
# subscripts rather than breaking as a `[[ ]]` dispatch delimiter. ──
# nsx.1 nested rvalue read: a[0]=2 → a[a[0]] = a[2] = 9
run_sash 'int array a=(2 0 9); echo $(( a[a[0]] ))'
[[ $rc = 0 && $out = "9" && -z $err ]]; chk nsx.1
# nsx.2 nested lvalue write: a[0]=1 → a[1] = 5
run_sash 'int array a=(1 0 9); (( a[a[0]] = 5 )); echo $a[1]'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk nsx.2
# nsx.3 triple nesting: a=(1 2 0 5); a[0]=1 → a[1]=2 → a[2]=0
run_sash 'int array a=(1 2 0 5); echo $(( a[a[a[0]]] ))'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk nsx.3

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
# SB5031. The sash compile is correct in all modes.
run_sash '(( f++ )); echo $f'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk b2a.3

# ── DD#77 D1 for string-array cells: a bare string cell / string index in arith
# is the same string→int crossing as a bare string scalar → SB4030, needing a
# disposition (`${a[i]:-N}` / `${a[i]:?msg}`). int-array cells (int) are fine.
# The reject checks are compile-time; the disposition escape hatch runs. ──
# scx.1 bare string-array cell operand → SB4030
run_sash 'array a=(hi 13 21); int x=$(( a[0] + 1 ))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk scx.1
# scx.2 read-modify of a string cell (`a[0]++` reads it) → SB4030
run_sash 'array a=(7 13 21); (( a[0]++ ))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk scx.2
# scx.3 a bare STRING scalar used as an int index → SB4030 (same crossing)
run_sash 'int array a=(7 13 21); s=hi; int x=$(( a[s] ))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk scx.3
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
# ── Direct-form cell dispositions (DG: a string cell behaves like a string
# variable). scx.7-11 WORD context; scx.12-14 ARITH context, the DD#77
# strict-int gate applies to the cell string; scx.15-16 operator parity. ──
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
# scx.12 arith: a numeric cell string passes the strict-int gate
run_sash 'array a=(hi 13 21); echo $(( ${a[1]:-0} + 1 ))'
[[ $rc = 0 && $out = "14" && -z $err ]]; chk scx.12
# scx.13 arith: a non-numeric cell string takes the `:-` default (0)
run_sash 'array a=(hi 13 21); echo $(( ${a[0]:-0} + 1 ))'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk scx.13
# scx.14 arith: a non-numeric cell string with `:?` aborts (strict-int)
run_sash 'array a=(hi 13 21); echo $(( ${a[0]:?nan} + 1 ))'
[[ $rc != 0 && -z $out ]]; chk scx.14
# scx.15 a pruned modifier on a cell rejects exactly like a scalar (SB4034)
run_sash 'array a=(hi 13 21); echo ${a[0]#h}'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk scx.15
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
# scx.21 malformed sed on a cell → SB4032 (mode-independent, shared with scalar)
run_sash 'array a=(hi 13 21); echo ${a[0]:s/h}'
[[ $rc != 0 && -z $out && $err == *SB4032* ]]; chk scx.21
# scx.22 a `:`-op with no cell lowering (`:+`) still rejects explicitly → SB4039
run_sash 'array a=(hi 13 21); echo ${a[0]:+x}'
[[ $rc != 0 && -z $out && $err == *SB4039* ]]; chk scx.22

# ── DD#76 int-valued collections (`int array` / `int map`) ──
# An int collection stores each element as an int cell (val_len double-duty);
# `${a[i]}` types Int (feeds `int x=${a[i]}` and arithmetic with no atoi),
# renders as decimal in string context. Compile-rejects (ivc.9, ivc.10) are
# backend-independent and run in every mode.
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
# ivc.8 axis stacking: `int array` inside a function (DD#86: `local` retired).
run_sash 'fn f { int array a=(7 8); echo ${a[1]}; }; f'
[[ $rc = 0 && $out = "8" && -z $err ]]; chk ivc.8
# ivc.9 a non-numeric element in an int collection → SB4030 (compile-reject)
run_sash 'int array a=(hi 2); echo ${a[0]}'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk ivc.9
# ivc.10 bare `a[i]=99` (numeric literal) on an int collection stores the int
# cell — DD#77/DD#30-refine: a bare numeric literal to a known-int LHS aligns
# with `int x=5` (see types.sh 30.5/30.8).
run_sash 'int array a=(10 20 30); a[1]=99; echo ${a[1]}'
[[ $rc = 0 && $out = "99" && -z $err ]]; chk ivc.10

# ivc.12-17 int cells reaching a POSITIONAL (generic) cell read. DD#76 stores
# an int cell as {val_ptr: null, val_len: <the i64>}; keyed reads (`${a[0]}`,
# arithmetic) go through sash_table_get where codegen knows the static type and
# itoa's. Splat / join / for-in go through sash_table_value, which has no type
# to consult and handed the null pointer straight out — `for x in @a` faulted
# (EFAULT) and `$(join a)` SEGFAULTED, with no compile diagnostic. The runtime
# now renders a null cell as itoa(val_len) at that one accessor.
# ivc.12 splat an int array into a for-in
run_sash 'int array a=(1 2); for x in @a; do echo $x; done'
[[ $rc = 0 && $out = "1
2" && -z $err ]]; chk ivc.12
# ivc.13 join an int array
run_sash 'int array a=(1 2); echo $(join a)'
[[ $rc = 0 && $out = "1 2" && -z $err ]]; chk ivc.13
# ivc.14 the same two for an int MAP (values are the int cells)
run_sash 'int map m=([k]=5); for x in @m; do echo $x; done'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk ivc.14
# ivc.15 join an int map
run_sash 'int map m=([k]=5); echo $(join m)'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk ivc.15
# ivc.16 an int cell holding ZERO — val_len 0 is a real value here, not the
# "absent" reading the same accessor uses for an out-of-range index.
run_sash 'int array a=(0 5); echo $(join a)'
[[ $rc = 0 && $out = "0 5" && -z $err ]]; chk ivc.16
# ivc.17 an EMPTY STRING element is ("", 0) — a real pointer — so it must stay
# empty and not render as "0". This is what makes null an unambiguous
# int-cell discriminator.
run_sash 'array b=("" q); echo "[$(join b)]"'
[[ $rc = 0 && $out = "[ q]" && -z $err ]]; chk ivc.17

# ivc.18-22 `a[i] += n` on an int collection. The read-modify-write arm that
# builds these (`IntCell(ArithBinOp{current, Add, rhs})`) had no spec at all:
# every ivc case above uses plain `=`. It is an ARITHMETIC add for an int cell
# and a string concat otherwise, so the two must be pinned together or the
# wrong one is silently correct-looking on single-digit values (1+2 vs "12").
# ivc.18 int array element +=
run_sash 'int array a=(10 20 30); a[1]+=5; echo ${a[1]}'
[[ $rc = 0 && $out = "25" && -z $err ]]; chk ivc.18
# ivc.19 int map element +=
run_sash 'int map m=([k]=5); m[k]+=3; echo ${m[k]}'
[[ $rc = 0 && $out = "8" && -z $err ]]; chk ivc.19
# ivc.20 CONTRAST: a non-int collection concatenates, it does not add
run_sash 'array a=(1 2); a[0]+=3; echo ${a[0]}'
[[ $rc = 0 && $out = "13" && -z $err ]]; chk ivc.20
# ivc.21 the += RHS may read another int cell
run_sash 'int array a=(10 20); a[0]+=${a[1]}; echo ${a[0]}'
[[ $rc = 0 && $out = "30" && -z $err ]]; chk ivc.21
# ivc.22 a negative addend subtracts (the arith path, not a concat of "-3")
run_sash 'int array a=(10); a[0]+=-3; echo ${a[0]}'
[[ $rc = 0 && $out = "7" && -z $err ]]; chk ivc.22

# ── DD#77 A: ${var:OP…} routed through the real param-expansion parser ──
# BUG (bug-arith-param-expansion-operator-zero): `${s:-N}`/`${s:?…}`/etc inside
# `(( ))`/`$(( ))` swallowed the operator into a bogus var name → silent 0. Now
# the body goes through parse_parameter_expansion. These assert the fix with
# stable cases (a valid-int read + an unset default — unchanged by DD#77 C).
# pex.1 ${s:-N} reads a set valid-int operand (was silently 0)
run_sash 's=5; echo $(( ${s:-9} ))'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk pex.1
# pex.2 ${s:-N} default fires on unset
run_sash 'echo $(( ${s:-9} ))'
[[ $rc = 0 && $out = "9" && -z $err ]]; chk pex.2
# pex.3 composes with an operator (was 1 via bogus-var 0)
run_sash 's=5; echo $(( ${s:-9} + 1 ))'
[[ $rc = 0 && $out = "6" && -z $err ]]; chk pex.3
# pex.4 in a (( )) condition (was reading 0 → wrong branch)
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
# (non-numeric); it exercises parse_strict_i64's range check. The message is
# checked only in native mode because the c harness does not expose $rerr here.
run_sash 's=9223372036854775808; echo $(( ${s:?not a number} + 0 ))'
[[ $rc != 0 && -z $out ]] \
  && { ! in_modes native || [[ $rerr == *": not a number"* ]]; }; chk pex.7
# pex.8 DD#77: out-of-range input via `:-N` yields the default (treated as
# not-a-valid-integer), not an overflow trap — the out-of-range companion to
# dsp.1 (non-numeric).
run_sash 's=9223372036854775808; echo $(( ${s:-0} + 0 ))'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk pex.8

# ── DD#77 C2: arith strict-disposition semantics for :- / :? ──
# In arithmetic the trigger is "not a valid integer" (unset/empty/non-numeric/
# out-of-range), gated by sash_is_int: `${s:-N}` → is_int(s)?s:N ; `${s:?M}` →
# is_int(s)?s:abort(M).
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

# ── DD#77 D4 / arith-disposition-hex-octal-default: the `:-` default in
# arithmetic must be a DD#72 INTEGER LITERAL (decimal or `0x`/`0o`/`0b`).
# A non-literal (interpolation, `base#digits`, leading-zero octal) still rejects
# at compile time (SB4030, mode-independent) — it would silently atoi to 0. The
# literal is lowered as its integer value. ──
# d4.1 a non-numeric default rejects (was a silent 0)
run_sash 's=x; echo $(( ${s:-hello} + 0 ))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk d4.1
# d4.2 a hex default is now ACCEPTED (was rejected) — compiles in every mode
run_sash 's=x; echo $(( ${s:-0xff} + 0 ))'
[[ $rc = 0 && -z $err ]]; chk d4.2
# d4.3 a decimal (incl. negative) literal default is accepted + lowered.
run_sash 's=x; echo $(( ${s:--7} + 0 ))'
[[ $rc = 0 && $out = "-7" && -z $err ]]; chk d4.3
# d4.4 hex default lowers to its value (255).
run_sash 's=x; echo $(( ${s:-0xff} + 0 ))'
[[ $rc = 0 && $out = "255" && -z $err ]]; chk d4.4
# d4.5 octal default lowers to its value (0o17 = 15).
run_sash 's=x; echo $(( ${s:-0o17} + 0 ))'
[[ $rc = 0 && $out = "15" && -z $err ]]; chk d4.5
# d4.6 binary default lowers to its value (0b101 = 5).
run_sash 's=x; echo $(( ${s:-0b101} + 0 ))'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk d4.6
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

# ── DD#94: newlines / `;` / comments inside `(( ))` ──
# Package A (DG 2026-09-07): a standalone `(( ))` is a sequence block — newline
# and `;` sequence exactly like `,` (value = last). A newline is a separator
# only after a COMPLETE sub-expression; with an operator pending it stays a
# continuation. Whitespace-preceded `#…EOL` is a comment. Deliberate divergence
# from bash, which rejects all three.
# 94.1 newline sequences like a comma
run_sash 'int a; int b
(( a = 1
   b = 2 ))
echo "$a$b"'
[[ $rc = 0 && $out = "12" && -z $err ]]; chk 94.1
# 94.2 …and a later line may use a name bound on an earlier one
run_sash 'int a; int b; int c
(( a = 2
   b = a * 3
   c = b + 1 ))
echo "$a $b $c"'
[[ $rc = 0 && $out = "2 6 7" && -z $err ]]; chk 94.2
# 94.3 the continuation rule is preserved: a pending binary operator means the
# newline continues the expression rather than separating (this is 3, not 1).
run_sash 'x=$(( 1 +
2 ))
echo $x'
[[ $rc = 0 && $out = "3" && -z $err ]]; chk 94.3
# 94.4 `;` sequences too, equal to `,`
run_sash 'int a; int b
(( a = 1; b = 2 ))
echo "$a$b"'
[[ $rc = 0 && $out = "12" && -z $err ]]; chk 94.4
# 94.5 mixed separators in one block
run_sash 'int a; int b; int c
(( a = 1, b = 2; c = 3 ))
echo "$a$b$c"'
[[ $rc = 0 && $out = "123" && -z $err ]]; chk 94.5
# 94.6 leading/trailing separators are harmless
run_sash 'int a
((
   a = 5
))
echo $a'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk 94.6
# 94.7 an end-of-line comment inside a spanned `(( ))`
run_sash 'int a; int b
(( a = 1   # first
   b = a + 1  # second
))
echo "$a$b"'
[[ $rc = 0 && $out = "12" && -z $err ]]; chk 94.7
# 94.8 …and inside a `$(( ))` value
run_sash 'x=$(( 1 + 2   # sum
))
echo $x'
[[ $rc = 0 && $out = "3" && -z $err ]]; chk 94.8
# 94.9 a `#` immediately after an alnum is STILL the DD#72 base-literal reject —
# `16#ff` must stay a loud error, never a silently truncated 16.
run_sash 'echo $(( 16#ff ))'
[[ $rc != 0 && -z $out && $err == *SB4035* ]]; chk 94.9
# 94.10 the C-style for-header is unaffected: `;` still delimits its three
# slots, and `,` sequences within a slot.
run_sash 'for (( i = 0, j = 10; i < 3; i++ )); do echo $i$j; done'
[[ $rc = 0 && $out = "010
110
210" && -z $err ]]; chk 94.10

# ── 31.40–31.52 malformed arithmetic is an error, never a silent value ──
#
# Two defects, one family. The Pratt parser's operand position recovered
# silently to `0` (`parse_prefix_atom`), and the double-quoted-string path
# then threw away whatever diagnostic did survive
# (`emit_string_part` ... `.unwrap_or(ArithExpr::Literal(0))`). Together they
# made `"$(( 1 + ))"` print 1 and `"$(( 1 +* 2 ))"` print 0 — wrong answers
# from input bash rejects outright. The identical expression OUTSIDE quotes
# was already a hard error, so the two positions disagreed.

# 31.40 a dangling binary operator → SB4020, not the LHS
run_sash 'int x=$(( 1 + )); echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk 31.40

# 31.41 ... and identically inside a double-quoted string (used to print 1)
run_sash 'echo "$(( 1 + ))"'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk 31.41

# 31.42 an operator where an operand is owed → SB4020 (used to print 0)
run_sash 'echo "$(( 1 +* 2 ))"'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk 31.42

# 31.43 the diagnostic names the offending token rather than being generic
run_sash 'echo "$(( 1 +* 2 ))"'
[[ $rc != 0 && $err == *"operand"* ]]; chk 31.43

# 31.44 a garbage operand in a string → SB4035, the same code as unquoted
run_sash 'echo "$(( @@@ ))"'
[[ $rc != 0 && -z $out && $err == *SB4035* ]]; chk 31.44

# 31.45 quoted and unquoted positions agree on the diagnostic code
run_sash 'int x=$(( @@@ )); echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB4035* ]]; chk 31.45

# 31.46 `$( )` inside `$(( ))` is rejected in a string too. Bash evaluates a
# nested command substitution here; sash rejects it in every other position,
# and this one used to be a silent 0 instead of joining them.
run_sash 'echo "$(( $(getpid) > 0 ))"'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk 31.46

# 31.47 a malformed arith inside a `${var:-WORD}` default word (the default is
# a full interpolating word, so it reaches the same parser)
run_sash 'x=abc; echo "${x:-$(( 1 + ))}"'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk 31.47

# 31.48 ... and inside a `${var:s/RE/SUB/}` replacement word
run_sash 'x=abcd; echo "${x:s/b/$(( 1 + ))/}"'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk 31.48

# ── the gate the above must not break: valid arithmetic still compiles ──

# 31.49 a well-formed expansion in a string is unaffected
run_sash 'echo "$(( 1 + 2 ))"'
[[ $rc = 0 && $out = "3" && -z $err ]]; chk 31.49

# 31.50 an empty `$(( ))` is 0, as in bash — the empty-expression early return
# is what lets an EOF in *operand* position be an error
run_sash 'echo "$(( ))"'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk 31.50

# 31.51 `for (( ;; ))` reaches the arith parser as a run of bare separators;
# it must stay a legitimately empty expression, not an owed operand
run_sash 'for (( ;; )); do echo hi; break; done'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 31.51

# 31.52 the other empty C-style `for` segments keep working
run_sash 'int i=0; for (( ; i<2; i++ )); do echo "$i"; done'
[[ $rc = 0 && $out = "0
1" && -z $err ]]; chk 31.52

# ── DD#31 comma condition: all four condition contexts ─────────
# "The comma in `(( ))` is syntactic sugar that splits one `(( ))` into
# multiple statements. Each non-final expression must be a mutation. In
# condition position the final expression must be a comparison."
#
# The C-style `for` header's test slot is a condition position like any
# other. It used to be the one of four that refused the form, because
# `lower_c_style_for` called `lower_arith_test_structured` directly and so
# skipped the `Comma` → `Group` decomposition — the mutation stayed embedded
# in one `ArithTest` and `check_arith_test_placement` rejected it (SB4020).
# Bash compiles all four. Pin the whole matrix, not just the one that broke.

# 31.53 `while` — the spelling DD#31 gives as its example
run_sash 'int k=0; while (( k++, k<3 )); do echo "$k"; done'
[[ $rc = 0 && $out = "1
2" && -z $err ]]; chk 31.53

# 31.54 `if`
run_sash 'int k=0; if (( k++, k<3 )); then echo "$k"; fi'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk 31.54

# 31.55 `until`
run_sash 'int k=0; until (( k++, k>2 )); do echo "$k"; done'
[[ $rc = 0 && $out = "1
2" && -z $err ]]; chk 31.55

# 31.56 the C-style `for` header test slot — the divergence
run_sash 'int k=0; for (( ; k++, k<3; )); do echo "$k"; done'
[[ $rc = 0 && $out = "1
2" && -z $err ]]; chk 31.56

# 31.57 the mutation must not be final — a comma condition ending in one is
# still SB5007, now via the same shared check `while` has always used
run_sash 'int k=0; for (( ; k<3, k++; )); do echo "$k"; done'
[[ $rc != 0 && $err == *SB5007* ]]; chk 31.57

# 31.58 a comma condition with no mutation anywhere is still SB5007 — this is
# the half of the DD#31 comma rule that no other path enforces
run_sash 'int k=0; for (( ; k<3, k>0; k++ )); do echo "$k"; done'
[[ $rc != 0 && $err == *"must be a mutation"* ]]; chk 31.58

# 31.59-31.63 pin the TEXT of the SB5007 briefs, not just the code. Found by
# coverage: `arith_brief`'s `==` / `!=` / `>=` operator arms were cold while
# `>` / `<` / `<=` beside them were hot — the existing SB5007 specs simply
# never spell those three.
# 31.59 a top-level mutation in a condition names its TARGET and claims no
# operator. MEASURED: this text comes from `validate_assignment_stmt`
# (validate.rs:1048), NOT from `arith_brief` — a mutation condition is
# decomposed before it could reach the latter. Said plainly because the obvious
# reading of the message is wrong, and a spec that names the wrong producer
# sends the next reader to the wrong file.
#
# It used to render `mutation x = ...` here, which was true only for this one
# spelling; see 31.90-31.94.
run_sash 'int x=0
if (( x = 5 )) { echo a }
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"mutation of x"* ]]; chk 31.59
# 31.60 `==` as a bare statement
run_sash 'int a=1
int b=2
(( a == b ))
echo $a $b'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"comparison a == b"* ]]; chk 31.60
# 31.61 `!=` as a bare statement
run_sash 'int a=1
int b=2
(( a != b ))
echo $a $b'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"comparison a != b"* ]]; chk 31.61
# 31.62 `>=` as a bare statement
run_sash 'int a=1
int b=2
(( a >= b ))
echo $a $b'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"comparison a >= b"* ]]; chk 31.62
# 31.63 a mutation diagnostic names its TARGET (DG, 2026-09-23). This spec has
# recorded three answers to the same program. It first read `mutation y ? ...`
# — the renderer knew six of nineteen operators and printed the rest as `?`.
# Made total, it read `mutation y + ...`: true, but it elided the parenthesised
# `x *= 2`, which is the entire thing being rejected. Naming the target ends
# both failure modes — there is no operator to get wrong and nothing to elide.
run_sash 'int x=1
int y=1
if (( y + (x *= 2) )) { echo a }
echo $x $y'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"mutation of x"* ]]; chk 31.63

# 31.64-31.68 DD#116: `tok_to_text` is a total renderer. It used to end in
# `_ => String::new()`, which deleted 35 of the 59 token kinds from the
# collected arithmetic text before the re-lexer ever saw them, so trailing
# garbage vanished and the expression compiled. bash reports a syntax error
# for every one of these.
# 31.64 a keyword after a complete expression, statement form
run_sash 'int x=1
(( x = 1 done ))
echo $x'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk 31.64
# 31.65 the same, expansion form
run_sash 'int x=$(( 1 esac ))
echo $x'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk 31.65
# 31.66 a redirect operator is not arithmetic either
run_sash 'int x=1
(( x = 1 <<< ))
echo $x'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk 31.66
# 31.67 an ANSI-C quote now reaches the re-lexer instead of vanishing
run_sash "int x=1
(( x = 1 \$'a' ))
echo \$x"
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk 31.67
# 31.68 the regression shield for the one token that legitimately reaches the
# renderer as a separator: `for (( ;; ))` collects as `;;` and must stay a
# legitimately empty expression, not a parse error.
run_sash 'for (( ;; )); do echo hi; break; done'
[[ $rc == 0 && $out == hi ]]; chk 31.68

# ---------------------------------------------------------------------------
# 31.69-31.74 `arith_brief`'s mutation arms are LIVE, and these are what reach
# them. Twice they were measured "provably dead" — by panic instrumentation
# plus the full sweep plus fourteen hand-written probes — because every probe
# put the mutation either at top level (decomposed into statements before
# validation, so the message comes from `validate_assignment_stmt`) or inside
# parens (wrapped in `ArithExpr::Paren`, so it falls to the `_` arm).
#
# The shape that reaches them is a mutation as the unparenthesised RIGHT
# operand of a binary operator. Nobody writes it: bash binds `=` loosest and
# rejects `1 > x = 2` as an assignment to a non-variable, while sash's parser
# groups it as `1 > (x = 2)`. Every context still refuses the program, so the
# precedence difference is not observable — but these arms run.
#
# They reach it through the **non-final comma** site. These used to use a plain
# `if (( … ))`, which stopped working the moment the mutation-in-condition
# message began naming its target instead of quoting the expression — a third
# measurement of the same arms, and the same answer each time: live, but only
# from the one caller that still quotes. `arith_brief` now has exactly two
# callers and both ask about a **non**-mutation, so a mutation reaches it only
# as an operand of the thing being quoted.
# ---------------------------------------------------------------------------

# 31.69 reaches the `Assign` arm
run_sash 'int x=0
if (( 1 > x = 2, x > 0 )) { echo a }
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"non-final comma expression 1 > x = ..."* ]]; chk 31.69

# 31.70 reaches the `CompoundAssign` arm
run_sash 'int x=0
if (( 1 > x += 2, x > 0 )) { echo a }
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"non-final comma expression 1 > x += ..."* ]]; chk 31.70

# 31.71 a compound operator outside the old two-arm set. This read `x ?=`
# before the renderer was made total — `?=` is not a spelling sash has.
run_sash 'int x=1
if (( 1 > x *= 2, x > 0 )) { echo a }
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"non-final comma expression 1 > x *= ..."* ]]; chk 31.71

# 31.72 reaches the `PostIncDec` arm, `++` direction.
run_sash 'int x=0
if (( x++ > 0, x > 0 )) { echo a }
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"non-final comma expression x++ > 0"* ]]; chk 31.72

# 31.73 the $(( )) site once rendered BOTH kinds of mutation as `x = ...`, so a
# compound assignment was reported with an operator the author never wrote.
# It now names the target, so there is no operator to get wrong — and the
# second clause is the actual assertion: the `*=` must not reappear.
run_sash 'int x=1
echo $(( x *= 2 ))
echo $x'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"mutation of x"* && $err != *"x *="* ]]; chk 31.73

# 31.74 ... and a shifted one, which had no dedicated arm anywhere
run_sash 'int x=1
echo $(( x <<= 1 ))
echo $x'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"mutation of x"* && $err != *"x <<="* ]]; chk 31.74

# ── tok_to_text: no token is silently dropped from collected `(( ))` ──
# The raw collector re-renders every token it consumes. It once ended in
# `_ => String::new()`, which DELETED 35 of 59 token kinds: `(( x = 1 done ))`
# collapsed to `x = 1` and compiled clean, where bash reports a syntax error.
# Each of the following reaches one arm of that match; the shield is that the
# token survives into the re-lexed text and produces a hard error rather than
# vanishing. (Measured: these eleven are the arms the suite never reached.)

# 31.75 `{` survives collection
run_sash 'int x=1
(( x = 1 + { ))
echo $x'
[[ $rc != 0 && -z $out && $err == *SB4035* ]]; chk 31.75

# 31.76 `}` survives collection
run_sash 'int x=1
(( x = 1 } 2 ))
echo $x'
[[ $rc != 0 && -z $out && $err == *SB4035* ]]; chk 31.76

# 31.77 `[[` survives collection
run_sash 'int x=1
(( x = 1 [[ 2 ))
echo $x'
[[ $rc != 0 && -z $out && $err == *SB4035* ]]; chk 31.77

# 31.78 `]]` survives collection
run_sash 'int x=1
(( x = 1 ]] 2 ))
echo $x'
[[ $rc != 0 && -z $out && $err == *SB4035* ]]; chk 31.78

# 31.79 `>&` survives collection
run_sash 'int x=1
(( x = 1 >& 2 ))
echo $x'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk 31.79

# 31.80 `<&` survives collection
run_sash 'int x=1
(( x = 1 <& 2 ))
echo $x'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk 31.80

# 31.81 `<>` survives collection
run_sash 'int x=1
(( x = 1 <> 2 ))
echo $x'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk 31.81

# 31.82 `&>` survives collection
run_sash 'int x=1
(( x = 1 &> 2 ))
echo $x'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk 31.82

# 31.83 a single-quoted operand survives collection
run_sash 'int x=1
(( x = '\''abc'\'' + 1 ))
echo $x'
[[ $rc != 0 && -z $out && $err == *SB4035* ]]; chk 31.83

# 31.84 a double-quoted operand survives collection. It is rendered as the
# placeholder `"..."`, which is lossy but always invalid, so the reject is
# guaranteed and SB4035 still names the real `"` the user typed.
run_sash 'int x=1
(( x = "abc" + 1 ))
echo $x'
[[ $rc != 0 && -z $out && $err == *SB4035* ]]; chk 31.84

# 31.85 `$(` survives collection
run_sash 'int x=1
(( x = $(echo 2) + 1 ))
echo $x'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk 31.85

# ── arith diagnostics name the source text, never a Rust variant ──
# SB4020 used to format the token with `{:?}`, so `>&` reported
# `found Amp` — an internal enum name for a character the user typed as `&`.
# `atok_to_text` is the single spelling list; the `!= *Amp*` half is the real
# shield, since a regression to `{:?}` would still contain "found".

# 31.86 the operand complaint shows `&`, not `Amp`
run_sash 'int x=1
(( x = 1 >& 2 ))
echo $x'
[[ $rc != 0 && $err == *SB4020* && $err == *'found `&`'* && $err != *Amp* ]]; chk 31.86

# 31.87 the expect() complaint shows `:`, not `Colon`
run_sash 'int x
x=$(( 1 ? 2 ))
echo $x'
[[ $rc != 0 && $err == *SB4020* && $err == *'expected `:`'* && $err != *Colon* ]]; chk 31.87

# 31.88 a keyword used as an identifier takes its spelling from
# `keyword_to_str`, not from the variant's Debug name (which agreed only by
# accident of every such variant being a single lowercase word)
run_sash 'for in in a b
do
  echo $in
done'
[[ $rc = 0 && $out = "a
b" && -z $err ]]; chk 31.88

# 31.89 control: parens are intercepted by step_arith_collect before
# tok_to_text (whose paren arms are unreachable, DD#116), so grouping and
# nesting must still evaluate normally
run_sash 'int x=$(( (1 + 2) * 3 ))
echo $x'
[[ $rc = 0 && $out = "9" && -z $err ]]; chk 31.89


# ── DD#94: the closing delimiter must not share a commented line ──
# "a comment runs to end of line, so a `))` on a commented line is part of the
# comment. `(( a = 1 # x ))` is therefore *unterminated*" (DD#94). The prose in
# journal 391 originally wrote these examples inline and claimed they ran;
# DG corrected it. These pin the rule so the next reader cannot repeat it.

# 94.11 DD#94's own worked example: inline `))` after a comment is unterminated
run_sash 'int a=0
(( a = 1 # x ))
echo $a'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"part of the comment"* ]]; chk 94.11

# 94.12 …and in an `$(( ))` rvalue. DD#94 records that `$((5 # 3))` used to
# print 5 — a silent truncation of the delimiter, the exact DD#72 failure.
run_sash 'echo $(( 5 # 3 ))'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"part of the comment"* ]]; chk 94.12

# 94.13 control: with `))` on its own line the same comment is fine
run_sash 'int a=0
(( a = 1 # x
))
echo $a'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk 94.13


# ── 31.90-31.94: the decomposed SB5007 names the TARGET, not a spelling ──
# `lower_arith_mutation_structured` collapses all four mutation spellings into
# `Assignment { op: Set, value: BinOp }` on purpose, so by the time
# `validate_assignment_stmt` builds this message the operator is gone. It used
# to call `assign_brief` regardless and quote `x = ...` for every one of them —
# an assignment the author never wrote for `x++`, and for `a[0] -= 2` it named
# the whole array. Reconstructing the operator from the BinOp is not possible
# either: `(( x = x - 2 ))` decomposes identically to `(( x -= 2 ))`.
# So it names the target and claims no operator. Each spelling gets a spec
# because each is a separate way in.

# 31.90 compound assignment: no `=` claimed
run_sash 'int x=4
if (( x -= 2 )); then echo y; fi
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"mutation of x"* ]]; chk 31.90

# 31.91 post-increment is not an assignment at all
run_sash 'int x=4
if (( x++ )); then echo y; fi
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"mutation of x"* ]]; chk 31.91

# 31.92 …and the decrement half
run_sash 'int x=4
if (( x-- )); then echo y; fi
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"mutation of x"* ]]; chk 31.92

# 31.93 a cell mutation names the CELL. This reported `mutation a = ...` —
# the whole array — which reads as a different program than the one written.
# The index is elided as `a[…]`, matching how every other diagnostic names a
# cell (bare_string_cell_error, scalar_subscript_diag, SB5006).
run_sash 'array a=(4 5)
if (( a[0] -= 2 )); then echo y; fi
echo ${a[0]}'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"mutation of a[…]"* ]]; chk 31.93

# 31.94 `while` reaches the same site as `if`
run_sash 'int x=4
while (( x /= 2 )); do echo y; done
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"mutation of x"* ]]; chk 31.94

# ── 31.95-31.104: arith_op_str renders every operator the author wrote ──
# A mutation nested under a binary operator keeps its shape, so `arith_brief`
# answers instead of the decomposed site and the operator survives into the
# message. `arith_op_str` is total over `IrArithOp` precisely so none of these
# can come back as `?` (31.63 records the regression that motivated it), but
# ten of the nineteen arms had never been rendered by any test. One spec per
# arm, because one spec per arm is what proves the arm.
#
# The non-final comma part is the shape that reaches it for EVERY operator.
# The bare-statement site reaches only the comparison-classified ones — a
# plain `(( 1 - x = 2 ))` is a legitimate hoisted store and compiles — so
# using it here would have silently covered four arms out of ten.

# 31.95 `-`
run_sash 'int x=4
if (( 1 - x = 2, x > 0 )); then echo y; fi
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"non-final comma expression 1 - x = ..."* ]]; chk 31.95

# 31.96 `/`
run_sash 'int x=4
if (( 1 / x = 2, x > 0 )); then echo y; fi
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"non-final comma expression 1 / x = ..."* ]]; chk 31.96

# 31.97 `%`
run_sash 'int x=4
if (( 1 % x = 2, x > 0 )); then echo y; fi
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"non-final comma expression 1 % x = ..."* ]]; chk 31.97

# 31.98 `**`
run_sash 'int x=4
if (( 1 ** x = 2, x > 0 )); then echo y; fi
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"non-final comma expression 1 ** x = ..."* ]]; chk 31.98

# 31.99 `&&`
run_sash 'int x=4
if (( 1 && x = 2, x > 0 )); then echo y; fi
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"non-final comma expression 1 && x = ..."* ]]; chk 31.99

# 31.100 `||`
run_sash 'int x=4
if (( 1 || x = 2, x > 0 )); then echo y; fi
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"non-final comma expression 1 || x = ..."* ]]; chk 31.100

# 31.101 `>>`
run_sash 'int x=4
if (( 1 >> x = 2, x > 0 )); then echo y; fi
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"non-final comma expression 1 >> x = ..."* ]]; chk 31.101

# 31.102 `&`
run_sash 'int x=4
if (( 1 & x = 2, x > 0 )); then echo y; fi
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"non-final comma expression 1 & x = ..."* ]]; chk 31.102

# 31.103 `|`
run_sash 'int x=4
if (( 1 | x = 2, x > 0 )); then echo y; fi
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"non-final comma expression 1 | x = ..."* ]]; chk 31.103

# 31.104 `^`
run_sash 'int x=4
if (( 1 ^ x = 2, x > 0 )); then echo y; fi
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"non-final comma expression 1 ^ x = ..."* ]]; chk 31.104

# ── 31.105-31.106: the other two mutation-brief sites ──
# There were three renderers and they did not agree. Both of these used to
# quote the expression; both now name the target, which is what made the
# disagreement unrepresentable rather than merely fixed.

# 31.105 the $(( )) site, inc/dec spelling. 31.73 and 31.74 do the compound
# spellings; all three now produce the same sentence, which is the point.
run_sash 'int x=4
echo $(( x++ ))'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"mutation of x"* ]]; chk 31.105

# 31.106 the nested-mutation site, PARENTHESISED. This said `mutation ...` —
# the renderer it used had no `CompoundAssign` arm and fell through to the
# elision, naming nothing at all. It must now agree exactly with 31.117, the
# same program without the parens: a paren changes the tree, not the culprit.
run_sash 'int x=4
int y=9
(( x = (y -= 2) ))
echo $x $y'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"mutation of y"* ]]; chk 31.106


# ── 31.107-31.117: `++x` is rejected in EVERY position, not most of them ──
# `PREFIX_INCDEC_INVARIANT` promises no pass after the parser can receive a
# `PreIncDec`, and every downstream arm is an `unreachable!()` on that basis.
# The promise was false. `reject_prefix_incdec` recursed through `BinOp`,
# `Assign`, `CompoundAssign`, `Comma` and `Paren` and ended in `_ => Ok(())`,
# so a prefix form inside a ternary branch, a unary operand, or the value of a
# subscript assignment sailed past and hit one of those ICEs — a compiler
# crash for a form the language plainly forbids. Found by consolidating the
# three mutation-brief renderers: the unified one has a `PreIncDec` arm, and
# it fired. Ten shapes, ten specs; the walk now delegates to
# `ArithExpr::children()`, which is total.

# 31.107 ternary, true slot, statement context
run_sash 'int x=1
(( 1 ? ++x : 0 ))
echo $x'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"prefix ++x is not allowed"* ]]; chk 31.107

# 31.108 ternary, condition slot
run_sash 'int x=1
int y=0
(( y = ++x ? 1 : 0 ))
echo $x $y'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"prefix ++x is not allowed"* ]]; chk 31.108

# 31.109 ternary, false slot
run_sash 'int x=1
int y=0
(( y = 0 ? 1 : ++x ))
echo $x $y'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"prefix ++x is not allowed"* ]]; chk 31.109

# 31.110 …and in a `$(( ))` value, which is a different entry point
run_sash 'int x=1
echo $(( 1 ? ++x : 0 ))'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"prefix ++x is not allowed"* ]]; chk 31.110

# 31.111 …and in a condition
run_sash 'int x=1
if (( 1 ? ++x : 0 )); then echo y; fi
echo $x'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"prefix ++x is not allowed"* ]]; chk 31.111

# 31.112 unary minus operand
run_sash 'int x=1
int y=0
(( y = -(++x) ))
echo $x $y'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"prefix ++x is not allowed"* ]]; chk 31.112

# 31.113 logical-not operand
run_sash 'int x=1
int y=0
(( y = !++x ))
echo $x $y'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"prefix ++x is not allowed"* ]]; chk 31.113

# 31.114 bitwise-not operand
run_sash 'int x=1
int y=0
(( y = ~++x ))
echo $x $y'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"prefix ++x is not allowed"* ]]; chk 31.114

# 31.115 the VALUE of a subscript assignment. The index half was already
# rejected — it is re-parsed through `parse_arith_str` on its own — so only
# this side was exposed, which is exactly the kind of asymmetry a partial
# hand-rolled walk produces.
run_sash 'array a=(1 2 3)
int i=0
(( a[0] = ++i ))
echo ${a[0]} $i'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"prefix ++i is not allowed"* ]]; chk 31.115

# 31.116 the decrement half takes the same path and gets its own advice
run_sash 'int x=1
int y=0
(( y = 0 ? 1 : --x ))
echo $x $y'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"prefix --x is not allowed"* ]]; chk 31.116

# 31.117 the rival consolidation itself: `reject_nested_mutation` used a
# renderer with no `CompoundAssign` arm, so this said `mutation ...`, naming
# nothing. It now names `y`, and 31.106 — the same program with the mutation
# parenthesised — must produce the identical sentence.
run_sash 'int x=4
int y=9
(( x = y -= 2 ))
echo $x $y'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"mutation of y"* ]]; chk 31.117


# ── 31.118-31.128: DG's rule — a mutation diagnostic names its target ──
# Five sites reported a mutation and four of them quoted the surrounding
# expression instead of naming what was mutated. Quoting is what let two of
# them print an operator the author never wrote, and what made `mutation ...`
# — naming nothing — an acceptable answer. They all say `mutation of <target>`
# now, so the same program reports the same sentence wherever it is caught.

# 31.118 the `--` direction of the inc/dec arm. cov.150/cov.151 used to be the
# `++`/`--` pair here; under target naming those two report identically, so the
# direction is only observable at the site that still quotes.
run_sash 'int x=0
if (( x-- > 0, x > 0 )) { echo a }
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"non-final comma expression x-- > 0"* ]]; chk 31.118

# 31.119 BUG (fixed 2026-09-23): `(( a[0] = y++ ))` was told to look inside a
# `$(( … ))` that does not appear anywhere in the program. `reject_nested_mutation`
# dispatches on the nodes that evaluate a sub-expression as an rvalue and its
# list omitted `SubscriptAssign`, so the statement fell past that gate to
# `check_arith_value_mutation` — which exists for the `$(( … ))` value path and
# says so. The scalar form one line below always answered correctly; two halves
# of one construct disagreeing is the tell. The negative clause is the actual
# shield: "inside" appears only in the borrowed `$(( … ))` message.
run_sash 'int array a=(1)
int y=2
(( a[0] = y++ ))
echo $a[0] $y'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"mutation of y"* && $err != *"inside"* ]]; chk 31.119

# 31.120 the control: same shape, scalar target. This is what 31.119 had to
# match and did not.
run_sash 'int x=1
int y=2
(( x = y++ ))
echo $x $y'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"mutation of y"* ]]; chk 31.120

# 31.121 the compound spelling of 31.119.
run_sash 'int array a=(1)
int y=2
(( a[0] += y++ ))
echo $a[0] $y'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"mutation of y"* ]]; chk 31.121

# 31.122 a cell as the *culprit* rather than the target — `b[…]`, eliding the
# index, which is how six other diagnostics name a cell.
run_sash 'int array a=(1)
int array b=(1 2)
(( a[0] = b[1] = 3 ))
echo $a[0] $b[1]'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"mutation of b[…]"* ]]; chk 31.122

# 31.123 the regression shield for 31.119: a cell assignment whose value is
# clean is a legitimate statement and must still compile and run.
run_sash 'int array a=(1)
int y=2
(( a[0] = y ))
echo $a[0]'
[[ $rc == 0 && $out == 2 ]]; chk 31.123

# 31.124 ... and the message 31.119 was wrongly borrowing still belongs to the
# construct that earned it: a real `$(( … ))` value. Deleting a wrong caller
# must not delete the message.
run_sash 'int array a=(1)
int z=$(( a[0] = 1 ))
echo $z'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"can't take effect"* ]]; chk 31.124

# 31.125 the nested condition site names a cell too.
run_sash 'int array a=(1 2)
if (( (a[0] = 3) )) { echo y }
echo $a[0]'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"mutation of a[…]"* ]]; chk 31.125

# 31.126 a mutation in a ternary branch of a condition. Under expression
# quoting this was `mutation ...` — the elision covered the one sub-expression
# that mattered.
run_sash 'int x=1
if (( 1 ? (x = 2) : 0 )) { echo y }
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"mutation of x"* ]]; chk 31.126

# 31.127 the loop-increment idiom, the one shape a person actually writes.
# It reported `mutation ...` for years.
run_sash 'int i=0
while (( (i = i + 1) < 3 )); do echo $i; done'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"mutation of i"* ]]; chk 31.127

# 31.128 the decomposed and nested paths agree on one program. `if (( x = 5 ))`
# is split into statements before validation and answered by
# `validate_assignment_stmt`; `if (( (x = 5) ))` stays an expression and is
# answered by `validate_arith_in_condition`. Two sites, one sentence.
run_sash 'int x=1
if (( (x = 5) )) { echo a }
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"mutation of x not allowed in condition"* ]]; chk 31.128


# ── DD#94 revised: `,` separates, `;`/newline terminate ──
# DG (2026-09-24): DD#94's "`,`, `;` and newline are interchangeable" overreached.
# A terminator says "that one is done", so repeating it terminates nothing twice
# and a run of them is trivia; a separator asserts an expression on EACH side, so
# an empty side is a contradiction. Letting `,` inherit the terminator privileges
# (run-collapsing, and being swallowed by a pending operator) made malformed
# arithmetic a *wrong answer*: `$(( 1 + , 2 ))` silently dropped the comma and
# printed 3. Found while chasing atok_to_text's cold arms (journal 398).
# 94.31 the wrong-answer case: this printed 3 before the fix.
run_sash 'echo "$(( 1 + , 2 ))"'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *'found `,`'* ]]; chk 94.31
# 94.32 a leading comma is an empty element, not trivia (printed 1 before).
run_sash 'echo "$(( , 1 ))"'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *'found `,`'* ]]; chk 94.32
# 94.33 an interior empty element (printed 2 before — the run collapsed).
run_sash 'echo "$(( 1 ,, 2 ))"'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *'found `,`'* ]]; chk 94.33
# 94.34 all-commas took the "a run of nothing but separators is a legitimately
# empty expression" escape and printed 0. That escape is for `;;` in a C-style
# for-header — terminators — so it no longer admits a comma.
run_sash 'echo "$(( ,, ))"'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *'found `,`'* ]]; chk 94.34
# 94.35 a trailing comma still owes an operand, where a trailing newline (94.6)
# simply ends the block.
run_sash 'echo "$(( 1, ))"'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"ends where an operand is expected"* ]]; chk 94.35
# 94.36 the block form answers the same way — one rule, both sites.
run_sash 'int x=0
(( x = 1 + , 2 ))
echo $x'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *'found `,`'* ]]; chk 94.36
# 94.37 a comma with real expressions on both sides still sequences (value=last).
run_sash 'echo "$(( 1, 2 ))"'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk 94.37
# 94.38 a newline before the comma is trivia, and both sides of the comma are
# real — so this is accepted, not an empty element.
run_sash 'int a; int b
(( a = 1
   , b = 2 ))
echo "$a$b"'
[[ $rc = 0 && $out = "12" && -z $err ]]; chk 94.38
# 94.39 …and the natural formatting, comma then newline.
run_sash 'int a; int b
(( a = 1,
   b = 2 ))
echo "$a$b"'
[[ $rc = 0 && $out = "12" && -z $err ]]; chk 94.39
# 94.40 the for-header's empty slots are `;`-delimited, so they are untouched —
# this is what the all-terminators escape is actually for.
run_sash 'int n=0
for ((;;)); do n=$((n+1)); if (( n > 2 )); then break; fi; done
echo $n'
[[ $rc = 0 && $out = "3" && -z $err ]]; chk 94.40

# ── DD#77: a lone disposition is still ARITHMETIC (journal 400) ──
# `arith_leaf_to_ssa` hands a `${…}` import back as the string it is, and every
# other position forced the conversion afterwards — an operator lowers its
# operands as ints, an int assignment types the destination. A lone leaf in
# *value* position had neither, so the string reached the sink verbatim: the
# conversion silently did not happen. Found while gating the taint bypass these
# same expansions were slipping through (spec/taint.sh atint.*).
# adisp.1 `${x:-N}` alone: a non-integer takes the default, it does not pass
# through. This printed `abc`.
run_sash 'x=abc; echo "$(( ${x:-7} ))"'
[[ $rc = 0 && $out = "7" && -z $err ]]; chk adisp.1
# adisp.2 redundant grouping is the same lone leaf — `(${x:-7})` printed `abc`
# too, so a fix that only looked at the top node would have missed it.
run_sash 'x=abc; echo "$(( (${x:-7}) ))"'
[[ $rc = 0 && $out = "7" && -z $err ]]; chk adisp.2
# adisp.3 an integer is unaffected — the default is a default, not a coercion.
run_sash 'x=5; echo "$(( ${x:-7} ))"'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk adisp.3
# adisp.4 `${x:?msg}` is defined to ABORT on a non-integer. Passing the string
# through meant the one disposition whose whole job is to fail, didn't.
run_sash 'x=abc; echo "$(( ${x:?not an int} ))"'
[[ $rc != 0 && -z $out && $rerr == *"not an int"* ]]; chk adisp.4
# adisp.5 the same lone leaf as a command argument, not just inside echo.
run_sash 'x=abc; /bin/echo "$(( ${x:-7} ))"'
[[ $rc = 0 && $out = "7" && -z $err ]]; chk adisp.5
# adisp.6 an operator always forced the conversion — the shape that worked,
# kept so a regression that re-breaks only the lone leaf is still visible here.
run_sash 'x=abc; echo "$(( ${x:-7} + 0 ))"'
[[ $rc = 0 && $out = "7" && -z $err ]]; chk adisp.6

set -e
tally
