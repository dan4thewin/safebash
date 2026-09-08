#!/bin/bash
# spec/var_modifiers.sh — DD#65 var-modifier prune. Built up per stage:
# toupper/tolower builtins (this stage), then basename/dirname sugar, the
# `:-` word, and the dropped-operator rejects.
set -eu
. common.sh
setup var_modifiers
plan=83
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
run_sash 'x=HeLLo; pat=hello; if match $(tolower $x) $(tolower $pat) { echo same } else { echo diff }'
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
# bvb.4 the retired `${#…}` length sugar rejects at parse (SB4038) regardless
#       of the operand — fires before the env-var reference check
run_sash 'echo ${#HOME}'
[[ $rc != 0 && -z $out && $err == *SB4038* ]]; chk bvb.4
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
# dr19-22: the NON-COLON default family (`-` `+` `=` `?`) is dropped too (DG
# 2026-08-29, DD#65 — no compiler coercion; one spelling, the colon forms).
# These used to parse as a bare `${x}` with the operator+word SILENTLY DROPPED
# (`${x+4}` even yielded `$x`, not the alternate) — now a clean SB4034.
run_sash 'x=5; echo "${x-4}"'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk vm.dr19
run_sash 'x=5; echo "${x+4}"'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk vm.dr20
run_sash 'x=5; echo "${x=4}"'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk vm.dr21
run_sash 'x=5; echo "${x?msg}"'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk vm.dr22
# un1-4: content the compiler does NOT recognise as an operator is rejected
# SB4047 (DG 2026-08-29: no ambiguity — never silently ignore trailing content).
# Distinct from SB4034 (known-dropped ops): these are genuinely unknown.
run_sash 'x=5; echo "${x&y}"'
[[ $rc != 0 && -z $out && $err == *SB4047* ]]; chk vm.un1
run_sash 'x=5; echo "${x:foo}"'
[[ $rc != 0 && -z $out && $err == *SB4047* ]]; chk vm.un2
run_sash 'x=5; echo "${x.y}"'
[[ $rc != 0 && -z $out && $err == *SB4047* ]]; chk vm.un3
run_sash 'x=5; echo "${x:}"'
[[ $rc != 0 && -z $out && $err == *SB4047* ]]; chk vm.un4

# ── Brace-less dispositions (DG 2026-09-04): `$x:-foo` / `$x:?msg` / `$x:~`
# desugar to the braced form. Only `:-`/`:?`/`:~` immediately after `$name`
# open a disposition; any other `:X` stays literal concatenation. Braces stay
# required for trailing concat, whitespace in the word, and inside `"…"`. ──
# vm.bl1 brace-less `:-` default, value present → the value
run_sash 'str x=hi; /bin/echo $x:-foo'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk vm.bl1
# vm.bl2 brace-less `:-` default, empty → the default word
run_sash 'str x=""; /bin/echo $x:-foo'
[[ $rc = 0 && $out = "foo" && -z $err ]]; chk vm.bl2
# vm.bl3 positional `$N:-` brace-less, empty → default
run_sash 'fn f { /bin/echo $1:-DEF }; f'
[[ $rc = 0 && $out = "DEF" && -z $err ]]; chk vm.bl3
# vm.bl4 positional `$N:-` brace-less, present → the arg. native,c: janet has a
# pre-existing positional-default bug — braced `${1:-DEF}` with an arg present
# ALSO returns DEF (identical janet output), so gate this to the correct
# backends (task janet-positional-default-present).
if in_modes native,c; then
    run_sash 'fn f { /bin/echo $1:-DEF }; f VAL'
    [[ $rc = 0 && $out = "VAL" && -z $err ]]; chk vm.bl4
else
    skip_chk vm.bl4
fi
# vm.bl5 brace-less `:~` elide, value present → the value
run_sash 'str x=hi; /bin/echo $x:~'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk vm.bl5
# vm.bl6 brace-less `:?` present → the value (no abort)
run_sash 'str x=hi; /bin/echo $x:?boom'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk vm.bl6
# vm.bl7 operand interpolation: `$1:-$y` uses `$y` as the default
run_sash 'fn f { str y=YY; /bin/echo $1:-$y }; f'
[[ $rc = 0 && $out = "YY" && -z $err ]]; chk vm.bl7
# vm.bl8 brace-less ≡ braced (same result side by side)
run_sash 'str x=hi; /bin/echo $x:-foo ${x:-foo}'
[[ $rc = 0 && $out = "hi hi" && -z $err ]]; chk vm.bl8
# vm.bl9 DISAMBIGUATION: `:X` that is not `:-`/`:?`/`:~` stays literal colon
# concatenation (path-like `$x:/bin`, `$x:3`) — unchanged by the sugar.
run_sash 'str x=hi; /bin/echo $x:/bin'
[[ $rc = 0 && $out = "hi:/bin" && -z $err ]]; chk vm.bl9
run_sash 'str x=hi; /bin/echo $x:3'
[[ $rc = 0 && $out = "hi:3" && -z $err ]]; chk vm.bl10
# vm.bl11 INTERPOLATION keeps braces mandatory (clarity): inside `"…"` a bare
# `$x:-foo` is `$x` + the literal `:-foo`, NOT a disposition.
run_sash 'str x=hi; /bin/echo "$x:-foo"'
[[ $rc = 0 && $out = "hi:-foo" && -z $err ]]; chk vm.bl11
# vm.bl12 whitespace in the word ends the brace-less operand: `$x:-a b` is
# default `a` then a separate word `b` (use braces for a spaced default).
run_sash 'str x=""; /bin/echo $x:-a b'
[[ $rc = 0 && $out = "a b" && -z $err ]]; chk vm.bl12
# vm.bl13 DD#91 follow-up: a `$( )` command substitution IS a valid brace-less
# operand now (DG 2026-09-04) — a clean builtin cmdsub default. native,c: janet
# has the pre-existing positional-default bug (janet-positional-default-present),
# so a positional with a cmdsub default is gated like vm.bl4.
if in_modes native,c; then
    run_sash 'fn f { /bin/echo $1:-$(getpid) }; f RUN'
    [[ $rc = 0 && $out = "RUN" && -z $err ]]; chk vm.bl13
else
    skip_chk vm.bl13
fi
# vm.bl14 `$(( ))` arithmetic operand brace-less
run_sash 'fn f { /bin/echo $1:-$((2 + 3)) }; f'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk vm.bl14
# vm.bl15 a `$( )` operand with an EXTERNAL capture is tainted → SB5020, exactly
# like the braced `${1:-$(ext)}` (the sugar changes only the spelling).
run_sash 'fn f { /bin/echo $1:-$(/bin/echo Z) }; f'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk vm.bl15

# ── DD#96: comma-chained `s///` edits ──
# DG 2026-09-08: `,` sequences edits, in BOTH the mutating statement form and
# the `${…}` expansion form, applied left to right. `,` is the separator sash
# already uses for a sequence of operations (DD#94 `(( ))`). Only `s///` edits
# chain (practical grounds).
# 96.1 statement form: two edits, left to right
run_sash 'v=ac
v =~ s/a/b/, s/c/d/
/bin/echo $v'
[[ $rc = 0 && $out = "bd" && -z $err ]]; chk 96.1
# 96.2 order matters — the second edit sees the first edit's output
run_sash 'v=a
v =~ s/a/b/, s/b/c/
/bin/echo $v'
[[ $rc = 0 && $out = "c" && -z $err ]]; chk 96.2
# 96.3 three edits chain
run_sash 'v=abc
v =~ s/a/1/, s/b/2/, s/c/3/
/bin/echo $v'
[[ $rc = 0 && $out = "123" && -z $err ]]; chk 96.3
# 96.4 per-edit flags: the `g` binds to ITS OWN edit, not the whole chain
run_sash 'v=aa
v =~ s/a/b/g, s/b/c/
/bin/echo $v'
[[ $rc = 0 && $out = "cb" && -z $err ]]; chk 96.4
# 96.5 expansion form chains the same way, and does not mutate
run_sash 'x=aa
/bin/echo ${x:s/a/b/, s/b/c/}
/bin/echo $x'
[[ $rc = 0 && $out = "ca
aa" && -z $err ]]; chk 96.5
# 96.6 a trailing comma continues onto the next line (DD#94 continuation rule)
run_sash 'v=abc
v =~ s/a/1/,
     s/b/2/,
     s/c/3/
/bin/echo $v'
[[ $rc = 0 && $out = "123" && -z $err ]]; chk 96.6
# 96.7 interpolation still works in a chained edit (DD#63)
run_sash 'p=a
v=ac
v =~ s/$p/X/, s/c/Y/
/bin/echo $v'
[[ $rc = 0 && $out = "XY" && -z $err ]]; chk 96.7
# 96.8 a single edit is unchanged (regression guard)
run_sash 'v=xay
v =~ s/a/b/
/bin/echo $v'
[[ $rc = 0 && $out = "xby" && -z $err ]]; chk 96.8
# 96.9 trailing junk after an edit is REJECTED. It used to be silently dropped:
# everything after the third `/` was treated as flags and only `g` was looked
# for, so `${x:s/a/b/:anything}` quietly applied just the first edit.
run_sash 'x=aa
/bin/echo ${x:s/a/b/:GARBAGE}'
[[ $rc != 0 && -z $out && $err == *SB4032* ]]; chk 96.9
# 96.10 …and the same in the statement form
run_sash 'v=aa
v =~ s/a/b/ junk
/bin/echo $v'
[[ $rc != 0 && -z $out && -n $err ]]; chk 96.10
# 96.11 only `s///` chains — a non-edit after a comma is malformed
run_sash 'x=aa
/bin/echo ${x:s/a/b/, u}'
[[ $rc != 0 && -z $out && $err == *SB4032* ]]; chk 96.11

set -e
tally
