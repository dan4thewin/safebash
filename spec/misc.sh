#!/bin/bash
# spec/misc.sh — miscellaneous compiler tests (DD#5, DD#6, DD#15, DD#20, DD#32, DD#37, DD#38, DD#52)
set -eu
. common.sh
setup misc
plan=27
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

# 15.1 --janet flag accepted (native-only — tests the --janet CLI surface
# directly; rerunning under SASH_MODE=janet would be redundant)
run_sash_janet() {
    rc=-1 out="" err=""
    echo "$1" > .input.sash
    sash --janet .input.sash > out 2> err
    rc=$?
    out=$(<out)
    err=$(<err)
}
if in_modes native; then
    run_sash_janet 'echo hello'
    [[ $rc = 0 && -z $err ]]; chk 15.1
else
    skip_chk 15.1
fi

# 15.2 janet reflects sash semantics
if in_modes native; then
    run_sash_janet 'x="hello world"; echo $x'
    [[ $rc = 0 && -z $err ]]; chk 15.2
else
    skip_chk 15.2
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
if in_modes native,c; then
    run_sash "s=\$($EXPAND /tmp/sash_test_config.conf); attest s [[ -n \$s ]]; - matchall \$s \"([^=\\x0a]*)=([^\\x0a]*)\" p; map config = @{p:~}; echo \${config[GREETING]}"
    [[ $rc = 0 && $out = "Hello Alice" && -z $err ]]; chk 54.1
else
    skip_chk 54.1
fi

# --- DD#75 §1: punctuation specials are sugar for the keyword forms ---
# Both spellings compile to the same node/behaviour: $$ ≡ $(getpid),
# $? ≡ $(status), $! ≡ $(getcpid). The keyword form is canonical (docs,
# diagnostics, sash-convert); the punctuation form is accepted input sugar.
# spv.1-3 are native,c: janet execs the getpid intrinsic and mis-tracks the
# arith-false status (filed bug-janet-special-var-arith).
if in_modes native,c; then
    # spv.1 $$ and $(getpid) are the same value (this process's pid)
    run_sash 'p=$$; q=$(getpid); [[ $p == $q ]] && echo Y'
    [[ $rc = 0 && $out = "Y" && -z $err ]]; chk spv.1
    # spv.2 $? sugar reads the last status (arith-false → 1)
    run_sash '(( 0 )); echo $?'
    [[ $rc = 0 && $out = "1" && -z $err ]]; chk spv.2
    # spv.3 $(status) keyword reads the same status
    run_sash '(( 0 )); echo $(status)'
    [[ $rc = 0 && $out = "1" && -z $err ]]; chk spv.3
else
    skip_chk spv.1; skip_chk spv.2; skip_chk spv.3
fi
# spv.4 $! and $(getcpid) are the same value (empty with no background job)
run_sash 'x=$!; y=$(getcpid); [[ $x == $y ]] && echo Y'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk spv.4

set -e
tally
