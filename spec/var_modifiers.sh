#!/bin/bash
# spec/var_modifiers.sh — DD#65 var-modifier prune. Built up per stage:
# toupper/tolower builtins (this stage), then basename/dirname sugar, the
# `:-` word, and the dropped-operator rejects.
set -eu
. common.sh
setup var_modifiers
plan=49
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

# ── Braced rejected/env vars reject like the unbraced form (was a silent
# bypass: `${RANDOM}`/`${HOME}` compiled to empty — validate_expr only checked
# `$name`, not `${name…}`). Compile-time, all modes. ──
# bvb.1 braced rejected var → SB5017
run_sash 'echo ${RANDOM}'
[[ $rc != 0 && -z $out && $err == *SB5017* ]]; chk bvb.1
# bvb.2 braced env var → SB5018 (use $(getenv HOME))
run_sash 'echo ${HOME}'
[[ $rc != 0 && -z $out && $err == *SB5018* ]]; chk bvb.2
# bvb.3 an operator form referencing an env var still rejects (the name is a
#       reference regardless of the modifier)
run_sash 'echo ${HOME:-x}'
[[ $rc != 0 && -z $out && $err == *SB5018* ]]; chk bvb.3
# bvb.4 the length form `${#HOME}` rejects too
run_sash 'echo ${#HOME}'
[[ $rc != 0 && -z $out && $err == *SB5018* ]]; chk bvb.4
# bvb.5 a normal braced read / disposition stays clean (no false positive)
run_sash 'x=5; echo "${x} ${x:-9}"'
[[ $rc = 0 && $out = "5 5" && -z $err ]]; chk bvb.5

# ── dropped ${var:OP} operators reject uniformly (DD#65, SB4034) ─────
# Every split_operator op EXCEPT `:-` was dropped from sash; each rejects with
# SB4034 pointing at the string builtins. Migration is sash-convert's job. One
# test per operator string so the reject set can't silently miss one.
# dr1-2: assign-default / alt (:=, :+) stay dropped. (:? is now DD#77 —
# un-pruned as the `:-`/`:?` abort/default pair; tested just below.)
run_sash 'echo "${foo:=x}"'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk vm.dr1
run_sash 'foo=hi; echo "${foo:+x}"'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk vm.dr2
# vm.req1 DD#77 `${var:?MESSAGE}` — set/non-empty yields the value (all modes)
run_sash 'foo=hi; echo "${foo:?x}"'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk vm.req1
# vm.req2 unset → abort, exit 1, stderr `<prog>: MESSAGE` (message text checked
# native-only — janet's argv0 is the script path, not the binary).
run_sash 'echo "${foo:?must be set}"'
[[ $rc != 0 && -z $out ]] \
  && { ! in_modes native || [[ $rerr == *": must be set"* ]]; }; chk vm.req2
# vm.req3 empty also aborts (the `:` includes empty, not just unset)
run_sash 'foo=; echo "${foo:?empty}"'
[[ $rc != 0 && -z $out ]] \
  && { ! in_modes native || [[ $rerr == *": empty"* ]]; }; chk vm.req3
# dr4-5: prefix strip (#, ##) → basename / sub
run_sash 'x=abc; echo "${x#a}"'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk vm.dr4
run_sash 'x=abc; echo "${x##a}"'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk vm.dr5
# dr6-7: suffix strip (%, %%) → dirname / sub
run_sash 'x=abc; echo "${x%c}"'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk vm.dr6
run_sash 'x=abc; echo "${x%%c}"'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk vm.dr7
# dr8-11: glob replace (/, //, /#, /%) → sub / gsub
run_sash 'x=abc; echo "${x/a/Z}"'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk vm.dr8
run_sash 'x=aba; echo "${x//a/Z}"'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk vm.dr9
run_sash 'x=abc; echo "${x/#a/Z}"'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk vm.dr10
run_sash 'x=abc; echo "${x/%c/Z}"'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk vm.dr11
# dr12-13: first-char case (^, ,) — no replacement
run_sash 'x=abc; echo "${x^}"'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk vm.dr12
run_sash 'x=ABC; echo "${x,}"'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk vm.dr13
# dr14-15: all-case (^^, ,,) → toupper / tolower builtins
run_sash 'x=abc; echo "${x^^}"'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk vm.dr14
run_sash 'x=ABC; echo "${x,,}"'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk vm.dr15
# dr16-18: the per-element ARRAY forms ${arr[@]OP} are dropped too (DD#65,
# consistency with the scalar drop) — use a `for x in @a` loop + sub/gsub.
run_sash 'array a=(foo bar baz); echo ${a[@]/ba/BA}'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk vm.dr16
run_sash 'array a=(pre_one pre_two three); echo ${a[@]#pre_}'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk vm.dr17
run_sash 'array a=(file.txt doc.txt note.md); echo ${a[@]%%.txt}'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk vm.dr18

set -e
tally
