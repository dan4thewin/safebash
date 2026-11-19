#!/bin/bash
# spec/glob.sh — DD#73: glob expansion yields normalized ABSOLUTE paths.
#
# A glob element is `<getcwd>/name`, not the bare directory-entry name —
# for referential stability across a chdir, and to close glob-sourced
# option-injection (a `-`-leading filename becomes `/abs/-name`, which no
# external command parses as an option). Lexical (no realpath); no relative
# segment survives.
#
# NOTE: `run_sash` (native) leaves `./test_bin` in the cwd during the run,
# so these tests use suffixed patterns (`*.www`, never bare `*`) to avoid
# matching the harness binary.
set -eu
. common.sh
setup glob
plan=137
echo "1..$plan" >&3

set +e

# g.1 a glob element is an absolute path `<cwd>/name`, not the bare name.
run_sash '/usr/bin/touch g1.www; for f in *.www; do echo "$f"; done'
cwd=$(pwd -P)
[[ $rc = 0 && $out = "$cwd/g1.www" && -z $err ]]; chk g.1

# g.2 option-injection closed: a `-`-leading filename becomes `<cwd>/-name`
#     (starts with `/`, so no external command reparses it as an option).
run_sash '/usr/bin/touch -- -rf.zz; for f in *.zz; do echo "$f"; done'
cwd=$(pwd -P)
[[ $rc = 0 && $out = "$cwd/-rf.zz" && $out = /* && -z $err ]]; chk g.2

# g.3 the array-literal form `array a=(*)` carries absolute elements too
#     (observed via for-in, which is disposition-exempt).
run_sash '/usr/bin/touch g3.rr; array a=(*.rr); for f in @a; do echo "$f"; done'
cwd=$(pwd -P)
[[ $rc = 0 && $out = "$cwd/g3.rr" && -z $err ]]; chk g.3

# g.4 referential stability: the absolute element still names the same file
#     after a chdir — the DD#73 correctness rationale. With a bare name,
#     `cat g4.tt` after `cd /` would fail.
run_sash 'echo hi > g4.tt; array a=(*.tt); cd /; /bin/cat @{a:?x}'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk g.4

# g.5 zero matches → empty (absolutization does not fabricate an entry).
run_sash 'array a=(*.nomatchQQ); /usr/bin/printf "[%s]" @{a:-none}'
[[ $rc = 0 && $out = "[none]" && -z $err ]]; chk g.5

# g.6 every element of a multi-match glob is absolute (order-independent).
run_sash '/usr/bin/touch g6a.pp g6b.pp; for f in *.pp; do echo "$f"; done'
cwd=$(pwd -P)
[[ $rc = 0 && $out == *"$cwd/g6a.pp"* && $out == *"$cwd/g6b.pp"* ]]; chk g.6

# g.7 a glob array's `len` agrees with its contents. The glob literal lowers
# to a `glob` value rather than an array literal, so it used to skip
# collection registration entirely — `join` saw every element while `len`
# read the table as a string and answered 0, with the `array` keyword present.
run_sash '/usr/bin/touch g7a.g7x g7b.g7x g7c.g7x; array a=(*.g7x); echo $(len a)'
[[ $rc = 0 && $out = "3" && -z $err ]]; chk g.7

# g.8 `len` and `join` agree about the same glob array
run_sash '/usr/bin/touch g8a.g8x g8b.g8x; array a=(*.g8x); echo $(len a); echo $(join a)'
[[ $rc = 0 && $out == "2"* && $out == *g8a.g8x* && $out == *g8b.g8x* && -z $err ]]; chk g.8

# g.9 a zero-match glob is a real EMPTY array — len 0, and still a collection
run_sash 'array a=(*.nomatchZZ); echo $(len a)'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk g.9

# ── SB5075: the `arr=(GLOB =~ 'RE')` regex-filter shorthand is retired (DD#108)
# It was a list literal whose meaning depended on its own contents: `(* =~ re)`
# filtered, `(x =~ y)` was a plain 3-element array, and `(* =~ a b)` silently
# became a 4-element one. Quoting could not disambiguate it either — every
# quoting of the operator desugared identically — so making quotes the escape
# would have meant letting quotes change a list's arity. The loop replaces it
# and names what it matches.

# g.10 the shorthand is rejected rather than silently becoming a literal array
run_sash "array a=(* =~ '\\.ff\$')
echo \$(len a)"
[[ $rc != 0 && -z $out && $err == *SB5075* ]]; chk g.10

# g.11 …including the form that was the original bug: a non-bare glob used to
# fall through to a 3-element array of un-expanded literal text, no diagnostic
run_sash "array a=(*.ff =~ '\\.ff\$')
echo \$(len a)"
[[ $rc != 0 && -z $out && $err == *SB5075* ]]; chk g.11

# g.12 the rule is uniform — a bare `=~` anywhere in an array literal is out,
# not just when element 0 happens to hold a glob metacharacter
run_sash 'array a=(x =~ y)
echo $(len a)'
[[ $rc != 0 && -z $out && $err == *SB5075* ]]; chk g.12

# g.13 a QUOTED `=~` is an ordinary string element and stays legal — this is
# the escape the diagnostic names, and the reason the rule can be uniform
run_sash "array a=(x '=~' y)
echo \$(len a)"
[[ $rc = 0 && $out = "3" && -z $err ]]; chk g.13

# g.14 the replacement the message advises actually compiles and filters
run_sash "/usr/bin/touch g14a.ff g14b.ff g14c.gg
array a
for f in * { if [[ \$f =~ '\\.ff\$' ]] { a += (\$f) } }
echo \$(len a)"
[[ $rc = 0 && $out = "2" && -z $err ]]; chk g.14

# forglob.1-forglob.3: a `for` over a glob keeps its expansion in a compiler
# temporary that is reserved like every other (DD#125), so it can neither
# clobber a source name that happens to share its spelling nor be shared by a
# nested loop (batch-531 F13). It used to be the fixed name `__forglob`. The
# expansion's order is not asserted here.

# forglob.1 a source global named `__forglob` survives the loop (it was `[]`)
run_sash '/usr/bin/touch fga.fgx fgb.fgx
__forglob=keep
for f in *.fgx {
  echo "$f"
}
echo "[$__forglob]"'
[[ $rc = 0 && -z $err && $out == *'/fga.fgx'* && $out == *'/fgb.fgx'* \
   && $out == *$'\n[keep]' ]]; chk forglob.1

# forglob.2 a nested glob loop no longer overwrites the outer loop's list; the
# outer loop ran once
run_sash '/usr/bin/touch fgo1.fgo fgo2.fgo fgi.fgi
for a in *.fgo {
  for b in *.fgi {
    echo "$a $b"
  }
}'
[[ $rc = 0 && -z $err && $out == *'/fgo1.fgo '*'/fgi.fgi'* \
   && $out == *'/fgo2.fgo '*'/fgi.fgi'* \
   && $(printf '%s\n' "$out" | wc -l) = 2 ]]; chk forglob.2

# forglob.3 nor is a source const of that name a write target (a false SB4044)
run_sash '/usr/bin/touch fgc.fgc
const __forglob=keep
for f in *.fgc {
  echo "$f"
}
echo "[$__forglob]"'
[[ $rc = 0 && -z $err && $out == *'/fgc.fgc'$'\n[keep]' ]]; chk forglob.3

# DD131: each pattern contributes its own sorted batch, including duplicates.
mkdir glob-data
touch glob-data/b.sel glob-data/a.sel glob-data/.hidden.sel \
    glob-data/-option.sel 'glob-data/with space.sel' 'glob-data/literal?[]'
base=$cwd/glob-data
selected="$base/-option.sel,$base/a.sel,$base/b.sel,$base/with space.sel"

run_sash "glob a 'glob-data/*.sel' 'glob-data/.*' 'glob-data/a.sel' 'glob-data/literal?[]'
echo \"\$(join a ,)\""
[[ $rc = 0 && $out = "$selected,$base/.hidden.sel,$base/a.sel,$base/literal?[]" && -z $err && -z $rerr ]]; chk globsel.1

run_sash 'array a=(old)
if glob a "glob-data/*.missing" "" { echo "ok:$(join a)" } else { echo wrong }
if glob empty "glob-data/*.missing" { echo "$(len empty)" } else { echo wrong }'
[[ $rc = 0 && $out = $'ok:old\n0' && -z $err && -z $rerr ]]; chk globsel.2

run_sash 'str pattern=glob-data/a.sel
fn change { pattern=glob-data/b.sel; echo glob-data/b.sel }
glob a "$pattern" "$(change)" "$pattern"
echo "$(join a ,)"'
[[ $rc = 0 && $out = "$base/a.sel,$base/b.sel,$base/b.sel" && -z $err && -z $rerr ]]; chk globsel.3

run_sash 'glob a "./glob-data/../glob-data//a.sel" "/dev/../dev//null" "glob-data/." "glob-data/.."
echo "$(join a ,)"'
[[ $rc = 0 && $out = "$base/a.sel,/dev/null" && -z $err && -z $rerr ]]; chk globsel.4

run_sash 'fn fill { glob A "glob-data/a.sel" }; fn relay { fill =A }
array a=(old); relay =a; echo "$(join a ,)"'
[[ $rc = 0 && $out = "old,$base/a.sel" && -z $err && -z $rerr ]]; chk globsel.5

run_sash 'for pattern in "glob-data/a.sel" "glob-missing/*" {
    if glob _batch "$pattern" { echo "ok:$(len _batch)" } else { echo "bad:$(len _batch)" }
}'
[[ $rc = 0 && $out = $'ok:1\nbad:0' && -z $err && -z $rerr ]]; chk globsel.6

run_sash 'array a=(); a[10]=old; glob a "glob-data/a.sel"
echo "${a[10]}:${a[11]}:$(len a)"'
[[ $rc = 0 && $out = "old:$base/a.sel:2" && -z $err && -z $rerr ]]; chk globsel.7

run_sash 'array a=(old)
if glob a "glob-data/a.sel" "glob-data/a.sel/*" { echo wrong } else { echo "$(join a)" }'
[[ $rc = 0 && $out = old && -z $err && -z $rerr ]]; chk globsel.8

run_sash 'array a=(old); - glob a "glob-missing/*"; echo "$(join a)"'
[[ $rc = 0 && $out = old && -z $err && -z $rerr ]]; chk globsel.9

run_sash "array a=(old)
for pattern in '*a*' 'glob*/a' \$'glob-data/a\\x00.sel' {
    if glob a \"\$pattern\" { echo wrong } else { echo \"\$(join a)\" }
}"
[[ $rc = 0 && $out = $'old\nold\nold' && -z $err && -z $rerr ]]; chk globsel.10

run_sash 'array a=(old)
ERR { - echo "err:$(join a)" }
glob a "glob-data/a.sel" "glob-missing/*"
echo wrong'
[[ $rc = 1 && $out = err:old && -z $err && $rerr == *"glob: open"* && $rerr == *glob-missing* ]]; chk globsel.11

run_sash 'fn bad -> int { /bin/false; return 7 }
array a=(old)
ERR { - echo "err:$(join a)" }
- glob a "$(bad)"
echo wrong'
[[ $rc = 1 && $out = err:old && -z $err && $rerr == *"/bin/false: command failed"* ]]; chk globsel.12

run_sash 'str pattern=$(- getenv GLOB_PATTERN); glob a "$pattern"; echo "$(join a)"'
[[ $rc != 0 && $err == *SB5020* ]]; chk globsel.13

run_sash 'argv 1 a; glob a "glob-data/a.sel"; echo "$(join a)"' unsafe
[[ $rc != 0 && $err == *SB5020* ]]; chk globsel.14

run_sash 'str a=old; glob a "glob-data/a.sel"; echo "$a"'
[[ $rc != 0 && $err == *SB4030* ]]; chk globsel.15

run_sash 'map a=([old]=value); glob a "glob-data/a.sel"; echo "$(len a)"'
[[ $rc != 0 && $err == *SB4030* ]]; chk globsel.16

run_sash 'int array a=(7); glob a "glob-data/a.sel"; echo "$(len a)"'
[[ $rc != 0 && $err == *SB4030* ]]; chk globsel.17

run_sash 'glob a; echo "$(len a)"'
[[ $rc != 0 && $err == *SB4045* && $err == *"at least one pattern"* ]]; chk globsel.18

run_sash 'str target=a; glob "$target" "glob-data/a.sel"'
[[ $rc != 0 && $err == *SB5063* ]]; chk globsel.19

run_sash 'int pattern=7; glob a $pattern; echo "$(len a)"'
[[ $rc != 0 && $err == *SB4030* ]]; chk globsel.20

run_sash 'glob a "glob-data/a.sel" > glob-output; echo "$(len a)"'
[[ $rc != 0 && $err == *SB4046* ]]; chk globsel.21

run_sash 'glob a "glob-data/a.sel" | /bin/cat; echo "$(len a)"'
[[ $rc != 0 && $err == *SB4022* ]]; chk globsel.22

run_sash 'glob a "glob-data/a.sel" &; echo "$(len a)"'
[[ $rc != 0 && $err == *SB4024* ]]; chk globsel.23

run_sash 'glob a "glob*/*.sel"; echo "$(len a)"'
[[ $rc != 0 && $err == *SB5006* && $err == *"final filename component"* ]]; chk globsel.24

run_sash 'glob a "*a*"; echo "$(len a)"'
[[ $rc != 0 && $err == *SB5006* && $err == *"at most one star"* ]]; chk globsel.25

run_sash "glob a \$'glob-data/a\\x00.sel'; echo \"\$(len a)\""
[[ $rc != 0 && $err == *SB5006* && $err == *"cannot contain NUL"* ]]; chk globsel.26

# Fail after a real entry buffer, not at the first directory read.
run_sash_under 'array a=(old)
if glob a "glob-data/*.sel" { echo wrong } else { echo "$(join a)" }' \
    strace -qq -e trace=open,getdents64,close \
    -e inject=getdents64:error=EIO:when=2 -o glob-read.trace
opened=$(sed -nE '/"glob-data"/s/.* = ([0-9]+)$/\1/p' glob-read.trace)
[[ $rc = 0 && $out = old && -z $err && -z $rerr && -n $opened ]] \
    && grep -qE 'getdents64\(.* = [1-9][0-9]*$' glob-read.trace \
    && grep -qE 'getdents64\(.*EIO.*INJECTED' glob-read.trace \
    && grep -qE "^close\\($opened\\) += 0$" glob-read.trace; chk globsel.27

run_sash_under 'array a=(old)
if glob a "glob-data/*.sel" { echo wrong } else { echo "$(join a)" }' \
    strace -qq -e trace=open,getdents64 -e inject=open:error=EACCES:when=1 -o glob-open.trace
[[ $rc = 0 && $out = old && -z $err && -z $rerr ]] \
    && grep -qE 'open\(.*"glob-data".*EACCES.*INJECTED' glob-open.trace \
    && ! grep -q getdents64 glob-open.trace; chk globsel.28

# Absolute patterns need no cwd; a failed relative attempt releases its fd.
run_sash_under "array a=(old)
if glob a 'glob-data/a.sel' { echo wrong } else { echo \"\$(join a)\" }
glob a '$base/a.sel'
echo \"\$(join a ,)\"" \
    strace -qq -e trace=open,getcwd,close -e inject=getcwd:error=ENOENT:when=1 -o glob-cwd.trace
opened=$(sed -nE '/"glob-data"/s/.* = ([0-9]+)$/\1/p' glob-cwd.trace)
[[ $rc = 0 && $out = "old"$'\n'"old,$base/a.sel" && -z $err && -z $rerr \
    && $(grep -c getcwd glob-cwd.trace) = 1 && -n $opened ]] \
    && grep -qE 'getcwd\(.*ENOENT.*INJECTED' glob-cwd.trace \
    && grep -qE "^close\\($opened\\) += 0$" glob-cwd.trace; chk globsel.29

run_sash_under 'array a=(old); int i=0
while (( i < 40 )) {
    - glob a "glob-data/*.sel"
    (( i++ ))
}
echo "$(join a)"' \
    prlimit --nofile=8:8 strace -qq -e trace=open,getdents64,close \
    -e inject=getdents64:error=EIO -o glob-repeat.trace
[[ $rc = 0 && $out = old && -z $err && -z $rerr \
    && $(grep -c 'EIO.*INJECTED' glob-repeat.trace) = 40 ]] \
    && ! grep -q EMFILE glob-repeat.trace; chk globsel.30

run_sash 'str stem=a
fn change { stem=b; echo .sel }
glob a "glob-data/$stem$(change)"
echo "$(join a)"'
[[ $rc = 0 && $out = "$base/a.sel" && -z $err && -z $rerr ]]; chk globsel.31

run_sash 'fn fill { glob args "glob-data/a.sel" }; fill'
[[ $rc != 0 && $err == *SB5052* ]]; chk globsel.32

run_sash 'glob _a "glob-data/a.sel"; glob _a "glob-data/b.sel"; echo "$(len _a)"'
[[ $rc != 0 && $err == *SB5103* && $err == *'`_a`'* ]]; chk globsel.33

run_sash 'fn fill { glob A "glob-data/a.sel" }; fn relay { fill =A }
array _a=(old); relay =_a; echo "$(len _a)"'
[[ $rc != 0 && $err == *SB5103* && $err == *'`_a`'* ]]; chk globsel.34

run_sash 'fn pattern { echo "${_a[0]}" }; glob _a "$(pattern)"; echo "$(len _a)"'
[[ $rc != 0 && $err == *SB5104* && $err == *'`_a`'* ]]; chk globsel.35

run_sash 'if glob _ "glob-data/a.sel" { echo done } else { echo wrong }'
[[ $rc = 0 && $out = done && -z $err && -z $rerr ]]; chk globsel.36

run_sash 'glob; echo wrong'
[[ $rc != 0 && $err == *SB4045* && $err == *"at least one pattern"* ]]; chk globsel.37

run_sash "glob \$'a' 'glob-data/a.sel'; echo \"\$(len a)\""
[[ $rc != 0 && $err == *SB5063* ]]; chk globsel.38

run_sash 'array a=(); a[9223372036854775806]=old
ERR { - echo "${a[9223372036854775806]}:${a[9223372036854775807]}:$(len a)" }
if glob a "glob-data/a.sel" "glob-data/b.sel" { echo wrong } else { echo caught }'
[[ $rc = 2 && $out = old::1 && -z $err && $rerr == *"arithmetic overflow"* ]]; chk globsel.39

mkdir glob-heap
printf -v suffix 'x%.0s' {1..100}
for n in {1..160}; do touch "glob-heap/$n-$suffix"; done
SASH_CFLAGS=--heap-size=16K run_sash_under 'array a=(old)
ERR { - echo "${a[0]}" }
if glob a "glob-heap/*" { echo wrong } else { echo caught }' \
    strace -qq -e trace=getdents64 -o glob-heap.trace
[[ $rc = 1 && $out = old && -z $err && $rerr == *"out of memory"* \
    && $(grep -cE 'getdents64\(.* = [1-9][0-9]*$' glob-heap.trace) -gt 1 ]]; chk globsel.40

mkdir glob-bytes
touch glob-bytes/$'\xff' glob-bytes/$'\x80' glob-bytes/z
run_sash 'glob a "glob-bytes/*"; echo "$(join a ,)"'
[[ $rc = 0 && $out = "$cwd/glob-bytes/z,$cwd/glob-bytes/"$'\x80'",$cwd/glob-bytes/"$'\xff' \
    && -z $err && -z $rerr ]]; chk globsel.41

SASH_CFLAGS=--no-taint-cst run_sash 'argv 1 a; glob a "glob-data/a.sel"; echo "$(join a)"' unsafe
[[ $rc != 0 && $err == *tainted* ]]; chk globsel.42

run_sash "ERR { array a=(\$'a\\x00b'); glob a 'glob-data/a.sel'; - /bin/echo @{a:-empty} }; fail 7"
[[ $rc != 0 && $err == *SB5101* && $err == *NUL* ]]; chk globsel.43

touch pattern
run_sash 'fn select { str pattern=read; glob batch pattern; echo "$pattern:$(join batch)" }; select'
[[ $rc = 0 && $out = "read:$cwd/pattern" && -z $err && -z $rerr ]]; chk globsel.44

mkdir glob-fresh
touch glob-fresh/7
run_sash 'for n in 1 0 {
    glob _a "glob-fresh/$(( 7 / ${n:?number} ))"
    echo "$(len _a)"
}
ERR { - echo "err:$(len _a)" }'
[[ $rc = 2 && $out = $'1\nerr:0' && -z $err && $rerr == *"division by zero"* ]]; chk globsel.45

# Only bare literal list words expand, including in otherwise mixed lists.
run_sash "array a=(first glob-data/*.sel '*.sel' \"*.sel\" \$'\\x2a.sel' last)
echo \"\$(join a ,)\""
[[ $rc = 0 && $out = "first,$selected,*.sel,*.sel,*.sel,last" && -z $err && -z $rerr ]]; chk globsugar.1

run_sash 'str pattern="glob-data/*.sel"; array a=($pattern)
echo "$(join a)"'
[[ $rc = 0 && $out = 'glob-data/*.sel' && -z $err && -z $rerr ]]; chk globsugar.2

run_sash "array a=('**' \"**\" \$'**'); echo \"\$(join a ,)\"
for word in '**' \"**\" \$'**' { echo \"\$word\" }"
[[ $rc = 0 && $out = $'**,**,**\n**\n**\n**' && -z $err && -z $rerr ]]; chk globsugar.3

run_sash 'str suffix=sel; array a=(glob-data/*.$suffix); echo "$(len a)"'
[[ $rc != 0 && $err == *SB5029* && $err == *"glob ARRAY PATTERN"* ]]; chk globsugar.4

run_sash 'str suffix=sel; array a=(glob-data/"*".$suffix); echo "$(join a)"'
[[ $rc = 0 && $out = 'glob-data/*.sel' && -z $err && -z $rerr ]]; chk globsugar.5

run_sash 'array a=(seed); a+=(literal glob-data/*.sel @a); echo "$(join a ,)"'
[[ $rc = 0 && $out = "seed,literal,$selected,seed" && -z $err && -z $rerr ]]; chk globsugar.6

run_sash 'for word in first glob-data/a* last glob-data/.* { echo "$word" }'
[[ $rc = 0 && $out = "first"$'\n'"$base/a.sel"$'\nlast\n'"$base/.hidden.sel" && -z $err && -z $rerr ]]; chk globsugar.7

run_sash 'str text=old; array values=(before)
fn change { text=new; values=(after); echo middle }
array a=($text @values glob-data/a* "$(change)" $text @values)
echo "$(join a ,)"'
[[ $rc = 0 && $out = "old,before,$base/a.sel,middle,new,after" && -z $err && -z $rerr ]]; chk globsugar.8

run_sash '__glob=keep; __globword=held; __batch_a=mark
array a=(glob-data/a* $__glob)
echo "$__glob:$__globword:$__batch_a:$(join a ,)"'
[[ $rc = 0 && $out = "keep:held:mark:$base/a.sel,keep" && -z $err && -z $rerr ]]; chk globsugar.9

printf -v deep 'd/%.0s' {1..300}
mkdir -p "$deep"
touch "${deep}deep.long"
run_sash "cd '$deep'; array a=(*.long); echo \"\$(join a)\""
[[ $rc = 0 && $out = "$cwd/${deep}deep.long" && -z $err && -z $rerr ]]; chk globsugar.10

printf -v segment 'x%.0s' {1..240}
printf -v deep "$segment/%.0s" {1..16}
printf -v leaf 'y%.0s' {1..240}
leaf+=.long
mkdir -p "$deep"
(cd "$deep" && touch "$leaf")
run_sash "cd '$deep'; array a=(*.long); echo \"\$(join a)\""
[[ $rc = 0 && ${#out} -gt 4095 && $out = "$cwd/$deep$leaf" && -z $err && -z $rerr ]]; chk globsugar.11

run_sash 'cd /; array a=(de*); for path in @a { echo "$path" }'
[[ $rc = 0 && $'\n'"$out"$'\n' == *$'\n/dev\n'* && $out != *//* && -z $err && -z $rerr ]]; chk globsugar.12

run_sash 'array a=(old)
ERR { - echo "$(join a)" }
a=(glob-data/a* glob-missing/*)
echo wrong'
[[ $rc = 1 && $out = old && -z $err && $rerr == *"glob: open"* ]]; chk globsugar.13

run_sash 'array a=(old)
ERR { - echo "$(join a)" }
a+=(before glob-data/a* glob-missing/*)
echo wrong'
[[ $rc = 1 && $out = old && -z $err && $rerr == *"glob: open"* ]]; chk globsugar.14

run_sash "glob a \$'glob-data/\\x2a.sel'; echo \"\$(join a ,)\""
[[ $rc = 0 && $out = "$selected" && -z $err && -z $rerr ]]; chk globsugar.15

run_sash 'str text=old
fn change { text=new; echo suffix }
array a=("$text$(change)" glob-data/a*)
echo "$(join a ,)"'
[[ $rc = 0 && $out = "oldsuffix,$base/a.sel" && -z $err && -z $rerr ]]; chk globsugar.16

SASH_CFLAGS=--heap-size=16K run_sash 'array a=(old)
int i=0
while (( i < 500 )) { a+=(); (( i++ )) }
a+=(@a)
echo "$(join a ,)"'
[[ $rc = 0 && $out = old,old && -z $err && -z $rerr ]]; chk globsugar.17

# Selector pattern tails are lists, not implicitly joined scalar strings.
run_sash 'array patterns=(glob-data/a.sel "glob-data/with space.sel")
glob a @patterns
echo "$(join a ,)"'
[[ $rc = 0 && $out = "$base/a.sel,$base/with space.sel" && -z $err && -z $rerr ]]; chk globlist.1

run_sash 'array patterns=(glob-data/a.sel glob-data/b.sel)
fn change { patterns=(glob-data/.hidden.sel); echo glob-data/-option.sel }
glob a "glob-data/literal?[]" @patterns "$(change)" @patterns
echo "$(join a ,)"'
[[ $rc = 0 && $out = "$base/literal?[],$base/a.sel,$base/b.sel,$base/-option.sel,$base/.hidden.sel" \
    && -z $err && -z $rerr ]]; chk globlist.2

run_sash_under 'array patterns; str empty=""; array a=(old)
if glob a @patterns ${empty:~} { echo "$(join a)" } else { echo wrong }
if glob _batch @patterns { echo "$(len _batch)" } else { echo wrong }' \
    strace -qq -e trace=open,getdents64,getcwd -o glob-empty-list.trace
[[ $rc = 0 && $out = $'old\n0' && -z $err && -z $rerr \
    && ! -s glob-empty-list.trace ]]; chk globlist.3

run_sash 'array empty; array patterns=(glob-data/b.sel)
glob a @{empty:-glob-data/a.sel}
glob a @{patterns:?patterns required} @{empty:~}
echo "$(join a ,)"'
[[ $rc = 0 && $out = "$base/a.sel,$base/b.sel" && -z $err && -z $rerr ]]; chk globlist.4

run_sash 'array patterns; array a=(old)
ERR { - echo "$(join a)" }
- glob a @{patterns:?patterns required}
echo wrong'
[[ $rc = 1 && $out = old && -z $err && $rerr == *"patterns required"* ]]; chk globlist.5

run_sash_under 'array patterns=(glob-data/a.sel "glob-missing/*"); array a=(old)
if glob a @patterns { echo wrong } else { echo "$(join a)" }' \
    strace -qq -e trace=open,getdents64 -o glob-list-failure.trace
[[ $rc = 0 && $out = old && -z $err && -z $rerr ]] \
    && grep -qE 'getdents64\(.* = [1-9][0-9]*$' glob-list-failure.trace \
    && grep -qE 'open\(.*"glob-missing".*ENOENT' glob-list-failure.trace; chk globlist.6

run_sash 'int array patterns=(7); glob a @patterns; echo "$(len a)"'
[[ $rc != 0 && $err == *SB4030* && $err == *"integer elements"* ]]; chk globlist.7

run_sash 'str pattern=""
fn change { pattern=glob-data/a.sel; echo glob-data/b.sel }
glob a ${pattern:~} "$(change)" ${pattern:~}
echo "$(join a ,)"'
[[ $rc = 0 && $out = "$base/b.sel,$base/a.sel" && -z $err && -z $rerr ]]; chk globlist.8

run_sash 'array patterns
glob a @{patterns:-a*b*c}; echo "$(len a)"'
[[ $rc != 0 && $err == *SB5006* && $err == *"at most one"* ]]; chk globlist.9

# rglob matches basenames without a glob-dotfile filter; batches stay ordered.
regex_selected="$base/-option.sel,$base/.hidden.sel,$base/a.sel,$base/b.sel,$base/with space.sel"
run_sash 'rglob a glob-data sel$ b "^a.*[.]sel$"
echo "$(join a ,)"'
[[ $rc = 0 && $out = "$regex_selected,$base/b.sel,$base/a.sel" && -z $err && -z $rerr ]]; chk rglob.1

ln -s glob-data rglob-link
run_sash 'rglob a "./rglob-link//." "^a[.]sel$"
rglob a "/dev/../dev//" "^null$"
echo "$(join a ,)"'
[[ $rc = 0 && $out = "$cwd/rglob-link/a.sel,/dev/null" && -z $err && -z $rerr ]]; chk rglob.2

run_sash 'array a=(old)
if rglob a glob-data glob-data { echo "$(join a)" } else { echo wrong }
if rglob empty glob-data "^absent$" { echo "$(len empty)" } else { echo wrong }'
[[ $rc = 0 && $out = $'old\n0' && -z $err && -z $rerr ]]; chk rglob.3

run_sash 'const pattern="^a[.]sel$"; attest pattern r?
rglob a glob-data "$pattern"
echo "$(join a)"'
[[ $rc = 0 && $out = "$base/a.sel" && -z $err && -z $rerr ]]; chk rglob.4

run_sash 'const pattern="^a[.]sel$"
if [[ r? $pattern ]] { echo valid }
rglob a glob-data "$pattern"; echo "$(len a)"'
[[ $rc != 0 && $err == *SB5106* ]]; chk rglob.5

run_sash 'str pattern="^a[.]sel$"; attest pattern r?
rglob a glob-data "$pattern"; echo "$(len a)"'
[[ $rc != 0 && $err == *SB5106* ]]; chk rglob.6

SASH_CFLAGS=--no-taint run_sash 'const pattern="^a[.]sel$"
rglob a glob-data "$pattern"; echo "$(len a)"'
[[ $rc != 0 && $err == *SB5106* ]]; chk rglob.7

SASH_CFLAGS=--no-taint run_sash 'const pattern="^a[.]sel$"; attest pattern r?
rglob a glob-data "${pattern:!}"; echo "$(len a)"'
[[ $rc != 0 && $err == *SB5105* ]]; chk rglob.8

run_sash 'rglob a glob-data "("; echo "$(len a)"'
[[ $rc != 0 && $err == *SB5023* ]]; chk rglob.9

run_sash 'array _patterns=("^b[.]sel$" "^a[.]sel$")
attest pattern in _patterns r?
rglob a glob-data "^a[.]sel$" @_patterns "^b[.]sel$"
echo "$(join a ,)"'
[[ $rc = 0 && $out = "$base/a.sel,$base/b.sel,$base/a.sel,$base/b.sel" \
    && -z $err && -z $rerr ]]; chk rglob.10

run_sash 'array patterns=("^a[.]sel$"); attest pattern in patterns r?
rglob a glob-data @patterns; echo "$(len a)"'
[[ $rc != 0 && $err == *SB5106* ]]; chk rglob.11

run_sash 'array _patterns=("^a[.]sel$")
rglob a glob-data @_patterns; echo "$(len a)"'
[[ $rc != 0 && $err == *SB5106* ]]; chk rglob.12

run_sash "argv 1 _patterns; attest pattern in _patterns r?
rglob a glob-data @{_patterns:-'^a[.]sel\$'}
echo \"\$(join a)\""
[[ $rc = 0 && $out = "$base/a.sel" && -z $err && -z $rerr ]]; chk rglob.13

# An empty default is one empty regex (matching every entry), not an empty list.
run_sash 'array _patterns=()
rglob a glob-data @{_patterns:-}
echo "$(join a ,)"'
[[ $rc = 0 && $out = "$base/-option.sel,$base/.hidden.sel,$base/a.sel,$base/b.sel,$base/literal?[],$base/with space.sel" \
    && -z $err && -z $rerr ]]; chk rglob.14

run_sash_under 'array patterns; array a=(old); str seen=before
fn directory -> int { seen=after; return 7 }
if rglob a "$(directory)" @patterns { echo "$seen:$(join a)" } else { echo wrong }
if rglob _batch missing-directory @{patterns:~} { echo "$(len _batch)" } else { echo wrong }' \
    strace -qq -e trace=open,getdents64,getcwd -o rglob-empty-list.trace
[[ $rc = 0 && $out = $'after:old\n0' && -z $err && -z $rerr \
    && ! -s rglob-empty-list.trace ]]; chk rglob.15

# The require-message is data, not a regex, and the operand's failure is uncaught.
run_sash_under 'array _patterns=(); array a=(old)
ERR { - echo "$(join a)" }
- rglob a missing-directory @{_patterns:?(}
echo wrong' \
    strace -qq -e trace=open,getdents64,getcwd -o rglob-required.trace
[[ $rc = 1 && $out = old && -z $err && $rerr == *": (" \
    && ! -s rglob-required.trace ]]; chk rglob.16

run_sash 'array _patterns=()
rglob a glob-data @{_patterns:-(}; echo "$(len a)"'
[[ $rc != 0 && $err == *SB5023* ]]; chk rglob.17

run_sash 'array _patterns=("^a[.]sel$")
if attest pattern in _patterns r? { array _copy=(@_patterns) } else { exit }
rglob a glob-data @_copy; echo "$(len a)"'
[[ $rc != 0 && $err == *SB5106* ]]; chk rglob.18

run_sash 'array _patterns=("^a[.]sel$"); attest pattern in _patterns r?
rglob a glob-data "${_patterns[7]}"; echo "$(len a)"'
[[ $rc != 0 && $err == *SB5106* ]]; chk rglob.19

run_sash 'array _patterns=("^a[.]sel$"); attest pattern in _patterns r?
rglob a glob-data "${_patterns[0]:?missing pattern}"
echo "$(join a)"'
[[ $rc = 0 && $out = "$base/a.sel" && -z $err && -z $rerr ]]; chk rglob.20

run_sash 'array directories=(glob-data)
rglob a @directories "^a[.]sel$"; echo "$(len a)"'
[[ $rc != 0 && $err == *SB5070* ]]; chk rglob.21

run_sash 'str directory=glob-data
rglob a ${directory:~} "^a[.]sel$"; echo "$(len a)"'
[[ $rc != 0 && $err == *SB5046* ]]; chk rglob.22

run_sash 'str directory=glob-data
array _patterns=("^a[.]sel$"); attest pattern in _patterns r?
fn change -> int { directory=missing-directory; return 0 }
rglob a "$directory" "${_patterns[$(change)]:?missing pattern}"
echo "$directory:$(join a)"'
[[ $rc = 0 && $out = "missing-directory:$base/a.sel" && -z $err && -z $rerr ]]; chk rglob.23

run_sash 'fn fill { rglob A glob-data @B }; fn relay { fill =A =B }
array a=(old); array _patterns=("^a[.]sel$"); attest pattern in _patterns r?
relay =a =_patterns; echo "$(join a ,)"'
[[ $rc = 0 && $out = "old,$base/a.sel" && -z $err && -z $rerr ]]; chk rglob.24

run_sash 'for directory in glob-data missing-directory {
    if rglob _batch "$directory" "^a[.]sel$" { echo "ok:$(len _batch)" } else { echo "bad:$(len _batch)" }
}'
[[ $rc = 0 && $out = $'ok:1\nbad:0' && -z $err && -z $rerr ]]; chk rglob.25

run_sash 'fn directory { echo "$(join _batch)" }
rglob _batch "$(directory)" "^a[.]sel$"; echo "$(len _batch)"'
[[ $rc != 0 && $err == *SB5104* && $err == *'`_batch`'* ]]; chk rglob.26

run_sash 'str directory=$(- getenv RGLOB_DIRECTORY)
rglob a "$directory" "."; echo "$(len a)"'
[[ $rc != 0 && $err == *SB5020* ]]; chk rglob.27

SASH_CFLAGS=--no-taint-cst run_sash 'str directory=$(- getenv RGLOB_DIRECTORY)
rglob a "$directory" "."; echo "$(len a)"'
[[ $rc != 0 && $err == *tainted* ]]; chk rglob.28

# Fail after the second pattern has read its first buffer; publish neither batch.
run_sash_under 'array a=(old)
if rglob a glob-data "^a[.]sel$" "^b[.]sel$" { echo wrong } else { echo "$(join a)" }' \
    strace -qq -e trace=open,getdents64,close \
    -e inject=getdents64:error=EIO:when=4 -o rglob-read.trace
opened=$(sed -nE '/"glob-data"/s/.* = ([0-9]+)$/\1/p' rglob-read.trace | tail -n 1)
[[ $rc = 0 && $out = old && -z $err && -z $rerr && -n $opened \
    && $(grep -cE 'getdents64\(.* = [1-9][0-9]*$' rglob-read.trace) = 2 ]] \
    && grep -qE 'getdents64\(.*EIO.*INJECTED' rglob-read.trace \
    && grep -qE "^close\\($opened\\) += 0$" rglob-read.trace; chk rglob.29

run_sash 'array a=(old)
ERR { - echo "err:$(join a)" }
rglob a rglob-missing "."
echo wrong'
[[ $rc = 1 && $out = err:old && -z $err && $rerr == *"rglob: open"* \
    && $rerr == *rglob-missing* && $rerr == *"No such file"* ]]; chk rglob.30

run_sash "array a=(old); str directory=\$'glob-data\\x00suffix'
if rglob a \"\$directory\" '.' { echo wrong } else { echo \"\$(join a)\" }"
[[ $rc = 0 && $out = old && -z $err && -z $rerr ]]; chk rglob.31

run_sash_under "array a=(old)
if rglob a glob-data '^a[.]sel\$' { echo wrong } else { echo \"\$(join a)\" }
rglob a '$base' '^a[.]sel\$'
echo \"\$(join a ,)\"" \
    strace -qq -e trace=open,getcwd,close -e inject=getcwd:error=ENOENT:when=1 -o rglob-cwd.trace
opened=$(sed -nE '/"glob-data"/s/.* = ([0-9]+)$/\1/p' rglob-cwd.trace)
[[ $rc = 0 && $out = "old"$'\n'"old,$base/a.sel" && -z $err && -z $rerr \
    && $(grep -c getcwd rglob-cwd.trace) = 1 && -n $opened ]] \
    && grep -qE 'getcwd\(.*ENOENT.*INJECTED' rglob-cwd.trace \
    && grep -qE "^close\\($opened\\) += 0$" rglob-cwd.trace; chk rglob.32

# Regex compilation exhausts this heap before directory enumeration starts.
SASH_CFLAGS=--heap-size=16K run_sash_under 'array a=(old)
ERR { - echo "${a[0]}" }
if rglob a glob-heap "." { echo wrong } else { echo caught }' \
    strace -qq -e trace=getdents64 -o rglob-heap.trace
[[ $rc = 1 && $out = old && -z $err && $rerr == *"out of memory"* \
    && ! -s rglob-heap.trace ]]; chk rglob.33

SASH_CFLAGS=--heap-size=128K run_sash_under 'array a=(old); int i=0
while (( i < 40 )) {
    - rglob a glob-data "^a[.]sel$"
    (( i++ ))
}
echo "$(join a)"' \
    prlimit --nofile=8:8 strace -qq -e trace=open,getdents64,close \
    -e inject=getdents64:error=EIO -o rglob-repeat.trace
[[ $rc = 0 && $out = old && -z $err && -z $rerr \
    && $(grep -c 'EIO.*INJECTED' rglob-repeat.trace) = 40 ]] \
    && ! grep -q EMFILE rglob-repeat.trace; chk rglob.34

run_sash 'rglob a glob-bytes "."; echo "$(join a ,)"'
[[ $rc = 0 && $out = "$cwd/glob-bytes/z,$cwd/glob-bytes/"$'\x80'",$cwd/glob-bytes/"$'\xff' \
    && -z $err && -z $rerr ]]; chk rglob.35

mkdir rglob-lines
touch rglob-lines/$'a.sel\nsuffix'
run_sash 'rglob anchored rglob-lines "^a[.]sel$"
rglob unanchored rglob-lines "a[.]sel"
echo "$(len anchored):$(join unanchored)"'
[[ $rc = 0 && $out = "0:$cwd/rglob-lines/"$'a.sel\nsuffix' && -z $err && -z $rerr ]]; chk rglob.36

run_sash 'rglob a glob-data; echo "$(len a)"'
[[ $rc != 0 && $err == *SB4045* && $err == *"at least one pattern"* ]]; chk rglob.37

run_sash 'int array patterns=(7)
rglob a glob-data @patterns; echo "$(len a)"'
[[ $rc != 0 && $err == *SB4030* && $err == *"integer elements"* ]]; chk rglob.38

run_sash 'array patterns; array a=(old)
fn directory -> int { /bin/false; return 7 }
ERR { - echo "$(join a)" }
- rglob a "$(directory)" @patterns
echo wrong'
[[ $rc = 1 && $out = old && -z $err && $rerr == *"/bin/false: command failed"* ]]; chk rglob.39

run_sash 'argv 1 _patterns
if attest pattern in _patterns r? {
    rglob _batch glob-data @{_patterns:?patterns required}
    echo "$(join _batch)"
} else { echo invalid }' '^a[.]sel$'
[[ $rc = 0 && $out = "$base/a.sel" && -z $err && -z $rerr ]]; chk rglob.40

run_sash 'array _patterns=("^a[.]sel$"); attest pattern in _patterns r?
array _copy=(@{_patterns:!})
rglob a glob-data @_copy; echo "$(len a)"'
[[ $rc != 0 && $err == *SB5106* ]]; chk rglob.41

run_sash 'array _patterns=("^a[.]sel$"); attest pattern in _patterns r?
array _copy=(@{_patterns:!}); attest pattern in _copy r?
rglob a glob-data "${_copy[0]}"
echo "$(join a)"'
[[ $rc = 0 && $out = "$base/a.sel" && -z $err && -z $rerr ]]; chk rglob.42

run_sash "array _empty=()
array _copy=(@{_empty:-'^a[.]sel\$'})
rglob a glob-data @_copy; echo \"\$(len a)\""
[[ $rc != 0 && $err == *SB5106* ]]; chk rglob.43

# Valid syntax can still exceed the engine's bounded matching workspace.
printf -v many_alternatives 'a|%.0s' {1..300}
many_alternatives=${many_alternatives%|}
SASH_CFLAGS=--heap-size=2M run_sash_under "array a=(old)
ERR { - echo \"\$(join a)\" }
if rglob a glob-data '$many_alternatives' { echo wrong } else { echo caught }" \
    strace -qq -e trace=getdents64 -o rglob-workspace.trace
[[ $rc = 1 && $out = old && -z $err && $rerr == *"regex matching workspace exhausted"* ]] \
    && grep -qE 'getdents64\(.* = [1-9][0-9]*$' rglob-workspace.trace; chk rglob.44

run_sash 'map a=([old]=value); rglob a glob-data "."; echo "$(len a)"'
[[ $rc != 0 && $err == *SB4030* ]]; chk rglob.45

run_sash 'int directory=7; rglob a $directory "."; echo "$(len a)"'
[[ $rc != 0 && $err == *SB4030* && $err == *'`directory` is an int variable'* ]]; chk rglob.46

run_sash 'rglob a glob-data "." | /bin/cat'
[[ $rc != 0 && $err == *SB4022* ]]; chk rglob.47

run_sash 'rglob a glob-data "." &'
[[ $rc != 0 && $err == *SB4024* ]]; chk rglob.48

run_sash 'array _patterns=("^a[.]sel$"); attest pattern in _patterns r?
array mutable=(@_patterns); array _copy=(@mutable)
rglob a glob-data "${_copy[0]}"; echo "$(len a)"'
[[ $rc != 0 && $err == *SB5106* ]]; chk rglob.49

set -e
tally
