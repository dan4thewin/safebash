#!/bin/bash
# spec/argv.sh — program-argument access (DD#60 argv/arg/argc trio).
# Covers C-semantics argc, scalar arg N, array-splat argv / argv N,
# the array-snapshot RHS, taint rules, and mixed-taint for-in rejection.
# Supersedes the DD#42 $(argv N) scalar shape.
set -eu
. common.sh
setup argv
plan=34
echo "1..$plan" >&3

set +e

# ── Trio scalar semantics ────────────────────────────────────────

# av.t1 $(argc) is C argc: 1 + user-arg count
run_sash 'echo $(argc)' a b c
[[ $rc = 0 && $out = "4" && -z $err ]]; chk av.t1

# av.t2 $(argc) with no user args is 1 (program name only)
run_sash 'echo $(argc)'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk av.t2

# av.t3 $(arg 0) is the program name (non-empty; exact value invocation-dependent)
run_sash 'x=$(arg 0); bless x [[ -n $x ]]; echo "$x"'
[[ $rc = 0 && -n $out && -z $err ]]; chk av.t3

# av.t4 $(arg 1) is the first user arg
run_sash 'x=$(arg 1); bless x [[ -n $x ]]; echo "$x"' alpha
[[ $rc = 0 && $out = "alpha" && -z $err ]]; chk av.t4

# av.t5 $(arg 2) is the second user arg
run_sash 'x=$(arg 2); bless x [[ -n $x ]]; echo "$x"' alpha beta
[[ $rc = 0 && $out = "beta" && -z $err ]]; chk av.t5

# av.t6 $(arg N) for N >= argc aborts at runtime (no silent empty)
run_sash 'x=$(arg 5); bless x [[ -n $x ]]; echo "$x"' one two
[[ $rc != 0 && -z $out ]]; chk av.t6

# av.t7 negative index is a compile-time error
run_sash 'x=$(arg -1); echo "$x"'
[[ $rc != 0 && -z $out && -n $err ]]; chk av.t7

# ── Slice / splat semantics ──────────────────────────────────────

# av.s1 $(argv) iterates the whole array from index 0 (includes program name)
run_sash 'for x in $(argv); do bless x [[ -n $x ]]; echo "$x"; done' a b c
[[ $rc = 0 && $out == *$'\na\nb\nc' ]]; chk av.s1

# av.s2 $(argv 1) iterates the user-arg slice (index 1..)
run_sash 'for x in $(argv 1); do bless x [[ -n $x ]]; echo "$x"; done' a b c
[[ $rc = 0 && $out = $'a\nb\nc' && -z $err ]]; chk av.s2

# av.s3 $(argv N) with N >= argc yields zero iterations (NOT an abort)
run_sash 'for x in $(argv 99); do echo iter; done' a b c
[[ $rc = 0 && -z $out && -z $err ]]; chk av.s3

# av.s4 $(argv 0) is identical to $(argv)
run_sash 'for x in $(argv 0); do bless x [[ -n $x ]]; echo "$x"; done' a b c
[[ $rc = 0 && $out == *$'\na\nb\nc' ]]; chk av.s4

# av.s5 unblessed use of an argv-splat loop var is a clean error, not an ICE
# (the loop binds tainted argv elements — body must bless before use).
run_sash 'for x in $(argv 1); do echo "$x"; done' a b c
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk av.s5

# ── Snapshot to named array ──────────────────────────────────────

# av.n1 array a=$(argv) snapshots the whole array; bless-all then iterate @a
run_sash 'array a=$(argv); bless x in a [[ -n $x ]]; for x in @a; do echo "$x"; done' a b c
[[ $rc = 0 && $out == *$'\na\nb\nc' ]]; chk av.n1

# av.n2 len of an argv-slice snapshot (count needs no bless)
run_sash 'array a=$(argv 1); echo $(len a)' a b c
[[ $rc = 0 && $out = "3" && -z $err ]]; chk av.n2

# av.n3 independent snapshots: slice and whole sized separately
run_sash 'array a=$(argv 1); array b=$(argv); echo $(len a) $(len b)' a b
[[ $rc = 0 && $out = "2 3" && -z $err ]]; chk av.n3

# av.n4 join over a blessed snapshot (DD#41 collection builtin)
run_sash 'array a=$(argv 1); bless x in a [[ -n $x ]]; echo $(join @a)' a b c
[[ $rc = 0 && $out = "a b c" && -z $err ]]; chk av.n4

# av.n5 iterating a tainted snapshot @a without bless-all is a clean error,
# not an ICE — DD#61 requires bless-all the array before reading it.
run_sash 'array a=$(argv 1); for x in @a; do echo "$x"; done' a b c
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk av.n5

# av.n6 joining a tainted snapshot @a without bless-all is a clean error
# (negative for av.n4) — the cmdsub arg `@a` is a use site of tainted data.
run_sash 'array a=$(argv 1); echo $(join @a)' a b c
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk av.n6

# av.n7 subscripting a tainted snapshot without bless-all is a clean error
# (negative for av.x5) — reading $a[0] exposes an element, needs bless-all.
run_sash 'array a=$(argv); echo $a[0]' a b c
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk av.n7

# ── Taint rules ──────────────────────────────────────────────────

# av.x1 bare $(arg 0) used without bless is rejected (SB5020)
run_sash 'echo $(arg 0)'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk av.x1

# av.x2 bare $(arg 1) used without bless is rejected (SB5020)
run_sash 'echo $(arg 1)'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk av.x2

# av.x3 $(argv) splat in command-arg position is rejected
run_sash 'echo $(argv)'
[[ $rc != 0 && -z $out && -n $err ]]; chk av.x3

# av.x4 scalar arg, blessed, then used — accepted
run_sash 'x=$(arg 1); bless x [[ -n $x ]]; echo "$x"' alpha
[[ $rc = 0 && $out = "alpha" && -z $err ]]; chk av.x4

# av.x5 snapshot array, bless-all, then read an element — accepted
run_sash 'array a=$(argv); bless x in a [[ -n $x ]]; echo $a[0]' a b c
[[ $rc = 0 && -n $out && -z $err ]]; chk av.x5

# av.x6 splat for-in with per-iteration body-bless, then use — accepted
run_sash 'for x in $(argv 1); do bless x [[ $x =~ . ]]; echo "got:$x"; done' a b
[[ $rc = 0 && $out = $'got:a\ngot:b' && -z $err ]]; chk av.x6

# av.x7 scalar assignment of the array-splat form is rejected
run_sash 'x=$(argv); echo done'
[[ $rc != 0 && -z $out && -n $err ]]; chk av.x7

# av.x8 array splat in a test expression is rejected
run_sash '[[ -n $(argv) ]] && echo yes'
[[ $rc != 0 && -z $out && -n $err ]]; chk av.x8

# ── Mixed-taint for-in rejection ─────────────────────────────────

# av.m1 clean + tainted sources in one for-in header is rejected
run_sash 'for x in head $(argv) tail; do echo "$x"; done'
[[ $rc != 0 && -z $out && -n $err ]]; chk av.m1

# av.m2 two tainted sources in one for-in header is rejected
run_sash 'for x in $(argv) $(argv 1); do echo "$x"; done'
[[ $rc != 0 && -z $out && -n $err ]]; chk av.m2

# av.m3 named array + argv in one for-in header is rejected
run_sash 'array a=(x y); for x in @a $(argv); do echo "$x"; done'
[[ $rc != 0 && -z $out && -n $err ]]; chk av.m3

# ── DD#46 positionals inside (( )) / $(( )) ──────────────────────
# $#/$1.. are function-local but legal inside arithmetic there; the
# arith lexer must scan the special single-char name and the validator
# must still reject them at top level (bug-dollar-hash-in-arith).

# av.h1 $# is readable inside $(( )) within a function
run_sash 'fn f { x=$(( $# + 10 )); echo $x; }
f a b'
[[ $rc = 0 && $out = "12" && -z $err ]]; chk av.h1

# av.h2 $? (clean everywhere) is readable inside $(( )) at top level
run_sash 'true
x=$(( $? + 5 ))
echo $x'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk av.h2

# av.h3 $# drives a (( )) comparison condition inside a function. The janet
# backend mishandles ALL (( )) conditions (emit_condition_inline emits a
# constant `true`); native/c are the reference. See bug-janet-arith-condition.
if in_modes native,c; then
    run_sash 'fn f {
  if (( $# < 2 )); then echo few; else echo many; fi
}
f a
f a b c'
    [[ $rc = 0 && $out = $'few\nmany' && -z $err ]]; chk av.h3
else
    skip_chk av.h3
fi

# av.h4 top-level $# inside (( )) is rejected (DD#46, SB5024)
run_sash 'if (( $# < 2 )); then echo x; fi'
[[ $rc != 0 && -z $out && $err == *SB5024* ]]; chk av.h4

set -e
tally
