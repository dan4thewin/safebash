#!/bin/bash
# DD131 trial: source-owned immutable instances and fresh construction.
set -eu
. common.sh
setup scratch
plan=109
echo "1..$plan" >&3

set +e

run_sash '_x=one; _x=two; echo "$_x"'
[[ $rc != 0 && $err == *SB5103* && $err == *'`_x`'* ]]; chk scratch.1

run_sash 'int _n=1; (( _n += 1 )); echo "$_n"'
[[ $rc != 0 && $err == *SB5103* && $err == *'`_n`'* ]]; chk scratch.2

run_sash 'array _a=(x); _a[0]=y; echo "${_a[0]}"'
[[ $rc != 0 && $err == *SB5103* && $err == *'`_a`'* ]]; chk scratch.3

run_sash 'array _a=(x); _a+=(y); echo "$(len _a)"'
[[ $rc != 0 && $err == *SB5103* && $err == *'`_a`'* ]]; chk scratch.4

run_sash '_x=one; unset _x; echo "[$_x]"'
[[ $rc != 0 && $err == *SB5103* && $err == *'`_x`'* ]]; chk scratch.5

run_sash 'fn clear { unset A[0] }; array _a=(x); clear =_a; echo "$(len _a)"'
[[ $rc != 0 && $err == *SB5103* && $err == *'`_a`'* ]]; chk scratch.6

run_sash 'fn change { A[0]=y }; array _a=(x); change =_a; echo "${_a[0]}"'
[[ $rc != 0 && $err == *SB5103* && $err == *'`_a`'* ]]; chk scratch.7

SASH_CFLAGS=--no-taint run_sash 'fn change { A[0]=y }; fn relay { change =A }; array _a=(x); relay =_a; echo "${_a[0]}"'
[[ $rc != 0 && $err == *SB5103* && $err == *'`_a`'* ]]; chk scratch.8

run_sash 'fn show { echo "${A[0]}" }; array _a=(ok); show =_a'
[[ $rc = 0 && $out = ok && -z $err && -z $rerr ]]; chk scratch.9

run_sash 'fn f { str x=before; x=after; echo "$x" }; f'
[[ $rc = 0 && $out = after && -z $err && -z $rerr ]]; chk scratch.10

run_sash '_x="$_x"; echo "$_x"'
[[ $rc != 0 && $err == *SB5104* && $err == *'`_x`'* ]]; chk scratch.11

SASH_CFLAGS=--no-taint run_sash '_x="${_x:!}"; echo "$_x"'
[[ $rc != 0 && $err == *SB5104* && $err == *'`_x`'* ]]; chk scratch.12

run_sash 'fn get { echo "$_x" }; _x=$(get); echo "$_x"'
[[ $rc != 0 && $err == *SB5104* && $err == *'`_x`'* ]]; chk scratch.13

run_sash 'fn get { echo "$_x" }; fn relay { get }; _x=$(relay); echo "$_x"'
[[ $rc != 0 && $err == *SB5104* && $err == *'`_x`'* ]]; chk scratch.14

run_sash 'fn get { echo "${A[0]}" }; array _a=($(get =_a)); echo "${_a[0]}"'
[[ $rc != 0 && $err == *SB5104* && $err == *'`_a`'* ]]; chk scratch.15

# Passing identity to an unused slot does not read its value.
run_sash 'fn get -> int { return 7 }; array _a=($(get =_a)); echo "${_a[0]}"'
[[ $rc = 0 && $out = 7 && -z $err && -z $rerr ]]; chk scratch.16

run_sash 'fn get -> int { A[0]=changed; return 7 }; array _a=($(get =_a)); echo "${_a[0]}"'
[[ $rc != 0 && $err == *SB5103* && $err == *'`_a`'* ]]; chk scratch.17

run_sash 'str switch=yes; fn get { if [[ $switch == yes ]] { keys _m k; echo "${k[0]}" } }; map _m=([key]=$(get)); echo "${_m[key]}"'
[[ $rc != 0 && $err == *SB5104* && $err == *'`_m`'* ]]; chk scratch.18

printf first > first
run_sash 'if readline _x <"${_x:-first}" { echo "${_x:!}" }'
[[ $rc != 0 && $err == *SB5104* && $err == *'`_x`'* ]]; chk scratch.19

run_sash '/bin/echo "${_line:!}" | readline _line; echo "${_line:!}"'
[[ $rc != 0 && $err == *SB5104* && $err == *'`_line`'* ]]; chk scratch.20

run_sash 'fn produce { echo "${_line:!}" }; produce | readline _line; echo "${_line:!}"'
[[ $rc != 0 && $err == *SB5104* && $err == *'`_line`'* ]]; chk scratch.21

run_sash '/bin/echo "${_line:!}" | while readline _line { echo "${_line:!}" }'
[[ $rc != 0 && $err == *SB5104* && $err == *'`_line`'* ]]; chk scratch.22

run_sash 'for item in one two { _x=$item; echo "$_x" }; echo "last:$_x"'
[[ $rc = 0 && $out = $'one\ntwo\nlast:two' && -z $err && -z $rerr ]]; chk scratch.23

run_sash 'for item in one two { const _x=$item; echo "$_x" }'
[[ $rc = 0 && $out = $'one\ntwo' && -z $err && -z $rerr ]]; chk scratch.24

run_sash 'for item in one two { array _a=($item); echo "${_a[0]}" }; echo "last:${_a[0]}"'
[[ $rc = 0 && $out = $'one\ntwo\nlast:two' && -z $err && -z $rerr ]]; chk scratch.25

run_sash 'for item in one two { if [[ $item == one ]] { _x=$item }; echo "[$_x]" }; echo "last:$_x"'
[[ $rc = 0 && $out = $'[one]\n[one]\nlast:one' && -z $err && -z $rerr ]]; chk scratch.26

run_sash 'for phase in yes no { array items=(); if [[ $phase == yes ]] { items=(one) }; for _item in @items { echo "body:$_item" }; echo "after:$_item" }'
[[ $rc = 0 && $out = $'body:one\nafter:one\nafter:one' && -z $err && -z $rerr ]]; chk scratch.27

run_sash 'array a=(old); array _a=(@{a:-}); a[0]=new; echo "${_a[0]} ${a[0]}"'
[[ $rc = 0 && $out = 'old new' && -z $err && -z $rerr ]]; chk scratch.28

run_sash 'for value in 3 4 { int array _a=($(( ${value:?number} ))); int map _m=([k]=$(( ${value:?number} ))); echo "${_a[0]} ${_m[k]}" }'
[[ $rc = 0 && $out = $'3 3\n4 4' && -z $err && -z $rerr ]]; chk scratch.29

run_sash 'fn f { _value=$1; echo "$_value"; if [[ $1 == one ]] { return f two } }; f one; f three'
[[ $rc = 0 && $out = $'one\ntwo\nthree' && -z $err && -z $rerr ]]; chk scratch.30

printf 'one\ntwo\n' > input
run_sash '/bin/cat input | while readline _line { attest _line [[ -n $_line ]]; echo "$_line" }; echo "last:[${_line:!}]"'
[[ $rc = 0 && $out = $'one\ntwo\nlast:[]' && -z $err && -z $rerr ]]; chk scratch.31

run_sash 'for _item in one two { _item=changed; echo "$_item" }'
[[ $rc != 0 && $err == *SB5103* && $err == *'`_item`'* ]]; chk scratch.32

run_sash 'for pass in first second { for _arg in $(argv 1) { echo "$pass:${_arg:!}" } }; echo "last:${_arg:!}"' one two
[[ $rc = 0 && $out = $'first:one\nfirst:two\nsecond:one\nsecond:two\nlast:two' && -z $err && -z $rerr ]]; chk scratch.33

run_sash 'for src in a b { read "$src" _a; echo "${_a[0]}" }'
[[ $rc = 0 && $out = $'a\nb' && -z $err && -z $rerr ]]; chk scratch.34

run_sash 'for item in one two { argv 1 _a; attest v in _a [[ -n $v ]]; echo "$item:${_a[0]}" }' argument
[[ $rc = 0 && $out = $'one:argument\ntwo:argument' && -z $err && -z $rerr ]]; chk scratch.35

run_sash 'map source=([k]=v); for _item in one two { keys source _keys; echo "${_keys[0]} $_item" }'
[[ $rc = 0 && $out = $'k one\nk two' && -z $err && -z $rerr ]]; chk scratch.36

run_sash 'for text in a z { if match "$text" a _m { echo "yes:$(len _m)" } else { echo "no:$(len _m)" } }'
[[ $rc = 0 && $out = $'yes:1\nno:0' && -z $err && -z $rerr ]]; chk scratch.37

run_sash 'for text in 7 bad { if attest text (( _n = text )) { echo "good:$_n" } else { echo "bad:$_n" } }'
[[ $rc = 0 && $out = $'good:7\nbad:0' && -z $err && -z $rerr ]]; chk scratch.38

run_sash 'int n=2; while (( n > 0 )) { (( n -= 1 )); int _x=$(( 7 / n )); echo "$_x" }; ERR { - echo "err:$_x" }'
[[ $rc = 2 && $out = $'7\nerr:0' && -z $err && $rerr == *'division by zero'* ]]; chk scratch.39

run_sash 'for text in $'"'"'k\x00v'"'"' $'"'"'k\x00new\x00dangling'"'"' { read "$text" pairs; map _m=@{pairs:?odd}; echo "$(len _m)" }; ERR { - echo "err:$(len _m)" }'
[[ $rc = 1 && $out = $'1\nerr:0' && -z $err && $rerr == *': odd' ]]; chk scratch.40

run_sash 'for path in first missing { _x=$(<$path); attest _x [[ -n $_x ]]; echo "$_x" }; ERR { - echo "err:[${_x:!}]" }'
[[ $rc = 1 && $out = $'first\nerr:[]' && -z $err && $rerr == *'missing: cannot read file'* ]]; chk scratch.41

run_sash 'for word in one two { if _s=$(/bin/sh -c '"'"'printf "%s" "$1"; exit 1'"'"' sh "$word") { echo unexpected } else { echo "[${_s:!}]" } }'
[[ $rc = 0 && $out = $'[one]\n[two]' && -z $err && -z $rerr ]]; chk scratch.42

run_sash 'for path in first missing { readline _x <"${path:?path}"; attest _x [[ -n $_x ]]; echo "$_x" }; ERR { - echo "err:[${_x:!}]" }'
[[ $rc = 1 && $out = $'first\nerr:[]' && -z $err && $rerr == *missing* ]]; chk scratch.43

# pipe2 never restarts; the second allocation is the second attempt's pipe.
run_sash_under 'for item in one two { /bin/echo "$item" | read _a; attest v in _a [[ -n $v ]]; echo "${_a[0]}" }; ERR { - echo "err:$(len _a)" }' \
    strace -f -qq -o pipes.trace -e trace=pipe2 -e inject=pipe2:error=EMFILE:when=2
[[ $rc = 1 && $out = $'one\n\nerr:0' && -z $err && $rerr == *'pipe: Too many open files'* ]] &&
    grep -Eq 'pipe2\(.*= -1 EMFILE .*INJECTED' pipes.trace
chk scratch.44

# These fail after several cells have been built, not at an initial reserve.
large=$(printf 'x\\x00%.0s' {1..2048})
SASH_CFLAGS=--heap-size=16K run_sash "read \$'$large' _a; echo \"\$(len _a)\"; ERR { - echo \"err:\$(len _a)\" }"
[[ $rc = 1 && $out = err:0 && -z $err && $rerr == *'out of memory'* ]]; chk scratch.45

large=$(printf 'x%.0s' {1..2048})
SASH_CFLAGS=--heap-size=128K run_sash "if matchall '$large' x _a { echo \"\$(len _a)\" }; ERR { - echo \"err:\$(len _a)\" }"
[[ $rc = 1 && $out = err:0 && -z $err && $rerr == *'out of memory'* ]]; chk scratch.46

pairs=$(printf 'k%s x ' {1..512})
SASH_CFLAGS=--heap-size=160K run_sash "array source=($pairs); echo ready; map _m=@{source:?odd}; echo \"\$(len _m)\"; ERR { if (( (len _m) == 0 )) { - echo empty } else { - echo partial } }"
[[ $rc = 1 && $out = $'ready\nempty' && -z $err && $rerr == *'out of memory'* ]]; chk scratch.47

run_sash 'for text in 1 2 { str _a=$text _b=$_a; echo "$_a $_b" }'
[[ $rc = 0 && $out = $'1 1\n2 2' && -z $err && -z $rerr ]]; chk scratch.48

run_sash 'int n=2; while (( n > 0 )) { (( n -= 1 )); int array _a=($(( 7 / n ))); echo "${_a[0]}" }; ERR { - echo "err:$(len _a)" }'
[[ $rc = 2 && $out = $'7\nerr:0' && -z $err && $rerr == *'division by zero'* ]]; chk scratch.49

run_sash 'int n=2; while (( n > 0 )) { (( n -= 1 )); int map _m=([k]=$(( 7 / n ))); echo "${_m[k]}" }; ERR { - echo "err:$(len _m)" }'
[[ $rc = 2 && $out = $'7\nerr:0' && -z $err && $rerr == *'division by zero'* ]]; chk scratch.50

run_sash '_x+=one; echo "$_x"'
[[ $rc != 0 && $err == *SB5103* && $err == *'`_x`'* ]]; chk scratch.51

# DD132: failed immutable candidates survive, but the read permission does not.
SCRATCH_INPUT=bad run_sash 'str _x=$(- getenv SCRATCH_INPUT); if attest _x [[ $_x == ok ]] { echo yes }; echo "${_x:!}"; echo done'
[[ $rc = 0 && $out = $'bad\ndone' && -z $err && -z $rerr ]]; chk scratchperm.1

run_sash 'str _x=ok; if attest _x p? { echo yes } else { fail 1 }; echo $_x'
[[ $rc != 0 && $err == *SB5107* && $err == *"'_x'"* ]]; chk scratchperm.2

run_sash 'str _x=bad; if attest _x [[ $_x == ok ]] { echo yes } else { echo $_x }'
[[ $rc != 0 && $err == *SB5107* && $err == *"'_x'"* ]]; chk scratchperm.3

run_sash 'str _x=bad; if attest _x [[ $_x == ok ]] { echo yes } else { echo ${_x:!} }; echo ${_x:!}; str _y=ok; if ! attest _y p? { fail 1 } else { echo $_y }'
[[ $rc = 0 && $out = $'bad\nbad\nok' && -z $err && -z $rerr ]]; chk scratchperm.4

run_sash 'str _x=ok; attest _x p? && echo $_x; echo done'
[[ $rc = 0 && $out = $'ok\ndone' && -z $err && -z $rerr ]]; chk scratchperm.5

SCRATCH_INPUT=bad run_sash 'str _x=$(- getenv SCRATCH_INPUT); attest _x [[ $_x == ok ]] && echo wrong; echo ${_x:!}'
[[ $rc = 0 && $out = bad && -z $err && -z $rerr ]]; chk scratchperm.6

run_sash 'str _x=bad; attest _x [[ $_x == ok ]] || echo $_x'
[[ $rc != 0 && $err == *SB5107* && $err == *"'_x'"* ]]; chk scratchperm.7

run_sash 'str _x=bad; if attest _x [[ $_x == ok ]] || /bin/false { echo yes } else { echo no }'
[[ $rc = 0 && $out = no && -z $err && -z $rerr ]]; chk scratchperm.8

run_sash 'str _x=ok; if attest _x p? && /bin/false { echo yes } else { echo no }'
[[ $rc = 0 && $out = no && -z $err && -z $rerr ]]; chk scratchperm.9

run_sash 'str _x=bad; ! attest _x [[ $_x == ok ]] && echo ${_x:!}; echo done'
[[ $rc = 0 && $out = $'bad\ndone' && -z $err && -z $rerr ]]; chk scratchperm.10

run_sash 'str _x=ok; ! attest _x p? || echo $_x; echo done'
[[ $rc = 0 && $out = $'ok\ndone' && -z $err && -z $rerr ]]; chk scratchperm.11

run_sash 'str _x=ok; if ! attest _x p? && /bin/true { echo wrong } else { echo false }; str _y=bad; if ! attest _y [[ $_y == ok ]] || /bin/false { echo true } else { echo wrong }'
[[ $rc = 0 && $out = $'false\ntrue' && -z $err && -z $rerr ]]; chk scratchperm.12

run_sash 'str _x=bad; attest _x [[ $_x == ok ]] || /bin/false; echo leaked'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed'* ]]; chk scratchperm.13

run_sash 'str _x=ok; if attest _x p? && [[ $_x == ok ]] { echo $_x } else { echo no }'
[[ $rc != 0 && $err == *SB5107* && $err == *"'_x'"* ]]; chk scratchperm.14

# An attest elsewhere in a general condition is still implicitly checked.
run_sash 'str _x=bad; if :; attest _x [[ $_x == ok ]] { echo yes } else { echo no }'
[[ $rc = 1 && -z $out && -z $err && $rerr == *"attest: _x = 'bad' failed the gate"* ]]; chk scratchperm.15

run_sash 'str _x=bad; if /bin/true && attest _x [[ $_x == ok ]] { echo yes } else { echo no }'
[[ $rc = 1 && -z $out && -z $err && $rerr == *"attest: _x = 'bad' failed the gate"* ]]; chk scratchperm.16

SASH_CFLAGS=--no-taint-cst run_sash 'str _x=bad; if attest _x [[ $_x == ok ]] { echo yes } else { echo $_x }'
[[ $rc != 0 && $err == *SB5107* ]]; chk scratchperm.17

SASH_CFLAGS=--no-taint run_sash 'str _x=bad; if attest _x [[ $_x == ok ]] { echo yes } else { echo $_x }; echo $_x'
[[ $rc = 0 && $out = $'bad\nbad' && -z $err && -z $rerr ]]; chk scratchperm.18

SCRATCH_INPUT=bad run_sash 'str x=$(- getenv SCRATCH_INPUT); if attest x p? { echo "$x" }; echo "${x:!}"'
[[ $rc != 0 && $err == *SB5089* ]]; chk scratchperm.19

SCRATCH_INPUT=bad run_sash 'const x=$(- getenv SCRATCH_INPUT); if attest x p? { echo "$x" } else { echo "${x:!}" }'
[[ $rc != 0 && $err == *SB5089* ]]; chk scratchperm.20

SCRATCH_INPUT=bad run_sash 'str x=$(- getenv SCRATCH_INPUT); if attest x (( _n = x )) { echo "$_n" }; echo "${x:!}"'
[[ $rc != 0 && $err == *SB5089* ]]; chk scratchperm.21

run_sash 'argv 1 _a; if attest v in _a g? { echo wrong } else { /bin/echo @{_a:!~} }; echo done' one 'two words'
[[ $rc = 0 && $out = $'one two words\ndone' && -z $err && -z $rerr ]]; chk scratchperm.22

run_sash 'map _m=([key]=bad); if attest k v in _m [[ $k == key && $v == ok ]] { echo yes } else { keys _m names; echo "$(join names)" }'
[[ $rc != 0 && $err == *SB5107* && $err == *"'_m'"* ]]; chk scratchperm.23

run_sash 'array _a=(ok); if attest v in _a p? { : } else { fail 1 }; array copy=(@_a); echo "${copy[0]}"'
[[ $rc != 0 && $err == *SB5107* && $err == *"'_a'"* ]]; chk scratchperm.24

run_sash 'array _a=(ok); if attest v in _a p? { : } else { fail 1 }; array copy=(@{_a:!}); echo "${copy[0]}"'
[[ $rc = 0 && $out = ok && -z $err && -z $rerr ]]; chk scratchperm.25

run_sash 'array _pairs=(key val); if attest v in _pairs p? { : }; map copy=@{_pairs:~}; echo "${copy[key]}"'
[[ $rc != 0 && $err == *SB5107* && $err == *"'_pairs'"* ]]; chk scratchperm.26

run_sash 'array _pairs=(key val); if attest v in _pairs p? { : }; map copy=@{_pairs:!~}; echo "${copy[key]}"'
[[ $rc = 0 && $out = val && -z $err && -z $rerr ]]; chk scratchperm.27

run_sash 'str _x=ok; if attest _x p? { : }; echo "$(strlen $_x)"'
[[ $rc != 0 && $err == *SB5107* && $err == *"'_x'"* ]]; chk scratchperm.28

run_sash 'str _x=ok; if attest _x p? { : }; echo "$(strlen ${_x:!})"'
[[ $rc = 0 && $out = 2 && -z $err && -z $rerr ]]; chk scratchperm.29

run_sash 'int _i=0; if attest _i [[ "$_i" == 0 ]] { : }; array a=(value); echo "${a[$_i]:!}"'
[[ $rc != 0 && $err == *SB5107* && $err == *"'_i'"* ]]; chk scratchperm.30

run_sash 'int _i=0; if attest _i [[ "$_i" == 0 ]] { : }; array a=(value); echo "${a[${_i:!}]:!}"'
[[ $rc = 0 && $out = value && -z $err && -z $rerr ]]; chk scratchperm.31

run_sash 'fn show { echo $_x }; str _x=ok; if attest _x p? { show }; show'
[[ $rc != 0 && $err == *SB5107* && $err == *"'_x'"* ]]; chk scratchperm.32

run_sash 'fn show { echo "${A[0]}" }; array _a=(ok); if attest v in _a p? { show =_a }; show =_a'
[[ $rc != 0 && $err == *SB5107* && $err == *"'_a'"* ]]; chk scratchperm.33

run_sash 'fn unused { echo okay }; array _a=(ok); if attest v in _a p? { : }; unused =_a'
[[ $rc = 0 && $out = okay && -z $err && -z $rerr ]]; chk scratchperm.34

run_sash 'str _x=ok; if attest _x p? { : }; text=$(echo $_x); echo "$text"'
[[ $rc != 0 && $err == *SB5107* && $err == *"'_x'"* ]]; chk scratchperm.35

run_sash 'str _x=ok; if attest _x p? { : }; echo $_x &; - wait'
[[ $rc != 0 && $err == *SB5107* && $err == *"'_x'"* ]]; chk scratchperm.36

run_sash 'str _x=ok; fn child { if attest _x p? { echo $_x } }; child &; - wait; echo $_x; EXIT { echo $_x }'
[[ $rc = 0 && $out = $'ok\nok\nok' && -z $err && -z $rerr ]]; chk scratchperm.37

run_sash 'for value in one two { _x=$value; echo "before:$_x"; if attest _x p? { echo "inside:$_x" }; echo "after:${_x:!}" }'
[[ $rc = 0 && $out = $'before:one\ninside:one\nafter:one\nbefore:two\ninside:two\nafter:two' && -z $err && -z $rerr ]]; chk scratchperm.38

run_sash 'fn f { str _x=$1; echo $_x; if attest _x p? { : } }; f one; f two'
[[ $rc = 0 && $out = $'one\ntwo' && -z $err && -z $rerr ]]; chk scratchperm.39

run_sash 'for value in one two { str _x=$value; if attest _x p? { : } else { continue }; echo $_x }'
[[ $rc != 0 && $err == *SB5107* && $err == *"'_x'"* ]]; chk scratchperm.40

run_sash 'str _x=ok; if attest _x p? { : }; EXIT { echo $_x }'
[[ $rc != 0 && $err == *SB5107* && $err == *"'_x'"* ]]; chk scratchperm.41

run_sash 'str _x=ok; if attest _x p? { : }; ERR { - echo $_x }'
[[ $rc != 0 && $err == *SB5107* && $err == *"'_x'"* ]]; chk scratchperm.42

SCRATCH_INPUT=bad run_sash 'str _x=$(- getenv SCRATCH_INPUT); if attest _x [[ $_x == ok ]] { : }; fail 7; ERR { - echo "${_x:!}" }'
[[ $rc = 7 && $out = bad && -z $err && -z $rerr ]]; chk scratchperm.43

SCRATCH_INPUT=bad run_sash 'str _x=$(- getenv SCRATCH_INPUT); if attest _x [[ $_x == ok ]] { : }; exit 7; EXIT { echo "${_x:!}" }'
[[ $rc = 7 && $out = bad && -z $err && -z $rerr ]]; chk scratchperm.44

run_sash 'str _x=ok; exit 0; if attest _x p? { : }; EXIT { echo $_x }'
[[ $rc = 0 && $out = ok && -z $err && -z $rerr ]]; chk scratchperm.45

run_sash 'for phase in check leave { _x=$phase; if [[ $phase == leave ]] { exit 0 }; if attest _x p? { : } }; fail 1; EXIT { echo $_x }'
[[ $rc = 0 && $out = leave && -z $err && -z $rerr ]]; chk scratchperm.46

run_sash 'str _x=ok; EXIT { if attest _x p? { fail 7 } else { fail 8 } }; ERR { - echo $_x }'
[[ $rc != 0 && $err == *SB5107* && $err == *"'_x'"* ]]; chk scratchperm.47

# Only the subject's eligible reads are exempt inside an attest predicate.
run_sash 'str _x=ok; if attest _x p? { : }; str y=ok; attest y [[ $y == $_x ]]; echo "$y"'
[[ $rc != 0 && $err == *SB5107* && $err == *"'_x'"* ]]; chk scratchperm.48

SCRATCH_INPUT=bad run_sash 'str _x=$(- getenv SCRATCH_INPUT); attest _x [[ $_x == ok ]] && echo yes; attest _x p?; echo "${_x:!}"'
[[ $rc != 0 && $err == *SB5088* ]]; chk scratchperm.49

run_sash 'int _i=0; if attest _i [[ "$_i" == 0 ]] { : }; array paths=(/dev/null); fn send { echo data }; send >"${paths[$_i]:!?path}"'
[[ $rc != 0 && $err == *SB5107* && $err == *"'_i'"* ]]; chk scratchperm.50

run_sash 'int _i=0; if attest _i [[ "$_i" == 0 ]] { : }; array paths=(/dev/null); fn send { echo data }; send >"${paths[${_i:!}]:!?path}"; echo done'
[[ $rc = 0 && $out = done && -z $err && -z $rerr ]]; chk scratchperm.51

run_sash 'str _s=12; ! attest _s (( _n = _s )) || echo "$_s:$_n"; echo done'
[[ $rc = 0 && $out = $'12:12\ndone' && -z $err && -z $rerr ]]; chk scratchperm.52

run_sash 'str _s=bad; if attest _s (( _n = _s )) || /bin/false { echo yes } else { echo "${_s:!}:$_n" }'
[[ $rc = 0 && $out = bad:0 && -z $err && -z $rerr ]]; chk scratchperm.53

run_sash 'array _a=(ok); ! attest v in _a p? || echo "${_a[0]}"; echo done'
[[ $rc = 0 && $out = $'ok\ndone' && -z $err && -z $rerr ]]; chk scratchperm.54

run_sash 'array _a=(bad); if attest v in _a [[ $v == ok ]] || /bin/false { echo yes } else { echo no }'
[[ $rc = 0 && $out = no && -z $err && -z $rerr ]]; chk scratchperm.55

run_sash 'str _x=ok; if attest _x p? { : }; attest _x p?; echo $_x'
[[ $rc = 0 && $out = ok && -z $err && -z $rerr ]]; chk scratchperm.56

run_sash 'array _a=(); if attest v in _a p? { : }; array copy=(@{_a:!}); echo "$(len copy)"'
[[ $rc = 0 && $out = 0 && -z $err && -z $rerr ]]; chk scratchperm.57

run_sash 'write bad | read _a; if attest v in _a [[ $v == ok ]] { : }; array copy=(@{_a:!}); echo "${copy[0]}"'
[[ $rc = 0 && $out = bad && -z $err && -z $rerr ]]; chk scratchperm.58

tally
