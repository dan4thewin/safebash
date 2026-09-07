#!/bin/bash
# spec/arith_literals.sh — DD#72 numeric literals in $(( )).
# Constants: decimal + 0x/0o/0b; leading-zero (010/08/0377) and bash base#
# (16#ff) reject (SB4035). Runtime string coercion: honors 0x/0o/0b, otherwise
# decimal with leading zeros ignored.
set -eu
. common.sh
setup arith_literals
plan=31
echo "1..$plan" >&3

set +e

# ── Constants: accepted forms (value check) ──────────────────────────
run_sash 'echo $((42))'
[[ $rc = 0 && $out = "42" && -z $err ]]; chk nl.d1
run_sash 'echo $((0))'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk nl.d2
# hex
run_sash 'echo $((0xff))'
[[ $rc = 0 && $out = "255" && -z $err ]]; chk nl.x1
run_sash 'echo $((0XFF))'
[[ $rc = 0 && $out = "255" && -z $err ]]; chk nl.x2
# octal (modern 0o prefix)
run_sash 'echo $((0o377))'
[[ $rc = 0 && $out = "255" && -z $err ]]; chk nl.o1
run_sash 'echo $((0O10))'
[[ $rc = 0 && $out = "8" && -z $err ]]; chk nl.o2
# binary
run_sash 'echo $((0b1010))'
[[ $rc = 0 && $out = "10" && -z $err ]]; chk nl.b1
run_sash 'echo $((0B1111))'
[[ $rc = 0 && $out = "15" && -z $err ]]; chk nl.b2
# in expressions / mixed bases
run_sash 'echo $((0xff + 0b1))'
[[ $rc = 0 && $out = "256" && -z $err ]]; chk nl.m1
run_sash 'echo $((0o17 * 2))'
[[ $rc = 0 && $out = "30" && -z $err ]]; chk nl.m2
# negatives (unary minus + prefixed literal)
run_sash 'echo $((-0xff))'
[[ $rc = 0 && $out = "-255" && -z $err ]]; chk nl.n1
run_sash 'echo $((-0o10))'
[[ $rc = 0 && $out = "-8" && -z $err ]]; chk nl.n2

# ── Constants: rejected forms (SB4035, parse-time → all modes) ───────
# leading-zero integers (C-octal creep): use 0o… or drop the zero
run_sash 'echo $((010))'
[[ $rc != 0 && -z $out && $err == *SB4035* ]]; chk nl.r1
run_sash 'echo $((08))'
[[ $rc != 0 && -z $out && $err == *SB4035* ]]; chk nl.r2
run_sash 'echo $((0377))'
[[ $rc != 0 && -z $out && $err == *SB4035* ]]; chk nl.r3
# bash arbitrary base# (dropped)
run_sash 'echo $((16#ff))'
[[ $rc != 0 && -z $out && $err == *SB4035* ]]; chk nl.r4
run_sash 'echo $((2#1010))'
[[ $rc != 0 && -z $out && $err == *SB4035* ]]; chk nl.r5
run_sash 'echo $((10#42))'
[[ $rc != 0 && -z $out && $err == *SB4035* ]]; chk nl.r6
# invalid digits for the prefix
run_sash 'echo $((0o8))'
[[ $rc != 0 && -z $out && $err == *SB4035* ]]; chk nl.r7
run_sash 'echo $((0b2))'
[[ $rc != 0 && -z $out && $err == *SB4035* ]]; chk nl.r8
# empty prefix (no digits)
run_sash 'echo $((0x))'
[[ $rc != 0 && -z $out && $err == *SB4035* ]]; chk nl.r9
# a bare stray '#' in arith is a clean error, not a silent skip. Since DD#94 a
# whitespace-preceded `#` is a COMMENT inside arithmetic, so this one eats the
# closing `))` and the error is the unterminated-arithmetic one.
run_sash 'echo $((5 # 3))'
[[ $rc != 0 && -z $out && -n $err ]]; chk nl.r10

# ── Runtime string→int coercion (data path) ─────────────────────────
# native/c use sash_atoi (DD#72); janet coerces string-vars-in-arith via its
# own runtime and can't `:+` a string var (pre-existing janet limitation — it
# illustrates the IR, native/c are the numeric oracles). Guard to native,c.
# DD#72 numeric-literal data coercion in arithmetic — under DD#77 D1 a bare
# string operand in arith is a compile error, so the conversion carries a `:?`
# disposition (`${x:?…}` validates + converts via the strict grammar, which
# still honors 0x/0o/0b and leading-zero-decimals; DD#72's bare-coercion is
# superseded). native,c — janet's arith VALUE path doesn't apply the
# disposition (janet-arith-subscript-read).
if in_modes native,c; then
    # honors 0x/0o/0b prefixes
    run_sash 'x=0xff; echo $(( ${x:?nan} + 0 ))'
    [[ $rc = 0 && $out = "255" && -z $err ]]; chk nl.s1
    run_sash 'x=0o10; echo $(( ${x:?nan} + 0 ))'
    [[ $rc = 0 && $out = "8" && -z $err ]]; chk nl.s2
    run_sash 'x=0b101; echo $(( ${x:?nan} + 0 ))'
    [[ $rc = 0 && $out = "5" && -z $err ]]; chk nl.s3
    # leading zeros ignored (decimal, NOT octal) — the anti-footgun
    run_sash 'x=010; echo $(( ${x:?nan} + 0 ))'
    [[ $rc = 0 && $out = "10" && -z $err ]]; chk nl.s4
    run_sash 'x=08; echo $(( ${x:?nan} + 0 ))'
    [[ $rc = 0 && $out = "8" && -z $err ]]; chk nl.s5
    run_sash 'x=0377; echo $(( ${x:?nan} + 0 ))'
    [[ $rc = 0 && $out = "377" && -z $err ]]; chk nl.s6
    # plain decimal data
    run_sash 'x=42; echo $(( ${x:?nan} + 0 ))'
    [[ $rc = 0 && $out = "42" && -z $err ]]; chk nl.s7
    # negative prefixed data
    run_sash 'x=-0xff; echo $(( ${x:?nan} + 0 ))'
    [[ $rc = 0 && $out = "-255" && -z $err ]]; chk nl.s8
    # zero
    run_sash 'x=0; echo $(( ${x:?nan} + 0 ))'
    [[ $rc = 0 && $out = "0" && -z $err ]]; chk nl.s10
else
    skip_chk nl.s1; skip_chk nl.s2; skip_chk nl.s3; skip_chk nl.s4; skip_chk nl.s5
    skip_chk nl.s6; skip_chk nl.s7; skip_chk nl.s8; skip_chk nl.s10
fi

set -e
tally
