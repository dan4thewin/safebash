#!/bin/bash
# spec/undeclared.sh — DD#62 Model B: reject a scalar read of a name
# introduced NOWHERE in scope (the typo footgun). Scalar zero values are
# meaningful (untyped → "", int → 0), so a KNOWN-but-maybe-unset read is
# allowed and yields the zero value — only a nowhere-introduced name (a
# typo) is a compile error (SB5031). Front-end check → all backends.
set -eu
. common.sh
setup undeclared
plan=18
echo "1..$plan" >&3

set +e

# ud.1 a bare read of a name assigned nowhere is a typo → SB5031
run_sash 'echo $foo'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk ud.1

# ud.2 the classic typo (msg vs mgs) is caught, and the message names it
run_sash 'msg=hi; echo $mgs'
[[ $rc != 0 && -z $out && $err == *SB5031* && $err == *mgs* ]]; chk ud.2

# ud.3 an assigned name reads fine
run_sash 'x=1; echo $x'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk ud.3

# ud.4 ${x:-DEFAULT} always compiles (explicit opt-out, even nowhere-introduced)
run_sash 'echo ${x:-def}'
[[ $rc = 0 && $out = "def" && -z $err ]]; chk ud.4

# ud.5 Model B: a KNOWN but maybe-unassigned read COMPILES (meaningful zero
#      on the else path) — NOT rejected. `x` is introduced by the then-branch.
#      native,c: janet diverges on scalar zero values (env-fallback; see
#      janet-scalar-zero-value).
if in_modes native,c; then
    run_sash 'if [[ -z zzz ]]; then x=1; fi; echo [$x]'
    [[ $rc = 0 && $out = "[]" && -z $err ]]; chk ud.5
else
    skip_chk ud.5
fi

# ud.6 a typed declaration introduces the name (and zero-inits). native,c:
#      janet emits `(var n )` (no typed zero-init) — janet-scalar-zero-value.
if in_modes native,c; then
    run_sash 'int n; echo $n'
    [[ $rc = 0 && $out = "0" && -z $err ]]; chk ud.6
else
    skip_chk ud.6
fi

# ud.7 a for-loop variable is introduced
run_sash 'for i in a b c; do echo $i; done'
[[ $rc = 0 && $out = $'a\nb\nc' && -z $err ]]; chk ud.7

# ud.8 a function sees an enclosing (global) name
run_sash 'g=hi; fn f { echo $g; }; f'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk ud.8

# ud.9 a name introduced ONLY inside another function is unknown here → typo
run_sash 'fn f { helper=1; }; echo $helper'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk ud.9

# ud.10 a typo inside a function body is caught
run_sash 'fn f { val=1; echo $vla; }; f'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk ud.10

# ud.11 order-insensitive: a read before the assignment in the same scope is
#       fine (the name is introduced in scope; reads the zero value first).
#       native,c: janet scalar zero-value divergence.
if in_modes native,c; then
    run_sash 'echo [$x]; x=5; echo [$x]'
    [[ $rc = 0 && $out = $'[]\n[5]' && -z $err ]]; chk ud.11
else
    skip_chk ud.11
fi

# ud.12 a typo inside a double-quoted interpolation is caught
run_sash 'name=world; echo "hello $naem"'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk ud.12

# ud.13 a read inside a ${x:-$y} default is still checked (y is a typo here)
run_sash 'echo ${x:-$undef_y}'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk ud.13

# ud.14 specials are not flagged ($? after a command)
run_sash '/bin/true; echo $?'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk ud.14

# ud.15 a typo inside a command substitution is caught (subshell reads see
#       the enclosing scope, so an unknown name there is still a typo)
run_sash 'echo $(echo $cs_typo)'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk ud.15

# ud.16 a command-substitution-local assignment is a valid introduction —
#       the validator must NOT flag the read of `csx`. Asserts only that
#       validation is clean (no SB5031); the multi-statement cmdsub runtime
#       path is a separate known gap (bug-multistatement-cmdsub-placeholder).
if in_modes native; then
    run_sash 'echo $(csx=1; echo $csx)'
    [[ $rc = 0 && $err != *SB5031* ]]; chk ud.16
else
    skip_chk ud.16
fi

# ud.17 `$LINENO` gets no special handling (DG: no magic vars) — an unassigned
#       magic var is rejected as an undefined variable, like any other name
run_sash 'echo $LINENO'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk ud.17
# ud.18 the braced form `${LINENO}` is rejected the same way
run_sash 'echo ${LINENO}'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk ud.18

set -e
tally
