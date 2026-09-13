#!/bin/bash
# spec/compiler.sh — acceptance tests for sash compiler changes (DD#57)
set -eu
. common.sh
setup compiler
plan=12
echo "1..$plan" >&3

set +e

# --- 57.63: sash accepts sash-native source only ---

# 57.63a sash-native input compiles
run_sash 'int x=5
echo $x'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk 57.42a

# 57.63b bash-specific input rejected (declare is not sash-native)
run_sash 'declare -i x=5
echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 57.42b

# --- 57.64: trivial bash indistinguishable from sash-native ---

run_sash 'x=hello
echo $x'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 57.43

# --- 57.70: non-sash node types are compile errors ---

# 57.70a select
run_sash 'select x in a b c; do echo $x; break; done'
[[ $rc != 0 && -z $out && -n $err ]]; chk 57.44a

# 57.70b coproc
run_sash 'coproc cat'
[[ $rc != 0 && -z $out && -n $err ]]; chk 57.44b

# --- 57.71: non-sash declaration keywords are compile errors ---

# 57.71a declare
run_sash 'declare -i x=5'
[[ $rc != 0 && -z $out && -n $err ]]; chk 57.45a

# 57.71b typeset
run_sash 'typeset x=5'
[[ $rc != 0 && -z $out && -n $err ]]; chk 57.45b

# 57.71c readonly
run_sash 'readonly x=5'
[[ $rc != 0 && -z $out && -n $err ]]; chk 57.45c

# 57.71d export
run_sash 'export FOO=bar'
[[ $rc != 0 && -z $out && -n $err ]]; chk 57.45d

# --- 57.74: removed flags --- (sash CLI flag rejection)
# native-only, and NOT a c-backend gap: these invoke the `sash` CLI directly
# rather than run_sash, so SASH_MODE never reaches them. Running the group in
# c mode would execute the identical three commands a second time. Audited
# 2026-09-13: keep the guard, it is de-duplication, not a suppressed defect.

if in_modes native; then
    # 57.74a --emit-source
    echo 'echo hello' > .input.sash
    sash --emit-source .input.sash > /dev/null 2>err
    rc=$?; err=$(<err)
    [[ $rc != 0 && -n $err ]]; chk 57.46a

    # 57.74b --annotate
    sash --annotate .input.sash > /dev/null 2>err
    rc=$?; err=$(<err)
    [[ $rc != 0 && -n $err ]]; chk 57.46b

    # 57.74c --nolint
    sash --nolint .input.sash > /dev/null 2>err
    rc=$?; err=$(<err)
    [[ $rc != 0 && -n $err ]]; chk 57.46c
else
    skip_chk 57.46a
    skip_chk 57.46b
    skip_chk 57.46c
fi

set -e
tally
