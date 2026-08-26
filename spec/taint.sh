#!/bin/bash
# spec/taint.sh — taint tracking tests (DD#24, DD#50, DD#51, DD#53, DD#61)
set -eu
. common.sh
setup taint
plan=103
echo "1..$plan" >&3

set +e

# 24.1 getenv produces tainted var
run_sash 'x=$(getenv HOME); echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.1

# 24.2 arg produces tainted var
run_sash 'x=$(arg 1); echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.2

# 24.3 read produces tainted var
run_sash 'read -r x; echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.3

# 24.4 mapfile produces tainted var
run_sash 'mapfile -t arr; echo ${arr[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.4

# 24.5 external cmd produces tainted var
run_sash 'x=$(/bin/date); echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.5

# 24.6 read from clean heredoc → clean
run_sash 'read x <<< "hello"; echo $x'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 24.6

# 24.7 user func capture → clean
run_sash 'fn greet { echo hello; }
x=$(greet)
echo $x'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 24.7

# 24.8 tainted var at use site → compile error
run_sash 'x=$(getenv PATH); /bin/ls $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.8

# 24.9 bless makes var clean
run_sash 'x=$(getenv HOME)
bless x [[ $x =~ ^/[a-zA-Z0-9/]+$ ]]
echo $x'
[[ $rc = 0 && -z $err ]]; chk 24.9

# 24.10 bless requires $VAR in test expression
run_sash 'x=$(getenv HOME)
bless x [[ 1 -eq 1 ]]
echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.10

# 24.11 bless requires $VAR on LHS
run_sash 'x=$(getenv HOME)
bless x [[ "foo" = $x ]]
echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.11

# 24.12 bless failure (unconditional) aborts at runtime
run_sash 'x=$(getenv HOME)
bless x [[ $x =~ ^NEVER_MATCH$ ]]
echo $x'
# bless fails at runtime → exits 1, no stdout
[[ $rc != 0 && -z $out ]]; chk 24.12

# 24.13 if bless failure → takes false branch, var still tainted
run_sash 'x=$(getenv HOME)
if bless x [[ $x =~ ^NEVER$ ]]; then
  echo $x
fi'
[[ $rc = 0 && -z $err ]]; chk 24.13

# 24.14 re-assignment from tainted source re-taints
run_sash 'x=$(getenv HOME)
bless x [[ $x =~ ^/[a-zA-Z0-9/]+$ ]]
x=$(getenv USER)
echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.14

# 24.15 literals and $#, $?, $$ are clean
run_sash 'x="hello"; echo $x'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 24.15

# 24.16 bless test SHALL NOT allow external commands or arithmetic
run_sash 'x=$(getenv HOME)
bless x [[ $(echo yes) = yes ]]
echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.16

# 24.17 compiler SHALL NOT auto-bless known external output
run_sash 'd=$(/bin/date +%Y-%m-%d); echo $d'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.17

# 24.18 known hostname — still tainted without explicit bless
run_sash 'h=$(/bin/hostname); echo $h'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.18

# 24.19 getenv without bless — tainted
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
run_sash 'x=$(getenv HOME); bless x [[ $x =~ ^/ ]]; echo $x'
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

# ── DD#61: array taint construction restriction + bless-all ──────────
#
# Construction-restriction and bless-form checks are front-end (validate /
# type_check) and fire in every backend, so they run cross-mode. The
# bless-all *runtime* tests need mapfile + bless enforcement, which the
# janet (IR) and c (SSA) oracles elide; those are guarded to native.

# --- Cross-mode: construction restriction (a tainted value may not enter
#     an array via literal, += append, or a[i]= subscript). ---

# 60.1 array literal with a tainted element → reject
run_sash 'array a=( $(getenv HOME) ); echo ${a[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.1

# 60.2 array literal with multiple tainted elements (the reported bug) → reject
run_sash 'array a=( $(getenv HOME) $(getenv PATH) ); echo ${a[1]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.2

# 60.3 the reported bug WITH a scalar bless present must STILL reject
run_sash 'array a=( $(getenv HOME) $(getenv PATH) )
bless a [[ $a[0] =~ ^\S*$ ]]
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

# 60.13 mapfile from a file → tainted; use without bless rejected
run_sash 'array a=(); mapfile -t a < d60a.txt; echo ${a[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.13

# 60.14 read -a → tainted
run_sash 'array a=(); read -a a; echo ${a[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.14

# ad.4 an AUTO-declared mapfile array (no prior `array a=()`) is still tainted —
#      an element read without bless rejects (the front-end auto-declare
#      registers the collection but must NOT clear the taint).
run_sash 'mapfile -t a < d60a.txt; echo ${a[0]}'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk ad.4

# 60.32 tainted array iterated without bless-all → reject (negative for 60.25)
run_sash 'array a=(); mapfile -t a < d60b.txt; for x in @a; do echo $x; done'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.32

# 60.33 tainted array joined without bless-all → reject (cmdsub-arg use site)
run_sash 'array a=(); mapfile -t a < d60b.txt; echo $(join @a)'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.33

# 60.34 tainted map (mapfile -m) subscripted without bless-all → reject
# (negative for the blessed map subscript 60.29; mirrors the array 60.13)
run_sash 'map m=(); mapfile -m m < d60m.txt; echo ${m[NAME]}'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.34

# 60.35 keys of a tainted (mapfile -m) map without bless-all → reject.
# keys exposes the file-derived keys; a use site like ${m[…]} (60.34).
run_sash 'map m=(); mapfile -m m < d60m.txt; for k in $(keys m); do echo "$k"; done'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.35

# 60.19 bless-all binder must be on the LHS (anti-tautology)
run_sash 'array a=(); mapfile -t a < d60b.txt
bless x in a [[ "foo" = $x ]]
echo ${a[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.19

# 60.20 bless-all predicate must reference the binder
run_sash 'array a=(); mapfile -t a < d60b.txt
bless x in a [[ 1 -eq 1 ]]
echo ${a[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.20

# 60.24 re-reading from a tainted source after bless-all re-taints
run_sash 'array a=(); mapfile -t a < d60b.txt
bless x in a [[ $x =~ ^[a-z]+$ ]]
mapfile -t a < d60b.txt
echo ${a[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.24

# 60.26 scalar bless applied to a (tainted) array → type error
run_sash 'array a=(); mapfile -t a < d60b.txt
bless a [[ ${a[0]} =~ ^[a-z]+$ ]]
echo ${a[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.26

# 60.27 bless-all (1 binder) applied to a scalar → type error
run_sash 'x=$(getenv HOME); bless y in x [[ $y =~ ^/ ]]; echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.27

# 60.28 1-binder bless-all on a map → type error (needs two binders)
run_sash 'map m=(); mapfile -m m < d60m.txt
bless v in m [[ $v =~ ^[A-Za-z]+$ ]]
echo ${m[NAME]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.28

# 60.30 map bless-all: both binders must be validated (value omitted) → reject
run_sash 'map m=(); mapfile -m m < d60m.txt
bless k v in m [[ $k =~ ^[A-Z]+$ ]]
echo ${m[NAME]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 60.30

# --- native+c: clean-construction + bless-all runtime semantics.
#     The c oracle now implements mapfile + RedirectPush/Pop + BlessAll
#     (2026-08-20). janet still elides bless enforcement / lacks mapfile, so
#     it stays skipped (janet deferred). ---
if in_modes native,c; then
    printf 'alpha\nbeta\n' > d60a.txt
    printf 'alpha\nbeta\ngamma\n' > d60b.txt
    printf 'alpha\n123\ngamma\n' > d60c.txt
    printf '' > d60empty.txt
    printf 'NAME=Alice\nCITY=Paris\n' > d60m.txt
    printf 'NAME=Alice\nAGE=30\n' > d60m2.txt

    # 60.9 array of clean literals
    run_sash 'array a=( one two ); echo ${a[0]}'
    [[ $rc = 0 && $out = "one" && -z $err ]]; chk 60.9

    # 60.10 escape hatch: array built from a blessed scalar
    run_sash 'h=$(getenv HOME)
bless h [[ $h =~ ^/ ]]
array a=( "$h" ); echo ${a[0]}'
    [[ $rc = 0 && -n $out && -z $err ]]; chk 60.10

    # 60.11 append of a clean element
    run_sash 'array a=(one); a+=( two ); echo ${a[1]}'
    [[ $rc = 0 && $out = "two" && -z $err ]]; chk 60.11

    # 60.12 subscript-assign of a clean value
    run_sash 'array a=(one two); a[0]=zero; echo ${a[0]}'
    [[ $rc = 0 && $out = "zero" && -z $err ]]; chk 60.12

    # 60.15 bless-all clears taint; element readable (runtime pass)
    run_sash 'array a=(); mapfile -t a < d60b.txt
bless x in a [[ $x =~ ^[a-z]+$ ]]
echo ${a[1]}'
    [[ $rc = 0 && $out = "beta" && -z $err ]]; chk 60.15

    # 60.16 bless-all then read a different element
    run_sash 'array a=(); mapfile -t a < d60b.txt
bless x in a [[ $x =~ ^[a-z]+$ ]]
echo ${a[2]}'
    [[ $rc = 0 && $out = "gamma" && -z $err ]]; chk 60.16

    # 60.17 bless-all runtime failure: one element fails → exit 1, no stdout
    run_sash 'array a=(); mapfile -t a < d60c.txt
bless x in a [[ $x =~ ^[a-z]+$ ]]
echo ${a[0]}'
    [[ $rc != 0 && -z $out ]]; chk 60.17

    # 60.18 if-form bless-all failure → false branch, no abort
    run_sash 'array a=(); mapfile -t a < d60c.txt
if bless x in a [[ $x =~ ^[a-z]+$ ]]; then echo ${a[0]}; fi
echo done'
    [[ $rc = 0 && $out = "done" && -z $err ]]; chk 60.18

    # 60.21 empty array → vacuous pass (∀ over ∅ is true)
    run_sash 'array a=(); mapfile -t a < d60empty.txt
bless x in a [[ $x =~ ^[a-z]+$ ]]
echo done'
    [[ $rc = 0 && $out = "done" && -z $err ]]; chk 60.21

    # 60.22 binder shadows outer var; outer untouched afterward
    run_sash 'x=outer; array a=(); mapfile -t a < d60b.txt
bless x in a [[ $x =~ ^[a-z]+$ ]]
echo $x'
    [[ $rc = 0 && $out = "outer" && -z $err ]]; chk 60.22

    # 60.23 bless-all with a unary predicate on the binder
    run_sash 'array a=(); mapfile -t a < d60b.txt
bless x in a [[ -n $x ]]
echo ${a[0]}'
    [[ $rc = 0 && $out = "alpha" && -z $err ]]; chk 60.23

    # 60.25 bless-all enables iteration over the cleaned array
    run_sash 'array a=(); mapfile -t a < d60b.txt
bless x in a [[ $x =~ ^[a-z]+$ ]]
for w in @a; do echo $w; done'
    [[ $rc = 0 && $out = $'alpha\nbeta\ngamma' && -z $err ]]; chk 60.25

    # 60.29 map bless-all clears, value readable (runtime pass)
    run_sash 'map m=(); mapfile -m m < d60m.txt
bless k v in m [[ $k =~ ^[A-Z]+$ && $v =~ ^[A-Za-z]+$ ]]
echo ${m[NAME]}'
    [[ $rc = 0 && $out = "Alice" && -z $err ]]; chk 60.29

    # 60.36 blessed map: keys + dynamic subscript idiom iterates pairs
    run_sash 'map m=(); mapfile -m m < d60m.txt
bless k v in m [[ $k =~ ^[A-Z]+$ && $v =~ ^[A-Za-z]+$ ]]
for k in $(keys m); do echo "$k=${m[$k]}"; done'
    [[ $rc = 0 && $out = $'NAME=Alice\nCITY=Paris' && -z $err ]]; chk 60.36

    # 60.31 map bless-all runtime failure: a value fails its predicate → exit 1
    run_sash 'map m=(); mapfile -m m < d60m2.txt
bless k v in m [[ $k =~ ^[A-Z]+$ && $v =~ ^[A-Za-z]+$ ]]
echo ${m[NAME]}'
    [[ $rc != 0 && -z $out ]]; chk 60.31

    # ad.1 mapfile into an UNDECLARED array (no prior `array a=()`) auto-declares
    #      it (mirrors argv, DG 2026-07-08). `$(len a)` was a codegen ICE
    #      (`undefined array: a`); now it is the count.
    run_sash 'mapfile -t a < d60a.txt; echo $(len a)'
    [[ $rc = 0 && $out = "2" && -z $err ]]; chk ad.1
    # ad.2 same for `read -a` into an undeclared array
    run_sash 'read -a a < d60a.txt; echo $(len a)'
    [[ $rc = 0 && $out = "1" && -z $err ]]; chk ad.2
    # ad.3 bless-all + element read on an AUTO-declared mapfile array (the
    #      front-end must register the target as a collection, else `bless e in
    #      a` sees a scalar → SB4030). Mirrors the explicit-decl path.
    run_sash 'mapfile -t a < d60a.txt; bless e in a [[ -n $e ]]; echo ${a[0]}'
    [[ $rc = 0 && $out = "alpha" && -z $err ]]; chk ad.3
else
    for id in 60.9 60.10 60.11 60.12 60.15 60.16 60.17 60.18 \
              60.21 60.22 60.23 60.25 60.29 60.31 60.36 ad.1 ad.2 ad.3; do
        skip_chk "$id"
    done
fi

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

# 24.20 a failed bless names the variable and shows its value (native only —
#       the c/janet harnesses discard runtime stderr). Regression for the
#       terse `bless: validation failed`.
if in_modes native,c; then
    run_sash 'x=$(getenv HOME)
bless x [[ $x =~ ^NEVER$ ]]
echo $x'
    [[ $rc = 1 && $rerr == *"bless: x = '"* && $rerr == *"failed the gate" ]]; chk 24.20
else
    skip_chk 24.20
fi

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
# bless at the source, not on a copy. Regression: scalar assignment used to
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
# np.5 bless at the SOURCE, then the copy is a clean read (the DD#24 idiom).
# native,c — janet fakes runtime getenv (janet-getpid-class-intrinsics).
if in_modes native,c; then
    run_sash 'y=$(getenv HOME); bless y [[ -n $y ]]; z=$y; echo "$z"'
    [[ $rc = 0 && -z $err ]]; chk np.5
else
    skip_chk np.5
fi
# np.6 assignment straight from a source is allowed (taints the target). native,c.
if in_modes native,c; then
    run_sash 'z=$(getenv HOME); bless z [[ -n $z ]]; echo "$z"'
    [[ $rc = 0 && -z $err ]]; chk np.6
else
    skip_chk np.6
fi
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
if in_modes native,c; then
    run_sash 'x=$(echo a; echo b); echo "[$x]"'
    [[ $rc = 0 && $out == $'[a\nb]' && -z $err ]]; chk mcs.3
else
    skip_chk mcs.3
fi
# mcs.4 blessing the tainted pipeline capture makes it usable (native+c)
if in_modes native,c; then
    run_sash 'x=$(/bin/echo hi | /usr/bin/tr h H); bless x [[ -n $x ]]; echo $x'
    [[ $rc = 0 && $out = "Hi" && -z $err ]]; chk mcs.4
else
    skip_chk mcs.4
fi

# ── DD#24: a `$( )` that READS already-tainted data is a use-site in EVERY
# value position, not just argument position (a laundering-bypass class). A
# tainted `argv 1` snapshot joined without bless-all must be rejected (SB5020)
# wherever the `$(join …)` appears. Previously only argument/for-word position
# was checked; assignment RHS / subscript / array-literal / test / redirect /
# nested all silently compiled and leaked the args at runtime.
run_sash 'argv 1 a; x=$(join @a); echo $x' a b
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.37
run_sash 'argv 1 a; array b=(); b[0]=$(join @a); echo ${b[0]}' a b
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.38
run_sash 'argv 1 a; array c=($(join @a)); echo hi' a b
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.39
run_sash 'argv 1 a; [[ $(join @a) = x ]] && echo hi' a b
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.40
run_sash 'argv 1 a; echo z > $(join @a)' a b
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.41
run_sash 'argv 1 a; s=$(echo $(join @a)); echo $s' a b
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
# 60.46 control: a blessed-all snapshot joined is clean and runs (native+c;
# janet's _args model doesn't expose the named-array join at runtime)
if in_modes native,c; then
    run_sash 'argv 1 a; bless x in a [[ -n $x ]]; echo $(join a)' one two
    [[ $rc = 0 && $out = "one two" && -z $err ]]; chk 60.46
else
    skip_chk 60.46
fi

set -e
tally
