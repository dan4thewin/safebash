#!/bin/bash
# spec/var_modifiers.sh — DD#65 var-modifier prune. Built up per stage:
# toupper/tolower builtins (this stage), then basename/dirname sugar, the
# `:-` word, and the dropped-operator rejects.
set -eu
. common.sh
setup var_modifiers
plan=7
echo "1..$plan" >&3

set +e

# ── toupper / tolower builtins (DD#65 — irreducible case conversion) ──
run_sash 's=hello; echo $(toupper $s)'
[[ $rc = 0 && $out = "HELLO" && -z $err ]]; chk vm.tu1
run_sash 's=World; echo $(tolower $s)'
[[ $rc = 0 && $out = "world" && -z $err ]]; chk vm.tu2
run_sash 'echo $(toupper hi)'
[[ $rc = 0 && $out = "HI" && -z $err ]]; chk vm.tu3
run_sash 'a=$(tolower ABC); echo "[$a]"'
[[ $rc = 0 && $out = "[abc]" && -z $err ]]; chk vm.tu4
# vm.tu5 digits / non-letters pass through
run_sash 'echo $(toupper aB3_z)'
[[ $rc = 0 && $out = "AB3_Z" && -z $err ]]; chk vm.tu5
# vm.tu6 concatenation of two conversions
run_sash 'echo $(toupper foo)-$(tolower BAR)'
[[ $rc = 0 && $out = "FOO-bar" && -z $err ]]; chk vm.tu6
# vm.tu7 the case-insensitive match idiom (DD#65: regex has no `i` flag)
run_sash 'x=HeLLo; pat=hello; match $(tolower $x) $(tolower $pat) && echo same || echo diff'
[[ $rc = 0 && $out = "same" && -z $err ]]; chk vm.tu7

set -e
tally
