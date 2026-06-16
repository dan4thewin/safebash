#!/bin/bash
# spec/misc.sh — miscellaneous compiler tests (DD#5, DD#6, DD#15, DD#20, DD#32, DD#37, DD#38, DD#52)
set -eu
. common.sh
setup misc
plan=23
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

# 20.3 lastpipe always on
run_sash 'echo hello | read x; echo $x'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 20.3

# 20.4 extglob @() rejected
run_sash 'case x in @(a|b)) echo yes;; esac'
[[ $rc != 0 && -z $out && -n $err ]]; chk 20.4

# 20.5 globstar ** rejected
run_sash 'for f in **/*.sh; do echo $f; done'
[[ $rc != 0 && -z $out && -n $err ]]; chk 20.5

# 20.6 pipefail always on
run_sash '/bin/false | /bin/true'
[[ $rc != 0 && -z $err ]]; chk 20.6

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

# 52.6a ${var^^} accepted
run_sash 'x=hello; echo ${x^^}'
[[ $rc = 0 && $out = "HELLO" && -z $err ]]; chk 52.6a

# 52.6b ${var,,} accepted
run_sash 'x=HELLO; echo ${x,,}'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 52.6b

# --- DD#54 sash-expand pipeline ---

# 54.1 sash-expand | mapfile -m loads config into map (DD#61: maps use
# two-binder bless-all — scalar bless on a map is now a type error)
printf 'NAME=Alice\nGREETING=Hello ${NAME}\n' > /tmp/sash_test_config.conf
EXPAND=$(which sash-expand)
run_sash "map config; $EXPAND /tmp/sash_test_config.conf | mapfile -m config; bless k v in config [[ \$k =~ ^[A-Z]+\$ && -n \$v ]]; echo \${config[GREETING]}"
[[ $rc = 0 && $out = "Hello Alice" && -z $err ]]; chk 54.1

set -e
tally
