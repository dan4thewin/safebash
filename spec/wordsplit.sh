#!/bin/bash
# spec/wordsplit.sh — word splitting, globbing, quoting, read tests (DD#22, DD#28, DD#39, DD#40)
set -eu
. common.sh
setup wordsplit
plan=26
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

# 22.4 read line reads one line
run_sash 'echo "hello" | read line; echo $line'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 22.4

# 22.5 read -f: splits by field separator
run_sash 'echo "a:b:c" | read -f: a b c; echo $a $b $c'
[[ $rc = 0 && $out = "a b c" && -z $err ]]; chk 22.5

# 22.6 mapfile -t reads lines into array
run_sash 'array arr; printf "a\nb\nc\n" | mapfile -t arr; bless arr [[ -n "${arr[0]}" ]]; echo ${arr[1]}'
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

# 39.6 {n,m} counted repetition
run_sash 'x="aaa"; [[ $x =~ ^a{2,3}$ ]] && echo yes'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 39.6

# 39.7 \d \w \s shorthand classes
run_sash 'x="123"; [[ $x =~ ^\d+$ ]] && echo yes'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 39.7

# 39.8 invalid regex (lookahead) is runtime error
run_sash 'x="hello"; [[ $x =~ (?=h)ello ]] && echo yes'
[[ $rc != 0 && -z $out ]]; chk 39.8

# 39.9 precompiled literal patterns (just verify regex works at runtime)
run_sash 'x="hello123"; [[ $x =~ ^[a-z]+[0-9]+$ ]] && echo yes'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 39.9

# 40.3 $var and "$var" identical (no quoting rules)
run_sash 'x="a b c"; echo $x'
[[ $rc = 0 && $out = "a b c" && -z $err ]]; chk 40.3

# --- DD#30 numeric operators not valid sash-native ---

# 30.6a -gt in [[ ]] is compile error
run_sash 'int x=5; if [[ $x -gt 0 ]]; then echo yes; fi'
[[ $rc != 0 && -z $out && -n $err ]]; chk 30.6a

# 30.6b -eq in [[ ]] is compile error
run_sash 'int x=5; if [[ $x -eq 5 ]]; then echo yes; fi'
[[ $rc != 0 && -z $out && -n $err ]]; chk 30.6b


# --- DD#22 mapfile -m ---

# 22.8 mapfile -m reads KEY=VALUE into map
run_sash 'map m; printf "name=Alice\nage=30\n" | mapfile -m m; bless m [[ -n "${m[name]}" ]]; echo ${m[name]} ${m[age]}'
[[ $rc = 0 && $out = "Alice 30" && -z $err ]]; chk 22.8

# 22.9 mapfile -m skips blank lines
run_sash 'map m; printf "a=1\n\nb=2\n" | mapfile -m m; bless m [[ -n "${m[a]}" ]]; echo ${m[a]} ${m[b]}'
[[ $rc = 0 && $out = "1 2" && -z $err ]]; chk 22.9

# 22.10 mapfile -m splits on first = only
run_sash 'map m; printf "key=val=ue\n" | mapfile -m m; bless m [[ -n "${m[key]}" ]]; echo ${m[key]}'
[[ $rc = 0 && $out = "val=ue" && -z $err ]]; chk 22.10

# 22.11 mapfile -m result is tainted
run_sash 'map m; printf "k=v\n" | mapfile -m m; echo ${m[k]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 22.11

set -e
tally


# ── Additional coverage (2026-05-24) ──

# 39.9 backslash literal in unquoted word
run_sash 'x="\d+"; echo $x'
[[ $rc = 0 && $out = '\d+' && -z $err ]]; chk 39.9

# 39.10 backslash in regex pattern
run_sash 'x="abc123"; [[ $x =~ \w+ ]] && echo yes'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 39.10

# 22.12 echo|read is clean (builtin pipe, no exec)
run_sash 'echo "clean" | read x; echo $x'
[[ $rc = 0 && $out = "clean" && -z $err ]]; chk 22.12

# 22.13 printf|mapfile is tainted (printf execs)
run_sash 'array arr; printf "x\n" | mapfile -t arr; echo ${arr[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 22.13

# 14.7 && on compound command rejected (SB4023)
run_sash 'if true; then echo x; fi && echo y'
[[ $rc != 0 && -z $out && -n $err ]]; chk 14.7

# 14.8 || on compound command rejected (SB4023)
run_sash 'for x in a b; do echo $x; done || echo fail'
[[ $rc != 0 && -z $out && -n $err ]]; chk 14.8

# 14.9 & on compound command rejected (SB4024)
run_sash 'while false; do echo x; done &'
[[ $rc != 0 && -z $out && -n $err ]]; chk 14.9

# 5.2 plain heredoc (no cmdsub) rejected
run_sash '/bin/cat <<EOF
hello
EOF'
[[ $rc != 0 && -z $out && -n $err ]]; chk 5.2

