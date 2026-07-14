#!/bin/bash
# spec/wordsplit.sh — word splitting, globbing, quoting, read tests (DD#22, DD#28, DD#39, DD#40)
set -eu
. common.sh
setup wordsplit
plan=70
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
run_sash 'array arr; printf "a\nb\nc\n" | mapfile -t arr; bless x in arr [[ -n $x ]]; echo ${arr[1]}'
[[ $rc = 0 && $out = "b" && -z $err ]]; chk 22.6

# 22.7 -r always implied on read
run_sash 'echo "hello" | read line; echo $line'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 22.7

# 28.1 for x in $var → compile error
run_sash 'choices="a b c"; for x in $choices; do echo $x; done'
[[ $rc != 0 && -z $out && -n $err ]]; chk 28.1

# 28.2 for x in @a accepted (sash-native bare splat — DD#41)
run_sash 'array a=(one two three); for x in @a; do echo $x; done'
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

# 39.4 ${var/pat/rep} dropped (DD#65) — use $(sub)/$(gsub); rejects SB4034
run_sash 'x="hello world"; echo ${x/world/earth}'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk 39.4

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
run_sash 'map m; printf "name=Alice\nage=30\n" | mapfile -m m; bless k v in m [[ -n $k && -n $v ]]; echo ${m[name]} ${m[age]}'
[[ $rc = 0 && $out = "Alice 30" && -z $err ]]; chk 22.8

# 22.9 mapfile -m skips blank lines
run_sash 'map m; printf "a=1\n\nb=2\n" | mapfile -m m; bless k v in m [[ -n $k && -n $v ]]; echo ${m[a]} ${m[b]}'
[[ $rc = 0 && $out = "1 2" && -z $err ]]; chk 22.9

# 22.10 mapfile -m splits on first = only
run_sash 'map m; printf "key=val=ue\n" | mapfile -m m; bless k v in m [[ -n $k && -n $v ]]; echo ${m[key]}'
[[ $rc = 0 && $out = "val=ue" && -z $err ]]; chk 22.10

# 22.11 mapfile -m result is tainted
run_sash 'map m; printf "k=v\n" | mapfile -m m; echo ${m[k]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 22.11

# --- DD#22 addendum: quotes always yield a single string ---
# Quoting an array splat is a compile error (no decorative-quote unwrap).

# ws.q1 quoted brace splat "${a[@]}" rejected
run_sash 'array a=(x y); for x in "${a[@]}"; do echo "$x"; done'
[[ $rc != 0 && -z $out && -n $err ]]; chk ws.q1

# ws.q2 quoted dollar splat "$a[@]" rejected
run_sash 'array a=(x y); for x in "$a[@]"; do echo "$x"; done'
[[ $rc != 0 && -z $out && -n $err ]]; chk ws.q2

# ws.q3 quoted argv splat "$(argv)" rejected
run_sash 'for x in "$(argv)"; do echo "$x"; done'
[[ $rc != 0 && -z $out && -n $err ]]; chk ws.q3

# ws.q4 bare dollar splat $a[@] accepted (input convenience; canonical is @a)
run_sash 'array a=(x y); for x in $a[@]; do echo "$x"; done'
[[ $rc = 0 && $out = $'x\ny' && -z $err ]]; chk ws.q4

# ws.q5 bare brace splat ${a[@]} accepted (input convenience; canonical is @a)
run_sash 'array a=(x y); for x in ${a[@]}; do echo "$x"; done'
[[ $rc = 0 && $out = $'x\ny' && -z $err ]]; chk ws.q5

# 64.1 DD#64: backslash is a literal byte in "..." — no \\ collapse
run_sash 'echo "a\\b"'
[[ $rc = 0 && $out = 'a\\b' && -z $err ]]; chk 64.1

# 64.2 DD#64: $ still interpolates in "..."; the preceding \ stays literal
run_sash 'x=hi; echo "v\$x"'
[[ $rc = 0 && $out = 'v\hi' && -z $err ]]; chk 64.2

# 64.3 DD#64: a literal " comes from single-quote concatenation
run_sash 'echo a'\''"'\''b'
[[ $rc = 0 && $out = 'a"b' && -z $err ]]; chk 64.3

# 64.4 DD#64 addendum: $'...' expands inside "..." (control byte in place)
run_sash $'echo "a$\'\\t\'b"'
[[ $rc = 0 && $out = $'a\tb' && -z $err ]]; chk 64.4

# 64.5 DD#64 addendum: C-string mixes with interpolation in "..."
run_sash $'x=V\necho "p$\'\\t\'$x"'
[[ $rc = 0 && $out = $'p\tV' && -z $err ]]; chk 64.5

# 64.6 DD#64 addendum: a literal " inside "..." via $'\x22' (universal hatch)
run_sash $'echo "a$\'\\x22\'b"'
[[ $rc = 0 && $out = 'a"b' && -z $err ]]; chk 64.6

# 64.7 DD#64 addendum: $'...' close is escape-aware (\' does not terminate)
run_sash $'echo "a$\'x\\\'y\'b"'
[[ $rc = 0 && $out = "ax'yb" && -z $err ]]; chk 64.7

# 64.8 DD#64 addendum: a lone ' (not preceded by $) stays literal in "..."
run_sash 'echo "it'\''s"'
[[ $rc = 0 && $out = "it's" && -z $err ]]; chk 64.8

# 62.1 DD#62: == is exact equality (regression)
run_sash 'if [[ abc == abc ]]; then echo y; fi'
[[ $rc = 0 && $out = y && -z $err ]]; chk 62.1

# 62.2 DD#62: unquoted RHS of == is NO LONGER a glob — literal
run_sash 'if [[ abc == ab* ]]; then echo glob; else echo exact; fi'
[[ $rc = 0 && $out = exact && -z $err ]]; chk 62.2

# 62.3 DD#62: literal star matches literal star
run_sash 'if [[ "ab*" == ab* ]]; then echo y; fi'
[[ $rc = 0 && $out = y && -z $err ]]; chk 62.3

# 62.4 DD#62: != is exact too (abc != literal ab*)
run_sash 'if [[ abc != ab* ]]; then echo y; fi'
[[ $rc = 0 && $out = y && -z $err ]]; chk 62.4

# 62.5 DD#62: -v is removed (rejected, not silently -n)
run_sash 'x=5; if [[ -v x ]]; then echo y; fi'
[[ $rc != 0 && -z $out && $err == *SB5028* ]]; chk 62.5

# 62.6 DD#62: -n is the way to test a non-empty value
run_sash 'x=5; if [[ -n $x ]]; then echo y; fi'
[[ $rc = 0 && $out = y && -z $err ]]; chk 62.6

# 62.7 && / || are left-associative: (false && A) || B runs B
run_sash 'false && echo A || echo B'
[[ $rc = 0 && $out = B && -z $err ]]; chk 62.7

# 62.8-62.10 nested && / || chains. Now correct in all three backends
# (bug-andor-backend-chain-status fixed): janet/c branch on exit status,
# not language value-truthiness, and every command sets the status var.
run_sash '[[ 1 = 1 ]] && echo A || echo B'
[[ $rc = 0 && $out = A && -z $err ]]; chk 62.8
run_sash 'false && echo A || echo B && echo C'
[[ $rc = 0 && $out = $'B\nC' && -z $err ]]; chk 62.9
run_sash 'true || echo A && echo B'
[[ $rc = 0 && $out = B && -z $err ]]; chk 62.10
# 62.11 per-command status pin: a successful echo must leave status 0 so a
# following `&& cmd` runs (the c/janet regression this fix closes).
run_sash 'echo A && echo B && echo C'
[[ $rc = 0 && $out = $'A\nB\nC' && -z $err ]]; chk 62.11


# ── Additional coverage (2026-05-24) ──

# 39.9 backslash literal in unquoted word
run_sash 'x="\d+"; echo $x'
[[ $rc = 0 && $out = '\d+' && -z $err ]]; chk 39.11

# 39.10 backslash in regex pattern
run_sash 'x="abc123"; [[ $x =~ \w+ ]] && echo yes'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 39.10

# 22.12 echo|read is clean (builtin pipe, no exec) — native only. `echo | read`
# is the lastpipe inline-pipe (the read builtin consumes the pipe and sets a
# PARENT variable): c crashes on an invalid pipe fd and janet spawns `read`
# as an external. See echo-read-inline-pipe-cjanet.
if in_modes native; then
    run_sash 'echo "clean" | read x; echo $x'
    [[ $rc = 0 && $out = "clean" && -z $err ]]; chk 22.12
else
    skip_chk 22.12
fi

# 22.13 printf|mapfile is tainted (printf execs)
run_sash 'array arr; printf "x\n" | mapfile -t arr; echo ${arr[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 22.13

# 22.14 @a command-arg splat → one verbatim arg per element (DD#20/#22).
# DD#69: an external splat needs a disposition even for a literal array;
# `@{a:?…}` requires non-empty (never fires here). /usr/bin/printf runs as a
# real external in all three backends.
run_sash 'array a=(one two three); /usr/bin/printf "[%s]\n" @{a:?empty}'
[[ $rc = 0 && $out = $'[one]\n[two]\n[three]' && -z $err ]]; chk 22.14

# 22.15 no word splitting WITHIN an element: a value with a space is one arg
run_sash 'array a=("a b" c); /usr/bin/printf "[%s]\n" @{a:?empty}'
[[ $rc = 0 && $out = $'[a b]\n[c]' && -z $err ]]; chk 22.15

# 22.16 splat interleaves with scalar args, preserving order
run_sash 'array a=(x y); /usr/bin/printf "%s|" pre @{a:?empty} post'
[[ $rc = 0 && $out = "pre|x|y|post|" && -z $err ]]; chk 22.16

# 22.17 multiple splats expand in order
run_sash 'array a=(x); array b=(y z); /usr/bin/printf "%s|" @{a:?e} @{b:?e}'
[[ $rc = 0 && $out = "x|y|z|" && -z $err ]]; chk 22.17

# 22.18 empty array → ZERO args via the `@{e:-}` disposition (cat reads
# stdin, not an empty-name file).
run_sash 'array e=(); /usr/bin/printf STDIN | cat @{e:-}'
[[ $rc = 0 && $out = "STDIN" && -z $err ]]; chk 22.18

# 22.19 splat in a pipeline stage (janet pipe plumbing fixed: os/pipe :WR)
run_sash 'array a=(p q r); /usr/bin/printf "[%s]\n" @{a:?e} | cat'
[[ $rc = 0 && $out = $'[p]\n[q]\n[r]' && -z $err ]]; chk 22.19

# 20.1 inline `*` glob in command-arg position is a compile error (DD#20/#69:
#      globs are array sugar; no inline glob expansion)
run_sash 'echo *'
[[ $rc != 0 && -z $out && -n $err ]]; chk 20.1

# 20.2 inline `*.sh` glob in command args is rejected too
run_sash 'ls *.sh'
[[ $rc != 0 && -z $out && -n $err ]]; chk 20.2

# 20.3 quoted "*" is a literal asterisk, not a glob
run_sash 'echo "*"'
[[ $rc = 0 && $out = "*" && -z $err ]]; chk 20.3

# 20.4 array-literal glob `array a=(*)` still globs (unchanged)
run_sash 'array a=(*); echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 20.4

# 20.5 for-in glob: 0 matches → loop body never runs (DD#20/#69). native/c
#      only — Janet lowers from IR and does not expand globs.
if in_modes native,c; then
    run_sash 'for f in *.nomatchZZZ; do echo ran; done; echo done'
    [[ $rc = 0 && $out = "done" && -z $err ]]; chk 20.5
else
    skip_chk 20.5
fi

# 20.6 for-in glob: iterates per matched entry, no word splitting (native/c)
if in_modes native,c; then
    run_sash '/usr/bin/touch only.www; for f in *.www; do echo "[$f]"; done'
    [[ $rc = 0 && $out = "[only.www]" && -z $err ]]; chk 20.6
else
    skip_chk 20.6
fi

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


set -e
tally
