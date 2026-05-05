#!/bin/bash
# spec/types.sh — type system: strings, ints, arrays, maps, qualifiers, dtoi/itod
set -eu
. common.sh
setup types
plan=29
echo "1..$plan" >&3

set +e

# 12.1 string length
run_sash 'x="hello"; echo ${#x}'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk 12.1

# 12.2 null-terminated for C interop
run_sash 'x="hello"; /usr/bin/printf "%s\n" "$x"'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 12.2

# 12.3 O(1) length — proves ${#x} works without scanning
run_sash 'x="hello world test"; echo ${#x}'
[[ $rc = 0 && $out = "17" && -z $err ]]; chk 12.3

# 16.1 int stores machine-width
run_sash 'int x=9223372036854775807; echo $x'
[[ $rc = 0 && $out = "9223372036854775807" && -z $err ]]; chk 16.1

# 16.2 fn -> int return type
run_sash 'fn add -> int { return $(( $1 + $2 )); }
echo $(add 3 4)'
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

# 41.1 array declaration
run_sash 'array arr=(a b c); echo ${arr[0]}'
[[ $rc = 0 && $out = "a" && -z $err ]]; chk 41.1

# 41.2 map declaration
run_sash 'map m=([k]=v); echo ${m[k]}'
[[ $rc = 0 && $out = "v" && -z $err ]]; chk 41.2

set -e
tally

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

# --- DD#33 array per-element ops ---

# 33.5 ${arr[@]/pat/rep} substitution per element
run_sash 'array a=(foo bar baz); echo ${a[@]/ba/BA}'
[[ $rc = 0 && $out = "foo BAr BAz" && -z $err ]]; chk 33.5

# 33.6 ${arr[@]#prefix} strip prefix per element
run_sash 'array a=(pre_one pre_two three); echo ${a[@]#pre_}'
[[ $rc = 0 && $out = "one two three" && -z $err ]]; chk 33.6

# 33.7 ${arr[@]%%suffix} strip suffix per element
run_sash 'array a=(file.txt doc.txt note.md); echo ${a[@]%%.txt}'
[[ $rc = 0 && $out = "file doc note.md" && -z $err ]]; chk 33.7

