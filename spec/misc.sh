#!/bin/bash
# spec/misc.sh — miscellaneous compiler tests (DD#5, DD#6, DD#15, DD#20, DD#32, DD#37, DD#38, DD#52)
set -eu
. common.sh
setup misc
plan=103
echo "1..$plan" >&3

set +e

# 5.1 $(cmd) inside heredoc → compile error
run_sash 'cat <<EOF
today is $(date)
EOF'
[[ $rc != 0 && -z $out && -n $err ]]; chk 5.1

# 6.1 ${!var} indirect expansion → compile error
run_sash 'x=HOME; echo ${!x}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 6.1

# ── DD#104: --emit-ir, the structural IR capture ──────────────────────────
# An inert data literal — data, not a program. These pin the CLI surface and
# shape; they deliberately do NOT shell out to an interpreter, so the suite
# carries no runtime dependency (the emitter's parseability is verified
# separately, over the whole corpus, not per-commit).
run_sash_emit_ir() {
    rc=-1 out="" err=""
    echo "$1" > .input.sash
    # `${SASH:-sash}`, not a bare `sash`: every other helper honours the
    # override, and this one silently did not. Standalone (`bash misc.sh`)
    # that meant these eight tests ran the *installed* binary rather than the
    # tree's, so a stale ~/.local/bin/sash failed 104.5 with the very
    # `:end-block nil` regression the test exists to catch — a failure with
    # nothing to do with the working tree. Under run.sh it happened to work,
    # because run.sh prepends target/release to PATH.
    timeout "$SPEC_TIMEOUT" ${SASH:-sash} --emit-ir .input.sash \
        </dev/null > out 2> err
    rc=$?
    out=$(<out)
    err=$(<err)
    rm -f .input.sash out err
}

# --- 104.x: --emit-ir capture ---
# native-only, and NOT a c-backend gap: the helper above runs `sash --emit-ir`
# directly, a backend-independent CLI surface that terminates before any
# codegen. c mode would re-run identical commands. Audited 2026-09-13.

# 104.1 --emit-ir accepted, and stamps a version a consumer can refuse on
if in_modes native; then
    run_sash_emit_ir 'echo hello'
    [[ $rc = 0 && -z $err && $out == *":ir-version 1"* ]]; chk 104.1
else
    skip_chk 104.1
fi

# 104.2 the capture is structural — IR node names, not an interpretation.
#       `echo` is a :command named "echo", not a dedicated :echo form.
if in_modes native; then
    run_sash_emit_ir 'echo hello'
    [[ $rc = 0 && $out == *":kind :command"* && $out == *'"echo"'* ]]; chk 104.2
else
    skip_chk 104.2
fi

# 104.3 arithmetic and loops are captured. A capture has no semantics to defer,
#       so it is complete from the start. This is the property that
#       distinguishes it.
if in_modes native; then
    run_sash_emit_ir 'int n=3
while (( n > 0 )); do
  n=$(( n - 1 ))
done'
    [[ $rc = 0 && $out == *":kind :while"* && $out == *":kind :arith-binop"* \
       && $out != *"TODO"* ]]; chk 104.3
else
    skip_chk 104.3
fi

# 104.5 an END block is captured in `:end-block`, not as a function in
#       `:statements`. The capture renders IR node names, never an
#       interpretation (DD#104), so this is only truthful because `END { }`
#       parses into `Program::end_block` — it used to emit a function named
#       `__sash_end__` and leave `:end-block nil`, which told a reader the
#       program had no exit handler. Journal 309.
if in_modes native; then
    run_sash_emit_ir 'echo hi
END {
echo bye
}'
    [[ $rc = 0 && -z $err && $out == *":end-block [{:kind :command"* \
       && $out != *"__sash_end__"* && $out != *":end-block nil"* ]]; chk 104.5
else
    skip_chk 104.5
fi

# 104.6-104.8 the capture never holds an arithmetic `++`/`--` or `a[i]`
# mutation node. `emit_ir` renders the post-validate AST, and by then every
# such form has been lowered away by `parse::lower_arith_command` (a top-level
# mutation, and a C-style `for` header) or rejected by validate (SB4020) —
# `ir::ARITH_IR_MUTATION_INVARIANT`. A probe over the whole sweep confirmed
# the two emitters were never reached, so they are gone and the arms ICE;
# these pin the lowering that keeps that true. A regression would surface as
# an ICE here rather than as a silently different capture.

# 104.6 `(( a++ ))` is captured as an assignment of `a + 1`, not an incdec node
if in_modes native; then
    run_sash_emit_ir 'int a=1; (( a++ )); echo $a'
    [[ $rc = 0 && -z $err && $out == *":kind :assignment"* && $out == *":kind :arith-binop"* && $out != *"incdec"* ]]; chk 104.6
else
    skip_chk 104.6
fi

# 104.7 `(( v[0] += 5 ))` likewise — an assignment whose value reads the cell,
#       never an `:arith-subscript-assign`
if in_modes native; then
    run_sash_emit_ir 'int array v=(1 2); (( v[0] += 5 )); echo ${v[0]}'
    [[ $rc = 0 && -z $err && $out == *":kind :assignment"* && $out == *":kind :arith-subscript"* && $out != *"arith-subscript-assign"* ]]; chk 104.7
else
    skip_chk 104.7
fi

# 104.8 a C-style `for` header is desugared at parse time into a group holding
#       the init assignment and a while, so its `i++` is an assignment too
if in_modes native; then
    run_sash_emit_ir 'int i=0; for (( i=0; i<2; i++ )); do echo $i; done'
    [[ $rc = 0 && -z $err && $out == *":kind :group"* && $out == *":kind :while"* && $out != *"incdec"* ]]; chk 104.8
else
    skip_chk 104.8
fi

# 104.4 a literal containing a quote and a backslash is escaped for the data
#       reader rather than terminating the string early.
#       The source must carry the quote in a single-quoted string: under DD#64
#       there is no `\\"` inside `"…"`, so the old spelling `x="a\\"b"` closed the
#       string at the backslash and left a third, unterminated `"` running to
#       end-of-source. That used to compile silently; it is SB4057 now.
if in_modes native; then
    run_sash_emit_ir "x='a\\\"b'; echo \$x"
    [[ $rc = 0 && $out == *'\\"'* ]]; chk 104.4
else
    skip_chk 104.4
fi

# 20.30 lastpipe always on
run_sash 'echo hello | readline x; echo $x'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 20.30

# 20.31 `case` is removed (DD#66) — rejected as a reserved word (this also
#      subsumes the old extglob `@()` reject, which lived inside a case arm)
run_sash 'case x in @(a|b)) echo yes;; esac'
[[ $rc != 0 && -z $out && -n $err ]]; chk 20.31

# 20.32 globstar ** rejected
run_sash 'for f in **/*.sh; do echo $f; done'
[[ $rc != 0 && -z $out && -n $err ]]; chk 20.32

# 20.33 pipefail always on
run_sash '/bin/false | /bin/true'
[[ $rc != 0 && -z $err ]]; chk 20.33

# --- 32.x: embedded source section ---
# native-only by construction, NOT a suppressed defect: the section lives in
# the ELF that Cranelift links. c mode produces a gcc-linked binary with no
# .sash_source at all, so there is nothing to assert. Audited 2026-09-13.

# 32.1 binary contains .sash_source section (native ELF only)
if in_modes native; then
    echo 'echo hello' > .input.sash
    sash .input.sash -o .binary 2>err
    rc=$?; err=$(<err)
    objdump -h .binary 2>/dev/null | grep -q sash_source
    [[ $? = 0 && $rc = 0 && -z $err ]]; chk 32.1
else
    skip_chk 32.1
fi

# 32.2 sash --extract dumps embedded source (native ELF only)
if in_modes native; then
    sash --extract .binary > out 2>err
    rc=$?; out=$(<out); err=$(<err)
    [[ $rc = 0 && $out == *"echo hello"* && -z $err ]]; chk 32.2
else
    skip_chk 32.2
fi

# 37.1 <(cmd) process substitution rejected
run_sash 'diff <(echo a) <(echo b)'
[[ $rc != 0 && -z $out && -n $err ]]; chk 37.1

# 37.2 >(cmd) process substitution rejected
run_sash 'echo hello > >(cat)'
[[ $rc != 0 && -z $out && -n $err ]]; chk 37.2

# 38.1 ( cmd ) subshell → compile error
run_sash '( echo hello )'
[[ $rc != 0 && -z $out && -n $err ]]; chk 38.1

# 38.4 background with &
run_sash 'echo bg &
wait'
[[ $rc = 0 && -z $err ]]; chk 38.4

# 38.5 $! stores last bg pid
run_sash '/bin/sleep 0 &
echo $!'
[[ $rc = 0 && -n $out && -z $err ]]; chk 38.5

# 52.1 ${var@U} rejected
run_sash 'x=hello; echo ${x@U}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 52.1

# 52.2 ${var@u} rejected
run_sash 'x=hello; echo ${x@u}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 52.2

# 52.3 ${var@L} rejected
run_sash 'x=hello; echo ${x@L}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 52.3

# 52.4 ${var@Q} rejected
run_sash 'x=hello; echo ${x@Q}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 52.4

# 52.5 ${var@E} rejected
run_sash 'x=hello; echo ${x@E}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 52.5

# 52.6a ${var^^} dropped (DD#65) — use $(toupper); rejects SB4034
run_sash 'x=hello; echo ${x^^}'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk 52.6a

# 52.6b ${var,,} dropped (DD#65) — use $(tolower); rejects SB4034
run_sash 'x=HELLO; echo ${x,,}'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk 52.6b

# --- DD#54 sash-expand pipeline ---

# 54.1 sash-expand output parsed into a config map (DD#99: `mapfile -m` is
# replaced by `matchall` + array->map, which also splits on the FIRST `=`
# only, so a value may contain one).
printf 'NAME=Alice\nGREETING=Hello ${NAME}\n' > /tmp/sash_test_config.conf
EXPAND=$(which sash-expand)
run_sash "s=\$($EXPAND /tmp/sash_test_config.conf); attest s [[ -n \$s ]]; - matchall \$s \"([^=\\x0a]*)=([^\\x0a]*)\" p; map config = @{p:~}; echo \${config[GREETING]}"
[[ $rc = 0 && $out = "Hello Alice" && -z $err ]]; chk 54.1

# --- DD#75 §1: punctuation specials are sugar for the keyword forms ---
# Both spellings compile to the same node/behaviour: $$ ≡ $(getpid),
# $? ≡ $(status), $! ≡ $(getcpid). The keyword form is canonical (docs,
# diagnostics, sash-convert); the punctuation form is accepted input sugar.
# spv.1 $$ and $(getpid) are the same value (this process's pid)
run_sash 'p=$$; q=$(getpid); [[ $p == $q ]] && echo Y'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk spv.1
# spv.2 $? sugar reads the last status (arith-false → 1)
run_sash '(( 0 )); echo $?'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk spv.2
# spv.3 $(status) keyword reads the same status
run_sash '(( 0 )); echo $(status)'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk spv.3
# spv.4 $! and $(getcpid) are the same value (empty with no background job)
run_sash 'x=$!; y=$(getcpid); [[ $x == $y ]] && echo Y'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk spv.4

# ── Heap exhaustion and slurp limits (bug-heap-exhaustion-silent-segv) ──
# The runtime heap is a fixed region and heap_4 returns NULL when it is
# exhausted. Nothing checked, so the next write dereferenced NULL and the
# program died with SIGSEGV, silently. These pin "fail loudly"; DD#107
# settles the capacity question (256 MB default, `--heap-size` to change it).
#
# hp.1-hp.3 compile with a deliberately tiny heap. Exhaustion is then a
# property of the *stated* ceiling rather than of whatever the default
# happens to be, so raising the default cannot silently stop testing the
# failure path — which is exactly what the old, capacity-dependent form
# would have done when DD#107 landed.
  /usr/bin/printf 'a\0%.0s' $(seq 1 20000) > hp_big.nul
  /usr/bin/printf 'x%.0s' $(seq 1 70000) > hp_wide.nul
  /usr/bin/printf '\0' >> hp_wide.nul
  /usr/bin/printf 'y%.0s' $(seq 1 30000) >> hp_wide.nul

  export SASH_CFLAGS=--heap-size=1M

  # hp.1 exceeding the heap is a clean diagnostic, not a segfault. 139 is
  # SIGSEGV; the point of the test is that the status is NOT 139.
  run_sash 'array a=()
int i=0
while (( i < 30000 )) {
  a[$i]="xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
  i=$(( i + 1 ))
}
echo $(len a)'
  [[ $rc != 0 && $rc != 139 && $rerr == *"out of memory"* ]]; chk hp.1

  # hp.2 ...and nothing reaches stdout, so a partial result is never
  # mistaken for a complete one
  [[ -z $out ]]; chk hp.2

  # hp.3 a bulk `read` past the heap goes the same way rather than segfaulting
  run_sash 'array a=()
read a < hp_big.nul
echo $(len a)'
  [[ $rc != 0 && $rc != 139 && $rerr == *"out of memory"* ]]; chk hp.3

  unset SASH_CFLAGS

  # hp.4 `sash_read_all` used to stop at a fixed 64 KiB, silently handing
  # back a prefix. A 100 KB stream with one separator must yield BOTH
  # elements — under the cap the tail was simply gone.
  run_sash 'array a=()
read a < hp_wide.nul
attest x in a [[ -n $x ]]
str e="${a[1]}"
echo "$(len a) $(strlen $e)"'
  [[ $rc = 0 && $out = "2 30000" && -z $err ]]; chk hp.4

  # hp.5 ...and the first element is its full 70000 bytes, not 65536
  run_sash 'array a=()
read a < hp_wide.nul
attest x in a [[ -n $x ]]
str e="${a[0]}"
echo $(strlen $e)'
  [[ $rc = 0 && $out = "70000" && -z $err ]]; chk hp.5

  # hp.6 DD#107: the default reservation absorbs 50k elements. The same
  # program is an out-of-memory abort under hp.1's 1 MB heap, so this pins
  # the raised default and not merely "some allocation succeeded".
  run_sash 'array a=()
int i=0
while (( i < 50000 )) {
  a[$i]="xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
  i=$(( i + 1 ))
}
echo $(len a)'
  [[ $rc = 0 && $out = "50000" && -z $rerr ]]; chk hp.6

  # hp.7 ...and `--heap-size` moves that ceiling: 30000 elements abort under
  # 1 MB (hp.1) and succeed under 16 MB. The flag is compiled in, so this
  # also pins that the emitted strong `sash_heap_size` displaces the weak
  # default in heap4.c at link time, in whichever backend is under test.
  SASH_CFLAGS=--heap-size=16M run_sash 'array a=()
int i=0
while (( i < 30000 )) {
  a[$i]="xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
  i=$(( i + 1 ))
}
echo $(len a)'
  [[ $rc = 0 && $out = "30000" && -z $rerr ]]; chk hp.7

# ── Command-substitution capture (sash_capture_drain) ──────────────────
# Three runtime functions and one inline codegen path each had their own
# copy of "drain the pipe and apply bash's $( ) rule", and the copies had
# drifted apart and away from bash. They now share `sash_capture_drain`.
  /usr/bin/printf 'x%.0s' $(seq 1 100000) > cap_big.txt

  # cap.1 a capture larger than the 64 KiB pipe buffer returns all of it.
  # The old fixed 64 KiB reader stopped there, so this did not truncate
  # quietly — the reader went away, the child took SIGPIPE, and the whole
  # program died with exit 141.
  run_sash 'str x=$(/bin/cat cap_big.txt)
attest x [[ -n $x ]]
echo $(strlen $x)'
  [[ $rc = 0 && $out = "100000" && -z $rerr ]]; chk cap.1

  # cap.2 ...and specifically not the SIGPIPE status
  [[ $rc != 141 ]]; chk cap.2

  # cap.3 bash strips EVERY trailing newline from $( ), not just one.
  # `$(printf 'a\n\n\n')` is "a" in bash; this returned "a\n\n" (len 3).
  run_sash 'str x=$(/bin/printf "a\n\n\n")
attest x [[ -n $x ]]
echo $(strlen $x)'
  [[ $rc = 0 && $out = "1" && -z $rerr ]]; chk cap.3

  # cap.4 output that is nothing but newlines strips to the empty string
  # rather than keeping the ones past the first
  run_sash 'str x=$(/bin/printf "\n\n\n")
attest x [[ -z $x ]]
echo "[$x]"'
  [[ $rc = 0 && $out = "[]" && -z $rerr ]]; chk cap.4

  # cap.5 interior newlines are untouched — only *trailing* ones go
  run_sash 'str x=$(/bin/printf "a\nb\n\n")
attest x [[ -n $x ]]
echo $(strlen $x)'
  [[ $rc = 0 && $out = "3" && -z $rerr ]]; chk cap.5

  # cap.6 capturing a user function's stdout goes through a different
  # runtime entry point (sash_capture_end). A NON-REGRESSION guard, not a
  # fixed bug: that path already stripped newlines correctly, and it
  # passes on both sides of the fix. It guards the reroute onto the shared
  # drain. The 64 KiB half of that path is cap.7/cap.8 below, which only
  # became reachable once the deadlock was gone.
  run_sash 'fn emit {
  /bin/printf "hello\n\n\n"
}
str x=$(emit)
attest x [[ -n $x ]]
echo $(strlen $x)'
  [[ $rc = 0 && $out = "5" && -z $rerr ]]; chk cap.6

  # A captured function body runs *in-process*: sash_capture_start points
  # fd 1 at a sink, the body runs, and sash_capture_end reads afterwards.
  # Nothing drains while the body writes. With a pipe as the sink that is a
  # deadlock, not a truncation — past the 64 KiB pipe buffer the writer
  # blocks forever waiting for a reader that cannot start until it returns.
  # The sink is a memfd for exactly this reason: an ordinary file never
  # blocks, but is still an fd, so redirects and forked children still work.
  # Pre-fix these three hit the hang guard (rc 124), not a wrong answer.

  # cap.7 in-process writes past the pipe buffer: ~450 KB from echo alone.
  # This body is *pure*, so it also exercises userspace buffer growth from
  # the 256-byte initial allocation up to ~500 KB.
  run_sash 'fn emit {
  int i=0
  while (( i < 20000 )); do
    echo "line-of-text-number-$i"
    (( i++ ))
  done
}
str x=$(emit)
attest x [[ -n $x ]]
echo $(strlen $x)'
  [[ $rc = 0 && $out = "508889" && -z $rerr ]]; chk cap.7

  # cap.8 the same wall via an *external* command inside the body. The
  # original filing named only in-process writes; a forked child inherits
  # fd 1 and blocks on the same buffer, so this hung too.
  run_sash 'fn emit {
  /bin/cat cap_big.txt
}
str x=$(emit)
attest x [[ -n $x ]]
echo $(strlen $x)'
  [[ $rc = 0 && $out = "100000" && -z $rerr ]]; chk cap.8

  # cap.9 ...and not by silently swallowing the hang guard
  [[ $rc != 124 ]]; chk cap.9

  # cap.10 a redirect *inside* a captured body still reaches the file, and
  # its output stays out of the capture. A NON-REGRESSION guard: it passes
  # on both sides of the fix. It pins why the sink stays an fd rather than
  # becoming a userspace buffer — a buffer keyed on "fd == 1" would swallow
  # the redirected write and lose the file, and nothing else would notice.
  run_sash 'fn emit {
  echo hidden > cap_redir.txt
  echo visible
}
str x=$(emit)
attest x [[ -n $x ]]
str f=$(/bin/cat cap_redir.txt)
attest f [[ -n $f ]]
echo "[$x][$f]"'
  [[ $rc = 0 && $out = "[visible][hidden]" && -z $rerr ]]; chk cap.10

  # cap.11 in-process and forked writes interleave in issue order. Also a
  # NON-REGRESSION guard — a pipe ordered these correctly too. It pins the
  # property the memfd has to preserve: fd 1 and the capture handle share
  # one file description, so a child's writes advance the same offset the
  # parent's do. Two independent handles would silently overwrite.
  run_sash 'fn emit {
  echo one
  /bin/echo two
  echo three
}
str x=$(emit)
attest x [[ -n $x ]]
echo "[$x]"'
  [[ $rc = 0 && $out = "[one
two
three]" && -z $rerr ]]; chk cap.11

  # cap.12 / cap.17 are a PAIR. They pin a split that is deliberately
  # OBSERVABLE: what a captured body sees when it probes its own stdout
  # depends on which capture path the compiler chose for it.
  #
  # A *pure* body — one that provably never forks onto this capture's
  # stdout and never rebinds fd 1 — is collected in a userspace buffer.
  # Nothing is redirected at all; `sash_write` diverts at the fd-1
  # boundary. So fd 1 is still the caller's real stdout, and the body sees
  # whatever the caller had. An *impure* body still points fd 1 at a memfd,
  # and sees a regular file.
  #
  # The tempting alternative was to hide this by disqualifying any body
  # that mentions /dev/stdout or uses -t, so the optimization would be
  # unobservable. DG rejected that: the two paths genuinely differ, and a
  # rule that only conceals the difference where someone might look is a
  # taboo, not a design. Pin both halves instead, out loud.
  #
  # cap.12 pure body: fd 1 is the caller's stdout. Under the spec harness
  # that is a pipe, so this prints PIPE — which is also what bash prints.
  # (Agreeing with bash is a side effect here, not the goal.)
  run_sash 'fn emit {
  if [[ -p /dev/stdout ]]; then
    echo PIPE
  else
    echo NOTPIPE
  fi
}
str x=$(emit)
attest x [[ -n $x ]]
echo "[$x]"'
  [[ $rc = 0 && $out = "[PIPE]" && -z $rerr ]]; chk cap.12

  # cap.17 the other half: the SAME probe in a body disqualified from the
  # buffer by a bare external. fd 1 is a memfd, so the probe sees a regular
  # file and prints NOTPIPE where bash prints PIPE. Adding or removing the
  # external is what moves this answer — that is the split, stated.
  # This half passes on both sides of the fast path (it is the behaviour
  # every capture had before); cap.12 is the one that moved.
  run_sash 'fn emit {
  /bin/true
  if [[ -p /dev/stdout ]]; then
    echo PIPE
  else
    echo NOTPIPE
  fi
}
str x=$(emit)
attest x [[ -n $x ]]
echo "[$x]"'
  [[ $rc = 0 && $out = "[NOTPIPE]" && -z $rerr ]]; chk cap.17

  # Nested capture: `o=$(h)` where `h` itself contains `i=$(g)`. Both
  # backends held the capture handles in ONE pair of slots, so the inner
  # CaptureStart overwrote the outer's and the outer CaptureEnd restored an
  # already-closed fd — `sash: read failed`, rc 1. Each capture site now
  # owns its own slot pair. Depth is always static: sash inlines functions
  # and rejects recursion (SB5037), so no runtime stack is needed.

  # cap.13 two deep
  run_sash 'fn g {
  echo inner
}
fn h {
  str i=$(g)
  attest i [[ -n $i ]]
  echo "outer-$i"
}
str o=$(h)
attest o [[ -n $o ]]
echo "[$o]"'
  [[ $rc = 0 && $out = "[outer-inner]" && -z $rerr ]]; chk cap.13

  # cap.14 three deep — one level would pass with a save/restore hack that
  # still only has one slot pair; this needs genuinely distinct slots
  run_sash 'fn a {
  echo A
}
fn b {
  str x=$(a)
  attest x [[ -n $x ]]
  echo "B-$x"
}
fn c {
  str y=$(b)
  attest y [[ -n $y ]]
  echo "C-$y"
}
str z=$(c)
attest z [[ -n $z ]]
echo "[$z]"'
  [[ $rc = 0 && $out = "[C-B-A]" && -z $rerr ]]; chk cap.14

  # cap.15 an inner capture of an EXTERNAL inside an outer function capture.
  # A NON-REGRESSION guard: it passes on both sides, because `$(cmd)` on an
  # external does not use CaptureStart/CaptureEnd at all — it forks and
  # drains (emit_cmdsub_exec), so it never consumed a capture site and could
  # not collide. Kept because that asymmetry is easy to forget, and because
  # it is exactly the shape the buffer fast path has to reason about: the
  # outer body yields its value with echo, the external output arrives via
  # an inner capture, so the outer never has a fork on its own stdout.
  run_sash 'fn wrap {
  str t=$(/bin/printf "ext")
  attest t [[ -n $t ]]
  echo "got-$t"
}
str o=$(wrap)
attest o [[ -n $o ]]
echo "[$o]"'
  [[ $rc = 0 && $out = "[got-ext]" && -z $rerr ]]; chk cap.15

  # cap.16 two sibling (non-nested) captures in sequence. Also a guard —
  # sequential reuse of one slot pair was always fine. It pins that
  # per-site numbering did not break the common case.
  run_sash 'fn p {
  echo one
}
fn q {
  echo two
}
str a=$(p)
str b=$(q)
attest a [[ -n $a ]]
attest b [[ -n $b ]]
echo "[$a][$b]"'
  [[ $rc = 0 && $out = "[one][two]" && -z $rerr ]]; chk cap.16

  # cap.18 an IMPURE capture nested inside a PURE one, where the inner body
  # also writes IN-PROCESS. This is the case the runtime capture stack
  # exists for, and the in-process write is the whole point of it.
  #
  # The outer site is buffered, so `sash_write` is diverting fd-1 writes
  # into memory. The inner site then genuinely redirects fd 1 to a memfd —
  # so for its duration the diversion must stop, or the inner `echo` lands
  # in the OUTER string and the inner capture comes back empty.
  # `sash_capture_start` pushes a transparent frame to suspend it, and
  # `sash_capture_end` pops.
  #
  # The `/bin/true` is load-bearing: it is what disqualifies `deep` from
  # the buffer. The `echo` is equally load-bearing, and this test was
  # VACUOUS without it — an earlier draft used `/bin/cat` as the whole
  # inner body and passed with the suspend deleted, because an external's
  # writes go straight to fd 1 from a forked child and never pass through
  # `sash_write` at all. Only an in-process write can be mis-diverted.
  run_sash 'fn deep {
  /bin/true
  echo inner-payload
}
fn outer {
  str d=$(deep)
  attest d [[ -n $d ]]
  echo "outer-$d"
}
str o=$(outer)
attest o [[ -n $o ]]
echo "[$o]"'
  [[ $rc = 0 && $out = "[outer-inner-payload]" && -z $rerr ]]; chk cap.18

  # cap.19 a capture of a function that BARE-CALLS another function. The
  # purity walk has to recurse through `outer`'s body into `inner`'s to
  # decide, and it can: the capture path looks the callee's body up and
  # hands it to the analysis UNLOWERED, before inlining has flattened it.
  # `inner` writes in-process, so the site is buffered.
  #
  # No spec reached that recursion before these three — the walk sat at 0
  # executions in a full sweep, and the comment on it read "this lookup
  # never succeeds", which is a true statement about the suite and a
  # misleading one about the code.
  run_sash 'fn inner {
  echo hi
}
fn outer {
  inner
}
str t=$(outer)
echo "[$t]"'
  [[ $rc = 0 && $out = "[hi]" && -z $rerr ]]; chk cap.19

  # cap.20 the same shape, disqualified — and the load-bearing one, because
  # cap.19 passes whichever way the analysis answers. Here `inner` wraps an
  # EXTERNAL, whose child inherits our stdout, so the buffer cannot see its
  # bytes. If the walk stopped at `outer`'s body instead of recursing, the
  # site would be judged pure, `/bin/echo`'s output would leak straight to
  # the real fd 1, and `$t` would come back empty.
  #
  # Verified rather than assumed: hand-patching the emitted C onto the
  # buffered path prints "hi\n[]" where this prints "[hi]".
  #
  # The bracketed form is what makes that visible. With a bare `echo "$t"`
  # the wrong answer prints "hi\n\n" and the harness's $( ) strips both
  # trailing newlines back to "hi" — a vacuous pass. (cap.18 documents the
  # same trap from the other direction.)
  run_sash 'fn inner {
  /bin/echo hi
}
fn outer {
  inner
}
str t=$(outer)
echo "[$t]"'
  [[ $rc = 0 && $out = "[hi]" && -z $rerr ]]; chk cap.20

  # cap.21 the module doc's headline case, reached through a bare call: a
  # body that CONSUMES external output and yields it with `echo` stays pure,
  # because the inner $( ) hands its child a pipe of its own and nothing but
  # our own `echo` ever reaches our stdout. Nested captures disqualify
  # nothing in either direction.
  run_sash 'fn wrap {
  str u=$(/bin/echo inner)
  attest u [[ -n $u ]]
  echo "got-$u"
}
fn outer {
  wrap
}
str t=$(outer)
echo "[$t]"'
  [[ $rc = 0 && $out = "[got-inner]" && -z $rerr ]]; chk cap.21
# ── `$(<file)` — the slurp form (DD-less; see dollar-lt-file-forks) ──
# It used to be desugared at PARSE time into `$(cat file)`, so a supposedly
# self-contained static binary acquired a runtime dependency on /usr/bin/cat
# and forked a process. The whole reason the spelling exists in bash is that
# it does NOT fork.

printf 'one\ntwo\n' > sf.txt

# sl.1 the value is unchanged by the rewrite: contents with every trailing
# newline stripped, exactly as a `$( )` capture strips them.
run_sash 's=$(<sf.txt)
attest s [[ -n $s ]]
echo "[$s]"'
[[ $rc = 0 && $out = "[one
two]" && -z $err ]]; chk sl.1

# sl.2 THE point of the change, asserted through behaviour the harness can
# see in both modes: a failed slurp reports the FILE, and never `cat` or the
# DD#55 "command failed" wording. Both of those can only appear if an external
# command was forked, so their absence is the no-fork property. (Grepping the
# binary for `/usr/bin/cat` would be more direct, but the harness deletes
# test_bin before the assertion runs, and the C path builds it under a
# different name.)
run_sash 's=$(</nonexistent/sash/slurp)
attest s [[ -n $s ]]
echo "[$s]"'
[[ $rc != 0 && $rerr != *cat* && $rerr != *"command failed"* ]]; chk sl.2

# sl.3 strips EVERY trailing newline, not one — shared with the capture path
# (`sash_capture_drain`) rather than reimplemented, so the two cannot drift.
printf 'x\n\n\n' > sn.txt
run_sash 's=$(<sn.txt)
attest s [[ -n $s ]]
echo "len=$(strlen $s)"'
[[ $rc = 0 && $out = "len=1" && -z $err ]]; chk sl.3

# sl.4 a file with no trailing newline at all is not truncated.
printf 'no-newline' > sw.txt
run_sash 's=$(<sw.txt)
attest s [[ -n $s ]]
echo "len=$(strlen $s)"'
[[ $rc = 0 && $out = "len=10" && -z $err ]]; chk sl.4

# sl.5 an empty file yields the empty string (and `${s:!}` is the sanctioned
# way to use a value that is legitimately empty).
: > se.txt
run_sash 's=$(<se.txt)
echo "len=$(strlen ${s:!})"'
[[ $rc = 0 && $out = "len=0" && -z $err ]]; chk sl.5

# sl.6 the path may be an expansion, not just a literal.
run_sash 'f=sf.txt
s=$(<$f)
attest s [[ -n $s ]]
echo "[$s]"'
[[ $rc = 0 && $out = "[one
two]" && -z $err ]]; chk sl.6

# sl.7 THE TAINT GATE. File contents are external input. While the form
# desugared to `$(cat …)` this was inherited for free from being an external
# capture; a direct read leaves that path, so taint must be re-asserted. A
# regression here is a hole straight through the taint model, not a cosmetic
# slip.
run_sash 's=$(<sf.txt)
echo "$s"'
[[ $rc != 0 && $err == *"SB5020"* ]]; chk sl.7

# sl.8 ...and in a `[[ ]]` operand too, which is a read position. This one
# regressed during the rewrite (the taint dispatcher has a `_` fall-through,
# so the new node was silently LEGAL there) and is the reason the test exists.
run_sash 'if [[ $(<sf.txt) == "one" ]]; then echo y; else echo n; fi'
[[ $rc != 0 && $err == *"SB5020"* ]]; chk sl.8

# sl.9 a missing file ABORTS rather than yielding an empty string. bash warns
# and continues with ""; sash will not hand the program a value it did not
# read. The diagnostic names the FILE (it used to name /usr/bin/cat).
run_sash 's=$(</nonexistent/sash/slurp)
attest s [[ -n $s ]]
echo "[$s]"'
[[ $rc = 1 && $rerr == *"/nonexistent/sash/slurp"* && $rerr == *"No such file"* ]]; chk sl.9

# sl.10 a directory also aborts with the path named. `open` SUCCEEDS on a
# directory (it is `read` that fails EISDIR), so without an explicit check
# this fell through to a generic "read failed" naming nothing.
mkdir -p sdir
run_sash 's=$(<sdir)
attest s [[ -n $s ]]
echo "[$s]"'
[[ $rc = 1 && $rerr == *"sdir"* && $rerr == *"Is a directory"* ]]; chk sl.10

# sl.11 A FILE WHOSE NAME LOOKS LIKE AN OPTION. This is the sharpest
# discriminator, and a third bug the desugar caused: `$(<-n)` became
# `cat -n`, so `-n` was parsed as a FLAG, cat read stdin instead, and the
# slurp yielded nothing. bash reads the file. Any filename reaching a real
# argv is an argument-injection surface; a direct open has no argv at all.
printf 'SECRET\n' > ./-n
run_sash 's=$(<-n)
attest s [[ -n $s ]]
echo "[$s]"'
[[ $rc = 0 && $out = "[SECRET]" && -z $err ]]; chk sl.11

# ---- non-ASCII source bytes round-trip (mojibake bug) ----------------------
# Every byte-at-a-time literal scanner built its String with
# `push(byte as char)`, which reads a byte as a Latin-1 code point and
# re-encodes it: 0xC3 became U+00C3 = C3 83. So `caf<C3><A9>` in the source
# became `caf<C3><83><C2><A9>` in the binary -- every non-ASCII literal
# outside single quotes was double-encoded. It survived because the
# corruption is self-consistent: two corrupted strings still compare equal,
# so only output bytes (or a byte length) reveal it.

# utf8.1 unquoted word
run_sash 'echo café'
[[ $rc = 0 && $out = "café" && -z $err ]]; chk utf8.1
# utf8.2 double-quoted
run_sash 'echo "café"'
[[ $rc = 0 && $out = "café" && -z $err ]]; chk utf8.2
# utf8.3 single-quoted -- the one path that was already correct (it slices the
# source range and decodes once). Pinned so a future refactor keeps it.
run_sash "echo 'café'"
[[ $rc = 0 && $out = "café" && -z $err ]]; chk utf8.3
# utf8.4 through an assignment and an expansion
run_sash 'v=café
echo $v'
[[ $rc = 0 && $out = "café" && -z $err ]]; chk utf8.4
# utf8.5 embedded in a longer double-quoted string with an expansion
run_sash 'v=café
echo "x $v y"'
[[ $rc = 0 && $out = "x café y" && -z $err ]]; chk utf8.5
# utf8.6 byte length, not character count: strlen is documented as the BYTE
# length. This reported 7 for a 5-byte string.
run_sash 'v=café
echo $(strlen $v)'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk utf8.6
# utf8.7 a 3-byte sequence (CJK) and a 4-byte one (emoji)
run_sash 'echo 日🎉'
[[ $rc = 0 && $out = "日🎉" && -z $err ]]; chk utf8.7
# utf8.8 a non-ASCII character in $'...' is written `\uHHHH`, and must be
# emitted as its UTF-8 encoding -- two bytes here, not four. This used to be
# spelled `$'caf\xc3\xa9'`; `\xNN` is now capped at `\x7f` (journal 371) and
# `\u` is the way to name a character above ASCII.
run_sash "echo \$'caf\\u00e9'"
[[ $rc = 0 && $out = "café" && -z $err ]]; chk utf8.8
# utf8.9 $'...' ASCII escapes still work alongside the byte path
run_sash "echo \$'a\\tb'"
[[ $rc = 0 && $out = "$(printf 'a\tb')" && -z $err ]]; chk utf8.9
# utf8.10 equality still holds -- it did even while both sides were corrupt,
# which is exactly why this went unnoticed
run_sash 'v=café
if [[ $v == café ]]; then echo ok; else echo no; fi'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk utf8.10
# utf8.11 the regex engine is BYTE-oriented (lib9.h's chartorune is an
# ASCII-only stub), so `.` matches one byte, not one character. bash in a
# UTF-8 locale matches one character. A real divergence, and now a coherent
# one: it used to disagree with BOTH models because the subject was
# double-encoded.
run_sash 'v=é
if [[ $v =~ ^..$ ]]; then echo two-bytes; else echo no; fi'
[[ $rc = 0 && $out = "two-bytes" && -z $err ]]; chk utf8.11

# --- esc.1-esc.18: the $'...' escape set (DD#115) --------------------------
# `simple_escape` was a near-entirely cold table; probing it found `$'\xff'`
# emitting U+FFFD, and five more silent divergences from bash. The set is now
# DD#72-shaped: a clean modern spelling is kept, archaic bash creep is a clean
# SB4063, and anything bash does not read as an escape still passes through.

# esc.1 the named escapes below 0x80 are kept and match bash exactly.
run_sash "printf '%s' \$'a\\nb\\tc'"
[[ $rc = 0 && $out = "$(printf 'a\nb\tc')" && -z $err ]]; chk esc.1
# esc.2 the rarer named escapes -- \a \b \e \f \v \r -- are kept too.
run_sash "printf '%s' \$'\\a\\b\\e\\f\\v\\r' | /usr/bin/od -An -tx1"
[[ $rc = 0 && $out = " 07 08 1b 0c 0b 0d" && -z $err ]]; chk esc.2
# esc.3 the three self-escapes. `\'` is the only way to get a quote inside.
run_sash "printf '%s' \$'x\\\\y\\'z\\\"w'"
[[ $rc = 0 && $out = 'x\y'"'"'z"w' && -z $err ]]; chk esc.3
# esc.4 `\xNN` names a BYTE. One or two hex digits.
run_sash "printf '%s' \$'\\x41\\x7a'"
[[ $rc = 0 && $out = "Az" && -z $err ]]; chk esc.4
# esc.5 `\x7f` is the cap, and is accepted -- the boundary is inclusive.
run_sash "printf '%s' \$'\\x7f' | /usr/bin/od -An -tx1"
[[ $rc = 0 && $out = " 7f" && -z $err ]]; chk esc.5
# esc.6 `\uHHHH` reads UP TO four hex digits, matching bash -- `\u41` is `A`.
run_sash "printf '%s' \$'\\u41'"
[[ $rc = 0 && $out = "A" && -z $err ]]; chk esc.6
# esc.7 ...and stops at four, so the rest is literal text: `\u0041BC` = "ABC".
run_sash "printf '%s' \$'\\u0041BC'"
[[ $rc = 0 && $out = "ABC" && -z $err ]]; chk esc.7
# esc.8 `\UHHHHHHHH` reads up to eight, reaching beyond the BMP.
run_sash "printf '%s' \$'\\U0001F600'"
[[ $rc = 0 && $out = "😀" && -z $err ]]; chk esc.8
# esc.9 a character above ASCII emits its UTF-8 encoding, not one byte each.
run_sash "printf '%s' \$'\\u00e9' | /usr/bin/od -An -tx1"
[[ $rc = 0 && $out = " c3 a9" && -z $err ]]; chk esc.9
# esc.10 what bash does not read as an escape stays literal, byte for byte.
# No divergence to fix, so no reason to reject.
run_sash "printf '%s' \$'\\q\\z9'"
[[ $rc = 0 && $out = '\q\z9' && -z $err ]]; chk esc.10
# esc.11 bare octal `\101` is REJECTED. sash never read it as bash does (bash
# takes 1-3 digits total); `\x41` says it exactly.
run_sash "echo \$'\\101'"
[[ $rc != 0 && -z $out && $err == *SB4063* ]]; chk esc.11
# esc.12 `\0101` likewise. This is the `010`/`16#ff` of DD#72 all over again:
# bash reads `\010` then `1`, sash read all four digits. Ambiguous by nature.
run_sash "echo \$'\\0101'"
[[ $rc != 0 && -z $out && $err == *SB4063* ]]; chk esc.12
# esc.19 `$'\0'` -- the NUL escape, and the most-used member of the octal
# family -- is rejected too. `\0` IS the octal form, so carving it out would
# contradict the rule; `\x00` says it exactly and is an exact replacement.
# Ten spec sites migrated (journal 371).
run_sash "echo \$'\\0'"
[[ $rc != 0 && -z $out && $err == *SB4063* ]]; chk esc.19
# esc.20 ...and `$'\x00'` is that replacement: it still builds the interior
# NUL, so the DD#55 argv abort still fires rather than truncating.
run_sash 'nul=$'"'"'\x00'"'"'
s="abc${nul}HID"
/usr/bin/printf "[%s]" $s'
[[ $rc != 0 && -z $out && $rerr == *NUL* ]]; chk esc.20
# esc.13 `\cX` control notation is REJECTED -- `\x01` says it better.
run_sash "echo \$'\\cA'"
[[ $rc != 0 && -z $out && $err == *SB4063* ]]; chk esc.13
# esc.14 `\x80` -- one past the cap. A lone byte above ASCII is not UTF-8
# text. This used to emit U+FFFD, silently.
run_sash "echo \$'\\x80'"
[[ $rc != 0 && -z $out && $err == *SB4063* ]]; chk esc.14
# esc.15 `\xff` -- the case that started this. DD#64 promised "any byte via
# \xNN"; it delivered `ef bf bd`. Now it is a diagnostic. DD#115 amends DD#64.
run_sash "echo \$'\\xff'"
[[ $rc != 0 && -z $out && $err == *SB4063* ]]; chk esc.15
# esc.16 a prefix with no hex digits is REJECTED, as DD#72 rejects a bare
# `0x`. bash passes `\x` through literally; a loud error beats a silent one.
run_sash "echo \$'\\x'"
[[ $rc != 0 && -z $out && $err == *SB4063* ]]; chk esc.16
# esc.17 a surrogate is not a character. bash encodes `\ud800` anyway and
# emits `ed a0 80` -- invalid UTF-8 that no conforming reader accepts.
run_sash "echo \$'\\ud800'"
[[ $rc != 0 && -z $out && $err == *SB4063* ]]; chk esc.17
# esc.18 above U+10FFFF is not a character either; bash emits `f4 90 80 80`.
run_sash "echo \$'\\U00110000'"
[[ $rc != 0 && -z $out && $err == *SB4063* ]]; chk esc.18


set -e
tally
