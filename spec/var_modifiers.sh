#!/bin/bash
# spec/var_modifiers.sh — DD#65 var-modifier prune. Built up per stage:
# toupper/tolower builtins (this stage), then basename/dirname sugar, the
# `:-` word, and the dropped-operator rejects.
set -eu
. common.sh
setup var_modifiers
plan=24
echo "1..$plan" >&3

set +e

# ── toupper / tolower builtins (DD#65 — irreducible case conversion) ──
run_sash 's=hello; echo $(toupper $s)'
[[ $rc = 0 && $out = "HELLO" && -z $err ]]; chk vm.tu1
run_sash 's=World; echo $(tolower $s)'
[[ $rc = 0 && $out = "world" && -z $err ]]; chk vm.tu2
run_sash 'echo $(toupper hi)'
[[ $rc = 0 && $out = "HI" && -z $err ]]; chk vm.tu3
run_sash 'a=$(tolower ABC); echo "[$a]"'
[[ $rc = 0 && $out = "[abc]" && -z $err ]]; chk vm.tu4
# vm.tu5 digits / non-letters pass through
run_sash 'echo $(toupper aB3_z)'
[[ $rc = 0 && $out = "AB3_Z" && -z $err ]]; chk vm.tu5
# vm.tu6 concatenation of two conversions
run_sash 'echo $(toupper foo)-$(tolower BAR)'
[[ $rc = 0 && $out = "FOO-bar" && -z $err ]]; chk vm.tu6
# vm.tu7 the case-insensitive match idiom (DD#65: regex has no `i` flag)
run_sash 'x=HeLLo; pat=hello; match $(tolower $x) $(tolower $pat) && echo same || echo diff'
[[ $rc = 0 && $out = "same" && -z $err ]]; chk vm.tu7

# ── basename / dirname sugar (DD#65 — rewritten to a fixed-pattern sub) ──
run_sash 'p=/a/b/c.txt; echo $(basename $p)'
[[ $rc = 0 && $out = "c.txt" && -z $err ]]; chk vm.bn1
run_sash 'p=/a/b/c.txt; echo $(dirname $p)'
[[ $rc = 0 && $out = "/a/b" && -z $err ]]; chk vm.bn2
# vm.bn3 a slashless path is its own basename
run_sash 'p=foo.txt; echo $(basename $p)'
[[ $rc = 0 && $out = "foo.txt" && -z $err ]]; chk vm.bn3
run_sash 'p=/usr/bin; echo $(basename $p)'
[[ $rc = 0 && $out = "bin" && -z $err ]]; chk vm.bn4
run_sash 'p=/usr/bin; echo $(dirname $p)'
[[ $rc = 0 && $out = "/usr" && -z $err ]]; chk vm.bn5
# vm.bn6 basename of a literal path
run_sash 'echo $(basename /x/y/z)'
[[ $rc = 0 && $out = "z" && -z $err ]]; chk vm.bn6

# ── `${var:-WORD}` default is a full interpolating word (DD#65) ──────
# V1 plain literal default
run_sash 'echo "${foo:-abc}"'
[[ $rc = 0 && $out = "abc" && -z $err ]]; chk vm.dv1
# V2 a literal `}` in the default via a $'...' C-string (the any-byte hatch;
# `:- ≡ "…"`, so quotes no longer strip — the C-string is how you get a `}`)
run_sash $'echo "${foo:-$\'a\\x7db\'}"'
[[ $rc = 0 && $out = "a}b" && -z $err ]]; chk vm.dv2
# V3 interpolation: an unset default interpolates a variable
run_sash 'bar=hi; echo "${foo:-$bar}"'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk vm.dv3
# V4 concatenation in the default
run_sash 'x=a; y=b; echo "${foo:-$x$y}"'
[[ $rc = 0 && $out = "ab" && -z $err ]]; chk vm.dv4
# V5 an unquoted `}` closes the expansion; trailing text concatenates
run_sash 'echo "${foo:-a}b"'
[[ $rc = 0 && $out = "ab" && -z $err ]]; chk vm.dv5
# V6 arithmetic (clean) is allowed in the default
run_sash 'echo "${foo:-$((1+2))}"'
[[ $rc = 0 && $out = "3" && -z $err ]]; chk vm.dv6
# V7 a set variable ignores the default entirely
run_sash 'foo=hi; echo "${foo:-x}"'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk vm.dv7
# V8 `:- ≡ "…"`: single-quotes are ordinary literal bytes (they no longer strip)
run_sash 'echo "${foo:-'\''x'\''}"'
[[ $rc = 0 && $out = "'x'" && -z $err ]]; chk vm.dv8
# V9 whitespace is part of the value and kept verbatim (no run-collapsing)
run_sash 'echo "${foo:-a  b}"'
[[ $rc = 0 && $out = "a  b" && -z $err ]]; chk vm.dv9
# V10 `'` is plain, so the first literal `}` closes the expansion: `${foo:-'a}`
# (default `'a`) then `b'}` is ordinary trailing text (embed `}` via dv2's hatch)
run_sash 'echo "${foo:-'\''a}b'\''}"'
[[ $rc = 0 && $out = "'ab'}" && -z $err ]]; chk vm.dv10
# V11 (negative of dv3) the default is a real interpolation context, so a bare
# env var is rejected (SB5018) exactly as elsewhere — must use $(getenv HOME)
run_sash 'echo "${foo:-$HOME}"'
[[ $rc != 0 && -z $out && $err == *SB5018* ]]; chk vm.dv11

set -e
tally
