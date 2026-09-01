#!/bin/bash
# spec/extern_args.sh — DD#87: every sash string is NUL-terminated, so a
# computed argument crosses into an EXTERNAL command (execve reads argv entries
# as C strings) without trailing garbage. These use `/bin/echo` deliberately —
# the builtin `echo` is length-based and never exercised the C-string boundary,
# which is exactly why this class of corruption went unnoticed. `[[ $out = … ]]`
# with an exact match catches any garbage the external command would append.
# native + c only: janet doesn't link sash-rt (it runs on the janet VM).
set -eu
. common.sh
setup extern_args
set +e
plan=8
echo "1..$plan" >&3

if in_modes native,c; then
    # xa.1 concat of an int var and a string var (the original repro)
    run_sash 'int n=5
s=hi
/bin/echo "n=$n s=$s"'
    [[ $rc = 0 && $out = "n=5 s=hi" && -z $rerr ]]; chk xa.1

    # xa.2 int interpolation (sash_itoa) into an external arg
    run_sash 'int n=42
/bin/echo "[$n]"'
    [[ $rc = 0 && $out = "[42]" && -z $rerr ]]; chk xa.2

    # xa.3 a partial substring (sash_substring must copy, not slice)
    run_sash 's=hello
/bin/echo "${s:0:2}"'
    [[ $rc = 0 && $out = "he" && -z $rerr ]]; chk xa.3

    # xa.4 substring from the middle — the tail must NOT leak
    run_sash 's=abcdef
/bin/echo "<${s:2:2}>"'
    [[ $rc = 0 && $out = "<cd>" && -z $rerr ]]; chk xa.4

    # xa.5 a chain of concats ending in a string var, to an external command
    run_sash 'a=one
b=two
int k=3
/bin/echo "$a-$b-$k"'
    [[ $rc = 0 && $out = "one-two-3" && -z $rerr ]]; chk xa.5

    # xa.6 join of argv into an external arg
    run_sash 'fn f { /bin/echo "$(join @args)" }
f x y z'
    [[ $rc = 0 && $out = "x y z" && -z $rerr ]]; chk xa.6

    # xa.7 an empty computed string arg to an external command
    run_sash 's=hi
/bin/echo "[${s:0:0}]"'
    [[ $rc = 0 && $out = "[]" && -z $rerr ]]; chk xa.7

    # xa.8 two mixed int/string interpolations, reversed order
    run_sash 'int n=7
s=end
/bin/echo "$s then $n"'
    [[ $rc = 0 && $out = "end then 7" && -z $rerr ]]; chk xa.8
else
    skip_chk xa.1; skip_chk xa.2; skip_chk xa.3; skip_chk xa.4
    skip_chk xa.5; skip_chk xa.6; skip_chk xa.7; skip_chk xa.8
fi

set -e
tally
