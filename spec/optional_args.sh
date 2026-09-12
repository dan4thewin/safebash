#!/bin/bash
# spec/optional_args.sh — DD#84 optional trailing args (`:-`/`:~`) and the
# scalar elision operator `:~`. Positive behavior pins optional arity and
# elision. The parse (SB4051) and validate (SB5046) rejects are
# backend-independent.
set -eu
. common.sh
setup optional_args
set +e
plan=32
echo "1..$plan" >&3

# ── Optional trailing args via `:-` (a trailing `${args[i]:-D}` is optional) ──
# 84.1 called with 0 args → the default fires (arity [0,1])
run_sash 'fn greet { /bin/echo Hi ${args[0]:-world} }
greet'
[[ $rc = 0 && $out = "Hi world" && -z $err ]]; chk 84.1

# 84.2 called with 1 arg → the arg
run_sash 'fn greet { /bin/echo Hi ${args[0]:-world} }
greet Sam'
[[ $rc = 0 && $out = "Hi Sam" && -z $err ]]; chk 84.2

# 84.3 too many args → SB5043 (arity max is 1)
run_sash 'fn greet { /bin/echo Hi ${args[0]:-world} }
greet a b'
[[ $rc != 0 && $err == *SB5043* ]]; chk 84.3

# 84.4 a required arg followed by an optional one — arity [1,2]
run_sash 'fn f { /bin/echo ${args[0]} ${args[1]:-D} }
f one'
[[ $rc = 0 && $out = "one D" && -z $err ]]; chk 84.4

# 84.5 the same fn, too few (needs >= 1) → SB5042
run_sash 'fn f { /bin/echo ${args[0]} ${args[1]:-D} }
f'
[[ $rc != 0 && $err == *SB5042* ]]; chk 84.5

# ── The elide operator `:~` — the token disappears when identity/absent ──
# 84.6 absent optional arg → elided (ls report, not ls "" report)
run_sash 'fn foo { /bin/echo ls ${args[0]:~} report }
foo'
[[ $rc = 0 && $out = "ls report" && -z $err ]]; chk 84.6

# 84.7 present arg → kept
run_sash 'fn foo { /bin/echo ls ${args[0]:~} report }
foo -l'
[[ $rc = 0 && $out = "ls -l report" && -z $err ]]; chk 84.7

# 84.8 passed empty string → elided (empty = identity)
run_sash 'fn foo { /bin/echo ls ${args[0]:~} report }
foo ""'
[[ $rc = 0 && $out = "ls report" && -z $err ]]; chk 84.8

# 84.9 runtime empty scalar in an arg list → elided
run_sash 'x=; /bin/echo a ${x:~} b'
[[ $rc = 0 && $out = "a b" && -z $err ]]; chk 84.9

# 84.10 runtime non-empty scalar → kept
run_sash 'x=MID; /bin/echo a ${x:~} b'
[[ $rc = 0 && $out = "a MID b" && -z $err ]]; chk 84.10

# 84.11 array literal: an empty `${…:~}` element is elided, indices compact
run_sash 'y=; array c=(x ${y:~} z); /bin/echo $(len c) : $(join c)'
[[ $rc = 0 && $out = "2 : x z" && -z $err ]]; chk 84.11

# 84.12 array literal: a non-empty element is kept
run_sash 'y=MID; array c=(x ${y:~} z); /bin/echo $(len c) : $(join c)'
[[ $rc = 0 && $out = "3 : x MID z" && -z $err ]]; chk 84.12

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

# ── Rejects: arity structural (transform) ──
# 84.17 an index read both bare and via a disposition → SB5047 (mixing)
run_sash 'fn f { /bin/echo ${args[0]} ${args[0]:-x} }
f a'
[[ $rc != 0 && $err == *SB5047* ]]; chk 84.17

# 84.18 a required arg after an optional one → SB5048 (optional must be trailing)
run_sash 'fn f { /bin/echo ${args[0]:-x} ${args[1]} }
f a b'
[[ $rc != 0 && $err == *SB5048* ]]; chk 84.18

# ── Positional `${N:OP}` == `${args[N-1]:OP}` (regression: `${N:~}`/`${N:-}`
# were mis-analyzed as REQUIRED, giving a spurious SB5042, and lowered as a
# scalar var "N" so they always elided/defaulted even when $N was present). ──

# 84.19 `${2:~}`, called with 1 arg → $2 elided, NO spurious SB5042 (the bug)
run_sash 'fn f { /bin/echo A $1 ${2:~} B }
f one'
[[ $rc = 0 && $out = "A one B" && -z $err ]]; chk 84.19

# 84.20 `${2:~}`, called with 2 args → $2 KEPT (value actually read)
run_sash 'fn f { /bin/echo A $1 ${2:~} B }
f one two'
[[ $rc = 0 && $out = "A one two B" && -z $err ]]; chk 84.20

# 84.21 `${2:-D}`, called with 1 arg → the default fires
run_sash 'fn f { /bin/echo A $1 ${2:-D} B }
f one'
[[ $rc = 0 && $out = "A one D B" && -z $err ]]; chk 84.21

# 84.22 `${2:-D}`, called with 2 args → $2 read (not the default)
run_sash 'fn f { /bin/echo A $1 ${2:-D} B }
f one two'
[[ $rc = 0 && $out = "A one two B" && -z $err ]]; chk 84.22

# 84.23 positional `${2:?}` is REQUIRED (like `${args[1]:?}`): too few → SB5042
run_sash 'fn f { /bin/echo $1 ${2:?need} }
f one'
[[ $rc != 0 && $err == *SB5042* ]]; chk 84.23

# 84.24 positional mixing: bare `$2` AND `${2:~}` → SB5047 (one or the other)
run_sash 'fn f { /bin/echo $1 $2 ${2:~} }
f a b'
[[ $rc != 0 && $err == *SB5047* ]]; chk 84.24

# 84.25-30 DD#92 addendum: the `args` array is IMMUTABLE + always clean — a
# tainted value can never be passed as an argument (passing is a read → SB5020),
# so `args` is unconditionally clean, and writing it is rejected (SB5052). The
# write forms: assignment, subscript-assign, read -a / argv / mapfile fill,
# declaration, and the (now meaningless) `attest … in args`. Compile-time, all
# backends.
run_sash 'fn f { args[0]=x; /bin/echo y }; f'
[[ $rc != 0 && -z $out && $err == *SB5052* ]]; chk 84.25
run_sash 'fn f { echo hi | read args; /bin/echo x }; f'
[[ $rc != 0 && -z $out && $err == *SB5052* ]]; chk 84.26
run_sash 'fn f { argv 1 args; /bin/echo x }; f'
[[ $rc != 0 && -z $out && $err == *SB5052* ]]; chk 84.27
# 84.28 `matchall … args` — every BULK FILL must reject the reserved array,
# not just the read family. This one was a real hole: `matchall` was added to
# the collection-target list but not to the immutability list, and SB5050
# ("dead variable") fired first and masked it. `args` is now exempt from
# SB5050 precisely because a write to it is illegal whether or not it is read,
# so the accurate rule gets to speak.
run_sash 'fn f { - matchall "a b" "[a-z]" args; /bin/echo x }; f'
[[ $rc != 0 && -z $out && $err == *SB5052* ]]; chk 84.28

# 84.31 …and the `|| :` spelling of the same discard is equally rejected —
# the hole was in the target check, not in how the result was consumed
run_sash 'fn f { matchall "a b" "[a-z]" args || :; /bin/echo x }; f'
[[ $rc != 0 && -z $out && $err == *SB5052* ]]; chk 84.31

# 84.32 a NORMAL bulk-fill target in a function is unaffected (control) —
# an unread one is still an ordinary dead variable
run_sash 'fn f { - matchall "a b" "[a-z]" ok; /bin/echo x }; f'
[[ $rc != 0 && -z $out && $err == *SB5050* ]]; chk 84.32
run_sash 'fn f { attest w in args [[ -n $w ]]; /bin/echo z }; f'
[[ $rc != 0 && -z $out && $err == *SB5052* ]]; chk 84.29
# 84.30 control: a differently-named local array is fine (only `args` is
# reserved) — reading `@args` is never a write, so it stays valid.
run_sash 'fn f { echo hi | read norm; /bin/echo @{norm:?} }; f'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 84.30

set -e
tally
