#!/bin/bash
# spec/types.sh — type system: strings, ints, arrays, maps, qualifiers, dtoi/itod
set -eu
. common.sh
setup types
plan=64
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

# 30.2 $(( )) is explicit string-to-int
run_sash 'x="42"; int y=$(( x )); echo $y'
[[ $rc = 0 && $out = "42" && -z $err ]]; chk 30.2

# 30.3 x=5 is a string
run_sash 'x=5; echo ${#x}'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk 30.3

# 30.4 int x=5 required for integer
run_sash 'int x=5; echo $(( x + 1 ))'
[[ $rc = 0 && $out = "6" && -z $err ]]; chk 30.4

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

# 58.6 $(f) for -> int function is type error
run_sash 'fn f -> int { return 42; }
x=$(f)'
[[ $rc != 0 && -z $out && -n $err ]]; chk 58.6

# 58.7 function/variable name collision is error
run_sash 'fn x { echo hi; }
x=hello'
[[ $rc != 0 && -z $out && -n $err ]]; chk 58.7

# 58.8 $(( (f) + 1 )) sexpr in expression
run_sash 'fn f -> int { return 10; }
int x=$(( (f) + 1 ))
echo "$x"'
[[ $rc = 0 && $out = "11" && -z $err ]]; chk 58.8

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

# 30.11 unset arr clears collection
run_sash 'array a=(one two three); unset a; echo ${#a[@]}'
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

# 33.16 the reject also fires for a mixed splat (`echo x @a y`)
run_sash 'array a=(one two); echo x @a y'
[[ $rc != 0 && -z $out && $err == *SB5030* ]]; chk 33.16

set -e
tally

