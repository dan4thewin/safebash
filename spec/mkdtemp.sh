#!/bin/bash
# DD135: direct values/statuses, owned directories, and failure boundaries.
. common.sh
plan=40
setup mkdtemp

set +e

# Poking the returned entropy makes real file/directory/symlink collisions
# deterministic without adding a test hook to the runtime.
strace -qq -o injector.trace \
    -e inject=getrandom:retval=1:poke_exit=@arg1=00 /bin/true || {
    echo "spec: mkdtemp requires strace with poke_exit support" >&3
    exit 1
}
tracer=(strace -qq -s 256 -e trace=getrandom,mkdirat,clone,fork,vfork)

# Only successful, non-injected mkdir calls prove ownership. This also finds
# older creations if a broken value carrier no longer returns their names.
clean_created() {
    local path
    [[ -f $1 ]] || return 0
    cat -- "$1"
    while IFS= read -r path; do
        if [[ -e $path || -L $path ]]; then
            rmdir -- "$path" || exit 1
        fi
    done < <(sed -nE 's/^mkdirat\(AT_FDCWD, "(\/tmp\/sash\.[0-9a-f]{32})", 0700\)[[:space:]]+= 0$/\1/p' "$1")
}

run_refusal() {
    clean_created refusal.trace
    : > refusal.trace
    run_sash_under "$1" "${tracer[@]}" -o refusal.trace
}

run_sash_under 'str p=$(mkdtemp); echo $p' \
    "${tracer[@]}" -o create.trace
[[ $rc == 0 && -z $err && -z $rerr && $out =~ ^/tmp/sash\.[0-9a-f]{32}$ \
   && -d $out && ! -L $out && -O $out && $(stat -c %a -- "$out") == 700 \
   && $(grep -c '^mkdirat(.*= 0$' create.trace) == 1 \
   && $(grep -cE '^(clone|fork|vfork)\(' create.trace) == 0 ]]; chk mkd.1
clean_created create.trace

run_sash_under 'str first=$(mkdtemp) second=$(mkdtemp)
echo $first
echo $second' "${tracer[@]}" -o stable.trace
mapfile -t paths <<< "$out"
[[ $rc == 0 && -z $err && -z $rerr && ${#paths[@]} == 2 \
   && ${paths[0]} != "${paths[1]}" && -d ${paths[0]} && -d ${paths[1]} \
   && $(grep -c '^mkdirat(.*= 0$' stable.trace) == 2 ]]; chk mkd.2
clean_created stable.trace

run_sash_under 'str p=$(mkdtemp); echo $p' \
    env TMPDIR="$PWD/missing" "${tracer[@]}" -o tmpdir.trace
[[ $rc == 0 && -z $err && -z $rerr && $out =~ ^/tmp/sash\.[0-9a-f]{32}$ \
   && -d $out && ! -e missing ]]; chk mkd.3
clean_created tmpdir.trace

run_sash_under 'if p=$(mkdtemp) { int code=$?; echo $p; echo $code } else { echo wrong }' \
    "${tracer[@]}" -o condition.trace
mapfile -t paths <<< "$out"
[[ $rc == 0 && -z $err && -z $rerr && ${#paths[@]} == 2 \
   && -d ${paths[0]} && ${paths[1]} == 0 ]]; chk mkd.4
clean_created condition.trace

all_caught=1
for cause in EACCES ENOMEM; do
    run_sash_under 'str p=old
if p=$(mkdtemp) { echo wrong } else { echo "[$p]" $? }' \
        "${tracer[@]}" -o caught.trace -e "inject=mkdirat:error=$cause"
    [[ $rc == 0 && -z $err && -z $rerr && $out == '[] 1' \
       && $(grep -c "mkdirat(.*$cause.*INJECTED" caught.trace) == 1 ]] || all_caught=0
    clean_created caught.trace
done
[[ $all_caught == 1 ]]; chk mkd.5

run_sash_under 'str p=old
ERR { - echo "[$p]" }
p=$(mkdtemp)
echo after' "${tracer[@]}" -o checked.trace -e inject=mkdirat:error=EACCES
[[ $rc == 1 && -z $err && $out == '[old]' \
   && $rerr == *'mkdtemp: /tmp: Permission denied'* \
   && $(grep -c 'mkdirat(.*EACCES.*INJECTED' checked.trace) == 1 ]]; chk mkd.6
clean_created checked.trace

run_sash_under 'str p=old; p=$(- mkdtemp)
array parts=($p)
echo "[$p] [$(join parts)]"; echo after' \
    "${tracer[@]}" -o discarded.trace -e inject=mkdirat:error=EROFS
[[ $rc == 0 && -z $err && -z $rerr && $out == $'[] []\nafter' \
   && $(grep -c 'mkdirat(.*EROFS.*INJECTED' discarded.trace) == 1 ]]; chk mkd.7
clean_created discarded.trace

run_sash_under 'str p=$(- mkdtemp); echo $p' \
    "${tracer[@]}" -o discard-success.trace
[[ $rc == 0 && -z $err && -z $rerr && $out =~ ^/tmp/sash\.[0-9a-f]{32}$ \
   && -d $out ]]; chk mkd.8
clean_created discard-success.trace

run_sash_under 'ERR {
    if work=$(mkdtemp) { - echo wrong } else { - echo "caught[$work]" }
}
fail 3' "${tracer[@]}" -o err-failed.trace -e inject=mkdirat:error=ENOSPC
[[ $rc == 3 && -z $err && -z $rerr && $out == 'caught[]' \
   && $(grep -c 'mkdirat(.*ENOSPC.*INJECTED' err-failed.trace) == 1 ]]; chk mkd.9
clean_created err-failed.trace

run_sash_under 'ERR {
    if work=$(mkdtemp) {
        - echo $work
        if /usr/bin/rmdir -- $work { - echo removed }
    }
}
fail 3' "${tracer[@]}" -o err-created.trace
[[ $rc == 3 && -z $err && -z $rerr && $out == /tmp/sash.*$'\nremoved' \
   && $(grep -c '^mkdirat(.*= 0$' err-created.trace) == 1 ]]; chk mkd.10
clean_created err-created.trace

run_sash_under 'str work=$(mkdtemp)
echo $work
ERR {
    if [[ -n $work ]] {
        if /usr/bin/rmdir -- $work { - echo removed }
    }
}
fail 3' "${tracer[@]}" -o err-inherited.trace
[[ $rc == 3 && -z $err && -z $rerr && $out == /tmp/sash.*$'\nremoved' \
   && $(grep -c '^mkdirat(.*= 0$' err-inherited.trace) == 1 ]]; chk mkd.11
clean_created err-inherited.trace

run_sash_under 'str work
ERR {
    if [[ -n $work ]] {
        if /usr/bin/rmdir -- $work { - echo removed }
    }
}
work=$(mkdtemp)
echo $work' "${tracer[@]}" -o err-empty.trace -e inject=mkdirat:error=EROFS
[[ $rc == 1 && -z $err && -z $out && $rerr == *'mkdtemp: /tmp:'* \
   && $(grep -c 'mkdirat(.*EROFS.*INJECTED' err-empty.trace) == 1 \
   && $(grep -cE '^(clone|fork|vfork)\(' err-empty.trace) == 0 ]]; chk mkd.12
clean_created err-empty.trace

run_refusal 'str p=$(mkdtemp template); echo $p'
[[ $rc != 0 && $err == *SB4045* ]]; chk mkd.13

run_refusal 'mkdtemp'
[[ $rc != 0 && $err == *SB5073* ]]; chk mkd.14

run_refusal 'str p=$(echo before; mkdtemp); echo ${p:!}'
[[ $rc != 0 && $err == *SB5073* ]]; chk mkd.15

run_refusal 'str p=$(mkdtemp; echo after); echo ${p:!}'
[[ $rc != 0 && $err == *SB5073* ]]; chk mkd.16

run_refusal 'int p=$(mkdtemp); echo $p'
[[ $rc != 0 && $err == *SB4030* ]]; chk mkd.17

run_refusal 'if str p=$(mkdtemp) { echo $p }'
[[ $rc != 0 && $err == *SB4025*'typed declaration'* ]]; chk mkd.18

run_refusal 'if p=$(- mkdtemp) { echo $p }'
[[ $rc != 0 && $err == *SB4025*'discarded status'* ]]; chk mkd.19

run_refusal 'str p; p=$(- mkdtemp) || echo "[$p]"'
[[ $rc != 0 && $err == *SB4025*'discarded status'* ]]; chk mkd.20

run_refusal 'str p=$(mkdtemp); echo $p $?'
[[ $rc != 0 && $err == *SB5012* ]]; chk mkd.21

run_refusal 'str p=$(- getenv MKD_INPUT); p=$(mkdtemp); echo $p'
[[ $rc != 0 && $err == *SB5020* ]]; chk mkd.22

SASH_CFLAGS=--no-taint-cst run_refusal \
    'str p=$(- getenv MKD_INPUT); p=$(mkdtemp); echo $p'
[[ $rc != 0 && $err == *'tainted variable p_'* ]]; chk mkd.23

run_refusal 'ERR { str work=$(mkdtemp); - echo $work }; fail'
[[ $rc = 1 && -z $err && -z $rerr && $out =~ ^/tmp/sash\.[0-9a-f]{32}$ &&
   -d $out && $(grep -c '^mkdirat(.*= 0$' refusal.trace) = 1 ]]; chk errmig.41

run_refusal 'str work=old
ERR { if work=$(mkdtemp) { - echo $work } }
fail'
[[ $rc != 0 && $err == *SB5102* ]]; chk mkd.25
clean_created refusal.trace
: > refusal.trace

run_sash_under 'if /bin/echo "$(mkdtemp)" { echo reached } else { echo caught }' \
    "${tracer[@]}" -o operand.trace -e inject=mkdirat:error=EACCES
[[ $rc == 1 && -z $err && -z $out && $rerr == *'mkdtemp: /tmp:'* \
   && $(grep -c 'mkdirat(.*EACCES.*INJECTED' operand.trace) == 1 ]]; chk mkd.26
clean_created operand.trace

run_sash_under 'fn allocate { str p=$(mkdtemp); echo $p; return 0 }
if allocate { echo reached } else { echo caught }' \
    "${tracer[@]}" -o body.trace -e inject=mkdirat:error=EACCES
[[ $rc == 1 && -z $err && -z $out && $rerr == *'mkdtemp: /tmp:'* \
   && $(grep -c 'mkdirat(.*EACCES.*INJECTED' body.trace) == 1 ]]; chk mkd.27
clean_created body.trace

suffix=$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')
[[ $suffix =~ ^[0-9a-f]{32}$ ]] || exit 1
collision=/tmp/sash.$suffix
mkdir -m 700 -- "$collision" || exit 1
printf keep > "$collision/marker" || exit 1

run_sash_under 'str p=$(mkdtemp); echo $p' "${tracer[@]}" -o dir-collision.trace \
    -e "inject=getrandom:retval=16:poke_exit=@arg1=$suffix:when=1"
[[ $rc == 0 && -z $err && -z $rerr && -d $out && $out != "$collision" \
   && $(<"$collision/marker") == keep \
   && $(grep -c 'mkdirat(.*EEXIST' dir-collision.trace) == 1 \
   && $(grep -c '^mkdirat(.*= 0$' dir-collision.trace) == 1 ]]; chk mkd.28
clean_created dir-collision.trace

run_sash_under 'str p=old
if p=$(mkdtemp) { echo wrong } else { echo "[$p]" $? }' \
    "${tracer[@]}" -o exhausted.trace \
    -e "inject=getrandom:retval=16:poke_exit=@arg1=$suffix"
[[ $rc == 0 && -z $err && -z $rerr && $out == '[] 1' \
   && $(<"$collision/marker") == keep \
   && $(grep -c 'mkdirat(.*EEXIST' exhausted.trace) == 128 \
   && $(grep -c '^mkdirat(.*= 0$' exhausted.trace) == 0 ]]; chk mkd.29
clean_created exhausted.trace

run_sash_under 'str p=old
if p=$(mkdtemp) { echo wrong } else { echo "[$p]" $? }' \
    "${tracer[@]}" -o collision-error.trace \
    -e "inject=getrandom:retval=16:poke_exit=@arg1=$suffix:when=1" \
    -e inject=mkdirat:error=EACCES:when=2
[[ $rc == 0 && -z $err && -z $rerr && $out == '[] 1' \
   && $(<"$collision/marker") == keep \
   && $(grep -c 'mkdirat(.*EEXIST' collision-error.trace) == 1 \
   && $(grep -c 'mkdirat(.*EACCES.*INJECTED' collision-error.trace) == 1 ]]; chk mkd.30
clean_created collision-error.trace
rm -- "$collision/marker" && rmdir -- "$collision" || exit 1

(set -C; printf keep > "$collision") || exit 1
run_sash_under 'str p=$(mkdtemp); echo $p' "${tracer[@]}" -o file-collision.trace \
    -e "inject=getrandom:retval=16:poke_exit=@arg1=$suffix:when=1"
[[ $rc == 0 && -z $err && -z $rerr && -d $out && $out != "$collision" \
   && -f $collision && ! -L $collision && $(<"$collision") == keep \
   && $(grep -c 'mkdirat(.*EEXIST' file-collision.trace) == 1 ]]; chk mkd.31
clean_created file-collision.trace
rm -- "$collision" || exit 1

printf keep > target
ln -s "$PWD/target" "$collision" || exit 1
run_sash_under 'str p=$(mkdtemp); echo $p' "${tracer[@]}" -o link-collision.trace \
    -e "inject=getrandom:retval=16:poke_exit=@arg1=$suffix:when=1"
[[ $rc == 0 && -z $err && -z $rerr && -d $out && $out != "$collision" \
   && -L $collision && $(readlink -- "$collision") == "$PWD/target" && $(<target) == keep \
   && $(grep -c 'mkdirat(.*EEXIST' link-collision.trace) == 1 ]]; chk mkd.32
clean_created link-collision.trace
rm -- "$collision" || exit 1

run_sash_under 'str p=$(mkdtemp); echo $p' \
    "${tracer[@]}" -o interrupted.trace -e inject=getrandom:error=EINTR:when=1
[[ $rc == 0 && -z $err && -z $rerr && -d $out \
   && $(grep -c 'getrandom(.*EINTR.*INJECTED' interrupted.trace) == 1 \
   && $(grep -c '^getrandom(' interrupted.trace) == 2 \
   && $(grep -c '^mkdirat(.*= 0$' interrupted.trace) == 1 ]]; chk mkd.33
clean_created interrupted.trace

run_sash_under 'str p=$(mkdtemp); echo $p' "${tracer[@]}" -o short.trace \
    -e inject=getrandom:retval=4:poke_exit=@arg1=01234567:when=1
[[ $rc == 0 && -z $err && -z $rerr && $out == /tmp/sash.01234567* && -d $out \
   && $(grep -c '^getrandom(' short.trace) == 2 \
   && $(grep -c '^mkdirat(.*= 0$' short.trace) == 1 ]]; chk mkd.34
clean_created short.trace

run_sash_under 'str p=old
if p=$(mkdtemp) { echo wrong } else { echo "[$p]" $? }' \
    "${tracer[@]}" -o no-entropy.trace -e inject=getrandom:retval=0
[[ $rc == 0 && -z $err && -z $rerr && $out == '[] 1' \
   && $(grep -c '^getrandom(.*INJECTED' no-entropy.trace) == 1 \
   && $(grep -c '^mkdirat(' no-entropy.trace) == 0 ]]; chk mkd.35
clean_created no-entropy.trace

all_discarded=1
for cause in ENOSYS ENOMEM; do
    run_sash_under 'str p=$(- mkdtemp); echo "[$p]"; echo after' \
        "${tracer[@]}" -o entropy-error.trace -e "inject=getrandom:error=$cause"
    [[ $rc == 0 && -z $err && -z $rerr && $out == $'[]\nafter' \
       && $(grep -c "getrandom(.*$cause.*INJECTED" entropy-error.trace) == 1 \
       && $(grep -c '^mkdirat(' entropy-error.trace) == 0 ]] || all_discarded=0
    clean_created entropy-error.trace
done
[[ $all_discarded == 1 ]]; chk mkd.36

SASH_CFLAGS=--heap-size=64 run_sash_under 'ERR { - echo ERR }
if p=$(mkdtemp) { echo $p } else { echo caught }' \
    "${tracer[@]}" -o heap-condition.trace
[[ $rc == 1 && -z $err && $out == ERR \
   && $rerr == *'out of memory (requested 43 bytes)'* \
   && $(grep -cE '^(getrandom|mkdirat)\(' heap-condition.trace) == 0 ]]; chk mkd.37
clean_created heap-condition.trace

SASH_CFLAGS=--heap-size=64 run_sash_under 'ERR { - echo ERR }
str p=$(- mkdtemp); echo $p; echo after' \
    "${tracer[@]}" -o heap-discard.trace
[[ $rc == 1 && -z $err && $out == ERR \
   && $rerr == *'out of memory (requested 43 bytes)'* \
   && $(grep -cE '^(getrandom|mkdirat)\(' heap-discard.trace) == 0 ]]; chk mkd.38
clean_created heap-discard.trace

run_refusal 'str p=$(mkdtemp 2>/dev/null); echo $p'
[[ $rc != 0 && $err == *SB4046* ]]; chk mkd.39

run_refusal 'str p=$(mkdtemp | /bin/cat); echo ${p:!}'
[[ $rc != 0 && $err == *SB5073* ]]; chk mkd.40
clean_created refusal.trace

set -e
tally
