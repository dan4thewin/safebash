#!/bin/bash
# spec/types.sh — type system: strings, ints, arrays, maps, qualifiers, dtoi/itod
set -eu
. common.sh
setup types
plan=127
echo "1..$plan" >&3

set +e

# --- DD#0 (implicit): bash control-flow constructs must apply
# compile-time analyses uniformly. A type error at top level must
# still fire when the same statement is wrapped in a Group, List
# (&&/||), Pipeline, etc. Regression for journal 056.

# 0.1 type-check descends into Group { ... }
run_sash 'int x=5
{ x=hello; }'
[[ $rc != 0 && -z $out && -n $err ]]; chk 0.1

# 0.2 type-check descends into left side of `&&`
run_sash 'int x=5
x=hello && /bin/true'
[[ $rc != 0 && -z $out && -n $err ]]; chk 0.2

# 0.3 type-check descends into right side of `&&`
run_sash 'int x=5
/bin/true && x=hello'
[[ $rc != 0 && -z $out && -n $err ]]; chk 0.3

# 0.4 type-check descends into pipeline stages
run_sash 'int x=5
x=hello | /bin/cat'
[[ $rc != 0 && -z $out && -n $err ]]; chk 0.4

# 12.1 string length
run_sash 'x="hello"; echo ${#x}'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk 12.1

# 12.2 null-terminated for C interop
run_sash 'x="hello"; /usr/bin/printf "%s\n" "$x"'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 12.2

# 12.3 O(1) length — proves ${#x} works without scanning
run_sash 'x="hello world test"; echo ${#x}'
[[ $rc = 0 && $out = "16" && -z $err ]]; chk 12.3

# 12.4 string interpolation in assignment (Concat in value position): the c
# backend lowers it via sash_concat (bug-c-janet-concat-in-value); native and
# janet already did.
run_sash 'b=world; a=hello$b; echo $a'
[[ $rc = 0 && $out = "helloworld" && -z $err ]]; chk 12.4

# 16.1 int stores machine-width
run_sash 'int x=9223372036854775807; echo $x'
[[ $rc = 0 && $out = "9223372036854775807" && -z $err ]]; chk 16.1

# 16.2 fn -> int return type, captured via $(( )) sexpr (DD#58)
run_sash 'fn add -> int { return $(( $1 + $2 )); }
int x=$(( (add 3 4) ))
echo "$x"'
[[ $rc = 0 && $out = "7" && -z $err ]]; chk 16.2

# 16.3 no implicit int-to-string conversion
run_sash 'int x=5; x="hello"'
[[ $rc != 0 && -z $out && -n $err ]]; chk 16.3

# 30.1 no implicit string-to-int
run_sash 'x="hello"; int y=$x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 30.1

# 30.2 $(( )) is explicit string-to-int — DD#77 D1: the bare string operand
# now needs a disposition (`${x:?…}` validates + converts). native,c (janet
# arith-value disposition gap, janet-arith-subscript-read).
if in_modes native,c; then
    run_sash 'x="42"; int y=$(( ${x:?nan} )); echo $y'
    [[ $rc = 0 && $out = "42" && -z $err ]]; chk 30.2
else
    skip_chk 30.2
fi

# 30.3 x=5 is a string
run_sash 'x=5; echo ${#x}'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk 30.3

# 30.4 int x=5 required for integer
run_sash 'int x=5; echo $(( x + 1 ))'
[[ $rc = 0 && $out = "6" && -z $err ]]; chk 30.4

# 30.5 DD#30-refine: reassigning a bare numeric literal to an already-int var
# is accepted (aligns `x=5` with `int x=5` once x is known int).
run_sash 'int x=0; x=5; echo $x'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk 30.5
# 30.6 negative literal to known-int
run_sash 'int x=0; x=-3; echo $x'
[[ $rc = 0 && $out = "-3" && -z $err ]]; chk 30.6
# 30.7 += bare literal to known-int (arith add)
run_sash 'int x=4; x+=3; echo $x'
[[ $rc = 0 && $out = "7" && -z $err ]]; chk 30.7
# 30.8 DD#76 int-collection cell accepts a bare numeric literal (like init).
# native,c — janet treats the int-collection var as a scalar int
# (janet-int-valued-collections).
if in_modes native,c; then
    run_sash 'int array a=(1 2); a[1]=99; echo ${a[1]}'
    [[ $rc = 0 && $out = "99" && -z $err ]]; chk 30.8
else
    skip_chk 30.8
fi
# 30.9 boundary: a bare arith EXPRESSION (not a literal) still needs $(( ))
run_sash 'int x=0; x=5+1'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 30.9
# 30.10 a NEW (untyped) var stays a string — `y=5` is not int (DD#30 unchanged)
run_sash 'y=5; int z=$y'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 30.10
# 30.11 scalar string `+=` accumulates (was a janet silent-miscompile: it
# dropped the += and emitted a plain set → "cd"; now "abcd" in all modes)
run_sash 's=ab; s+=cd; echo $s'
[[ $rc = 0 && $out = "abcd" && -z $err ]]; chk 30.11

# 33.1 indexed array integer keys
run_sash 'array a=(one two three); echo ${a[1]}'
[[ $rc = 0 && $out = "two" && -z $err ]]; chk 33.1

# 33.2 map string keys
run_sash 'map m=([name]=Alice); echo ${m[name]}'
[[ $rc = 0 && $out = "Alice" && -z $err ]]; chk 33.2

# 33.3 string key on array → compile error
run_sash 'array a=(one two); echo ${a[foo]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 33.3

# 33.4 integer key on map → compile error
run_sash 'map m=([a]=1); echo ${m[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 33.4

# 33.5 dynamic integer index ${a[$i]} expands the variable (read)
run_sash 'array a=(x y z); i=1; echo ${a[$i]}'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 33.5

# 33.6 dynamic string key ${m[$k]} expands the variable (read)
run_sash 'map m=([a]=1 [b]=2); k=b; echo ${m[$k]}'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk 33.6

# 33.14 non-brace dynamic integer index $a[$i] reads like ${a[$i]} (decision B,
# 2026-06-21 — deliberately diverging from bash, where $a[$i] is ${a[0]} plus
# literal "[$i]"). native is the reference; c/janet lack dynamic-subscript value
# reads (same pre-existing gap as 33.5/33.6).
run_sash 'array a=(x y z); i=2; echo $a[$i]'
[[ $rc = 0 && $out = "z" && -z $err ]]; chk 33.14

# 33.15 non-brace dynamic string key $m[$k] reads like ${m[$k]}
run_sash 'map m=([a]=1 [b]=2); k=b; echo $m[$k]'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk 33.15

# 33.7 dynamic index assignment a[$i]=v (write, indexed)
run_sash 'array a=(x y z); i=1; a[$i]=ZZ; echo ${a[1]}'
[[ $rc = 0 && $out = "ZZ" && -z $err ]]; chk 33.7

# 33.8 dynamic key assignment m[$k]=v (write, map)
run_sash 'map m=([a]=1); k=b; m[$k]=2; echo ${m[b]}'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk 33.8

# 33.11 bare string-key ASSIGNMENT on an indexed array → compile error
# (subscript-sigil invariant, DD#75 §4: enforce on write as on read 33.3)
run_sash 'array a=(x y z); a[i]=Q; echo ${a[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 33.11
# 33.12 literal + sigilled index assignments still work (not over-rejected)
run_sash 'array a=(x y z); i=1; a[$i]=P; a[2]=Q; echo ${a[$i]}${a[2]}'
[[ $rc = 0 && $out = "PQ" && -z $err ]]; chk 33.12

# 33.9 flat map literal map m=(a 1 b 2) → reject; require [k]=v form
run_sash 'map m=(a 1 b 2); echo ${m[a]}'
[[ $rc != 0 && -z $out && $err == *SB4031* ]]; chk 33.9

# 33.10 keys + dynamic subscript idiom iterates key/value pairs
run_sash 'map m=([a]=1 [b]=2); for k in $(keys m); do echo "$k=${m[$k]}"; done'
[[ $rc = 0 && $out = $'a=1\nb=2' && -z $err ]]; chk 33.10

# 35.1 stackable: local const int
run_sash 'fn f { local const int x=5; echo $x; }; f'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk 35.1

# 35.2 const on array → compile error
run_sash 'const array a=(1 2 3)'
[[ $rc != 0 && -z $out && -n $err ]]; chk 35.2

# 36.1 dtoi
run_sash 'int x=$(dtoi 2 "3.14"); echo $x'
[[ $rc = 0 && $out = "314" && -z $err ]]; chk 36.1

# 36.2 itod
run_sash 'int x=314; echo $(itod 2 $x)'
[[ $rc = 0 && $out = "3.14" && -z $err ]]; chk 36.2

# 36.3 roundtrip
run_sash 'int x=$(dtoi 2 "3.14"); echo $(itod 2 $x)'
[[ $rc = 0 && $out = "3.14" && -z $err ]]; chk 36.3

# 36.4 dtoi/itod are builtins
run_sash 'echo $(dtoi 0 "42")'
[[ $rc = 0 && $out = "42" && -z $err ]]; chk 36.4

# 36.5 $(len arr) is int-typed — assignable to an int var (was wrongly typed
#      string → SB4030). len yields an array's element count.
run_sash 'array a=(x y z); int n=$(len a); echo $n'
[[ $rc = 0 && $out = "3" && -z $err ]]; chk 36.5

# 36.6 $(epochseconds) is int-typed too. (The janet backend does not implement
#      the epochseconds intrinsic yet — it tries to exec it — so guard the
#      runtime to native,c; the int-typing fix itself is backend-independent.)
if in_modes native,c; then
    run_sash 'int n=$(epochseconds); echo $((n > 0))'
    [[ $rc = 0 && $out = "1" && -z $err ]]; chk 36.6
else
    skip_chk 36.6
fi

# 36.7 a string-returning value function stays string: `int n=$(toupper s)` is
#      still a type error (SB4030) — the int whitelist wasn't over-broadened.
run_sash 'int n=$(toupper hi)'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 36.7

# 41.1 array declaration
run_sash 'array arr=(a b c); echo ${arr[0]}'
[[ $rc = 0 && $out = "a" && -z $err ]]; chk 41.1

# 41.2 map declaration
run_sash 'map m=([k]=v); echo ${m[k]}'
[[ $rc = 0 && $out = "v" && -z $err ]]; chk 41.2


# --- DD#58 integer function capture syntax ---

# 58.1 $((f)) captures zero-arg -> int function
run_sash 'fn f -> int { return 42; }
int x=$((f))
echo "$x"'
[[ $rc = 0 && $out = "42" && -z $err ]]; chk 58.1

# 58.2 $(( (add a b) )) sexpr call with args
run_sash 'fn add -> int { return $(( $1 + $2 )); }
int x=$(( (add 3 4) ))
echo "$x"'
[[ $rc = 0 && $out = "7" && -z $err ]]; chk 58.2

# 58.3 nested sexpr calls
run_sash 'fn dbl -> int { return $(( $1 * 2 )); }
fn add -> int { return $(( $1 + $2 )); }
int x=$(( (add (dbl 3) 4) ))
echo "$x"'
[[ $rc = 0 && $out = "10" && -z $err ]]; chk 58.3

# 58.4 ((x = f)) assignment form
run_sash 'fn f -> int { return 42; }
int x=0
((x = f))
echo "$x"'
[[ $rc = 0 && $out = "42" && -z $err ]]; chk 58.4

# 58.5 if ((f > 0)) boolean context
run_sash 'fn f -> int { return 1; }
if ((f > 0)); then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 58.5

# 58.6 $(f) on a -> int function captures the RETURN value (Option C): `$(f)`
#      is equivalent to `$((f))`. A string var receives it as a string.
run_sash 'fn f -> int { return 42; }
x=$(f); echo $x'
[[ $rc = 0 && $out = "42" && -z $err ]]; chk 58.6

# 58.7 function/variable name collision is error
run_sash 'fn x { echo hi; }
x=hello'
[[ $rc != 0 && -z $out && -n $err ]]; chk 58.7

# 58.8 $(( (f) + 1 )) sexpr in expression
run_sash 'fn f -> int { return 10; }
int x=$(( (f) + 1 ))
echo "$x"'
[[ $rc = 0 && $out = "11" && -z $err ]]; chk 58.8

# 58.9 Option C: `int n=$(f)` on a -> int function captures the return value
#      (identical to `$((f))`) — the `$()`/`$(())` unification. (Was a silent
#      0, then briefly SB4018; now it just works.)
run_sash 'fn f -> int { return 42; }
int n=$(f); echo $n'
[[ $rc = 0 && $out = "42" && -z $err ]]; chk 58.9

# 58.10 `n += $(f)` likewise captures + adds the return value (n: 0 → 42).
run_sash 'fn f -> int { return 42; }
int n=0; n+=$(f); echo $n'
[[ $rc = 0 && $out = "42" && -z $err ]]; chk 58.10

# 58.11 the arithmetic form is equivalent — same result.
run_sash 'fn f -> int { return 42; }
int n=$((f)); echo $n'
[[ $rc = 0 && $out = "42" && -z $err ]]; chk 58.11

# 58.12 Option C in a command ARGUMENT: `echo $(f)` prints the return value,
#       and it works embedded in a double-quoted string too.
run_sash 'fn f -> int { return 42; }
echo "port=$(f)"'
[[ $rc = 0 && $out = "port=42" && -z $err ]]; chk 58.12

# 58.13 `$(f arg…)` passes positional args to the int function.
run_sash 'fn add -> int { return $(( $1 + $2 )); }
int n=$(add 3 4); echo $n'
[[ $rc = 0 && $out = "7" && -z $err ]]; chk 58.13

# 58.14 a sexpr call arg that is itself a PARENTHESISED arith expression
# `(dbl (2 + 1))` computes dbl(3)=6. Regression guard for
# bug-arith-sexpr-nested-cmdsub-arg: the `$(( ))` content collector must count
# individual parens, so the `))` of `(2 + 1))` are two single closes, not the
# arith terminator (which truncated the content and yielded a silent 0).
run_sash 'fn dbl -> int { return $(( $1 * 2 )); }
int x=$(( (dbl (2 + 1)) )); echo $x'
[[ $rc = 0 && $out = "6" && -z $err ]]; chk 58.14
# 58.15 the same nested-paren sexpr arg works in a `(( ))` condition
run_sash 'fn dbl -> int { return $(( $1 * 2 )); }
if (( (dbl (2 + 1)) == 6 )); then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 58.15

# --- DD#30 unset semantics ---

# 30.8 unset var (scalar) → compile error
run_sash 'x=hello; unset x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 30.8

# 30.9 unset arr[N] removes element
run_sash 'array a=(one two three); unset a[1]; echo ${a[0]} ${a[2]}'
[[ $rc = 0 && $out = "one three" && -z $err ]]; chk 30.9

# 30.10 unset map[key] removes key
run_sash 'map m=([a]=1 [b]=2); unset m[a]; echo ${m[b]}'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk 30.10

# 30.11 unset arr clears collection (canonical length ${#a}; DD#75 §4 retired ${#a[@]})
run_sash 'array a=(one two three); unset a; echo ${#a}'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk 30.11

# --- DD#65: per-element array ops dropped → `for x in @a` loop + sub/gsub ---
# (the `${arr[@]OP}` forms now reject SB4034; see var_modifiers.sh vm.dr16-18)

# 33.16 per-element substitution via loop + sub (was ${arr[@]/ba/BA})
run_sash 'array a=(foo bar baz); for x in @a; do echo $(sub $x ba BA); done'
[[ $rc = 0 && $out = $'foo\nBAr\nBAz' && -z $err ]]; chk 33.16

# 33.17 per-element strip prefix via loop + sub `^pre_` (was ${arr[@]#pre_})
run_sash 'array a=(pre_one pre_two three); for x in @a; do echo $(sub $x "^pre_" ""); done'
[[ $rc = 0 && $out = $'one\ntwo\nthree' && -z $err ]]; chk 33.17

# 33.18 per-element strip suffix via loop + sub `\.txt$` (was ${arr[@]%%.txt})
run_sash 'array a=(file.txt doc.txt note.md); for x in @a; do echo $(sub $x "\.txt$" ""); done'
[[ $rc = 0 && $out = $'file\ndoc\nnote.md' && -z $err ]]; chk 33.18

# --- DD#62 section A: [[ ]] operands are pre-lowered SsaValues ---
# Subscript operands carry their map-vs-indexed key-ness from transform
# (subscript_to_value), so a dynamic map key ${m[$k]} resolves as a STRING
# key inside [[ ]] instead of the old codegen guessing integer-key from the
# subscript syntax. Regression for the deleted lower_subscript key-type bug.

# 62.11 map literal key as a [[ ]] operand
run_sash 'map m=([k]=v); if [[ ${m[k]} == v ]]; then echo yes; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 62.11

# 62.12 map dynamic key ${m[$k]} resolves as a string key (the bug fix)
run_sash 'map m=([a]=1 [b]=2); k=b; if [[ ${m[$k]} == 2 ]]; then echo yes; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 62.12

# 62.13 same dynamic-key operand with != (exact inequality, DD#62)
run_sash 'map m=([a]=1 [b]=2); k=b; if [[ ${m[$k]} != 1 ]]; then echo yes; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 62.13

# 62.15 bare ${m[$k]} truthiness (implicit -n) in [[ ]]
run_sash 'map m=([a]=x); k=a; if [[ ${m[$k]} ]]; then echo yes; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 62.15

# 62.14 indexed dynamic key ${a[$i]} stays integer-keyed in [[ ]]. janet now
# coerces the array index to an integer in emit_expr_subscript
# (bug-janet-array-dynamic-subscript-read), so this works in all backends.
run_sash 'array a=(10 20 30); i=2; if [[ ${a[$i]} == 30 ]]; then echo yes; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 62.14

# --- subscript append a[i]+=v (read-modify-write). All backends: c lowers the
# Concat value (bug-c-janet-concat-in-value) and janet lowers indexed
# assignment via `put` (bug-janet-indexed-assignment).
# 33.11 indexed literal-index append concatenates onto the current element
run_sash 'array a=(1 2 3); a[0]+=9; echo ${a[0]}'
[[ $rc = 0 && $out = "19" && -z $err ]]; chk 33.11

# 33.12 indexed dynamic-index append
run_sash 'array a=(x y); i=1; a[$i]+=z; echo ${a[1]}'
[[ $rc = 0 && $out = "yz" && -z $err ]]; chk 33.12

# 33.13 map subscript append
run_sash 'map m=([k]=ab); m[k]+=cd; echo ${m[k]}'
[[ $rc = 0 && $out = "abcd" && -z $err ]]; chk 33.13

# 33.14 `echo @a` is a clean compile error (SB5030), not an ICE — echo joins
# its args, so an array splat there would flatten the array (DD#22/DD#69 §3);
# the user writes the explicit `echo $(join @a)`.
run_sash 'array a=(one two three); echo @a'
[[ $rc != 0 && -z $out && $err == *SB5030* ]]; chk 33.14

# 33.15 the directed-to explicit join compiles and prints the joined string
run_sash 'array a=(one two three); echo $(join @a)'
[[ $rc = 0 && $out = "one two three" && -z $err ]]; chk 33.15

# 33.23 `$(join)` with no args is a clean SB5040 (was a compiler ICE panic)
run_sash 'echo $(join)'
[[ $rc != 0 && -z $out && $err == *SB5040* ]]; chk 33.23
# 33.24 `$(join arr x y)` with >2 args rejects (extras were silently dropped)
run_sash 'array a=(1 2); echo $(join a x y)'
[[ $rc != 0 && -z $out && $err == *SB5040* ]]; chk 33.24

# 33.16 the reject also fires for a mixed splat (`echo x @a y`)
run_sash 'array a=(one two); echo x @a y'
[[ $rc != 0 && -z $out && $err == *SB5030* ]]; chk 33.16

# 33.17 $(keys m) is an array splat: in command-arg position it is SB5025
# (the DD#60/#75 context rule, same as $(argv)) — not a silent space-join.
run_sash 'map m=([a]=1 [b]=2); printf "%s\n" $(keys m)'
[[ $rc != 0 && -z $out && $err == *SB5025* ]]; chk 33.17
# 33.18 $(keys m) in scalar-assignment context → SB5025 (array splat in scalar)
run_sash 'map m=([a]=1 [b]=2); x=$(keys m); echo $x'
[[ $rc != 0 && -z $out && $err == *SB5025* ]]; chk 33.18

# 33.19–33.21 join arr SEP — DD#75: array first, separator second.
# (janet's join intrinsic only handles the `@a` space-join today: it execs
# a bare-name `join a` as an external command, and errors on a separator —
# guard to native,c; the arg-order + native reject are backend-independent.)
if in_modes native,c; then
    run_sash 'array a=(x y z); echo $(join a ",")'
    [[ $rc = 0 && $out = "x,y,z" && -z $err ]]; chk 33.19
    run_sash 'array a=(x y z); echo $(join @a ", ")'
    [[ $rc = 0 && $out = "x, y, z" && -z $err ]]; chk 33.20
    run_sash 'array a=(x y z); echo $(join a)'
    [[ $rc = 0 && $out = "x y z" && -z $err ]]; chk 33.21
else
    skip_chk 33.19; skip_chk 33.20; skip_chk 33.21
fi
# 33.22 join on a NON-array operand is a clean compile error, never a segfault
run_sash 's=hi; echo $(join s ",")'
[[ $rc != 0 && -z $out && -n $err ]]; chk 33.22

# DD#75 §2 — polymorphic `len`: dispatch on the operand's TYPE, not the `$`
# sigil (array/map → count; string → byte length; undefined → compile error).
# 75.1 len on a string → byte length (was garbage: arrlen on a non-table)
run_sash 's=hello; int n=$(len s); echo $n'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk 75.1
# 75.2 len on an array → element count
run_sash 'array a=(1 2 3); int n=$(len a); echo $n'
[[ $rc = 0 && $out = "3" && -z $err ]]; chk 75.2
# 75.3 len on a map → entry count
run_sash 'map m=([x]=1 [y]=2); int n=$(len m); echo $n'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk 75.3
# 75.4 ${#s} (string length) is Int-typed sugar for $(len s): int n=${#s}
run_sash 's=hello; int n=${#s}; echo $n'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk 75.4
# 75.5 ${#a} on an array → element count (Int), was SB4030 / 0
run_sash 'array a=(1 2 3); int n=${#a}; echo $n'
[[ $rc = 0 && $out = "3" && -z $err ]]; chk 75.5
# 75.6 ${#a} in a string context still renders the count
run_sash 'array a=(x y z w); echo ${#a}'
[[ $rc = 0 && $out = "4" && -z $err ]]; chk 75.6

# DD#75 §3 — `keys` is map-only (an indexed array's keys are trivially
# 0..len-1; a scalar has none). Only maps have non-trivial string keys.
# 75.7 keys on a map → the string keys (still works)
# (janet lacks the keys intrinsic — it execs `keys m` as an external command,
# same class as the join gap; pre-existing, cf. janet:33.10. Guard to native,c.)
if in_modes native,c; then
    run_sash 'map m=([a]=1 [b]=2); for k in $(keys m); do echo "$k"; done'
    [[ $rc = 0 && $out = $'a\nb' && -z $err ]]; chk 75.7
else
    skip_chk 75.7
fi
# 75.8 keys on an indexed array → compile error (was blank/null output)
run_sash 'array a=(x y z); for k in $(keys a); do echo "$k"; done'
[[ $rc != 0 && -z $out && -n $err ]]; chk 75.8
# 75.9 keys on a scalar → compile error
run_sash 's=hi; for k in $(keys s); do echo "$k"; done'
[[ $rc != 0 && -z $out && -n $err ]]; chk 75.9

# DD#75 §4 — retire the [@]/[*] bracket-subscript collection forms; `@a` is
# the sole splat. Each retired form → compile error; the canonical spelling
# works. (These are parse-level rejects: ${a[@]} shares the arrvals repr with
# @a, so they are distinguished only at parse.)
# 75.10 braced splat ${a[@]} → reject (use @a)
run_sash 'array a=(x y z); for w in ${a[@]}; do echo "$w"; done'
[[ $rc != 0 && -z $out && -n $err ]]; chk 75.10
# 75.11 braced star ${a[*]} → reject
run_sash 'array a=(x y z); for w in ${a[*]}; do echo "$w"; done'
[[ $rc != 0 && -z $out && -n $err ]]; chk 75.11
# 75.12 non-brace $a[@] → reject
run_sash 'array a=(x y z); for w in $a[@]; do echo "$w"; done'
[[ $rc != 0 && -z $out && -n $err ]]; chk 75.12
# 75.13 length ${#a[@]} → reject (use ${#a})
run_sash 'array a=(x y z); int n=${#a[@]}; echo $n'
[[ $rc != 0 && -z $out && -n $err ]]; chk 75.13
# 75.14 keys ${!a[@]} → reject (use $(keys m))
run_sash 'array a=(x y z); for k in ${!a[@]}; do echo "$k"; done'
[[ $rc != 0 && -z $out && -n $err ]]; chk 75.14
# 75.15 canonical @a splat still works (the replacement)
run_sash 'array a=(x y z); for w in @a; do echo "$w"; done'
[[ $rc = 0 && $out = $'x\ny\nz' && -z $err ]]; chk 75.15
# 75.16 indexed element access ${a[$i]} / ${a[1]} unaffected
run_sash 'array a=(x y z); i=2; echo ${a[$i]}${a[1]}'
[[ $rc = 0 && $out = "zy" && -z $err ]]; chk 75.16

# DD#75 §5 — positional parameters are the implicit 0-indexed `args` array
# ($1..$9 ≡ ${args[0]}..${args[8]}; $# ≡ ${#args}; $@ ≡ @args). Amended
# 2026-08-21: 0-indexed (was 1-indexed) + `shift` dropped. Each inline gets a
# uniquely-named args array, so nested/recursive calls never clobber positionals.
# 75.17 $1 is the first argument
run_sash 'fn f { echo $1; }
f hello'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 75.17
# 75.18 $# is the argument count (= ${#args})
run_sash 'fn f { echo $#; }
f a b c'
[[ $rc = 0 && $out = "3" && -z $err ]]; chk 75.18
# 75.19 a nested function call does NOT clobber the caller's positionals
run_sash 'fn g { echo $1; }
fn f { g Z; echo $1; }
f A'
[[ $rc = 0 && $out = $'Z\nA' && -z $err ]]; chk 75.19
# 75.20 user-written ${args[i]} reads the same array (native,c — janet uses its
# own _args model and does not expose the `args` name). DD#81: `: ${args[1]}`
# acknowledges the skipped middle param (fixed arity must read args in order).
if in_modes native,c; then
    run_sash 'fn f { : ${args[1]}; echo ${args[0]} ${args[2]}; }
f a b c'
    [[ $rc = 0 && $out = "a c" && -z $err ]]; chk 75.20
else
    skip_chk 75.20
fi
# 75.21 @args splat iterates every argument (native,c)
if in_modes native,c; then
    run_sash 'fn f { for x in @args; do echo "$x"; done; }
f a b'
    [[ $rc = 0 && $out = $'a\nb' && -z $err ]]; chk 75.21
else
    skip_chk 75.21
fi
# 75.22 more than 9 args are reachable via ${args[N]} (the old scalar model
# capped at $9) (native,c). DD#81: reaching a high index without the lower ones
# makes the function variadic — `: @args` marks it (takes any number).
if in_modes native,c; then
    run_sash 'fn f { : @args; echo ${args[10]}; }
f a b c d e f g h i j k'
    [[ $rc = 0 && $out = "k" && -z $err ]]; chk 75.22
else
    skip_chk 75.22
fi
# 75.23 `$(len args)` is the arg count (= $#), reading the implicit array — the
# intrinsic must resolve `args`. native,c (janet's `_args` model doesn't expose
# the `args` name; see janet-arith-subscript-read / the 75.20-22 guards).
if in_modes native,c; then
    run_sash 'fn f { echo $(len args); }
f a b c'
    [[ $rc = 0 && $out = "3" && -z $err ]]; chk 75.23
else
    skip_chk 75.23
fi
# 75.24 `$(join args SEP)` joins the implicit array. native,c
if in_modes native,c; then
    run_sash 'fn f { echo $(join args ,); }
f a b c'
    [[ $rc = 0 && $out = "a,b,c" && -z $err ]]; chk 75.24
else
    skip_chk 75.24
fi
# 75.25 `args` is function-local — a top-level `${args[0]}` reference is
# rejected (SB5024), like `$1`. Compile-time, mode-independent.
run_sash 'echo ${args[0]}'
[[ $rc != 0 && -z $out && $err == *SB5024* ]]; chk 75.25
# 75.26 top-level `@args` splat is rejected the same way
run_sash 'for x in @args; do echo "$x"; done'
[[ $rc != 0 && -z $out && $err == *SB5024* ]]; chk 75.26
# 75.27 DD#75 §2: `len <undefined>` is a compile error (SB5039), not a silent 0
run_sash 'echo $(len nope)'
[[ $rc != 0 && -z $out && $err == *SB5039* ]]; chk 75.27
# 75.28 no false positive: `len` of a declared scalar and array both compile
run_sash 's=hi; array a=(1 2 3); echo "$(len s) $(len a)"'
[[ $rc = 0 && $out = "2 3" && -z $err ]]; chk 75.28
# 75.29 a loop variable is a valid len operand (the type_check-side gap this
# avoids: SB5031's scope machinery already tracks `for` variables)
run_sash 'array a=(hi bye); for e in @a; do echo $(len e); done'
[[ $rc = 0 && $out = $'2\n3' && -z $err ]]; chk 75.29
# 75.30 `len <typo>` inside a function is also rejected
run_sash 'fn f { echo $(len bogus); }
f'
[[ $rc != 0 && -z $out && $err == *SB5039* ]]; chk 75.30

# ── DD#77 `[[ -i EXPR ]]` — strict integer-validity test ──
# True iff the operand's value is a complete integer (sign + decimal digits, or
# a 0x/0o/0b prefix with valid digits; empty/unset → false). Lets an author
# validate/branch before converting. All backends (sash_is_int / janet _isint).
# 77.1 literal integer → true
run_sash 'if [[ -i 42 ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 77.1
# 77.2 non-numeric → false
run_sash 'if [[ -i abc ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk 77.2
# 77.3 a set numeric string var → true
run_sash 's=42; if [[ -i $s ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 77.3
# 77.4 partial-numeric (trailing junk) → false (strict, no partial parse)
run_sash 's=42abc; if [[ -i $s ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk 77.4
# 77.5 negative and radix-prefixed → true
run_sash 's=-7; if [[ -i $s ]] && [[ -i 0xff ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 77.5
# 77.6 empty → false
run_sash 's=; if [[ -i $s ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk 77.6
# 77.7 DG's example: validate-or-sanitize, then use (composes with =~ s///g)
run_sash 's=42abc; [[ -i $s ]] || s =~ s/\D//g; echo $s'
[[ $rc = 0 && $out = "42" && -z $err ]]; chk 77.7
# 77.8 range-checked: i64::MAX is valid, MAX+1 is not (DD#77, DG). All backends.
run_sash 'if [[ -i 9223372036854775807 ]] && ! [[ -i 9223372036854775808 ]]; then echo ok; else echo no; fi'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 77.8
# 77.9 a validated value converts safely; `-i` guarantees no overflow trap.
# native,c — janet's arith VALUE path misreads an embedded ${…}
# (janet-arith-subscript-read).
if in_modes native,c; then
    run_sash 's=100; [[ -i $s ]] && echo $(( ${s:-0} * 2 ))'
    [[ $rc = 0 && $out = "200" && -z $err ]]; chk 77.9
else
    skip_chk 77.9
fi

set -e
tally

