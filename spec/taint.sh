#!/bin/bash
# spec/taint.sh — taint tracking tests (DD#24, DD#50, DD#51, DD#53, DD#61, DD#93)
set -eu
. common.sh
setup taint
plan=310
echo "1..$plan" >&3

set +e

# 24.1 getenv produces tainted var
run_sash 'x=$(getenv HOME); echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.1

# 24.2 arg produces tainted var
run_sash 'x=$(arg 1); echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.2

# 24.3 read produces tainted var
run_sash 'readline x; echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.3

# 24.4 mapfile produces tainted var
run_sash 'read arr; echo ${arr[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.4

# 24.5 external cmd produces tainted var
run_sash 'x=$(/bin/date); echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.5

# 24.6 read from a clean inline pipe → clean (sash has no here-strings — DD#83;
# `read x <<< "hello"` is written `echo hello | read x`)
run_sash 'echo hello | readline x; echo $x'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 24.6

# 24.7 user func capture → clean
run_sash 'fn greet { echo hello; }
x=$(greet)
echo $x'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 24.7

# 24.8 tainted var at use site → compile error
run_sash 'x=$(getenv PATH); /bin/ls $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.8

# 24.9 attest makes var clean
run_sash 'x=$(getenv HOME)
attest x [[ $x =~ ^/[a-zA-Z0-9/]+$ ]]
echo $x'
[[ $rc = 0 && -z $err ]]; chk 24.9

# 24.10 attest requires $VAR in test expression
run_sash 'x=$(getenv HOME)
attest x [[ 1 -eq 1 ]]
echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.10

# 24.11 attest requires $VAR on LHS
run_sash 'x=$(getenv HOME)
attest x [[ "foo" == $x ]]
echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.11

# 24.12 attest failure (unconditional) aborts at runtime
run_sash 'x=$(getenv HOME)
attest x [[ $x =~ ^NEVER_MATCH$ ]]
echo $x'
# attest fails at runtime → exits 1, no stdout
[[ $rc != 0 && -z $out ]]; chk 24.12

# 24.13 if attest failure → takes false branch, no abort. DD#121 R2: the else
# must leave the subject clean, so it reassigns from a literal.
run_sash 'x=$(getenv HOME)
if attest x [[ $x =~ ^NEVER$ ]]; then
  echo "$x"
else
  x=no
fi
echo "$x"'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk 24.13

# 24.14 re-assignment from tainted source re-taints
run_sash 'x=$(getenv HOME)
attest x [[ $x =~ ^/[a-zA-Z0-9/]+$ ]]
x=$(getenv USER)
echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.14

# 24.15 literals and $#, $?, $$ are clean
run_sash 'x="hello"; echo $x'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 24.15

# 24.16 attest test SHALL NOT allow external commands or arithmetic
run_sash 'x=$(getenv HOME)
attest x [[ $(echo yes) == yes ]]
echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.16

# 24.17 compiler SHALL NOT auto-attest known external output
run_sash 'd=$(/bin/date +%Y-%m-%d); echo $d'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.17

# 24.18 known hostname — still tainted without explicit attest
run_sash 'h=$(/bin/hostname); echo $h'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.18

# 24.19 getenv without attest — tainted
run_sash 'x=$(getenv HOME); echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.19

# 50.1 tainted global at call site → error
run_sash 'x=$(getenv HOME)
fn f { echo $x; }
f'
[[ $rc != 0 && -z $out && -n $err ]]; chk 50.1

# 51.1 SSA backstop — tainted use caught before codegen
run_sash 'x=$(arg 1); echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 51.1

# 53.1 $HOME is an env var — must use $(getenv HOME)
run_sash 'echo $HOME'
[[ $rc != 0 && -z $out && -n $err ]]; chk 53.1

# 53.2 only way to read env is getenv
run_sash 'x=$(getenv HOME); attest x [[ $x =~ ^/ ]]; echo $x'
[[ $rc = 0 && -z $err ]]; chk 53.2

# 53.3 getenv result is tainted
run_sash 'x=$(getenv HOME); echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 53.3

# 53.4 no mechanism to push to env
run_sash 'export FOO=bar'
[[ $rc != 0 && -z $out && -n $err ]]; chk 53.4

# 53.5 export keyword compile error
run_sash 'export PATH=/usr/bin'
[[ $rc != 0 && -z $out && -n $err ]]; chk 53.5

# 53.6 per-command env vars use /usr/bin/env
run_sash '/usr/bin/env FOO=bar /bin/echo hello'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 53.6

# 53.10 $0 rejected — use $(arg 0)
run_sash 'echo $0'
[[ $rc != 0 && -z $out && -n $err ]]; chk 53.10

# 53.7 $RANDOM rejected
run_sash 'echo $RANDOM'
[[ $rc != 0 && -z $out && -n $err ]]; chk 53.7

# 53.8 $BASH_VERSION rejected
run_sash 'echo $BASH_VERSION'
[[ $rc != 0 && -z $out && -n $err ]]; chk 53.8

# 53.9 $PATH requires getenv
run_sash 'echo $PATH'
[[ $rc != 0 && -z $out && -n $err ]]; chk 53.9

# ── DD#61: array taint construction restriction + attest-all ──────────
#
# Construction-restriction and attest-form checks are front-end (validate /
# type_check) and fire in every backend, so they run cross-mode. The
# attest-all *runtime* tests need mapfile + attest enforcement, which the c
# (SSA) oracle gained on 2026-08-20 — they are cross-mode too, and no test in
# this file is native-guarded.

# --- Cross-mode: construction restriction (a tainted value may not enter
#     an array via literal, += append, or a[i]= subscript). ---

# 60.1 array literal with a tainted element → reject
run_sash 'array a=( $(getenv HOME) ); echo ${a[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.1

# 60.2 array literal with multiple tainted elements (the reported bug) → reject
run_sash 'array a=( $(getenv HOME) $(getenv PATH) ); echo ${a[1]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.2

# 60.3 the reported bug WITH a scalar attest present must STILL reject
run_sash 'array a=( $(getenv HOME) $(getenv PATH) )
attest a [[ $a[0] =~ ^\S*$ ]]
echo $a[1]'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.3

# 60.4 += append of a tainted element → reject
run_sash 'array a=(one); a+=( $(getenv HOME) ); echo ${a[1]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.4

# 60.5 subscript-assign of a tainted value → reject
run_sash 'array a=(one two); a[0]=$(getenv HOME); echo ${a[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.5

# 60.6 mixed clean+tainted literal → reject
run_sash 'array a=( ok $(getenv HOME) ); echo ${a[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.6

# 60.7 map literal with a tainted value → reject
run_sash 'map m=( [k]=$(getenv HOME) ); echo ${m[k]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.7

# 60.8 concat embedding a tainted cmdsub in an array literal → reject
run_sash 'array a=( "pre$(getenv HOME)" ); echo ${a[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.8

# 60.13 mapfile from a file → tainted; use without attest rejected
run_sash '/usr/bin/tr "\n" "\0" < d60a.txt | read a; echo ${a[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.13

# 60.14 read -a → tainted
run_sash 'read a; echo ${a[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.14

# ad.4 an AUTO-declared mapfile array (no prior `array a=()`) is still tainted —
#      an element read without attest rejects (the front-end auto-declare
#      registers the collection but must NOT clear the taint).
run_sash '/usr/bin/tr "\n" "\0" < d60a.txt | read a; echo ${a[0]}'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk ad.4

# 60.32 tainted array iterated without attest-all → reject (negative for 60.25)
run_sash '/usr/bin/tr "\n" "\0" < d60b.txt | read a; for x in @a; do echo $x; done'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.32

# 60.33 tainted array joined without attest-all → reject (cmdsub-arg use site)
run_sash '/usr/bin/tr "\n" "\0" < d60b.txt | read a; echo $(join a)'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.33

# 60.34 DG (2026-09-10): "the read of the array must have a clean array, so
# no tainted maps." `map m = @{a:DISP}` is an ordinary READ of `a` — measurably
# identical to `join a` and `for x in @a` — so DD#24's no-propagation applies
# and a TAINTED array cannot be paired into a map at all. The gate that used to
# sit on the map now sits one step earlier, on the array, and it is the SAME
# gate rather than a new rule.
run_sash '/usr/bin/tr "\n=" "\0\0" < d60m.txt | read p
map m = @{p:~}
echo ${m[NAME]}'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.34


# 60.19 attest-all binder must be on the LHS (anti-tautology)
run_sash '/usr/bin/tr "\n" "\0" < d60b.txt | read a
attest x in a [[ "foo" == $x ]]
echo ${a[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.19

# 60.20 attest-all predicate must reference the binder
run_sash '/usr/bin/tr "\n" "\0" < d60b.txt | read a
attest x in a [[ 1 -eq 1 ]]
echo ${a[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.20

# 60.24 re-reading from a tainted source after attest-all re-taints
run_sash '/usr/bin/tr "\n" "\0" < d60b.txt | read a
attest x in a [[ $x =~ ^[a-z]+$ ]]
/usr/bin/tr "\n" "\0" < d60b.txt | read a
echo ${a[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.24

# 60.26 scalar attest applied to a (tainted) array → type error
run_sash '/usr/bin/tr "\n" "\0" < d60b.txt | read a
attest a [[ ${a[0]} =~ ^[a-z]+$ ]]
echo ${a[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.26

# 60.27 attest-all (1 binder) applied to a scalar → type error
run_sash 'x=$(getenv HOME); attest y in x [[ $y =~ ^/ ]]; echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.27

# 60.28 1-binder attest-all on a map → type error (needs two binders)
run_sash '/usr/bin/tr "\n=" "\0\0" < d60m.txt | read p_m
attest e in p_m [[ -n $e ]]
map m = @{p_m:~}
attest v in m [[ $v =~ ^[A-Za-z]+$ ]]
echo ${m[NAME]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.28

# 60.30 map attest-all: both binders must be validated (value omitted) → reject
run_sash '/usr/bin/tr "\n=" "\0\0" < d60m.txt | read p_m
attest e in p_m [[ -n $e ]]
map m = @{p_m:~}
attest k v in m [[ $k =~ ^[A-Z]+$ ]]
echo ${m[NAME]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.30

# --- clean-construction + attest-all runtime semantics.
#     The c oracle now implements mapfile + RedirectPush/Pop + AttestAll
#     (2026-08-20). ---
printf 'alpha\nbeta\n' > d60a.txt
printf 'alpha\nbeta\ngamma\n' > d60b.txt
printf 'alpha\n123\ngamma\n' > d60c.txt
printf '' > d60empty.txt
printf 'NAME=Alice\nCITY=Paris\n' > d60m.txt
printf 'NAME=Alice\nAGE=30\n' > d60m2.txt

# 60.9 array of clean literals
run_sash 'array a=( one two ); echo ${a[0]}'
[[ $rc = 0 && $out = "one" && -z $err ]]; chk 60.9

# 60.10 escape hatch: array built from a attested scalar
run_sash 'h=$(getenv HOME)
attest h [[ $h =~ ^/ ]]
array a=( "$h" ); echo ${a[0]}'
[[ $rc = 0 && -n $out && -z $err ]]; chk 60.10

# 60.11 append of a clean element
run_sash 'array a=(one); a+=( two ); echo ${a[1]}'
[[ $rc = 0 && $out = "two" && -z $err ]]; chk 60.11

# 60.12 subscript-assign of a clean value
run_sash 'array a=(one two); a[0]=zero; echo ${a[0]}'
[[ $rc = 0 && $out = "zero" && -z $err ]]; chk 60.12

# 60.15 attest-all clears taint; element readable (runtime pass)
run_sash '/usr/bin/tr "\n" "\0" < d60b.txt | read a
attest x in a [[ $x =~ ^[a-z]+$ ]]
echo ${a[1]}'
[[ $rc = 0 && $out = "beta" && -z $err ]]; chk 60.15

# 60.16 attest-all then read a different element
run_sash '/usr/bin/tr "\n" "\0" < d60b.txt | read a
attest x in a [[ $x =~ ^[a-z]+$ ]]
echo ${a[2]}'
[[ $rc = 0 && $out = "gamma" && -z $err ]]; chk 60.16

# 60.17 attest-all runtime failure: one element fails → exit 1, no stdout
run_sash '/usr/bin/tr "\n" "\0" < d60c.txt | read a
attest x in a [[ $x =~ ^[a-z]+$ ]]
echo ${a[0]}'
[[ $rc != 0 && -z $out ]]; chk 60.17

# 60.18 if-form attest-all failure → false branch, no abort. DD#121 R2: the
# else resolves the array with a clean literal.
run_sash '/usr/bin/tr "\n" "\0" < d60c.txt | read a
if attest x in a [[ $x =~ ^[a-z]+$ ]]; then echo ${a[0]}; else a=(dflt); fi
echo done'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk 60.18

# 60.21 empty array → vacuous pass (∀ over ∅ is true)
run_sash '/usr/bin/tr "\n" "\0" < d60empty.txt | read a
attest x in a [[ $x =~ ^[a-z]+$ ]]
echo done'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk 60.21

# 60.22 binder shadows outer var; outer untouched afterward
run_sash 'x=outer; /usr/bin/tr "\n" "\0" < d60b.txt | read a
attest x in a [[ $x =~ ^[a-z]+$ ]]
echo $x'
[[ $rc = 0 && $out = "outer" && -z $err ]]; chk 60.22

# 60.23 attest-all with a unary predicate on the binder
run_sash '/usr/bin/tr "\n" "\0" < d60b.txt | read a
attest x in a [[ -n $x ]]
echo ${a[0]}'
[[ $rc = 0 && $out = "alpha" && -z $err ]]; chk 60.23

# 60.25 attest-all enables iteration over the cleaned array
run_sash '/usr/bin/tr "\n" "\0" < d60b.txt | read a
attest x in a [[ $x =~ ^[a-z]+$ ]]
for w in @a; do echo $w; done'
[[ $rc = 0 && $out = $'alpha\nbeta\ngamma' && -z $err ]]; chk 60.25

# 60.35 …and the consequence, stated positively: once the array is vouched
# for, the map built from it is CLEAN — no map-level attest-all, because there
# is no longer any way for a map to be tainted. This is what makes 60.34 a
# relocation of the gate rather than a hole in it.
run_sash '/usr/bin/tr "\n=" "\0\0" < d60m.txt | read p
attest e in p [[ -n $e ]]
map m = @{p:~}
for k in $(keys m); do echo "$k=${m[$k]}"; done'
[[ $rc = 0 && $out = $'NAME=Alice\nCITY=Paris' && -z $err ]]; chk 60.35

# 60.29 map attest-all clears, value readable (runtime pass)
run_sash '/usr/bin/tr "\n=" "\0\0" < d60m.txt | read p_m
attest e in p_m [[ -n $e ]]
map m = @{p_m:~}
attest k v in m [[ $k =~ ^[A-Z]+$ && $v =~ ^[A-Za-z]+$ ]]
echo ${m[NAME]}'
[[ $rc = 0 && $out = "Alice" && -z $err ]]; chk 60.29

# 60.36 attested map: keys + dynamic subscript idiom iterates pairs
run_sash '/usr/bin/tr "\n=" "\0\0" < d60m.txt | read p_m
attest e in p_m [[ -n $e ]]
map m = @{p_m:~}
attest k v in m [[ $k =~ ^[A-Z]+$ && $v =~ ^[A-Za-z]+$ ]]
for k in $(keys m); do echo "$k=${m[$k]}"; done'
[[ $rc = 0 && $out = $'NAME=Alice\nCITY=Paris' && -z $err ]]; chk 60.36

# 60.31 map attest-all runtime failure: a value fails its predicate → exit 1
run_sash '/usr/bin/tr "\n=" "\0\0" < d60m2.txt | read p_m
attest e in p_m [[ -n $e ]]
map m = @{p_m:~}
attest k v in m [[ $k =~ ^[A-Z]+$ && $v =~ ^[A-Za-z]+$ ]]
echo ${m[NAME]}'
[[ $rc != 0 && -z $out ]]; chk 60.31

# ad.1 mapfile into an UNDECLARED array (no prior `array a=()`) auto-declares
#      it (mirrors argv, DG 2026-07-08). `$(len a)` was a codegen ICE
#      (`undefined array: a`); now it is the count.
run_sash '/usr/bin/tr "\n" "\0" < d60a.txt | read a; echo $(len a)'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk ad.1
# ad.2 same for `read -a` into an undeclared array
run_sash 'read a < d60a.txt; echo $(len a)'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk ad.2
# ad.3 attest-all + element read on an AUTO-declared mapfile array (the
#      front-end must register the target as a collection, else `attest e in
#      a` sees a scalar → SB4030). Mirrors the explicit-decl path.
run_sash '/usr/bin/tr "\n" "\0" < d60a.txt | read a; attest e in a [[ -n $e ]]; echo ${a[0]}'
[[ $rc = 0 && $out = "alpha" && -z $err ]]; chk ad.3

# tilde.1 unquoted leading `~` → focused SB4036 (not the misleading $HOME
# SB5018); DD#53 no home-directory magic. Compile-time reject, all modes.
run_sash 'echo ~'
[[ $rc != 0 && -z $out && $err == *SB4036* ]]; chk tilde.1
# tilde.2 `~/path` → same SB4036
run_sash 'echo ~/x'
[[ $rc != 0 && -z $out && $err == *SB4036* ]]; chk tilde.2
# tilde.3 quoted / mid-word tilde stays a literal (unaffected)
run_sash "echo '~' \"~\" a~b"
[[ $rc = 0 && $out = "~ ~ a~b" && -z $err ]]; chk tilde.3

# 24.20 a failed attest names the variable and shows its value. Regression
#       for the terse `attest: validation failed`.
run_sash 'x=$(getenv HOME)
attest x [[ $x =~ ^NEVER$ ]]
echo $x'
[[ $rc = 1 && $rerr == *"attest: x = '"* && $rerr == *"failed the gate" ]]; chk 24.20
# dd78.1-dd78.7 DD#78 closed the `:-` taint hole with a read discipline: the
# default was an interpolating word, so the checker had to walk it and prove
# whatever it expanded to was clean. DG retired that on 2026-09-30 -- "let's
# change it to match the `:?` so eager/side-effects are a non-issue" -- and the
# operand is a literal. Every program below is still rejected; the code moved
# from SB5020/SB5035 to SB4070, and the read discipline it needed is gone.
# dd78.1 a tainted variable in a `:-` default
run_sash 'y=$(getenv HOME); echo ${x:-$y}'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk dd78.1
# dd78.2 a tainted source inline
run_sash 'echo ${x:-$(getenv HOME)}'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk dd78.2
# dd78.3 the argv source
run_sash 'echo ${x:-$(arg 1)}'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk dd78.3
# dd78.4 a CLEAN intrinsic default used to be the accepted case -- the whole
# point of a read discipline is that it lets safe expansions through. There is
# no such category now: the operand is literal, so `$(getcwd)` is rejected
# exactly like `$(getenv HOME)`. What you write instead is a conditional.
run_sash 'x=hi; echo ${x:-$(getcwd)}'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk dd78.4
# dd78.5 a literal default is accepted (x unset → the default)
run_sash 'echo ${x:-plain}'
[[ $rc = 0 && $out = "plain" && -z $err ]]; chk dd78.5
# dd78.6 `$(random)` had its own code (SB5035) because it is clean but
# STATEFUL -- eager evaluation advanced the PRNG on the untaken path. The
# general rule subsumes it and SB5035 has been deleted.
run_sash 'echo ${x:-$(random)}'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk dd78.6
# dd78.7 a tainted default cannot sneak through a store either. This used to
# depend on DD#24 (the default is a read, so the assignment rejects); it now
# needs no taint reasoning at all.
run_sash 'y=$(getenv HOME); z=${x:-$y}; echo $z'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk dd78.7
# dd78.8-dd78.12 DD#78's read discipline does not reach the `:?` MESSAGE,
# because the message is never read. It is a literal, and a `$` in it is SB4070
# (see vm.req4-vm.req10). These specs pin the taint consequence: a tainted
# value cannot escape through an abort message by any route — the interpolating
# spellings are rejected outright, and the one spelling that IS allowed, a
# single-quoted message, is literal text and still reads nothing.
# dd78.8 a tainted var in a message is SB4070 — rejected before taint is even
#        a question (it used to be SB5050: `h` really was dead)
run_sash 'str h=$(getenv HOME)
str y=""
str z=${y:?$h}
echo "[$z]"'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk dd78.8
# dd78.9 an inline tainted source is likewise SB4070, so there is no route by
#        which `$(getenv …)` reaches stderr through a message
run_sash 'str y=""
str z=${y:?$(getenv HOME)}
echo "[$z]"'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk dd78.9
# dd78.10 the `:!?` vouching form does not buy an exemption — `!` covers the
#        subject read, and the message is still parsed as a message
run_sash 'str h=$(getenv HOME)
attest h [[ -n $h ]]
str y=""
str z=${y:!?$h}
echo "[$z][$h]"'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk dd78.10
# dd78.11 the quoted escape is NOT a taint hole: `$h` inside `'…'` is two
#        literal characters, so a tainted `h` is not read and nothing escapes
run_sash 'str h=$(getenv HOME)
attest h [[ -n $h ]]
str y=""
str z=${y:?'"'"'$h'"'"'}
echo "[$z][$h]"'
[[ $rc != 0 && -z $out ]] \
  && { ! in_modes native || [[ $rerr == *': $h' ]]; }; chk dd78.11
# dd78.12 a plain message is unchanged
run_sash 'str y=""
str z=${y:?plain}
echo "[$z]"'
[[ $rc != 0 && -z $out ]] \
  && { ! in_modes native || [[ $rerr == *": plain" ]]; }; chk dd78.12

# ── DD#24 no propagation (journal 036 `x=$1` is a compile error): reading a
# stored tainted variable in a scalar assignment RHS is a rejected read, uniform
# with every other read site. Assignment *from a source* is the only way in;
# attest at the source, not on a copy. Regression: scalar assignment used to
# silently PROPAGATE taint (caught only at a later sink). ──
# np.1 z=$y (y tainted) is a rejected read, not a silent propagate
run_sash 'y=$(getenv HOME); z=$y'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk np.1
# np.2 quoted copy is equally a read
run_sash 'y=$(getenv HOME); z="$y"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk np.2
# np.3 a tainted read buried in a concat is rejected
run_sash 'y=$(getenv HOME); z="pre${y}post"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk np.3
# np.4 string `+=` append of a tainted read is rejected
run_sash 'y=$(getenv HOME); s=clean; s+=$y'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk np.4
# np.5 attest at the SOURCE, then the copy is a clean read (the DD#24 idiom).
run_sash 'y=$(getenv HOME); attest y [[ -n $y ]]; z=$y; echo "$z"'
[[ $rc = 0 && -z $err ]]; chk np.5
# np.6 assignment straight from a source is allowed (taints the target).
run_sash 'z=$(getenv HOME); attest z [[ -n $z ]]; echo "$z"'
[[ $rc = 0 && -z $err ]]; chk np.6
# np.7 a clean copy chain is unaffected
run_sash 'a=5; b=$a; c=$b; echo $c'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk np.7

# ── Multi-statement / pipeline command substitution taint (DG 2026-08-19). The
# whole body is scanned for externals — a pipeline/multi-stmt body that runs an
# external captures TAINTED output (was a hole: only stmts.first() was checked,
# so `$(a | b)` / `$(x=1; cat f)` bypassed the scan). Compile-time, all modes. ──
# mcs.1 pipeline body with an external stage → tainted use rejected
run_sash 'x=$(cat /etc/hostname | tr a b); echo $x'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk mcs.1
# mcs.2 multi-statement body running an external → tainted
run_sash 'echo $(y=1; cat /etc/hostname)'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk mcs.2
# mcs.3 a purely-clean body (builtins only) is NOT tainted — capture is usable
run_sash 'x=$(echo a; echo b); echo "[$x]"'
[[ $rc = 0 && $out == $'[a\nb]' && -z $err ]]; chk mcs.3
# mcs.4 attesting the tainted pipeline capture makes it usable
run_sash 'x=$(/bin/echo hi | /usr/bin/tr h H); attest x [[ -n $x ]]; echo $x'
[[ $rc = 0 && $out = "Hi" && -z $err ]]; chk mcs.4

# ── DD#24: a `$( )` that READS already-tainted data is a use-site in EVERY
# value position, not just argument position (a laundering-bypass class). A
# tainted `argv 1` snapshot joined without attest-all must be rejected (SB5020)
# wherever the `$(join …)` appears. Previously only argument/for-word position
# was checked; assignment RHS / subscript / array-literal / test / redirect /
# nested all silently compiled and leaked the args at runtime.
run_sash 'argv 1 a; x=$(join a); echo $x' a b
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.37
run_sash 'argv 1 a; array b=(); b[0]=$(join a); echo ${b[0]}' a b
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.38
run_sash 'argv 1 a; array c=($(join a)); echo hi' a b
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.39
run_sash 'argv 1 a; [[ $(join a) == x ]] && echo hi' a b
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.40
run_sash 'argv 1 a; x=$(join a); echo z > $x' a b
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.41
run_sash 'argv 1 a; s=$(echo $(join a)); echo $s' a b
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.42
# 60.43 bare-name reader `join a` (not `@a`) reads element data too (DD#24)
run_sash 'argv 1 a; x=$(join a); echo $x' a b
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.43
# 60.44/60.45 a direct tainted scalar in a `[[ ]]` operand / redirect target
# is a read too — these positions were entirely unchecked before.
run_sash 'x=$(getenv HOME); [[ $x == y ]] && echo z'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.44
run_sash 'x=$(getenv HOME); echo hi > $x'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.45
# 60.46 control: a attested-all snapshot joined is clean and runs.
run_sash 'argv 1 a; attest x in a [[ -n $x ]]; echo $(join a)' one two
[[ $rc = 0 && $out = "one two" && -z $err ]]; chk 60.46

# ── Gap-fill: same laundering-bypass class across read-form × source × more
# positions, so a regression in any one path is caught (DG 2026-08-26).
# 60.47 `echo $t` (a tainted SCALAR through a cmdsub) in assignment RHS
run_sash 'x=$(getenv HOME); y=$(echo $x); echo $y'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.47
# 60.48 mapfile-sourced (not argv) tainted array joined in assignment RHS
run_sash '/usr/bin/tr "\n" "\0" < d60b.txt | read a; x=$(join a); echo $x'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.48
# 60.49 the same relocation in a value position: `keys` of a map built from an
# UNVOUCHED array is rejected at the pairing, before `keys` is ever reached.
run_sash '/usr/bin/tr "\n=" "\0\0" < d60m.txt | read p
map m = @{p:~}
array k=($(keys m)); echo hi'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.49
# DD#114: a subscript INDEX is a read of whatever it expands, in every
# position. DD#24 lists `arr[$x]=v` among the use-sites of `x` and says the
# list is position-uniform; `validate`'s three taint walkers all matched
# `Subscript { name, .. }` and dropped the index. Two positions accepted a
# tainted index outright; the rest were caught only by the SSA backstop,
# whose message has no SB code and no source location.
# 60.60 external command argument
run_sash 'array a=(x y)
i=$(printf 1)
/bin/echo "${a[$i]}"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.60
# 60.61 builtin argument
run_sash 'array a=(x y)
i=$(printf 1)
echo "${a[$i]}"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.61
# 60.62 assignment RHS — accepted before the fix, and the value reached
# `echo` with no diagnostic at all.
run_sash 'array a=(x y)
i=$(printf 1)
s="${a[$i]}"
echo "$s"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.62
# 60.63 `[[ … ]]` operand — also accepted before the fix.
run_sash 'array a=(x y)
i=$(printf 1)
if [[ -n ${a[$i]} ]]; then echo yes; fi'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.63
# 60.64 the write index — DD#24's own canonical example, `arr[$x]=v`.
run_sash 'array a=(x y)
i=$(printf 1)
a[$i]=v
echo "${a[1]}"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.64
# 60.65 …and the braced spelling of it
run_sash 'array a=(x y)
i=$(printf 1)
a[${i}]=v
echo "${a[1]}"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.65
# 60.66 a map key is the same read
run_sash 'map m=([k]=v)
k=$(printf k)
echo "${m[$k]}"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.66
# 60.67 the diagnostic names the INDEX variable, not the container — the
# container is clean here, so blaming it would send the author to the wrong
# `attest`.
[[ $err == *'$k'* ]]; chk 60.67
# 60.68 …and points at the line the index is on. The index sub-parser runs a
# fresh Scanner over the index text alone, so its spans used to be 1:1
# relative to that substring: every one of these reported line 1.
[[ $err == *:3:* ]]; chk 60.68
# 60.69 a clean index is untouched
run_sash 'array a=(x y)
int i=1
echo "${a[$i]}"'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 60.69
# 60.72 the arith attest's target is an int, and the compiler knows it. It did
# not: `(( n = s ))` on its own parses as an assignment, so `check_assignment`
# declared the target, but wrapped in `attest` the statement stays a Command
# and nothing entered `n` in the type environment at all. It defaulted to
# String, which let a string intrinsic run on an int — the silent int→string
# render DD#105 exists to reject.
run_sash 'i=$(printf 5)
attest i (( n = i ))
echo $(strlen $n)'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 60.72

# 60.70 …and an attested one is the documented way through. `attest i i?`
# alone is not enough any more: it declassifies but leaves `i` a string, and
# an array index must be int-typed (DD#80). DD#93 Part D's arith form is the
# one that converts, and `n` is the int it converted to.
run_sash 'array a=(x y)
i=$(printf 1)
attest i (( n = i ))
echo "${a[$n]}"'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 60.70
# 60.71 a literal index never involved a read
run_sash 'map m=([k]=v)
echo "${m[k]}"'
[[ $rc = 0 && $out = "v" && -z $err ]]; chk 60.71
# 60.50 bare-name `join a` reader reached through a `[[ ]]` operand
run_sash 'argv 1 a; [[ $(join a) == x ]] && echo hi' a b
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.50
# 60.51 a tainted array subscript `${a[0]}` (value-flow, not cmdsub) in a test
run_sash '/usr/bin/tr "\n" "\0" < d60b.txt | read a; [[ ${a[0]} == x ]] && echo hi'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.51
# 60.52 bare-name `join a` reader as a redirect target (routed through a
# scalar — a direct `$(…)` redirect *target* is now rejected structurally at
# parse, SB4049, so the taint of the array-join source is exercised via `$x`)
run_sash 'argv 1 a; x=$(join a); echo hi > $x' a b
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.52
# 60.53 `echo $t` (tainted scalar) through a cmdsub in a `[[ ]]` operand
run_sash 'x=$(getenv HOME); [[ $(echo $x) == y ]] && echo z'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.53
# 60.54 control (positive): a CLEAN `$(echo …)` capture still compiles & runs
# — guards the assignment-RHS cmdsub read-check against over-rejection.
run_sash 'y=$(echo hello); echo $y'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 60.54
# 60.55 control (positive): a CLEAN var in a `[[ ]]` operand still runs — guards
# the new Test-operand read-check against over-rejecting clean values.
run_sash 'x=hello; [[ $x == hello ]] && echo yes'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 60.55

# ── attest-fallback: `attest V [[t]] || V=DEFAULT` (DG 2026-09-01) ──────────
# A attest as the left operand of `&&`/`||` is a *caught* gate (like `if attest`),
# not a bare abort. `attest V [[t]] || V=dflt` = "V validated, else the default"
# — a clean, usable value either way. The fallback is trusted, not re-validated
# (caveat programmer). Attest is a runtime construct.
# bfb.1 valid input passes the gate → V keeps its value (was wrongly the
#       default before the fix)
run_sash 'v=$(arg 1)
attest v [[ $v =~ ^[0-9]+$ ]] || v=8080
/bin/echo "$v"' 5000
[[ $rc = 0 && $out = "5000" && -z $rerr ]]; chk bfb.1

# bfb.2 invalid input fails the gate → the default (no abort)
run_sash 'v=$(arg 1)
attest v [[ $v =~ ^[0-9]+$ ]] || v=8080
/bin/echo "$v"' abc
[[ $rc = 0 && $out = "8080" && -z $rerr ]]; chk bfb.2

# bfb.3 empty input fails the gate → the default
run_sash 'v=$(arg 1)
attest v [[ $v =~ ^[0-9]+$ ]] || v=8080
/bin/echo "$v"' ""
[[ $rc = 0 && $out = "8080" && -z $rerr ]]; chk bfb.3

# bfb.4 V is clean and reusable after the fallback (both branches clean)
run_sash 'v=$(arg 1)
attest v [[ $v =~ ^[0-9]+$ ]] || v=8080
/bin/echo "$v $v"' abc
[[ $rc = 0 && $out = "8080 8080" && -z $rerr ]]; chk bfb.4

# DD#121 R2 superseded the `attest … && CMD` spelling: `&&` has no failure
# branch, so it can never leave the subject clean. bfb.5/6 now use the `if`
# form with a resolving `else`; bfb.7 pins the rejection of the old spelling.
# bfb.5 the guarded command runs only on pass
run_sash 'v=$(arg 1)
if attest v [[ $v =~ ^[0-9]+$ ]]; then /bin/echo "valid $v"; else v=none; fi
/bin/echo done' 42
[[ $rc = 0 && $out = $'valid 42\ndone' && -z $rerr ]]; chk bfb.5

# bfb.6 on fail: the guarded command is skipped, execution continues (no abort)
run_sash 'v=$(arg 1)
if attest v [[ $v =~ ^[0-9]+$ ]]; then /bin/echo "valid $v"; else v=none; fi
/bin/echo reached' abc
[[ $rc = 0 && $out = "reached" && -z $rerr ]]; chk bfb.6

# bfb.7 DD#121 R2: `attest … && CMD` is rejected — no failure branch exists
run_sash 'v=$(arg 1)
attest v [[ $v =~ ^[0-9]+$ ]] && /bin/echo "valid $v"
/bin/echo done'
[[ $rc != 0 && -z $out && $err == *SB5089* ]]; chk bfb.7

# bfb.8 …and so is a caught `if attest` with no `else` at all
run_sash 'v=$(arg 1)
if attest v [[ $v =~ ^[0-9]+$ ]]; then /bin/echo "valid $v"; fi'
[[ $rc != 0 && -z $out && $err == *SB5089* ]]; chk bfb.8

# bfb.9 …and an `else` that falls through without resolving the subject
run_sash 'v=$(arg 1)
if attest v [[ $v =~ ^[0-9]+$ ]]; then /bin/echo "valid $v"; else /bin/echo bad; fi'
[[ $rc != 0 && -z $out && $err == *SB5089* ]]; chk bfb.9

# bfb.10 leaving the branch resolves it — nothing reaches the merge
run_sash 'v=$(arg 1)
if attest v [[ $v =~ ^[0-9]+$ ]]; then /bin/echo "valid $v"; else exit 3; fi' 42
[[ $rc = 0 && $out = "valid 42" && -z $rerr ]]; chk bfb.10

# ── DD#121 R1: an attest must be adjacent to the operation that taints ──
# bfb.11 a statement between the taint source and its attest is rejected
run_sash 'v=$(arg 1)
str u=x
attest v [[ -n $v ]]
/bin/echo "$v$u"'
[[ $rc != 0 && -z $out && $err == *SB5088* ]]; chk bfb.11

# bfb.12 …and the same program with the attest moved up compiles
run_sash 'v=$(arg 1)
attest v [[ -n $v ]]
str u=x
/bin/echo "$v$u"' 42
[[ $rc = 0 && $out = "42x" && -z $rerr ]]; chk bfb.12

# bfb.13 R3 (unchanged): `:!` is still the way to read without attesting
run_sash 'v=$(arg 1)
str u=x
/bin/echo "${v:!}$u"' 42
[[ $rc = 0 && $out = "42x" && -z $rerr ]]; chk bfb.13

# bfb.14 attesting a name that is already clean stays a harmless no-op
run_sash 'str v=hello
attest v [[ -n $v ]]
/bin/echo "$v"'
[[ $rc = 0 && $out = "hello" && -z $rerr ]]; chk bfb.14

# ── DD#121: a branch no longer leaks a removal past the merge ──
# bfb.15 the attest inside the `if` cleans `a` only on that path
run_sash 'argv a
str c=maybe
if [[ $c == yes ]]; then
  argv a
  attest v in a [[ -n $v ]]
fi
for v in @a; do /bin/echo "$v"; done'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk bfb.15

# bfb.16 …and a zero-trip loop body leaks nothing either
run_sash 'argv a
str c=maybe
while [[ $c == yes ]]; do
  argv a
  attest v in a [[ -n $v ]]
  break
done
for v in @a; do /bin/echo "$v"; done'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk bfb.16

# ── SB5067 under DD#121 R1: a function may not LEAVE a global tainted ──
# The rule is net-effect, not "clean at exit": a body inherits the top-level
# tainted set and R1 forbids cleaning an inherited taint, so the predicate is
# "the exit set may not contain a global the body did not inherit". Because R1
# pins the attest to the statement immediately below the taint, there is no
# ordering question about where in the body the repair may sit.
# bfb.17 taint a global and attest it adjacently — the caller reads it clean
run_sash 'array g=()
fn f {
    argv 1 g
    attest v in g [[ -n $v ]]
}
f
for v in @g { /bin/echo $v }' one two
[[ $rc = 0 && $out = $'one\ntwo' && -z $rerr ]]; chk bfb.17

# bfb.18 …and the same body without the attest is SB5067
run_sash 'array g=()
fn f {
    argv 1 g
}
f
for v in @g { /bin/echo $v }'
[[ $rc != 0 && -z $out && $err == *SB5067*leaves\ the\ global* ]]; chk bfb.18

# bfb.19 the in-body attest is a real runtime gate, not a compile-time waiver
run_sash 'array g=()
fn f {
    argv 1 g
    attest v in g [[ -n $v ]]
}
f
for v in @g { /bin/echo $v }' "" two
[[ $rc != 0 && $rerr == *"attest: g failed the gate"* ]]; chk bfb.19

# bfb.20 one branch attesting does not excuse another that leaves it tainted
run_sash 'array g=()
fn f {
    if (( 1 == 1 )) {
        argv 1 g
        attest v in g [[ -n $v ]]
    } else {
        argv 1 g
    }
}
f
for v in @g { /bin/echo $v }'
[[ $rc != 0 && -z $out && $err == *SB5067* ]]; chk bfb.20

# bfb.21 re-tainting a global the top level already attested clean is a net
# insert — the caller's clean view would otherwise be silently falsified
run_sash 'array g=()
argv 1 g
attest v in g [[ -n $v ]]
fn f {
    argv 1 g
}
f
for v in @g { /bin/echo $v }'
[[ $rc != 0 && -z $out && $err == *SB5067* ]]; chk bfb.21

# bfb.22 a body that never touches an already-tainted global is not blamed
run_sash 'array g=()
fn f {
    /bin/echo hi
}
f
argv 1 g
attest v in g [[ -n $v ]]
for v in @g { /bin/echo $v }' one
[[ $rc = 0 && $out = $'hi\none' && -z $rerr ]]; chk bfb.22

# ── SB5067 extended to `$( … )` bodies (DG, 2026-09-30) ──────────────────
# DD#86 calls a `$( … )` body an anonymous function body, and sash inlines it
# rather than forking a subshell, so a write to an enclosing name PERSISTS.
# The same net-effect rule therefore applies: a body may not LEAVE a global
# tainted that it did not inherit. Before this, the function form was a located
# SB5067 and the identical `$( )` form was silent.
# bfb.23 the shape that was silent: global read before, tainted inside a
# body, never read after — so neither SB5050 nor the SSA backstop sees it
run_sash 'str t=clean
/bin/echo $t
str q=$(t=$(getenv HOME); echo x)
/bin/echo $q'
[[ $rc != 0 && -z $out && $err == *SB5067*leaves\ the\ global* ]]; chk bfb.23
# bfb.24 `:!` at the read does NOT rescue it. SB5067 is a BODY-EXIT rule and
# never examines the read site, so the bypass is evaluated too late to matter
# — which is what closes the `:!` exception for anonymous bodies too.
run_sash 'str t=clean
str q=$(t=$(getenv HOME); echo x)
/bin/echo $q
/bin/echo $t:!'
[[ $rc != 0 && -z $out && $err == *SB5067* ]]; chk bfb.24
# bfb.25 the repair is the same one a function body gets: an ADJACENT in-body
# attest. The write still escapes (DD#86), and the attest is a real runtime
# gate, not a compile-time waiver.
run_sash 'str t=clean
str q=$(t=$(getenv HOME); attest t [[ -n $t ]]; echo x)
/bin/echo $q
/bin/echo $t'
[[ $rc = 0 && $out == x* && -z $err ]]; chk bfb.25
# bfb.26 a NON-adjacent in-body attest is R1 (SB5088), not SB5067 — the two
# rules stay distinct inside a body exactly as they do inside a function
run_sash 'str t=clean
str q=$(t=$(getenv HOME); echo mid; attest t [[ -n $t ]]; echo x)
/bin/echo $q
/bin/echo $t'
[[ $rc != 0 && -z $out && $err == *SB5088* ]]; chk bfb.26
# bfb.27 control: a name the body INTRODUCES is body-local (DD#86), so
# tainting it is not a global leak and must stay quiet
run_sash 'str q=$(str L=$(getenv HOME); attest L [[ -n $L ]]; echo $L)
/bin/echo $q'
[[ $rc = 0 && -n $out && -z $err ]]; chk bfb.27
# bfb.28 a `$( )` inside a FUNCTION body reports exactly once, attributed to
# the body. `collect_global_taint_sites` is structural, so the enclosing
# function does not gather the same site, and the cmdsub's clone-and-discard
# keeps it out of the function's exit set.
run_sash 'str t=clean
/bin/echo $t
fn f {
    str q=$(t=$(getenv HOME); echo x)
    /bin/echo $q
}
f'
[[ $rc != 0 && -z $out && $(grep -c SB5067 <<<"$err") = 1 ]]; chk bfb.28

# ── DD#91: the `:!` taint-off MODIFIER (orthogonal to the dispositions) ──
# A sanctioned per-use bypass — use a tainted value without a attest predicate
# (the middle ground between `attest` and `--no-taint`).
# 91.1 tainted `${x:!}` compiles & runs (the bypass).
run_sash 'x=$(getenv HOME); /bin/echo ${x:!}'
[[ $rc = 0 && -n $out && -z $err ]]; chk 91.1
# 91.2 control: `${x:~}` on the SAME tainted value is rejected (SB5020) — a
# compile-time check, so it holds in every backend.
run_sash 'x=$(getenv HOME); /bin/echo ${x:~}'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 91.2
# 91.3 brace-less `$x:!` bypasses too
run_sash 'x=$(getenv HOME); /bin/echo $x:!'
[[ $rc = 0 && -n $out && -z $err ]]; chk 91.3
# 91.4 the `:!` value propagates CLEAN into an array literal
run_sash 'x=$(getenv HOME); array a=(${x:!} lit); /bin/echo @a:?e'
[[ $rc = 0 && -n $out && -z $err ]]; chk 91.4
# 91.5 tainted array splat `@a:!~` bypasses attest-all
run_sash 'argv 1 a; /usr/bin/printf "%s" @a:!~' HELLO
[[ $rc = 0 && $out = "HELLO" && -z $err ]]; chk 91.5
# 91.6 braced `@{a:!~}` likewise
run_sash 'argv 1 a; /usr/bin/printf "%s" @{a:!~}' HI
[[ $rc = 0 && $out = "HI" && -z $err ]]; chk 91.6
# 91.7 control: tainted `@a:?e` (no trust) is rejected (SB5020)
run_sash 'argv 1 a; /bin/echo @a:?e' X
[[ $rc != 0 && $err == *SB5020* ]]; chk 91.7
# 91.8 `!` must be followed by a disposition or nothing — `${x:!foo}` is
# malformed and must NOT quietly degrade to a trusted bare `${x}`
run_sash 'x=hi; /bin/echo ${x:!foo}'
[[ $rc != 0 && -z $out && $err == *SB4051* ]]; chk 91.8
# 91.9 `@{a:!WORD}` takes no word — SB5033 (array)
run_sash 'array a=(); /bin/echo @{a:!x}'
[[ $rc != 0 && -z $out && $err == *SB5033* ]]; chk 91.9
# 91.10 a scalar assignment is now LEGAL — with the elide split out, `:!` is
# just "the value, trusted", so it belongs anywhere a bare expansion does.
# (This reverses the original DD#91 rule; see its 2026-09-09 addendum.)
run_sash 'x=hi; z=${x:!}; /bin/echo $z'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 91.10
# 91.11 `:!` on a CLEAN var is a harmless no-op taint-off (just the value)
run_sash 'x=hi; /bin/echo ${x:!}'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 91.11

# ── DD#91 addendum: `!` is a MODIFIER, `-`/`?`/`~` are DISPOSITIONS ──────────
# `-`/`?`/`~` say what to do when the value is ABSENT; `!` says how to treat
# the value we HAVE. Grammar: `: [!] [ -WORD | ?MSG | ~ ]`. `!` must lead,
# which the grammar forces: `-`/`?` words run greedily to `}`, so `${u:-foo!}`
# is the word "foo!" and a trailing modifier could never be seen.

# 91.12 `:!` alone is "bare, but trusted" — an EMPTY value contributes an empty
# argument, exactly as bare `$u` does. It does NOT elide any more.
run_sash 'u=""
/usr/bin/printf "[%s]" A ${u:!} B'
[[ $rc = 0 && $out = "[A][][B]" && -z $err ]]; chk 91.12

# 91.13 …and `:!~` is the old `:!`: trusted AND elided when empty.
run_sash 'u=""
/usr/bin/printf "[%s]" A ${u:!~} B'
[[ $rc = 0 && $out = "[A][B]" && -z $err ]]; chk 91.13

# 91.14 `:!-WORD` — trusted, with a default when empty
run_sash 'u=""
/bin/echo ${u:!-fallback}'
[[ $rc = 0 && $out = "fallback" && -z $err ]]; chk 91.14

# 91.15 `:!-WORD` takes the VALUE when non-empty (the `:-` half still applies)
run_sash 'u=set
/bin/echo ${u:!-fallback}'
[[ $rc = 0 && $out = "set" && -z $err ]]; chk 91.15

# 91.16 `:!?MSG` — trusted, aborts when empty
run_sash 'u=""
/bin/echo ${u:!?empty please}'
[[ $rc != 0 && -z $out ]]; chk 91.16

# 91.17 the whole point: a TAINTED value is trusted through a combination too,
# not just through bare `:!`
run_sash 'x=$(getenv HOME); /bin/echo ${x:!-none}'
[[ $rc = 0 && -n $out && -z $err ]]; chk 91.17

# 91.18 a splat still must state its emptiness behaviour — taint-off is not
# such a statement, so bare `@a:!` is SB5032 exactly as bare `@a` is. Under the
# old coupling this silently supplied an elide the author never wrote.
run_sash 'argv 1 a; /usr/bin/printf "%s" @a:!' X
[[ $rc != 0 && $err == *SB5032* ]]; chk 91.18

# 91.19 `@{a:!-WORD}` — trusted splat with a substitute element when empty
run_sash 'array a=(); /usr/bin/printf "[%s]" @{a:!-sub}'
[[ $rc = 0 && $out = "[sub]" && -z $err ]]; chk 91.19

# 91.20 `@{a:!?MSG}` — trusted splat, aborts when empty
run_sash 'array a=(); /usr/bin/printf "%s" @{a:!?nothing}'
[[ $rc != 0 && -z $out ]]; chk 91.20

# 91.21 the modifier is orthogonal, so the disposition still works normally on
# a NON-empty trusted splat
run_sash 'argv 1 a; /usr/bin/printf "[%s]" @{a:!-sub}' P Q
[[ $rc = 0 && $out = "[P][Q]" && -z $err ]]; chk 91.21

# 91.22 order is fixed: `~!` is not a spelling. It must not be read as `:~`
# with a stray word either — that would silently drop the trust.
run_sash 'x=hi; /bin/echo ${x:~!}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 91.22

# 91.23 DG's reported line: split a tainted scalar into a CLEAN array with a
# per-use trust, no attest predicate
run_sash 'ckd=$(getenv HOME)
read $ckd:! opts
/bin/echo "$(len opts)"'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk 91.23

# ── DD#93 Part A: `bless` renamed to `attest` (hard rename, no alias) ──
# 93.1 `bless` is no longer a keyword — it is just an unknown command now
run_sash 'x=hi; bless x'
[[ $rc != 0 && $err == *SB5011* ]]; chk 93.1
# 93.2 `attest` clears taint end-to-end (the renamed primitive works).
run_sash 'x=$(getenv HOME)
attest x [[ $x =~ ^/[a-zA-Z0-9/]+$ ]]
/bin/echo $x'
[[ $rc = 0 && -z $err ]]; chk 93.2
# 93.3 diagnostic wording follows the rename ("without attest")
run_sash 'x=$(getenv HOME); /bin/echo $x'
[[ $rc != 0 && $err == *"without attest"* ]]; chk 93.3

# ── DD#93 Part B: `attest VAR OP` unary shorthand ──
# `attest x OP` ≡ `attest x [[ OP $x ]]`. The operator binds the attested name
# by construction, so a wrong-var predicate (SB5020) cannot be written in this
# form — strictly safer than the full form, and still greppable.
# 93.20 the scalar shorthand declassifies, exactly like the full predicate.
run_sash 'x=$(getenv HOME)
attest x -n
/bin/echo $x'
[[ $rc = 0 && -n $out && -z $err ]]; chk 93.20
# 93.21 …and is exactly equivalent to spelling the predicate out
run_sash 'x=$(getenv HOME)
attest x [[ -n $x ]]
/bin/echo $x'
short=$out
run_sash 'x=$(getenv HOME)
attest x -n
/bin/echo $x'
[[ $rc = 0 && $out = "$short" && -z $err ]]; chk 93.21
# 93.22 attest-all (array): the operator binds the element binder
run_sash 'array a=(1 2 3)
attest e in a i?
/bin/echo @{a:?empty}'
[[ $rc = 0 && $out = "1 2 3" && -z $err ]]; chk 93.22
# 93.22b …and the shorthand really is checked per element: one non-integer
# element aborts the attest-all at runtime.
run_sash 'array a=(1 x 3)
attest e in a i?
/bin/echo @{a:?empty}'
[[ $rc != 0 && -z $out ]]; chk 93.22b
# 93.23 the map form binds BOTH binders — `attest k v in m OP` is
# `[[ OP $k && OP $v ]]` (DG 2026-09-07), since a map predicate that references
# only one binder is SB5020.
run_sash 'map m
m[K]=V
attest k v in m g?
/bin/echo ${m[K]}'
[[ $rc = 0 && $out = "V" && -z $err ]]; chk 93.23
# 93.24 a shorthand whose predicate fails aborts at runtime, like the full form
run_sash 'x=$(getenv SASH_SPEC_UNSET_VAR)
attest x -n
/bin/echo $x'
[[ $rc != 0 && -z $out ]]; chk 93.24
# 93.25 `-v`/`-o`/`-R` stay SB5028-rejected in the shorthand (DD#93). These
# reach codegen otherwise, which has no lowering for them and ICEs.
run_sash 'x=$(getenv HOME)
attest x -v
/bin/echo $x'
[[ $rc != 0 && -z $out && $err == *SB5028* ]]; chk 93.25
# 93.26 …and in the full predicate form too. `attest` carries its predicate as
# a Test arg exactly like `[[ ]]`, so the same operator rules apply to both.
run_sash 'x=$(getenv HOME)
attest x [[ -o $x ]]
/bin/echo $x'
[[ $rc != 0 && -z $out && $err == *SB5028* ]]; chk 93.26

# ── DD#93 Part D: `attest SOURCE (( TARGET = SOURCE ))` ──
# The int-assignment IS the predicate: the conversion can only succeed on a
# valid, representable i64, so performing it proves the property. On success it
# declassifies BOTH names — the source string and the int it binds.
# 93.30 the conversion binds the target, and the target is usable as an int
run_sash 's=42
attest s (( c = s ))
echo $(( c + 1 ))'
[[ $rc = 0 && $out = "43" && -z $err ]]; chk 93.30
# 93.31 a value that is not a valid i64 aborts, like a failed [[ ]] predicate
run_sash 's=abc
attest s (( c = s ))
echo $(( c ))'
[[ $rc != 0 && -z $out ]]; chk 93.31
# 93.31b …with the same attest-fail message the [[ ]] forms produce.
run_sash 's=abc
attest s (( c = s ))
echo $(( c ))'
[[ $rc != 0 && $rerr == *"attest: s = 'abc' failed the gate"* ]]; chk 93.31b
# 93.32 both names are declassified: the tainted SOURCE (here `$(arg 1)`) is
# usable afterwards too, not just the bound int it was converted into.
run_sash 'n=$(arg 1)
attest n (( c = n ))
/bin/echo $n $(( c ))' 42
[[ $rc = 0 && $out = "42 42" && -z $err ]]; chk 93.32
# 93.33 the `if` form branches instead of aborting (DG 2026-09-07).
run_sash 's=abc
if attest s (( c = s )); then echo "ok $(( c ))"; else echo "not an int"; fi'
[[ $rc = 0 && $out = "not an int" && -z $err ]]; chk 93.33
# 93.34 …and on the else path the target is declared but unassigned, reading as
# `0` — what a declared-but-unassigned int reads as everywhere else in sash.
run_sash 's=abc
if attest s (( c = s )); then echo "ok"; else echo "c=[$(( c ))] [$c]"; fi'
[[ $rc = 0 && $out = "c=[0] [0]" && -z $err ]]; chk 93.34
# 93.35 distinct names are required — same-name in-place retype is deferred
run_sash 's=42
attest s (( s = s ))
echo $(( s ))'
[[ $rc != 0 && -z $out && $err == *SB5053* ]]; chk 93.35
# 93.36 the bare source inside the `(( ))` is exempt from the DD#77 D1
# disposition rule (SB4030) — the `attest` supplies the accountability, which
# is the whole point of the form. The same expression outside an attest is
# still rejected.
run_sash 's=42
echo $(( s + 1 ))'
[[ $rc != 0 && $err == *SB4030* ]]; chk 93.36
# 93.37 …and the attest's assignment is not a `$(( … ))` rvalue, so the
# "assignment can't take effect" rule (SB4020) does not apply to it either.
run_sash 's=7
attest s (( c = s ))
echo $(( c ))'
[[ $rc = 0 && $out = "7" && -z $err ]]; chk 93.37

# ── DD#93 Part D addendum: wider arith RHS + predicate taint (DG 2026-09-07) ──
# The conversion still has to succeed for a wider RHS to be computable, so the
# gate (`i?` on the attested name) proves just as much — the RHS may therefore
# be any expression that REFERENCES the attested variable. Because that RHS
# yields a VALUE (unlike a `[[ ]]` predicate, which yields only a boolean), any
# other name it reads must be clean, or a tainted value would be laundered into
# the clean target.
# 93.40 a wider RHS binds the computed value
run_sash 'x=$(printf 123)
attest x (( y = x * 2 ))
echo $(( y ))'
[[ $rc = 0 && $out = "246" && -z $err ]]; chk 93.40
# 93.41 …and still declassifies the source
run_sash 'x=$(printf 123)
attest x (( y = x * 2 ))
/bin/echo $x'
[[ $rc = 0 && $out = "123" && -z $err ]]; chk 93.41
# 93.42 the RHS must REFERENCE the attested variable — otherwise the attest
# proves nothing about it.
run_sash 'x=$(printf 123)
attest x (( y = 5 ))
echo $(( y ))'
[[ $rc != 0 && -z $out && $err == *SB5054* ]]; chk 93.42
# 93.43 …and the predicate must be an assignment at all
run_sash 'x=$(printf 123)
attest x (( x ))
/bin/echo $x'
[[ $rc != 0 && -z $out && $err == *SB5054* ]]; chk 93.43
# DD#113: the `(( … ))` attest predicate is scalar-only. A attest-all lead
# carrying one used to decode as nothing at all, fall through to the generic
# command path, and exec a bare `attest` at runtime — the internal
# `sash: bug: bare command passed to sash_exec` assertion reaching user output.
# These flip the non-scalar arm of validate.rs::attest_arith_near_miss.
# 93.49 array attest-all with an arith predicate
run_sash 'array a=(1 2)
attest e in a (( 1 ))
/bin/echo ${a[0]}'
[[ $rc != 0 && -z $out && $err == *SB5054* ]]; chk 93.49
# 93.50 …and the message names the real constraint, not the scalar one
[[ $err == *scalar-only* ]]; chk 93.50
# 93.51 map attest-all with an arith predicate
run_sash 'map m=([k]=v)
attest k v in m (( 1 ))
/bin/echo ${m[k]}'
[[ $rc != 0 && -z $out && $err == *SB5054* ]]; chk 93.51
# 93.52 an over-long lead is the same error, not a parse failure
run_sash 'array a=(1 2)
attest x y z w in a (( 1 ))
/bin/echo ${a[0]}'
[[ $rc != 0 && -z $out && $err == *SB5054* ]]; chk 93.52
# 93.53 an assignment shape does not buy its way in either — it was already
# caught incidentally by SB4020, which said nothing about the real problem.
run_sash 'array a=(1 2)
attest e in a (( t = e ))
/bin/echo ${a[0]}'
[[ $rc != 0 && -z $out && $err == *SB5054* ]]; chk 93.53
# 93.54 the scalar form is untouched
run_sash 'x=$(printf 123)
attest x (( y = x ))
echo $(( y ))'
[[ $rc = 0 && $out = "123" && -z $err ]]; chk 93.54
# 93.44 a CLEAN extra operand is fine
run_sash 'int k=10
x=$(printf 123)
attest x (( y = x + k ))
echo $(( y ))'
[[ $rc = 0 && $out = "133" && -z $err ]]; chk 93.44
# 93.45 a TAINTED extra operand is rejected — the value would be laundered into
# the clean target.
run_sash 'x=$(getenv HOME)
z=$(getenv PATH)
attest x (( y = x + z ))
echo $(( y ))'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 93.45
# 93.46 an attest `[[ ]]` predicate may not read another TAINTED name either
# (DG 2026-09-07): even a boolean gate should not be influenced by data the
# author has not vouched for. The attested name itself is tainted by
# definition and stays exempt.
run_sash 'x=$(printf 5)
z=$(printf 5)
attest x [[ $x == $z ]]
/bin/echo $x'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 93.46
# 93.47 …while a clean operand in a predicate is still fine
run_sash 'z=5
x=$(printf 5)
attest x [[ $x == $z ]]
/bin/echo $x'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk 93.47
# 93.48 DG's report verbatim (2026-09-07): this ICE'd in codegen — the SB4020
# exemption did not check that the predicate actually classified, so a
# near-miss reached the value path and panicked.
run_sash 'x=$(printf 123); attest x ((y=x*2)); echo $y'
[[ $rc = 0 && $out = "246" && -z $err ]]; chk 93.48


# ═══════ DD#24: an external stage ANYWHERE upstream taints the read ═══════
# taint_pipeline consulted only the FIRST stage, so prefixing any pipeline
# with a builtin laundered the whole thing: `echo ig | /bin/cat secret | read x`
# yielded the file's contents CLEAN. The SSA backstop that should have caught
# it had `clean_pipe: true` hardcoded for every lastpipe read. Both layers
# failed. DD#24: any output that passed through execve(2) is tainted, so a read
# target is clean only when EVERY upstream stage is compiler-computed.

# tl.1 the laundering shape is rejected (scalar read)
run_sash 'echo ig | /bin/cat /etc/hostname | readline x
echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk tl.1

# tl.2 …a scalar read (the multi-target form is retired, DD#99 stage 6)
run_sash 'echo ig | /bin/cat /etc/hostname | readline x
echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk tl.2

# tl.3 …array read
run_sash 'echo ig | /bin/cat /etc/hostname | read a
echo "${a[0]}"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk tl.3

# tl.4 …mapfile
run_sash 'echo ig | /bin/cat /etc/hostname | read a
echo "${a[0]}"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk tl.4

# tl.5 …and with the external stage third, not second
run_sash 'echo ig | /bin/cat | /bin/cat | readline x
echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk tl.5

# tl.6 attest still admits the value (the reject is not a dead end)
run_sash 'echo ig | /bin/cat /etc/hostname | readline x
attest x p?
echo "ok"'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk tl.6

# tl.7 an ALL-builtin pipeline stays clean — no attest needed (no regression)
run_sash 'echo "a b" | readline x
echo "$x"'
[[ $rc = 0 && $out = "a b" && -z $err ]]; chk tl.7

# tl.8 a clean pipeline into an ARRAY read likewise needs no attest — the
# fix must not over-taint the shapes that were always legitimate.
# (The SSA-backstop half of this fix is pinned by taint_switch.sh sw.9.)
# The split is NUL-only (DD#99), so "p q\n" is ONE field — the subject here is
# that a clean producer needs no attest, not the field count.
# Lastpipe used to bind an array target as a scalar string, which made `len`
# count bytes.
run_sash 'echo "p q" | read a
echo "$(len a)"'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk tl.8

# --- DD#99: SB5021 names a route that still exists ---
# The advice used to read "external data enters arrays via read/mapfile" for
# every target. `mapfile` is retired (DD#99) and a MAP has no bulk read at
# all — it is built from an already-vouched-for array. The message now
# follows the declared kind of the target.

# 60.56 a tainted value into an ARRAY element: says "array", points at
# `read ARR`, and no longer names the retired `mapfile`
run_sash 'array a=()
a[0]=$(getenv HOME)
echo "${a[0]}"'
[[ $rc != 0 && -z $out && $err == *SB5021* \
   && $err == *"array 'a'"* && $err == *'read ARR'* \
   && $err != *mapfile* ]]; chk 60.56

# 60.57 a tainted value into a MAP element: says "map", and names the real
# route (array -> attest -> `map m = @{ARR:DISP}`)
run_sash 'map m
m[k]=$(getenv HOME)
echo "${m[k]}"'
[[ $rc != 0 && -z $out && $err == *SB5021* \
   && $err == *"map 'm'"* && $err == *'@{ARR:DISP}'* \
   && $err != *mapfile* ]]; chk 60.57

# 60.58 the map LITERAL form gets the map wording too
run_sash 'map m=([k]=$(getenv HOME))
echo "${m[k]}"'
[[ $rc != 0 && -z $out && $err == *SB5021* \
   && $err == *"map 'm'"* && $err != *mapfile* ]]; chk 60.58

# 60.59 the array LITERAL form gets the array wording
run_sash 'array a=($(getenv HOME))
echo "${a[0]}"'
[[ $rc != 0 && -z $out && $err == *SB5021* \
   && $err == *"array 'a'"* && $err != *mapfile* ]]; chk 60.59

# ─── DD#82 backstop: a name collision must not launder an execve ─────────
# The SSA taint backstop classifies a command-substitution head. DD#23
# rewrites an external head to an absolute path, so only a BARE name can be
# a user function. The classifier used to fall back to the basename, which
# made `/bin/cat` match a user function named `cat` and report Clean for a
# real execve. Measured (journal 363): that fallback was true ONLY in the
# collision case, and the bare-name test never fired at all, because a
# user-function capture is lowered elsewhere and never reaches that arm.
printf 'payload\n' > d82a.txt

# dd82.1 an external capture whose basename collides with a user function is
#        still TAINTED — unattested use rejects exactly as without collision
run_sash 'fn cat { echo fromfn; }
str x=$(/bin/cat d82a.txt)
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk dd82.1

# dd82.2 …and attesting is what makes it usable: the collision changes
#        nothing about the handling the value requires
run_sash 'fn cat { echo fromfn; }
str x=$(/bin/cat d82a.txt)
attest x [[ -n $x ]]
echo $x'
[[ $rc = 0 && $out = "payload" ]]; chk dd82.2

# dd82.3 control: same program with no colliding function — same outcome
run_sash 'str x=$(/bin/cat d82a.txt)
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk dd82.3

# dd82.4 control: a user-function capture is Clean by DD#24 even when the
#        function itself runs an external. Deliberate, and unchanged here.
run_sash 'fn mk { /bin/cat d82a.txt; }
str x=$(mk)
echo $x'
[[ $rc = 0 && $out = "payload" ]]; chk dd82.4

# ─── the two-binder attest against a NON-map: the shape diagnostic ──────
# `attest K V in M` requires M to be a map. The message that says so was
# built by a function whose 10 calls all returned early in a full sweep —
# the diagnostic itself was never once produced. It is reachable; nothing
# had asked for it. (The obvious probe misses it: if the condition does
# not validate BOTH binders, an earlier check fires instead and masks it.)

# 93.55 …against an indexed array: the form is wrong, and the fix is one
#       binder rather than two, so the message says which
run_sash 'array a=(1 2 3)
attest k v in a [[ -n $k && -n $v ]]
echo done'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *"indexed array"* \
   && $err == *"one binder"* ]]; chk 93.55

# 93.56 …against a declared scalar: a different remedy, so a different text
run_sash 'str s=hi
attest k v in s [[ -n $k && -n $v ]]
echo done'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *"scalar attest"* ]]; chk 93.56

# 93.57 …an int is a scalar for this purpose too
run_sash 'int n=1
attest k v in n [[ -n $k && -n $v ]]
echo done'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *"scalar attest"* ]]; chk 93.57

# 93.58 positive control: against a real map the form is accepted
run_sash 'array a=(x y)
map m = @{a:~}
attest k v in m [[ -n $k && -n $v ]]
echo done'
[[ $rc = 0 && $out = "done" ]]; chk 93.58

# ── attest on an INT variable (codegen/dispatch.rs::emit_attest) ──
# Every attest spec in the suite attested a *string*. The int paths in
# emit_attest — the `_ =>` value fallback and the `VarSlot::Int` copy arm —
# had never executed, so a working feature had no gate at all. Found via
# coverage (journal 399/400).
#
# Writing the block turned up a taint-quarantine bypass, so it now gates both.
# DD#24 §Quarantine lists `${x:op}` "in any expression" as a rejected read, and
# `$(( … ))` is an expression: the arithmetic disposition *converts* (DD#77),
# it does not declassify. Only `attest s (( n = s ))` does both (DD#93 Part D).
# atint.1 the laundering shape is SB5020 at the READ, with a location — it used
# to reach the SSA backstop (or nothing at all, atint.11) and `attest n` then
# declassified a value nothing had vouched for.
run_sash 'x=$(arg 1); int n=$(( ${x:-0} )); attest n [[ -n $n ]]; echo "$n"' 7
[[ $rc != 0 && -z $out && $err == *"SB5020"* ]]; chk atint.1
# atint.2 the same read inside a `(( … ))` mutation statement.
run_sash 'x=$(arg 1); int n=0; (( n = ${x:-0} )); attest n [[ -n $n ]]; echo "$n"' 7
[[ $rc != 0 && -z $out && $err == *"SB5020"* ]]; chk atint.2
# atint.3 DD#24's sanctioned route: attest the SOURCE, then convert. The
# disposition is now a plain conversion of a vouched-for value.
run_sash 'x=$(arg 1); attest x i?; int n=$(( ${x:-0} )); echo "$n"' 7
[[ $rc = 0 && $out = "7" && -z $err ]]; chk atint.3
# atint.4 DD#93 Part D: the conversion IS the validation, and declassifies both.
run_sash 'x=$(arg 1); attest x (( n = x )); echo "$n"' 7
[[ $rc = 0 && $out = "7" && -z $err ]]; chk atint.4
# atint.5 attest on an int as a runtime gate — the passing case. After the
# SB5020 fix no int can be tainted (every route is SB4030 or SB5020), so the
# int form is validation-only, like DD#93's map 2-binder form and for the same
# reason: attest is the only assert-with-diagnostic in the language.
run_sash 'int n=5; attest n [[ -n $n ]]; echo "$n"'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk atint.5
# atint.6 a failing gate aborts with exit 1 and names BOTH the variable and the
# value. This is the divergence that broke the tie: the C backend printed
# `n = '9'` (emit_var_value → sash_itoa) while native passed zeros for a
# non-Str slot and printed only `n`. Same program, two diagnostics — native was
# wrong, and the differential oracle is what caught it. With the int form now
# validation-only, this message is the whole point of it.
run_sash 'int n=9; attest n [[ $n == 5 ]]; echo "$n"'
[[ $rc = 1 && -z $out && $rerr == *"attest: n = '9' failed the gate"* ]]; chk atint.6
# atint.7 the string form names its value the same way — one sentence shape for
# both types, which is what the int case now agrees with.
run_sash 'x=$(arg 1); attest x [[ $x == good ]]; echo "$x"' bad
[[ $rc = 1 && -z $out && $rerr == *"attest: x = 'bad' failed the gate"* ]]; chk atint.7
# atint.8 the attested int is a real value, not a constant folded past.
run_sash 'int n=21; attest n [[ -n $n ]]; echo "$(( n * 2 ))"'
[[ $rc = 0 && $out = "42" && -z $err ]]; chk atint.8
# atint.9 an arithmetic CONDITION is a read position too. `taint_stmt` sent
# StmtKind::ArithTest to a walker that only visits child bodies — of which an
# arith test has none — so the condition itself went unchecked.
run_sash 'x=$(arg 1); if (( ${x:-0} > 5 )); then echo hi; fi' 7
[[ $rc != 0 && -z $out && $err == *"SB5020"* ]]; chk atint.9
# atint.10 …and in a `while` condition.
run_sash 'x=$(arg 1); while (( ${x:-0} > 5 )); do echo hi; done' 7
[[ $rc != 0 && -z $out && $err == *"SB5020"* ]]; chk atint.10
# atint.11 the shape that compiled CLEAN before the fix — no SB5020, and no SSA
# backstop either, because a lone leaf never became a tainted int. It printed
# attacker-controlled bytes verbatim.
run_sash 'x=$(arg 1); echo "$(( ${x:-0} ))"' 7
[[ $rc != 0 && -z $out && $err == *"SB5020"* ]]; chk atint.11
# atint.12 a tainted read in a subscript index inside arithmetic.
run_sash 'x=$(arg 1); array a=(1 2); echo "${a[${x:-0}]}"' 1
[[ $rc != 0 && -z $out && $err == *"SB5020"* ]]; chk atint.12
# atint.13 THE SAME ENUM VARIANT, A DIFFERENT WALKER. atint.11 made
# `check_expr_taint` total over ExprKind. `expr_uses_tainted` — the predicate
# behind SB5021 — kept its own `_ => false`, and it swallowed
# ExprKind::Arithmetic in exactly the same way. An arithmetic disposition
# placed into a COLLECTION was laundered outright: `z` never entered the
# tainted set, so every later check saw a clean array.
run_sash 'x=$(arg 1); array z=($(( ${x:-0} ))); echo "${z[0]}"' 7
[[ $rc != 0 && -z $out && $err == *"SB5021"* ]]; chk atint.13
# atint.14 ...and the map spelling, which reaches the element through the
# other arm of taint_array_literal_assignment.
run_sash 'x=$(arg 1); map m=([k]=$(( ${x:-0} ))); echo "${m[k]}"' 7
[[ $rc != 0 && -z $out && $err == *"SB5021"* ]]; chk atint.14
# atint.15 WHY IT WAS A BYPASS AND NOT MERELY A LATE CATCH. `echo "${z[0]}"`
# was stopped by the SSA backstop — a bare sentence with no SB code and no
# span, but stopped. A `[[ ]]` operand is not a command argument, so nothing
# stopped THIS one: it compiled, it ran, and it reported whether the tainted
# value matched. That is an oracle. A genuinely tainted array (`read arr`) is
# SB5020 in the identical position, which is what makes this a hole in the
# validator rather than a decision about the language.
run_sash 'x=$(arg 1); array z=($(( ${x:-0} ))); if [[ ${z[0]} == 7 ]]; then echo MATCH; else echo NO; fi' 7
[[ $rc != 0 && $out != "MATCH" && $err == *"SB5021"* ]]; chk atint.15
# atint.16 the new arm must not OVER-reject: clean arithmetic into a
# collection is still legal, and still computes. A taint arm that rejects
# everything passes atint.13-15 and is useless.
run_sash 'int n=5; array z=($(( n + 1 ))); echo "${z[0]}"'
[[ $rc = 0 && $out = "6" && -z $err ]]; chk atint.16
# ── The SB4030 shield ────────────────────────────────────────────────
# `check_arith_taint` has no arm for a BARE name in arithmetic
# (`ArithExpr::Var` / `DollarVar`), and `walk_arith_expansion_roots` hands it
# none, because neither variant carries an `Expr`. Both rely on the same
# claim: **a tainted int is not constructible**, so a bare name in arithmetic
# is either clean or not an int. That claim lived only in a doc comment.
# These pin it. If any of atint.17-atint.21 starts COMPILING, the comment is
# wrong and both walkers need the arm they currently do without.

# atint.17 THE FIRST HALF: no string reaches a bare arithmetic name at all.
# Read this one honestly — it is NOT a taint discriminator. A clean `x=7`
# produces the identical SB4030, and that is exactly the property being
# pinned: DD#77 requires a disposition, so `ArithExpr::Var` never holds a
# string, tainted or otherwise. If DD#77 were relaxed, a tainted string WOULD
# reach the variant both walkers decline to visit. Nothing else in the suite
# asserts the bare-name case; the neighbouring dsp.*/pex.* specs all exercise
# the disposition, which is the permitted spelling.
run_sash 'x=$(arg 1); echo "$(( x ))"' 7
[[ $rc != 0 && -z $out && $err == *"SB4030"* ]]; chk atint.17
# atint.18 ...and `$x`, which parses to the OTHER bare variant (DollarVar).
# Two variants, one answer — a divergence here would give the shield a seam.
run_sash 'x=$(arg 1); echo "$(( $x ))"' 7
[[ $rc != 0 && -z $out && $err == *"SB4030"* ]]; chk atint.18
# atint.19 THE SECOND HALF, and this one IS a discriminator: the clean
# counterpart `int n=7` compiles and prints 7. A tainted int cannot be
# DECLARED from argv...
run_sash 'int n=$(arg 1); echo "$(( n ))"' 7
[[ $rc != 0 && -z $out && $err == *"SB4030"* ]]; chk atint.19
# atint.20 ...nor from the environment.
run_sash 'int n=$(getenv SASH_SPEC_NOPE); echo "$(( n ))"'
[[ $rc != 0 && -z $out && $err == *"SB4030"* ]]; chk atint.20
# atint.21 ...nor by copying a tainted array element. This is the sharpest
# of the five: the clean counterpart `array arr=(7); int n=${arr[0]}` is
# SB4030, and the tainted one is SB5020 — two different codes for the same
# shape, so the assertion cannot be satisfied by the type rule alone. (DD#99
# `read` is how external data legitimately arrives, and it arrives in an
# array, which makes this the route an attacker would actually try.)
run_sash 'read arr; int n=${arr[0]}; echo "$(( n ))"'
[[ $rc != 0 && -z $out && $err == *"SB5020"* ]]; chk atint.21
# 61.1-61.7 DD#61: a tainted value must not reach an array/map element through
# a `:-` default. These were the specs that closed a real hole, and they still
# pin the same programs -- but the mechanism under them changed on 2026-09-30.
# A `:-` default is a LITERAL now (SB4070), so none of these can express a
# tainted default at all. The hole is shut by construction rather than by
# SB5021 catching it, which is the stronger form of the same guarantee.
# 61.1 subscript assignment
run_sash 'str x=""
array a=(q)
a[0]=${x:-$(getenv HOME)}
echo "${a[0]}"'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk 61.1
# 61.2 array-literal element
run_sash 'str x=""
array a=( ${x:-$(getenv HOME)} )
echo "${a[0]}"'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk 61.2
# 61.3 map value
run_sash 'str x=""
map m=( [k]=${x:-$(getenv HOME)} )
echo "${m[k]}"'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk 61.3
# 61.4 a file slurp
run_sash 'str x=""
array a=( ${x:-$(</no/such/file/at/all)} )
echo "${a[0]}"'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk 61.4
# 61.5 an external command capture
run_sash 'str x=""
array a=( ${x:-$(/bin/date)} )
echo "${a[0]}"'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk 61.5
# 61.6 nested inside a quoted concat
run_sash 'str x=""
array a=( "pre${x:-$(getenv HOME)}" )
echo "${a[0]}"'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk 61.6
# 61.7 the slurp form, read back through a test. Before DD#61 this compiled
#      silently and compared the file contents.
run_sash 'str x=""
array a=( ${x:-$(</no/such/file/at/all)} )
if [[ ${a[0]} == 4242 ]] { echo "leaked" }'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk 61.7

# 61.8 CONTROL: a clean default in the same position must still WORK, so 61.1
#      pins "a source may not enter" and not "`:-` is banned here".
run_sash 'str x=""
array a=(q)
a[0]=${x:-fallback}
echo "${a[0]}"'
[[ $rc = 0 && $out = "fallback" && -z $err ]]; chk 61.8

# 61.9 CONTROL: DD#91's `!` still vouches — `is_taint_source` honours
#      `taint_off` exactly as `pexp_uses_tainted` does.
run_sash 'str t=$(getenv HOME)
array a=( ${t:!} )
echo "ok:${a[0]}"'
[[ $rc = 0 && $out == ok:* && -z $err ]]; chk 61.9

# 61.10 a source in a subscript INDEX is a read (SB5020), not a source of the
#       stored value: `z=${b[$(getenv N)]}` stores b's element, but reads the
#       env var to choose which. `reject_scalar_rhs_read` checked the index
#       with the *value* walker, which treats a bare source as sanctioned
#       assignment-from-source — right for the RHS, wrong for an index.
run_sash 'array b=(one two)
str z=${b[$(getenv HOME)]}
echo "$z"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 61.10

# 61.11 the same index read inside a collection element
run_sash 'array b=(one two)
array a=(q)
a[0]=${b[$(getenv HOME)]}
echo "${a[0]}"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 61.11

# 61.12 CONTROL: a clean index must still work, in both positions
run_sash 'array b=(zero one two)
int i=2
array a=(q)
a[0]=${b[$i]}
echo "${a[0]}"'
[[ $rc = 0 && $out = "two" && -z $err ]]; chk 61.12
# 91.24-91.33 DD#91's `:!` vouches for a READ. With a literal operand there is
# no read in the default to vouch for, so `:!-EXPANSION` is SB4070 in every
# spelling -- including the four that used to be ACCEPTED here because `!`
# covered both subject and operand.
# 91.24 `${h:!-${u:!}}`, formerly accepted
run_sash 'str h=$(getenv HOME)
str u=$(getenv NOSUCHVAR_9124)
str z=${h:!-${u:!}}
echo "ok:$z"'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk 91.24
# 91.25 the operands the other way round, formerly accepted
run_sash 'str u=$(getenv NOSUCHVAR_9125)
str h=$(getenv HOME)
str z=${u:!-${h:!}}
echo "ok:$z"'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk 91.25
# 91.26 the brace-less spelling, formerly accepted
run_sash 'str u=$(getenv NOSUCHVAR_9126)
str h=$(getenv HOME)
str z=$u:!-${h:!}
echo "ok:$z"'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk 91.26
# 91.27 an unvouched tainted operand (was SB5020)
run_sash 'str h=$(getenv HOME)
str p=$(getenv PATH)
str z=${h:!-$p}
echo "$z"'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk 91.27
# 91.28 a taint SOURCE materialised in the default (was SB5020)
run_sash 'str h=$(getenv HOME)
str z=${h:!-$(getenv PATH)}
echo "$z"'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk 91.28
# 91.29 `!` on the operand but not the subject (was SB5020)
run_sash 'str h=$(getenv HOME)
str p=$(getenv PATH)
str z=${h:-${p:!}}
echo "$z"'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk 91.29
# 91.30 into an array literal, formerly accepted
run_sash 'str h=$(getenv HOME)
str u=$(getenv NOSUCHVAR_9130)
array a=( ${h:!-${u:!}} )
echo "ok:${a[0]}"'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk 91.30

# 91.31 `!` is a taint annotation, not a change of type: an int subject read
#       through `:!-` must take the SAME path as without it. Wrapping the
#       subject in `Trusted` hid it from `source_is_int`, which matches
#       `Var`/`Subscript` structurally, until that learned to see through the
#       wrapper. A set, non-zero int cannot show this — both paths return it —
#       so the probe is the identity element: at 0 the int ternary yields the
#       default, while the string path atoi's "0" and yields 0.
run_sash 'int n=$(( 0 ))
int a=${n:-7}
int b=${n:!-7}
echo "$a:$b"'
[[ $rc = 0 && $out = "7:7" && -z $rerr ]]; chk 91.31
# 91.32 an unvouched operand into an array literal (was SB502x)
run_sash 'str h=$(getenv HOME)
str p=$(getenv PATH)
array a=( ${h:!-$p} )
echo "${a[0]}"'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk 91.32
# 91.33 a taint source into an array literal (was SB502x)
run_sash 'str h=$(getenv HOME)
array a=( ${h:!-$(getenv PATH)} )
echo "${a[0]}"'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk 91.33
# ── DD#91 addendum: `!` is a MODIFIER, so a vouched splat is legal in every
#    position a bare one is — including a `for`-in source. Before 2026-09-30
#    `reject_tainted_array_iter` resolved the splat without ever consulting
#    `pe.taint_off`, so 91.34 was SB5020: the modifier was silently ignored,
#    re-imposing exactly the position restriction the addendum removed.
# 91.34 a vouched splat iterates a tainted array
run_sash 'array g=()
argv 1 g
for v in @{g:!} { echo "[$v]" }' one two
[[ $rc = 0 && $out == "[one]
[two]" && -z $rerr ]]; chk 91.34
# 91.35 control: the same loop, unvouched, is still rejected
run_sash 'array g=()
argv 1 g
for v in @g { echo "[$v]" }' one
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 91.35
# 91.36 `:!` is per-USE, not a declassification — a later bare read still fails
run_sash 'array g=()
argv 1 g
for v in @{g:!} { echo "$v" }
echo @{g:-x}' one
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 91.36
# 91.37 nested vouched loops over the same tainted array
run_sash 'array g=()
argv 1 g
for a in @{g:!} { for b in @{g:!} { echo "$a$b" } }' x y
[[ $rc = 0 && $out == "xx
xy
yx
yy" && -z $rerr ]]; chk 91.37
# 91.38 a clean array is unaffected by the modifier
run_sash 'array g=(a b)
for v in @{g:!} { echo "[$v]" }'
[[ $rc = 0 && $out == "[a]
[b]" && -z $rerr ]]; chk 91.38

# ── DD#86: the front end taint-checks a `$( … )` body ──────────────────────
# `check_cmdsub_inner_reads` was the only thing that looked inside a body: it
# matched `if let StmtKind::Command`, so a Declaration / If / While / Redirected
# statement was skipped outright, and it measured every read against the OUTER
# tainted set, which never heard of a name the body itself defined. The net
# effect was that NOTHING in a body was checked — every case below reached the
# SSA backstop instead, with no SB code and no span. `taint_stmt` now descends
# into `cmdsub_bodies()` with a cloned set, exactly as `taint_function_body`
# does for the construct DD#86 says a body behaves like.
#
# Each spec asserts the SB code AND that the diagnostic is located — the
# message must carry a `:LINE:COL:` span (the leading filename differs by
# mode — native compiles with `-c`, the c backend from `.input.sash`), because
# the backstop's
# message is the thing being replaced and it carries neither a code nor a span. The backstop itself is still tested — see `bkstp.*` in
# taint_switch.sh, which now turns the front end off to reach it.

# cbt.1 a builtin's argument
run_sash 'str q=$(str h=$(getenv HOME); echo "$h")
attest q [[ -n $q ]]
/bin/echo "$q"'
[[ $rc != 0 && -z $out && $err == *SB5020* && $err == *:[0-9]*:[0-9]*:* ]]; chk cbt.1
# cbt.2 a test operand — an `if` body is not a Command, so the old walk skipped
#       the whole statement
run_sash 'str q=$(str h=$(getenv HOME); if [[ -n $h ]]; then /bin/echo y; fi)
attest q [[ -n $q ]]
/bin/echo "$q"'
[[ $rc != 0 && -z $out && $err == *SB5020* && $err == *:[0-9]*:[0-9]*:* ]]; chk cbt.2
# cbt.3 a redirect target — the position that let an environment variable
#       choose a filename (the original P1 report)
run_sash 'str q=$(str h=$(getenv HOME); /bin/echo x > $h)
attest q [[ -z $q ]]
/bin/echo "[$q]"'
[[ $rc != 0 && -z $out && $err == *SB5020* && $err == *:[0-9]*:[0-9]*:* ]]; chk cbt.3
# cbt.4 a `while` condition
run_sash 'str q=$(str h=$(getenv HOME); while [[ -n $h ]]; do break; done; /bin/echo y)
attest q [[ -n $q ]]
/bin/echo "$q"'
[[ $rc != 0 && -z $out && $err == *SB5020* && $err == *:[0-9]*:[0-9]*:* ]]; chk cbt.4
# cbt.5 a subscript value gets the more specific SB5021, as it does outside
run_sash 'str q=$(array a=(z); str h=$(getenv HOME); a[0]=$h; /bin/echo "${a[0]}")
attest q [[ -n $q ]]
/bin/echo "$q"'
[[ $rc != 0 && -z $out && $err == *SB5021* && $err == *:[0-9]*:[0-9]*:* ]]; chk cbt.5
# cbt.6 the tainted definition must be visible to LATER statements in the same
#       body — hole 2. The read is two statements after the definition.
run_sash 'str q=$(str h=$(getenv HOME); /bin/echo start; /bin/echo "$h")
attest q [[ -n $q ]]
/bin/echo "$q"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk cbt.6
# cbt.7 a wrapper statement inside the body recurses like any other
run_sash 'str v=a
str q=$(if [[ $v == a ]]; then str h=$(getenv HOME); /bin/echo "$h"; fi)
attest q [[ -n $q ]]
/bin/echo "$q"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk cbt.7
# cbt.8 the duplication guard. `taint_stmt` descends unconditionally while
#       `taint_child_bodies` still walks `structural_bodies()`; take the body
#       twice and this prints SB5020 once per path.
run_sash 'str v=a
int i=0
str q=$(while (( i < 1 )); do if [[ $v == a ]]; then str h=$(getenv HOME); /bin/echo "$h"; fi; i=$(( i + 1 )); done)
attest q [[ -n $q ]]
/bin/echo "$q"'
[[ $rc != 0 && -z $out && $(grep -c SB5020 <<< "$err") = 1 ]]; chk cbt.8
# cbt.9 attesting INSIDE the body clears it, so the descent is a real taint
#       walk and not a blanket rejection of bodies that touch a source
run_sash 'str q=$(str h=$(getenv HOME); attest h [[ -n $h ]]; /bin/echo "$h")
attest q [[ -n $q ]]
/bin/echo "[$q]"'
[[ $rc = 0 && -n $out && -z $err ]]; chk cbt.9
# cbt.10 control: a body with no tainted read is untouched by the descent
run_sash 'str q=$(str h=ok; /bin/echo "$h")
attest q [[ -n $q ]]
/bin/echo "$q"'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk cbt.10

# ── div.1-div.9 — DD#121 R2: the "don't fall through" exemption ──
# R2 exempts a path that diverts: "A path that does not fall through (`exit` /
# `return` / `break` / `continue`) is exempt."  Two independent faults made
# that exemption unreachable; these pin both fixes and their guard rails.
#
#  (1) `taint_list` merged the `||` right operand's out-state unconditionally,
#      undoing the exemption `check_list_caught_resolution` had just honoured.
#      So `attest … || exit 1` left the subject tainted and the next read was
#      SB5020 — rejecting the very form the SB5089 text recommends. The `if`
#      path already filtered on `block_falls_through` via `merge_paths`; the
#      list path now mirrors it.
#  (2) the SSA backstop unioned every phi source, including sources from a
#      block that exits and so never reaches the merge. A backstop false
#      positive is a user-facing ICE (ssa_taint.rs), so it filters diverging
#      predecessors out of the phi rule.
#
# The SCALAR caught-attest forms (`attest x … || exit 1`, `if attest x …;
# then :; else exit 1; fi`) are still rejected: the transform deliberately
# restores the pre-attest version at an attest merge and emits no phi
# (control_flow.rs "deliberate, not a simplification"), whose stated premise
# — that SB5089 leaves no clean version to merge to — is exactly what this
# exemption breaks. See notes/tasks/bug-attest-merge-drops-diverted-path.md.

# div.1 collection subject, `|| exit 1`, read after the merge — fault (1).
# This was SB5020 before the fix.
run_sash 'argv a
attest v in a [[ -n $v ]] || exit 1
for v in @a; do echo "$v"; done' one two
[[ $rc = 0 && $out == *$'\none\ntwo' && -z $err ]]; chk div.1
# div.2 `|| return` inside a function is the same exemption
run_sash 'f() {
argv 1 a
attest v in a [[ -n $v ]] || return 1
echo "$(len a)"
}
f' one two
[[ $rc = 0 && $out = 2 && -z $err ]]; chk div.2
# div.3 the resolve arm of R2 still works — the fix must not disturb it
run_sash 'str x=$(getenv HOME)
attest x [[ $x =~ ^/ ]] || x=d
echo "$x"'
[[ $rc = 0 && -n $out && -z $err ]]; chk div.3
# div.4 a `||` right that FALLS THROUGH still rejects: the fix stops the
# exemption being undone, it does not widen it
run_sash 'str x=$(getenv HOME)
attest x [[ $x =~ ^/ ]] || echo no
echo "$x"'
[[ $rc != 0 && $err == *SB5089* ]]; chk div.4
# div.5 `&&` has no failure branch at all — still SB5089
run_sash 'str x=$(getenv HOME)
attest x [[ $x =~ ^/ ]] && echo "$x"'
[[ $rc != 0 && $err == *SB5089* ]]; chk div.5
# div.6 a diverting right operand does not launder an UNATTESTED read — the
# exemption concerns resolution at the merge, not the taint itself
run_sash 'str x=$(getenv HOME)
/bin/true || exit 1
echo "$x"'
[[ $rc != 0 && $err == *SB5020* ]]; chk div.6
# div.7 the collection guard rail: a falling-through right still rejects
run_sash 'argv a
attest v in a [[ -n $v ]] || echo no
for v in @a; do echo "$v"; done' one
[[ $rc != 0 && $err == *SB5089* ]]; chk div.7
# div.8 fault (2) standing alone, no attest at the merge: a clean value on the
# surviving path, the other path exits. The phi unioned the diverting path's
# tainted version, so the backstop rejected what the front end allows.
run_sash 'str c=$(arg 1)
attest c [[ -n $c ]]
str x=$(arg 2)
if [[ $c == y ]]; then x=safe; else exit 1; fi
echo "$x"' y tainted
[[ $rc = 0 && $out = "safe" && -z $err && $err != *backstop* ]]; chk div.8
# div.9 the converse: when the other path FALLS THROUGH the tainted version
# does reach the merge, and must still be caught
run_sash 'str c=$(arg 1)
attest c [[ -n $c ]]
str x=$(arg 2)
if [[ $c == y ]]; then x=safe; fi
echo "$x"' y tainted
[[ $rc != 0 && $err == *SB5020* ]]; chk div.9

# div.10-div.16 — the scalar forms, and the merge that makes them work.
# `lower_if_attest` was the only two-branch construct in the transform that
# never merged: it emitted no phi and leaked whatever version the else path
# left, so code after the `if` read a version defined on exactly one path.
# R2 is what makes a real merge both safe and necessary — "clean on every
# path that falls through" means a clean version always reaches the merge.

# div.10 scalar `|| exit 1`, read after the merge. The merge now reads the
# attested version; the diverting else is not a predecessor and gets no phi.
run_sash 'str x=$(getenv HOME)
attest x [[ $x =~ ^/ ]] || exit 1
echo "$x"'
[[ $rc = 0 && -n $out && -z $err ]]; chk div.10
# div.11 the form named in DD#121's own table
run_sash 'str x=$(getenv HOME)
if attest x [[ $x =~ ^/ ]]; then :; else exit 1; fi
echo "$x"'
[[ $rc = 0 && -n $out && -z $err ]]; chk div.11
# div.12 the diverting path still diverts when the attest FAILS
run_sash 'str x=$(arg 1)
attest x [[ -n $x ]] || exit 1
echo "reached"' ""
[[ $rc = 1 && -z $out ]]; chk div.12
# div.13 resolve, attest passes: the merge takes the THEN version, not the
# else version it used to leak
run_sash 'str x=$(arg 1)
if attest x [[ -n $x ]]; then echo t; else x=d; fi
echo "$x"' keep
[[ $rc = 0 && $out = "t
keep" && -z $err ]]; chk div.13
# div.14 resolve, attest fails: the merge takes the ELSE version
run_sash 'str x=$(arg 1)
if attest x [[ -n $x ]]; then echo t; else x=d; fi
echo "$x"' ""
[[ $rc = 0 && $out = "d" && -z $err ]]; chk div.14
# div.15 the same for a collection subject on the failing path — this is the
# shape of bug-caught-attest-all-fail-path-undefined-handle, which was a
# SIGSEGV when the attested version was defined on one path only
run_sash 'argv 1 a
if attest v in a [[ $v == y ]]; then echo t; else a=(d); fi
for v in @a; do echo "$v"; done' n
[[ $rc = 0 && $out = "d" && -z $err ]]; chk div.15
# div.16 and on its passing path
run_sash 'argv 1 a
if attest v in a [[ $v == y ]]; then echo t; else a=(d); fi
for v in @a; do echo "$v"; done' y
[[ $rc = 0 && $out = "t
y" && -z $err ]]; chk div.16

set -e
tally
