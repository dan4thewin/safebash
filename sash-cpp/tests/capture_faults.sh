#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C SASH_CPP_TEST_TRACE=1

die() {
    printf 'capture faults: %s\n' "$*" >&2
    exit 1
}

(( $# == 1 )) || die 'usage: capture_faults.sh BUILD'
build=$1
[[ -n $build ]] || die 'BUILD must not be empty'
program=$build/runtime-tests
helper=$build/command-helper
directory=$build/capture-faults
trace_set=read,write,pipe2,close,wait4,memfd_create
[[ -x $program && -x $helper && -x $build/error_paths ]] || die 'build the test prerequisites first'
mkdir -p "$directory"

prefix=$directory/memfd-canary
if timeout 10 strace -qq -e trace=memfd_create -e inject=memfd_create:error=ENOSYS:when=1 \
    -o "$prefix.trace" "$build/error_paths" capture checked "$helper" \
    >"$prefix.out" 2>"$prefix.err"; then
    die 'the memfd injection canary unexpectedly succeeded'
else
    observed=$?
fi
[[ $observed == 2 && ! -s $prefix.out ]] || die 'the memfd canary failed at a different boundary'
grep -qE '^memfd_create[(].*ENOSYS.*INJECTED' "$prefix.trace" ||
    die 'the canary did not inject memfd_create'
grep -q '^error_paths: memfd_create: ' "$prefix.err" || die 'the canary lost its operation context'

prefix=$directory/no-memfd
timeout 10 strace -qq -e "trace=$trace_set" -e inject=memfd_create:error=ENOSYS:when=1 \
    -o "$prefix.trace" "$program" --capture-no-memfd "$helper" >"$prefix.out" 2>"$prefix.err" ||
    die "pipe capture depended on memfd; evidence: $prefix.*"
! grep -q '^memfd_create[(]' "$prefix.trace" || die 'pipe capture attempted memfd_create'
printf '%s\n' 'ok pipe capture without memfd (injection canary failed as expected)'

for occurrence in 1 2; do
    for effect in EIO EINTR; do
        prefix=$directory/read-$occurrence-$effect
        baseline=$prefix.baseline
        timeout 10 strace -qq -s 64 -e "trace=$trace_set" -o "$baseline.trace" \
            "$program" --capture-read-interrupted "$helper" >"$baseline.out" 2>"$baseline.err" ||
            die "capture read baseline failed; evidence: $baseline.*"
        site=$(awk -v wanted="$occurrence" '
            /^read[(]/ { count++ }
            index($0, "write(2, \"__SASH_CPP_TEST_BEGIN__") == 1 { begun=1; next }
            begun && !fd && /^pipe2[(]\[[0-9]+, [0-9]+\], O_CLOEXEC[)] *= 0$/ {
                value=$0; sub(/^pipe2[(]\[/, "", value); split(value, fds, ", "); fd=fds[1]
            }
            begun && fd && index($0, "read(" fd ",") == 1 && /, 8192[)]/ &&
                $NF ~ /^[1-9][0-9]*$/ {
                if (++seen == wanted) { print count, fd; found=1; exit }
            }
            END { if (!found) exit 1 }
        ' "$baseline.trace") || die 'the baseline did not reach the requested positive data read'
        read -r number fd <<<"$site"
        mode=--capture-read-failure
        [[ $effect != EINTR ]] || mode=--capture-read-interrupted
        timeout 10 strace -qq -s 64 -e "trace=$trace_set" \
            -e "inject=read:error=$effect:when=$number" -o "$prefix.trace" \
            "$program" "$mode" "$helper" >"$prefix.out" 2>"$prefix.err" ||
            die "capture read assertion failed; evidence: $prefix.*"
        grep -qE "^read[(]$fd, .*, 8192[)].*$effect.*INJECTED" "$prefix.trace" ||
            die "injection missed the calibrated capture fd; evidence: $prefix.*"
        if [[ $effect == EINTR ]]; then
            cmp -s "$baseline.out" "$prefix.out" && cmp -s "$baseline.err" "$prefix.err" ||
                die 'an interrupted capture read changed bytes or completion'
        fi
        printf 'ok capture data read %d: %s\n' "$occurrence" "$effect"
    done
done

prefix=$directory/wait
timeout 10 strace -qq -e "trace=$trace_set" -e inject=wait4:error=EIO:when=1 \
    -o "$prefix.trace" "$program" --capture-wait-failure "$helper" \
    >"$prefix.out" 2>"$prefix.err" || die "capture wait assertion failed; evidence: $prefix.*"
grep -qE '^wait4[(].*EIO.*INJECTED' "$prefix.trace" || die 'the wait probe did not inject wait4'
printf '%s\n' 'ok capture bytes and child exit retained after wait EIO'
