#!/bin/bash
# spec/taint_switch.sh — DD#82 taint bypass switches: --no-taint (full) and
# --no-taint-cst (CST off, SSA backstop on, native only). Compile flags are
# injected via SASH_CFLAGS (common.sh).
set -eu
. common.sh
setup taint_switch
plan=25
echo "1..$plan" >&3

set +e

# sw.1 control: with both layers on, a tainted read is rejected (SB5020)
run_sash 'echo $(getenv HOME)'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk sw.1

# ── --no-taint: full bypass (CST + SSA) ──
export SASH_CFLAGS=--no-taint
# sw.6 the switch disables TAINT only — a language rule ($@, SB5045) still fires
run_sash 'echo "$@"'
[[ $rc != 0 && -z $out && $err == *SB5045* ]]; chk sw.6
# sw.7 a clean program is unaffected by the switch
run_sash 'echo hello'
[[ $rc = 0 && $out = hello && -z $err ]]; chk sw.7
# sw.2 a tainted scalar compiles & runs (leaks) under full bypass
run_sash 'x=$(arg 1); echo $x' hello
[[ $rc = 0 && $out = hello && -z $err ]]; chk sw.2
# sw.3 a tainted join compiles & runs under full bypass
run_sash 'argv 1 a; echo $(join a)' one two
[[ $rc = 0 && $out = "one two" && -z $err ]]; chk sw.3

# sw.12 the failing path of a *caught* attest-all, joined back with a later
# read of the subject, must still see a valid array
# (`bug-caught-attest-all-fail-path-undefined-handle`, SIGSEGV in
# `sash_table_len`). DD#121 rejects this source outright, so `--no-taint` is
# the only way it is still reachable — which is exactly why it is pinned here.
#
# The read must be a splat (`for v in @a`), not `len`/`join`: only the splat's
# `arrlen` lowering goes through `codegen::types::find_array_slot`, the
# name-keyed lookup that used to be able to select a version defined solely in
# the passing block.
#
# REPEATED ON PURPOSE. The defect was decided by `HashMap` iteration order at
# *compile* time, so one source produced two different binaries and a single
# compile missed it ~40% of the time. Six compiles take it under 1%. Re-running
# one binary proves nothing — each iteration must compile afresh.
_ok=1
for _ in 1 2 3 4 5 6; do
    run_sash 'argv 1 a
if attest v in a [[ $v == IMPOSSIBLE ]]; then echo p; else echo f; fi
for v in @a; do echo "$v"; done' one
    [[ $rc = 0 && $out = "f
one" && -z $err ]] || _ok=0
done
[[ $_ok = 1 ]]; chk sw.12
# sw.13 the passing path of the same form still retags and reads correctly —
# guards the fix (an alias, not a fresh copy) against breaking the clean case
run_sash 'argv 1 a
if attest v in a [[ -n $v ]]; then echo p; else echo f; fi
for v in @a; do echo "$v"; done' one two
[[ $rc = 0 && $out = "p
one
two" && -z $err ]]; chk sw.13
unset SASH_CFLAGS

# ── --no-taint-cst: CST off, SSA backstop remains (native only) ──
export SASH_CFLAGS=--no-taint-cst
# sw.4 the SSA backstop still rejects an assigned tainted source
run_sash 'x=$(getenv HOME); echo $x'
[[ $rc != 0 && -z $out ]]; chk sw.4
# sw.5 the SSA backstop catches a value propagated through echo
run_sash 'y=$(getenv HOME); x=$(echo $y); echo $x'
[[ $rc != 0 && -z $out ]]; chk sw.5
# sw.8 the SSA backstop catches an operand propagated through a transform
# builtin ($(toupper …)) — cmd_subst_taint propagates operand taint (DD#82)
run_sash 'y=$(getenv HOME); x=$(toupper $y); echo $x'
[[ $rc != 0 && -z $out ]]; chk sw.8
# sw.9 the SSA backstop rejects a pipeline laundered through a leading
# builtin (`echo ig | /bin/cat f | read x`). It could not: `clean_pipe` was
# hardcoded `true` for every lastpipe read, so the backstop was inert for
# this shape and the front-end pass was the only layer — and it missed it.
run_sash 'echo ig | /bin/cat /etc/hostname | readline x
echo "$x"'
[[ $rc != 0 && -z $out ]]; chk sw.9
# sw.10 the SSA backstop rejects a tainted ARRAY at a subscript read. This is
# the one tainted-array origin the backstop classifies: the array literal is
# an `Assign` whose value carries the cmdsub's taint. (The bulk-read origins
# DD#61 names — `read`/`mapfile`/`argv` fills — define no SSA VarRef and stay
# CST-only; see notes/tasks/ssa-taint-full-backstop.md, deferred by DG
# 2026-09-12.) Assert the message so this cannot pass on the scalar backstop.
run_sash 'array a=($(getenv HOME))
echo ${a[0]}'
[[ $rc != 0 && -z $out && $err == *"tainted array a[...]"* ]]; chk sw.10
# sw.11 control: a CLEAN array subscript is not false-positived by the
# backstop with the CST pass off — the reject above is about taint, not about
# arrays being unclassifiable at SSA.
run_sash 'array a=(x y)
echo ${a[0]}'
[[ $rc = 0 && $out = x && -z $err ]]; chk sw.11

# ── SSA taint backstop: the read positions the four-arm match never saw ────
# Moved here from taint.sh 2026-09-30. These ran under a normal compile and
# reached the backstop only because the front-end taint pass did not descend
# into a `$( … )` body — the vehicle every negative below used. That gap is
# closed (bug-taint-escape-in-cmdsub-body), so under a normal compile validate
# now rejects them FIRST, with a located SB code, and the backstop message
# never appears. Testing the backstop therefore requires turning the front end
# off explicitly, which is what `--no-taint-cst` is for.
#
# The front-end behaviour these used to assert incidentally is covered by
# `cbt.*` in taint.sh.
# `check_inst_uses` enumerated 4 of 19 `SsaInst` variants and ended in
# `_ => {}`, so `SsaInst::Test` and `SsaInst::RedirectPush` were never checked
# at all — both are listed in `classify_inst_def`, the write side, which has
# been exhaustive since it was written. The front-end pass does not descend
# into a `$( )` body (notes/tasks/bug-taint-escape-in-cmdsub-body.md), which is
# what made the gap reachable. Enumeration now lives in
# `SsaInst::taint_reads`, total over the enum with no wildcard.

# bkstp.1 NEGATIVE — a tainted REDIRECT TARGET inside a `$( )` body. This
#         compiled, and the binary wrote a file named by the environment.
run_sash '/bin/echo $(str h=$(getenv TARGET); echo pwned > $h)'
[[ $rc != 0 && -z $out && $err == *"taint backstop"* ]]; chk bkstp.1

# bkstp.2 NEGATIVE — a tainted TEST OPERAND inside a `$( )` body. This
#         compiled, and the binary branched on external input.
run_sash '/bin/echo $(str h=$(getenv TARGET); if [[ -n $h ]] { echo yes })'
[[ $rc != 0 && -z $out && $err == *"taint backstop"* ]]; chk bkstp.2

# bkstp.3 POSITIVE — an `attest` predicate reads its tainted subject by
#         design, and the caught forms reach SSA as a bare `SsaInst::Test`,
#         structurally identical to a user-written `[[ ]]`. Without
#         `TestOrigin` marking it, making `Test` a sink rejects every
#         caught-attest program — it took out 24.13 and bfb.1-6 in one sweep.
run_sash 'str v=$(arg 1)
if attest v [[ $v =~ ^[0-9]+$ ]] { /bin/echo "ok $v" } else { v=none }' 42
[[ $rc = 0 && $out = "ok 42" && -z $rerr ]]; chk bkstp.3

# bkstp.4 POSITIVE — an attested value is still usable as a redirect target.
#         A `Redirect`'s target is an unlowered `ir::Expr`, never an
#         `SsaValue`, so the check resolves by bare NAME via `taint_of_name`;
#         an over-broad name match would reject this.
run_sash 'str v=$(arg 1)
attest v [[ $v =~ ^[a-z/]+$ ]]
/bin/echo pwned > ${v:?no}
/bin/echo done' /dev/null
[[ $rc = 0 && $out = "done" && -z $rerr ]]; chk bkstp.4
# bkstp.5-bkstp.12 build a tainted array inside a `$( )` body so only the SSA
#         backstop can see it. The vehicle used to be `@{a:-$t}` -- a tainted
#         scalar spliced through a splat default. That spelling is SB4070 since
#         2026-09-30 (a disposition default is a literal), so the tainted
#         element is now spliced directly and the splat carries a literal `z`.
#         The array is declared outside the body on purpose: a body-declared
#         array false-positives SB5031, which would mask this.
# bkstp.5 NEGATIVE -- an unvouched tainted element reaches `echo`.
run_sash 'array a=( )
/bin/echo $(str t=$(getenv T); array b=( $t @{a:-z} ); echo $b[0])'
[[ $rc != 0 && -z $out && $err == *"taint backstop"* ]]; chk bkstp.5

# bkstp.6 POSITIVE — a CLEAN splat default still derives clean. Guards the
#         fix against the lazy over-approximation of tainting every splat
#         that carries any word at all.
run_sash 'array a=( )
/bin/echo $(array b=( @{a:-ok} ); echo $b[0])'
[[ $rc = 0 && $out = "ok" && -z $rerr ]]; chk bkstp.6
# bkstp.7 NEGATIVE -- DD#91 scopes `!` to its own subject's read. `@{a:!-z}`
#         vouches for the ARRAY, not for the unrelated `$t` beside it, so the
#         element's taint still counts.
run_sash 'array a=( )
/bin/echo $(str t=$(getenv T); array b=( $t @{a:!-z} ); echo $b[0])'
[[ $rc != 0 && -z $out && $err == *"taint backstop"* ]]; chk bkstp.7

# bkstp.8 POSITIVE — `@{a:-}` has `has_word` false and its `word` field is a
#         placeholder the program never wrote. Examining it would reject
#         valid programs for a value that does not exist.
run_sash 'array a=( x )
array b=( @{a:-} )
/bin/echo $b[0]'
[[ $rc = 0 && $out = "x" && -z $rerr ]]; chk bkstp.8

# bkstp.9 POSITIVE — DD#91 `${t:!~}`. `elide_taint` reads the flag and answers
#         Clean, so the array derives clean and the later read is allowed.
#         In a `$( )` body so the front end is not what decides it, and so the
#         pair below differs by exactly one character.
run_sash '/bin/echo $(str t=$(getenv HOME); array a=( ${t:!~} ); echo $a[0])'
[[ $rc = 0 && $out = "$HOME" && -z $rerr ]]; chk bkstp.9

# bkstp.10 NEGATIVE — the same elide WITHOUT the `!`. `:~` says what to do
#          when the value is absent; it vouches for nothing, so the array is
#          tainted and reading it is caught.
run_sash '/bin/echo $(str t=$(getenv HOME); array a=( ${t:~} ); echo $a[0])'
[[ $rc != 0 && -z $out && $err == *"taint backstop"* ]]; chk bkstp.10
# bkstp.11 NEGATIVE -- taint carried across a second splat. `c`'s own word is
#          the clean literal `z`, so `splat_taint` can only reach the right
#          answer via `b` itself, which is the one read this spec isolates.
run_sash 'array a=( )
/bin/echo $(str t=$(getenv HOME); array b=( $t @{a:-z} ); array c=( @{b:-z} ); echo $c[0])'
[[ $rc != 0 && -z $out && $err == *"taint backstop"* ]]; chk bkstp.11
# bkstp.12 POSITIVE -- `@{b:!-z}` is an attest-all for the splat's own read, so
#          `c` derives clean and keeps `b`'s elements. The counterpart to
#          bkstp.7: there `!` did NOT cover an unrelated word, here it DOES
#          cover the subject it names.
run_sash 'array a=( )
/bin/echo $(str t=$(getenv HOME); array b=( $t @{a:-z} ); array c=( @{b:!-z} ); echo $c[0])'
[[ $rc = 0 && $out = "$HOME" && -z $rerr ]]; chk bkstp.12
unset SASH_CFLAGS

set -e
tally
