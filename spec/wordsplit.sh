#!/bin/bash
# spec/wordsplit.sh — word splitting, globbing, quoting, read tests (DD#22, DD#28, DD#39, DD#40)
set -eu
. common.sh
setup wordsplit
plan=133
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
run_sash 'echo "hello" | readline line; echo $line'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 22.4

# 22.5 `read -f:` (field separator) is retired — DD#99 splits on NUL only, so
# a separator is converted upstream (`tr`) or matched with `matchall`. The
# multi-target form goes with it: `read` takes one array, or a source and an
# array.
run_sash 'echo "a:b:c" | read -f: a b c; echo $a $b $c'
[[ $rc != 0 && -z $out && $err == *SB5057* ]]; chk 22.5

# 22.6 mapfile -t reads lines into array
run_sash 's="a
b
c"
- matchall $s "[^\x0a]+" arr
echo ${arr[1]}'
[[ $rc = 0 && $out = "b" && -z $err ]]; chk 22.6

# 22.7 -r always implied on read
run_sash 'echo "hello" | readline line; echo $line'
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
run_sash 's="name=Alice
age=30"
- matchall $s "([^=\x0a]*)=([^\x0a]*)" p
map m = @{p:~}
echo ${m[name]} ${m[age]}'
[[ $rc = 0 && $out = "Alice 30" && -z $err ]]; chk 22.8

# 22.9 mapfile -m skips blank lines
run_sash 's="a=1

b=2"
- matchall $s "([^=\x0a]*)=([^\x0a]*)" p
map m = @{p:~}
echo ${m[a]} ${m[b]}'
[[ $rc = 0 && $out = "1 2" && -z $err ]]; chk 22.9

# 22.10 mapfile -m splits on first = only
run_sash 's="key=val=ue"
- matchall $s "([^=\x0a]*)=([^\x0a]*)" p
map m = @{p:~}
echo ${m[key]}'
[[ $rc = 0 && $out = "val=ue" && -z $err ]]; chk 22.10

# 22.11 an EXTERNAL producer taints the array, and a tainted array cannot be
# paired into a map at all (DG 2026-09-10: "the read of the array must have a
# clean array, so no tainted maps"). The gate that used to sit on the map now
# sits on the array — same gate, one step earlier.
run_sash '/usr/bin/printf "k=v\n" | read p
map m = @{p:~}
echo ${m[k]}'
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

# ws.q4 bare dollar splat $a[@] → retired (DD#75 §4 — use @a); SB4038
run_sash 'array a=(x y); for x in $a[@]; do echo "$x"; done'
[[ $rc != 0 && -z $out && $err == *SB4038* ]]; chk ws.q4

# ws.q5 bare brace splat ${a[@]} → retired (DD#75 §4 — use @a); SB4038
run_sash 'array a=(x y); for x in ${a[@]}; do echo "$x"; done'
[[ $rc != 0 && -z $out && $err == *SB4038* ]]; chk ws.q5

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

# 62.20 -o (shell-option test) is unsupported — reject at validate, not codegen ICE
run_sash 'if [[ -o errexit ]]; then echo y; fi'
[[ $rc != 0 && -z $out && $err == *SB5028* ]]; chk 62.20

# 62.21 -R (nameref test) is unsupported — reject at validate, not codegen ICE
run_sash 'if [[ -R x ]]; then echo y; fi'
[[ $rc != 0 && -z $out && $err == *SB5028* ]]; chk 62.21

# 62.6 DD#62: -n is the way to test a non-empty value
run_sash 'x=5; if [[ -n $x ]]; then echo y; fi'
[[ $rc = 0 && $out = y && -z $err ]]; chk 62.6

# 62.7 SUPERSEDED by DD#95 (2026-09-08). This used to pin the shell rule that
# `&&`/`||` are equal-precedence and left-associative, so `false && A || B` ran
# B. That is exactly the text which groups differently inside `[[ ]]`, so sash
# now rejects the mixture rather than have one spelling mean two things.
run_sash 'false && echo A || echo B'
[[ $rc != 0 && -z $out && $err == *SB4053* ]]; chk 62.7

# 62.8-62.10 nested && / || chains. Branch on exit status, not language
# value-truthiness, and every command sets the status var.
# 62.9/62.10 are likewise superseded by DD#95 — both mix the two operators.
run_sash 'if [[ 1 = 1 ]] { echo A } else { echo B }'
[[ $rc = 0 && $out = A && -z $err ]]; chk 62.8
run_sash 'false && echo A || echo B && echo C'
[[ $rc != 0 && -z $out && $err == *SB4053* ]]; chk 62.9
run_sash 'true || echo A && echo B'
[[ $rc != 0 && -z $out && $err == *SB4053* ]]; chk 62.10
# 62.30 per-command status pin: a successful echo must leave status 0 so a
# following `&& cmd` runs.
run_sash 'echo A && echo B && echo C'
[[ $rc = 0 && $out = $'A\nB\nC' && -z $err ]]; chk 62.30


# ── Additional coverage (2026-05-24) ──

# 39.9 backslash literal in unquoted word
run_sash 'x="\d+"; echo $x'
[[ $rc = 0 && $out = '\d+' && -z $err ]]; chk 39.11

# 39.10 backslash in regex pattern
run_sash 'x="abc123"; [[ $x =~ \w+ ]] && echo yes'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 39.10

# ── the regex dialect, pinned (DD#39 clarification, DG 2026-09-11) ──
# The engine is Plan 9 / Pike's, chosen deliberately; POSIX compliance is not
# a goal. DD#39's PROSE went stale twice (it claimed no `{n,m}`, then claimed
# no `\d`/`\w`) because nothing executable pinned the surface. These do.
# Each divergence below is deliberate, not a gap -- see the DD before "fixing"
# one. Real wrong ANSWERS remain bugs (bug-regex-blind-past-interior-nul).

# 39.12 a literal `-` in a bracket class is spelled `\-`, in any position.
# POSIX would make a first/last `-` a literal; this engine does not, so the
# escape is the spelling (same shape as `\{` and `\[`, 2026-07-08 ruling).
run_sash 'str a="a-b"; str b="-ab"; str c="a_b-c"
if [[ $a =~ ^[A-Za-z\-]+$ ]]; then echo 1; fi
if [[ $b =~ ^[\-A-Za-z]+$ ]]; then echo 2; fi
if [[ $c =~ ^[a-z_\-]+$ ]]; then echo 3; fi'
[[ $rc = 0 && $out = $'1\n2\n3' && -z $err ]]; chk 39.12

# 39.13 ...and an UNescaped first/last `-` is a compile error, deliberately.
# bash accepts this; sash does not, and that divergence is the ruling.
run_sash 'str s=hi; if [[ $s =~ ^[A-Za-z-]+$ ]]; then echo m; fi'
[[ $rc != 0 && -z $out && $err == *SB5023* ]]; chk 39.13

# 39.14 the six shorthand classes are supported (DD#39 claimed otherwise
# until 2026-09-11 -- the vendored `bldshortclass` provides them)
run_sash 'str d="123"; str w="a_1"; str p="a b"
if [[ $d =~ ^\d+$ ]]; then echo d; fi
if [[ $w =~ ^\w+$ ]]; then echo w; fi
if [[ $p =~ ^a\sb$ ]]; then echo s; fi
if [[ $w =~ ^\D ]]; then echo D; fi
if [[ $p =~ ^\S ]]; then echo S; fi'
[[ $rc = 0 && $out = $'d\nw\ns\nD\nS' && -z $err ]]; chk 39.14

# 39.15 `{n,m}` works and is capped at 32 (a vendored addition, not upstream)
run_sash 'str s="aa"; if [[ $s =~ ^a{2,3}$ ]]; then echo ok; fi'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 39.15

# 39.16 ...over the cap is a compile error
run_sash 'str s="aaa"; if [[ $s =~ ^a{33}$ ]]; then echo m; fi'
[[ $rc != 0 && -z $out && $err == *SB5023* ]]; chk 39.16

# 22.12 echo|read is clean (builtin pipe, no exec). `echo | read` is the
# lastpipe inline-pipe: the read builtin consumes the pipe and sets a PARENT
# variable, in-process (no fork).
run_sash 'echo "clean" | readline x; echo $x'
[[ $rc = 0 && $out = "clean" && -z $err ]]; chk 22.12

# 22.13 printf|read is tainted (printf execs — an external producer)
run_sash '/usr/bin/printf "x\n" | read arr
echo ${arr[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 22.13

# 22.14 @a command-arg splat → one verbatim arg per element (DD#20/#22).
# DD#69: an external splat needs a disposition even for a literal array;
# `@{a:?…}` requires non-empty (never fires here). /usr/bin/printf runs as a
# real external in both backends.
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

# 22.18 empty array → ZERO args via the `@{e:~}` elide disposition (cat reads
# stdin, not an empty-name file). (`@{e:-}` now yields one EMPTY arg — bash
# `"${a[@]:-}"` parity, DD#69/#84 2026-09-04 — so `:~` is the drop form.)
run_sash 'array e=(); /usr/bin/printf STDIN | cat @{e:~}'
[[ $rc = 0 && $out = "STDIN" && -z $err ]]; chk 22.18

# 22.19 splat in a pipeline stage
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
run_sash 'array a=(*); : @a; echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 20.4

# 20.5 for-in glob: 0 matches → loop body never runs (DD#20/#69).
run_sash 'for f in *.nomatchZZZ; do echo "$f"; done; echo done'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk 20.5

# 20.6 for-in glob: iterates per matched entry, no word splitting.
#      DD#73: the element is a normalized absolute path `<cwd>/only.www`.
run_sash '/usr/bin/touch only.www; for f in *.www; do echo "[$f]"; done'
cwd=$(pwd -P)
[[ $rc = 0 && $out = "[$cwd/only.www]" && -z $err ]]; chk 20.6

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

# 22.20 `readc` was removed (dropped in favor of NUL-splitting + read; see
# sash-getopt) — it is now an unknown command
run_sash 'echo x | readc a'
[[ $rc != 0 && -z $out && -n $err ]]; chk 22.20

# ── read: unified flag parsing + `-d ''` NUL (sash-getopt idiom) ──
# One shared classifier (ir::classify_read_flags) drives every read path.

# 22.21 `-d ''` (empty delimiter) = NUL: `read -a arr -d ''` splits
# NUL-separated fields into an array (bash's read -d '' convention). This is
# the sash-getopt consumer: `sash-getopt … | read -a arr -d ''`.
run_sash 'printf "a\0b c\0d" > f22_21; read arr < f22_21; attest x in arr [[ -n $x ]]; echo "$(len arr):${arr[1]}"; rm -f f22_21'
[[ $rc = 0 && $out = "3:b c" && -z $err ]]; chk 22.21

# 22.22 a `-d` delimiter VALUE (`:`) is not mistaken for a target variable
# (the shared classifier skips flag values — previously a dead-var false
# positive on `:`). `-d :` reads one record up to the first `:`.
run_sash 'printf "a:b" | read -d : x; attest x [[ -n $x ]]; echo "[$x]"'
[[ $rc != 0 && -z $out && $err == *SB5057* ]]; chk 22.22

# 22.23 an unknown read flag is rejected consistently in BOTH the standalone
# and lastpipe paths (the two parsers are unified) — `-z` is not a read flag
run_sash 'echo hi | read -z x; echo $x'
[[ $rc != 0 && -z $out && $err == *SB5057* ]]; chk 22.23

# 22.24 same rejection standalone (contract with cov.20)
run_sash 'read -z x < /etc/hostname'
[[ $rc != 0 && -z $out && $err == *SB5057* ]]; chk 22.24

# 22.25 `-r` (raw) is accepted as an ignored no-op — sash reads are ALWAYS
# raw (no backslash processing), so `-r` requests the default. bash scripts
# use `read -r` ubiquitously; a hand-written `read -r x` must not be an error.
run_sash 'echo hi | read -r x; echo "[$x]"'
[[ $rc != 0 && -z $out && $err == *SB5057* ]]; chk 22.25

# 22.26 `-d` uses TERMINATOR semantics: a trailing delimiter does NOT create a
# trailing empty element (bash `mapfile -d ''` / `find -print0` / `xargs -0`).
# NUL-terminated input (the sash-getopt idiom) yields only the real fields, so
# every element is non-empty and a attest-all `[[ -n $x ]]` succeeds.
run_sash 'printf "x\0y\0" | read arr; attest e in arr [[ -n $e ]]; echo "$(len arr)"'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk 22.26

# 22.27 interior empty records are KEPT under terminator semantics; only the
# single trailing empty from a final delimiter is dropped (`a::b:` → a,"",b).
run_sash 'array arr=(); printf "a::b:" | read -a arr -d :; attest e in arr [[ $e = $e ]]; echo "$(len arr):[${arr[1]}]"'
[[ $rc != 0 && -z $out && $err == *SB5057* ]]; chk 22.27

# ── read -a / builtin-echo elide bug fixes (DG 2026-09-04, journal 244) ──
# 22.28 a FRESH `read -a` target inside a called function no longer segfaults.
# The function is lowered twice (dead template + inline); the autodeclared array
# table registration used to leak from the template into the inline, so the
# inline skipped the table `Assign` and the runtime read filled a NULL table.
run_sash 'fn f { echo hi | read norm; /bin/echo "$(len norm):${norm[0]}" }; f'
[[ $rc = 0 && $out = "1:hi" && -z $err ]]; chk 22.28

# 22.29 builtin `echo` with an elide arg (`${x:~}`/`${x:!~}`) — an empty value
# contributes NOTHING (no value, no separator), matching the external argv
# builder. Previously ICE'd (native) / double-spaced (c).
run_sash 'fn f { x=""; echo a $x:~ b }; f'
[[ $rc = 0 && $out = "a b" && -z $err ]]; chk 22.29

# 22.30 a non-empty elide arg is written normally
run_sash 'fn f { x=hi; echo a $x:~ b }; f'
[[ $rc = 0 && $out = "a hi b" && -z $err ]]; chk 22.30

# 22.31 the reported line (DD#99: `echo -n` is now `write`)
# of a `:!`-trusted value piped into a fresh array read) now works end-to-end.
run_sash 'fn f { x=hi; write $x:! | read norm; /bin/echo "$(len norm):${norm[0]}" }; f'
[[ $rc = 0 && $out = "1:hi" && -z $err ]]; chk 22.31



# ── DD#87: read -a elements must survive the execve boundary ──────────────
# `sash_split_into_array` stored each element as a BORROWED (ptr,len) slice
# into the source buffer, with no NUL at its own length. Length-based
# consumers (len/join/subscript/builtin echo) were correct, but an EXTERNAL
# command reads argv as C strings, so it read past the element to the end of
# the buffer — `read -a x -d:` on "a:b:c" passed "a:b:c" as element 0.
# Only NUL-delimited input happened to work (the buffer already had NULs),
# which is exactly the case spec 22.26 covered.

# 22.40 an element passed to an EXTERNAL command is the element, not a suffix
run_sash '/usr/bin/printf "a\0b\0c" | read x
attest e in x [[ $e = $e ]]
/usr/bin/printf "[%s]" ${x[0]}'
[[ $rc = 0 && $out = "[a]" && -z $err ]]; chk 22.40

# 22.41 …for every element, via a for-in splat
run_sash '/usr/bin/printf "a\0b\0c" | read x
attest e in x [[ $e = $e ]]
for v in @x { /usr/bin/printf "[%s]" $v }'
[[ $rc = 0 && $out = "[a][b][c]" && -z $err ]]; chk 22.41

# 22.42 whitespace-split elements likewise
run_sash '/usr/bin/printf "a\0b\0c" | read x
attest e in x [[ $e = $e ]]
/usr/bin/printf "[%s]" ${x[1]}'
[[ $rc = 0 && $out = "[b]" && -z $err ]]; chk 22.42

# 22.43 the length-based consumers stay correct (no regression from the copy)
run_sash '/usr/bin/printf "a\0b\0c" | read x
attest e in x [[ $e = $e ]]
echo "$(len x)|$(join x)|${x[2]}"'
[[ $rc = 0 && $out = "3|a b c|c" && -z $err ]]; chk 22.43


# ════════ DD#99 stage 1: `readline` — one line into one scalar ════════
# `readline v` is the line-terminated scalar read. It is today's single-target
# `read v` under its own name, so that in stage 3 `read` can become the
# array-only, NUL-split form without the two meanings colliding.

# rl.1 reads one line into a scalar
run_sash 'echo "hello world" | readline v
echo "[$v]"'
[[ $rc = 0 && $out = "[hello world]" && -z $err ]]; chk rl.1

# rl.2 stops at the newline — the second line is not consumed
run_sash 'printf "one\ntwo\n" | readline v
attest v p?
echo "[$v]"'
[[ $rc = 0 && $out = "[one]" && -z $err ]]; chk rl.2

# rl.3 does NOT split on whitespace: the whole line is one value (DD#22 — the
# implicit whitespace split is exactly what DD#99 removes)
run_sash 'echo "a b c d" | readline v
echo "[$v]"'
[[ $rc = 0 && $out = "[a b c d]" && -z $err ]]; chk rl.3

# rl.4 auto-declares its target (no prior declaration needed)
run_sash 'echo hi | readline fresh
echo "[$fresh]"'
[[ $rc = 0 && $out = "[hi]" && -z $err ]]; chk rl.4

# rl.5 the target is tainted, like every read target (DD#24)
run_sash '/bin/cat /etc/hostname | readline v
echo "[$v]"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk rl.5

# rl.6 …and attest admits it
run_sash '/bin/cat /etc/hostname | readline v
attest v p?
echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk rl.6

# rl.7 works as a `while` condition, one line per iteration.
# Both backends. This was native-only because the c emitter wrote the EOF
# result to __exit_status while the loop condition branches on __test_result,
# so the condition read a stale value and the loop never ended.
run_sash 'printf "a\nb\n" | while readline v { attest v p?; echo "[$v]" }'
[[ $rc = 0 && $out = "[a]
[b]" && -z $err ]]; chk rl.7

# rl.8 takes no flags — it is a distinct construct, not a flag surface
run_sash 'echo hi | readline -r v
echo "$v"'
[[ $rc != 0 && -z $out && $err == *SB5057* ]]; chk rl.8

# rl.9 takes exactly ONE target — the multi-target split is `read` + `map`
run_sash 'echo "a b" | readline x y
echo "$x$y"'
[[ $rc != 0 && -z $out && $err == *SB5057* ]]; chk rl.9

# rl.10 …and at least one
run_sash 'echo hi | readline'
[[ $rc != 0 && $err == *SB5057* ]]; chk rl.10


# ════════ DD#99 stage 2: `read` — NUL-separated fields into an array ════════
# Ships under a temporary name so the existing `read` keeps working until the
# tree has migrated (DG: "call it read instead of read ... then replace").
# NUL is the only separator; the target is always an array, auto-declared.

# nr.1 splits a NUL-separated stream into an array
run_sash '/usr/bin/printf "a\0b\0c" | read f
attest e in f [[ $e = $e ]]
echo "$(len f):${f[0]}:${f[2]}"'
[[ $rc = 0 && $out = "3:a:c" && -z $err ]]; chk nr.1

# nr.2 does NOT split on whitespace — NUL is the only separator (DD#22)
run_sash 'echo "a b c" | read f
attest e in f [[ $e = $e ]]
echo "$(len f):${f[0]}"'
[[ $rc = 0 && $out = "1:a b c" && -z $err ]]; chk nr.2

# nr.3 …nor on newlines
run_sash '/usr/bin/printf "x\ny\n" | read f
attest e in f [[ $e = $e ]]
echo "$(len f)"'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk nr.3

# nr.4 auto-declares its target (no prior `array f=()`)
run_sash '/usr/bin/printf "p\0q" | read fresh
attest e in fresh [[ $e = $e ]]
echo "$(len fresh)"'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk nr.4

# nr.5 the whole pipeline round-trip DD#99 is built on: convert a separator to
# NUL in-process, then split on it — no external `tr`, no flags
run_sash 'sep=$'"'"'\x00'"'"'
s=a:b:c
s =~ s/:/$sep/g
echo $s | read f
attest e in f [[ $e = $e ]]
echo "$(len f):${f[1]}"'
[[ $rc = 0 && $out = "3:b" && -z $err ]]; chk nr.5

# nr.6 targets are tainted like every read target (DD#24)
run_sash '/bin/cat /etc/hostname | read f
echo "${f[0]}"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk nr.6

# nr.7 takes no flags
run_sash 'echo hi | read -a f
echo "${f[0]}"'
[[ $rc != 0 && -z $out && $err == *SB5057* ]]; chk nr.7

# nr.8 takes exactly one target
run_sash 'echo hi | read f g
echo "${f[0]}"'
[[ $rc != 0 && -z $out && $err == *SB5057* ]]; chk nr.8


# ── stage 2b: `read $SRC ARRAY` — split a scalar (DD#99's "split") ──
# DG's observation that a `split` op is just a two-argument `read`: value in
# (sigil), reference out (bare name).

# nr.9 splits a NUL-bearing scalar into an array, no pipeline, no fork
run_sash 'sep=$'"'"'\x00'"'"'
s=a:b:c
s =~ s/:/$sep/g
read $s f
echo "$(len f):${f[0]}:${f[2]}"'
[[ $rc = 0 && $out = "3:a:c" && -z $err ]]; chk nr.9

# nr.10 a scalar with no NUL is a single element
run_sash 's="a b c"
read $s f
echo "$(len f):${f[0]}"'
[[ $rc = 0 && $out = "1:a b c" && -z $err ]]; chk nr.10

# nr.11 auto-declares the target
run_sash 'sep=$'"'"'\x00'"'"'
s=p:q
s =~ s/:/$sep/g
read $s brandnew
attest e in brandnew [[ $e = $e ]]
echo "$(len brandnew)"'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk nr.11

# nr.12 taint flows from the source scalar to the array (DD#24)
run_sash 's=$(/bin/cat /etc/hostname)
read $s f
echo "${f[0]}"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk nr.12

# nr.13 …and a clean source stays clean
run_sash 's="x y"
read $s f
echo "$(len f)"'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk nr.13

# nr.14 the second argument is a binding, so it takes no sigil (DD#99)
run_sash 's="a"
read $s @f
echo "$(len f)"'
[[ $rc != 0 && -z $out && -n $err ]]; chk nr.14


# ── DD#24 x DD#99: a two-arg `read`'s target inherits the SOURCE ────────
# The one-arg form reads stdin, so its target is tainted by definition. The
# two-arg form splits a value that is already in the program: the source is an
# ordinary READ, checked like any other, and DD#24's "no propagation" then
# makes the result clean — exactly as `x=$y` does. Tainting the target
# regardless (what it used to do) made a clean split unusable and pushed the
# author toward a needless `attest`, which is laundering by habit.

# nr.15 a clean source yields a CLEAN array — no attest required
run_sash 's="a b c"
read $s f
echo "$(len f):${f[0]}"'
[[ $rc = 0 && $out = "1:a b c" && -z $err ]]; chk nr.15

# nr.16 …and a TAINTED source is rejected at the source READ, not laundered
# into the target. DD#24: there is no taint flow inside the program.
run_sash 'k=$(getenv HOME)
read $k f
echo "${f[0]}"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk nr.16

# nr.17 the one-arg (stdin) form still taints its target — nr.6 covers the
# rejection; this pins that the two-arg fix did not disarm the one-arg case
run_sash '/bin/cat /etc/hostname | read f
echo "${f[0]}"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk nr.17

# nr.18 an ELIDING disposition in the source slot is rejected, not an ICE.
# `read ${u:~} f` used to panic in codegen. Eliding here would not shorten
# a list — it would drop the source and silently re-read STDIN instead, a
# different input entirely. DD#91's elidable positions are variadic ones.
run_sash 'u=""
read ${u:~} f
echo "${f[0]}"'
[[ $rc != 0 && -z $out && $err == *SB5046* ]]; chk nr.18

# nr.19 …but `:!` alone is NOT an elide (DD#91 addendum) — it is the value,
# trusted — so it is legal in the source slot. This is the spelling DG
# reported; it used to be a spurious SB5020, then a misdirected SB5046.
run_sash 'u=""
read ${u:!} f
echo "$(len f)"'
[[ $rc = 0 && -z $err ]]; chk nr.19

# nr.20 the ELIDING combination is still rejected there, for the reason nr.18
# gives: eliding the source would silently re-read stdin instead
run_sash 'u=""
read ${u:!~} f
echo "${f[0]}"'
[[ $rc != 0 && -z $out && $err == *SB5046* ]]; chk nr.20

# ---------------------------------------------------------------------------
# DD#64, second half — `\<newline>` is a line continuation in an *unquoted*
# word too. `skip_whitespace` and `scan_dq_escape` had it; `scan_word_chars`
# did not, so a word split across lines silently became two statements.
# ---------------------------------------------------------------------------

# 64.9 a word split mid-way across a line continuation is one word
run_sash '/bin/echo a\
b'
[[ $rc = 0 && $out = "ab" && -z $err ]]; chk 64.9

# 64.10 the silent case: both halves happened to be valid on their own, so
# this compiled clean and printed two lines instead of one.
run_sash '/bin/echo start\
/bin/echo second'
[[ $rc = 0 && $out = "start/bin/echo second" && -z $err ]]; chk 64.10

# 64.11 a continuation *between* words still separates them (skip_whitespace)
run_sash '/bin/echo a \
b'
[[ $rc = 0 && $out = "a b" && -z $err ]]; chk 64.11

# 64.12 a continuation inside `"…"` still joins (scan_dq_escape, unchanged)
run_sash '/bin/echo "a\
b"'
[[ $rc = 0 && $out = "ab" && -z $err ]]; chk 64.12

# 64.13 `'"'"'…'"'"' is verbatim — no continuation, the newline is data
run_sash "/bin/echo 'a\\
b'"
[[ $rc = 0 && $out = $'a\\\nb' && -z $err ]]; chk 64.13

# 64.14 DD#64's deliberate divergence survives: a `\` that is NOT at
# end-of-line is a verbatim byte in an unquoted word, so `a\b` is `a\b`
# (bash prints `ab`). Only end-of-line is structural.
run_sash '/bin/echo a\b'
[[ $rc = 0 && $out = 'a\b' && -z $err ]]; chk 64.14

# 64.15 a trailing `\` with nothing after it at all is still a continuation —
# it just has no newline to join to, so it vanishes and the word ends. bash
# discards it the same way (`echo a\` prints `a`). This spec first asserted
# `a\`, pinning a divergence nobody had checked against bash; see journal 329.
run_sash '/bin/echo a\'
[[ $rc = 0 && $out = "a" && -z $err ]]; chk 64.15

# 64.16 the same trailing `\` reached through `skip_whitespace` instead of
# `scan_word_chars` — a blank in front of it puts the scanner in the other
# path, which had the identical hole.
run_sash '/bin/echo a \'
[[ $rc = 0 && $out = "a" && -z $err ]]; chk 64.16

# 64.17 a word ending in `\` at end-of-input: the backslash goes, the word
# stays whole.
run_sash '/bin/echo ab\'
[[ $rc = 0 && $out = "ab" && -z $err ]]; chk 64.17

# 64.18 quoting still protects it — `'"'"'a\'"'"' is two bytes, continuation or not.
run_sash "/bin/echo 'a\\'"
[[ $rc = 0 && $out = 'a\' && -z $err ]]; chk 64.18

set -e
tally
