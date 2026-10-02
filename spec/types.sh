#!/bin/bash
# spec/types.sh — type system: strings, ints, arrays, maps, qualifiers
set -eu
. common.sh
setup types
plan=307
echo "1..$plan" >&3

set +e

# --- DD#0 (implicit): bash control-flow constructs must apply
# compile-time analyses uniformly. A type error at top level must
# still fire when the same statement is wrapped in a Group, List
# (&&/||), Pipeline, etc. Regression for journal 056.

# 0.1 type-check descends into Group { ... }. Asserts the specific code:
# this read `-n $err`, which would keep passing on an unrelated *parse*
# error, and briefly did while the group surface was being reworked.
run_sash 'int x=5
{ x=hello; }'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 0.1

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

# 12.1 string length ($(strlen x); the ${#x} sugar is retired — DD#75 §4)
run_sash 'x="hello"; echo $(strlen $x)'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk 12.1

# 12.2 null-terminated for C interop
run_sash 'x="hello"; /usr/bin/printf "%s\n" "$x"'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 12.2

# 12.3 O(1) length — $(strlen x) is a counted-string read, no scan
run_sash 'x="hello world test"; echo $(strlen $x)'
[[ $rc = 0 && $out = "16" && -z $err ]]; chk 12.3

# 12.4 string interpolation in assignment (Concat in value position): the c
# backend lowers it via sash_concat.
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
# now needs a disposition (`${x:?…}` validates + converts).
run_sash 'x="42"; int y=$(( ${x:?nan} )); echo $y'
[[ $rc = 0 && $out = "42" && -z $err ]]; chk 30.2

# 30.3 x=5 is a string (one byte long)
run_sash 'x=5; echo $(strlen $x)'
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
run_sash 'int array a=(1 2); a[1]=99; echo ${a[1]}'
[[ $rc = 0 && $out = "99" && -z $err ]]; chk 30.8
# 30.9 boundary: a bare arith EXPRESSION (not a literal) still needs $(( ))
run_sash 'int x=0; x=5+1'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 30.9
# 30.10 a NEW (untyped) var stays a string — `y=5` is not int (DD#30 unchanged)
run_sash 'y=5; int z=$y'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 30.10
# 30.11 scalar string `+=` accumulates ("abcd", not a plain set).
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
run_sash 'array a=(x y z); int i=1; echo ${a[$i]}'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 33.5

# 33.6 dynamic string key ${m[$k]} expands the variable (read)
run_sash 'map m=([a]=1 [b]=2); k=b; echo ${m[$k]}'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk 33.6

# 33.14 non-brace dynamic integer index $a[$i] reads like ${a[$i]} (decision B,
# 2026-06-21 — deliberately diverging from bash, where $a[$i] is ${a[0]} plus
# literal "[$i]").
run_sash 'array a=(x y z); int i=2; echo $a[$i]'
[[ $rc = 0 && $out = "z" && -z $err ]]; chk 33.14

# 33.15 non-brace dynamic string key $m[$k] reads like ${m[$k]}
run_sash 'map m=([a]=1 [b]=2); k=b; echo $m[$k]'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk 33.15

# 33.7 dynamic index assignment a[$i]=v (write, indexed)
run_sash 'array a=(x y z); int i=1; a[$i]=ZZ; echo ${a[1]}'
[[ $rc = 0 && $out = "ZZ" && -z $err ]]; chk 33.7

# 33.8 dynamic key assignment m[$k]=v (write, map)
run_sash 'map m=([a]=1); k=b; m[$k]=2; echo ${m[b]}'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk 33.8

# 33.11 bare string-key ASSIGNMENT on an indexed array → compile error
# (subscript-sigil invariant, DD#75 §4: enforce on write as on read 33.3)
run_sash 'array a=(x y z); a[i]=Q; echo ${a[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 33.11
# 33.12 literal + sigilled index assignments still work (not over-rejected)
run_sash 'array a=(x y z); int i=1; a[$i]=P; a[2]=Q; echo ${a[$i]}${a[2]}'
[[ $rc = 0 && $out = "PQ" && -z $err ]]; chk 33.12

# 33.9 flat map literal map m=(a 1 b 2) → reject; require [k]=v form
run_sash 'map m=(a 1 b 2); echo ${m[a]}'
[[ $rc != 0 && -z $out && $err == *SB4031* ]]; chk 33.9

# 33.10 keys + dynamic subscript idiom iterates key/value pairs
run_sash 'map m=([a]=1 [b]=2); for k in $(keys m); do echo "$k=${m[$k]}"; done'
[[ $rc = 0 && $out = $'a=1\nb=2' && -z $err ]]; chk 33.10

# 35.1 stackable: const int (DD#86: `local` axis retired; a fn-local const int)
run_sash 'fn f { const int x=5; echo $x; }; f'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk 35.1

# 35.2 const on array → compile error
run_sash 'const array a=(1 2 3)'
[[ $rc != 0 && -z $out && -n $err ]]; chk 35.2

# 35.3-35.9 SB4044: a const/readonly variable cannot be reassigned (validate
# check + transform backstop). Front-end rejects — all modes.
# 35.3 plain reassignment
run_sash 'const x=1; x=2'
[[ $rc != 0 && -z $out && $err == *SB4044* ]]; chk 35.3
# 35.4 append (+=)
run_sash 'const x=1; x+=2'
[[ $rc != 0 && -z $out && $err == *SB4044* ]]; chk 35.4
# 35.5 arithmetic increment `(( x++ ))`
run_sash 'const x=1; (( x++ ))'
[[ $rc != 0 && -z $out && $err == *SB4044* ]]; chk 35.5
# 35.6 arithmetic assignment `(( x=… ))`
run_sash 'const x=1; (( x=5 ))'
[[ $rc != 0 && -z $out && $err == *SB4044* ]]; chk 35.6
# 35.7 re-declaration of a const is SB5049, not SB4044. The user did not
# reassign anything, so "cannot be reassigned" described the wrong act; this
# is one name declared twice in one scope (DD#86), which is what SB5049 says.
# Both codes used to fire. SB4044 still covers a bare `x=2` (35.3).
run_sash 'const x=1; const x=2'
[[ $rc != 0 && -z $out && $err == *SB5049* && $err != *SB4044* ]]; chk 35.7
# 35.8 backstop: `read` into a const (a channel the validate walk doesn't
# enumerate — caught by VersionState::define so it can never panic)
run_sash 'const x=1; readline x'
[[ $rc != 0 && -z $out && $err == *SB4044* ]]; chk 35.8
# 35.9 no false positive: declaring then reading a const is fine
run_sash 'const x=1; echo $x'
[[ $rc = 0 && $out = 1 && -z $err ]]; chk 35.9

# DD#36 (fixed-point dtoi/itod) is SUPERSEDED — the pair was dropped as YAGNI
# (DG 2026-09-13). They are now ordinary unknown tokens: SB5011 everywhere, with
# no removed-builtin special case. 36.2/36.3 retired with them.
# 36.1 dtoi is gone — an unknown command, in every position
run_sash 'int x=$(dtoi 2 "3.14"); echo $x'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk 36.1

# 36.4 itod likewise — value position and statement position alike
run_sash 'int x=314; echo $(itod 2 $x)'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk 36.4

# 36.5 $(len arr) is int-typed — assignable to an int var (was wrongly typed
#      string → SB4030). len yields an array's element count.
run_sash 'array a=(x y z); int n=$(len a); echo $n'
[[ $rc = 0 && $out = "3" && -z $err ]]; chk 36.5

# 36.6 $(epochseconds) is int-typed too.
run_sash 'int n=$(epochseconds); echo $((n > 0))'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk 36.6

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

# 58.30 `n += $(f)` likewise captures + adds the return value (n: 0 → 42).
run_sash 'fn f -> int { return 42; }
int n=0; n+=$(f); echo $n'
[[ $rc = 0 && $out = "42" && -z $err ]]; chk 58.30

# 58.31 the arithmetic form is equivalent — same result.
run_sash 'fn f -> int { return 42; }
int n=$((f)); echo $n'
[[ $rc = 0 && $out = "42" && -z $err ]]; chk 58.31

# 58.32 Option C in a command ARGUMENT: `echo $(f)` prints the return value,
#       and it works embedded in a double-quoted string too.
run_sash 'fn f -> int { return 42; }
echo "port=$(f)"'
[[ $rc = 0 && $out = "port=42" && -z $err ]]; chk 58.32

# 58.33 `$(f arg…)` passes positional args to the int function.
run_sash 'fn add -> int { return $(( $1 + $2 )); }
int n=$(add 3 4); echo $n'
[[ $rc = 0 && $out = "7" && -z $err ]]; chk 58.33

# 58.34 a sexpr call arg that is itself a PARENTHESISED arith expression
# `(dbl (2 + 1))` computes dbl(3)=6. Regression guard for
# bug-arith-sexpr-nested-cmdsub-arg: the `$(( ))` content collector must count
# individual parens, so the `))` of `(2 + 1))` are two single closes, not the
# arith terminator (which truncated the content and yielded a silent 0).
run_sash 'fn dbl -> int { return $(( $1 * 2 )); }
int x=$(( (dbl (2 + 1)) )); echo $x'
[[ $rc = 0 && $out = "6" && -z $err ]]; chk 58.34
# 58.35 the same nested-paren sexpr arg works in a `(( ))` condition
run_sash 'fn dbl -> int { return $(( $1 * 2 )); }
if (( (dbl (2 + 1)) == 6 )); then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 58.35

# --- DD#30 unset semantics ---

# 30.40 unset var (scalar) → compile error
run_sash 'x=hello; unset x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 30.40

# 30.41 unset arr[N] removes element
run_sash 'array a=(one two three); unset a[1]; echo ${a[0]} ${a[2]}'
[[ $rc = 0 && $out = "one three" && -z $err ]]; chk 30.41

# 30.42 unset map[key] removes key
run_sash 'map m=([a]=1 [b]=2); unset m[a]; echo ${m[b]}'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk 30.42

# 30.43 unset arr clears collection (canonical length $(len a); DD#75 §4).
run_sash 'array a=(one two three); unset a; echo $(len a)'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk 30.43

# 30.44 the unset target is a BINDING, not a value. A quoted target slipped
# past the scalar rejection (30.40) entirely: `unset "x"` compiled clean and
# silently did nothing, because the check skipped any operand it could not
# read as a name.
run_sash 'x=hello; echo $x; unset "x"'
[[ $rc != 0 && $err == *SB5063* ]]; chk 30.44

# 30.45 a sigilled target is a value, not a binding → rejected
run_sash 'array a=(x); n=a; echo "$(len a)$n"; unset $n'
[[ $rc != 0 && $err == *SB5063* ]]; chk 30.45

# 30.46 `unset arr[N]` is not a no-op. 30.41 above asserts only the two
# *untouched* neighbours, so it passed while the delete matched nothing: an
# indexed array keys its entries by the 8 raw bytes of an i64, and `unset`
# deleted with the ASCII key "1". The length and the emptied slot pin it.
run_sash 'array a=(one two three)
unset a[1]
echo "n=$(len a) 0=${a[0]} 1=[${a[1]}] 2=${a[2]}"'
[[ $rc = 0 && $out = 'n=2 0=one 1=[] 2=three' && -z $err ]]; chk 30.46

# 30.47 index 0 is not special
run_sash 'array a=(x y z)
unset a[0]
echo "n=$(len a) 0=[${a[0]}] 1=${a[1]}"'
[[ $rc = 0 && $out = 'n=2 0=[] 1=y' && -z $err ]]; chk 30.47

# 30.48 deleting every element empties the array
run_sash 'array a=(x y)
unset a[0]
unset a[1]
echo "n=$(len a)"'
[[ $rc = 0 && $out = "n=0" && -z $err ]]; chk 30.48

# 30.49 a deleted index is writable again — the length counts live entries,
# not a high-water mark
run_sash 'array a=(x y z)
unset a[1]
a[1]=Q
echo "n=$(len a) 1=${a[1]}"'
[[ $rc = 0 && $out = "n=3 1=Q" && -z $err ]]; chk 30.49

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

# 62.15 DD#118: -n on a map element (the implicit form is retired)
run_sash 'map m=([a]=x); k=a; if [[ -n ${m[$k]} ]]; then echo yes; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 62.15

# 62.14 indexed dynamic key ${a[$i]} stays integer-keyed in [[ ]].
run_sash 'array a=(10 20 30); int i=2; if [[ ${a[$i]} == 30 ]]; then echo yes; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 62.14

# --- subscript append a[i]+=v (read-modify-write).
# 33.40 indexed literal-index append concatenates onto the current element
run_sash 'array a=(1 2 3); a[0]+=9; echo ${a[0]}'
[[ $rc = 0 && $out = "19" && -z $err ]]; chk 33.40

# 33.41 indexed dynamic-index append
run_sash 'array a=(x y); int i=1; a[$i]+=z; echo ${a[1]}'
[[ $rc = 0 && $out = "yz" && -z $err ]]; chk 33.41

# 33.13 map subscript append
run_sash 'map m=([k]=ab); m[k]+=cd; echo ${m[k]}'
[[ $rc = 0 && $out = "abcd" && -z $err ]]; chk 33.13

# 33.42 `echo @a` is a clean compile error (SB5030), not an ICE — echo joins
# its args, so an array splat there would flatten the array (DD#22/DD#69 §3);
# the user writes the explicit `echo $(join a)`.
run_sash 'array a=(one two three); echo @a'
[[ $rc != 0 && -z $out && $err == *SB5030* ]]; chk 33.42

# 33.43 the directed-to explicit join compiles and prints the joined string
run_sash 'array a=(one two three); echo $(join a)'
[[ $rc = 0 && $out = "one two three" && -z $err ]]; chk 33.43

# 33.23 `$(join)` with no args is a clean SB5040 (was a compiler ICE panic)
run_sash 'echo $(join)'
[[ $rc != 0 && -z $out && $err == *SB5040* ]]; chk 33.23
# 33.24 `$(join arr x y)` with >2 args rejects (extras were silently dropped)
run_sash 'array a=(1 2); echo $(join a x y)'
[[ $rc != 0 && -z $out && $err == *SB5040* ]]; chk 33.24

# 33.44 the reject also fires for a mixed splat (`echo x @a y`)
run_sash 'array a=(one two); echo x @a y'
[[ $rc != 0 && -z $out && $err == *SB5030* ]]; chk 33.44

# 33.45 $(keys m) is an array splat: in command-arg position it is SB5025
# (the DD#60/#75 context rule, same as $(argv)) — not a silent space-join.
run_sash 'map m=([a]=1 [b]=2); printf "%s\n" $(keys m)'
[[ $rc != 0 && -z $out && $err == *SB5025* ]]; chk 33.45
# 33.46 $(keys m) in scalar-assignment context → SB5025 (array splat in scalar)
run_sash 'map m=([a]=1 [b]=2); x=$(keys m); echo $x'
[[ $rc != 0 && -z $out && $err == *SB5025* ]]; chk 33.46

# 33.47 the SB5025 splat message names the map the program actually declares.
# It used to quote a fixed exemplar `$(keys m)` whatever the map was called,
# so the backticked "your expression" named a map nobody had written.
run_sash 'map config=([a]=1 [b]=2); echo $(keys config)'
[[ $rc != 0 && -z $out && $err == *'`$(keys config)`'* && $err != *'$(keys m)'* ]]; chk 33.47
# 33.48 a non-name operand is SB5062's to report, not SB5025's to paraphrase.
# `keys` takes a bare map name, so `$(keys "config")` can never work — quoting
# it back inside "valid only as a for-in source" would suggest a `for` line
# that is itself rejected. Same diagnostic in every position, too: the for-in
# form already said SB5062 while command-arg position said SB5025.
run_sash 'map config=([a]=1 [b]=2); echo $(keys "config")'
[[ $rc != 0 && -z $out && $err == *SB5062* && $err != *SB5025* ]]; chk 33.48
# 33.49 …and the for-in position agrees, rather than reporting the context rule
run_sash 'map config=([a]=1 [b]=2); for k in $(keys "config") { echo $k; }'
[[ $rc != 0 && -z $out && $err == *SB5062* ]]; chk 33.49

# 33.19–33.21 join arr SEP — DD#75: array first, separator second.
run_sash 'array a=(x y z); echo $(join a ",")'
[[ $rc = 0 && $out = "x,y,z" && -z $err ]]; chk 33.19
run_sash 'array a=(x y z); echo $(join a ", ")'
[[ $rc = 0 && $out = "x, y, z" && -z $err ]]; chk 33.20
run_sash 'array a=(x y z); echo $(join a)'
[[ $rc = 0 && $out = "x y z" && -z $err ]]; chk 33.21
# 33.22 join on a NON-array operand is a clean compile error, never a segfault
run_sash 's=hi; echo $(join s ",")'
[[ $rc != 0 && -z $out && -n $err ]]; chk 33.22

# DD#75 §2 — polymorphic `len`: dispatch on the operand's TYPE, not the `$`
# sigil (array/map → count; string → byte length; undefined → compile error).
# 75.1 strlen on a string → byte length (was garbage: arrlen on a non-table)
run_sash 's=hello; int n=$(strlen $s); echo $n'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk 75.1
# 75.2 len on an array → element count
run_sash 'array a=(1 2 3); int n=$(len a); echo $n'
[[ $rc = 0 && $out = "3" && -z $err ]]; chk 75.2
# 75.3 len on a map → entry count
run_sash 'map m=([x]=1 [y]=2); int n=$(len m); echo $n'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk 75.3
# 75.4 the `${#s}` string-length sugar is retired → SB4038 (use $(len s))
run_sash 's=hello; int n=${#s}; echo $n'
[[ $rc != 0 && -z $out && $err == *SB4038* ]]; chk 75.4
# 75.5 the `${#a}` array-length sugar is retired → SB4038 (use $(len a))
run_sash 'array a=(1 2 3); int n=${#a}; echo $n'
[[ $rc != 0 && -z $out && $err == *SB4038* ]]; chk 75.5
# 75.6 `${#a}` inside $(( )) is retired too → SB4038 (use (len a))
run_sash 'array a=(x y z w); echo $(( ${#a} + 1 ))'
[[ $rc != 0 && -z $out && $err == *SB4038* ]]; chk 75.6

# DD#75 §3 — `keys` is map-only (an indexed array's keys are trivially
# 0..len-1; a scalar has none). Only maps have non-trivial string keys.
# 75.7 keys on a map → the string keys (still works)
run_sash 'map m=([a]=1 [b]=2); for k in $(keys m); do echo "$k"; done'
[[ $rc = 0 && $out = $'a\nb' && -z $err ]]; chk 75.7
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
# 75.13 length ${#a[@]} → reject (use $(len a))
run_sash 'array a=(x y z); int n=${#a[@]}; echo $n'
[[ $rc != 0 && -z $out && -n $err ]]; chk 75.13
# 75.14 keys ${!a[@]} → reject (use $(keys m))
run_sash 'array a=(x y z); for k in ${!a[@]}; do echo "$k"; done'
[[ $rc != 0 && -z $out && -n $err ]]; chk 75.14
# 75.15 canonical @a splat still works (the replacement)
run_sash 'array a=(x y z); for w in @a; do echo "$w"; done'
[[ $rc = 0 && $out = $'x\ny\nz' && -z $err ]]; chk 75.15
# 75.16 indexed element access ${a[$i]} / ${a[1]} unaffected
run_sash 'array a=(x y z); int i=2; echo ${a[$i]}${a[1]}'
[[ $rc = 0 && $out = "zy" && -z $err ]]; chk 75.16

# DD#75 §5 — positional parameters are the implicit 0-indexed `args` array
# ($1..$9 ≡ ${args[0]}..${args[8]}; $# ≡ $(len args); $@ ≡ @args). Amended
# 2026-08-21: 0-indexed (was 1-indexed) + `shift` dropped. Each inline gets a
# uniquely-named args array, so nested/recursive calls never clobber positionals.
# 75.17 $1 is the first argument
run_sash 'fn f { echo $1; }
f hello'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 75.17
# 75.18 $# is the argument count (= $(len args))
run_sash 'fn f { : @args; echo $#; }
f a b c'
[[ $rc = 0 && $out = "3" && -z $err ]]; chk 75.18
# 75.19 a nested function call does NOT clobber the caller's positionals
run_sash 'fn g { echo $1; }
fn f { g Z; echo $1; }
f A'
[[ $rc = 0 && $out = $'Z\nA' && -z $err ]]; chk 75.19
# 75.20 user-written ${args[i]} reads the same array. DD#81: `: ${args[1]}`
# acknowledges the skipped middle param (fixed arity must read args in order).
run_sash 'fn f { : ${args[1]}; echo ${args[0]} ${args[2]}; }
f a b c'
[[ $rc = 0 && $out = "a c" && -z $err ]]; chk 75.20
# 75.21 @args splat iterates every argument
run_sash 'fn f { for x in @args; do echo "$x"; done; }
f a b'
[[ $rc = 0 && $out = $'a\nb' && -z $err ]]; chk 75.21
# 75.22 more than 9 args are reachable via ${args[N]} (the old scalar model
# capped at $9). DD#81: reaching a high index without the lower ones
# makes the function variadic — `: @args` marks it (takes any number).
run_sash 'fn f { : @args; echo ${args[10]}; }
f a b c d e f g h i j k'
[[ $rc = 0 && $out = "k" && -z $err ]]; chk 75.22
# 75.23 `$(len args)` is the arg count (= $#), reading the implicit array — the
# intrinsic must resolve `args`.
run_sash 'fn f { : @args; echo $(len args); }
f a b c'
[[ $rc = 0 && $out = "3" && -z $err ]]; chk 75.23
# 75.24 `$(join args SEP)` joins the implicit array.
run_sash 'fn f { echo $(join args ,); }
f a b c'
[[ $rc = 0 && $out = "a,b,c" && -z $err ]]; chk 75.24
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
# 75.28 no false positive: `strlen` of a declared scalar and `len` of an
#       array both compile
run_sash 's=hi; array a=(1 2 3); echo "$(strlen $s) $(len a)"'
[[ $rc = 0 && $out = "2 3" && -z $err ]]; chk 75.28
# 75.29 a loop variable is a valid len operand (the type_check-side gap this
# avoids: SB5031's scope machinery already tracks `for` variables)
run_sash 'array a=(hi bye); for e in @a; do echo $(strlen $e); done'
[[ $rc = 0 && $out = $'2\n3' && -z $err ]]; chk 75.29
# 75.30 `len <typo>` inside a function is also rejected
run_sash 'fn f { echo $(len bogus); }
f'
[[ $rc != 0 && -z $out && $err == *SB5039* ]]; chk 75.30

# ── SB5062: a collection/variable operand must be a plain NAME ──────────
# `len`/`join`/`keys` resolve their operand as a variable NAME, not a value.
# Any other spelling used to be stringified by a source-text formatter and
# then looked up as if it were a name, which silently found nothing (0 or
# empty) or crashed the compiler. Reject the spelling instead.
# 75.31 a quoted expansion is not a name — it silently read as 0
run_sash 's=hi; echo $(len "$s")'
[[ $rc != 0 && -z $out && $err == *SB5062* ]]; chk 75.31
# 75.32 a subscript cell is not a name — silently read as 0
run_sash 'array a=(x y); echo $(len ${a[0]})'
[[ $rc != 0 && -z $out && $err == *SB5062* ]]; chk 75.32
# 75.33 a quoted literal is not a name — silently read as 0
run_sash 'echo $(len "lit")'
[[ $rc != 0 && -z $out && $err == *SB5062* ]]; chk 75.33
# 75.34 `join $a` silently produced empty output (only bare `a`/`@a` resolve)
run_sash 'array a=(x y); echo $(join $a ",")'
[[ $rc != 0 && -z $out && $err == *SB5062* ]]; chk 75.34
# 75.35 a quoted operand to `join` was not even counted as a read of `a`
run_sash 'array a=(x y); echo $(len a); echo $(join "a" ",")'
[[ $rc != 0 && -z $out && $err == *SB5062* ]]; chk 75.35

# 75.36 `keys @m` ICEd the compiler — a splat is not a map reference
run_sash 'map m=([k]=v); for k in $(keys @m); do echo "$k"; done'
[[ $rc != 0 && -z $out && $err == *SB5062* ]]; chk 75.36
# 75.37 `keys $m` ICEd the same way
run_sash 'map m=([k]=v); for k in $(keys $m); do echo "$k"; done'
[[ $rc != 0 && -z $out && $err == *SB5062* ]]; chk 75.37
# 75.38 control: every spelling that survives still works. Three that used to
# be here no longer exist: `len s` / `len $s` are `strlen` now (a scalar is not
# a collection), and `len @a` / `join @a SEP` are gone with DD#100 Part D.
run_sash 's=hi; array a=(x y z); map m=([k]=v)
echo "$(strlen $s) $(len a) $(len m)"
echo "$(join a ",")"
for k in $(keys m); do echo "$k"; done'
[[ $rc = 0 && $out = $'2 3 1\nx,y,z\nk' && -z $err ]]; chk 75.38

# ── SB5064: `$a` on a collection is an error (DG 2026-09-11) ──
# It was neither rejected nor meaningful: every value context silently
# expanded to nothing, while the `len` operand slot resolved the same
# spelling as the collection NAME (`len $a` on a 3-element array gave 3).

# 75.39 scalar-sigil read of an array in a value context
run_sash 'array a=(x y z); echo $a'
[[ $rc != 0 && -z $out && $err == *SB5064* ]]; chk 75.39

# 75.40 same for a map
run_sash 'map m=([k]=v); echo $m'
[[ $rc != 0 && -z $out && $err == *SB5064* ]]; chk 75.40

# 75.41 assigning from it (silently produced an empty string)
run_sash 'array a=(x y z); str x=$a; echo $x'
[[ $rc != 0 && -z $out && $err == *SB5064* ]]; chk 75.41

# 75.42 the operand slot: `len $a` returned the ARRAY length for a spelling
# that is not the array
run_sash 'array a=(x y z); echo $(len $a)'
[[ $rc != 0 && -z $out && $err == *SB5064* ]]; chk 75.42

# 75.43 control: the correct spellings are unaffected.
run_sash 'array a=(x y z); str s=hi
echo "${a[1]} $(len a) $(join a ",") $s $(strlen $s)"'
[[ $rc = 0 && $out = "y 3 x,y,z hi 2" && -z $err ]]; chk 75.43

# ── SB5064 must see AUTO-declared collections too (P0, 2026-09-28) ──
# `read`, `argv` and the 3-argument `match`/`matchall` auto-declare their
# target (DD#99, DD#60, DD#63) — but SB5064 consulted a collection set built
# only from explicit `array`/`map` declarations, so `$x` on an auto-declared
# target sailed straight past the guard written for exactly that spelling.
# What it sailed into was a memory disclosure: codegen stores the `SashTable*`
# in the SCALAR pointer slot with length 0, `sash_argv_guard` validates
# nothing at length 0, and `execve` then read that raw heap pointer to the
# first NUL — printing a live address. Identical on both backends.
# Was: bug-autodeclared-collection-scalar-read-leaks-pointer.

# 75.101 `argv` — the disclosure repro itself
run_sash 'argv x
attest e in x [[ -n $e ]]
/bin/echo $x'
[[ $rc != 0 && -z $out && $err == *SB5064* ]]; chk 75.101

# 75.102 `read` auto-declares the same way
run_sash 'read x
attest e in x [[ -n $e ]]
/bin/echo $x'
[[ $rc != 0 && -z $out && $err == *SB5064* ]]; chk 75.102

# 75.103 the 3-argument `match` captures into a collection
run_sash 'str s=abc
match "$s" "b" x
attest e in x [[ -n $e ]]
/bin/echo $x'
[[ $rc != 0 && -z $out && $err == *SB5064* ]]; chk 75.103

# 75.104 …and so does `matchall`
run_sash 'str s=abc
matchall "$s" "b" x
attest e in x [[ -n $e ]]
/bin/echo $x'
[[ $rc != 0 && -z $out && $err == *SB5064* ]]; chk 75.104

# 75.105 the assignment context is caught too — this is how the pointer got
# laundered into a second variable
run_sash 'argv x
attest e in x [[ -n $e ]]
str y=$x
echo "$y"'
[[ $rc != 0 && -z $out && $err == *SB5064* ]]; chk 75.105

# 75.106 control: `readline` binds a SCALAR (DD#99), so `$x` stays legal —
# the new set must not over-collect it
printf 'one\n' > t75.lines
run_sash 'readline x < t75.lines
attest x [[ -n $x ]]
echo "[$x]"'
[[ $rc = 0 && $out = "[one]" && -z $err ]]; chk 75.106

# 75.107 control: the correct spellings on an auto-declared collection still
# compile and run
run_sash 'argv x
attest e in x [[ -n $e ]]
echo "${x[1]} ${x[2]} $(len x)"' p q
[[ $rc = 0 && $out = "p q 3" && -z $err ]]; chk 75.107

# ── SB5068: a subscript on a scalar is an error (2026-09-12) ──
# The exact dual of SB5064. `${s[0]}` was neither supported nor rejected: it
# lowered to a table read, so the scalar's char* was dereferenced as a
# SashTable*. One type mistake, seven measured outcomes -- SEGFAULT in six
# spellings and a compiler ICE in the seventh.
# Was: bug-scalar-subscript-segfault.

# 75.44 read form -- segfaulted (rc 139)
run_sash 'str s=hello; echo "${s[0]}"'
[[ $rc != 0 && -z $out && $err == *SB5068* ]]; chk 75.44

# 75.45 dynamic index -- segfaulted
run_sash 'str s=hello; str k=x; echo "${s[$k]}"'
[[ $rc != 0 && -z $out && $err == *SB5068* ]]; chk 75.45

# 75.46 int subject -- ICEd the compiler at codegen/types.rs
run_sash 'int n=5; echo "${n[0]}"'
[[ $rc != 0 && -z $out && $err == *SB5068* ]]; chk 75.46

# 75.47 loop variable -- the face type_check could not have caught, because a
# for-variable is never declared into TypeEnv.vars
run_sash 'array a=(1 2 3); for x in @a; do echo "${x[0]}"; done'
[[ $rc != 0 && -z $out && $err == *SB5068* ]]; chk 75.47

# 75.48 write form -- segfaulted
run_sash 'str s=hello; s[0]=x; echo "$s"'
[[ $rc != 0 && -z $out && $err == *SB5068* ]]; chk 75.48

# 75.49 arithmetic operand -- segfaulted
run_sash 'str s=hello; echo $(( s[0] + 1 ))'
[[ $rc != 0 && -z $out && $err == *SB5068* ]]; chk 75.49

# 75.50 [[ ]] operand -- segfaulted
run_sash 'str s=hello; if [[ ${s[0]} == h ]]; then echo y; fi'
[[ $rc != 0 && -z $out && $err == *SB5068* ]]; chk 75.50

# 75.51 an undefined name stays SB5031 -- an unintroduced variable is a
# different mistake and keeps its own diagnostic
run_sash 'echo "${nope[0]}"'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk 75.51

# 75.52 control: a declared collection still subscripts, read and write
run_sash 'array a=(x y z); map m=([k]=v)
a[0]=w
echo "${a[0]} ${a[1]} ${m[k]}"'
[[ $rc = 0 && $out = "w y v" && -z $err ]]; chk 75.52

# 75.53 a bulk-fill target is subscriptable with no declaration at all --
# `argv g` introduces the collection, so SB5068 must not fire (taint still
# demands the attest, which is SB5020's job)
run_sash 'argv g; echo "${g[0]}"'
[[ $rc != 0 && -z $out && $err == *SB5020* && $err != *SB5068* ]]; chk 75.53

# 75.54 control: `args` is the other undeclared collection, and subscripts.
# (The DD#100 slots A..E are the third undeclared case; spec/collparam.sh
# covers them.)
run_sash 'f() { echo "${args[0]}"; }
f one'
[[ $rc = 0 && $out = "one" && -z $err ]]; chk 75.54

# ── DD#75 §2 / DD#102: `strlen` names the byte-length question, of a VALUE ──
# `len` dispatched on the operand's TYPE — bytes for a scalar, elements for a
# collection — so two different questions shared one spelling with nothing at
# the call site to say which was being asked. `strlen` asks for bytes outright
# and does not dispatch. It is int-typed like `len`, so it is legal in `$( )`,
# as an `int` initialiser, and as an arith sexpr operand.
#
# DD#102: the operand is a VALUE, lowered exactly as `toupper`'s is. `strlen`
# was the only non-collection intrinsic that resolved a NAME, so the adjacent
# `toupper s` and `strlen s` asked about different things — the first about the
# one-character string "s", the second about the variable `s`.
run_sash 'str s = hello
echo $(strlen $s)'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk 75.55

# 75.56 int-typed: assigning to an `int` is not SB4030
run_sash 'str s = hello
int n = $(strlen $s)
echo $n'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk 75.56

# 75.57 empty scalar is 0 bytes, not an error
run_sash 'str s = ""
echo $(strlen $s)'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk 75.57

# 75.58 `$s` and `s` are DIFFERENT operands, exactly as they are to `toupper`:
# the sigil is the whole difference between the variable and a literal word.
# Before DD#102 both spellings named the variable and this printed "5 5".
run_sash 'str s = hello
echo "$(strlen $s) $(strlen s)"'
[[ $rc = 0 && $out = "5 1" && -z $err ]]; chk 75.58

# 75.59 arith sexpr operand (DD#58).
run_sash 'str s = hello
int n = $(( (strlen $s) * 2 ))
echo $n'
[[ $rc = 0 && $out = "10" && -z $err ]]; chk 75.59

# 75.60 the sexpr operand keeps the same distinction. The rebuild that feeds
# an int intrinsic from sexpr args collapsed `Var` and `DollarVar` into one
# `Literal` — lossless while every int intrinsic resolved a name, and silently
# wrong the moment `strlen` took a value: `(strlen $s)` measured the name.
run_sash 'str s = hello
echo "$s" > /dev/null
int n = $(( (strlen s) * 2 ))
echo $n'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk 75.60

# 75.61 a collection name is a word like any other — `strlen a` measures the
# one-character string "a", just as `toupper a` yields "A". This used to be
# SB4030 ("needs a scalar"), which only made sense while the operand was a
# name. `len` remains the way to count a collection.
run_sash 'array a = ( x y z )
echo "$(strlen a) $(len a)"'
[[ $rc = 0 && $out = "1 3" && -z $err ]]; chk 75.61

# 75.62-75.65 every spelling `len` cannot resolve as a name, `strlen` now
# takes as a value. All four were SB5062/SB5039 rejections under the name
# operand; under DD#102 each is an ordinary argument evaluation.
run_sash 'str s = hello
echo $(strlen "$s")'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk 75.62

# a bare word is its own text, not a missing variable
run_sash 'echo $(strlen hello)'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk 75.63

run_sash 'echo $(strlen $(echo hi))'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk 75.64

# 75.65 the cell form, which the name operand could not express at all. A cell
# is a scalar, so `${a[0]}` was always a coherent question; it was rejected
# only because `a[0]` is not a name. DD#102 dissolves the question — a cell
# interpolates like any other value. (The converter needs this for bash's
# `${#a[i]}`.)
run_sash 'array a = ( hello xy )
echo $(strlen ${a[0]})'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk 75.65

# ── DD#77 `[[ i? EXPR ]]` — strict integer-validity test ──
# True iff the operand's value is a complete integer (sign + decimal digits, or
# a 0x/0o/0b prefix with valid digits; empty/unset → false). Lets an author
# validate/branch before converting.
# DD#93 Part C renamed this operator from `-i` to `i?` (hard, no alias).
# 77.1 literal integer → true
run_sash 'if [[ i? 42 ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 77.1
# 77.2 non-numeric → false
run_sash 'if [[ i? abc ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk 77.2
# 77.3 a set numeric string var → true
run_sash 's=42; if [[ i? $s ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 77.3
# 77.4 partial-numeric (trailing junk) → false (strict, no partial parse)
run_sash 's=42abc; if [[ i? $s ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk 77.4
# 77.5 negative and radix-prefixed → true
run_sash 's=-7; if [[ i? $s ]] && [[ i? 0xff ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 77.5
# 77.6 empty → false
run_sash 's=; if [[ i? $s ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk 77.6
# 77.7 DG's example: validate-or-sanitize, then use (composes with =~ s///g)
run_sash 's=42abc; [[ i? $s ]] || s =~ s/\D//g; echo $s'
[[ $rc = 0 && $out = "42" && -z $err ]]; chk 77.7
# 77.8 range-checked: i64::MAX is valid, MAX+1 is not (DD#77, DG). All backends.
run_sash 'if [[ i? 9223372036854775807 ]] && ! [[ i? 9223372036854775808 ]]; then echo ok; else echo no; fi'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 77.8
# 77.9 a validated value converts safely; `i?` guarantees no overflow trap.
run_sash 's=100; [[ i? $s ]] && echo $(( ${s:-0} * 2 ))'
[[ $rc = 0 && $out = "200" && -z $err ]]; chk 77.9

# ── DD#77 conversion dispositions over the strict grammar ────────────
# 77.1-77.9 above cover the `i?` TEST (sash_is_int). These cover the
# CONVERSION — `${s:-N}` / `${s:?M}` in arithmetic, i.e. sash_atoi_or and
# sash_atoi_or_abort over parse_strict_i64. hint.2/hint.3 only ever passed
# a well-formed value, so every edge of the grammar was uncovered; the only
# tests of it were in sash-rt-tests/tests/unit_tests.rs, against the raw
# `sash_atoi`, whose lenient C-atoi behaviour no sash program can reach.
# 77.10 leading whitespace is not part of the grammar — `:-` takes the default
run_sash 's="  42"
int n=$(( ${s:-7} ))
echo $n'
[[ $rc = 0 && $out = "7" && -z $err ]]; chk 77.10

# 77.11 …and `:?` aborts with the given message rather than parsing it
run_sash 's="  42"
int n=$(( ${s:?not an int} ))
echo $n'
[[ $rc != 0 && $rerr == *"not an int"* ]]; chk 77.11

# 77.12 trailing junk is rejected whole — no partial parse to 42 (cf. 77.4)
run_sash 's="42abc"
int n=$(( ${s:-7} ))
echo $n'
[[ $rc = 0 && $out = "7" && -z $err ]]; chk 77.12

# 77.13 a leading `+` IS part of the grammar and converts
run_sash 's="+42"
int n=$(( ${s:-7} ))
echo $n'
[[ $rc = 0 && $out = "42" && -z $err ]]; chk 77.13

# 77.14 leading zeros are decimal, not octal — `007` is 7, not an error
run_sash 's="007"
int n=$(( ${s:-9} ))
echo $n'
[[ $rc = 0 && $out = "7" && -z $err ]]; chk 77.14

# 77.15 out-of-range input is a DISPOSITION concern, not an arithmetic
# overflow trap (DD#77): i64::MAX+1 is simply not a valid integer, so `:-`
# yields the default and the program exits 0. Contrast 25.8, where an
# operation overflows and the trap fires.
run_sash 's="9223372036854775808"
int n=$(( ${s:-7} ))
echo $n'
[[ $rc = 0 && $out = "7" && -z $err ]]; chk 77.15

# 77.16 …while i64::MIN itself is in range and converts exactly
run_sash 's="-9223372036854775808"
int n=$(( ${s:-7} ))
echo $n'
[[ $rc = 0 && $out = "-9223372036854775808" && -z $err ]]; chk 77.16

# ── DD#93 Part C: content-shape tests `p?` / `g?` ──
# Sibling shape tests to `i?`, spelled with the same `?` suffix (kept out of
# bash's saturated `-X` namespace). Each is "non-empty AND every byte in an
# ASCII class": `p?` printable (0x20–0x7E), `g?` graph (0x21–0x7E, printable
# non-space). General operators — valid anywhere a unary test is, not only in
# `attest`.
# 93.10 p? on printable text → true
run_sash 'if [[ p? hello ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 93.10
# 93.11 p? admits the space (0x20) — that is the whole difference from g?
run_sash 's="a b"; if [[ p? $s ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 93.11
# 93.12 p? on empty → false (non-empty is required, as for i?)
run_sash 's=; if [[ p? $s ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk 93.12
# 93.13 g? on printable non-space → true (punctuation/digits included)
run_sash 's=/usr/bin:42; if [[ g? $s ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 93.13
# 93.14 g? rejects an embedded space
run_sash 's="a b"; if [[ g? $s ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk 93.14
# 93.15 g? on empty → false
run_sash 's=; if [[ g? $s ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk 93.15
# 93.16 ASCII-only: high bytes (≥ 0x80) are rejected by both. A UTF-8-aware
# variant would get a distinct spelling (DD#93); ambiguous non-ASCII is part
# of what a shape test screens out.
run_sash 'if [[ p? café || g? café ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk 93.16
# 93.17 general operators: compose with ! and && like any unary test
run_sash 's=abc; if ! [[ i? $s ]] && [[ g? $s ]]; then echo ok; else echo no; fi'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 93.17
# 93.18 …and drive a while loop (not attest-only)
run_sash 'n=3; while [[ i? $n ]]; do echo $n; n=; done'
[[ $rc = 0 && $out = "3" && -z $err ]]; chk 93.18
# 93.19 the `-i` → `i?` rename is hard: `-i` is no longer an operator, so it
# is a bare word and the test no longer parses (DD#93 Part C, no alias).
run_sash 'if [[ -i 42 ]]; then echo yes; else echo no; fi'
[[ $rc != 0 && -z $out && $err == *SB4014* ]]; chk 93.19


# ------------------------------------------- string operations on an int var
# Asking an int-typed slot for a string pointer panicked at codegen
# (`types.rs:29`, "expected string var, got int"). The comment there asserted
# the Int arm was "not user-reachable input"; it was. `${n#…}`/`${n^^}` are
# already rejected at parse (SB4034, DD#65 pruned operators), so the live
# cases were substring and a redirect target.

# ice.11 substring of an int variable is a type error, not a panic
run_sash 'int n=512; v=${n:1:2}; echo $v'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err != *ICE* ]]; chk ice.11

# ice.12 …in a command argument too
run_sash 'int n=512; echo ${n:1:2}'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err != *ICE* ]]; chk ice.12

# ice.13 an int variable as a redirect target is a type error, not a panic
run_sash 'int n=5; echo hi > $n'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err != *ICE* ]]; chk ice.13

# ── DD#75 §2 amendment + DD#100 Part D: one spelling per question ──
# `len` dispatched on the operand's TYPE — bytes for a scalar, elements for a
# collection — so two questions shared one spelling with nothing at the call
# site to say which was being asked. `len` is collection-only now and `strlen`
# asks for bytes. Separately, DD#100 Part D drops the splat operand: `len @a`
# and `join @a SEP` were exact synonyms of the bare-name forms.

# 75.66 `len <scalar>` names the replacement
run_sash 'str s = hello
echo $(len s)'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'$(strlen $s)'* ]]; chk 75.66

# 75.67 a loop variable is a scalar too — this was the polymorphic case
run_sash 'array a=(hi bye); for e in @a; do echo $(len e); done'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 75.67

# 75.68 `len` still counts an array, and `strlen` still measures a string
run_sash 'array a=(x y z); str s=hello
echo "$(len a) $(strlen $s)"'
[[ $rc = 0 && $out = "3 5" && -z $err ]]; chk 75.68

# 75.69 DD#100 Part D: the splat operand is gone from `len`
run_sash 'array a=(x y z); echo $(len @a)'
[[ $rc != 0 && -z $out && $err == *SB5062* ]]; chk 75.69

# 75.70 …and from `join`
run_sash 'array a=(x y z); echo $(join @a ",")'
[[ $rc != 0 && -z $out && $err == *SB5062* ]]; chk 75.70

# 75.71 `for … in @a` is NOT affected — a splat is still a for-in source and a
# command argument; only the name-resolving builtins stopped taking one.
run_sash 'array a=(x y z); for e in @a; do echo $e; done'
[[ $rc = 0 && $out = $'x\ny\nz' && -z $err ]]; chk 75.71

# 75.72 SB5025 used to recommend the spelling DD#100 Part D removes
run_sash 'array a=(x y); x=@a; /bin/echo "$x"'
[[ $rc != 0 && $err == *SB5025* && $err == *'$(join a)'* && $err != *'$(join @a)'* ]]; chk 75.72

# 75.73 an undeclared name given a collection value reports THAT, not a
# misleading "`f` is a scalar" from the new `len` check running first
run_sash 'f=(a b c); echo $(len f)'
[[ $rc != 0 && -z $out && $err == *SB5055* ]]; chk 75.73

# 75.74 `join ARRAY SEP` joins with the separator in every backend.
run_sash 'array a=(x y z); echo $(join a ,)'
[[ $rc = 0 && $out = "x,y,z" && -z $err ]]; chk 75.74

# 75.75 the one-arg bare-name form likewise lowers in every backend.
run_sash 'array a=(one two three); echo $(join a)'
[[ $rc = 0 && $out = "one two three" && -z $err ]]; chk 75.75

# 75.76 the SB5062 advice no longer recommends the splat DD#100 Part D
# removed — telling the user to write a spelling the validator rejects
run_sash 'array a=(x y); echo $(join "a b" ,)'
[[ $rc != 0 && $err == *SB5062* && $err != *splat* ]]; chk 75.76

# ── `len` is collection-only for BOTH spellings of a name (DG 2026-09-12) ──
# DD#75 §2 made `len` answer only the element-count question, and `len s` was
# rejected. `len $s` was not: the type check resolved an operand name through
# `join_operand_name`, which returned "" for a `$`-sigilled expansion, so the
# guard never ran and the byte length came back — the exact dispatch-on-type
# behaviour DD#75 removed, reachable by adding one character.

# 75.77 the spelling that slipped through
run_sash 'str s=hello; echo $(len $s)'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 75.77

# 75.78 it reports the same thing as the bare spelling, naming the variable
run_sash 'str s=hello; echo $(len $s)'
[[ $err == *'`s` is a scalar'* && $err == *strlen* ]]; chk 75.78

# 75.79 an int scalar is equally not a collection through the `$` spelling
run_sash 'int n=42; echo $(len $n)'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 75.79

# 75.80 control: the guard still fires on the bare spelling, and neither
# spelling disturbs a real collection operand
run_sash 'array a=(x y z); map m=([k]=v); echo "$(len a) $(len m)"'
[[ $rc = 0 && $out = "3 1" && -z $err ]]; chk 75.80

# ── Diagnostic advice must be a form the compiler accepts ──
# A diagnostic's message is a string literal: nothing compiles it and nothing
# tests it. When the accepted surface moves, the rejection stays correct and
# the advice silently rots — leaving the worst failure mode there is, where
# the compiler tells you to write something and then rejects that too.
# Three instances found: SB4035 (`${a[i]}`), SB5030 (`$(join @a)`), and the
# string→int hint below. Each pins BOTH the message text and a RUN of the
# form it recommends; asserting text alone is how SB5030 survived being
# found, written down, and assigned.

# hint.1 the string→int hint used to read "use `$(($var))`", which is itself
#        SB4030 — "a string→int conversion must carry a disposition". It sent
#        the user from one SB4030 to another.
run_sash 's=5
int n=$s
echo $n'
[[ $rc != 0 && $err == *SB4030* && $err == *'must carry a disposition'* \
   && $err != *'use `$(($var))`'* ]]; chk hint.1

# hint.2 …and the `:-` form it now recommends compiles and runs
run_sash 's=5
int n=$(( ${s:-0} ))
echo $n'
[[ $rc = 0 && $out = 5 && -z $err ]]; chk hint.2

# hint.3 …as does the `:?` form
run_sash 's=7
int n=$(( ${s:?not an int} ))
echo $n'
[[ $rc = 0 && $out = 7 && -z $err ]]; chk hint.3

# hint.4 the int→string direction: the hint names quoting, and quoting works
run_sash 's=hi
int v=1
s=$v
echo "$s"'
[[ $rc != 0 && $err == *SB4030* && $err == *'quote it'* ]]; chk hint.4

# hint.5 …and the recommended form runs
run_sash 's=hi
int v=1
s="$v"
echo "$s"'
[[ $rc = 0 && $out = 1 && -z $err ]]; chk hint.5

# hint.6 neither hint may name a placeholder that reads as a real variable.
#        `$var` looked like the user's own variable; `VAR` cannot be mistaken
#        for one. (The right-hand side is an arbitrary expression here — the
#        hint is not given its spelling, so a placeholder is the honest form.)
run_sash 's=5
int n=$s
echo $n'
[[ $rc != 0 && $err == *'${VAR:-0}'* && $err != *'$var'* ]]; chk hint.6

# ── map growth past the initial table capacity ───────────────────────
# sash_table_new floors capacity at 4, so table_grow (sash-rt/src/lib.rs)
# only runs at the 5th key. The largest map literal anywhere else in this
# corpus has 3 keys, so the rehash path had no spec coverage at all — its
# only test was sash-rt-tests/tests/unit_tests.rs, which aborts before it.
# 35.10 a map of 8 keys rehashes and every key still reads back
run_sash 'map m=([a]=1 [b]=2 [c]=3 [d]=4 [e]=5 [f]=6 [g]=7 [h]=8)
echo "${m[a]}${m[e]}${m[h]}"
echo $(len m)'
[[ $rc = 0 && $out = "158
8" && -z $err ]]; chk 35.10

# 35.11 keys inserted one at a time across the growth boundary survive it
run_sash 'map m=([k1]=v1)
m[k2]=v2; m[k3]=v3; m[k4]=v4; m[k5]=v5; m[k6]=v6
echo $(len m)
echo "${m[k1]}${m[k6]}"'
[[ $rc = 0 && $out = "6
v1v6" && -z $err ]]; chk 35.11

# --- SB5079: splatting a non-collection (the SB5068 defect, one op over) ---
# `@name` lowers to a collection traversal, so a scalar subject made the
# generated code read a `char*` as a `SashTable*`. Three segfaults and two
# compiler ICEs, all compiling clean beforehand.

# 75.81 a string subject in a for-in splat -- segfaulted
run_sash 'str s=hi
for x in @s {
echo $x
}'
[[ $rc != 0 && -z $out && $err == *SB5079* ]]; chk 75.81

# 75.82 a string subject to strlen -- segfaulted
run_sash 'str s=hi
echo $(strlen @s)'
[[ $rc != 0 && -z $out && $err == *SB5079* ]]; chk 75.82

# 75.83 an int subject -- compiler ICE (codegen/types.rs), as in SB5068
run_sash 'int i=5
for x in @i {
echo $x
}'
[[ $rc != 0 && -z $out && $err == *SB5079* ]]; chk 75.83

# 75.84 a scalar splatted into a user function -- segfaulted
run_sash 'fn f {
echo $(len args)
}
str s=hi
f @s'
[[ $rc != 0 && -z $out && $err == *SB5079* ]]; chk 75.84

# 75.85 an unintroduced name. SB5031 enumerates scalar READS, and a splat is
# not one, so unlike the subscript case this was NOT already covered -- it
# ICE'd. Reported under SB5031 all the same: it is the same mistake.
run_sash 'for x in @nosuch {
echo $x
}'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk 75.85

# 75.86 an array splat still traverses -- guards against over-rejection
run_sash 'array a=(1 2)
for x in @a {
echo $x
}'
[[ $rc = 0 && $out == *"1"* && $out == *"2"* && -z $err ]]; chk 75.86

# 75.87 a map splat still traverses its values
run_sash 'map m=([k]=v [j]=w)
for x in @m {
echo $x
}'
[[ $rc = 0 && $out == *"v"* && $out == *"w"* && -z $err ]]; chk 75.87

# 75.88 strlen of a collection splat is REJECTED (SB5070). DD#102 made strlen
# take a value like toupper, but SB5070's operand set -- written before that
# decision -- still classed it with the name-taking `len`/`join`, so `strlen @a`
# was the one silent join left in the language: it quietly meant
# `strlen "$(join a ' ')"`. DG: "strlen should only work on scalars, not
# collections."
run_sash 'array a=(12 345)
echo $(strlen @a)'
[[ $rc != 0 && -z $out && $err == *SB5070* ]]; chk 75.88

# 75.89 an array splatted into a user function still arrives as args
run_sash 'fn f {
: @args
echo $(len args)
}
array a=(1 2)
f @a'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk 75.89

# --- SB5070 claims only COLLECTION splats ---
# Its message asserts "`@x` is an array splat" and advises `join x SEP`. For a
# scalar or an unintroduced subject both halves are false, and the remedy it
# names is itself rejected -- so it must defer to SB5079/SB5031.

# 75.90 a scalar subject in a scalar-operand intrinsic is SB5079, not SB5070
run_sash 'str s=hi
echo $(toupper @s)'
[[ $rc != 0 && -z $out && $err == *SB5079* && $err != *SB5070* ]]; chk 75.90

# 75.91 an unintroduced subject is SB5031, not SB5070
run_sash 'echo $(toupper @nosuch)'
[[ $rc != 0 && -z $out && $err == *SB5031* && $err != *SB5070* ]]; chk 75.91

# 75.92 a DD#100 collection slot has no declaration in scope but IS a
# collection, so SB5070 still claims it
run_sash 'fn f {
echo $(toupper @A)
}
array a=(x)
f =a'
[[ $rc != 0 && -z $out && $err == *SB5070* ]]; chk 75.92

# 75.93 the form SB5070 advises actually works
run_sash 'array a=(12 345)
echo $(strlen "$(join a ,)")'
[[ $rc = 0 && $out = "6" && -z $err ]]; chk 75.93

# 75.94 `len args` counts the arguments of the enclosing function. `args` is
# the function-local argv collection, so DD#75's collection-only `len` takes
# it directly. Reached `check_len_operand`'s `name == "args"` arm, which no
# spec had exercised.
run_sash 'fn f {
: @args
echo $(len args)
}
f a b c'
[[ $rc = 0 && $out = "3" && -z $err ]]; chk 75.94

# 75.95 it agrees with $# for the same frame
run_sash 'fn f {
: @args
echo "$(len args) $#"
}
f x y'
[[ $rc = 0 && $out = "2 2" && -z $err ]]; chk 75.95

# 75.96 zero arguments is 0, not an error
run_sash 'fn f {
echo "$(len args) $#"
}
f'
[[ $rc = 0 && $out = "0 0" && -z $err ]]; chk 75.96

# 75.97 an empty string is an argument: counted, not skipped
run_sash 'fn f {
: @args
echo "$(len args) $#"
}
f "" ""'
[[ $rc = 0 && $out = "2 2" && -z $err ]]; chk 75.97

# 75.98 `args` is per-frame -- a nested call does not disturb the caller's
run_sash 'fn g {
: @args
echo "g=$(len args)"
}
fn f {
: @args
g x y z
echo "f=$(len args)"
}
f a b'
[[ $rc = 0 && $out = "g=3
f=2" && -z $err ]]; chk 75.98

# 75.100 `args` is indexed, not associative, so `keys` refuses it the same
# way it refuses any array
run_sash 'fn f {
for k in $(keys args) {
echo "$k"
}
}
f m n'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 75.100

# ---------------------------------------------------------------------------
# Substring operands must be decimal integers (SB4030).
#
# A substring is lowered as an integer CONSTANT, and the parse used to fall
# back to 0 on anything it could not read. So `${s:1x}` silently yielded the
# whole string, and `${s:1+1}` — which bash evaluates, printing `cdef` —
# yielded `abcdef` here. Unparseable now means reject.
# ---------------------------------------------------------------------------

# sub.1 the working forms are untouched: plain offset
run_sash 'str s="abcdef"
echo ${s:1}'
[[ $rc = 0 && $out = "bcdef" && -z $err ]]; chk sub.1

# sub.2 offset and length
run_sash 'str s="abcdef"
echo ${s:2:3}'
[[ $rc = 0 && $out = "cde" && -z $err ]]; chk sub.2

# sub.3 the parenthesised form is the ONLY way to write a negative offset —
# bare `${s:-2}` is the `:-` disposition, not a substring.
run_sash 'str s="abcdef"
echo ${s:(-2)}'
[[ $rc = 0 && $out = "ef" && -z $err ]]; chk sub.3

# sub.4 an omitted length is 0, as in bash
run_sash 'str s="abcdef"
echo "[${s:1:}]"'
[[ $rc = 0 && $out = "[]" && -z $err ]]; chk sub.4

# sub.5 trailing garbage in the offset
run_sash 'str s="abcdef"
echo ${s:1x}'
[[ $rc != 0 && $err == *SB4030* ]]; chk sub.5

# sub.6 trailing garbage in the length
run_sash 'str s="abcdef"
echo ${s:0:3x}'
[[ $rc != 0 && $err == *SB4030* ]]; chk sub.6

# sub.7 a space is not a separator — bash calls this a syntax error
run_sash 'str s="abcdef"
echo ${s:1 2}'
[[ $rc != 0 && $err == *SB4030* ]]; chk sub.7

# sub.8 no arithmetic is evaluated. bash prints `cdef`; this used to print the
# whole string, which is the divergence that made the silent 0 a bug.
run_sash 'str s="abcdef"
echo ${s:1+1}'
[[ $rc != 0 && $err == *SB4030* ]]; chk sub.8

# sub.9 ... including the parenthesised spelling, which reaches the same check
# because `try_substring_op` strips the parens
run_sash 'str s="abcdef"
echo ${s:(1+1)}'
[[ $rc != 0 && $err == *SB4030* ]]; chk sub.9

# sub.10 a third part was silently ignored
run_sash 'str s="abcdef"
echo ${s:1:2:3}'
[[ $rc != 0 && $err == *SB4030* ]]; chk sub.10

# ── decl.*  declaration initializer shape (SB4064) and `+=` (SB4065) ──
# Before 2026-09-22 NOTHING checked that a declaration's initializer matched
# the kind being declared, in either direction. A scalar into a collection
# slot compiled clean and SEGFAULTED on the first read (`len a` returned
# uninitialised memory rather than faulting); an array literal into a scalar
# silently produced "". bash reads `declare -a a=x` as `a[0]=x`, but sash has
# no implicit one-element promotion — DG chose to reject the mismatch.

# decl.1-decl.4 scalar value into a collection slot — every one segfaulted
run_sash 'array a=x; echo "${a[0]}"'
[[ $rc != 0 && -z $out && $err == *SB4064* ]]; chk decl.1
run_sash 'array a=""; echo "${a[0]}"'
[[ $rc != 0 && -z $out && $err == *SB4064* ]]; chk decl.2
run_sash 'map m=v; echo "${m[k]}"'
[[ $rc != 0 && -z $out && $err == *SB4064* ]]; chk decl.3
run_sash 'array a=$(echo x); echo "${a[0]}"'
[[ $rc != 0 && -z $out && $err == *SB4064* ]]; chk decl.4

# decl.5-decl.7 the mirror: a `( … )` literal into a scalar slot
run_sash 'str s=(x); echo "$s"'
[[ $rc != 0 && -z $out && $err == *SB4064* ]]; chk decl.5
run_sash 'int n=(1); echo "$n"'
[[ $rc != 0 && -z $out && $err == *SB4064* ]]; chk decl.6
run_sash 'const c=(1); echo "$c"'
[[ $rc != 0 && -z $out && $err == *SB4064* ]]; chk decl.7

# decl.8-decl.11 `+=` in a declaration: it creates, so there is nothing to
# append to. `array a+=(x)` was the worst of these -- the `=` branch checked
# for `(` and the `+=` branch did not, so `(x)` parsed as the STRING "(x)".
run_sash 'array a+=(x); echo "${a[0]}"'
[[ $rc != 0 && -z $out && $err == *SB4065* ]]; chk decl.8
run_sash 'array a+=x; echo "${a[0]}"'
[[ $rc != 0 && -z $out && $err == *SB4065* ]]; chk decl.9
run_sash 'str s+="a"; echo "$s"'
[[ $rc != 0 && -z $out && $err == *SB4065* ]]; chk decl.10
run_sash 'int n+=1; echo "$n"'
[[ $rc != 0 && -z $out && $err == *SB4065* ]]; chk decl.11

# decl.12-decl.17 the valid forms still work. decl.12/decl.15 matter most:
# a BARE declaration used to be an in-band `Literal("")`, indistinguishable
# from the explicit `array a=""` of decl.2 -- it is now `value: None`.
run_sash 'array a; a[0]=z; echo "${a[0]}"'
[[ $rc = 0 && $out = "z" && -z $err ]]; chk decl.12
run_sash 'array a=(); echo "$(len a)"'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk decl.13
run_sash 'map m=(); echo "$(len m)"'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk decl.14
run_sash 'map m; m[k]=v; echo "${m[k]}"'
[[ $rc = 0 && $out = "v" && -z $err ]]; chk decl.15
run_sash 'int n; echo "$n"'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk decl.16
run_sash 'str s; echo "[$s]"'
[[ $rc = 0 && $out = "[]" && -z $err ]]; chk decl.17

# decl.18-decl.20 append is still fine as a STATEMENT, once declared
run_sash 'str s="a"; s+="b"; echo "$s"'
[[ $rc = 0 && $out = "ab" && -z $err ]]; chk decl.18
run_sash 'array a=(x); a+=(y); echo "${a[1]}"'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk decl.19
run_sash 'int n=1; n+=2; echo "$n"'
[[ $rc = 0 && $out = "3" && -z $err ]]; chk decl.20

# ── SB4064 must not shadow SB5025 (DD#117 follow-up) ─────────────────
# `array a=$(argv …)` is a scalar into a collection, so DD#117's SB4064
# matched it — and SB4064 runs in an earlier phase that exits before
# SB5025's, which made the better diagnostic unreachable. SB4064's own
# advice ("write `a=(…)`") is wrong here: `array a=($(argv 1))` is
# rejected in turn. SB4064 defers to SB5025 for this one shape.

# decl.21 an argv splat in an array declaration keeps SB5025's advice
run_sash 'array a=$(argv 1); echo "${a[0]}"' x y z
[[ $rc != 0 && -z $out && $err == *SB5025* ]]; chk decl.21

# decl.22 likewise for a map declaration
run_sash 'map m=$(argv 1); echo "${m[k]}"' x y z
[[ $rc != 0 && -z $out && $err == *SB5025* ]]; chk decl.22

# decl.23 control: a NON-argv command substitution still gets SB4064
run_sash 'array a=$(echo x); echo "${a[0]}"' x y z
[[ $rc != 0 && -z $out && $err == *SB4064* ]]; chk decl.23

# decl.24-decl.28 the int→string mismatch on a DECLARATION.
# hint.4/hint.5 cover the same mismatch on an *assignment* (`s=$v`), which is a
# different renderer: an assignment goes through check_assign_type_match, which
# builds the message from type_name()/type_name(), while a declaration goes
# through check_scalar_assignment's hard-coded String←Int arm. The two produce
# byte-identical text, so hint.4 passing said nothing about this arm — it was
# reached by no spec at all until these.

# decl.24 declaring a str from an int variable
run_sash 'int n = 5; str s = $n; echo "$s"'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'quote it'* ]]; chk decl.24

# decl.25 …from an arithmetic expansion
run_sash 'str s = $(( 1 + 2 )); echo "$s"'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk decl.25

# decl.26 …and from a capture of an `-> int` function
run_sash 'fn f -> int { return 7 }; str s = $(f); echo "$s"'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk decl.26

# decl.27 control: a bare numeric literal is NOT an int value — it still declares
run_sash 'str s = 5; echo "$s"'
[[ $rc = 0 && $out = 5 && -z $err ]]; chk decl.27

# decl.28 control: the form the hint recommends compiles and runs
run_sash 'int n = 5; str s = "$n"; echo "$s"'
[[ $rc = 0 && $out = 5 && -z $err ]]; chk decl.28

# 75.108 a splat of an array declared in the SAME `$( )` body. `check_scalar_splat`
# built its `introduced` set from a program-wide `program_scopes` union, which
# walks `child_bodies()` -- total over StmtKind, so a body hanging off an
# EXPRESSION can never appear. `a` is declared one statement earlier and was
# reported "introduced nowhere".
run_sash '/bin/echo $(array a=( ); array b=( @{a:-z} ); echo $b[0])'
[[ $rc = 0 && $out = "z" && -z $err ]]; chk 75.108

# 75.109 the same flat union in the other direction, and this one RAN: a
# function-local array splatted at top level printed its elements. DD#86 makes
# a name first assigned in a function body local, and the `$y` / `$a[0]`
# spellings of that read both reject -- only the splat leaked.
run_sash 'fn f { array a=(1 2); /bin/echo @{a:-z}; }
f
/bin/echo @{a:-z}'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk 75.109

# 75.110 DD#86 rule 2 for a `$( )` body: a name first introduced inside is
# local to it, so splatting it outside is SB5031. The fix must not widen the
# scope, only place the check in it.
run_sash '/bin/echo $(array a=(1 2); echo $a[0])
/bin/echo @{a:-z}'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk 75.110

# 75.111 a splat inside the function that declares the array still traverses --
# guards the scope-aware check against rejecting the in-scope case.
run_sash 'fn f { array a=(1 2); /bin/echo @{a:-z}; }
f'
[[ $rc = 0 && $out = "1 2" && -z $err ]]; chk 75.111

# --- SB5070 must be keyed on the COMMAND, not on `$( )` syntax ---
# `match` is legal in two positions: a `$( )` value and a DD#70 must-consume
# boolean statement. The operand scan ran only over the first, so a splat in
# statement position reached `lower_value` as a scalar and ICEd the compiler
# ("ArraySplat (@a) must be expanded by the argv builder"). `cd` is
# statement-only and so was never scanned at all.

# 75.112 a splat in the PATTERN operand of a statement-position `match`
run_sash 'array a=(b)
str s=abc
if match "$s" @a; then echo y; else echo n; fi'
[[ $rc != 0 && -z $out && $err == *SB5070* ]]; chk 75.112

# 75.113 ... and in the SUBJECT operand
run_sash 'array a=(b)
if match @a xyz; then echo y; else echo n; fi'
[[ $rc != 0 && -z $out && $err == *SB5070* ]]; chk 75.113

# 75.114 `cd` takes a scalar path; a splat there ICEd
run_sash 'array a=(/tmp)
cd @a
echo ok'
[[ $rc != 0 && -z $out && $err == *SB5070* ]]; chk 75.114

# 75.115 the statement scan must reach nested bodies and if-conditions --
# `child_bodies()` is what carries it into the function and the condition
run_sash 'array a=(b)
fn f {
str s=abc
if match "$s" @a; then echo y; fi
}
f'
[[ $rc != 0 && -z $out && $err == *SB5070* ]]; chk 75.115

# 75.116 the `$( )` half must still report EXACTLY once -- the statement walk
# does not descend into command-substitution bodies, so nothing is doubled
run_sash 'array a=(b)
str s=abc
str r=$(match "$s" @a)
echo "$r"'
[[ $rc != 0 && -z $out && $(grep -c SB5070 <<< "$err") = 1 ]]; chk 75.116

# 75.117-75.127 the bulk-fill TARGET TYPE. A fill writes the DD#33 raw-8-byte
# i64 keys 0, 1, 2 …; a map is string-keyed, so filling one wrote a key of
# eight NUL bytes — a DD#87 breach reachable from ordinary safe code, which
# surfaced far downstream as `argument contains a NUL byte` at exec. DD#99
# settles the rule ("maps are built by assignment, not by `read`"); nothing
# enforced it, because `register_collection_target` only guarded against a
# SCALAR target and a map is in `collections` too.
#
# All six spellings reach the one check, so the cohort is really pinning the
# DISPATCH: which commands fill, and at which arity.
# 75.117 `read ARRAY` — the stdin form, target is the only arg
run_sash 'map m
read m
echo "k=$(len m)"'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 75.117
# 75.118 `read SRC ARRAY` — the split form, target is the SECOND arg
run_sash 'map m
str ln="a b"
read $ln m
echo "k=$(len m)"'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 75.118
# 75.119 `argv ARRAY` (DD#60)
run_sash 'map m
argv m
echo "k=$(len m)"'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 75.119
# 75.120 `argv N ARRAY` — the two-arg spelling, target last
run_sash 'map m
argv 1 m
echo "k=$(len m)"'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 75.120
# 75.121 `matchall S P ARRAY` (DD#63) — only the 3-arg form fills
run_sash 'map m
str s="abc"
matchall "$s" "." m || :
echo "k=$(len m)"'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 75.121
# 75.122 `match S P ARRAY` — singular match fills too
run_sash 'map m
str s="abc"
match "$s" "." m || :
echo "k=$(len m)"'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 75.122
# 75.123 POSITIVE control — a declared ARRAY target is the whole point of the
# form and must not be caught by the new check.
run_sash 'array a
read a
echo "n=$(len a)"'
[[ $rc = 0 && $out = "n=0" && -z $err ]]; chk 75.123
# 75.124 POSITIVE control — an UNDECLARED target still auto-declares to an
# array (DG 2026-07-08), so the check must key on the map declaration, not on
# the absence of one.
run_sash 'read u
echo "n=$(len u)"'
[[ $rc = 0 && $out = "n=0" && -z $err ]]; chk 75.124
# 75.125 the mirror: `readline` fills a SCALAR, and given an array it compiled
# and silently discarded the line, leaving len=0.
run_sash 'array a
readline a
echo "k=$(len a)"'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 75.125
# 75.126 …and given a map.
run_sash 'map m
readline m
echo "k=$(len m)"'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 75.126
# 75.127 POSITIVE control — `readline` into a DECLARED scalar, the shape the
# new check must not catch. Fed by a clean pipe so the value is clean (DD#20)
# and the assertion is about the type check, not about taint.
run_sash 'str s
echo hi | readline s
echo "[$s]"'
[[ $rc = 0 && $out = "[hi]" && -z $err ]]; chk 75.127

# 75.128-75.131 `keys` on a non-map used to be answered by a TAINT rule before
# the type rule could speak. `check_collection_reader_taint` had a `keys` arm
# that said "keys of tainted map 'a'" — but DD#93 closed every route to a
# tainted map at construction, so the only collection that could reach it was a
# tainted ARRAY. It therefore called an indexed array a map, preempted SB4030,
# and advised an `attest k v in a` that is itself rejected. It also hid the
# SB4030 above, since the taint pass runs before type_check. The arm is gone;
# the `join` one stays, because arrays genuinely can be tainted (taint.sh
# 60.37-60.41 pin that half).
# 75.128 a TAINTED indexed array — the only shape the deleted arm could reach.
# The useful complaint is the type error, and SB5020 must not appear.
run_sash 'argv 1 a
for k in $(keys a) { echo "$k" }' x y
[[ $rc != 0 && -z $out && $err == *SB4030* && $err != *SB5020* ]]; chk 75.128
# 75.129 a CLEAN indexed array — same answer, which is the point: `keys` on an
# array is a type error whatever its taint, so taint was never the right axis.
run_sash 'array a=(x)
for k in $(keys a) { echo "$k" }'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err != *SB5020* ]]; chk 75.129
# 75.130 the legitimate use is untouched: a clean map still iterates.
run_sash 'map m=([k1]=v1)
for k in $(keys m) { echo "$k" }'
[[ $rc = 0 && $out = "k1" && -z $err ]]; chk 75.130
# 75.131 control for the surviving arm in the same file, so deleting both at
# once cannot pass: `join` on a tainted array is still SB5020, not a type error.
run_sash 'argv 1 a
str j=$(join a)
echo "$j"' x y
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 75.131

set -e
tally
