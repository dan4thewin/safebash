#!/bin/bash
# spec/taint.sh — taint tracking tests (DD#24, DD#50, DD#51, DD#53, DD#61)
set -eu
. common.sh
setup taint
plan=64
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

# 60.32 tainted array iterated without bless-all → reject (negative for 60.25)
run_sash 'array a=(); mapfile -t a < d60b.txt; for x in @a; do echo $x; done'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.32

# 60.33 tainted array joined without bless-all → reject (cmdsub-arg use site)
run_sash 'array a=(); mapfile -t a < d60b.txt; echo $(join @a)'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 60.33

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

# --- Native-only: clean-construction + bless-all runtime semantics.
#     The janet/c oracles elide bless enforcement and lack mapfile, so
#     these runtime checks are meaningful only under native codegen. ---
if in_modes native; then
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

    # 60.31 map bless-all runtime failure: a value fails its predicate → exit 1
    run_sash 'map m=(); mapfile -m m < d60m2.txt
bless k v in m [[ $k =~ ^[A-Z]+$ && $v =~ ^[A-Za-z]+$ ]]
echo ${m[NAME]}'
    [[ $rc != 0 && -z $out ]]; chk 60.31
else
    for id in 60.9 60.10 60.11 60.12 60.15 60.16 60.17 60.18 \
              60.21 60.22 60.23 60.25 60.29 60.31; do
        skip_chk "$id"
    done
fi

set -e
tally
