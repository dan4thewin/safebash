#!/bin/bash
# spec/taint.sh — taint tracking tests (DD#24, DD#50, DD#51, DD#53, DD#61, DD#93)
set -eu
. common.sh
setup taint
plan=556
echo "1..$plan" >&3

set +e

# DD#91 + DD#123: trust modifies a cell's stored value, not its type or index.
run_sash 'array a=()
argv 1 a
echo "${a[0]:!}|${a[7]:!}|${a[7]:!-fallback}|${a[0]:!?missing}"
echo A ${a[7]:!~} B' hello
[[ $rc = 0 && $out = $'hello||fallback|hello\nA B' && -z $rerr ]]; chk ctrs.1

run_sash 'array a=(13 hi)
echo "$(( ${a[0]:!-7} )):$(( ${a[1]:!-7} ))"'
[[ $rc = 0 && $out = "13:7" && -z $rerr ]]; chk ctrs.2

run_sash 'int array a=(0 9)
echo "$(( ${a[0]:!-7} )):$(( ${a[1]:!?missing} )):$(( ${a[1]:!} ))"'
[[ $rc = 0 && $out = "7:9:9" && -z $rerr ]]; chk ctrs.3

run_sash 'map m=([k]=13 [bad]=hi)
echo "${m[k]:!}|${m[missing]:!-fallback}|${m[k]:!?missing}"
echo A ${m[missing]:!~} B
echo "$(( ${m[k]:!-7} )):$(( ${m[bad]:!-7} ))"'
[[ $rc = 0 && $out = $'13|fallback|13\nA B\n13:7' && -z $rerr ]]; chk ctrs.4

run_sash 'array a=(13); echo $(( ${a[0]:!} ))'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *disposition* ]]; chk ctrs.5

run_sash 'array a=(); argv 1 a; echo ${a[9]:!?cell-missing}' one
[[ $rc != 0 && -z $out && $rerr == *cell-missing* ]]; chk ctrs.6

run_sash 'map m=([k]=hi); echo $(( ${m[k]:!?cell-not-int} ))'
[[ $rc != 0 && -z $out && $rerr == *cell-not-int* ]]; chk ctrs.7

# A vouched use does not declassify the collection for a later bare read.
run_sash 'array a=(); argv 1 a; echo ${a[0]:!}; echo ${a[0]}' one
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk ctrs.8

run_sash 'array a=(); argv 1 a
echo $a[0]:! $a[7]:!-fallback $a[0]:!?missing
echo A $a[7]:!~ B' hello
[[ $rc = 0 && $out = $'hello fallback hello\nA B' && -z $rerr ]]; chk ctrs.9

run_sash 'array a=(); argv 1 a
array b=(before ${a[7]:!~} ${a[0]:!~} after)
echo $(join b)' hello
[[ $rc = 0 && $out = "before hello after" && -z $rerr ]]; chk ctrs.10

run_sash 'array a=(hello); echo "${a[0]:!~}"'
[[ $rc != 0 && -z $out && $err == *SB5046* && $err == *':!~'* ]]; chk ctrs.11

# Reading only through the cell index is still a read for SB5050.
run_sash 'array a=(hello); int i=0; echo ${a[$i]:!-fallback}'
[[ $rc = 0 && $out = "hello" && -z $rerr ]]; chk ctrs.12

# The separate vouched use keeps the old dead-variable fault from masking
# the index-taint regression. Trusting a cell never vouches for its index.
run_sash 'array a=(hello); str i=$(arg 1); : ${i:!}
echo ${a[$(( ${i:-0} ))]:!}' 0
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk ctrs.13

run_sash 'array a=(hello); str i=$(arg 1); : ${i:!}
str z=${a[$(( ${i:-0} ))]:!}; echo "$z"' 0
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk ctrs.14

run_sash 'array a=(hello); str i=$(arg 1); : ${i:!}
array b=(${a[$(( ${i:-0} ))]:!}); echo $(join b)' 0
[[ $rc != 0 && -z $out && $err == *SB5021* ]]; chk ctrs.15

run_sash 'map m=([k]=hello); str key=$(arg 1); : ${key:!}
echo ${m[$key]:!}' k
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk ctrs.16

run_sash 'array a=(hello); echo ${a[$missing]:!-fallback}'
[[ $rc != 0 && -z $out && $err == *SB5031* && $err == *missing* ]]; chk ctrs.17

# Shared expression walks must see rejected operators nested inside the index.
run_sash 'array a=(hello); int i=0; echo ${a[$(( ${i@Q} ))]:!-fallback}'
[[ $rc != 0 && -z $out && $err == *SB4015* && $err == *'${var@Q}'* ]]; chk ctrs.18

run_sash 'map m=([k]=hello)
array b=(${m[$(arg 1)]:!}); echo $(join b)' k
[[ $rc != 0 && -z $out && $err == *SB5021* ]]; chk ctrs.19

run_sash 'map m=([k]=hello); echo ${m[$(arg 1)]:!}' k
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk ctrs.20

run_sash 'str s=13; echo $(( ${s:!} ))'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *disposition* ]]; chk ctrs.21

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

# 24.6 a builtin pipe is a kernel read too; attest beside the read (DD#127).
run_sash 'echo hello | readline x; attest x [[ $x == hello ]]; echo $x'
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

# --- the SPELLINGS an attest operand may use to name its binder -------
# 24.10/24.11 pin that the test must name the binder, and that only the LHS
# counts. `expr_references_var` walks that LHS, and two of its arms were
# reached by no spec at all: a parameter expansion and a concatenation. Both
# are ordinary ways to write the operand, so an arm that silently answered
# "no" would reject a correct program, and one that silently answered "yes"
# would let an attest through that validates nothing. The negative in atop.3
# is what tells those two apart — without it, an arm hard-wired to `true`
# passes atop.1 and atop.2 both.

# atop.1 a PARAMETER EXPANSION operand names the binder
run_sash 'str h=$(getenv NOPE_XYZ)
attest h [[ ${h:-zz} == zz ]]
echo "ok [$h]"'
[[ $rc = 0 && $out = "ok []" && -z $err ]]; chk atop.1

# atop.2 a CONCATENATION operand names the binder
run_sash 'str h=$(getenv NOPE_XYZ)
attest h [[ ${h}tail == tail ]]
echo "ok [$h]"'
[[ $rc = 0 && $out = "ok []" && -z $err ]]; chk atop.2

# atop.3 the same spelling naming a DIFFERENT variable does not count
run_sash 'str g=zz
str h=$(getenv NOPE_XYZ)
attest h [[ ${g:-qq} == zz ]]
echo "ok [$h]"'
[[ $rc != 0 && -z $out && $err == *SB5020* && $err == *"doesn't reference"* ]]; chk atop.3

# atop.4 a NEGATED predicate still references its binder. `test_references_var`
# recurses through `!`, and nothing reached that arm — a wrong answer here is
# silent either way, so atop.5 pins the other direction.
run_sash 'str h=$(getenv NOPE_XYZ)
attest h [[ ! $h == zz ]]
echo "ok [$h]"'
[[ $rc = 0 && $out = "ok []" && -z $err ]]; chk atop.4

# atop.5 …and a negated predicate naming a different variable still does not
run_sash 'str g=zz
str h=$(getenv NOPE_XYZ)
attest h [[ ! $g == qq ]]
echo "ok [$h]"'
[[ $rc != 0 && -z $out && $err == *SB5020* && $err == *"doesn't reference"* ]]; chk atop.5

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

# 24.16 an attest predicate SHALL NOT run code: no `$( … )`, `$(<…)` or
# `$(( … ))` in it (SB5098; atpred.1, .3-.8, .13-.31)
run_sash 'x=$(getenv HOME)
attest x [[ $(echo yes) == yes ]]
echo $x'
[[ $rc != 0 && -z $out && $err == *:2:13:*SB5098* \
   && $err == *'`$(echo …)` runs `echo`'* ]]; chk 24.16

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
# 24.21-24.29 (journal 537; batch-531 B25, B31; DD#24): a `-` or `!` on a
# reader pipeline's stage must not change the reader's taint. `-` on the
# source stage hid the source, so x came out clean (B25, CRITICAL). `-` or
# `!` on the reader stage hid the clean producer, so 24.22-24.25 got a false
# SB5020 and 24.28/24.29 a false SB5067 (B31).
printf 'secret\n' > b25.txt
# 24.21 `- source | readline x`: x is tainted. It printed the file
run_sash '- /bin/cat b25.txt | readline x
echo "got $x"'
[[ $rc != 0 && -z $out \
   && $err == *':2:6: error [SB5020] tainted variable $x used without attest'* ]]; chk 24.21
# 24.22-25 the marks remain legal; their kernel-read data needs an attest.
run_sash 'echo hi | - readline x
attest x [[ $x == hi ]]
echo "[$x]"'
[[ $rc = 0 && $out = "[hi]" && -z $rerr ]]; chk 24.22
# 24.23 (positive) ... `- read ar`
run_sash 'echo hi | - read ar
attest v in ar [[ -n $v ]]
echo "[${ar[0]}]"'
[[ $rc = 0 && $out = $'[hi\n]' && -z $rerr ]]; chk 24.23
# 24.24 (positive) ... `! readline x` in a condition
run_sash 'if echo hi | ! readline x { echo "neg[${x:!}]" } else { echo "pos[${x:!}]" }'
[[ $rc = 0 && $out = "pos[hi]" && -z $rerr ]]; chk 24.24
# 24.25 (positive) ... `! read ar` in a condition
run_sash 'if echo a | ! read ar { echo "t[${ar[0]:!}]" } else { echo "f[${ar[0]:!}]" }'
[[ $rc = 0 && $out = $'f[a\n]' && -z $rerr ]]; chk 24.25
# 24.26 an external producer still taints a `- readline` reader
run_sash '/bin/echo hi | - readline x
echo "[$x]"'
[[ $rc != 0 && -z $out \
   && $err == *':2:6: error [SB5020] tainted variable $x used without attest'* ]]; chk 24.26
# 24.27 ... and a negated one in a condition
run_sash 'if - /bin/cat b25.txt | ! readline x { echo "neg" } else { echo "pos[$x]" }'
[[ $rc != 0 && -z $out \
   && $err == *':1:65: error [SB5020] tainted variable $x used without attest'* ]]; chk 24.27
# 24.28 `! readline x` in a return producer: the read succeeds, so p fails.
#       Trust the read explicitly to observe the status rather than taint.
run_sash 'str x=init
fn p { return echo abc | ! readline x }
if p { echo "t[${x:!}]" } else { echo "f[${x:!}]" }'
[[ $rc = 0 && $out = "f[abc]" && -z $rerr ]]; chk 24.28
# 24.29 (positive) ... `- readline x`: the read fails at EOF, but its verdict
#       does not join, so `:` decides
run_sash 'str x=init
fn p { return : | - readline x }
if p { echo "t[${x:!}]" } else { echo "f[${x:!}]" }'
[[ $rc = 0 && $out = "t[]" && -z $rerr ]]; chk 24.29
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

# ── DD#127: single-use functions may pass their own taint to the caller ──
# An adjacent in-body attest still works. Without it, the caller receives
# taint and must attest before reading; shared functions are pinned below.
# bfb.17 taint a global and attest it adjacently — the caller reads it clean
run_sash 'array g=()
fn f {
    argv 1 g
    attest v in g [[ -n $v ]]
}
f
for v in @g { /bin/echo $v }' one two
[[ $rc = 0 && $out = $'one\ntwo' && -z $rerr ]]; chk bfb.17

# bfb.18 without the attest, the single-use call taints the caller's global
run_sash 'array g=()
fn f {
    argv 1 g
}
f
for v in @g { /bin/echo $v }'
[[ $rc != 0 && -z $out && $err == *SB5020* && $err != *SB5067* ]]; chk bfb.18

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
[[ $rc != 0 && -z $out && $err == *SB5020* && $err != *SB5067* ]]; chk bfb.20

# bfb.21 the call invalidates the caller's earlier attest
run_sash 'array g=()
argv 1 g
attest v in g [[ -n $v ]]
fn f {
    argv 1 g
}
f
for v in @g { /bin/echo $v }'
[[ $rc != 0 && -z $out && $err == *SB5020* && $err != *SB5067* ]]; chk bfb.21

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

# ── DD#127: an anonymous capture body is single-use ─────────────────────
# Its writes persist (DD#86); leaving taint is legal, reading it is not.
# bfb.23 no read follows the write, so no attest is required
run_sash 'str t=clean
/bin/echo $t
str q=$(t=$(getenv HOME); echo x)
/bin/echo $q'
[[ $rc = 0 && $out = $'clean\nx' && -z $err && -z $rerr ]]; chk bfb.23
# bfb.24 the sanctioned per-use bypass is legal for this single-use body
run_sash 'str t=clean
str q=$(t=$(getenv HOME); echo x)
/bin/echo $q
/bin/echo $t:!'
[[ $rc = 0 && $out == $'x\n'* && -z $err && -z $rerr ]]; chk bfb.24
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
# bfb.28 both the capture and its enclosing function have one call site;
# neither requires an in-body attest when nobody reads the tainted global
run_sash 'str t=clean
/bin/echo $t
fn f {
    str q=$(t=$(getenv HOME); echo x)
    /bin/echo $q
}
f'
[[ $rc = 0 && $out = $'clean\nx' && -z $err && -z $rerr ]]; chk bfb.28

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

# tl.7 builtin pipe data with spaces remains usable after attest.
run_sash 'echo "a b" | readline x
attest x [[ $x == "a b" ]]
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

# dd82.4 a user-function capture of file contents is a kernel read (DD#127).
run_sash 'fn mk { /bin/cat d82a.txt; }
str x=$(mk)
echo $x'
[[ $rc != 0 && -z $out && $err == *SB5020* && $err != *backstop* ]]; chk dd82.4

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

# 93.59 attest-all with NO predicate: `classify_attest` answers None and
#       `handle_attest_stmt` ICEs on it ("`attest` shape is fixed by
#       parse_attest before lowering (SB4014)"). It was not: the scalar
#       path is guarded by a peek, the attest-all path was not, so this
#       parsed clean, slipped `check_attest_form`'s `let … else { return }`,
#       and crashed the compiler. The parser now makes the ICE's claim true.
run_sash 'array a=(p q)
attest x in a
echo done'
[[ $rc != 0 && -z $out && $err == *SB4014* \
   && $err == *"or a test operator in attest-all"* ]]; chk 93.59

# 93.60 …same with two binders (the map spelling), which reached the same ICE
run_sash 'array a=(p q)
attest x y in a
echo done'
[[ $rc != 0 && -z $out && $err == *SB4014* \
   && $err == *"or a test operator in attest-all"* ]]; chk 93.60

# 93.61 positive control: the Part B operator shorthand IS a predicate, so
#       attest-all without `[[ ]]` is still accepted
run_sash 'array a=(p q)
attest x in a -n
echo done'
[[ $rc = 0 && $out = "done" ]]; chk 93.61

# A third binder cannot be classified. Declare the names to get past SB5031,
# which otherwise masks the transform ICE this malformed form used to reach.
run_sash 'str x=p
array a=(p q)
attest x y z in a -n
echo done'
[[ $rc != 0 && -z $out && $err == *SB4014* \
   && $err == *"one array binder or two map binders"* ]]; chk 93.62

run_sash 'str x=p; str y=q; str z=r
array a=(p q)
attest x y z in a [[ -n $x && -n $y && -n $z ]]
echo done'
[[ $rc != 0 && -z $out && $err == *SB4014* \
   && $err == *"one array binder or two map binders"* ]]; chk 93.63

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
run_sash 'int n=5; attest n [[ -n "$n" ]]; echo "$n"'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk atint.5
# atint.6 a failing gate aborts with exit 1 and names BOTH the variable and the
# value. This is the divergence that broke the tie: the C backend printed
# `n = '9'` (emit_var_value → sash_itoa) while native passed zeros for a
# non-Str slot and printed only `n`. Same program, two diagnostics — native was
# wrong, and the differential oracle is what caught it. With the int form now
# validation-only, this message is the whole point of it.
run_sash 'int n=9; attest n [[ "$n" == 5 ]]; echo "$n"'
[[ $rc = 1 && -z $out && $rerr == *"attest: n = '9' failed the gate"* ]]; chk atint.6
# atint.7 the string form names its value the same way — one sentence shape for
# both types, which is what the int case now agrees with.
run_sash 'x=$(arg 1); attest x [[ $x == good ]]; echo "$x"' bad
[[ $rc = 1 && -z $out && $rerr == *"attest: x = 'bad' failed the gate"* ]]; chk atint.7
# atint.8 the attested int is a real value, not a constant folded past.
run_sash 'int n=21; attest n [[ -n "$n" ]]; echo "$(( n * 2 ))"'
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

# 61.23-61.25 DD#61: a SCALAR attest whose name is a collection. The advice has
# to name the binders the collection actually has, or the user follows it into a
# second error. Both arms were cold until 2026-10-04 and were kept on a
# loud-vs-silent argument that nothing executed; these warm them.
# 61.23 scalar attest on a map advises the two-binder form
run_sash 'map m=([k]=v)
attest m [[ -n ${m[k]} ]]
echo "${m[k]}"'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *"attest k v in m"* ]]; chk 61.23

# 61.24 scalar attest on an array advises the one-binder form
run_sash 'array a=(one)
attest a [[ -n ${a[0]} ]]
echo "${a[0]}"'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *"attest x in a"* ]]; chk 61.24

# 61.25 CONTROL: the same scalar attest on an actual scalar is accepted, so the
# two above fail on the collection-ness and not on the form.
run_sash 'str s="one"
attest s [[ -n $s ]]
echo "$s"'
[[ $rc = 0 && $out = "one" && -z $err ]]; chk 61.25
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

# ── capwalk.1-capwalk.11 — a `$( )` body is walked once, before its statement ──
# Batch 531's B29/B30. A capture body used to be walked twice: in full by
# `taint_cmdsub_bodies`, which ran AFTER the statement's own effect, and
# partially by a shallow read check wherever a capture can sit (a right side,
# an argument or operand, an array or map element, an `arg` default). So a
# tainted read in a body was reported twice (three times in an `arg` default),
# and a body that read the name its own statement assigns saw that
# assignment's taint. The shallow check also missed a `-`-suppressed command,
# so `$(- join a ,)` reached only the SSA backstop, with no code and no
# position. Now each body is walked once, as statements, against the state
# before its statement. A declaration walks each initializer's bodies just
# before binding that initializer, so a later initializer sees an earlier one
# (journal 555).

# capwalk.1 a tainted read in a body is reported once (the use of `y` on
#           line 3 is a separate report)
run_sash 'str x=$(getenv HOME)
str y=$(/bin/echo "$x")
echo "$y"'
[[ $rc != 0 && -z $out && $err == *":2:19: error [SB5020] tainted variable \$x"* \
   && $(grep -c ':2:19: error \[SB5020\]' <<< "$err") = 1 \
   && $err == *":3:6: error [SB5020] tainted variable \$y"* ]]; chk capwalk.1
# capwalk.2 `b`'s body runs after `a` is bound, so it sees `a` tainted: one
#           report, not two (walked twice) and not none (walked before `a`)
run_sash 'str a=$(getenv X) b=$(/bin/echo "$a")
attest b [[ -n $b ]]
echo "$b"'
[[ $rc != 0 && -z $out && $err == *":1:33: error [SB5020] tainted variable \$a"* \
   && $(grep -c 'error \[SB5020\]' <<< "$err") = 1 ]]; chk capwalk.2
# capwalk.3 `y` is still clean when the body reads it; only the assignment
#           that the capture feeds taints it
run_sash 'y=hi
y=$(/bin/echo "$y")
attest y [[ $y =~ ^[a-z]+$ ]]
echo "[$y]"'
[[ $rc = 0 && $out = "[hi]" && -z $err && -z $rerr ]]; chk capwalk.3
# capwalk.4 the same holds for a read beside the capture in the right side
run_sash 'y=hi
y="$y$(/bin/echo b)"
attest y [[ $y =~ ^[a-z]+$ ]]
echo "[$y]"'
[[ $rc = 0 && $out = "[hib]" && -z $err && -z $rerr ]]; chk capwalk.4
# capwalk.5 a suppressed `join` of a tainted collection, in a declaration.
#           (`len` reads `a` too: a program whose only reader is `$(- join ..)`
#           meets the false SB5050 of bug-peel-family-classifiers first.)
run_sash 'array a=()
argv 1 a
int n=$(len a)
str y=$(- join a ,)
echo "$n $y"'
[[ $rc != 0 && -z $out \
   && $err == *":4:16: error [SB5020] join of tainted collection 'a' without attest"* ]]; chk capwalk.5
# capwalk.6 ... in a command's argument
run_sash 'array a=()
argv 1 a
int n=$(len a)
echo "$n $(- join a ,)"'
[[ $rc != 0 && -z $out \
   && $err == *":4:19: error [SB5020] join of tainted collection 'a' without attest"* ]]; chk capwalk.6
# capwalk.7 ... in an array element
run_sash 'array a=()
argv 1 a
int n=$(len a)
array b=("$(- join a ,)")
echo "$n ${b[0]}"'
[[ $rc != 0 && -z $out \
   && $err == *":4:20: error [SB5020] join of tainted collection 'a' without attest"* ]]; chk capwalk.7
# capwalk.8 a call's arguments are read before it fills its slots: `fill`
#           refills `g` from argv, but `"${g[0]}"` was read while `g` was clean
run_sash 'fn fill { argv A; echo "$1" }
array g=(x)
fill "${g[0]}" =g'
[[ $rc = 0 && $out = "x" && -z $err && -z $rerr ]]; chk capwalk.8
# capwalk.9 a tainted read in an argument's capture is reported once (the
#           capture's own bare use is a separate report)
run_sash 't=$(getenv HOME)
echo "$(/bin/echo "$t")"'
[[ $rc != 0 && -z $out && $err == *":2:19: error [SB5020] tainted variable \$t"* \
   && $(grep -c ':2:19: error \[SB5020\]' <<< "$err") = 1 \
   && $err == *":2:6: error [SB5020] tainted command substitution used without attest"* ]]; chk capwalk.9
# capwalk.10 ... in a map element's capture
run_sash 't=$(getenv HOME)
map m=([k]=$(/bin/echo "$t"))'
[[ $rc != 0 && -z $out && $err == *":2:24: error [SB5020] tainted variable \$t"* \
   && $(grep -c ':2:24: error \[SB5020\]' <<< "$err") = 1 \
   && $err == *":2:12: error [SB5021] tainted value cannot be placed in map 'm'"* ]]; chk capwalk.10
# capwalk.11 ... in an `arg` default, where it was reported three times
#           (SB4070 is the default's own rule: no `$` in a default)
run_sash 'argv 1 a
t=$(getenv HOME)
x=$(arg 1 $t)'
[[ $rc != 0 && -z $out && $err == *":3:11: error [SB5020] tainted variable \$t"* \
   && $(grep -c ':3:11: error \[SB5020\]' <<< "$err") = 1 \
   && $err == *":3:11: error [SB4070]"* ]]; chk capwalk.11

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
return 0
}
f || exit 1' one two
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
# DD#128: even the sole surviving path cannot resolve taint by a later write.
run_sash 'str c=$(arg 1)
attest c [[ -n $c ]]
str x=$(arg 2)
if [[ $c == y ]]; then x=safe; else exit 1; fi
echo "$x"' y tainted
[[ $rc != 0 && $err == *':5:6: error [SB5020]'* ]]; chk adjpolicy.1
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

# div.17-div.23 — a `{ … }` diverts exactly as a bare `exit` does.
# DG (2026-10-02): "why isn't `attest s g? || { exit 1; }` handled identically
# to `if ! attest s g? { exit 1 }`?" — the two spellings are the same program,
# so a difference is a defect. `block_falls_through` only inspected a trailing
# literal Command, and a group is not one, so every braced divert read as
# falling through and got a false SB5089. That closed off
# `|| { echo "bad input" >&2; exit 1; }` — the shape people actually write,
# and the only way to say anything before diverting, since the group is the
# language's sole multi-statement `||` right operand.
#
# The predicate is shared, so the same blindness hit an `else { … }` body.
# Both are fixed by one recursion; div.19/div.22 are the else-side pins.

# div.17 the minimal braced divert
run_sash 'str x=$(getenv HOME)
attest x [[ $x =~ ^/ ]] || { exit 1; }
echo "$x"'
[[ $rc = 0 && -n $out && -z $err ]]; chk div.17
# div.18 the shape that motivated it: say something, then divert
run_sash 'str x=$(arg 1)
attest x [[ -n $x ]] || { echo "bad input" >&2; exit 7; }
echo "got=$x"' val
[[ $rc = 0 && $out = "got=val" && -z $err ]]; chk div.18
# div.19 the same blindness on an `else` body
run_sash 'str x=$(getenv HOME)
if attest x [[ $x =~ ^/ ]]; then :; else { exit 1; }; fi
echo "$x"'
[[ $rc = 0 && -n $out && -z $err ]]; chk div.19
# div.20 the braced divert really diverts when the attest fails
run_sash 'str x=$(arg 1)
attest x [[ -n $x ]] || { echo "bad input" >&2; exit 7; }
echo "got=$x"' ""
[[ $rc = 7 && -z $out && $rerr == *"bad input"* ]]; chk div.20
# div.21 a braced right that FALLS THROUGH is still SB5089 — looking through
# the group must not weaken R2
run_sash 'str x=$(getenv HOME)
attest x [[ $x =~ ^/ ]] || { echo no; }
echo "$x"'
[[ $rc != 0 && $err == *SB5089* ]]; chk div.21
# div.22 and the else-side guard rail
run_sash 'str x=$(getenv HOME)
if attest x [[ $x =~ ^/ ]]; then :; else { :; }; fi
echo "$x"'
[[ $rc != 0 && $err == *SB5089* ]]; chk div.22
# div.23 `&&` has no failure branch, braced or not
run_sash 'str x=$(getenv HOME)
attest x [[ $x =~ ^/ ]] && { echo ok; }
echo "$x"'
[[ $rc != 0 && $err == *SB5089* ]]; chk div.23

# DD#121 — the negated spelling of a caught `if attest` (div.24-div.32).
#
# DD#121 names three caught forms; `if ! attest … { … }` is none of them, so
# the attest was walked as an ordinary nested body and reported SB5088 — "not
# adjacent" about an attest that is adjacent. DG's fix is a translation, not a
# new form: `if ! attest C { A } else { B }` is the same program as
# `if attest C { B } else { A }`, and the parser now rewrites it at
# construction. No polarity reaches the R2 resolution check or the SSA merge,
# whose "resolve" and "divert" arms would otherwise have to invert in step.
#
# Both spellings of `if` build the `If` through one constructor: a form that
# is unrepresentable in one spelling but not the other is not normalised.

# div.24 the brace form, pass path — the subject is clean afterwards
run_sash 'str x=$(arg 1)
if ! attest x [[ -n $x ]] { exit 7 }
echo "got=$x"' val
[[ $rc = 0 && $out = "got=val" && -z $err ]]; chk div.24
# div.25 ... and the fail path really diverts
run_sash 'str x=$(arg 1)
if ! attest x [[ -n $x ]] { echo "bad input" >&2; exit 7 }
echo "got=$x"' ""
[[ $rc = 7 && -z $out && $rerr == *"bad input"* ]]; chk div.25
# div.26 the then/fi spelling goes through the same constructor
run_sash 'str x=$(arg 1)
if ! attest x [[ -n $x ]]; then exit 7; fi
echo "got=$x"' val
[[ $rc = 0 && $out = "got=val" && -z $err ]]; chk div.26
# div.27 with an `else`, the user's else body is the pass path
run_sash 'str x=$(arg 1)
if ! attest x [[ -n $x ]] { exit 7 } else { echo pass }
echo "got=$x"' val
[[ $rc = 0 && $out == *pass* && $out == *got=val* && -z $err ]]; chk div.27
# div.28 guard rail: a negated form whose divert branch FALLS THROUGH leaves
# the subject tainted at the merge — still SB5089, exactly as unnegated
run_sash 'str x=$(arg 1)
if ! attest x [[ -n $x ]] { echo no }
echo "got=$x"'
[[ $rc != 0 && $err == *SB5089* ]]; chk div.28
# div.29 guard rail: neither arm resolving is SB5089 too
run_sash 'str x=$(arg 1)
if ! attest x [[ -n $x ]] { echo no } else { echo yes }
echo "got=$x"'
[[ $rc != 0 && $err == *SB5089* ]]; chk div.29
# div.30 an `elif` chain NESTS rather than swaps — it hangs off the unnegated
# condition being false, so `if !C {A} elif C2 {B} else {D}` is
# `if C { if C2 {B} else {D} } else {A}`. Here the attest passes and the
# elif is taken; the subject is clean inside it.
run_sash 'str x=$(arg 1)
if ! attest x [[ -n $x ]] { exit 7 } elif [[ $x == hi ]] { echo mid } else { echo other }
echo "got=$x"' hi
[[ $rc = 0 && $out == *mid* && $out == *got=hi* && -z $err ]]; chk div.30
# div.31 ... the same chain's else arm
run_sash 'str x=$(arg 1)
if ! attest x [[ -n $x ]] { exit 7 } elif [[ $x == hi ]] { echo mid } else { echo other }
echo "got=$x"' val
[[ $rc = 0 && $out == *other* && $out == *got=val* && -z $err ]]; chk div.31
# div.32 the rewrite is attest-only: an ordinary negated condition keeps its
# polarity, and `--emit-ir` keeps showing it
run_sash 'if ! /bin/false { echo taken } else { echo not }'
[[ $rc = 0 && $out = "taken" && -z $err ]]; chk div.32

# div.33-div.37 — the MAP form of `if attest`, which was specced nowhere.
#
# `AttestKind::of` has exactly one caller (`lower_if_attest`) and its three
# arms map 1:1 onto `AttestForm`. Seven specs used `if attest v in a` (the
# array arm); none used `if attest k v in m`, so the `AttestForm::Map` arm
# was cold — found by ranking `coverage.db`.
#
# That matters because div.15/div.16 exist for the array arm specifically to
# pin bug-caught-attest-all-fail-path-undefined-handle, a SIGSEGV from an
# attested version defined on only one path. The map form reaches the same
# `AttestKind::Coll` lowering with two binders instead of one and had no
# equivalent. These are its twins.

# div.33 map form, passing path: the then-branch runs and the map is clean
# afterwards (the retag landed).
run_sash 'argv 1 a
attest e in a [[ -n $e ]]
map m = @{a:~}
if attest k v in m [[ $k =~ ^[A-Z]+$ && $v =~ ^[a-z]+$ ]]; then echo t; else m=([K]=d); fi
echo ${m[K]}' K v
[[ $rc = 0 && $out = "t
v" && -z $err ]]; chk div.33

# div.34 ... and the failing path takes the ELSE version at the merge. This
# is div.15's shape: the attested version exists on one path only.
run_sash 'argv 1 a
attest e in a [[ -n $e ]]
map m = @{a:~}
if attest k v in m [[ $k =~ ^[A-Z]+$ && $v =~ ^[a-z]+$ ]]; then echo t; else m=([K]=d); fi
echo ${m[K]}' k v
[[ $rc = 0 && $out = "d" && -z $err ]]; chk div.34

# div.35 the key binder decides the branch, not just the value
run_sash 'argv 1 a
attest e in a [[ -n $e ]]
map m = @{a:~}
if attest k v in m [[ $k =~ ^[A-Z]+$ && $v =~ ^[a-z]+$ ]]; then echo t; else echo f; fi' k v
[[ $rc = 0 && $out = "f" && -z $err ]]; chk div.35

# div.36 ... and so does the value binder
run_sash 'argv 1 a
attest e in a [[ -n $e ]]
map m = @{a:~}
if attest k v in m [[ $k =~ ^[A-Z]+$ && $v =~ ^[a-z]+$ ]]; then echo t; else echo f; fi' K V
[[ $rc = 0 && $out = "f" && -z $err ]]; chk div.36

# div.37 the both-binders rule (60.30) holds in the `if` position too — a
# predicate that validates only the key is rejected, not silently accepted.
run_sash 'argv 1 a
attest e in a [[ -n $e ]]
map m = @{a:~}
if attest k v in m [[ $k =~ ^[A-Z]+$ ]]; then echo t; else echo f; fi' K v
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk div.37

# capres.1-20 (journal 538; batch-531 B37; DD#23, DD#24): command resolution
# reaches every `$( )` body the checkers reach. resolve.rs kept a private copy
# of the walk that skipped a parameter expansion's index and the leaves of
# `$(( ))` and `(( ))`, so a command there kept its bare head, which the taint
# pass judges by its spelling. Base compiled .1/.2/.6-.8/.12/.14-.16 and
# printed what the tainted key selected; it caught the rest only in the SSA
# backstop, with no position and a synthetic name. The keys are `map` keys:
# an indexed-array index may not be a capture (DD#80, SB4030).
printf k > b37.key
# capres.1 an assignment value
run_sash 'map m=([k]=v)
b=${m[$(cat b37.key)]:-dflt}
echo "$b"'
[[ $rc != 0 && -z $out \
   && $err == *':2:3: error [SB5020] tainted command substitution used without attest'* ]]; chk capres.1
# capres.2 a declaration value
run_sash 'map m=([k]=v)
str b=${m[$(cat b37.key)]:-dflt}
echo "$b"'
[[ $rc != 0 && -z $out \
   && $err == *':2:7: error [SB5020] tainted command substitution used without attest'* ]]; chk capres.2
# capres.3 a command argument
run_sash 'map m=([k]=v)
echo "${m[$(cat b37.key)]:-dflt}"'
[[ $rc != 0 && -z $out \
   && $err == *':2:6: error [SB5020] tainted command substitution used without attest'* ]]; chk capres.3
# capres.4 an assignment target's index
run_sash 'map m=([k]=v)
m[${m[$(cat b37.key)]:-k}]=z
echo "${m[k]} ${m[v]:-none}"'
[[ $rc != 0 && -z $out \
   && $err == *':2:3: error [SB5020] tainted command substitution used without attest'* ]]; chk capres.4
# capres.5 a `[[ ]]` operand
run_sash 'map m=([k]=v)
if [[ ${m[$(cat b37.key)]:-q} == v ]] {
  echo yes
}'
[[ $rc != 0 && -z $out \
   && $err == *':2:7: error [SB5020] tainted command substitution used without attest'* ]]; chk capres.5
# capres.6 an array literal's element
run_sash 'map m=([k]=v)
array q=(${m[$(cat b37.key)]:-d})
echo "${q[0]}"'
[[ $rc != 0 && -z $out \
   && $err == *":2:10: error [SB5021] tainted value cannot be placed in array 'q'"* ]]; chk capres.6
# capres.7 a map literal's value
run_sash 'map m=([k]=v)
map q=([z]=${m[$(cat b37.key)]:-d})
echo "${q[z]}"'
[[ $rc != 0 && -z $out \
   && $err == *":2:12: error [SB5021] tainted value cannot be placed in map 'q'"* ]]; chk capres.7
# capres.8 a `$(( ))` operand
run_sash 'map n=([k]=5)
int x=$(( ${n[$(cat b37.key)]:-0} + 1 ))
echo "$x"'
[[ $rc != 0 && -z $out \
   && $err == *':2:7: error [SB5020] tainted command substitution used without attest'* ]]; chk capres.8
# capres.9 an index inside `$(( ))`
run_sash 'map v=([k]=1)
int array w=(5 6)
echo $(( w[${v[$(cat b37.key)]:-0}] + 1 ))'
[[ $rc != 0 && -z $out \
   && $err == *':3:6: error [SB5020] tainted command substitution used without attest'* ]]; chk capres.9
# capres.10 an expansion's index inside another's index
run_sash 'map v=([k]=1)
array w=(5 6)
echo $(( ${w[${v[$(cat b37.key)]:-0}]:-0} + 1 ))'
[[ $rc != 0 && -z $out \
   && $err == *':3:6: error [SB5020] tainted command substitution used without attest'* ]]; chk capres.10
# capres.11 a `(( ))` condition
run_sash 'map n=([k]=5)
if (( ${n[$(cat b37.key)]:-0} > 0 )) {
  echo yes
}'
[[ $rc != 0 && -z $out \
   && $err == *':2:4: error [SB5020] tainted command substitution used without attest'* ]]; chk capres.11
# capres.12 a `(( ))` statement
run_sash 'map n=([k]=5)
int x=0
(( x = ${n[$(cat b37.key)]:-0} ))
echo "$x"'
[[ $rc != 0 && -z $out \
   && $err == *':3:1: error [SB5020] tainted command substitution used without attest'* ]]; chk capres.12
# capres.13 a `(( ))` assignment target's index
run_sash 'map v=([k]=1)
int array w=(5 6)
(( w[${v[$(cat b37.key)]:-0}] = 9 ))
echo "${w[1]}"'
[[ $rc != 0 && -z $out \
   && $err == *':3:1: error [SB5020] tainted command substitution used without attest'* ]]; chk capres.13
# capres.14 in a function body
run_sash 'map m=([k]=v)
fn f {
  b=${m[$(cat b37.key)]:-dflt}
  echo "$b"
}
f'
[[ $rc != 0 && -z $out \
   && $err == *':3:5: error [SB5020] tainted command substitution used without attest'* ]]; chk capres.14
# capres.15 in a capture body
run_sash 'map m=([k]=v)
y=$(echo ${m[$(cat b37.key)]:-dflt})
echo "$y"'
[[ $rc != 0 && -z $out \
   && $err == *':2:10: error [SB5020] tainted command substitution used without attest'* ]]; chk capres.15
# Capture-index resolution also runs in EXIT.
run_sash 'map m=([k]=v)
EXIT {
  b=${m[$(cat b37.key)]:-dflt}
  echo "$b"
}'
[[ $rc != 0 && -z $out \
   && $err == *':3:5: error [SB5020] tainted command substitution used without attest'* ]]; chk h128.capres.16
# capres.17 (positive) a builtin in the index stays a builtin, so it is clean
run_sash 'map m=([k]=v)
b=${m[$(echo k)]:-dflt}
echo "$b"'
[[ $rc = 0 && $out = v && -z $rerr ]]; chk capres.17
# capres.18 (positive) ... and a function call stays a call, inside `$(( ))`
run_sash 'map n=([k]=5)
fn g { echo k }
int x=$(( ${n[$(g)]:-0} + 1 ))
echo "$x"'
[[ $rc = 0 && $out = 6 && -z $rerr ]]; chk capres.18
# capres.19 (positive) ... inside a `(( ))` statement
run_sash 'map n=([k]=5)
int x=0
(( x = ${n[$(echo k)]:-0} ))
echo "$x"'
[[ $rc = 0 && $out = 5 && -z $rerr ]]; chk capres.19
# capres.20 (positive) under --no-taint, the resolved head runs
SASH_CFLAGS="${SASH_CFLAGS:-} --no-taint" run_sash 'map m=([k]=v)
b=${m[$(cat b37.key)]:-dflt}
echo "$b"'
[[ $rc = 0 && $out = v && -z $rerr ]]; chk capres.20

# ── DD#95 inside attest predicates ──
# Both backends evaluated a predicate's `&&` / `||` eagerly, so a right-hand
# `${y:?}` aborted a program whose result was already decided (fixed
# 2026-10-31, journal 550). The if-form attest-all also ran every later
# cell's predicate after the first failure, and the C backend's statement
# form printed a value in its gate message (both fixed 2026-10-31, journal
# 553). atlazy.1 is not here: its predicate held a `$(( ))`, which SB5098
# now rejects (atpred.17).

# atlazy.2 a passing left operand decides an attest's `||`: a right-hand
# `${y:?}` is never evaluated
run_sash 'str y=""
x=$(arg 1)
attest x [[ -n $x || ${y:?unset} == b ]]
echo "ok $x"' v
[[ $rc = 0 && $out = 'ok v' && -z $err && -z $rerr ]]; chk atlazy.2

# atlazy.3 a failing left operand decides `&&`: the gate fails, unevaluated
run_sash 'str y=""
x=$(arg 1)
attest x [[ $x == ok && ${y:?unset} == b ]]
echo "ok $x"' bad
[[ $rc = 1 && -z $out && -z $err \
   && $rerr == *"attest: x = 'bad' failed the gate"* && $rerr != *unset* ]]; chk atlazy.3

# atlazy.4 the caught `if attest` form is lazy as well
run_sash 'str y=""
x=$(arg 1)
if attest x [[ $x == ok || ${y:?unset} == b ]] { echo "ok $x" } else { x=none; echo "no $x" }' ok
[[ $rc = 0 && $out = 'ok ok' && -z $err && -z $rerr ]]; chk atlazy.4

# atlazy.5 the if-form attest-all takes the else branch at the first failing
# cell; no later cell's predicate runs
run_sash 'str y=""
argv 1 a
if attest v in a [[ $v == ok && ${y:?unset} == b ]] { echo all } else { a=(d); echo some }
echo "done ${a[0]}"' bad ok
[[ $rc = 0 && $out = $'some\ndone d' && -z $err && -z $rerr ]]; chk atlazy.5

# atlazy.6 ... for a map too, in insertion order
run_sash 'str y=""
argv 1 a
attest e in a [[ -n $e ]]
map m = @{a:~}
if attest k v in m [[ $k =~ ^[A-Z]+$ && $v =~ ^[a-z]+$ && ${y:?unset} == b ]] { echo all } else { m=([K]=d); echo some }
echo "done ${m[K]}"' k v K w
[[ $rc = 0 && $out = $'some\ndone d' && -z $err && -z $rerr ]]; chk atlazy.6

# atlazy.7 the if-form attest-all, with every cell passing on its left
# operand: no cell's right operand runs
run_sash 'str y=""
argv 1 a
if attest v in a [[ $v == ok || ${y:?unset} == b ]] { echo all } else { a=(d); echo some }
echo "done ${a[0]}"' ok ok
[[ $rc = 0 && $out = $'all\ndone ok' && -z $err && -z $rerr ]]; chk atlazy.7

# atlazy.8 the statement form names the collection, with no value clause, in
# both backends (DD#93): no one value failed the gate
run_sash 'argv 1 a
attest v in a [[ $v == ok ]]
echo "done ${a[0]}"' bad ok
[[ $rc = 1 && -z $out && -z $err && $rerr == *'attest: a failed the gate'* \
   && $rerr != *"= '"* ]]; chk atlazy.8

# atlazy.9 ... for a map as well, where the key may be what failed
run_sash 'argv 1 a
attest e in a [[ -n $e ]]
map m = @{a:~}
attest k v in m [[ $k =~ ^[A-Z]+$ && $v =~ ^[a-z]+$ ]]
echo "done ${m[K]}"' k v K w
[[ $rc = 1 && -z $out && -z $err && $rerr == *'attest: m failed the gate'* \
   && $rerr != *"= '"* ]]; chk atlazy.9

# ── A caught attest evaluates its predicate once and cleans what it checked ──
# The scalar `if attest` tested its predicate in the condition, then again
# through the aborting statement gate in its pass block. A file test stat'd
# its path twice, and an answer that changed in between aborted the *caught*
# form instead of choosing a branch. The caught attest-all over a DD#100
# slot checked the caller's collection but retagged the slot name, which no
# version defines: native codegen ICEd, or a later read of the slot failed
# the SSA taint backstop on a program the validator had accepted.

# atonce.1 one stat, so a second answer that would differ is never asked for
: > atonce.file
run_sash_under 'x=$(getenv ATONCE)
if attest x [[ -f $x ]] { echo pass } else { x=none; echo fail }
echo "end $x"' env ATONCE="$PWD/atonce.file" strace -qq -o atonce.trace \
    -P "$PWD/atonce.file" -e inject=newfstatat:error=ENOENT:when=2
[[ $rc = 0 && $out = $'pass\nend '"$PWD/atonce.file" && -z $err && -z $rerr \
   && $(grep -c . atonce.trace) = 1 ]]; chk atonce.1

# atonce.2 ... in the `||` form too, which lowers to the same caught attest
run_sash_under 'x=$(getenv ATONCE)
attest x [[ -f $x ]] || { echo caught; exit 4 }
echo "end $x"' env ATONCE="$PWD/atonce.file" strace -qq -o atonce.trace \
    -P "$PWD/atonce.file" -e inject=newfstatat:error=ENOENT:when=2
[[ $rc = 0 && $out = "end $PWD/atonce.file" && -z $err && -z $rerr \
   && $(grep -c . atonce.trace) = 1 ]]; chk atonce.2
rm -f atonce.file atonce.trace

# atslot.7 a body fills its own slot, attests it in the caught form and reads
# it after the `if`: the pass branch retags the caller's collection
run_sash 'fn f {
  argv 1 A
  if attest v in A [[ $v != z ]] { echo "ok ${A[0]}" } else { echo bad; exit 1 }
  echo "after ${A[1]}"
}
array a=()
f =a' a b
[[ $rc = 0 && $out = $'ok a\nafter b' && -z $err && -z $rerr ]]; chk atslot.7

# atslot.8 ... in the `||` form, with no later read: the retag alone reached
# native codegen
run_sash 'fn f {
  argv 1 A
  attest v in A [[ $v != z ]] || exit 3
  echo ok
}
array a=()
f =a' a b
[[ $rc = 0 && $out = ok && -z $err && -z $rerr ]]; chk atslot.8


# ── DD#66: a dispatch pattern is a test operand ─────────────────────────────
# The validator checked a dispatch's subject and arm bodies but never its
# patterns, so a tainted read there was left to the SSA backstop (no code, no
# position) — and in a capture, to a codegen ICE.

# dsptaint.1 a tainted variable in a pattern, at the pattern's position
run_sash 'x=$(/bin/echo a)
[[ a == "$x" { echo P } || { echo other } ]]'
[[ $rc != 0 && -z $out \
   && $err == *"2:9: error [SB5020] tainted variable \$x used without attest"* ]]; chk dsptaint.1

# dsptaint.2 an external capture as a later pattern
run_sash 'x=a
[[ $x == b { echo B } "$(/bin/echo a)" { echo P } || { echo other } ]]'
[[ $rc != 0 && -z $out \
   && $err == *"2:23: error [SB5020] tainted command substitution used without attest"* ]]; chk dsptaint.2

# dsptaint.3 control: an attested variable in a pattern compiles and matches
run_sash 'x=$(/bin/echo a)
attest x [[ $x =~ ^[a-z]+$ ]]
[[ a == b "$x" { echo "P $x" } || { echo other } ]]'
[[ $rc = 0 && $out = "P a" && -z $err && -z $rerr ]]; chk dsptaint.3

# ── atpred.1-atpred.31: an attest predicate tests values; it does not run
# code (SB5098, DD#24; strict, DG 2026-10-31, journal 552) ──
# A `[[ ]]` predicate holds no `$( … )` of any kind, no `$(<file)` and no
# `$(( … ))`, at any depth; a DD#93 Part D conversion's value calls nothing.
# What a predicate may READ is SB5020's (atpred.13-.16). Before the rule a
# body, a function, an external or a statement builtin there reached a
# backend "should have been lifted" ICE in both modes; echo, the value
# intrinsics, `getenv`, `$(<file)` and `$(( ))` evaluated in place, so a
# `getenv` or `$(<file)` comparand blessed whatever the attacker supplied.
# The skipped ids atpred.2 and .9-.12 pinned in-place evaluation the strict
# rule removes; their programs are rejected at .17-.19.

# atpred.1 a user function in a predicate capture would receive the value
run_sash 'fn f { echo b }
x=$(arg 1)
attest x [[ $x == $(f) ]]
echo "$x"' b
[[ $rc != 0 && -z $out && $err == *:3:19:*SB5098* \
   && $err == *'`$(f …)` calls function `f`'* ]]; chk atpred.1

# atpred.3 ... including a bare int-function name in the DD#93 Part D form
run_sash 'fn f -> int { return 4 }
s=$(getenv SHLVL)
attest s (( n = s + f ))
echo "$n"'
[[ $rc != 0 && -z $out && $err == *SB5098* \
   && $err == *'its arithmetic calls function `f`'* ]]; chk atpred.3

# atpred.4 a capture body of several statements runs code
run_sash 'x=$(arg 1)
attest x [[ $x == $(y=b; echo $y) ]]
echo "$x"' b
[[ $rc != 0 && -z $out && $err == *:2:19:*SB5098* \
   && $err == *'runs something other than one plain command'* ]]; chk atpred.4

# atpred.5 a suppressed capture lowers as a body, even with an in-place head
run_sash 'x=$(arg 1)
attest x [[ $x == $(- toupper b) ]]
echo "$x"' B
[[ $rc != 0 && -z $out && $err == *:2:19:*SB5098* \
   && $err == *'`$(- …)` is a suppressed capture'* ]]; chk atpred.5

# A statement builtin runs code; the predicate rule reaches EXIT.
run_sash 'EXIT {
  x=$(getenv HOME)
  attest x [[ $x == $(read y) ]]
  echo "$x"
}
echo main'
[[ $rc != 0 && -z $out && $err == *:3:21:*SB5098* \
   && $err == *'`$(read …)` runs `read`'* ]]; chk h128.atpred.6

# atpred.7 a shape rule, not a taint rule: it holds under --no-taint, where
# SB5020 is silent
SASH_CFLAGS=--no-taint run_sash 'x=$(arg 1)
attest x [[ $x == $(/bin/echo b) ]]
echo "$x"' b
[[ $rc != 0 && -z $out && $err == *:2:19:*SB5098* \
   && $err == *'runs `/bin/echo`'* && $err != *SB5020* ]]; chk atpred.7

# atpred.8 a pipeline body is not one plain command
SASH_CFLAGS=--no-taint run_sash 'x=$(arg 1)
attest x [[ $x == $(echo a | /usr/bin/tr a b) ]]
echo "$x"' b
[[ $rc != 0 && -z $out && $err == *:2:19:*SB5098* \
   && $err == *'runs something other than one plain command'* ]]; chk atpred.8

# atpred.13 a taint SOURCE read inline vouches for nothing, even when no
# other name is tainted (the check used to return early in that case)
run_sash 'x=$(arg 1)
attest x [[ $x == $(getenv USER) ]]
echo "$x"' b
[[ $rc != 0 && -z $out && $err == *:2:19:*SB5020* \
   && $err == *'tainted command substitution used without attest'* ]]; chk atpred.13

# atpred.14 ... nor does file contents
run_sash 'x=$(arg 1)
attest x [[ $x == $(</etc/hostname) ]]
echo "$x"' b
[[ $rc != 0 && -z $out && $err == *:2:19:*SB5020* \
   && $err == *'tainted file contents'* ]]; chk atpred.14

# atpred.15 ... nor in the caught `if attest` form
run_sash 'x=$(arg 1)
if attest x [[ $x == $(getenv USER) ]]; then echo "ok $x"; else x=none; fi
echo "$x"' b
[[ $rc != 0 && -z $out && $err == *:2:22:*SB5020* \
   && $err == *'tainted command substitution used without attest'* ]]; chk atpred.15

# atpred.16 a DD#93 Part D value's `${…}` import is a read like any other,
# rejected by the front end rather than left to the unlocated SSA backstop
run_sash 't=$(getenv USER)
s=$(arg 1)
attest s (( n = ${t:-0} + s ))
echo "$n"' 5
[[ $rc != 0 && -z $out && $err == *SB5020* \
   && $err == *'tainted variable ${t ...} used without attest'* \
   && $err != *backstop* ]]; chk atpred.16

# atpred.17 no `$(( ))` in a predicate, even quoted; the rule reaches an
# attest inside a function body (a quoted word reports at its `"`)
run_sash 'fn g {
  x=$(arg 1)
  attest x [[ -n $x && "$(( 2 + 1 ))" == 3 ]]
  echo "$x"
}
g' b
[[ $rc != 0 && -z $out && $err == *:3:24:*SB5098* \
   && $err == *'`$(( … ))` evaluates arithmetic'* ]]; chk atpred.17

# atpred.18 a value intrinsic is code too
run_sash 'x=$(arg 1)
attest x [[ $x == $(toupper b) ]]
echo "$x"' B
[[ $rc != 0 && -z $out && $err == *:2:19:*SB5098* \
   && $err == *'`$(toupper …)` runs `toupper`'* ]]; chk atpred.18

# atpred.19 ... even one that reads only the attested name, under --no-taint
SASH_CFLAGS=--no-taint run_sash 'x=$(arg 1)
attest x [[ $x == "$(tolower "$x")" ]]
echo "$x"' abc
[[ $rc != 0 && -z $out && $err == *:2:19:*SB5098* \
   && $err == *'`$(tolower …)` runs `tolower`'* && $err != *SB5020* ]]; chk atpred.19

# atpred.20 the rule reaches the caught `if attest` form
run_sash 'x=$(arg 1)
if attest x [[ $x == "$(echo b)" ]] { echo "ok $x" } else { x=none; echo "no $x" }' b
[[ $rc != 0 && -z $out && $err == *:2:22:*SB5098* \
   && $err == *'`$(echo …)` runs `echo`'* ]]; chk atpred.20

# atpred.21 `$(<file)` reads a file: a shape rule, so --no-taint too
SASH_CFLAGS=--no-taint run_sash 'x=$(arg 1)
attest x [[ $x == $(</etc/hostname) ]]
echo "$x"' b
[[ $rc != 0 && -z $out && $err == *:2:19:*SB5098* \
   && $err == *'`$(<…)` reads a file'* && $err != *SB5020* ]]; chk atpred.21

# atpred.22 at any depth: a capture in a map key (reported at the `${`)
run_sash 'map m=([k]=b)
x=$(arg 1)
attest x [[ $x == ${m[$(echo k)]} ]]
echo "$x"' b
[[ $rc != 0 && -z $out && $err == *:3:19:*SB5098* \
   && $err == *'`$(echo …)` runs `echo`'* ]]; chk atpred.22

# atpred.23 ... and arithmetic in one
run_sash 'map m=([1]=b)
x=$(arg 1)
attest x [[ $x == ${m[$(( 1 ))]} ]]
echo "$x"' b
[[ $rc != 0 && -z $out && $err == *:3:19:*SB5098* \
   && $err == *'`$(( … ))` evaluates arithmetic'* ]]; chk atpred.23

# atpred.24 a Part D value calls no function, as a sexpr either
run_sash 'fn f -> int { return 4 }
s=$(arg 1)
attest s (( n = (f) + s ))
echo "$n"' 1
[[ $rc != 0 && -z $out && $err == *:3:1:*SB5098* \
   && $err == *'its arithmetic calls function `f`'* ]]; chk atpred.24

# atpred.25 ... nor an intrinsic
run_sash 's=$(arg 1)
attest s (( n = (strlen $s) + s ))
echo "$n"' 1
[[ $rc != 0 && -z $out && $err == *:2:1:*SB5098* \
   && $err == *'its arithmetic calls `strlen`'* ]]; chk atpred.25

# atpred.26 the `s///` sugar is a `$(gsub …)` call (DD#63), and the message
# names both spellings
run_sash 'x=$(arg 1)
attest x [[ $x == "${x:s/[^a-z]//g}" ]]
echo "ok $x"' abc
[[ $rc != 0 && -z $out && $err == *:2:19:*SB5098* \
   && $err == *'`$(gsub …)` runs `gsub` (a `${…:s/…/…/g}` expansion is that call)'* ]]; chk atpred.26

# atpred.27 the rule reaches the attest-all form
run_sash 'argv a
attest x in a [[ $x == "$(echo b)" ]]
echo "${a[0]}"' b
[[ $rc != 0 && -z $out && $err == *:2:24:*SB5098* \
   && $err == *'`$(echo …)` runs `echo`'* ]]; chk atpred.27

# atpred.28 control: comparisons, regex matches, file tests and a `${…}`
# operand stay legal ...
run_sash 'x=$(arg 1)
attest x [[ $x == abc || $x =~ ^z+$ || -f $x || ${x:-d} == d ]]
echo "ok $x"' abc
[[ $rc = 0 && $out = 'ok abc' && -z $err && -z $rerr ]]; chk atpred.28

# atpred.29 ... and still decide the gate
run_sash 'x=$(arg 1)
attest x [[ $x == abc || $x =~ ^z+$ || -f $x || ${x:-d} == d ]]
echo "ok $x"' no
[[ $rc = 1 && -z $out && -z $err \
   && $rerr == *"attest: x = 'no' failed the gate"* ]]; chk atpred.29

# atpred.30 the rule reaches an attest inside a capture body
run_sash 'y=$(x=$(arg 1); attest x [[ $x == $(echo b) ]]; echo "$x")
echo "$y"' b
[[ $rc != 0 && -z $out && $err == *:1:35:*SB5098* \
   && $err == *'`$(echo …)` runs `echo`'* ]]; chk atpred.30

# atpred.31 the sugar is the one way code reaches a Part D value's `${…}`
# import; reported at the attest
run_sash 't=2
s=$(arg 1)
attest s (( n = ${t:s/2/3/} + s ))
echo "$n"' 1
[[ $rc != 0 && -z $out && $err == *:3:1:*SB5098* \
   && $err == *'`$(sub …)` runs `sub` (a `${…:s/…/…/}` expansion is that call)'* ]]; chk atpred.31

# ── DD#127: global summaries apply at calls, not definitions ────────────

# A later taint cannot travel backwards into an earlier call.
run_sash 'str x=safe
fn show { echo "$x" }
show
x=$(/bin/echo dirty)'
[[ $rc = 0 && $out = safe && -z $err && -z $rerr ]]; chk gflow.1

# Nor can a later overwrite hide an unsafe call. Report at that call.
run_sash 'str x=safe
fn show { echo "$x" }
x=$(/bin/echo dirty)
show
x=safe'
[[ $rc != 0 && $err == *":4:1: error [SB5020]"*
   && $err == *'`show`, called here with `x` tainted, reports at 2:'* ]]; chk gflow.2

# Received taint is not adjacent to the callee's first statement.
run_sash 'str x=safe
fn check { attest x [[ -n $x ]]; echo "$x" }
x=$(/bin/echo dirty)
check'
[[ $rc != 0 && $err == *":4:1: error [SB5088]"*
   && $err == *'`check`, called here with `x` tainted'* ]]; chk gflow.3

# A single-use call is one compound statement for caller-side adjacency.
run_sash 'str x=clean
fn fill { x=$(/bin/echo dirty) }
fill
attest x [[ $x == dirty ]]
echo "$x"'
[[ $rc = 0 && $out = dirty && -z $err && -z $rerr ]]; chk gflow.4

# Shared bodies own their re-taint even when the caller was already tainted.
# Attribute it once, to the last source, and name both call sites.
run_sash 'str x=clean
fn fill {
    x=$(/bin/echo first)
    x=$(/bin/echo second)
}
x=$(/bin/echo initial)
fill
fill
echo "${x:!}"'
[[ $rc != 0 && $(grep -c 'error \[SB5067\]' <<<"$err") = 1
   && $err == *":4:5: error [SB5067]"*
   && $err == *'shared (it is called at 7:1, 8:1)'* ]]; chk gflow.5

# Inherited-read witnesses relay through a chain of summaries.
run_sash 'str x=safe
fn leaf { echo "$x" }
fn middle { leaf }
x=$(/bin/echo dirty)
middle
x=safe'
[[ $rc != 0 && $err == *":5:1: error [SB5020]"*
   && $err == *'`middle`, called here'*'`leaf`, called here'* ]]; chk gflow.6

# Capturing the sole call does not change its outward global obligation.
run_sash 'str x=clean
fn fill { x=$(/bin/echo dirty); echo value }
str result=$(fill)
attest x [[ $x == dirty ]]
echo "$result $x"'
[[ $rc = 0 && $out = "value dirty" && -z $err && -z $rerr ]]; chk gflow.7

# An anonymous capture's taint reaches the next statement.
run_sash 'str x=clean
str result=$(x=$(/bin/echo dirty); echo value)
echo "$result $x"'
[[ $rc != 0 && $err == *SB5020* && $err != *SB5067* ]]; chk gflow.8

# B18: a background child cannot clean its parent's global.
run_sash 'str x=clean
fn clean { x=safe }
x=$(/bin/echo dirty)
clean &
- wait
echo "$x"'
[[ $rc != 0 && $err == *SB5020* ]]; chk gflow.9

# The same boundary holds for non-final and final function stages.
run_sash 'str x=clean
fn clean { x=safe }
x=$(/bin/echo dirty)
clean | /bin/cat
echo "$x"'
[[ $rc != 0 && $err == *SB5020* ]]; chk gflow.10
run_sash 'str x=clean
fn clean { x=safe }
x=$(/bin/echo dirty)
echo | clean
echo "$x"'
[[ $rc != 0 && $err == *SB5020* ]]; chk gflow.11

# Discarding child effects must not discard the child's read obligations.
run_sash 'str x=clean
fn show { echo "$x" }
x=$(/bin/echo dirty)
show &
- wait
show | /bin/cat
x=safe'
[[ $rc != 0 && $(grep -c 'error \[SB5020\]' <<<"$err") = 2
   && $err == *":4:1: error [SB5020]"*
   && $err == *":6:1: error [SB5020]"* ]]; chk gflow.12

# An early return contributes its state even when the fallthrough cleans it.
run_sash 'str x=clean
fn fill {
    x=$(/bin/echo dirty)
    if [[ $1 == yes ]] { return 1 }
    x=safe
    return 0
}
- fill no
echo "$x"'
[[ $rc != 0 && $err == *SB5020* && $err != *SB5067* ]]; chk gflow.13

# A self-tail back edge can carry taint to an earlier read in the body.
run_sash 'str x=clean
fn repeat -> int {
    echo "$x"
    if (( $1 > 0 )) {
        x=$(/bin/echo dirty)
        return $(( (repeat 0) ))
    }
    x=safe
    return 0
}
int n=$(( (repeat 1) ))
echo "$n"'
[[ $rc != 0 && $err == *SB5020* && $err != *SB5067* ]]; chk gflow.14

# A bare int-function name in arithmetic is a call, including in a declaration.
run_sash 'str x=clean
fn fill -> int { x=$(/bin/echo dirty); return 7 }
int n=$(( fill ))
attest x [[ $x == dirty ]]
echo "$n $x"'
[[ $rc = 0 && $out = "7 dirty" && -z $err && -z $rerr ]]; chk gflow.15

# Two arithmetic sites make the function shared even within one expression.
run_sash 'str x=clean
fn fill -> int { x=$(/bin/echo dirty); return 7 }
int n=$(( (fill) + (fill) ))
echo "$n ${x:!}"'
[[ $rc != 0 && $err == *SB5067*'shared (it is called at'* ]]; chk gflow.16

# The self-tail site is a back edge, not a second external call site.
run_sash 'str x=clean
fn fill -> int {
    if (( $1 > 0 )) {
        x=$(/bin/echo dirty)
        return $(( (fill 0) ))
    }
    return 7
}
int n=$(( (fill 1) ))
attest x [[ $x == dirty ]]
echo "$n $x"'
[[ $rc = 0 && $out = "7 dirty" && -z $err && -z $rerr ]]; chk gflow.17

# A real parent-process call changes the value, not its unresolved taint.
run_sash 'str x=clean
fn clean { x=safe }
x=$(/bin/echo dirty)
clean
echo "$x"'
[[ $rc != 0 && $err == *':5:6: error [SB5020]'* ]]; chk adjpolicy.2

# A conditional overwrite is not a must-kill.
run_sash 'str x=clean
fn clean { if (( $1 > 0 )) { x=safe } }
x=$(/bin/echo dirty)
clean 0
echo "$x"'
[[ $rc != 0 && $err == *SB5020* ]]; chk gflow.19

# A skipped lazy capture cannot clean the enclosing state.
run_sash 'str x=clean
x=$(/bin/echo dirty)
str selector=yes
if [[ $selector == yes || "$(x=safe; echo no)" == yes ]] { echo "$x" }'
[[ $rc != 0 && $err == *SB5020* ]]; chk gflow.20

# A later operand's value call cannot erase an earlier argument read.
run_sash 'str x=clean
fn clean -> int { x=safe; return 7 }
x=$(/bin/echo dirty)
echo "$x" $(( clean ))'
[[ $rc != 0 && $err == *SB5020* ]]; chk gflow.21

# Nor can a skipped arithmetic call clean the next statement's input.
run_sash 'str x=clean
fn clean -> int { x=safe; return 7 }
x=$(/bin/echo dirty)
int n=$(( 1 || clean ))
echo "$x $n"'
[[ $rc != 0 && $err == *SB5020* ]]; chk gflow.22

run_sash 'fn f {
argv 1 A
if attest v in A [[ $v != z ]] { echo "ok ${A[0]}" } else { echo bad; exit 1 }
}
array a=()
f =a
echo "${a[0]}"' a z
[[ $rc = 1 && $out = bad && -z $err && -z $rerr ]]; chk atslot.9

# DD#127: taint follows the read boundary, not the apparent stdout producer.
run_sash 'fn f { /bin/echo external }
str x=$(f)
echo "$x"'
[[ $rc != 0 && $err == *SB5020* && $err != *backstop* ]]; chk outflow.1

run_sash 'fn inner { /bin/echo external }
fn outer { inner }
str x=$(outer)
echo "$x"'
[[ $rc != 0 && $err == *SB5020* && $err != *backstop* ]]; chk outflow.2

# /dev/null does not protect a saved capture fd from being reopened.
run_sash 'fn f { /bin/echo discarded >/dev/null; echo internal }
str x=$(f)
echo "$x"'
[[ $rc != 0 && $err == *SB5020* && $err != *backstop* ]]; chk outflow.3

# Even builtin-only output is tainted when its capture drains a memfd.
run_sash 'str x=$(echo discarded >/dev/null; echo internal)
echo "$x"'
[[ $rc != 0 && $err == *SB5020* && $err != *backstop* ]]; chk outflow.4

run_sash 'echo hello | readline x
echo "$x"'
[[ $rc != 0 && $err == *SB5020* && $err != *backstop* ]]; chk outflow.5

run_sash 'write hello | read a
echo "${a[0]}"'
[[ $rc != 0 && $err == *SB5020* && $err != *backstop* ]]; chk outflow.6

run_sash 'fn f { echo hello }
f | readline x
echo "$x"'
[[ $rc != 0 && $err == *SB5020* && $err != *backstop* ]]; chk outflow.7

run_sash 'echo hello | while readline x { echo "$x" }'
[[ $rc != 0 && $err == *SB5020* && $err != *backstop* ]]; chk outflow.8

run_sash 'str x=$(: | echo hello)
echo "$x"'
[[ $rc != 0 && $err == *SB5020* && $err != *backstop* ]]; chk outflow.9

run_sash 'fn f { echo hello }
str x=$(f)
str y=$(f)
echo "$x $y"'
[[ $rc = 0 && $out = "hello hello" && -z $err && -z $rerr ]]; chk outflow.10

# An inner kernel read does not taint the outer, compiler-owned echo buffer.
run_sash 'fn f {
    str t=$(/bin/echo external)
    attest t [[ $t == external ]]
    echo "$t"
}
str x=$(f)
str y=$(f)
echo "$x $y"'
[[ $rc = 0 && $out = "external external" && -z $err && -z $rerr ]]; chk outflow.11

run_sash 'fn count -> int { /bin/echo side; return 7 }
int n=$(count)
echo "$n"'
[[ $rc = 0 && $out = $'side\n7' && -z $err && -z $rerr ]]; chk outflow.12

run_sash 'fn count -> int { /bin/echo side; return 7 }
str x=$(toupper "$(count)")
echo "$x"'
[[ $rc = 0 && $out = $'side\n7' && -z $err && -z $rerr ]]; chk outflow.13

# The same value call's stdout does disqualify an enclosing body capture.
run_sash 'fn count -> int { /bin/echo side; return 7 }
fn f { int n=$(count); echo "$n" }
str x=$(f)
echo "$x"'
[[ $rc != 0 && $err == *SB5020* && $err != *backstop* ]]; chk outflow.14

# DD#127: shared output is attested inside its producer, not at each caller.
run_sash 'fn raw { /bin/echo hello }
str x=$(raw)
attest x [[ $x == hello ]]
str y=$(raw)
attest y [[ $y == hello ]]
echo "$x $y"'
[[ $rc != 0 && $err == *:3:1:*SB5100* && $err == *'`raw`'* &&
   $err == *'called at 2:9, 4:9'* && $err != *backstop* ]]; chk outown.1

run_sash 'fn raw { /bin/echo hello }
fn relay { raw }
str x=$(relay)
attest x [[ $x == hello ]]
echo "$x"
raw'
[[ $rc != 0 && $err == *SB5100* && $err == *'`raw`'* ]]; chk outown.2

# The innermost shared writer, not its shared wrapper.
run_sash 'fn raw { /bin/echo hello }
fn relay { raw }
str x=$(relay)
attest x [[ $x == hello ]]
echo "$x"
relay
raw'
[[ $rc != 0 && $err == *SB5100* && $err == *'`raw` is shared'* &&
   $err != *'`relay` is shared'* ]]; chk outown.3

run_sash 'fn left { /bin/echo left }
fn right { /bin/echo right }
fn relay { if [[ $1 == left ]] { left } else { right } }
str x=$(relay left)
attest x [[ -n $x ]]
echo "$x"
left
right'
[[ $rc != 0 && $err == *SB5100* && $err == *'`left` is shared'* &&
   $err == *'`right` is shared'* ]]; chk outown.4

run_sash 'fn raw { /bin/echo hello }
raw | readline x
attest x [[ $x == hello ]]
echo "$x"
raw'
[[ $rc != 0 && $err == *SB5100* && $err == *'`raw` is shared'* ]]; chk outown.5

# This still uses a real pipe; buffer optimization is a separate change.
run_sash 'fn pure { echo hello }
pure | readline x
attest x [[ $x == hello ]]
echo "$x"
pure'
[[ $rc != 0 && $err == *SB5100* && $err == *'`pure` is shared'* ]]; chk outown.6

run_sash 'fn raw { /bin/echo hello }
raw | read a
attest v in a [[ -n $v ]]
echo "${a[0]}"
raw'
[[ $rc != 0 && $err == *SB5100* && $err == *'`raw` is shared'* ]]; chk outown.7

run_sash 'fn raw { /bin/echo hello }
raw | while readline x {
    attest x [[ $x == hello ]]
    echo "$x"
}
raw'
[[ $rc != 0 && $err == *SB5100* && $err == *'`raw` is shared'* ]]; chk outown.8

run_sash 'fn raw { /bin/echo 7 }
str x=$(raw)
attest x (( n = x ))
echo "$n"
raw'
[[ $rc != 0 && $err == *SB5100* && $err == *'`raw` is shared'* ]]; chk outown.9

run_sash 'fn raw { /bin/echo hello }
str x=$(raw)
attest x [[ $x == hello ]] || x=fallback
echo "$x"
raw'
[[ $rc != 0 && $err == *SB5100* && $err == *'`raw` is shared'* ]]; chk outown.10

run_sash 'fn raw { /bin/echo hello }
str x=$(raw)
if attest x [[ $x == hello ]] { echo "$x" } else { x=fallback }
raw'
[[ $rc != 0 && $err == *SB5100* && $err == *'`raw` is shared'* ]]; chk outown.11

# Ownership must survive the existing global and slot summaries.
run_sash 'fn raw { /bin/echo hello }
fn fill { raw | read A }
array a=()
fill =a
attest v in a [[ -n $v ]]
echo "${a[0]}"
raw'
[[ $rc != 0 && $err == *SB5100* && $err == *'`raw` is shared'* ]]; chk outown.12

run_sash 'str x=""
fn raw { /bin/echo hello }
fn fill { x=$(raw) }
fill
attest x [[ $x == hello ]]
echo "$x"
raw'
[[ $rc != 0 && $err == *SB5100* && $err == *'`raw` is shared'* ]]; chk outown.13

run_sash 'fn raw { /bin/echo hello }
fn fill { raw | read A }
fn relay { fill =B }
array a=()
array b=()
echo "$(len a)"
relay =a =b
attest v in b [[ -n $v ]]
echo "${b[0]}"
raw'
[[ $rc != 0 && $err == *SB5100* && $err == *'`raw` is shared'* ]]; chk outown.14

# A new source on one branch cannot erase the other branch's ownership.
run_sash 'fn raw { /bin/echo hello }
str x=$(raw)
str choose=yes
if [[ $choose == no ]] { x=$(/bin/echo other) }
attest x [[ -n $x ]]
echo "$x"
raw'
[[ $rc != 0 && $err == *SB5100* && $err == *'`raw` is shared'* ]]; chk outown.15

# An unconditional new source does replace the old provenance, even at a call.
run_sash 'str x=""
fn raw { /bin/echo hello }
fn replace { x=$(/bin/echo replacement) }
x=$(raw)
replace
attest x [[ $x == replacement ]]
echo "$x"
raw'
[[ $rc = 0 && $out = $'replacement\nhello' && -z $err && -z $rerr ]]; chk outown.16

run_sash 'fn raw { /bin/echo hello }
str x=$(raw)
x=clean
attest x [[ $x == clean ]]
str y=$(raw)
y=${y:!}
attest y [[ $y == hello ]]
echo "$x $y"'
[[ $rc != 0 && $err == *':4:1: error [SB5088]'* &&
   $err == *':4:1: error [SB5100]'* && $err == *':7:1: error [SB5088]'* &&
   $err == *':7:1: error [SB5100]'* ]]; chk adjpolicy.3

run_sash 'fn raw { /bin/echo hello }
str x=$(raw)
echo "${x:!}"
raw'
[[ $rc = 0 && $out = $'hello\nhello' && -z $err && -z $rerr ]]; chk outown.18

# The nested capture is not an output path of the outer capture.
run_sash 'fn raw { /bin/echo inner }
str x=$(str y=$(raw); echo "${y:!}"; /bin/echo outer)
attest x [[ -n $x ]]
echo "$x"
raw'
[[ $rc = 0 && $out = $'inner\nouter\ninner' && -z $err && -z $rerr ]]; chk outown.19

# In-place value calls, unlike nested captures, write to the enclosing capture.
run_sash 'fn count -> int { /bin/echo side; return 7 }
str x=$(echo "$(( (count) ))")
attest x [[ -n $x ]]
echo "$x"
: $(( (count) ))'
[[ $rc != 0 && $err == *SB5100* && $err == *'`count` is shared'* ]]; chk outown.20

run_sash 'str x=""
fn left { /bin/echo left }
fn right { /bin/echo right }
fn fill {
    if [[ $1 == left ]] { x=$(left); return 0 }
    x=$(right)
    return 0
}
fill left
attest x [[ -n $x ]]
echo "$x"
left
right'
[[ $rc != 0 && $err == *SB5100* && $err == *'`left` is shared'* &&
   $err == *'`right` is shared'* ]]; chk outown.21

run_sash 'str x=""
fn raw { /bin/echo hello }
fn fill -> int { x=$(raw); return 7 }
int n=$(( (fill) ))
attest x [[ -n $x ]]
echo "$x $n"
raw'
[[ $rc != 0 && $err == *SB5100* && $err == *'`raw` is shared'* ]]; chk outown.22

# A named reader is not a way to move the attest outside the producer.
run_sash 'fn raw { /bin/echo hello }
fn consume { readline x; attest x [[ $x == hello ]]; echo "$x" }
raw | consume
raw'
[[ $rc != 0 && $err == *SB5100* && $err == *'`raw` is shared'* &&
   $err == *consume* ]]; chk outown.23

run_sash 'fn raw { /bin/echo hello }
fn consume { readline x; attest x [[ $x == hello ]]; echo "$x" }
fn relay { consume }
raw | relay
raw'
[[ $rc != 0 && $err == *SB5100* && $err == *'`raw` is shared'* &&
   $err == *consume* ]]; chk outown.24

printf 'file\n' > input-own
run_sash 'fn raw { /bin/echo hello }
fn consume {
    readline ignored
    : "${ignored:!}"
    readline x <input-own
    attest x [[ $x == file ]]
    echo "$x"
}
raw | consume
raw'
[[ $rc = 0 && $out = $'file\nhello' && -z $err && -z $rerr ]]; chk outown.25

# Calls and captures in a parent reader's body use stdin, not its private pipe.
run_sash 'fn raw { /bin/echo hello }
fn stdin_only { - readline z; attest z [[ $z == "" ]]; echo empty }
raw | while readline x {
    echo "${x:!}"
    str y=$(- readline z; attest z [[ $z == "" ]]; echo empty)
    echo "$y"
    stdin_only
}
raw'
[[ $rc = 0 && $out = $'hello\nempty\nempty\nhello' && -z $err && -z $rerr ]]; chk outown.26

run_sash 'fn raw { /bin/echo hello }
str x=$(raw)
x+=$(/bin/echo suffix)
attest x [[ -n $x ]]
echo "$x"
raw'
[[ $rc != 0 && $err == *SB5100* && $err == *'`raw` is shared'* ]]; chk outown.27

run_sash 'fn raw { /bin/echo hello }
str x=$(raw)
echo fresh | readline x
attest x [[ $x == fresh ]]
echo "$x"
raw'
[[ $rc = 0 && $out = $'fresh\nhello' && -z $err && -z $rerr ]]; chk outown.28

run_sash 'fn raw { /bin/echo hello }
fn consume { str x=$(/bin/cat); attest x [[ $x == hello ]]; echo "$x" }
raw | consume
raw'
[[ $rc != 0 && $err == *SB5100* && $err == *'`raw` is shared'* &&
   $err == *consume* ]]; chk outown.29

run_sash 'fn raw { /bin/echo hello }
fn fill { read A }
fn consume {
    array a=()
    fill =a
    attest v in a [[ -n $v ]]
    echo "${a[0]}"
}
raw | consume
raw'
[[ $rc != 0 && $err == *SB5100* && $err == *'`raw` is shared'* &&
   $err == *consume* ]]; chk outown.30

run_sash 'str x=""
str choose=no
fn raw { /bin/echo hello }
fn maybe_replace {
    if [[ $choose == yes ]] { x=$(/bin/echo other) }
    attest x [[ -n $x ]]
}
x=$(raw)
maybe_replace
echo "$x"
raw'
[[ $rc != 0 && $err == *SB5100* && $err == *'`raw` is shared'* &&
   $err == *maybe_replace* ]]; chk outown.31

run_sash 'fn raw { /bin/echo hello }
fn consume { readline x 0<&0; attest x [[ $x == hello ]]; echo "$x" }
raw | consume
raw'
[[ $rc != 0 && $err == *SB5100* && $err == *'`raw` is shared'* ]]; chk outown.32

# Neither whole-value splitting nor unset discards unresolved ownership.
run_sash 'fn raw { /bin/echo hello }
str choose=no
fn replace {
    read "clean" A
    if [[ $choose == yes ]] { echo other | read A }
}
raw | read a
replace =a
attest v in a [[ $v == clean ]]
echo "${a[0]}"
raw'
[[ $rc != 0 && $err == *':9:1: error [SB5100]'* &&
   $err == *'`raw` is shared'* ]]; chk adjpolicy.4

run_sash 'fn raw { /bin/echo hello }
str choose=no
fn erase {
    unset A
    if [[ $choose == yes ]] { echo other | read A }
}
raw | read a
erase =a
attest v in a [[ -n $v ]]
echo "$(len a)"
raw'
[[ $rc != 0 && $err == *':9:1: error [SB5100]'* &&
   $err == *'`raw` is shared'* ]]; chk adjpolicy.5

# DD#121: diverting loop paths retain their state at the actual target.
run_sash 'str x=clean
for k in a b {
    if [[ $k == a ]] { x=$(/bin/echo dirty); break }
    x=clean
}
echo "$x"'
[[ $rc != 0 && $err == *':6:6: error [SB5020]'* ]]; chk loopflow.1

run_sash 'str x=clean
for k in b a {
    if [[ $k == a ]] { x=$(/bin/echo dirty); continue }
    x=clean
}
echo "$x"'
[[ $rc != 0 && $err == *':6:6: error [SB5020]'* ]]; chk loopflow.2

run_sash 'str x=clean
int n=0
while (( n < 2 )) {
    echo "$x"
    (( n += 1 ))
    if (( n == 1 )) { x=$(/bin/echo dirty); continue }
    x=clean
}'
[[ $rc != 0 && $err == *':4:10: error [SB5020]'* ]]; chk loopflow.3

run_sash 'str x=clean
int n=0
while (( n < 2 )) {
    (( n += 1 ))
    if (( n == 1 )) { x=$(/bin/echo dirty); break }
    x=clean
}
echo "$x"'
[[ $rc != 0 && $err == *':8:6: error [SB5020]'* ]]; chk loopflow.4

run_sash 'str x=$(/bin/echo dirty)
array a=()
for x in @a { : }
echo "$x"'
[[ $rc != 0 && $err == *':4:6: error [SB5020]'* ]]; chk loopflow.5

run_sash 'str x=clean
while x=$(/bin/sh -c "echo dirty; exit 1") { x=clean }
echo "$x"'
[[ $rc != 0 && $err == *':3:6: error [SB5020]'* ]]; chk loopflow.6

run_sash 'str x=clean
while readline x { x=clean }
echo "[$x]"'
[[ $rc != 0 && $err == *':3:6: error [SB5020]'* ]]; chk loopflow.7

# The live collection is read again at subsequent loop heads.
run_sash 'array a=(p)
for x in @a { argv a; echo "$x" }' p q
[[ $rc != 0 && $err == *':2:10: error [SB5020]'* &&
   $err == *"iterating tainted array 'a'"* ]]; chk loopflow.8

# Binding and its adjacency apply only to an entered iteration, every time.
run_sash 'for x in $(argv 1) { attest x [[ $x == ok ]] }
echo "[$x]"' ok
[[ $rc = 0 && $out = '[ok]' && -z $err && -z $rerr ]]; chk loopflow.9

run_sash 'for x in a b {
    echo "$x"
    x=$(/bin/echo dirty)
}
echo "${x:!}"'
[[ $rc != 0 && $err == *':2:10: error [SB5020]'* ]]; chk adjpolicy.6

run_sash 'str x=clean
for outer in a b { : "$outer"
    for inner in c d {
        if [[ $inner == c ]] { x=$(/bin/echo dirty); break 2 }
        x=clean
    }
    x=clean
}
echo "$x"'
[[ $rc != 0 && $err == *':9:6: error [SB5020]'* ]]; chk loopflow.11

run_sash 'str x=clean
for outer in a b { : "$outer"
    for inner in c d {
        if [[ $inner == c ]] { x=$(/bin/echo dirty); continue 2 }
        x=clean
    }
    x=clean
}
echo "$x"'
[[ $rc != 0 && $err == *':9:6: error [SB5020]'* ]]; chk loopflow.12

# SB5097 permits oversized positive literals; they still clamp outward.
run_sash 'str x=clean
for outer in a b { : "$outer"
    for inner in c d {
        if [[ $inner == c ]] { x=$(/bin/echo dirty); break 999999999999999999999999999 }
        x=clean
    }
    x=clean
}
echo "$x"'
[[ $rc != 0 && $err == *':9:6: error [SB5020]'* ]]; chk loopflow.13

# The while condition belongs to its own loop, not the enclosing for.
run_sash 'str x=clean
for k in a b { : "$k"
    while x=$(/bin/echo dirty); break { x=clean }
    attest x [[ $x == dirty ]]
    x=clean
}
echo "$x"'
[[ $rc = 0 && $out = clean && -z $err && -z $rerr ]]; chk adjpolicy.7

# Nested capture loops do not hide the enclosing body's later cleanup.
run_sash 'str x=clean
for k in a b { : "$k"
    y=$(
        for j in c d {
            if [[ $j == c ]] { x=$(/bin/echo dirty); break }
        }
        echo hi
    )
    attest x [[ $x == dirty ]]
    x=clean
}
echo "$x $y"'
[[ $rc = 0 && $out = 'clean hi' && -z $err && -z $rerr ]]; chk adjpolicy.8

# DD#127 ownership travels with the taint, including diverted paths.
run_sash 'fn raw { /bin/echo dirty }
str x=clean
for k in a b {
    if [[ $k == a ]] { x=$(raw); break }
    x=clean
}
attest x [[ $x == dirty ]]
raw'
[[ $rc != 0 && $err == *SB5100* && $err == *'`raw` is shared'* ]]; chk loopflow.16

run_sash 'fn raw { /bin/echo dirty }
str x=clean
for k in b a {
    if [[ $k == a ]] { x=$(raw); continue }
    x=clean
}
attest x [[ $x == dirty ]]
raw'
[[ $rc != 0 && $err == *SB5100* && $err == *'`raw` is shared'* ]]; chk loopflow.17

# The same exits feed single-use summaries and shared-function source sites.
run_sash 'str x=clean
fn fill {
    for k in a b {
        if [[ $k == a ]] { x=$(/bin/echo dirty); break }
        x=clean
    }
}
fill
echo "$x"'
[[ $rc != 0 && $err == *':9:6: error [SB5020]'* ]]; chk loopflow.18

run_sash 'str x=clean
fn fill {
    for k in a b {
        if [[ $k == a ]] {
            x=$(/bin/echo dirty)
            break
        }
        x=clean
    }
}
fill
fill
echo "${x:!}"'
[[ $rc != 0 && $err == *':5:13: error [SB5067]'* &&
   $err == *'`fill`'* ]]; chk loopflow.19

# Quiet inner-loop probes keep the enclosing depth without exporting exits.
run_sash 'str x=clean
for outer in a b { : "$outer"
    x=$(/bin/echo clean)
    attest x [[ $x == clean ]]
    for inner in c d {
        echo "$x"
        if [[ $inner == c ]] { x=$(/bin/echo dirty); continue 2 }
        x=clean
    }
}'
[[ $rc = 0 && $out = $'clean\nclean' && -z $err && -z $rerr ]]; chk adjpolicy.9

# The final head must retain a binder value tainted by the preceding body.
run_sash 'for x in a b { x=$(/bin/echo dirty) }
echo "$x"'
[[ $rc != 0 && $err == *':2:6: error [SB5020]'* ]]; chk loopflow.21

# DD#128: overwriting bytes does not discharge a live binding's taint.
run_sash 'x=$(arg 1)
x=safe
echo "$x"' dirty
[[ $rc != 0 && $err == *':3:6: error [SB5020]'* ]]; chk adjresolve.1

run_sash 'x=$(arg 1)
x=safe
attest x [[ $x == safe ]]' dirty
[[ $rc != 0 && $err == *':3:1: error [SB5088]'* ]]; chk adjresolve.2

run_sash 'x=$(arg 1)
x=${x:!}
echo "$x"' dirty
[[ $rc != 0 && $err == *':3:6: error [SB5020]'* ]]; chk adjresolve.3

run_sash 'argv 1 a
a=(safe)
echo "${a[0]}"' dirty
[[ $rc != 0 && $err == *':3:6: error [SB5020]'* ]]; chk adjresolve.4

run_sash 'argv 1 a
unset a
echo "${a[0]:-missing}"' dirty
[[ $rc != 0 && $err == *':3:6: error [SB5020]'* ]]; chk adjresolve.5

run_sash 'x=$(arg 1)
for x in safe { echo "$x" }' dirty
[[ $rc != 0 && $err == *SB5020* && $err == *'$x'* ]]; chk adjresolve.6

run_sash 'fn buffered { echo safe }
x=$(arg 1)
x=$(buffered)
echo "$x"' dirty
[[ $rc != 0 && $err == *':4:6: error [SB5020]'* ]]; chk adjresolve.7

# A helper's literal write neither satisfies its own read nor clears a slot.
run_sash 'str x
fn reset { x=safe; echo "$x" }
x=$(arg 1)
reset' dirty
[[ $rc != 0 && $err == *':4:1: error [SB5020]'* && $err == *'reset'* ]]; chk adjresolve.8

run_sash 'fn reset { unset A }
argv 1 a
reset =a
echo "${a[0]:-missing}"' dirty
[[ $rc != 0 && $err == *':4:6: error [SB5020]'* ]]; chk adjresolve.9

# R2 is mandatory even without a later read. Permission does not enter helpers.
run_sash 'str x
fn reset { x=safe }
x=$(arg 1)
attest x [[ $x == allowed ]] || reset' dirty
[[ $rc != 0 && $err == *':4:1: error [SB5089]'* ]]; chk adjresolve.10

# Only the failing path continues, so a mistaken permission for y cannot hide
# behind the success path's still-tainted y at the merge.
run_sash 'y=$(arg 2)
x=$(arg 1)
if attest x [[ $x == allowed ]] { exit 0 } else { x=safe; y=safe }
echo "$y"' dirty other
[[ $rc != 0 && $err == *':4:6: error [SB5020]'* ]]; chk adjresolve.11

run_sash 'str mode=yes
x=$(arg 1)
if attest x [[ $x == allowed ]] {
    :
} else {
    if [[ $mode == yes ]] { x=safe } else { x=fallback }
}
echo "$x"' dirty
[[ $rc = 0 && $out = safe && -z $err && -z $rerr ]]; chk adjresolve.12

run_sash 'argv 1 a
attest v in a [[ $v == allowed ]] || unset a
echo "[$(join a ,)]"' dirty
[[ $rc = 0 && $out = '[]' && -z $err && -z $rerr ]]; chk adjresolve.13

run_sash 'argv 1 a
attest v in a [[ $v == allowed ]] || read "fallback" a
echo "${a[0]}"' dirty
[[ $rc = 0 && $out = fallback && -z $err && -z $rerr ]]; chk adjresolve.14

# A genuine acquisition in a helper has its own adjacent resolution. Unlike
# a reset helper, this can resolve taint that the slot carried on entry.
run_sash 'fn recover {
    argv 1 A
    attest v in A [[ $v == allowed ]] || unset A
}
argv 1 a
recover =a
echo "[$(join a ,)]"' dirty
[[ $rc = 0 && $out = '[]' && -z $err && -z $rerr ]]; chk adjresolve.15

run_sash 'fn reset { unset A }
argv 1 a
attest v in a [[ $v == allowed ]] || reset =a' dirty
[[ $rc != 0 && $err == *':3:1: error [SB5089]'* ]]; chk adjresolve.16

run_sash 'x=$(arg 1)
attest x [[ $x == allowed ]] || x=$(arg 2)' dirty other
[[ $rc != 0 && $err == *':2:1: error [SB5089]'* ]]; chk adjresolve.17

run_sash 'x=$(arg 1)
if attest x [[ $x == allowed ]] { : } else {
  : $(x=safe; echo "$x")
  x=safe
}' dirty
[[ $rc != 0 && $err == *':3:20: error [SB5020]'* ]]; chk adjresolve.18

set -e
tally
