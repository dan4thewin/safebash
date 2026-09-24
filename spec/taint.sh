#!/bin/bash
# spec/taint.sh — taint tracking tests (DD#24, DD#50, DD#51, DD#53, DD#61, DD#93)
set -eu
. common.sh
setup taint
plan=216
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
attest x [[ "foo" = $x ]]
echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.11

# 24.12 attest failure (unconditional) aborts at runtime
run_sash 'x=$(getenv HOME)
attest x [[ $x =~ ^NEVER_MATCH$ ]]
echo $x'
# attest fails at runtime → exits 1, no stdout
[[ $rc != 0 && -z $out ]]; chk 24.12

# 24.13 if attest failure → takes false branch, var still tainted
run_sash 'x=$(getenv HOME)
if attest x [[ $x =~ ^NEVER$ ]]; then
  echo $x
fi'
[[ $rc = 0 && -z $err ]]; chk 24.13

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
attest x [[ $(echo yes) = yes ]]
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
attest x in a [[ "foo" = $x ]]
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

# 60.18 if-form attest-all failure → false branch, no abort
run_sash '/usr/bin/tr "\n" "\0" < d60c.txt | read a
if attest x in a [[ $x =~ ^[a-z]+$ ]]; then echo ${a[0]}; fi
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

# ── DD#78 disposition-default discipline: a `:-` default is a read position,
# so a tainted value may not sneak through it, and it must be side-effect-free.
# Closes a taint hole (tainted defaults were silently accepted). ──
# dd78.1 tainted var in a :- default (direct use) → SB5020
run_sash 'y=$(getenv HOME); echo ${x:-$y}'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk dd78.1
# dd78.2 tainted source ($(getenv)) in a :- default → SB5020
run_sash 'echo ${x:-$(getenv HOME)}'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk dd78.2
# dd78.3 tainted source ($(arg N)) in a :- default → SB5020
run_sash 'echo ${x:-$(arg 1)}'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk dd78.3
# dd78.4 a clean intrinsic default is accepted ($(getcwd))
run_sash 'x=hi; echo ${x:-$(getcwd)}'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk dd78.4
# dd78.5 a literal default is accepted (x unset → the default)
run_sash 'echo ${x:-plain}'
[[ $rc = 0 && $out = "plain" && -z $err ]]; chk dd78.5
# dd78.6 $(random) default is a side-effect under eager eval → SB5035
run_sash 'echo ${x:-$(random)}'
[[ $rc != 0 && -z $out && $err == *SB5035* ]]; chk dd78.6
# dd78.7 a tainted default cannot sneak through a store either — DD#24 (no
# propagation): the `:-` default is a read, so the assignment itself rejects.
run_sash 'y=$(getenv HOME); z=${x:-$y}; echo $z'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk dd78.7

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
run_sash 'argv 1 a; [[ $(join a) = x ]] && echo hi' a b
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
run_sash 'x=$(getenv HOME); [[ $x = y ]] && echo z'
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
# 60.70 …and an attested one is the documented way through
run_sash 'array a=(x y)
i=$(printf 1)
attest i i?
echo "${a[$i]}"'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 60.70
# 60.71 a literal index never involved a read
run_sash 'map m=([k]=v)
echo "${m[k]}"'
[[ $rc = 0 && $out = "v" && -z $err ]]; chk 60.71
# 60.50 bare-name `join a` reader reached through a `[[ ]]` operand
run_sash 'argv 1 a; [[ $(join a) = x ]] && echo hi' a b
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.50
# 60.51 a tainted array subscript `${a[0]}` (value-flow, not cmdsub) in a test
run_sash '/usr/bin/tr "\n" "\0" < d60b.txt | read a; [[ ${a[0]} = x ]] && echo hi'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.51
# 60.52 bare-name `join a` reader as a redirect target (routed through a
# scalar — a direct `$(…)` redirect *target* is now rejected structurally at
# parse, SB4049, so the taint of the array-join source is exercised via `$x`)
run_sash 'argv 1 a; x=$(join a); echo hi > $x' a b
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.52
# 60.53 `echo $t` (tainted scalar) through a cmdsub in a `[[ ]]` operand
run_sash 'x=$(getenv HOME); [[ $(echo $x) = y ]] && echo z'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.53
# 60.54 control (positive): a CLEAN `$(echo …)` capture still compiles & runs
# — guards the assignment-RHS cmdsub read-check against over-rejection.
run_sash 'y=$(echo hello); echo $y'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 60.54
# 60.55 control (positive): a CLEAN var in a `[[ ]]` operand still runs — guards
# the new Test-operand read-check against over-rejecting clean values.
run_sash 'x=hello; [[ $x = hello ]] && echo yes'
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

# bfb.5 `attest && X` runs X only on pass
run_sash 'v=$(arg 1)
attest v [[ $v =~ ^[0-9]+$ ]] && /bin/echo "valid $v"
/bin/echo done' 42
[[ $rc = 0 && $out = $'valid 42\ndone' && -z $rerr ]]; chk bfb.5

# bfb.6 `attest && X` on fail: X skipped, execution continues (caught, no abort)
run_sash 'v=$(arg 1)
attest v [[ $v =~ ^[0-9]+$ ]] && /bin/echo "valid $v"
/bin/echo reached' abc
[[ $rc = 0 && $out = "reached" && -z $rerr ]]; chk bfb.6

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
# coverage (journal 399). The shape matters: `int n=$(arg 1)` is SB4030, so a
# tainted int arrives through an arithmetic disposition.
# atint.1 the taint backstop still demands the attest — without it this is
# rejected, which is what makes the rest of the block meaningful.
run_sash 'x=$(arg 1); int n=$(( ${x:-0} )); echo "$n"' 7
[[ $rc != 0 && -z $out && $err == *"tainted variable"* ]]; chk atint.1
# atint.2 a tainted int declassified by attest, gate passing.
run_sash 'x=$(arg 1); int n=$(( ${x:-0} )); attest n [[ -n $n ]]; echo "$n"' 7
[[ $rc = 0 && $out = "7" && -z $err ]]; chk atint.2
# atint.3 …and the value really is the converted one, not a constant.
run_sash 'x=$(arg 1); int n=$(( ${x:-0} )); attest n [[ -n $n ]]; echo "$(( n * 2 ))"' 21
[[ $rc = 0 && $out = "42" && -z $err ]]; chk atint.3
# atint.4 the disposition's default survives the attest too.
run_sash 'x=$(arg 1); int n=$(( ${x:-99} )); attest n [[ -n $n ]]; echo "$n"' notanint
[[ $rc = 0 && $out = "99" && -z $err ]]; chk atint.4
# atint.5 a failing gate aborts with exit 1 and names BOTH the variable and
# the value. This is the divergence that broke the tie: the C backend printed
# `n = '9'` (emit_var_value → sash_itoa) while native passed zeros for a
# non-Str slot and printed only `n`. Same program, two diagnostics — native
# was wrong, and the differential oracle is what caught it.
run_sash 'x=$(arg 1); int n=$(( ${x:-0} )); attest n [[ $n == 5 ]]; echo "$n"' 9
[[ $rc = 1 && -z $out && $rerr == *"attest: n = '9' failed the gate"* ]]; chk atint.5
# atint.6 …and the same gate passes on the value it is gating for.
run_sash 'x=$(arg 1); int n=$(( ${x:-0} )); attest n [[ $n == 5 ]]; echo "$n"' 5
[[ $rc = 0 && $out = "5" && -z $err ]]; chk atint.6
# atint.7 the string form names its value the same way — one sentence shape
# for both types, which is what the int case now agrees with.
run_sash 'x=$(arg 1); attest x [[ $x == good ]]; echo "$x"' bad
[[ $rc = 1 && -z $out && $rerr == *"attest: x = 'bad' failed the gate"* ]]; chk atint.7
# atint.8 attest on a CLEAN int is accepted and is a plain copy — it proves
# nothing about taint but must not miscompile.
run_sash 'int n=5; attest n [[ -n $n ]]; echo "$n"'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk atint.8

set -e
tally
