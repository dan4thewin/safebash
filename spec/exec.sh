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
plan=33
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

set -e
tally
