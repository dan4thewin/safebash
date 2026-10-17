#!/bin/bash
# spec/exec.sh — DD#14: the `exec` builtin (process primitives).
#
# `exec` has two forms:
#   - fd manipulation:   `exec > file` / `exec 2>&1` — redirect the RUNNING
#     process's file descriptors, persisting for the rest of the program (no
#     fork). $? = 0.
#   - program replacement: `exec CMD args` — execve(2) the current process with
#     CMD; control never returns (statements after it are unreachable).
#
# The target CMD is resolved and constrained exactly like a command head
# (DD#23): a literal, compile-time-resolvable external path; a variable /
# expansion target is rejected (SB5011), as are tainted args without attest
# (DD#24).
#
# Both backends. The c backend gained `exec` alongside this un-gating; before
# that it emitted a `/* TODO command */` no-op, so `exec CMD` silently fell
# through to the following statements and `exec > file` un-did its own
# redirect via the per-command restore.
set -eu
. common.sh
setup exec
plan=70
echo "1..$plan" >&3

set +e

# ex.1 replace form, absolute path: process becomes /bin/echo, prints its args
run_sash 'exec /bin/echo replaced'
[[ $rc = 0 && $out = replaced && -z $err ]]; chk ex.1
# ex.2 no-return: statements after a successful `exec` never run
run_sash 'echo before; exec /bin/echo mid; echo after'
[[ $rc = 0 && $out = $'before\nmid' && -z $err ]]; chk ex.2
# ex.3 bare-name target is resolved to its external path (DD#23) — `echo` here
# is the external /bin/echo, not the inline builtin
run_sash 'exec echo viapath'
[[ $rc = 0 && $out = viapath && -z $err ]]; chk ex.3
# ex.4 a PATH-resolved external with args
run_sash 'exec grep -c . /etc/hostname'
[[ $rc = 0 && $out = 1 && -z $err ]]; chk ex.4
# ex.5 fd form persists across multiple statements: after `exec > file`, every
# later stdout write lands in the file (the redirect is on the process, not one
# command), and the program's own stdout is empty
run_sash 'exec > exec_fd_out; echo a; echo b'
[[ $rc = 0 && -z $out && -z $err ]] && [[ $(cat exec_fd_out 2>/dev/null) = $'a\nb' ]]; chk ex.5
# ex.6 fd form result actually went to the file (read it back in a fresh run via
# a second statement before the exec)
run_sash 'echo pre; exec > exec_fd_out2; echo captured'
[[ $rc = 0 && $out = pre && -z $err ]] && [[ $(cat exec_fd_out2 2>/dev/null) = captured ]]; chk ex.6
rm -f exec_fd_out exec_fd_out2

# --- rejections (DD#23/#24 discipline, same as any command) ---

# ex.7 a variable target is a runtime-chosen command → SB5011 (the security
# rule: exec must not replace the process with an unresolved target)
run_sash 'x=/bin/echo; exec $x hi'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk ex.7
# ex.8 an unresolvable bare target → SB5011 (not found)
run_sash 'exec totallyboguscmd_zzz'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk ex.8
# ex.9 a tainted arg to exec is rejected without attest (DD#24), like any command
run_sash 'x=$(getenv HOME); exec /bin/echo $x'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk ex.9
# ex.10 a attested tainted arg is accepted
run_sash 'x=$(getenv HOME); attest x [[ -n $x ]]; exec /bin/echo $x'
[[ $rc = 0 && -n $out && -z $err ]]; chk ex.10
# ex.11 the exec target is recorded in the external manifest
out=$(sash --manifest -c 'exec grep x /etc/hostname' 2>/dev/null); rc=$?
[[ $rc = 0 && $out == *'/usr/bin/grep'* ]]; chk ex.11

# --- close N (DD#14 fd-manipulation): the sanctioned fd-close form. Bash's
# `N>&-` is rejected as redirect syntax (SB4048); `close N` is the builtin. ---

# cl.1 open a numbered fd, write to it, then close it — the file gets the write
# and the program's own stdout stays empty
run_sash 'exec 9> cl_fd_out; /bin/echo hi >&9; close 9'
[[ $rc = 0 && -z $out && -z $err ]] && [[ $(cat cl_fd_out 2>/dev/null) = hi ]]; chk cl.1
rm -f cl_fd_out
# cl.2 a bare close of an unopened fd is a failed syscall → DD#55 abort with the
# errno reason (EBADF), nonzero exit
run_sash 'close 9'
[[ $rc != 0 && -z $out && $rerr == *'close: 9: Bad file descriptor'* ]]; chk cl.2
# cl.3 a caught close (if/…) does not abort — the failure is the condition's
# verdict, so the else branch runs
run_sash 'if close 9 { /bin/echo bad } else { /bin/echo caught }'
[[ $rc = 0 && $out = caught && -z $err ]]; chk cl.3
# cl.4 a `-`-suppressed close does not abort either
run_sash '- close 9; /bin/echo ok'
[[ $rc = 0 && $out = ok && -z $err ]]; chk cl.4
# cl.5 close with no argument → SB4052
run_sash 'close'
[[ $rc != 0 && -z $out && $err == *SB4052* ]]; chk cl.5
# cl.6 close with a non-fd (variable/word) argument → SB4052
run_sash 'close foo'
[[ $rc != 0 && -z $out && $err == *SB4052* ]]; chk cl.6
# cl.7 close with more than one argument → SB4052
run_sash 'close 1 2'
[[ $rc != 0 && -z $out && $err == *SB4052* ]]; chk cl.7
# cl.8 the bash `N>&-` close-via-redirect form is still rejected (SB4048),
# directing to `close N`
run_sash 'exec 9> foo; exec 9>&-'
[[ $rc != 0 && -z $out && $err == *SB4048* ]]; chk cl.8

# cl.9-11 BUG (sash_redirect closed the fd it had just installed): `open` hands
# back the LOWEST free fd, and `dup2(src, fd)` with `src == fd` is a POSIX
# no-op that closes nothing — so the unconditional `close(src)` that followed
# destroyed the redirect whenever the target fd WAS the lowest free one. With
# only 0-2 open that is exactly fd 3: `exec 3> f` left fd 3 closed, `close 3`
# reported EBADF, and — worst — every later `>&3` silently fell through to the
# real stdout instead of the file. fd 9 (cl.1) never hit it, which is why it
# shipped. These pin the target-is-lowest-free case at two different fds.
# cl.9 exec onto fd 3 installs the redirect and the fd stays open to close
run_sash 'exec 3> cl_fd3; /bin/echo three >&3; close 3'
[[ $rc = 0 && -z $out && -z $err ]] && [[ $(cat cl_fd3 2>/dev/null) = three ]]; chk cl.9
rm -f cl_fd3
# cl.10 the same hazard one fd up: with 3 already open, `exec 4>` makes 4 the
# lowest free fd. Both files must get their own write.
run_sash 'exec 3> cl_fd3; exec 4> cl_fd4; /bin/echo a >&3; /bin/echo b >&4; close 3; close 4'
[[ $rc = 0 && -z $out && -z $err ]] &&
  [[ $(cat cl_fd3 2>/dev/null) = a && $(cat cl_fd4 2>/dev/null) = b ]]; chk cl.10
rm -f cl_fd3 cl_fd4
# cl.11 an ordinary scoped redirect still closes its temp fd — the guard must
# not leak the source fd in the src != fd case.
run_sash '/bin/echo scoped > cl_fd5; exec 3> cl_fd3; /bin/echo t >&3; close 3'
[[ $rc = 0 && -z $out && -z $err ]] &&
  [[ $(cat cl_fd5 2>/dev/null) = scoped && $(cat cl_fd3 2>/dev/null) = t ]]; chk cl.11
rm -f cl_fd3 cl_fd5


# ── `<&` and `<>` ────────────────────────────────────────────────────
# Both were ACCEPTED and silently did nothing: c dropped the redirect
# through a fail-open `/* TODO */` arm, and native had no arm for either
# op so the save happened and the redirect never did. The command then
# ran on the wrong fd. bash answers all four of these.

# ex.12 `<&N` duplicates an input fd, exactly as `>&N` does for output.
printf 'hello\n' > ddf
run_sash 'exec 9< ddf
readline line <&9
attest line [[ -n $line ]]
echo "got:$line"'
[[ $rc = 0 && $out = "got:hello" && -z $err ]]; chk ex.12

# ex.13 `<>` opens read/write; reading through it sees the content.
printf 'hello\n' > rwf
run_sash 'readline line <> rwf
attest line [[ -n $line ]]
echo "got:$line"'
[[ $rc = 0 && $out = "got:hello" && -z $err ]]; chk ex.13

# ex.14 `<>` defaults to fd 0, NOT fd 1 — it is a `<` form. So this
# writes to stdout and leaves the file alone. Defaulting it to 1 (the
# first implementation) silently overwrote the head of the file.
printf 'ABCDEFGH\n' > rwk
run_sash 'echo xy <> rwk'
[[ $rc = 0 && $out = "xy" && -z $err && $(cat rwk) = "ABCDEFGH" ]]; chk ex.14

# ex.15 `<>` creates a file that does not exist (and does not truncate,
# which ex.14 covers).
rm -f rwnew
run_sash 'echo made <> rwnew'
[[ $rc = 0 && -f rwnew && -z $err ]]; chk ex.15


# ── `exec CMD …` builds argv with the SHARED builder ──────────────────────
# Both backends had their own copy of the argv builder behind `exec`, each a
# stale fork of the one every other command used. Three faults followed from
# the duplication, all of them here:
#   - `@a` reached the scalar lowering and ICEd the COMPILER (both backends);
#   - `${x:~}` did the same (both backends);
#   - native's copy never called the DD#99 stage-7 NUL guard, so a NUL-bearing
#     argument was TRUNCATED at execve and the program exited 0 — the callee
#     received a shorter argument than the program computed and checked.
# The c backend did guard, so a differential spec would have caught the third
# the moment one existed. There was none: `exec` had no splat, elide, or NUL
# coverage at all.

# ex.16 a splat expands to one argv entry per element
run_sash 'array a=(x y)
exec /bin/echo A @{a:~}'
[[ $rc = 0 && $out = "A x y" && -z $err ]]; chk ex.16

# ex.17 an empty array contributes zero entries
run_sash 'array a=()
exec /bin/echo A @{a:~} B'
[[ $rc = 0 && $out = "A B" && -z $err ]]; chk ex.17

# ex.18 DD#84: an empty scalar is elided
run_sash 'str p=""
exec /bin/echo A ${p:~} B'
[[ $rc = 0 && $out = "A B" && -z $err ]]; chk ex.18

# ex.19 …and a non-empty one is kept
run_sash 'str p=Q
exec /bin/echo A ${p:~} B'
[[ $rc = 0 && $out = "A Q B" && -z $err ]]; chk ex.19

# ex.20 DD#99 stage 7: a NUL-bearing argument ABORTS rather than truncating.
# This is the one that was silently wrong on native: it printed `[abc]` and
# exited 0, dropping everything past the NUL.
run_sash 'nul=$'"'"'\x00'"'"'
s="abc${nul}HIDDEN"
exec /usr/bin/printf "[%s]" $s'
[[ $rc != 0 && -z $out && $rerr == *NUL* ]]; chk ex.20

# ex.21 …and so does a NUL inside a splat ELEMENT (mirrors an.2 for exec)
run_sash 'nul=$'"'"'\x00'"'"'
s="abc${nul}HID"
array a=($s x)
exec /usr/bin/printf "[%s]" @{a:~}'
[[ $rc != 0 && $rerr == *NUL* ]]; chk ex.21

# ex.22 control: the guard does not fire on a clean argument
run_sash 'exec /usr/bin/printf "[%s]" ok'
[[ $rc = 0 && $out = "[ok]" && -z $rerr ]]; chk ex.22

# Bare exec is a successful no-op, including its condition verdict and END.
run_sash 'exec; echo ok'
[[ $rc = 0 && $out = ok && -z $rerr ]]; chk ex.23
run_sash 'if exec { echo $(status) } else { echo failed }'
[[ $rc = 0 && $out = 0 && -z $rerr ]]; chk ex.24
run_sash 'END { echo end }; exec; echo body'
[[ $rc = 0 && $out = $'body\nend' && -z $rerr ]]; chk ex.25

run_sash 'if exec 9> ex_condition { echo $(status) } else { echo failed }; close 9'
[[ $rc = 0 && $out = 0 && -f ex_condition && -z $rerr ]]; chk ex.26

# Failed setup must replace an earlier true verdict, skip the body and roll back.
run_sash '/bin/true
if exec 9> ex_missing/file { echo bad } else { echo $(status) }
echo done'
[[ $rc = 0 && $out = $'1\ndone' && -z $rerr ]]; chk ex.27
run_sash 'END { echo end }; exec 9> ex_missing/file; echo bad'
[[ $rc != 0 && $out = end && $rerr == *'ex_missing/file: No such file or directory'* ]]; chk ex.28
run_sash '- exec 9> ex_missing/file; echo after'
[[ $rc != 0 && -z $out && $rerr == *'ex_missing/file: No such file or directory'* ]]; chk ex.29
run_sash 'END { echo end }; exec /bin/echo bad > ex_missing/file; echo after'
[[ $rc != 0 && $out = end && $rerr == *'ex_missing/file: No such file or directory'* ]]; chk ex.30
run_sash 'if exec /bin/echo bad 9> ex_missing/file { echo bad } else { echo $(status) }; echo done'
[[ $rc = 0 && $out = $'1\ndone' && -z $rerr ]]; chk ex.31

# Persistent redirects commit their backups instead of leaking one every time.
saved_soft_limit=$(ulimit -Sn)
ulimit -Sn 128
run_sash 'int n=0
while (( n<200 )) { exec 9> /dev/null; (( n++ )) }
close 9
readline line < /etc/hostname
attest line [[ -n $line ]]
echo ok'
ulimit -Sn "$saved_soft_limit"
[[ $rc = 0 && $out = ok && -z $rerr ]]; chk ex.32

# Discarding backups must not close a later explicit target in the same list.
run_sash 'exec > ex_stdout 3> ex_fd3
/bin/echo fd3 >&3
echo stdout
close 3'
[[ $rc = 0 && -z $out && -z $rerr &&
   $(cat ex_stdout) = stdout && $(cat ex_fd3) = fd3 ]]; chk ex.33

# Saving stdout must not make a closed dup source appear open.
run_sash '- close 3
if exec 1>&3 { echo wrong } else { echo $(status) }
echo restored'
[[ $rc = 0 && $out = $'1\nrestored' && -z $rerr ]]; chk ex.34

# A scoped redirect to an originally closed fd must close it again.
run_sash 'echo quiet 9> ex_scoped
if close 9 { echo leaked } else { echo restored }'
[[ $rc = 0 && $out = $'quiet\nrestored' && -z $rerr ]]; chk ex.35
run_sash 'echo scoped > ex_first > ex_second; echo restored'
[[ $rc = 0 && $out = restored && ! -s ex_first &&
   $(cat ex_second) = scoped && -z $rerr ]]; chk ex.36
run_sash 'if exec 9> ex_rollback 8> ex_missing/file { echo bad } else { echo caught }
if close 9 { echo leaked } else { echo closed }'
[[ $rc = 0 && $out = $'caught\nclosed' && -z $rerr ]]; chk ex.37

printf 'keep\n' > ex_keep
run_sash 'if exec 1>&9 > ex_keep { echo bad } else { echo caught }; echo restored'
[[ $rc = 0 && $out = $'caught\nrestored' && $(cat ex_keep) = keep && -z $rerr ]]; chk ex.38

# Neither a replacement nor an ordinary external may inherit private backups.
fd_scan='for fd in /proc/self/fd/*; do if [ "$fd" -ef ex_original ]; then echo leaked; exit 1; fi; done; echo clean'
run_sash "exec 9> ex_original; exec /bin/sh -c '$fd_scan' 9> ex_new"
[[ $rc = 0 && $out = clean && -z $rerr ]]; chk ex.39
run_sash "exec 9> ex_original; /bin/sh -c '$fd_scan' 9> ex_new; /bin/echo restored >&9; close 9"
[[ $rc = 0 && $out = clean && $(cat ex_original) = restored && -z $rerr ]]; chk ex.40

# Bare exec inside a temporary scope must not steal the invoker's diagnostics.
run_sash 'f() { exec }; f 2> ex_diag; false'
[[ $rc != 0 && -z $out && ! -s ex_diag && $rerr == *false* ]]; chk ex.41

run_sash 'END { echo end }; - close 3; exec 1>&3; echo bad'
[[ $rc != 0 && $out = end && $rerr == *'3: Bad file descriptor'* ]]; chk ex.42

# Numeric dup operands must fit the OS fd type, not fall back to fd 2.
run_sash 'exec 1>&2147483648'
[[ $rc != 0 && -z $out && $err == *SB4048* && $err == *'0..2147483647'* ]]; chk ex.43
run_sash 'exec 1>&184467440737095516160'
[[ $rc != 0 && -z $out && $err == *SB4048* ]]; chk ex.44
run_sash 'exec 9> ex_numeric; echo ok >&09; close 9'
[[ $rc = 0 && -z $out && $(cat ex_numeric) = ok && -z $rerr ]]; chk ex.45

# Invalid high descriptors must fail the actual redirect, not backup allocation.
ulimit -Sn 70
run_sash 'if exec > ex_no_backup 99> /dev/null { echo bad } else { echo $(status) }; echo restored'
[[ $rc = 0 && $out = $'1\nrestored' && -z $rerr ]]; chk ex.46
run_sash 'if exec 1>&99 { echo bad } else { echo $(status) }; echo restored'
ulimit -Sn "$saved_soft_limit"
[[ $rc = 0 && $out = $'1\nrestored' && -z $rerr ]]; chk ex.47
run_sash 'if exec 1>&2147483647 { echo bad } else { echo $(status) }; echo restored'
[[ $rc = 0 && $out = $'1\nrestored' && -z $rerr ]]; chk ex.48

# A function's close operands must not name its enclosing redirect's backups.
run_sash 'fn f { - close 3; - close 4; - close 5; - close 6; - close 7; - close 8; echo body }
END { echo end }
f > ex_function
echo after'
[[ $rc = 0 && $out = $'after\nend' && $(cat ex_function) = body && -z $rerr ]]; chk ex.49

# The invoker's channel is private too, even when source names its old number.
run_sash 'if close 100 { echo private } else { echo closed }'
[[ $rc = 0 && $out = closed && -z $rerr ]]; chk ex.50
run_sash '- close 102; false'
[[ $rc != 0 && -z $out && $rerr == *'false: command failed (exit 1)'* ]]; chk ex.51
run_sash 'echo main; END { if close 100 { echo private } else { echo closed } }'
[[ $rc = 0 && $out = $'main\nclosed' && -z $rerr ]]; chk ex.52

# A capture's saved stdout and memfd must survive closes inside its body.
run_sash 'str x=$(- close 3; - close 4; - close 5; - close 6; - close 7; - close 8; echo captured)
attest x [[ -n $x ]]
echo "[$x]"'
[[ $rc = 0 && $out = '[captured]' && -z $rerr ]]; chk ex.53

# The same applies to the parent-side read fd of a lastpipe loop.
run_sash 'int n=0
/usr/bin/printf "one\ntwo\n" | while readline line {
    attest line [[ -n $line ]]
    - close 3; - close 4; - close 5; - close 6; - close 7; - close 8
    (( n++ ))
}
echo $n'
[[ $rc = 0 && $out = 2 && -z $rerr ]]; chk ex.54

# Do not reserve unrelated numeric data: that exhausts a finite fd namespace.
numbers=$(seq 3 127 | tr '\n' ' ')
ulimit -Sn 128
run_sash "array nums=($numbers); echo \$(len nums) > ex_numbers; echo done"
ulimit -Sn "$saved_soft_limit"
[[ $rc = 0 && $out = done && $(cat ex_numbers) = 125 && -z $rerr ]]; chk ex.55

run_sash 'if exec 1>&100 { echo private } else { echo caught }; echo restored'
[[ $rc = 0 && $out = $'caught\nrestored' && -z $rerr ]]; chk ex.56

# A lastpipe reader must not leak into an external called from its body.
# The script's own fd 3/4 remain inheritable; the pipe uses unreserved 5/6.
run_sash "exec 3> /dev/null; exec 4> /dev/null
/usr/bin/printf 'one\n' | while readline line {
    attest line [[ -n \$line ]]
    /bin/sh -c 'test -e /proc/self/fd/3 && test -e /proc/self/fd/4 && test ! -e /proc/self/fd/5 && test ! -e /proc/self/fd/6 && echo clean'
}
close 3; close 4"
[[ $rc = 0 && $out = clean && -z $rerr ]]; chk ex.57

run_sash 'close 2147483648'
[[ $rc != 0 && -z $out && $err == *SB4052* && $err == *'0..2147483647'* ]]; chk cl.12
run_sash 'close 184467440737095516160'
[[ $rc != 0 && -z $out && $err == *SB4052* ]]; chk cl.13

set -e
tally
