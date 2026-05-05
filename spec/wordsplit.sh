#!/bin/bash
# spec/wordsplit.sh — word splitting, globbing, quoting, read tests (DD#22, DD#28, DD#39, DD#40)
set -eu
. common.sh
setup wordsplit
plan=22
echo "1..$plan" >&3

set +e

# 22.1 $var and "$var" semantically identical
run_sash 'x="hello world"; echo $x'
[[ $rc = 0 && $out = "hello world" && -z $err ]]; chk 22.1

# 22.2 IFS does not exist
run_sash 'IFS=:; echo ok'
[[ $rc != 0 && -z $out && -n $err ]]; chk 22.2

# 22.3 for x in $choices → compile error
run_sash 'x="a b c"; for y in $x; do echo $y; done'
[[ $rc != 0 && -z $out && -n $err ]]; chk 22.3

# 22.4 read -r line reads one line
run_sash 'echo "hello" | read -r line; echo $line'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 22.4

# 22.5 read -d: splits by delimiter
run_sash 'echo "a:b:c" | read -d: -r a b c; echo $a $b $c'
[[ $rc = 0 && $out = "a b c" && -z $err ]]; chk 22.5

# 22.6 mapfile -t reads lines into array
run_sash 'printf "a\nb\nc\n" | mapfile -t arr; echo ${arr[1]}'
[[ $rc = 0 && $out = "b" && -z $err ]]; chk 22.6

# 22.7 -r always implied on read
run_sash 'echo "hello" | read line; echo $line'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 22.7

# 28.1 for x in $var → compile error
run_sash 'choices="a b c"; for x in $choices; do echo $x; done'
[[ $rc != 0 && -z $out && -n $err ]]; chk 28.1

# 28.2 for x in "${arr[@]}" accepted
run_sash 'array a=(one two three); for x in "${a[@]}"; do echo $x; done'
[[ $rc = 0 && $out == *"one"* && -z $err ]]; chk 28.2

# 39.1 only * glob supported, ? and [abc] rejected
run_sash 'for f in /tmp/?; do echo $f; done'
[[ $rc != 0 && -z $out && -n $err ]]; chk 39.1a

run_sash 'for f in /tmp/[abc]; do echo $f; done'
[[ $rc != 0 && -z $out && -n $err ]]; chk 39.1b

# 39.2 var =~ s/re/str/ mutates in place
run_sash 'x="hello world"; x =~ s/world/earth/; echo $x'
[[ $rc = 0 && $out = "hello earth" && -z $err ]]; chk 39.2

# 39.3 ${var:s/re/str/} returns without mutating
run_sash 'x="hello world"; echo ${x:s/world/earth/}'
[[ $rc = 0 && $out = "hello earth" && -z $err ]]; chk 39.3

# 39.4 ${var/pat/rep} accepted via glob-to-regex coercion
run_sash 'x="hello world"; echo ${x/world/earth}'
[[ $rc = 0 && $out = "hello earth" && -z $err ]]; chk 39.4

# 39.5 literal regex validated at compile time
run_sash 'x="hello"; x =~ s/[invalid/repl/; echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 39.5

# 39.6 no {n,m} counted repetition
run_sash 'x="aaa"; [[ $x =~ a{2,3} ]] && echo yes'
[[ $rc != 0 && -z $out && -n $err ]]; chk 39.6

# 39.7 no \d \w shortcuts
run_sash 'x="123"; [[ $x =~ \d+ ]] && echo yes'
[[ $rc != 0 && -z $out && -n $err ]]; chk 39.7

# 39.8 no lookahead/lookbehind
run_sash 'x="hello"; [[ $x =~ (?=h)ello ]] && echo yes'
[[ $rc != 0 && -z $out && -n $err ]]; chk 39.8

# 39.9 precompiled literal patterns (just verify regex works at runtime)
run_sash 'x="hello123"; [[ $x =~ ^[a-z]+[0-9]+$ ]] && echo yes'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 39.9

# 40.3 $var and "$var" identical (no quoting rules)
run_sash 'x="a b c"; echo $x'
[[ $rc = 0 && $out = "a b c" && -z $err ]]; chk 40.3

set -e
tally

# --- DD#30 numeric operators not valid sash-native ---

# 30.6a -gt in [[ ]] is compile error
run_sash 'int x=5; if [[ $x -gt 0 ]]; then echo yes; fi'
[[ $rc != 0 && -z $out && -n $err ]]; chk 30.6a

# 30.6b -eq in [[ ]] is compile error
run_sash 'int x=5; if [[ $x -eq 5 ]]; then echo yes; fi'
[[ $rc != 0 && -z $out && -n $err ]]; chk 30.6b

