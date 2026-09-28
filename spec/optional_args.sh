#!/bin/bash
# spec/optional_args.sh — DD#84 optional trailing args (`:-`/`:~`) and the
# scalar elision operator `:~`. Positive behavior pins optional arity and
# elision. The parse (SB4051) and validate (SB5046) rejects are
# backend-independent.
set -eu
. common.sh
setup optional_args
set +e
plan=67
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
run_sash 'x=hi; [[ ${x:~} == y ]] && echo z'
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

# ── DD#91 addendum: SB5046 keys on the ELIDE, and must say so accurately ──
# `!` is a modifier, not a disposition, so `${x:!~}` is elide-plus-trust.
# The diagnostic used to render only `pe.operator`, quoting `${x:~}` back at
# an author who wrote `${x:!~}` and advising `${x:-DEFAULT}` — which drops the
# taint-off, so following it traded SB5046 for SB5020.

# 84.33 `:!~` in a value position is SB5046, quoted as written
run_sash 'str x=a
str y=${x:!~}
echo $y'
[[ $rc != 0 && $err == *SB5046* && $err == *'`${x:!~}`'* ]]; chk 84.33

# 84.34 the advice keeps the `!` — `${x:!-DEFAULT}`, not `${x:-DEFAULT}`
run_sash 'str x=a
str y=${x:!~}
echo $y'
[[ $rc != 0 && $err == *'`${x:!-DEFAULT}`'* && $err != *'`${x:-DEFAULT}`'* ]]; chk 84.34

# 84.35 and the advised form actually compiles on a TAINTED subject, which is
#       the case the old advice broke
run_sash 'str x=$(getenv HOME)
str y=${x:!-D}
if [[ -n $y ]]; then echo done; fi'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk 84.35

# 84.36 plain `:~` is unchanged — no `!` leaks into its message or advice
run_sash 'str x=a
str y=${x:~}
echo $y'
[[ $rc != 0 && $err == *SB5046* && $err == *'`${x:~}`'* && $err != *':!'* ]]; chk 84.36


# ── Which COMMANDS take an elidable argument list ─────────────────────────
# DD#84 names three elidable positions: an external / command argument list,
# a sash-VARARGS builtin arg list (`echo ${x:~}`), and an array literal
# element. A fixed-arity builtin is none of them — its arguments are values
# in fixed ROLES, so eliding one does not shorten a list, it shifts the rest
# into the wrong slots.
#
# The check enforcing this was an allow-by-default denylist naming exactly
# the two exceptions that had been reported (`read`, `readline`), each added
# after it panicked codegen. The list had already been patched twice — see
# ice.7-ice.10 (arithmetic) and nr.18/nr.20 (read) — and eleven commands were
# still open, every one of them an ICE rather than a diagnostic. It is now an
# allowlist over `BuiltinCmd`, total, so a new builtin is non-elidable until
# someone decides otherwise.

# 84.37 `cd` — a path is a value, not one of a list
run_sash 'str p=""
cd ${p:~}'
[[ $rc != 0 && $err == *SB5046* && $err != *ICE* ]]; chk 84.37

# 84.38 `match` in statement position (DD#70). The pattern slot is a role:
# eliding it would make `match "$s"` a one-argument match, not a shorter one.
run_sash 'str s=abc
str p=""
if match "$s" ${p:~} { echo y }'
[[ $rc != 0 && $err == *SB5046* && $err != *ICE* ]]; chk 84.38

# 84.39 `sub` — three fixed roles (subject, pattern, replacement)
run_sash 'str s=abc
str p=""
str r=$(sub "$s" ${p:~} X)
echo "$r"'
[[ $rc != 0 && $err == *SB5046* && $err != *ICE* ]]; chk 84.39

# 84.40 `gsub` — same shape as sub
run_sash 'str s=abc
str p=""
str r=$(gsub "$s" ${p:~} X)
echo "$r"'
[[ $rc != 0 && $err == *SB5046* && $err != *ICE* ]]; chk 84.40

# 84.41 `toupper` — unary
run_sash 'str p=""
str r=$(toupper ${p:~})
echo "$r"'
[[ $rc != 0 && $err == *SB5046* && $err != *ICE* ]]; chk 84.41

# 84.42 `tolower` — unary
run_sash 'str p=""
str r=$(tolower ${p:~})
echo "$r"'
[[ $rc != 0 && $err == *SB5046* && $err != *ICE* ]]; chk 84.42

# 84.43 `strlen` — unary
run_sash 'str p=""
int r=$(strlen ${p:~})
echo "$r"'
[[ $rc != 0 && $err == *SB5046* && $err != *ICE* ]]; chk 84.43

# 84.44 `basename` — unary
run_sash 'str p=""
str r=$(basename ${p:~})
echo "$r"'
[[ $rc != 0 && $err == *SB5046* && $err != *ICE* ]]; chk 84.44

# 84.45 `dirname` — unary
run_sash 'str p=""
str r=$(dirname ${p:~})
echo "$r"'
[[ $rc != 0 && $err == *SB5046* && $err != *ICE* ]]; chk 84.45

# 84.46 `return` — an exit status is a value
run_sash 'fn f {
  str p=""
  return ${p:~}
}
f'
[[ $rc != 0 && $err == *SB5046* && $err != *ICE* ]]; chk 84.46

# ── …and which ones DO. None of these had any coverage before. ────────────

# 84.47 builtin `echo` is DD#84's own worked example: empty elides
run_sash 'str p=""
echo A ${p:~} B'
[[ $rc = 0 && $out = "A B" && -z $err ]]; chk 84.47

# 84.48 …and non-empty is kept
run_sash 'str p=Q
echo A ${p:~} B'
[[ $rc = 0 && $out = "A Q B" && -z $err ]]; chk 84.48

# 84.49 `write` IS `echo` minus the newline (DD#99), so it elides identically
run_sash 'str p=""
write A ${p:~} B
echo ""'
[[ $rc = 0 && $out = "A B" && -z $err ]]; chk 84.49

# 84.50 a user function call is a command argument list: argc really drops
run_sash 'fn f {
  : @args
  int n=$(len args)
  echo "argc=$n"
}
str p=""
f A ${p:~} B'
[[ $rc = 0 && $out = "argc=2" && -z $err ]]; chk 84.50

# 84.51 …and does not drop when the value is non-empty
run_sash 'fn f {
  : @args
  int n=$(len args)
  echo "argc=$n"
}
str p=Q
f A ${p:~} B'
[[ $rc = 0 && $out = "argc=3" && -z $err ]]; chk 84.51

# 84.52 an external command's argument list (the DD's first bullet)
run_sash 'str p=""
/bin/echo A ${p:~} B'
[[ $rc = 0 && $out = "A B" && -z $err ]]; chk 84.52

# ── DD#84 (DG, 2026-09-28): a runtime-variable argument count is meaningful
# only where the callee accepts any number. A splat and a `:~` elide each
# expand to 0..n tokens at run time, so a FIXED-arity callee — whose argc must
# be known at compile time — cannot be called with one, and a VARIADIC callee
# cannot have its floor starved by an empty one. SB5087.
#
# Before this rule both directions were wrong: an elide silently
# under-delivered into a fixed callee, and a splat was counted as the constant
# 1, so a 3-element array was rejected as "called with 1".

# 84.53 a `:~` elide into a fixed-arity callee (was: silent under-delivery)
run_sash 'fn f { str a=${args[0]}; str b=${args[1]}; echo "a=[$a] b=[$b]" }
str x=""
f one ${x:~}'
[[ $rc != 0 && $err == *SB5087* ]]; chk 84.53

# 84.54 a bare splat into a fixed-arity callee (was: false "called with 1")
run_sash 'fn f { str a=${args[0]}; str b=${args[1]}; echo "[$a][$b]" }
array q=(p d)
f @q'
[[ $rc != 0 && $err == *SB5087* ]]; chk 84.54

# 84.55 …and a splat that provably yields one is still an unknown count
run_sash 'fn f { str a=${args[0]}; str b=${args[1]}; echo "[$a][$b]" }
array q=(p d)
f @{q:-}'
[[ $rc != 0 && $err == *SB5087* ]]; chk 84.55

# 84.56 the count is not "1": a three-element array is rejected the same way
run_sash 'fn f { str a=${args[0]}; str b=${args[1]}; echo "[$a][$b]" }
array q=(p d x)
f @q'
[[ $rc != 0 && $err == *SB5087* ]]; chk 84.56

# 84.57 an empty bare splat cannot satisfy a variadic callee's floor
run_sash 'fn f { : @args; str a=${args[0]}; echo "a=[$a]" }
array q=()
f @{q:~}'
[[ $rc != 0 && $err == *SB5087* ]]; chk 84.57

# 84.58 …nor can a `:~` elide
run_sash 'fn f { : @args; str a=${args[0]}; echo "a=[$a]" }
str x=""
f ${x:~}'
[[ $rc != 0 && $err == *SB5087* ]]; chk 84.58

# 84.59 `@{a:-}` guarantees one element (2026-09-04 addendum), so it satisfies
run_sash 'fn f { : @args; str a=${args[0]}; echo "a=[$a]" }
array q=()
f @{q:-}'
[[ $rc = 0 && $out = "a=[]" && -z $err ]]; chk 84.59

# 84.60 `@{a:?}` aborts on empty, so any path that continues delivers one
run_sash 'fn f { : @args; str a=${args[0]}; echo "a=[$a]" }
array q=(z)
f @{q:?empty}'
[[ $rc = 0 && $out = "a=[z]" && -z $err ]]; chk 84.60

# 84.61 a fixed argument ahead of the splat satisfies the floor on its own
run_sash 'fn f { : @args; str a=${args[0]}; echo "a=[$a]" }
array q=(z)
f one @{q:~}'
[[ $rc = 0 && $out = "a=[one]" && -z $err ]]; chk 84.61

# 84.62 floor 0 (no required index) accepts an empty splat — zero arguments
run_sash 'fn f { for y in @args; do echo "<$y>"; done }
array q=()
f @{q:~}'
[[ $rc = 0 && -z $out && -z $err ]]; chk 84.62

# 84.63 the rule is about USER functions: an external still elides
run_sash 'str x=""
/usr/bin/printf "[%s]" ${x:~}
echo ""'
[[ $rc = 0 && $out = "[]" && -z $err ]]; chk 84.63

# 84.64 …a varargs builtin still elides
run_sash 'str x=""
echo A ${x:~} B'
[[ $rc = 0 && $out = "A B" && -z $err ]]; chk 84.64

# 84.65 …and an array literal element still elides
run_sash 'str x=""
array b=(p ${x:~} q)
echo "n=$(len b)"'
[[ $rc = 0 && $out = "n=2" && -z $err ]]; chk 84.65

# 84.66 `: @args` must not promote a DD#84 optional index back to required —
# the same body without it accepts a zero-argument call, so this must too
run_sash 'fn f { : @args; echo "[${args[0]:-}]" }
f'
[[ $rc = 0 && $out = "[]" && -z $err ]]; chk 84.66

# 84.67 …and the optional index still reads when it IS supplied
run_sash 'fn f { : @args; echo "[${args[0]:-}]" }
f hi'
[[ $rc = 0 && $out = "[hi]" && -z $err ]]; chk 84.67

set -e
tally
