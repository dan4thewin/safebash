#!/bin/bash
# spec/filetest.sh — [[ ]] file/path tests, NUL predicates and binary comparisons.
# NUL-free paths retain their existing semantics. Section A pins -N, which compares
# WHOLE SECONDS with `>=` and ignores nanoseconds entirely (bash test.c);
# sash previously used a strict `>` plus a nanosecond tiebreak and answered
# "no" to the two cases -N exists to report.
set -eu
. common.sh
setup filetest
plan=109
echo "1..$plan" >&3

set +e

# ── A. -N: modified since last read ──────────────────────────────────
# Bash: mtime >= atime on whole seconds. Nanoseconds are NOT consulted.
# A1 freshly written, never read — mtime == atime to the nanosecond.
# The case -N exists to report. A strict `>` answered "no".
printf 'x\n' > n1
run_sash 'if [[ -N n1 ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.1

# A2 same, with both stamps pinned exactly equal
printf 'x\n' > n2; touch -d '@1700000000.000000000' n2
run_sash 'if [[ -N n2 ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.2

# A3 mtime strictly newer than atime
printf 'x\n' > n3
touch -a -d '@1700000000' n3; touch -m -d '@1700000900' n3
run_sash 'if [[ -N n3 ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.3

# A4 mtime older than atime — read since last write
printf 'x\n' > n4
touch -m -d '@1700000000' n4; touch -a -d '@1700000900' n4
run_sash 'if [[ -N n4 ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.4

# A5 same second, atime nanoseconds GREATER than mtime's. Bash still
# says yes because it truncates to seconds. The old nsec tiebreak
# made this "no".
printf 'x\n' > n5
touch -m -d '@1700000000.000000100' n5; touch -a -d '@1700000000.000000900' n5
run_sash 'if [[ -N n5 ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.5

# A6 nonexistent file is never "modified since read"
run_sash 'if [[ -N nosuch ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.6

# ── B. portable core: -e / -f / -d ─────────────────────────────────
printf 'x\n' > plain; mkdir -p adir

run_sash 'if [[ -e plain ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.7
run_sash 'if [[ -e nosuch ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.8
run_sash 'if [[ -f plain ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.9
# -f is regular-file, not merely existing: a directory is "no"
run_sash 'if [[ -f adir ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.10
run_sash 'if [[ -d adir ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.11
run_sash 'if [[ -d plain ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.12

# ── C. the remaining operators ──────────────────────────────────────
: > empty; ln -sf plain slink; mkfifo afifo 2>/dev/null
printf '#!/bin/sh\n' > execf; chmod +x execf

# -s: exists AND has nonzero size
run_sash 'if [[ -s plain ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.13
run_sash 'if [[ -s empty ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.14

# -r / -w: access permission
run_sash 'if [[ -r plain ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.15
run_sash 'if [[ -w plain ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.16
run_sash 'if [[ -r nosuch ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.17

# -x: execute permission, not mere existence
run_sash 'if [[ -x execf ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.18
run_sash 'if [[ -x plain ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.19

# -L / -h: symlink. Both test the link itself, not its target.
run_sash 'if [[ -L slink ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.20
run_sash 'if [[ -L plain ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.21
run_sash 'if [[ -h slink ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.22
# -f follows the link, so a symlink to a regular file is "yes"
run_sash 'if [[ -f slink ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.23

# -c / -b: character vs block special. /dev/null is character.
run_sash 'if [[ -c /dev/null ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.24
run_sash 'if [[ -b /dev/null ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.25

# -p: named pipe.  -S: socket (a plain file is not one)
run_sash 'if [[ -p afifo ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.26
run_sash 'if [[ -S plain ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.27

# -O: owned by the effective uid.  -a: historical alias of -e.
run_sash 'if [[ -O plain ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.28

# -G compares the file's group with the effective gid; a missing path is false.
printf 'x\n' > groupf
chgrp "$(id -g)" groupf
group_rc=$?
run_sash 'if [[ -G groupf ]]; then echo yes; else echo no; fi
if [[ -G nosuch ]]; then echo yes; else echo no; fi'
[[ $group_rc = 0 && $rc = 0 && $out = $'yes\nno' && -z $err ]]; chk ft.47

# ── D. stat-mode bits: -u / -g / -k ─────────────────────────────────
# The codegen table has implemented these as codes 13/12/10 since it was
# written; the PARSER simply never listed them, so `[[ -k f ]]` reported
# SB4014 and the capability was unreachable. Each bit is checked both set
# and clear, so a constant-`no` implementation cannot pass.
printf 'x\n' > modef
chmod u+s,g+s,+t modef
run_sash 'if [[ -u modef ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.29
run_sash 'if [[ -g modef ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.30
run_sash 'if [[ -k modef ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.31

# `plain` carries none of the three
run_sash 'if [[ -u plain ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.32
run_sash 'if [[ -g plain ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.33
run_sash 'if [[ -k plain ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.34

# a missing path is "no", not an error
run_sash 'if [[ -u nosuch ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.35

# `-g` the mode bit must not collide with `g?`, the DD#93 content-shape
# test that shares the letter.
run_sash 'if [[ g? "hi" ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.36

# ── D. binary comparisons ────────────────────────────────────────────
# `<`/`>`/`-nt`/`-ot`/`-ef` were implemented in native and fell through
# to a `/* TODO */` arm in the C backend, which returned constant 0 — so
# every one of them was silently FALSE in c while native and bash said
# true. Nothing pinned them in either backend. These run unguarded in
# BOTH modes on purpose: a mode guard here is what let the gap survive.

# D1-D3 string ordering is a BYTE compare (the runtime is locale-free,
# so this is LC_ALL=C bash), via the same sash_strcmp native uses.
run_sash 'v=abc; if [[ $v < abd ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.37

run_sash 'v=abd; if [[ $v < abc ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.38

run_sash 'v=abd; if [[ $v > abc ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.39

# D4 uppercase sorts before lowercase on bytes ('Z' 0x5A < 'a' 0x61).
# Under a collating locale bash would answer the other way; this pins
# that sash is byte-ordered.
run_sash 'v=Z; if [[ $v < a ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.40

# D5-D6 -nt / -ot on mtime.
touch -d '@1700000000' b_old; touch -d '@1700009000' b_new
run_sash 'if [[ b_new -nt b_old ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.41

run_sash 'if [[ b_old -ot b_new ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.42

# D7-D8 -ef is same dev+inode, so a hard link is "yes" and a distinct
# file with identical content is "no".
ln b_old b_link
run_sash 'if [[ b_old -ef b_link ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.43

run_sash 'if [[ b_old -ef b_new ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.44

# D9 -nt resolves SUB-SECOND. bash compares timespec (nanoseconds), not
# whole seconds, so two files 1 ms apart inside the SAME whole second
# still order. A seconds-granularity implementation passes ft.41 and
# fails here, which is the only reason this test exists.
touch -d '@1700000000.000000000' b_ns1; touch -d '@1700000000.001000000' b_ns2
run_sash 'if [[ b_ns2 -nt b_ns1 ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.45

# D10 ... and the same pair is NOT newer the other way (strict >, and no
# accidental symmetry from a truncating compare).
run_sash 'if [[ b_ns1 -nt b_ns2 ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.46

# DD135: counted NUL is distinct from the string's storage terminator.
run_sash $'array samples=("" abc $\'\\u00e9\' $\'\\x00abc\' $\'ab\\x00cd\' $\'abc\\x00\' $\'\\x00\')
for s in @samples {
    if [[ 0? $s ]] { echo nul } else { echo free }
}
echo "0?"'
[[ $rc = 0 && $out = $'free\nfree\nfree\nnul\nnul\nnul\nnul\n0?' && -z $err && -z $rerr ]]; chk nulcheck.1

run_sash $'str empty="" plain=abc nul=$\'a\\x00b\'
if [[ ! 0? $empty && ! 0? $plain ]] { echo free }
if [[ 0? $nul || -n ${empty:?should-not-run} ]] { echo contains }
if [[ ! 0? $nul ]] { echo wrong } else { echo negated }'
[[ $rc = 0 && $out = $'free\ncontains\nnegated' && -z $err && -z $rerr ]]; chk nulcheck.2

printf 'a\0b\n' > nul-line
run_sash 'str s; readline s <nul-line; attest s 0?; echo "$(strlen $s)"
str t; readline t <plain; attest t [[ ! 0? $t ]]; echo "$t"'
[[ $rc = 0 && $out = $'3\nx' && -z $err && -z $rerr ]]; chk nulcheck.3

run_sash $'array a=($\'a\\x00\' $\'\\x00b\')
attest item in a 0?
str k=$\'k\\x00\'
map m
m[$k]=$\'v\\x00\'
attest key value in m 0?
echo checked'
[[ $rc = 0 && $out = checked && -z $err && -z $rerr ]]; chk nulcheck.4

run_sash $'int calls=0
fn sample { (( calls += 1 )); echo $\'a\\x00b\' }
if [[ 0? $(sample) ]] { echo "$calls" }'
[[ $rc = 0 && $out = 1 && -z $err && -z $rerr ]]; chk nulcheck.5

# False answers alone cannot prove that no truncated pathname was looked up.
run_sash_under $'array paths=($\'\\x00plain\' $\'plain\\x00tail\' $\'plain\\x00\')
for p in @paths {
    if [[ -e $p ]] { echo wrong }
    if [[ -a $p ]] { echo wrong }
    if [[ -f $p ]] { echo wrong }
    if [[ -d $p ]] { echo wrong }
    if [[ -r $p ]] { echo wrong }
    if [[ -w $p ]] { echo wrong }
    if [[ -x $p ]] { echo wrong }
    if [[ -s $p ]] { echo wrong }
    if [[ -L $p ]] { echo wrong }
    if [[ -h $p ]] { echo wrong }
    if [[ -c $p ]] { echo wrong }
    if [[ -p $p ]] { echo wrong }
    if [[ -k $p ]] { echo wrong }
    if [[ -b $p ]] { echo wrong }
    if [[ -g $p ]] { echo wrong }
    if [[ -u $p ]] { echo wrong }
    if [[ -G $p ]] { echo wrong }
    if [[ -O $p ]] { echo wrong }
    if [[ -S $p ]] { echo wrong }
    if [[ -N $p ]] { echo wrong }
}
echo checked' strace -qq -e trace=%file -o nul-unary.trace
[[ $rc = 0 && $out = checked && -z $err && -z $rerr ]] &&
    ! grep -Eq '^(newfstatat|statx|stat|lstat|access|faccessat|faccessat2)\(' nul-unary.trace
chk nulcheck.6

run_sash_under $'array paths=($\'\\x00b_old\' $\'b_old\\x00tail\' $\'b_old\\x00\' $\'missing\\x00tail\')
for p in @paths {
    if [[ $p -nt b_old ]] { echo wrong }
    if [[ b_new -nt $p ]] { echo wrong }
    if [[ $p -ot b_new ]] { echo wrong }
    if [[ b_old -ot $p ]] { echo wrong }
    if [[ $p -ef b_link ]] { echo wrong }
    if [[ b_link -ef $p ]] { echo wrong }
}
echo checked' strace -qq -e trace=%file -o nul-binary.trace
[[ $rc = 0 && $out = checked && -z $err && -z $rerr ]] &&
    ! grep -Eq '^(newfstatat|statx|stat|lstat|access|faccessat|faccessat2)\(' nul-binary.trace
chk nulcheck.7

# Positive syscall control, ordinary missing-file semantics, and single evaluation.
run_sash_under 'int calls=0
fn oldpath { (( calls += 1 )); echo b_old }
fn newpath { (( calls += 1 )); echo b_new }
if [[ $(newpath) -nt $(oldpath) ]] { echo "calls=$calls" }
if [[ b_new -nt missing ]] { echo newer }
if [[ missing -ot b_new ]] { echo older }
if [[ b_old -ef b_link ]] { echo same }
if [[ -e plain ]] { echo exists }' strace -qq -e trace=%file -o nul-positive.trace
[[ $rc = 0 && $out = $'calls=2\nnewer\nolder\nsame\nexists' && -z $err && -z $rerr ]] &&
    grep -E '^(newfstatat|statx|stat|lstat|access|faccessat|faccessat2)\(' nul-positive.trace
chk nulcheck.8

run_sash 'str s; readline s <plain
if [[ ! 0? ${s:!} ]] { echo "$s" }'
[[ $rc != 0 && $err == *:2:29:*SB5020* ]]; chk nulcheck.9

run_sash 'int n=0; if [[ 0? $n ]] { echo wrong }'
[[ $rc != 0 && $err == *SB4030* && $err == *quote* ]]; chk nulcheck.10

# DD93: cpath? checks spelling, not existence, type, authorization or symlinks.
# /dev/null/child cannot exist; the predicate must not try to resolve it.
run_sash_under 'array paths=(/ /a /a/ /A0_z/file-1 /a/.config /a/file..bak /... /.-name /dev/null/child)
for p in @paths {
    if [[ cpath? $p ]] { echo $p } else { echo wrong }
}
echo cpath?' strace -qq -e trace=%file,getcwd -o cpath.trace
[[ $rc = 0 && $out = $'/\n/a\n/a/\n/A0_z/file-1\n/a/.config\n/a/file..bak\n/...\n/.-name\n/dev/null/child\ncpath?' && -z $err && -z $rerr ]] &&
    ! grep -Eq '^(newfstatat|statx|stat|lstat|access|faccessat|faccessat2|open|openat|openat2|readlink|readlinkat|getcwd)\(' cpath.trace
chk cpath.1

run_sash $'array paths=("" relative a/b ./a ../a $\'\\x00/a\' $\'/a\\x00\' $\'/\\x00\')
for p in @paths {
    if [[ cpath? $p ]] { echo wrong }
}
echo checked'
[[ $rc = 0 && $out = checked && -z $err && -z $rerr ]]; chk cpath.2

run_sash 'array paths=(// /// //a /a//b /a// /./a /a/./b /a/. /a/./ /../a /a/../b /a/.. /a/../)
for p in @paths {
    if [[ cpath? $p ]] { echo wrong }
}
echo checked'
[[ $rc = 0 && $out = checked && -z $err && -z $rerr ]]; chk cpath.3

run_sash 'array paths=(/- /-a /-a/b /a/-b /a/b/-c/ /--help)
for p in @paths {
    if [[ cpath? $p ]] { echo wrong }
}
echo checked'
[[ $rc = 0 && $out = checked && -z $err && -z $rerr ]]; chk cpath.4

# Exercise every byte, including NUL, controls, shell punctuation and high bytes.
# A slash between the two letters is a valid component separator.
cpath_bytes=
for (( byte=0; byte<256; byte++ )); do
    printf -v cpath_byte '\\x%02x' "$byte"
    cpath_bytes+=$cpath_byte
done
printf '%b' "$cpath_bytes" > cpath-bytes
run_sash 'str bytes=$(<cpath-bytes)
data=${bytes:!}
echo "$(strlen $data)"
int i=0
for (( i=0; i<256; i+=1 )) {
    b=${data:0:1}
    if [[ cpath? "/a${b}z" ]] { echo "$i" }
    data=${data:1}
}'
expected=$(printf '%s\n' 256 45 46 47 {48..57} {65..90} 95 {97..122})
[[ $rc = 0 && $out = "$expected" && -z $err && -z $rerr ]]; chk cpath.5

run_sash 'str bad="/tmp/a b" empty=""
if [[ ! cpath? $bad && cpath? /tmp/good ]] { echo rejected }
if [[ cpath? / || cpath? ${empty:?should-not-run} ]] { echo lazy }
if [[ cpath? $bad && cpath? ${empty:?should-not-run} ]] { echo wrong } else { echo false }'
[[ $rc = 0 && $out = $'rejected\nlazy\nfalse' && -z $err && -z $rerr ]]; chk cpath.6

run_sash 'int calls=0
fn sample { (( calls += 1 )); write /tmp/once }
if [[ cpath? $(sample) ]] { echo "$calls" }'
[[ $rc = 0 && $out = 1 && -z $err && -z $rerr ]]; chk cpath.7

# Full and shorthand predicates validate acquired data without changing its bytes.
run_sash 'path=$(- arg 1)
attest path cpath?
echo $path
file=$(- arg 2)
attest file [[ cpath? $file && -f $file ]]
echo $file' /var/.cache/file-1 /proc/self/exe
[[ $rc = 0 && $out = $'/var/.cache/file-1\n/proc/self/exe' && -z $err && -z $rerr ]]; chk cpath.8

run_sash 'path=$(- arg 1)
attest path cpath?
echo $path' ../file
[[ $rc = 1 && -z $out && -z $err && $rerr == *"attest: path"* && $rerr == *"failed the gate"* ]]; chk cpath.9

run_sash 'path=$(- arg 1)
if attest path cpath? { echo accepted } else { path=/fallback }
echo $path' /tmp/../leaf
[[ $rc = 0 && $out = /fallback && -z $err && -z $rerr ]]; chk cpath.10

run_sash 'argv 1 paths
attest p in paths cpath?
echo $(join paths)' /alpha /.hidden/value-2/
[[ $rc = 0 && $out = "/alpha /.hidden/value-2/" && -z $err && -z $rerr ]]; chk cpath.11

run_sash 'argv 1 paths
attest p in paths cpath?
echo $(join paths)' /alpha /a//b
[[ $rc = 1 && -z $out && -z $err && $rerr == *"attest: paths"* && $rerr == *"failed the gate"* ]]; chk cpath.12

run_sash 'map paths
paths[/src]=/dst
attest k v in paths cpath?
echo $paths[/src]'
[[ $rc = 0 && $out = /dst && -z $err && -z $rerr ]]; chk cpath.13

# Map shorthand tests both binders, not merely the value or merely the key.
run_sash 'map paths
paths[relative]=/dst
attest k v in paths cpath?
echo $paths[relative]'
key_rc=$rc key_out=$out key_err=$err key_rerr=$rerr
run_sash 'map paths
paths[/src]=relative
attest k v in paths cpath?
echo $paths[/src]'
[[ $key_rc = 1 && -z $key_out && -z $key_err && $key_rerr == *"attest: paths"* && $key_rerr == *"failed the gate"* &&
   $rc = 1 && -z $out && -z $err && $rerr == *"attest: paths"* && $rerr == *"failed the gate"* ]]; chk cpath.14

run_sash 'path=$(- arg 1)
if [[ cpath? ${path:!} ]] { echo $path }' /tmp/good
[[ $rc != 0 && $err == *SB5020* ]]; chk cpath.15

run_sash 'int n=1; if [[ cpath? $n ]] { echo wrong }'
[[ $rc != 0 && $err == *SB4030* && $err == *quote* ]]; chk cpath.16

run_sash 'str path=/tmp
attest path [[ cpath? "$path$(write /child)" ]]
echo $path'
[[ $rc != 0 && $err == *SB5098* ]]; chk cpath.17

# DD119: only a directly tested immutable scalar gets the redirect exemption.
run_sash '_path=$(- arg 1)
attest _path -n
echo direct > $_path' redir-direct
[[ $rc = 0 && -z $err && -z $rerr && $(<redir-direct) = direct ]]; chk redirproof.1

mkdir redir-dir
run_sash '_dir=$(- arg 1)
attest _dir cpath?
echo literal > $_dir/log' "$PWD/redir-dir"
[[ $rc = 0 && -z $err && -z $rerr && $(<redir-dir/log) = literal ]]; chk redirproof.2

run_sash 'const path=redir-quoted
attest path [[ -n ""${path}"" ]]
echo quoted > "${path}"'
[[ $rc = 0 && -z $err && -z $rerr && $(<redir-quoted) = quoted ]]; chk redirproof.3

# Each whitelisted predicate must independently authorize its success arm.
good=1
for op in -n 'cpath?' 'p?' 'g?' -e -f -d -r -w -x -s -L -h; do
    case $op in
        -d) path="$PWD/adir" ;;
        -x) path="$PWD/execf" ;;
        -L|-h) path="$PWD/slink" ;;
        *) path="$PWD/plain" ;;
    esac
    rm -f "$path.redir"
    run_sash "const path='$path'
if [[ $op \"\$path\" ]] { echo checked > \$path.redir } else { echo wrong }"
    [[ $rc = 0 && -z $err && -z $rerr && -z $out && $(<"$path.redir") = checked ]] || good=0
done
[[ $good = 1 ]]; chk redirproof.4

run_sash '_path=redir-and
[[ -n $_path ]] && echo short > $_path'
[[ $rc = 0 && -z $err && -z $rerr && $(<redir-and) = short ]]; chk redirproof.5

run_sash '_path=$(- arg 1)
if attest _path -n { echo caught > $_path } else { echo empty }' redir-caught
[[ $rc = 0 && -z $err && -z $rerr && -z $out && $(<redir-caught) = caught ]]; chk redirproof.6

run_sash '_path=$(- arg 1)
attest _path -n
echo wrong > $_path' ""
[[ $rc = 1 && -z $err && -z $out && $rerr == *"attest: _path"* && $rerr == *"failed the gate"* ]]; chk redirproof.7

run_sash '_path=$(- arg 1)
if attest _path cpath? { echo wrong > $_path } else { echo empty }' ""
[[ $rc = 0 && -z $err && -z $rerr && $out = empty ]]; chk redirproof.8

run_sash 'const path=redir-continuing
if [[ -n $path ]] { echo success } else { exit 1 }
echo continued > $path'
[[ $rc = 0 && -z $err && -z $rerr && $out = success && $(<redir-continuing) = continued ]]; chk redirproof.9

run_sash 'for name in redir-loop-a redir-loop-b {
    _path=$name
    attest _path -n
    echo $name > $_path
}'
[[ $rc = 0 && -z $err && -z $rerr && $(<redir-loop-a) = redir-loop-a && $(<redir-loop-b) = redir-loop-b ]]; chk redirproof.10

run_sash 'fn writefile {
    _path=$1
    attest _path -n
    echo $1 > $_path
}
writefile redir-call-a
writefile redir-call-b'
[[ $rc = 0 && -z $err && -z $rerr && $(<redir-call-a) = redir-call-a && $(<redir-call-b) = redir-call-b ]]; chk redirproof.11

run_sash 'fn unused {
    _path=redir-unused-good
    attest _path -n
    echo unused > $_path
}
echo defined'
[[ $rc = 0 && -z $err && -z $rerr && $out = defined && ! -e redir-unused-good ]]; chk redirproof.12

run_sash '_text=$(
    _path=redir-capture
    attest _path -n
    echo written > $_path
    echo captured
)
attest _text -n
echo $_text'
[[ $rc = 0 && -z $err && -z $rerr && $out = captured && $(<redir-capture) = written ]]; chk redirproof.13

# Attached calls, stages and parent readers share the original target obligation.
run_sash 'const path=redir-forms
attest path -n
fn output { echo function }
output > $path
/bin/echo pipeline | /bin/cat > $path.pipe
readline _line < $path
attest _line -n
echo $_line
/bin/cat < $path.pipe'
[[ $rc = 0 && -z $err && -z $rerr && $out = $'function\npipeline' && $(<redir-forms) = function && $(<redir-forms.pipe) = pipeline ]]; chk redirproof.14

run_sash 'const path=redir-exec
attest path -n
exec > $path
echo descriptor'
[[ $rc = 0 && -z $err && -z $rerr && -z $out && $(<redir-exec) = descriptor ]]; chk redirproof.15

run_sash '_dir=redir-dir
attest _dir -n
_file=$_dir/derived
attest _file -n
echo own > $_file'
[[ $rc = 0 && -z $err && -z $rerr && $(<redir-dir/derived) = own ]]; chk redirproof.16

run_sash 'str path=redir-mutable
attest path -n
echo wrong > $path'
[[ $rc != 0 && $err == *SB4030*"redirect target"* && $err == *'`$path`'* ]]; chk redirproof.17

run_sash 'const path=redir-untested
echo wrong > $path'
[[ $rc != 0 && $err == *SB4030*"redirect target"* && $err == *'`$path`'* ]]; chk redirproof.18

# Neither exact copies (in either direction) nor precomputed paths inherit proof.
good=1
for source in \
    '_path=redir-copy; attest _path -n; _copy=$_path; echo wrong > $_copy' \
    '_path=redir-copy; _copy=$_path; attest _path -n; echo wrong > $_copy' \
    '_path=redir-copy; _copy=$_path; attest _copy -n; echo wrong > $_path' \
    '_path=redir-dir; attest _path -n; _copy=$_path/derived; echo wrong > $_copy'; do
    run_sash "$source"
    [[ $rc != 0 && $err == *SB4030*"redirect target"* ]] || good=0
done
[[ $good = 1 ]]; chk redirproof.19

good=1
for target in '$_path$_other' '$_path$_path' '$_path${_other:?missing}'; do
    run_sash "_path=redir-multi; _other=tail
attest _path -n
attest _other -n
echo wrong > $target"
    [[ $rc != 0 && $err == *SB4030*"redirect target"* ]] || good=0
done
[[ $good = 1 ]]; chk redirproof.20

# Source-level compound tests must not become simple witnesses after lowering.
good=1
for source in \
    '_path=redir-compound; if [[ -n $_path && -n $_path ]] { echo wrong > $_path }' \
    '_path=redir-compound; if [[ -n $_path || -n $_path ]] { echo wrong > $_path }' \
    '_path=redir-compound; attest _path [[ -n $_path && -n $_path ]]; echo wrong > $_path' \
    '_path=redir-compound; if attest _path [[ -n $_path && -n $_path ]] { echo wrong > $_path }'; do
    run_sash "$source"
    [[ $rc != 0 && $err == *SB4030*"redirect target"* ]] || good=0
done
[[ $good = 1 ]]; chk redirproof.21

good=1
for op in -a -N -c 'r?' 'i?' '0?' -z; do
    run_sash "const path=123
if [[ $op \$path ]] { echo wrong > \$path }"
    [[ $rc != 0 && $err == *SB4030*"redirect target"* ]] || good=0
done
[[ $good = 1 ]]; chk redirproof.22

# A test of derived or disposition-protected bytes is not a direct binding test.
good=1
for test in '-d $_path/suffix' '-n ${_path:?missing}' '-n ${_path:-fallback}'; do
    run_sash "_path=redir-operand
attest _path [[ $test ]]
echo wrong > \$_path"
    [[ $rc != 0 && $err == *SB4030*"redirect target"* ]] || good=0
done
[[ $good = 1 ]]; chk redirproof.23

good=1
for test in '! -z $_path' '$_path == redir-deduction' '$_path != ""'; do
    run_sash "_path=redir-deduction
if [[ $test ]] { echo wrong > \$_path }"
    [[ $rc != 0 && $err == *SB4030*"redirect target"* ]] || good=0
done
[[ $good = 1 ]]; chk redirproof.24

good=1
for source in \
    '_path=redir-flow; if [[ -n $_path ]] { echo yes } else { echo wrong > $_path }' \
    '_path=redir-flow; if [[ -n $_path ]] { echo yes }; echo wrong > $_path' \
    '_path=redir-flow; echo wrong > $_path; attest _path -n'; do
    run_sash "$source"
    [[ $rc != 0 && $err == *SB4030*"redirect target"* ]] || good=0
done
[[ $good = 1 ]]; chk redirproof.25

good=1
for source in \
    'const path=redir-body; fn prove { attest path -n }; prove; echo wrong > $path' \
    'const path=redir-body; attest path -n; fn use { echo wrong > $path }; use' \
    'fn pathvalue { _path=redir-body; attest _path -n; echo $_path }; _result=$(pathvalue); echo wrong > $_result'; do
    run_sash "$source"
    [[ $rc != 0 && $err == *SB4030*"redirect target"* ]] || good=0
done
[[ $good = 1 ]]; chk redirproof.26

good=1
for source in \
    'array _paths=(redir-cell); attest p in _paths -n; echo wrong > ${_paths[0]}' \
    'array _paths=(redir-cell); attest p in _paths -n; _path=${_paths[0]}; echo wrong > $_path' \
    'array _paths=(redir-cell); if [[ -n ${_paths[0]} ]] { echo wrong > ${_paths[0]} }'; do
    run_sash "$source"
    [[ $rc != 0 && $err == *SB4030*"redirect target"* ]] || good=0
done
[[ $good = 1 ]]; chk redirproof.27

run_sash 'fn unused {
    _path=redir-unused-bad
    echo wrong > $_path
}
echo defined'
[[ $rc != 0 && $err == *:3:18:*SB4030* && $err == *'`$_path`'* ]]; chk redirproof.28

# Read permission and taint are independent of the redirect certificate.
run_sash '_path=$(- arg 1)
if attest _path -n { echo success } else { exit 1 }
echo wrong > $_path' redir-permission
[[ $rc != 0 && $err == *SB5107* ]]; chk redirproof.29

run_sash '_path=$(- arg 1)
if [[ -n ${_path:!} ]] { echo trusted > ${_path:!} }' redir-trusted
[[ $rc = 0 && -z $err && -z $rerr && $(<redir-trusted) = trusted ]]; chk redirproof.30

run_sash '_path=$(- arg 1)
if [[ -n ${_path:!} ]] { echo wrong > $_path }' redir-tainted
[[ $rc != 0 && $err == *SB5020* ]]; chk redirproof.31

run_sash '_path=$(- arg 1)
if attest _path -n { echo success } else { exit 1 }
echo trusted > ${_path:!}' redir-caught-trusted
[[ $rc = 0 && -z $err && -z $rerr && $out = success && $(<redir-caught-trusted) = trusted ]]; chk redirproof.32

good=1
for flags in --no-taint --no-taint-cst; do
    SASH_CFLAGS=$flags run_sash '_path=redir-bypass; echo wrong > ${_path:!}'
    [[ $rc != 0 && $err == *SB4030*"redirect target"* ]] || good=0
done
[[ $good = 1 ]]; chk redirproof.33

run_sash 'for name in redir-fresh-a redir-fresh-b {
    _path=$name
    if [[ $name == redir-fresh-a ]] {
        attest _path -n
    } else {
        echo wrong > $_path
    }
}'
[[ $rc != 0 && $err == *SB4030*"redirect target"* ]]; chk redirproof.34

# Deferred source obligations do not disappear after a terminal command.
good=1
for end in exit fail; do
    run_sash "const path=redir-dead
$end
echo wrong > \$path"
    [[ $rc != 0 && $err == *'SB4030'*'redirect target'* ]] || good=0
done
[[ $good = 1 ]]; chk redirproof.35

tally
