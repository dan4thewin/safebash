#!/bin/bash
# spec/optional_args.sh — DD#84 optional trailing args (`:-`/`:~`) and the
# scalar elision operator `:~`. Positive behavior (optional arity, elision) is
# native,c — janet has no `args`-array model and does not elide (its `_args`
# model is a separate gap, filed janet-optional-args-elision). The parse
# (SB4051) and validate (SB5046) rejects are backend-independent (all modes).
set -eu
. common.sh
setup optional_args
set +e
plan=18
echo "1..$plan" >&3

# ── Optional trailing args via `:-` (a trailing `${args[i]:-D}` is optional) ──
# 84.1 called with 0 args → the default fires (arity [0,1])
if in_modes native,c; then
    run_sash 'fn greet { /bin/echo Hi ${args[0]:-world} }
greet'
    [[ $rc = 0 && $out = "Hi world" && -z $err ]]; chk 84.1
else skip_chk 84.1; fi

# 84.2 called with 1 arg → the arg
if in_modes native,c; then
    run_sash 'fn greet { /bin/echo Hi ${args[0]:-world} }
greet Sam'
    [[ $rc = 0 && $out = "Hi Sam" && -z $err ]]; chk 84.2
else skip_chk 84.2; fi

# 84.3 too many args → SB5043 (arity max is 1)
if in_modes native,c; then
    run_sash 'fn greet { /bin/echo Hi ${args[0]:-world} }
greet a b'
    [[ $rc != 0 && $err == *SB5043* ]]; chk 84.3
else skip_chk 84.3; fi

# 84.4 a required arg followed by an optional one — arity [1,2]
if in_modes native,c; then
    run_sash 'fn f { /bin/echo ${args[0]} ${args[1]:-D} }
f one'
    [[ $rc = 0 && $out = "one D" && -z $err ]]; chk 84.4
else skip_chk 84.4; fi

# 84.5 the same fn, too few (needs >= 1) → SB5042
if in_modes native,c; then
    run_sash 'fn f { /bin/echo ${args[0]} ${args[1]:-D} }
f'
    [[ $rc != 0 && $err == *SB5042* ]]; chk 84.5
else skip_chk 84.5; fi

# ── The elide operator `:~` — the token disappears when identity/absent ──
# 84.6 absent optional arg → elided (ls report, not ls "" report)
if in_modes native,c; then
    run_sash 'fn foo { /bin/echo ls ${args[0]:~} report }
foo'
    [[ $rc = 0 && $out = "ls report" && -z $err ]]; chk 84.6
else skip_chk 84.6; fi

# 84.7 present arg → kept
if in_modes native,c; then
    run_sash 'fn foo { /bin/echo ls ${args[0]:~} report }
foo -l'
    [[ $rc = 0 && $out = "ls -l report" && -z $err ]]; chk 84.7
else skip_chk 84.7; fi

# 84.8 passed empty string → elided (empty = identity)
if in_modes native,c; then
    run_sash 'fn foo { /bin/echo ls ${args[0]:~} report }
foo ""'
    [[ $rc = 0 && $out = "ls report" && -z $err ]]; chk 84.8
else skip_chk 84.8; fi

# 84.9 runtime empty scalar in an arg list → elided
if in_modes native,c; then
    run_sash 'x=; /bin/echo a ${x:~} b'
    [[ $rc = 0 && $out = "a b" && -z $err ]]; chk 84.9
else skip_chk 84.9; fi

# 84.10 runtime non-empty scalar → kept
if in_modes native,c; then
    run_sash 'x=MID; /bin/echo a ${x:~} b'
    [[ $rc = 0 && $out = "a MID b" && -z $err ]]; chk 84.10
else skip_chk 84.10; fi

# 84.11 array literal: an empty `${…:~}` element is elided, indices compact
if in_modes native,c; then
    run_sash 'y=; array c=(x ${y:~} z); /bin/echo $(len c) : $(join c)'
    [[ $rc = 0 && $out = "2 : x z" && -z $err ]]; chk 84.11
else skip_chk 84.11; fi

# 84.12 array literal: a non-empty element is kept
if in_modes native,c; then
    run_sash 'y=MID; array c=(x ${y:~} z); /bin/echo $(len c) : $(join c)'
    [[ $rc = 0 && $out = "3 : x MID z" && -z $err ]]; chk 84.12
else skip_chk 84.12; fi

# ── Rejects: parse (SB4051) + validate (SB5046) are backend-independent ──
# 84.13 `:~` takes no word → SB4051 (all modes: parse)
run_sash 'echo ${x:~foo}'
[[ $rc != 0 && $err == *SB4051* ]]; chk 84.13

# 84.14 `:~` in a scalar assignment RHS (value required) → SB5046
run_sash 'x=hi; n=${x:~}; echo $n'
[[ $rc != 0 && $err == *SB5046* ]]; chk 84.14

# 84.15 `:~` in a `[[ ]]` operand (value required) → SB5046
run_sash 'x=hi; [[ ${x:~} = y ]] && echo z'
[[ $rc != 0 && $err == *SB5046* ]]; chk 84.15

# 84.16 `:~` nested inside a concatenation (not a standalone token) → SB5046
run_sash 'x=hi; /bin/echo foo${x:~}'
[[ $rc != 0 && $err == *SB5046* ]]; chk 84.16

# ── Rejects: arity structural (transform → native,c) ──
# 84.17 an index read both bare and via a disposition → SB5047 (mixing)
if in_modes native,c; then
    run_sash 'fn f { /bin/echo ${args[0]} ${args[0]:-x} }
f a'
    [[ $rc != 0 && $err == *SB5047* ]]; chk 84.17
else skip_chk 84.17; fi

# 84.18 a required arg after an optional one → SB5048 (optional must be trailing)
if in_modes native,c; then
    run_sash 'fn f { /bin/echo ${args[0]:-x} ${args[1]} }
f a b'
    [[ $rc != 0 && $err == *SB5048* ]]; chk 84.18
else skip_chk 84.18; fi

set -e
tally
