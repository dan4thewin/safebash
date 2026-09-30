#!/bin/bash
# spec/argv.sh — program-argument access (DD#60 argv/arg/argc trio).
# Covers C-semantics argc, scalar arg N, array-splat argv / argv N,
# the array-snapshot RHS, taint rules, and mixed-taint for-in rejection.
# Supersedes the DD#42 $(argv N) scalar shape.
set -eu
. common.sh
setup argv
plan=76
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
run_sash 'x=$(arg 0); attest x [[ -n $x ]]; echo "$x"'
[[ $rc = 0 && -n $out && -z $err ]]; chk av.t3

# av.t4 $(arg 1) is the first user arg
run_sash 'x=$(arg 1); attest x [[ -n $x ]]; echo "$x"' alpha
[[ $rc = 0 && $out = "alpha" && -z $err ]]; chk av.t4

# av.t5 $(arg 2) is the second user arg
run_sash 'x=$(arg 2); attest x [[ -n $x ]]; echo "$x"' alpha beta
[[ $rc = 0 && $out = "beta" && -z $err ]]; chk av.t5

# av.t6 $(arg N) for N >= argc aborts at runtime (no silent empty)
run_sash 'x=$(arg 5); attest x [[ -n $x ]]; echo "$x"' one two
[[ $rc != 0 && -z $out ]]; chk av.t6

# av.t7 negative index is a compile-time error (DD#60, SB5038) — attested so the
# ONLY error is the negative index, not a downstream unattested-read taint check
run_sash 'x=$(arg -1); attest x [[ -n $x ]]; echo "$x"'
[[ $rc != 0 && -z $out && $err = *SB5038* ]]; chk av.t7
# av.t7b the DEFAULT form `$(arg -N D)` also rejects the negative literal
run_sash 'x=$(arg -1 fallback); attest x [[ -n $x ]]; echo "$x"'
[[ $rc != 0 && -z $out && $err = *SB5038* ]]; chk av.t7b

# ── DD#60: a literal `arg` index must be a non-negative integer (SB5078) ──
# The runtime parses the index by taking leading digits, so a malformed
# literal silently reads a DIFFERENT argument and exits 0. av.t6 already
# establishes the intent — an out-of-range index aborts rather than yielding
# empty — and these are the same rule for the same reason.

# av.t8 a non-numeric index read argument 0, the PROGRAM NAME, with exit 0.
run_sash 'x=$(arg abc); attest x [[ -n $x ]]; echo "$x"'
[[ $rc != 0 && -z $out && $err = *SB5078* ]]; chk av.t8

# av.t8b trailing junk is the silent one: `1x` read argument 1 and looked
# entirely successful.
run_sash 'x=$(arg 1x); attest x [[ -n $x ]]; echo "$x"' alpha
[[ $rc != 0 && -z $out && $err = *SB5078* ]]; chk av.t8b

# av.t8c a lone `-` is not a negative literal (nothing follows it), so it
# fell past the SB5038 guard and read argument 0.
run_sash 'x=$(arg -); attest x [[ -n $x ]]; echo "$x"'
[[ $rc != 0 && -z $out && $err = *SB5078* ]]; chk av.t8c

# av.t8d `-x` likewise — `-` then a non-digit.
run_sash 'x=$(arg -x); attest x [[ -n $x ]]; echo "$x"'
[[ $rc != 0 && -z $out && $err = *SB5078* ]]; chk av.t8d

# av.t8e an explicit `+` is not accepted either; `+1` read argument 1.
run_sash 'x=$(arg +1); attest x [[ -n $x ]]; echo "$x"' alpha
[[ $rc != 0 && -z $out && $err = *SB5078* ]]; chk av.t8e

# av.t8f the DEFAULT form rejects a malformed index too.
run_sash 'x=$(arg abc fallback); attest x [[ -n $x ]]; echo "$x"'
[[ $rc != 0 && -z $out && $err = *SB5078* ]]; chk av.t8f

# av.t8g a NEGATIVE literal keeps its own DD#60 diagnostic — the new check
# runs after it, so SB5038 is not swallowed by the broader rule.
run_sash 'x=$(arg -42); attest x [[ -n $x ]]; echo "$x"'
[[ $rc != 0 && -z $out && $err = *SB5038* ]]; chk av.t8g

# av.t8h a leading-zero index is a perfectly good integer and still works —
# the guard tests digits, not spelling.
run_sash 'x=$(arg 01); attest x [[ -n $x ]]; echo "$x"' alpha
[[ $rc = 0 && $out = "alpha" && -z $err ]]; chk av.t8h

# av.t8i a COMPUTED index is a runtime concern, not a typo, and is untouched.
run_sash 'int i=1; x=$(arg $i); attest x [[ -n $x ]]; echo "$x"' alpha
[[ $rc = 0 && $out = "alpha" && -z $err ]]; chk av.t8i

# ── DD#60 addendum: $(arg N DEFAULT) opt-in default ──────────────

# av.d1 $(arg N DEFAULT) returns argv[N] when in range
run_sash 'x=$(arg 1 fallback); attest x [[ -n $x ]]; echo "$x"' hello
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk av.d1

# av.d2 $(arg N DEFAULT) returns the default when N is out of range
run_sash 'x=$(arg 1 fallback); attest x [[ -n $x ]]; echo "$x"'
[[ $rc = 0 && $out = "fallback" && -z $err ]]; chk av.d2

# av.d3 the default fires for a high OOB index too (no abort, unlike bare arg)
run_sash 'x=$(arg 5 mydef); attest x [[ -n $x ]]; echo "$x"' one two
[[ $rc = 0 && $out = "mydef" && -z $err ]]; chk av.d3

# av.d4 a clean intrinsic default (quoted) is accepted and used on OOB.
run_sash 'x=$(arg 9 "$(getcwd)"); attest x [[ -n $x ]]; echo "$x"'
[[ $rc = 0 && -n $out && -z $err ]]; chk av.d4

# av.d5 DD#78: a tainted default is rejected (a read must not sneak through)
run_sash 'y=$(getenv HOME); x=$(arg 1 $y); attest x [[ -n $x ]]; echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk av.d5

# av.d6 arity: more than an index + default is a compile error
run_sash 'x=$(arg 1 2 3); attest x [[ -n $x ]]; echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB5036* ]]; chk av.d6

# av.d7 DD#78: a $(random) default is a side effect under eager eval → reject
run_sash 'x=$(arg 1 "$(random)"); attest x [[ -n $x ]]; echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB5035* ]]; chk av.d7

# ── Slice / splat semantics ──────────────────────────────────────

# av.s1 $(argv) iterates the whole array from index 0 (includes program name)
run_sash 'for x in $(argv); do attest x [[ -n $x ]]; echo "$x"; done' a b c
[[ $rc = 0 && $out == *$'\na\nb\nc' ]]; chk av.s1

# av.s2 $(argv 1) iterates the user-arg slice (index 1..)
run_sash 'for x in $(argv 1); do attest x [[ -n $x ]]; echo "$x"; done' a b c
[[ $rc = 0 && $out = $'a\nb\nc' && -z $err ]]; chk av.s2

# av.s3 $(argv N) with N >= argc yields zero iterations (NOT an abort)
run_sash 'for x in $(argv 99); do attest x [[ -n $x ]]; echo iter; done' a b c
[[ $rc = 0 && -z $out && -z $err ]]; chk av.s3

# av.s4 $(argv 0) is identical to $(argv)
run_sash 'for x in $(argv 0); do attest x [[ -n $x ]]; echo "$x"; done' a b c
[[ $rc = 0 && $out == *$'\na\nb\nc' ]]; chk av.s4

# av.s5 unattested use of an argv-splat loop var is a clean error, not an ICE
# (the loop binds tainted argv elements — body must attest before use).
run_sash 'for x in $(argv 1); do echo "$x"; done' a b c
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk av.s5

# ── argv snapshot → out-param builtin `argv [N] a` (DD#60) ───────────
# `argv a` fills array a with all program args; `argv N a` fills the
# argv[N:] slice (array last, like read -a / mapfile / match S P arr).
# Auto-declares `a` (DG 2026-07-08) — no prior `array a=()` needed. The
# filled array is tainted (DD#61): attest-all before reading.

# av.n1 `argv a` fills the whole array; attest-all then iterate @a
run_sash 'argv a; attest x in a [[ -n $x ]]; for x in @a; do echo "$x"; done' a b c
[[ $rc = 0 && $out == *$'\na\nb\nc' ]]; chk av.n1

# av.n2 len of an argv-slice snapshot (count needs no attest)
run_sash 'argv 1 a; echo $(len a)' a b c
[[ $rc = 0 && $out = "3" && -z $err ]]; chk av.n2

# av.n3 independent snapshots: slice and whole sized separately
run_sash 'argv 1 a; argv b; echo $(len a) $(len b)' a b
[[ $rc = 0 && $out = "2 3" && -z $err ]]; chk av.n3

# av.n4 join over a attested snapshot (DD#41 collection builtin)
run_sash 'argv 1 a; attest x in a [[ -n $x ]]; echo $(join a)' a b c
[[ $rc = 0 && $out = "a b c" && -z $err ]]; chk av.n4

# av.n5 iterating a tainted snapshot @a without attest-all is a clean error,
# not an ICE — DD#61 requires attest-all the array before reading it.
run_sash 'argv 1 a; for x in @a; do echo "$x"; done' a b c
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk av.n5

# av.n6 joining a tainted snapshot @a without attest-all is a clean error
# (negative for av.n4) — the cmdsub arg `@a` is a use site of tainted data.
run_sash 'argv 1 a; echo $(join a)' a b c
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk av.n6

# av.n7 subscripting a tainted snapshot without attest-all is a clean error
# (negative for av.x5) — reading $a[0] exposes an element, needs attest-all.
run_sash 'argv a; echo $a[0]' a b c
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk av.n7

# ── Taint rules ──────────────────────────────────────────────────

# av.x1 bare $(arg 0) used without attest is rejected (SB5020)
run_sash 'echo $(arg 0)'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk av.x1

# av.x2 bare $(arg 1) used without attest is rejected (SB5020)
run_sash 'echo $(arg 1)'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk av.x2

# av.x3 $(argv) splat in command-arg position is rejected
run_sash 'echo $(argv)'
[[ $rc != 0 && -z $out && -n $err ]]; chk av.x3

# av.x4 scalar arg, attested, then used — accepted
run_sash 'x=$(arg 1); attest x [[ -n $x ]]; echo "$x"' alpha
[[ $rc = 0 && $out = "alpha" && -z $err ]]; chk av.x4

# av.x5 snapshot array, attest-all, then read an element — accepted
run_sash 'argv a; attest x in a [[ -n $x ]]; echo $a[0]' a b c
[[ $rc = 0 && -n $out && -z $err ]]; chk av.x5

# av.x6 splat for-in with per-iteration body-attest, then use — accepted
run_sash 'for x in $(argv 1); do attest x [[ $x =~ . ]]; echo "got:$x"; done' a b
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
run_sash 'fn f { : @args; x=$(( $# + 10 )); echo $x; }
f a b'
[[ $rc = 0 && $out = "12" && -z $err ]]; chk av.h1

# av.h2 $? (clean everywhere) is readable inside $(( )) at top level
run_sash 'true
x=$(( $? + 5 ))
echo $x'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk av.h2

# av.h3 $# drives a (( )) comparison condition inside a function.
run_sash 'fn f {
  : @args
  if (( $# < 2 )); then echo few; else echo many; fi
}
f a
f a b c'
[[ $rc = 0 && $out = $'few\nmany' && -z $err ]]; chk av.h3

# av.h4 top-level $# inside (( )) is rejected (DD#46, SB5024)
run_sash 'if (( $# < 2 )); then echo x; fi'
[[ $rc != 0 && -z $out && $err == *SB5024* ]]; chk av.h4

# ── DD#60 revision: the old snapshot-assignment form is rejected ─────

# av.rej1 `array a=$(argv)` (old snapshot assignment) → rejected; a command
#         substitution no longer yields an array — use `argv a`.
run_sash 'array a=$(argv); echo done' a b c
[[ $rc != 0 && -z $out && -n $err ]]; chk av.rej1

# av.rej2 `array a=$(argv 1)` (old slice snapshot assignment) → rejected
run_sash 'array a=$(argv 1); echo done' a b c
[[ $rc != 0 && -z $out && -n $err ]]; chk av.rej2

# av.rej3 a slice start past the end → empty array (zero iters), not an
#         abort (parity with the old `$(argv N)` behavior)
run_sash 'argv 5 a; echo $(len a)' a b
[[ $rc = 0 && $out = "0" && -z $err ]]; chk av.rej3

# ── The target is a BINDING, not a value (SB5063) ────────────────────
# A quoted target silently wrote nowhere: the array kept its old contents
# and, because `argv` is a tainted source, the attest obligation vanished
# along with the write. Same rule DD#99 already enforces for `read`.

# av.rej4 quoted target → rejected (was: silent no-op, stale data survives)
run_sash 'array a=(old); argv 1 "a"; echo $(len a)' x y z
[[ $rc != 0 && -z $out && $err == *SB5063* ]]; chk av.rej4

# av.rej5 a sigilled target is a value, not a binding → rejected
run_sash 'str t=a; array a=(old); argv 1 $t; echo "$(len a)$t"' x y z
[[ $rc != 0 && -z $out && $err == *SB5063* ]]; chk av.rej5

# av.rej6 control: the bare target still binds, taints, and needs an attest
run_sash 'array a=(old); argv 1 a; attest x in a [[ -n $x ]]; echo $(len a)' x y z
[[ $rc = 0 && $out = "3" && -z $err ]]; chk av.rej6

# ── The FORM is checked at compile time (SB4045 / SB5085) ────────────
# av.rej4/5 fixed the quoted *target*. The index and the arity had no
# check at all: `try_handle_argv` returned None for any shape it could
# not lower, and None in `try_builtin` means "not a builtin", so the
# statement became an ordinary command named `argv`. It compiled clean
# and the binary died with `sash: bug: bare command passed to
# sash_exec`. Eight spellings reached that. The index is read with
# `expr_to_string`, a *display* renderer that keeps quotes, so `"1"`
# never parsed as 1.

# av.f1 a zero-argument `argv` has no target (was: runtime ICE)
run_sash 'argv; echo hi' x y z
[[ $rc != 0 && -z $out && $err == *SB4045* ]]; chk av.f1

# av.f2 three arguments is neither `a` nor `N a` (was: runtime ICE)
run_sash 'argv 1 2 a; echo $(len a)' x y z
[[ $rc != 0 && -z $out && $err == *SB4045* ]]; chk av.f2

# av.f3 the brackets in the docs mean "optional", not literal — taking
# SB5025's own advice verbatim used to produce a binary that ICEd
run_sash 'argv [1] a; echo $(len a)' x y z
[[ $rc != 0 && -z $out && $err == *SB5085* ]]; chk av.f3

# av.f4 a double-quoted index names no index (was: runtime ICE)
run_sash 'argv "1" a; echo $(len a)' x y z
[[ $rc != 0 && -z $out && $err == *SB5085* ]]; chk av.f4

# av.f5 likewise single-quoted
run_sash "argv '1' a; echo \$(len a)" x y z
[[ $rc != 0 && -z $out && $err == *SB5085* ]]; chk av.f5

# av.f6 a negative index is rejected, not clamped (was: runtime ICE)
run_sash 'argv -1 a; echo $(len a)' x y z
[[ $rc != 0 && -z $out && $err == *SB5085* ]]; chk av.f6

# av.f7 a non-integer index (was: runtime ICE)
run_sash 'argv 1.5 a; echo $(len a)' x y z
[[ $rc != 0 && -z $out && $err == *SB5085* ]]; chk av.f7

# av.f8 an index too large for i64 is rejected, not wrapped
run_sash 'argv 9999999999999999999999 a; echo $(len a)' x y z
[[ $rc != 0 && -z $out && $err == *SB5085* ]]; chk av.f8

# av.f9 a sigilled index is rejected too. DG ruled 2026-09-22 that a
# compile-time-constant fill start is the RULE, not a gap: the message
# names the two alternatives (whole-array index, or loop-and-skip).
run_sash 'int n=1; argv $n a; echo "$n$(len a)"' x y z
[[ $rc != 0 && -z $out && $err == *SB5085* ]]; chk av.f9

# av.f10 control: index 0 still lowers, and is the same as bare `argv a`
# (argv[0] included) — `argv 1 a` is the one that drops the program name
run_sash 'argv 0 a; echo $(len a)' x y z
[[ $rc = 0 && $out = "4" && -z $err ]]; chk av.f10

# av.f11 the advice must not render a metavariable as literal syntax.
# SB5025 said "snapshot into an array with `argv [N] a`"; `[N]` is a
# placeholder for a number but sits inside a code span, so it reads as
# literal brackets — and taking it literally is av.f3, which is rejected.
# DG: "are you putting literal brackets in the argv? I just did argv 2 a
# and it works". The advice now leads with `argv a`, which compiles as
# written, and spells the optional start bare, matching `$(arg N)`.
run_sash 'echo $(argv)' x y z
[[ $rc != 0 && $err == *SB5025* && $err != *"argv [N] a"* && $err == *"argv a"* ]]; chk av.f11


# ── The BODY of a `$( )` is the whole body (2026-09-29) ──────────────
# `validate::cmdsub_command_is` classified a substitution from
# `stmts.first()` and from the command *name* alone. Two consequences,
# pointing opposite ways.
#
# The splat is an expression: `transform::argv_splat_start` lowers the
# slice, not the body, so `for x in $(argv; CMD)` compiled clean,
# iterated the arguments, and silently discarded `CMD` — a side effect
# written there simply did not happen, with no diagnostic. `keys`
# escaped the identical defect only because SB5073 catches it first.
#
# And `argv` has two DD#60 shapes that the name alone cannot separate:
# the splat `$(argv N)` and the out-param fill `argv N a`. The fill was
# read as a misplaced splat and answered with SB5025, whose advice is
# "snapshot into an array with `argv a` (or `argv N a`)" — telling the
# author to write the form they had just written. It is a statement
# everywhere else (top level, a function body, an `if` body); DG ruled
# 2026-09-29 that a `$( )` body is no exception.

# av.c1 a statement after the splat cannot be silently dropped
run_sash 'for x in $(argv; /usr/bin/touch /tmp/sash-spec-avc1); do attest x [[ -n $x ]]; echo "$x"; done' p q
[[ $rc != 0 && -z $out && $err == *SB5025* && $err == *"whole command substitution"* ]]; chk av.c1

# av.c2 same for the sliced spelling
run_sash 'for x in $(argv 1; /bin/echo extra); do attest x [[ -n $x ]]; echo "$x"; done' p q
[[ $rc != 0 && -z $out && $err == *SB5025* ]]; chk av.c2

# av.c3 a redirection wrapper must not smuggle it past the check —
# `sole_cmdsub_command` looks through `Redirected`, so every `argv`
# shape test shares one unwrap rather than each writing its own
run_sash 'for x in $(argv 2>/dev/null; /bin/echo extra); do attest x [[ -n $x ]]; echo "$x"; done' p q
[[ $rc != 0 && -z $out && $err == *SB5025* ]]; chk av.c3

# av.c4 and in a capture, where the splat is not legal at all: the
# rejection must not be the only thing standing between the author and
# a silently-dropped `/bin/echo`
run_sash 'str s=$(argv; /bin/echo hi); attest s [[ -n $s ]]; echo "$s"' p q
[[ $rc != 0 && $out != "hi" && $err == *SB5025* ]]; chk av.c4

# av.c5 the DD#60 out-param fill is a statement inside `$( )` too —
# DD#86's addendum makes the body an anonymous function, so `b` is
# local to it and the substitution yields what the body prints
run_sash 'str s=$(argv 0 b; attest x in b [[ -n $x ]]; /bin/echo @{b:-none}); attest s [[ -n $s ]]; echo "$s"' p q
[[ $rc = 0 && $out = *" p q" && -z $err ]]; chk av.c5

# av.c6 with a start index, same as anywhere else (argv[0] dropped)
run_sash 'str s=$(argv 1 b; attest x in b [[ -n $x ]]; /bin/echo @{b:-none}); attest s [[ -n $s ]]; echo "$s"' p q
[[ $rc = 0 && $out = "p q" && -z $err ]]; chk av.c6

# av.c7 the two shapes collide at ONE argument: `argv b` is the fill
# (an array name), `$(argv 1)` is the splat (a bare index literal).
# SB5063 rejects a digit target and SB5085 rejects a non-literal index,
# so the argument itself discriminates and neither can be the other.
run_sash 'str s=$(argv b); attest s [[ -z $s ]]; echo "[$s]"' p q
[[ $rc = 0 && $out = "[]" && -z $err ]]; chk av.c7

# av.c8 control: the splat spelling is untouched
run_sash 'for x in $(argv 1); do attest x [[ -n $x ]]; echo "$x"; done' p q
[[ $rc = 0 && $out = $'p\nq' && -z $err ]]; chk av.c8


set -e
tally
