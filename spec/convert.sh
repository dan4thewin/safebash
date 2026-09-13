#!/bin/bash
# spec/convert.sh — acceptance tests for sash-convert (DD#57)
set -eu
. common.sh
# sash-convert is a separate tool, independent of sash backend.
# Run only in native mode to avoid 3× redundancy.
[[ ${SASH_MODE:-native} != native ]] && exit 0
setup convert
plan=145
echo "1..$plan" >&3

set +e

# --- DD#57: Mechanical conversions ---

# 57.1 declare -i → int
run_convert '#!/bin/bash
declare -i x=5
echo $x'
[[ $rc = 0 && $out == *'int x=5'* && $out != *'declare'* && -z $err ]]; chk 57.1

# 57.2 declare -a → array
run_convert '#!/bin/bash
declare -a arr=(1 2 3)'
[[ $rc = 0 && $out == *'array arr=(1 2 3)'* && $out != *'declare'* && -z $err ]]; chk 57.2

# 57.3 declare -A → map
run_convert '#!/bin/bash
declare -A m=([a]=1)'
[[ $rc = 0 && $out == *'map m=([a]=1)'* && $out != *'declare'* && -z $err ]]; chk 57.3

# 57.4a declare -r → const
run_convert '#!/bin/bash
declare -r x=5'
[[ $rc = 0 && $out == *'const x=5'* && $out != *'declare'* && -z $err ]]; chk 57.4a

# 57.4b readonly → const
run_convert '#!/bin/bash
readonly msg=hello'
[[ $rc = 0 && $out == *'const msg=hello'* && $out != *'readonly'* && -z $err ]]; chk 57.4b

# 57.4c declare -ri → const int
run_convert '#!/bin/bash
declare -ri MAX=100'
[[ $rc = 0 && $out == *'const int MAX=100'* && $out != *'declare'* && -z $err ]]; chk 57.4c

# 57.5 backtick → $()
run_convert '#!/bin/bash
x=`date`'
[[ $rc = 0 && $out == *'$(date)'* && $out != *'`'* && -z $err ]]; chk 57.5

# 57.6a [ ] → [[ ]]
run_convert '#!/bin/bash
if [ "$x" -gt 0 ]; then echo yes; fi'
[[ $rc = 0 && $out == *'[['* && $out == *']]'* && $out != *'[ "'* && -z $err ]]; chk 57.6a

# 57.6b complex numeric comparison gets MANUAL annotation
run_convert '#!/bin/bash
if [[ "$x" -gt 0 ]]; then echo yes; fi'
[[ $rc = 0 && $out == *'MANUAL'* && $out == *'-gt'* && -z $err ]]; chk 57.6b

# 57.7a ${var-x} → ${var:-x}
run_convert '#!/bin/bash
echo ${HOME-default}'
[[ $rc = 0 && $out == *'${HOME:-default}'* && -z $err ]]; chk 57.7a

# 57.7b ${var:-x} unchanged
run_convert '#!/bin/bash
echo ${HOME:-default}'
[[ $rc = 0 && $out == *'${HOME:-default}'* && -z $err ]]; chk 57.7b

# 57.8 let → (( ))
run_convert '#!/bin/bash
let "x=3+4"'
[[ $rc = 0 && $out == *'(( x=3+4 ))'* && $out != *'let'* && -z $err ]]; chk 57.8

# 57.9 typeset → bare assignment
run_convert '#!/bin/bash
typeset y=10'
[[ $rc = 0 && $out == *'y=10'* && $out != *'typeset'* && -z $err ]]; chk 57.9

# 57.15 output is valid sash-native (convert then compile)
cat > .input.sh << 'SH'
#!/bin/bash
declare -i x=5
echo $x
SH
sash-convert .input.sh -o .converted.sash 2>err
rc=$?; err=$(<err)
[[ $rc = 0 && -z $err ]]; chk 57.15a
sash .converted.sash -o .binary 2>err
rc=$?; err=$(<err)
[[ $rc = 0 && -z $err ]]; chk 57.15b

# 57.16 --annotate adds "# was: ..." comments
run_convert_annotate '#!/bin/bash
declare -i x=5'
[[ $rc = 0 && $out == *'# was: declare -i x=5'* && -z $err ]]; chk 57.16a

run_convert_annotate '#!/bin/bash
echo hello'
[[ $rc = 0 && $out != *'# was:'* && -z $err ]]; chk 57.16b

# --- DD#57: Arithmetic side-effect extraction ---

# 57.10 $((x=10)) → ((x=10))
run_convert '#!/bin/bash
y=$((x=10))'
[[ $rc = 0 && $out == *'(( x=10 ))'* && -z $err ]]; chk 57.10

# --- DD#57: Subshell conversion ---

# 57.11 subshell with assignments → synthetic function or MANUAL
run_convert '#!/bin/bash
(x=5; echo $x)'
[[ $rc = 0 && ( $out != *'(x=5'* || $out == *'MANUAL'* ) && -z $err ]]; chk 57.11

# 57.12 subshell with cd → save/restore or MANUAL
run_convert '#!/bin/bash
(cd /tmp; ls)'
[[ $rc = 0 && ( $out != *'(cd /tmp'* || $out == *'MANUAL'* ) && -z $err ]]; chk 57.12

# --- DD#57: Shellcheck integration ---

# 57.31 shellcheck consumed internally — CHECK hint appears for SC2086
run_convert '#!/bin/bash
x=hello
echo $x'
[[ $rc = 0 && $out == *'# CHECK:'*'SC2086'* && -z $err ]]; chk 57.31a

# 57.31b SC2248 also triggers CHECK hint (optional check, stricter quoting)
run_convert '#!/bin/bash
x=hello
cmd $x'
[[ $rc = 0 && $out == *'# CHECK:'* && -z $err ]]; chk 57.31b

# 57.32 shellcheck-unclean input still converts successfully
run_convert '#!/bin/bash
x=hello
echo $x'
[[ $rc = 0 && $out == *'echo $x'* && -z $err ]]; chk 57.32

# --- DD#57: Format consistency ---

# 57.38 convert output matches --extract from compiled binary
cat > .input.sh << 'SH'
#!/bin/bash
declare -i x=5
echo $x
SH
sash-convert .input.sh -o .converted.sash 2>err
sash .converted.sash -o .binary 2>>err
sash --extract .binary > .extracted.sash 2>>err
err=$(<err)
diff -q .converted.sash .extracted.sash > /dev/null 2>&1
[[ $? = 0 && -z $err ]]; chk 57.38

# --- DD#57 / DD#60: Positional conversion ---

# 57.13 top-level $1 → a hoisted `arg1=$(arg 1)` + placeholder attest.
# `$(arg 1)` is tainted (DD#60), so the inline substitution this spec used to
# assert could never compile — every use raised SB5020.
run_convert '#!/bin/bash
echo $1'
[[ $rc = 0 && $out == *'arg1=$(arg 1); attest arg1 [[ -n $arg1 ]]'* \
   && $out == *'MANUAL'* && $out == *'echo $arg1'* && -z $err ]]; chk 57.13a

# 57.13b ${1} → the same hoist, ${@} → $(argv 1), ${#} → $(( (argc) - 1 ))
run_convert '#!/bin/bash
echo ${1} ; for x in ${@}; do echo $x; done ; echo ${#}'
[[ $rc = 0 && $out == *'arg1=$(arg 1)'* && $out == *'$(argv 1)'* \
   && $out == *'$(( (argc) - 1 ))'* && -z $err ]]; chk 57.13b

# 57.13c ${#x} length sugar (DD#75 §4). This spec used to assert the bug: it
# required `${#arr}` AND `${#s}` to become `$(len …)`, which conflated the
# element count with a byte length and — after DD#75 made `len` collection-only
# — emitted a program that did not compile. The subscript says which question
# is being asked; see 57.74–57.78. `${#}` (arg count) is unaffected.
run_convert '#!/bin/bash
arr=(a b c); echo ${#arr}; s=hi; echo ${#s}; echo ${#arr[@]}'
[[ $rc = 0 && $out == *'$(strlen ${arr[0]})'* && $out == *'$(strlen $s)'* && $out == *'$(len arr)'* && $out != *'${#'* && -z $err ]]; chk 57.13c

# conv.f1 $@ inside a function is preserved (function-local, DD#46)
run_convert '#!/bin/bash
greet() { echo "$@"; }'
[[ $rc = 0 && $out == *'"$@"'* && $out != *'argv'* && -z $err ]]; chk conv.f1

# conv.f2 $1 inside a function is preserved
run_convert '#!/bin/bash
one() { echo "$1"; }'
[[ $rc = 0 && $out == *'"$1"'* && $out != *'arg 1'* && -z $err ]]; chk conv.f2

# conv.f3 $# inside a function is preserved
run_convert '#!/bin/bash
n() { echo "$#"; }'
[[ $rc = 0 && $out == *'"$#"'* && $out != *'argc'* && -z $err ]]; chk conv.f3

# conv.q1 top-level for-in over "$@" → bare $(argv 1) (quotes dropped, DD#22)
#
# `$(argv 1)`, not `$(argv)`: DD#60 says `$(argv)` includes argv[0], the
# program name, while bash's "$@" is argv[1:]. The old output iterated one
# extra time with the program's own path as the first value. The body gains a
# leading attest because the loop variable is tainted per-iteration.
run_convert '#!/bin/bash
for x in "$@"; do echo "$x"; done'
[[ $rc = 0 && $out == *'for x in $(argv 1)'* && $out != *'for x in $(argv)'* \
   && $out == *'do attest x [[ -n $x ]];'* && $out == *'MANUAL'* && -z $err ]]; chk conv.q1

# --- DD#57: Trap conversion ---

# 57.14 moved to spec/signals.sh (57.14a, 57.14b)

# --- DD#57: MANUAL annotations ---

# 57.20 eval
run_convert '#!/bin/bash
eval "echo hello"'
[[ $rc = 0 && $out == *'# MANUAL:'*'eval'* && $out == *'eval "echo hello"'* && -z $err ]]; chk 57.17

# 57.21a source
run_convert '#!/bin/bash
source helper.sh'
[[ $rc = 0 && $out == *'# MANUAL:'*'source'* && $out == *'source helper.sh'* && -z $err ]]; chk 57.18a

# 57.21b dot command
run_convert '#!/bin/bash
. helper.sh'
[[ $rc = 0 && $out == *'# MANUAL:'* && $out == *'. helper.sh'* && -z $err ]]; chk 57.18b

# 57.22 ${!var} indirect expansion
run_convert '#!/bin/bash
x=HOME
echo ${!x}'
[[ $rc = 0 && $out == *'# MANUAL:'*'indirect'* && $out == *'${!x}'* && -z $err ]]; chk 57.19

# 57.23 nameref
run_convert '#!/bin/bash
declare -n ref=x'
[[ $rc = 0 && $out == *'# MANUAL:'*'nameref'* && $out == *'declare -n ref=x'* && -z $err ]]; chk 57.20

# 57.24 process substitution
run_convert '#!/bin/bash
diff <(echo a) <(echo b)'
[[ $rc = 0 && $out == *'# MANUAL:'*'process substitution'* && $out == *'diff'* && -z $err ]]; chk 57.21

# 57.25 ${var@X} parameter transformation
run_convert '#!/bin/bash
x=hello
echo ${x@U}'
[[ $rc = 0 && $out == *'# MANUAL:'*'parameter transformation'* && $out == *'${x@U}'* && -z $err ]]; chk 57.22

# 57.26 heredoc command substitution
run_convert '#!/bin/bash
cat <<EOF
today is $(date)
EOF'
[[ $rc = 0 && $out == *'# MANUAL:'*'heredoc'* && -z $err ]]; chk 57.23

# 57.27 export
run_convert '#!/bin/bash
export PATH="/usr/bin"'
[[ $rc = 0 && $out == *'# MANUAL:'*'export'* && $out == *'export PATH='* && -z $err ]]; chk 57.24

# 57.28 select
run_convert '#!/bin/bash
select opt in a b c; do echo $opt; break; done'
[[ $rc = 0 && $out == *'# MANUAL:'*'select'* && $out == *'select opt'* && -z $err ]]; chk 57.25

# 57.29 coproc
run_convert '#!/bin/bash
coproc cat'
[[ $rc = 0 && $out == *'# MANUAL:'*'coproc'* && $out == *'coproc cat'* && -z $err ]]; chk 57.26

# 57.30a set -e
run_convert '#!/bin/bash
set -e'
[[ $rc = 0 && $out == *'# MANUAL:'*'set'* && $out == *'set -e'* && -z $err ]]; chk 57.27a

# 57.30b set -u
run_convert '#!/bin/bash
set -u'
[[ $rc = 0 && $out == *'# MANUAL:'*'set'* && $out == *'set -u'* && -z $err ]]; chk 57.27b

# 57.30c shopt
run_convert '#!/bin/bash
shopt -s nullglob'
[[ $rc = 0 && $out == *'# MANUAL:'*'shopt'* && $out == *'shopt -s nullglob'* && -z $err ]]; chk 57.27c

# 57.31 alias
run_convert '#!/bin/bash
alias ll="ls -la"'
[[ $rc = 0 && $out == *'# MANUAL:'*'alias'* && $out == *'alias ll='* && -z $err ]]; chk 57.28

# 57.32 unconvertible subshell
run_convert '#!/bin/bash
(echo hello; echo world)'
[[ $rc = 0 && $out == *'# MANUAL:'*'subshell'* && $out == *'echo hello'* && -z $err ]]; chk 57.29

# 57.33 unconvertible lines pass through unchanged
run_convert '#!/bin/bash
eval "echo hello"'
[[ $rc = 0 && $out == *'eval "echo hello"'* && $out == *'# MANUAL:'* && -z $err ]]; chk 57.30

# --- DD#57: CHECK annotations ---

# 57.40 SC2086 → CHECK hint
run_convert '#!/bin/bash
x=hello
echo $x'
[[ $rc = 0 && $out == *'# CHECK:'*'SC2086'* && $out == *'echo $x'* && -z $err ]]; chk 57.33

# 57.42 unquoted vars not altered, only annotated
run_convert '#!/bin/bash
x=hello
echo $x'
[[ $rc = 0 && $out == *'echo $x'* && $out != *'echo "$x"'* && -z $err ]]; chk 57.34

# --- DD#57: ATTEST annotations ---

# 57.50a known external → attest line
run_convert '#!/bin/bash
today=$(date +%Y-%m-%d)'
[[ $rc = 0 && $out == *'attest today'* && $out == *'today=$(date +%Y-%m-%d)'* && -z $err ]]; chk 57.35a

# 57.50b known external → attest line
run_convert '#!/bin/bash
host=$(hostname)'
[[ $rc = 0 && $out == *'attest host'* && $out == *'host=$(hostname)'* && -z $err ]]; chk 57.35b

# 57.51 unknown external → ATTEST hint
run_convert '#!/bin/bash
result=$(some_unknown_command arg1)'
[[ $rc = 0 && $out == *'# ATTEST:'*'some_unknown_command'* && $out == *'result=$(some_unknown_command arg1)'* && -z $err ]]; chk 57.36

# 57.52 read -r VAR → readline VAR (DD#99)
run_convert '#!/bin/bash
read -r line'
[[ $rc = 0 && $out == *'readline line'* && $out != *'read -r line'* && -z $err ]]; chk 57.37

# --- DD#57: I/O ---

# 57.60a file argument
cat > .input.sh << 'SH'
#!/bin/bash
declare -i x=5
SH
rc=-1 out="" err=""
sash-convert .input.sh > out 2> err
rc=$?; out=$(<out); err=$(<err)
[[ $rc = 0 && $out == *'int x=5'* && -z $err ]]; chk 57.39a

# 57.60b stdin
rc=-1 out="" err=""
sash-convert < .input.sh > out 2> err
rc=$?; out=$(<out); err=$(<err)
[[ $rc = 0 && $out == *'int x=5'* && -z $err ]]; chk 57.39b

# 57.61 -o file
rc=-1 err=""
sash-convert .input.sh -o .output.sash 2> err
rc=$?; err=$(<err)
[[ $rc = 0 && $(<.output.sash) == *'int x=5'* && -z $err ]]; chk 57.40

# 57.65 convert | compile → working binary
cat > .input.sh << 'SH'
#!/bin/bash
declare -i x=5
echo $x
SH
rc=-1 out="" err=""
sash-convert .input.sh -o .converted.sash 2>err
sash .converted.sash -o .binary 2>>err
./.binary > out 2>>err
rc=$?; out=$(<out); err=$(<err)
[[ $rc = 0 && $out = "5" && -z $err ]]; chk 57.41


# --- DD#30 numeric comparison conversion ---

# 30.5 convert [[ $x -gt N ]] to (( x > N ))
run_convert '#!/bin/bash
if [[ $x -gt 5 ]]; then echo yes; fi'
[[ $rc = 0 && $out == *'(( x > 5 ))'* && -z $err ]]; chk 30.5a

run_convert '#!/bin/bash
if [[ $x -le 10 ]]; then echo yes; fi'
[[ $rc = 0 && $out == *'(( x <= 10 ))'* && -z $err ]]; chk 30.5b

run_convert '#!/bin/bash
if [[ $x -eq 0 ]]; then echo yes; fi'
[[ $rc = 0 && $out == *'(( x == 0 ))'* && -z $err ]]; chk 30.5c


# --- DD#30 unset scalar conversion ---

# 30.30 convert unset var (scalar) to var=""
run_convert '#!/bin/bash
x=hello
unset x'
[[ $rc = 0 && $out == *'x=""'* && -z $err ]]; chk 30.30


# --- DD#53 tilde expansion (~, ~/path) — converter side (compiler side: taint.sh 53.x) ---

# 57.47 bare ~ → hoisted $(getenv HOME) + attest, use $home
run_convert '#!/bin/bash
echo ~'
[[ $rc = 0 && $out == *'home=$(getenv HOME)'* && $out == *'attest home'* && $out == *'echo $home'* && -z $err ]]; chk 57.47

# 57.48 ~/path → $home/path
run_convert '#!/bin/bash
echo ~/foo'
[[ $rc = 0 && $out == *'echo $home/foo'* && $out == *'home=$(getenv HOME)'* && -z $err ]]; chk 57.48

# 57.49 tilde in assignment value → $home/p
run_convert '#!/bin/bash
x=~/p'
[[ $rc = 0 && $out == *'x=$home/p'* && $out == *'home=$(getenv HOME)'* && -z $err ]]; chk 57.49

# 57.50 ~/$x concatenation composes as $home/$x
run_convert '#!/bin/bash
echo ~/$x'
[[ $rc = 0 && $out == *'echo $home/$x'* && -z $err ]]; chk 57.50

# 57.51 quoted ~ is a literal — untouched, no getenv hoist
run_convert "#!/bin/bash
echo '~'"
[[ $rc = 0 && $out == *"echo '~'"* && $out != *'getenv HOME'* && -z $err ]]; chk 57.51

# 57.52 embedded tilde (a~b) is a literal — untouched
run_convert '#!/bin/bash
echo a~b'
[[ $rc = 0 && $out == *'echo a~b'* && $out != *'getenv HOME'* && -z $err ]]; chk 57.52

# 57.53 ~user not mechanically resolvable → MANUAL, untouched
run_convert '#!/bin/bash
echo ~root'
[[ $rc = 0 && $out == *'# MANUAL:'*'tilde'* && $out == *'echo ~root'* && -z $err ]]; chk 57.53

# 57.54 multi-tilde word (~/a:~/b) → MANUAL, no partial rewrite
run_convert '#!/bin/bash
PATH=~/bin:~/lib'
[[ $rc = 0 && $out == *'# MANUAL:'*'tilde'* && $out == *'PATH=~/bin:~/lib'* && $out != *'$home'* && -z $err ]]; chk 57.54

# 57.55 tilde in command position → MANUAL, untouched
run_convert '#!/bin/bash
~/bin/prog foo'
[[ $rc = 0 && $out == *'# MANUAL:'*'command position'* && $out == *'~/bin/prog foo'* && -z $err ]]; chk 57.55

# --- DD#99: the read family ---

# 57.56 plain `read VAR` (no -r) also becomes readline
run_convert '#!/bin/bash
read line'
[[ $rc = 0 && $out == *'readline line'* && -z $err ]]; chk 57.56

# 57.57 the trim difference gets a CHECK — readline keeps the line verbatim
run_convert '#!/bin/bash
read -r line'
[[ $rc = 0 && $out == *'# CHECK:'*'IFS whitespace'* && -z $err ]]; chk 57.57

# 57.58 `IFS= read -r` is the exact equivalent — no CHECK, and IFS= is dropped
run_convert '#!/bin/bash
IFS= read -r line'
[[ $rc = 0 && $out == *'readline line'* && $out != *'IFS='* && $out != *'# CHECK:'* && -z $err ]]; chk 57.58

# 57.59 `while IFS= read -r x` converts in place (do/done is accepted sugar)
run_convert '#!/bin/bash
while IFS= read -r x; do
  echo "$x"
done'
[[ $rc = 0 && $out == *'while readline x; do'* && -z $err ]]; chk 57.59

# 57.61 multi-target read has no equivalent → MANUAL, left alone
run_convert '#!/bin/bash
read a b c'
[[ $rc = 0 && $out == *'# MANUAL:'*'multi-target'* && $out == *'read a b c'* && -z $err ]]; chk 57.61

# 57.62 read -a splits on IFS, sash read splits on NUL → MANUAL, never a guess
run_convert '#!/bin/bash
read -a arr'
[[ $rc = 0 && $out == *'# MANUAL:'*'read -a'*'NUL'* && $out == *'read -a arr'* && -z $err ]]; chk 57.62

# 57.63 a combined flag carrying -a is caught too
run_convert '#!/bin/bash
read -ra arr'
[[ $rc = 0 && $out == *'# MANUAL:'*'read -a'* && -z $err ]]; chk 57.63

# 57.64 any other read flag → MANUAL naming it
run_convert '#!/bin/bash
read -p "name? " ans'
[[ $rc = 0 && $out == *'# MANUAL:'*'read -p'* && $out == *'read -p'* && -z $err ]]; chk 57.64

# 57.65 bare read fills $REPLY, which sash does not have → MANUAL
run_convert '#!/bin/bash
read'
[[ $rc = 0 && $out == *'# MANUAL:'*'REPLY'* && -z $err ]]; chk 57.65

# 57.66 mapfile -t A < f → tr | read, with the redirect moved onto tr
run_convert '#!/bin/bash
mapfile -t names < f'
[[ $rc = 0 && $out == *'/usr/bin/tr "\n" "\0" < f | read names'* && $out != *'mapfile'* && -z $err ]]; chk 57.66

# 57.67 readarray is the same builtin
run_convert '#!/bin/bash
readarray -t L'
[[ $rc = 0 && $out == *'/usr/bin/tr "\n" "\0" | read L'* && $out != *'readarray'* && -z $err ]]; chk 57.67

# 57.68 mapfile as a pipeline stage keeps the upstream producer
run_convert '#!/bin/bash
cat z | mapfile -t P'
[[ $rc = 0 && $out == *'cat z | /usr/bin/tr "\n" "\0" | read P'* && -z $err ]]; chk 57.68

# 57.69 without -t bash keeps the trailing newline — no sash equivalent
run_convert '#!/bin/bash
mapfile lines < f'
[[ $rc = 0 && $out == *'# MANUAL:'*'without -t'* && $out == *'mapfile lines < f'* && -z $err ]]; chk 57.69

# 57.70 the ATTEST hint names the collection form for a bulk target
run_convert '#!/bin/bash
mapfile -t names < f'
[[ $rc = 0 && $out == *'# ATTEST: names'*'attest x in names'* && -z $err ]]; chk 57.70

# 57.71 and the scalar form for readline
run_convert '#!/bin/bash
read -r line'
[[ $rc = 0 && $out == *'# ATTEST: line'*'attest line [['* && -z $err ]]; chk 57.71

# 57.72 an after-hint sorts before the next line's before-hint, so a MANUAL
# note never reads as belonging to the preceding command
run_convert '#!/bin/bash
read -r x
read a b c'
[[ $rc = 0 && $out == *'# ATTEST: x'*'# MANUAL:'*'multi-target'* && -z $err ]]; chk 57.72

# 57.73 converted read output compiles and runs
cat > .input.sh << 'SH'
#!/bin/bash
while IFS= read -r line; do
  echo "[$line]"
done
SH
rc=-1 out="" err=""
sash-convert .input.sh 2>err \
  | sed 's|^# ATTEST: line.*|attest line [[ $line =~ ^.*$ ]]|' > .converted.sash
sash .converted.sash -o .binary 2>>err
printf 'aa\nbb\n' | ./.binary > out 2>>err
rc=$?; out=$(<out); err=$(<err)
[[ $rc = 0 && $out == '[aa]
[bb]' && -z $err ]]; chk 57.73

# --- DD#75 §4 + DD#97: the four `${#…}` questions, and the keyword ---

# bash collapses four different questions into one spelling family, and the
# converter used to collapse them further by stripping the subscript: `${#a}`
# (bash: 1) and `${#a[@]}` (bash: 3) both became `$(len a)`. `${#s}` on a
# scalar became `$(len s)`, which since DD#75 does not even compile.

# 57.74 `${#a[@]}` / `${#a[*]}` — element count
run_convert '#!/bin/bash
a=(x y z)
echo "${#a[@]}"
echo "${#a[*]}"'
[[ $rc = 0 && $out == *'$(len a)'* && $out != *'strlen'* && -z $err ]]; chk 57.74

# 57.75 bare `${#a}` on an array — byte length of ELEMENT 0, not the count
run_convert '#!/bin/bash
a=(x y z)
echo "${#a}"'
[[ $rc = 0 && $out == *'$(strlen ${a[0]})'* && $out != *'$(len a)'* && -z $err ]]; chk 57.75

# 57.76 bare `${#s}` on a scalar — byte length of the scalar
run_convert '#!/bin/bash
s=hello
echo "${#s}"'
[[ $rc = 0 && $out == *'$(strlen $s)'* && $out != *'len s'* && -z $err ]]; chk 57.76

# 57.77 `${#a[i]}` — byte length of one element
run_convert '#!/bin/bash
a=(x yy zzz)
echo "${#a[1]}"'
[[ $rc = 0 && $out == *'$(strlen ${a[1]})'* && $out != *'$(len a)'* && -z $err ]]; chk 57.77

# 57.78 the two spellings no longer convert to the same thing
run_convert '#!/bin/bash
a=(x y z)
echo "${#a}"
echo "${#a[@]}"'
[[ $rc = 0 && $out == *'$(strlen ${a[0]})'* && $out == *'$(len a)'* && -z $err ]]; chk 57.78

# 57.79 `${##…}` is a prefix strip, not a length — left alone
run_convert '#!/bin/bash
p=aabb
echo "${p##a}"'
[[ $rc = 0 && $out == *'${p##a}'* && $out != *'strlen'* && $out != *'len '* && -z $err ]]; chk 57.79

# 57.80 DD#97: bash declares an array implicitly; the keyword is introduced
run_convert '#!/bin/bash
a=(x y z)'
[[ $rc = 0 && $out == *'array a=(x y z)'* && -z $err ]]; chk 57.80

# 57.81 a subscripted assignment cannot take the keyword inline
run_convert '#!/bin/bash
a[0]=x'
[[ $rc = 0 && $out == *'array a'* && $out == *'a[0]=x'* && -z $err ]]; chk 57.81

# 57.82 an already-declared name does not get a second keyword
run_convert '#!/bin/bash
declare -A m
m[k]=v'
[[ $rc = 0 && $out == *'map m'* && $out != *'array m'* && -z $err ]]; chk 57.82

# 57.83 mapfile fills an array, so its target needs declaring too
run_convert '#!/bin/bash
mapfile -t c < f'
[[ $rc = 0 && $out == *'array c'* && -z $err ]]; chk 57.83

# 57.84 ROUND TRIP: all four rows compile and agree with bash (1 3 3 1 5)
rc=-1 out="" err=""
cat > .input.sh <<'SH'
#!/bin/bash
a=(x y z)
s=hello
echo "${#a}"
echo "${#a[@]}"
echo "${#a[*]}"
echo "${#a[1]}"
echo "${#s}"
SH
sash-convert .input.sh 2>err > .converted.sash
sash .converted.sash -o .binary 2>>err
./.binary > out 2>>err
rc=$?; out=$(<out); err=$(<err)
[[ $rc = 0 && $out == $'1\n3\n3\n1\n5' && -z $err ]]; chk 57.84

# 57.85 ROUND TRIP: a subscripted assignment compiles and runs
rc=-1 out="" err=""
cat > .input.sh <<'SH'
#!/bin/bash
a[0]=x
a[1]=y
echo "${a[1]}"
SH
sash-convert .input.sh 2>err > .converted.sash
sash .converted.sash -o .binary 2>>err
./.binary > out 2>>err
rc=$?; out=$(<out); err=$(<err)
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 57.85


# ── conv.x1-x9 DD#60 positionals: converted output runs, and agrees with bash ──
#
# The gate on `converter-positional-use-needs-bless` is "output either compiles
# or carries a MANUAL marker". These go further and round-trip it: convert,
# compile the conversion, run it with `a b c`, and compare against what bash
# printed for the same script. A substring assertion on converted text cannot
# catch a conversion that compiles and prints the wrong thing — and three of
# these did exactly that.

# conv.x1 a hoisted positional round-trips
run_convert '#!/bin/bash
echo "$1"'
converted=$out
run_sash "$converted" a b c
[[ $rc = 0 && $out = "a" && -z $err ]]; chk conv.x1

# conv.x2 two expansions in one string keep the separator between them.
#
# Stock tree-sitter-bash 0.25 gives the SECOND simple_expansion inside a
# string a span that includes the preceding whitespace ("$1 $1" → the second
# node is " $1"), so rewriting that span silently deleted the space: this
# printed "aa". Every expansion family the converter rewrites was affected.
run_convert '#!/bin/bash
echo "$1 $2"'
converted=$out
run_sash "$converted" a b c
[[ $rc = 0 && $out = "a b" && -z $err ]]; chk conv.x2

# conv.x3 the same source twice — separator kept, and the hoist is emitted once
run_convert '#!/bin/bash
echo "$1 $1"'
converted=$out
[[ $(grep -c 'arg1=\$(arg 1)' <<<"$converted") = 1 ]]; chk conv.x3a
run_sash "$converted" a b c
[[ $rc = 0 && $out = "a a" && -z $err ]]; chk conv.x3b

# conv.x4 $# is the user-arg count, not -1.
#
# The converter emitted `$(($(argc) - 1))`. A command substitution is not an
# arithmetic operand: it parsed as literal 0, so this printed **-1** — a
# wrong answer, silently, since the argv trio landed. `(argc)` is the DD#58
# arithmetic sexpr spelling.
run_convert '#!/bin/bash
echo "$#"'
converted=$out
run_sash "$converted" a b c
[[ $rc = 0 && $out = "3" && -z $err ]]; chk conv.x4

# conv.x5 for-in over "$@" iterates the USER arguments only.
#
# `$(argv)` includes argv[0] (DD#60), so the old output ran one extra
# iteration with the program's own path as the first value. Bash's "$@" is
# argv[1:] — that is `$(argv 1)`.
run_convert '#!/bin/bash
for x in "$@"; do echo "$x"; done'
converted=$out
run_sash "$converted" a b c
[[ $rc = 0 && $out = "a
b
c" && -z $err ]]; chk conv.x5

# conv.x6 the same, written across lines and braced, with an empty arg list
run_convert '#!/bin/bash
for x in "${@}"; do
  echo "[$x]"
done'
converted=$out
run_sash "$converted"
[[ $rc = 0 && -z $out && -z $err ]]; chk conv.x6

# conv.x7 a splat into a command carries a MANUAL marker — bash runs the
# command with zero arguments when "$@" is empty, and neither sash spelling
# that states the empty case (`@{a:-WORD}`, `@{a:?WORD}`) does that, so there
# is no faithful mechanical conversion to emit.
run_convert '#!/bin/bash
/bin/echo "$@"'
[[ $rc = 0 && $out == *'MANUAL'* && $out == *'SB5032'* && -z $err ]]; chk conv.x7

# conv.x8 function-local positionals still pass through untouched (DD#46)
run_convert '#!/bin/bash
one() { echo "$1 $#"; }'
[[ $rc = 0 && $out == *'echo "$1 $#"'* && $out != *'arg1='* && -z $err ]]; chk conv.x8

# conv.x9 a known env var twice in one string also keeps its separator
run_convert '#!/bin/bash
echo "$HOME $HOME"'
[[ $rc = 0 && $out == *'echo "$home $home"'* && -z $err ]]; chk conv.x9

# ── DD#80 subscripts ─────────────────────────────────────────
# bash evaluates every subscript arithmetically; sash takes a direct key. The
# bash idioms convert to sash that does not compile unless the subscript is
# rewritten (SB4030 for a bare name, SB4041 for an expression).

# 57.86 a bare name subscript on an array is a variable read → $i
run_convert '#!/bin/bash
declare -a a=(10 20 30)
i=1
echo "${a[i]}"'
[[ $rc = 0 && $out == *'echo "${a[$i]}"'* && -z $err ]]; chk 57.86

# 57.87 an expression subscript becomes explicit arithmetic
run_convert '#!/bin/bash
declare -a a=(10 20 30)
i=1
echo "${a[i+1]}"'
[[ $rc = 0 && $out == *'${a[$(( i+1 ))]}'* && -z $err ]]; chk 57.87

# 57.88 a nested subscript nests as an expansion, not as arithmetic.
# tree-sitter-bash cannot parse this form at all — it yields an ERROR node —
# so the rewrite runs off the raw text.
run_convert '#!/bin/bash
declare -a a=(10 20 30)
echo "${a[a[0]]}"'
[[ $rc = 0 && $out == *'${a[${a[0]}]}'* && -z $err ]]; chk 57.88

# 57.89 an integer literal is already a direct key — untouched
run_convert '#!/bin/bash
declare -a a=(10 20 30)
echo "${a[5]}"'
[[ $rc = 0 && $out == *'echo "${a[5]}"'* && -z $err ]]; chk 57.89

# 57.90 a bareword key on a map is already direct — untouched
run_convert '#!/bin/bash
declare -A m
m[foo]=1
echo "${m[foo]}"'
[[ $rc = 0 && $out == *'echo "${m[foo]}"'* && $out != *'${m[$foo]}'* && -z $err ]]; chk 57.90

# 57.91 an already-direct $var subscript is left alone (not double-dollared)
run_convert '#!/bin/bash
declare -a a=(10 20 30)
i=1
echo "${a[$i]}"'
[[ $rc = 0 && $out == *'echo "${a[$i]}"'* && $out != *'$$'* && -z $err ]]; chk 57.91

# 57.92 `@` in a subscript is a splat, not a ${var@X} transformation — it must
# not draw the MANUAL note (it did before the subscript work)
run_convert '#!/bin/bash
declare -a a=(1 2)
echo "${a[@]}"'
[[ $rc = 0 && $out != *'parameter transformation'* && -z $err ]]; chk 57.92

# ── DD#75 retired bracket forms ──────────────────────────────
# `${a[@]}` / `${a[*]}` / `${!a[@]}` are rejected by the compiler (SB4038).
# What replaces one depends on context: `@a` is a splat and is legal only as
# a for-in source, `$(join a)` is a scalar, and `$(keys m)` is a splat that
# cannot be joined at all (SB5025).

# 57.93 a for-in source becomes the bare splat, quotes and all
run_convert '#!/bin/bash
declare -a a=(1 2 3)
for v in "${a[@]}"; do echo "$v"; done'
[[ $rc = 0 && $out == *'for v in @a;'* && $out != *'[@]'* && -z $err ]]; chk 57.93

# 57.94 a value context becomes the join
run_convert '#!/bin/bash
declare -a a=(1 2 3)
x="${a[@]}"'
[[ $rc = 0 && $out == *'x=$(join a)'* && -z $err ]]; chk 57.94

# 57.95 `echo` takes the join — it rejects a splat outright (SB5030), and the
# join is what bash'"'"'s quoted form produces anyway
run_convert '#!/bin/bash
declare -a a=(1 2 3)
echo "${a[@]}"'
[[ $rc = 0 && $out == *'echo $(join a)'* && -z $err ]]; chk 57.95

# 57.96 `[*]` is the joined string everywhere
run_convert '#!/bin/bash
declare -a a=(1 2 3)
echo "${a[*]}"'
[[ $rc = 0 && $out == *'echo $(join a)'* && -z $err ]]; chk 57.96

# 57.97 spliced into a larger string, only the expansion is replaced
run_convert '#!/bin/bash
declare -a a=(1 2 3)
echo "pre${a[@]}post"'
[[ $rc = 0 && $out == *'"pre$(join a)post"'* && -z $err ]]; chk 57.97

# 57.98 a map'"'"'s keys in a for-in header become $(keys m)
run_convert '#!/bin/bash
declare -A m=([x]=1)
for k in "${!m[@]}"; do echo "$k"; done'
[[ $rc = 0 && $out == *'for k in $(keys m);'* && -z $err ]]; chk 57.98

# 57.99 an indexed array'"'"'s "keys" are indices, and sash has no index-range
# sugar — say so rather than invent a loop shape
run_convert '#!/bin/bash
declare -a a=(1 2 3)
for i in "${!a[@]}"; do echo "$i"; done'
[[ $rc = 0 && $out == *'MANUAL'* && $out == *'index-range'* \
   && $out != *'indirect expansion'* && -z $err ]]; chk 57.99

# 57.100 a splat into an external command has no faithful mechanical answer —
# an empty array would run it with too few arguments (SB5032)
run_convert '#!/bin/bash
declare -a a=(1 2 3)
/bin/grep foo "${a[@]}"'
[[ $rc = 0 && $out == *'MANUAL'* && $out == *'SB5032'* && -z $err ]]; chk 57.100

# 57.101 `${#a[@]}` already reached the length path — still $(len a)
run_convert '#!/bin/bash
declare -a a=(1 2 3)
echo "${#a[@]}"'
[[ $rc = 0 && $out == *'$(len a)'* && -z $err ]]; chk 57.101

# ── here-strings: `consumer <<< WORD` → `producer | consumer` (DD#83) ──

# 57.102 the plain form — `<<<` is exactly a pipe in sash
run_convert '#!/bin/bash
x=hello
cat <<< "$x"'
[[ $rc = 0 && $out == *'echo "$x" | cat'* && $out != *'<<<'* && -z $err ]]; chk 57.102

# 57.103 the consumer keeps its own arguments, and they stay on the right
run_convert '#!/bin/bash
x=hello
grep -c foo <<< "$x"'
[[ $rc = 0 && $out == *'echo "$x" | grep -c foo'* && -z $err ]]; chk 57.103

# 57.104 a `$(…)` word pipes the command directly — `echo $(cmd)` does not
# compile (SB5020, tainted substitution inline), so there is no other form
run_convert '#!/bin/bash
grep foo <<< "$(ls)"'
[[ $rc = 0 && $out == *'ls | grep foo'* && $out != *'echo'* && -z $err ]]; chk 57.104

# 57.105 …and that swap is a real divergence: `$(…)` collapses trailing
# newlines, the pipe does not
run_convert '#!/bin/bash
grep foo <<< "$(ls)"'
[[ $rc = 0 && $out == *'CHECK'* && $out == *'trailing newline'* ]]; chk 57.105

# 57.106 the common idiom — a compound consumer is a `redirected_statement`,
# so the producer goes in front of the whole loop
run_convert '#!/bin/bash
while read -r l; do echo "$l"; done <<< "$(printf "a\nb\n")"'
# (the CHECK note quotes `<<<`, so pin the absence of the *redirect*)
[[ $rc = 0 && $out == *'| while readline l'* && $out != *'done <<<'* && -z $err ]]; chk 57.106

# 57.107 `read v <<< "$x"` converts — the here-string is a redirect, not a
# computed target (it used to draw the DD#99 "not a plain name" MANUAL)
run_convert '#!/bin/bash
x=hello
read v <<< "$x"'
[[ $rc = 0 && $out == *'echo "$x" | readline v'* \
   && $out != *'not a plain name'* && -z $err ]]; chk 57.107

# 57.108 a literal word needs no quoting — sash `echo` never word-splits
run_convert '#!/bin/bash
cat <<< literal'
[[ $rc = 0 && $out == *'echo literal | cat'* && -z $err ]]; chk 57.108

# 57.109 except `-n`, which both bash and sash read as the retired
# no-newline flag (SB5059) rather than as text
run_convert '#!/bin/bash
cat <<< -n'
[[ $rc = 0 && $out == *'printf "%s\n" -n | cat'* && $out != *'echo -n'* && -z $err ]]; chk 57.109

# 57.110 another redirect on the same statement still binds the consumer
run_convert '#!/bin/bash
x=hello
cat <<< "$x" > out'
[[ $rc = 0 && $out == *'echo "$x" | cat  > out'* || $out == *'echo "$x" | cat > out'* ]]; chk 57.110

# ── DD#75 §1: the keyword specials are canonical, the punctuation is sugar ──

# 57.111 $? → $(status)
run_convert '#!/bin/bash
/bin/true
echo "$?"'
[[ $rc = 0 && $out == *'echo "$(status)"'* && -z $err ]]; chk 57.111

# 57.112 $$ → $(getpid), spliced inside a string
run_convert '#!/bin/bash
echo "pid-$$"'
[[ $rc = 0 && $out == *'echo "pid-$(getpid)"'* && -z $err ]]; chk 57.112

# 57.113 $! → $(getcpid)
run_convert '#!/bin/bash
/bin/sleep 0 &
echo $!'
[[ $rc = 0 && $out == *'echo $(getcpid)'* && -z $err ]]; chk 57.113

# 57.114 in arithmetic the canonical spelling is the DD#58 sexpr — the
# command-substitution form is not an arith operand (SB4020), the same trap
# `$#` → `$(( (argc) - 1 ))` already had
run_convert '#!/bin/bash
n=$(( $? + $$ ))'
[[ $rc = 0 && $out == *'$(( (status) + (getpid) ))'* && -z $err ]]; chk 57.114

# 57.115 …and these three are process-wide, so unlike $@/$*/$# they convert
# inside a function body too
run_convert '#!/bin/bash
f() { /bin/true; echo $?; }'
[[ $rc = 0 && $out == *'echo $(status)'* && -z $err ]]; chk 57.115

# ── whole-node rewrites must compose with patches inside the node ──
# A `[ … ]` replacement and a patch inside it cannot both be applied: the
# inner one re-emits source the outer one already consumed. The output was
# silently corrupt, not an error.

# 57.116 `[ "$HOME" = x ]` — was `if [[ $HOME = x ]]$home" = x ]`
run_convert '#!/bin/bash
if [ "$HOME" = x ]; then echo y; fi'
[[ $rc = 0 && $out == *'if [[ $home = x ]]; then echo y; fi'* && -z $err ]]; chk 57.116

# 57.117 …and the hoisted getenv+attest survives the splice
run_convert '#!/bin/bash
if [ "$HOME" = x ]; then echo y; fi'
[[ $rc = 0 && $out == *'home=$(getenv HOME)'* && $out == *'attest home'* ]]; chk 57.117

# 57.118 `[ "$1" = x ]` — the commonest shape of the bug
run_convert '#!/bin/bash
if [ "$1" = x ]; then echo y; fi'
[[ $rc = 0 && $out == *'if [[ $arg1 = x ]]; then echo y; fi'* && -z $err ]]; chk 57.118

# 57.119 `${#s}` inside a test converts to $(strlen …) exactly once
run_convert '#!/bin/bash
s=abc
if [ "${#s}" -gt 0 ]; then echo y; fi'
[[ $rc = 0 && $out == *'$(strlen $s)'* && $out != *'${#s}'* && -z $err ]]; chk 57.119

# 57.120 a test with nothing to splice is untouched by the composition
run_convert '#!/bin/bash
if [ x = y ]; then echo y; fi'
[[ $rc = 0 && $out == *'if [[ x = y ]]; then echo y; fi'* && -z $err ]]; chk 57.120

set -e
tally
