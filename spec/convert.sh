#!/bin/bash
# spec/convert.sh — acceptance tests for sash-convert (DD#57)
set -eu
. common.sh
# sash-convert is a separate tool, independent of sash backend.
# Run only in native mode to avoid 3× redundancy.
[[ ${SASH_MODE:-native} != native ]] && exit 0
setup convert
plan=88
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

# 57.13 top-level $1 → $(arg 1)
run_convert '#!/bin/bash
echo $1'
[[ $rc = 0 && $out == *'$(arg 1)'* && -z $err ]]; chk 57.13a

# 57.13b ${1} → $(arg 1), ${@} → $(argv), ${#} → $(($(argc) - 1))
run_convert '#!/bin/bash
echo ${1} ; for x in ${@}; do echo $x; done ; echo ${#}'
[[ $rc = 0 && $out == *'$(arg 1)'* && $out == *'$(argv)'* && $out == *'$(($(argc) - 1))'* && -z $err ]]; chk 57.13b

# 57.13c ${#x} length sugar → $(len x) (DD#75 §4; strips any subscript). ${#}
# (arg count) is unaffected — it still maps to $(($(argc) - 1)) above.
run_convert '#!/bin/bash
arr=(a b c); echo ${#arr}; s=hi; echo ${#s}; echo ${#arr[@]}'
[[ $rc = 0 && $out == *'$(len arr)'* && $out == *'$(len s)'* && $out != *'${#'* && -z $err ]]; chk 57.13c

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

# conv.q1 top-level for-in over "$@" → bare $(argv) (quotes dropped, DD#22)
run_convert '#!/bin/bash
for x in "$@"; do echo "$x"; done'
[[ $rc = 0 && $out == *'for x in $(argv)'* && -z $err ]]; chk conv.q1

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


set -e
tally
