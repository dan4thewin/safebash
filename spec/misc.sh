#!/bin/bash
# spec/misc.sh — miscellaneous compiler tests (DD#5, DD#6, DD#15, DD#20, DD#32, DD#37, DD#38, DD#52)
set -eu
. common.sh
setup misc
plan=34
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
# An inert janet data literal — data, not a program. These pin the CLI
# surface and the shape; they deliberately do NOT shell out to janet, so the
# suite carries no janet dependency (the emitter's parseability is verified
# separately, over the whole corpus, not per-commit).
run_sash_emit_ir() {
    rc=-1 out="" err=""
    echo "$1" > .input.sash
    sash --emit-ir .input.sash > out 2> err
    rc=$?
    out=$(<out)
    err=$(<err)
    rm -f .input.sash out err
}

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

# 104.3 arithmetic and loops are captured. `--janet-map` rendered these as
#       `(sash/TODO …)`; a capture has no semantics to defer, so it is
#       complete from the start. This is the property that distinguishes it.
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

# 104.4 a literal containing a quote and a backslash is escaped for janet's
#       reader rather than terminating the string early
if in_modes native; then
    run_sash_emit_ir 'x="a\"b"; echo $x'
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
# only, so a value may contain one). native,c: janet does not emit
# `map m = @{a:DISP}` (task `janet-map-from-array`).
printf 'NAME=Alice\nGREETING=Hello ${NAME}\n' > /tmp/sash_test_config.conf
EXPAND=$(which sash-expand)
run_sash "s=\$($EXPAND /tmp/sash_test_config.conf); attest s [[ -n \$s ]]; - matchall \$s \"([^=\\x0a]*)=([^\\x0a]*)\" p; map config = @{p:~}; echo \${config[GREETING]}"
[[ $rc = 0 && $out = "Hello Alice" && -z $err ]]; chk 54.1

# --- DD#75 §1: punctuation specials are sugar for the keyword forms ---
# Both spellings compile to the same node/behaviour: $$ ≡ $(getpid),
# $? ≡ $(status), $! ≡ $(getcpid). The keyword form is canonical (docs,
# diagnostics, sash-convert); the punctuation form is accepted input sugar.
# spv.1-3 are native,c: janet execs the getpid intrinsic and mis-tracks the
# arith-false status (filed bug-janet-special-var-arith).
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
# program died with SIGSEGV, silently. These pin "fail loudly" — the
# capacity question itself is a separate, open decision for DG.
# native,c only: janet runs on the Janet VM, not sash-rt.
  /usr/bin/printf 'a\0%.0s' $(seq 1 20000) > hp_big.nul
  /usr/bin/printf 'x%.0s' $(seq 1 70000) > hp_wide.nul
  /usr/bin/printf '\0' >> hp_wide.nul
  /usr/bin/printf 'y%.0s' $(seq 1 30000) >> hp_wide.nul

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

set -e
tally
