#!/bin/bash
# spec/convert.sh — acceptance tests for sash-convert (DD#57)
set -eu
. common.sh
setup convert
plan=56
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

# 57.6b [[ ]] unchanged
run_convert '#!/bin/bash
if [[ "$x" -gt 0 ]]; then echo yes; fi'
[[ $rc = 0 && $out == *'[[ "$x" -gt 0 ]]'* && -z $err ]]; chk 57.6b

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
[[ $rc = 0 && $out == *'# CHECK:'*'SC2086'* && -z $err ]]; chk 57.31

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

# --- DD#57: Positional conversion ---

# 57.13 $1 → $(argv 1)
run_convert '#!/bin/bash
echo $1'
[[ $rc = 0 && $out == *'$(argv 1)'* && -z $err ]]; chk 57.13

# --- DD#57: Trap conversion ---

# 57.14 trap "" SIG → trap : SIG
run_convert '#!/bin/bash
trap "" SIGINT'
[[ $rc = 0 && $out == *'trap : SIGINT'* && $out != *'trap ""'* && -z $err ]]; chk 57.14

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

# --- DD#57: BLESS annotations ---

# 57.50a known external → bless line
run_convert '#!/bin/bash
today=$(date +%Y-%m-%d)'
[[ $rc = 0 && $out == *'bless today'* && $out == *'today=$(date +%Y-%m-%d)'* && -z $err ]]; chk 57.35a

# 57.50b known external → bless line
run_convert '#!/bin/bash
host=$(hostname)'
[[ $rc = 0 && $out == *'bless host'* && $out == *'host=$(hostname)'* && -z $err ]]; chk 57.35b

# 57.51 unknown external → BLESS hint
run_convert '#!/bin/bash
result=$(some_unknown_command arg1)'
[[ $rc = 0 && $out == *'# BLESS:'*'some_unknown_command'* && $out == *'result=$(some_unknown_command arg1)'* && -z $err ]]; chk 57.36

# 57.52 read → BLESS hint
run_convert '#!/bin/bash
read -r line'
[[ $rc = 0 && $out == *'# BLESS:'*'line'*'read'* && $out == *'read -r line'* && -z $err ]]; chk 57.37

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

# 30.7 convert unset var (scalar) to var=""
run_convert '#!/bin/bash
x=hello
unset x'
[[ $rc = 0 && $out == *'x=""'* && -z $err ]]; chk 30.7


set -e
tally
