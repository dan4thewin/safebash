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
# expansion target is rejected (SB5011), as are tainted args without bless
# (DD#24). `exec` is native-only for now (the c/janet backends emit no exec —
# task c-janet-exec-support), so this suite runs only in native mode.
set -eu
. common.sh
setup exec
[[ ${SASH_MODE:-native} != native ]] && exit 0
plan=11
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
# ex.9 a tainted arg to exec is rejected without bless (DD#24), like any command
run_sash 'x=$(getenv HOME); exec /bin/echo $x'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk ex.9
# ex.10 a blessed tainted arg is accepted
run_sash 'x=$(getenv HOME); bless x [[ -n $x ]]; exec /bin/echo $x'
[[ $rc = 0 && -n $out && -z $err ]]; chk ex.10
# ex.11 the exec target is recorded in the external manifest
out=$(sash --manifest -c 'exec grep x /etc/hostname' 2>/dev/null); rc=$?
[[ $rc = 0 && $out == *'/usr/bin/grep'* ]]; chk ex.11

set -e
tally
