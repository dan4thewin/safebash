#!/bin/bash
# spec/var_modifiers.sh — DD#65 var-modifier prune. Built up per stage:
# toupper/tolower builtins (this stage), then basename/dirname sugar, the
# `:-` word, and the dropped-operator rejects.
set -eu
. common.sh
setup var_modifiers
plan=156
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
# vm.req2 unset → abort, exit 1, stderr `<prog>: MESSAGE`.
run_sash 'echo "${foo:?must be set}"'
[[ $rc != 0 && -z $out ]] \
  && { ! in_modes native || [[ $rerr == *": must be set"* ]]; }; chk vm.req2
# vm.req3 empty also aborts (the `:` includes empty, not just unset)
run_sash 'foo=; echo "${foo:?empty}"'
[[ $rc != 0 && -z $out ]] \
  && { ! in_modes native || [[ $rerr == *": empty"* ]]; }; chk vm.req3
# vm.req4-vm.req8 DD#78: the `:?` MESSAGE is a full interpolating word, exactly
#       like the `:-` default. It was parsed as an ExprKind::Literal, so sash
#       printed `$m` where bash prints the value — an oracle divergence, and
#       the reason the operand was invisible to every other pass.
# vm.req4 a variable in the message expands
run_sash 'str m=boom
str y=""
str z=${y:?$m}
echo "[$z]"'
[[ $rc != 0 && -z $out ]] \
  && { ! in_modes native || [[ $rerr == *": boom" ]]; }; chk vm.req4
# vm.req5 so does an arithmetic expansion — a different interpolation form, so
#       this is not just testing the `$name` case twice
run_sash 'str y=""
str z=${y:?count $(( 2 + 3 ))}
echo "[$z]"'
[[ $rc != 0 && -z $out ]] \
  && { ! in_modes native || [[ $rerr == *": count 5" ]]; }; chk vm.req5
# vm.req6 literal text and an expansion mix in one message
run_sash 'str m=boom
str y=""
str z=${y:?pre $m post}
echo "[$z]"'
[[ $rc != 0 && -z $out ]] \
  && { ! in_modes native || [[ $rerr == *": pre boom post" ]]; }; chk vm.req6
# vm.req7 the message is only materialized on the abort path — a set value
#       yields the value and never evaluates it
run_sash 'str m=boom
str y=ok
str z=${y:?$m}
echo "[$z]"'
[[ $rc = 0 && $out = "[ok]" && -z $err ]]; chk vm.req7
# vm.req8 a message with no expansion in it is unchanged — the guard that the
#       interpolating parse did not break plain text
run_sash 'str y=""
str z=${y:?plain text}
echo "[$z]"'
[[ $rc != 0 && -z $out ]] \
  && { ! in_modes native || [[ $rerr == *": plain text" ]]; }; chk vm.req8
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

# un5-6: an unterminated `${` runs off the end of the source. `scan_by_brace_depth`
# loops `while pos < len && depth > 0`; the length guard read 0 hits against
# 10,820 in branch coverage (journal 312) because every `${` in the suite closes.
# Following that arm found the scan stopping at end-of-input and pretending it
# had found the `}` -- `echo ${x` compiled and printed the value. SB4057.
run_sash 'x=5; echo ${x'
[[ $rc != 0 && -z $out && $err == *SB4057* ]]; chk vm.un5
run_sash 'x=5; echo "${x"'
[[ $rc != 0 && -z $out && $err == *SB4057* ]]; chk vm.un6

# un7-11: a subscript's collection name must be a *variable* name. The `${…}`
# body scanner accepts any run of `[A-Za-z0-9_]`, which also admits a leading
# digit and an empty run, so `${1[0]}` and `${[0]}` built a Subscript on a
# collection nothing can declare and codegen panicked ("ICE: undefined array").
# Found by branch-coverage mining. DG chose the parse-time reject reusing the
# existing SB4047 body family.
run_sash 'echo ${[0]}'
[[ $rc != 0 && -z $out && $err == *SB4047* && $err != *ICE* ]]; chk vm.un7
run_sash 'echo ${1[0]}'
[[ $rc != 0 && -z $out && $err == *SB4047* && $err != *ICE* ]]; chk vm.un8
run_sash 'echo ${12[0]}'
[[ $rc != 0 && -z $out && $err == *SB4047* && $err != *ICE* ]]; chk vm.un9
# the same two spellings inside arithmetic, which reaches the subscript by a
# second path (`try_braced_subscript`, parse/arith.rs)
run_sash 'echo $(( ${1[0]} ))'
[[ $rc != 0 && -z $out && $err == *SB4047* && $err != *ICE* ]]; chk vm.un10
run_sash 'echo $(( ${[0]} ))'
[[ $rc != 0 && -z $out && $err == *SB4047* && $err != *ICE* ]]; chk vm.un11

# un12: a digit *inside* the name is still a name — the reject is a leading
# digit, not any digit, so this must keep working
run_sash 'array a1=(7 8)
echo ${a1[1]}'
[[ $rc = 0 && $out = "8" && -z $err ]]; chk vm.un12

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
# vm.bl4 positional `$N:-` brace-less, present → the arg.
run_sash 'fn f { /bin/echo $1:-DEF }; f VAL'
[[ $rc = 0 && $out = "VAL" && -z $err ]]; chk vm.bl4
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
# operand now (DG 2026-09-04) — a clean builtin cmdsub default.
run_sash 'fn f { /bin/echo $1:-$(getpid) }; f RUN'
[[ $rc = 0 && $out = "RUN" && -z $err ]]; chk vm.bl13
# vm.bl14 `$(( ))` arithmetic operand brace-less
run_sash 'fn f { /bin/echo $1:-$((2 + 3)) }; f'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk vm.bl14
# vm.bl15 a `$( )` operand with an EXTERNAL capture is tainted → SB5020, exactly
# like the braced `${1:-$(ext)}` (the sugar changes only the spelling).
run_sash 'fn f { /bin/echo $1:-$(/bin/echo Z) }; f'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk vm.bl15
# vm.bl16 `${…}` interpolation inside a brace-less operand. The operand scan
# must follow the braces to their match rather than stopping at the first `}`
# (lexer.rs:skip_braced_to_close) — without it the operand would end mid-word.
run_sash 'str y=inner; str x=""; /bin/echo $x:-${y}'
[[ $rc = 0 && $out = "inner" && -z $err ]]; chk vm.bl16
# vm.bl17 a `)` inside quotes inside a `$( )` brace-less operand. The paren
# scan counts depth, so it must skip quoted regions or this `)` closes the
# substitution early and the operand is truncated to `$(echo "a`.
run_sash 'str x=""; /bin/echo $x:-$(echo "a)b")'
[[ $rc = 0 && $out = "a)b" && -z $err ]]; chk vm.bl17
# vm.bl18 a `` `…` `` run inside a brace-less operand. Asserted as brace-less ≡
# braced rather than against a literal: the default word is a `"…"` body, where
# a backtick is a literal character (DG 2026-08-28), and this test is about the
# two spellings agreeing — it stays correct if that rule is ever revisited.
run_sash 'str x=""; str a=$x:-`echo hi`; str b=${x:-`echo hi`}; [[ $a == $b ]] && /bin/echo same'
[[ $rc = 0 && $out = "same" && -z $err ]]; chk vm.bl18
# vm.bl19-vm.bl23 the brace-less operand's own `$name` scanner
# (`lexer.rs::skip_dollar_var`). The whole sweep called it three times,
# always with a single alphabetic byte followed by a terminator, so seven
# of its branches had never run: the special/digit arm, the leading
# underscore, an underscore *inside* a name, the multi-byte name loop, and
# the run-out-of-source exit. vm.bl16 covers `${…}` in this position, which
# takes `skip_braced_to_close` instead and left this path untested.
# vm.bl19 a multi-byte name — and the source ends at its last byte, so the
# scan loop exits on end-of-input rather than on a terminator
run_sash 'str abc=Q; str x=""; /bin/echo $x:-$abc'
[[ $rc = 0 && $out = "Q" && -z $err ]]; chk vm.bl19
# vm.bl20 an underscore *inside* the name (not alphanumeric, still a name byte)
run_sash 'str a_b=W; str x=""; /bin/echo $x:-$a_b'
[[ $rc = 0 && $out = "W" && -z $err ]]; chk vm.bl20
# vm.bl21 a *leading* underscore — the second alternative of the name-start
# test, which the alphabetic one had always short-circuited past
run_sash 'str _v=U; str x=""; /bin/echo $x:-$_v'
[[ $rc = 0 && $out = "U" && -z $err ]]; chk vm.bl21
# vm.bl22 `$1` — the special/digit arm. `$@ $* $# $? $! $$` and any digit
# share it; a positional is the spelling that compiles here.
run_sash 'fn f { str q=""; /bin/echo $q:-$1 }; f ARG'
[[ $rc = 0 && $out = "ARG" && -z $err ]]; chk vm.bl22
# vm.bl23 a `$` that starts no name at all stays a literal `$`, and the
# operand continues past it
run_sash 'str x=""; /bin/echo $x:-a$%b'
[[ $rc = 0 && $out = 'a$%b' && -z $err ]]; chk vm.bl23

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


# ---------------------------------------------- elide (:~) in arithmetic
# `${x:~}` never yields a value, so an arithmetic operand position has no
# meaning for it — the same reasoning SB5046 already applies to an assignment
# RHS. The elide check walked commands, tests and array literals but never
# descended into an `ArithExpr`, so these reached codegen and PANICKED
# ("ICE: ElideEmpty (:~) must be expanded by a list builder").

# ice.7 `(( y = ${x:~} ))` — a bare arithmetic statement
run_sash 'x=hi; (( y = ${x:~} )); echo $y'
[[ $rc != 0 && -z $out && $err == *SB5046* && $err != *ICE* ]]; chk ice.7

# ice.8 `$(( ${x:~} ))` — arithmetic in value position
run_sash 'x=hi; v=$(( ${x:~} )); echo $v'
[[ $rc != 0 && -z $out && $err == *SB5046* && $err != *ICE* ]]; chk ice.8

# ice.9 an arithmetic condition
run_sash 'x=hi; if (( ${x:~} > 1 )) { echo T }'
[[ $rc != 0 && -z $out && $err == *SB5046* && $err != *ICE* ]]; chk ice.9

# ice.10 inside an arithmetic subscript index (the arith walker skipped
# Subscript/SubscriptAssign children entirely)
run_sash 'int array a=(1 2); x=hi; (( y = a[${x:~}] )); echo $y'
[[ $rc != 0 && -z $out && $err == *SB5046* && $err != *ICE* ]]; chk ice.10


# ══════ the s/// STATEMENT form must accept whatever the source says ══════
# `reconstruct_sed_raw` rebuilt the payload token by token and broke on any
# token it did not enumerate, so metacharacters that lex as operators (= # & <
# > ;) and `$'…'` all gave SB4032 "malformed … expected three / delimiters" on
# a perfectly well-formed payload. The EXPANSION form `${v:s/…/…/}` never had
# this: DD#63 Path B scans the payload from the raw source. The statement form
# now does the same, so the two agree.

# sr.1 `=` in the RE — the k=v case DD#99's map building needs
run_sash 's=NAME=x
s =~ s/=/-/
echo $s'
[[ $rc = 0 && $out = "NAME-x" && -z $err ]]; chk sr.1

# sr.2 the other operator-lexed metacharacters
run_sash 's="a#b"
s =~ s/#/-/
echo $s'
[[ $rc = 0 && $out = "a-b" && -z $err ]]; chk sr.2

# sr.3 `;` — lexes as a statement separator
run_sash 's="a;b"
s =~ s/;/-/
echo $s'
[[ $rc = 0 && $out = "a-b" && -z $err ]]; chk sr.3

# sr.4 `&` and the redirect chars
run_sash 's="a&b"
s =~ s/&/-/
echo $s'
[[ $rc = 0 && $out = "a-b" && -z $err ]]; chk sr.4

# sr.5 `<` / `>`
run_sash 's="a<b>c"
s =~ s/</-/, s/>/-/
echo $s'
[[ $rc = 0 && $out = "a-b-c" && -z $err ]]; chk sr.5

# sr.6 `$'\0'` directly in the replacement — DD#64's escape, and the DD#99
# separator conversion. The result carries an interior NUL, so compare length.
run_sash 's=a:b:c
s =~ s/:/$'"'"'\x00'"'"'/g
echo $(strlen $s)'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk sr.6

# sr.7 …and the bytes really are NUL (round-trips through a NUL-split read)
run_sash 'array f=()
s=a:b:c
s =~ s/:/$'"'"'\x00'"'"'/g
echo $s | read f
attest e in f [[ $e == $e ]]
echo "$(len f):${f[1]}"'
[[ $rc = 0 && $out = "3:b" && -z $err ]]; chk sr.7

# sr.8 the expansion form agrees (it always worked — pin it so they cannot drift)
run_sash 's=NAME=x
t=${s:s/=/-/}
echo $t'
[[ $rc = 0 && $out = "NAME-x" && -z $err ]]; chk sr.8

# sr.9 DD#96 chaining still works across the change (regression guard)
run_sash 'v=abc
v =~ s/a/1/, s/b/2/
echo $v'
[[ $rc = 0 && $out = "12c" && -z $err ]]; chk sr.9

# sr.10 …including the multi-line chain (DD#96 continuation)
run_sash 'v=abc
v =~ s/a/1/,
    s/b/2/
echo $v'
[[ $rc = 0 && $out = "12c" && -z $err ]]; chk sr.10


# sr.11 a literal `/` in the replacement is spelled `$'\x2f'` (DD#63's own
# answer, now working in the statement form too). Because the payload is
# scanned from raw SOURCE, `$'\x2f'` contains no `/` byte, so the delimiter
# split and the escape decode cannot interfere.
run_sash 's="a-b"
s =~ s/-/$'"'"'\x2f'"'"'/
echo $s'
[[ $rc = 0 && $out = "a/b" && -z $err ]]; chk sr.11

# sr.12 …and in the RE slot
run_sash 's="a/b"
s =~ s/$'"'"'\x2f'"'"'/-/
echo $s'
[[ $rc = 0 && $out = "a-b" && -z $err ]]; chk sr.12

# sr.13 a BARE `/` in the replacement is still the delimiter, so a fourth one
# is malformed — NOT a silent split that leaves `/` to be run as a command.
# (The raw-source scan regressed this until the payload was required to end at
# a statement boundary.)
run_sash 's="a-b"
s =~ s/-/x/y/
echo $s'
[[ $rc != 0 && -z $out && $err == *SB4032* ]]; chk sr.13

# sr.14 trailing junk after the flags is malformed, as it has always been
run_sash 's="a-b"
s =~ s/-/x/zz9
echo $s'
[[ $rc != 0 && -z $out && $err == *SB4032* ]]; chk sr.14

# sr.15 a `;`-separated statement after the payload still parses
run_sash 's="a-b"
s =~ s/-/x/; echo $s'
[[ $rc = 0 && $out = "axb" && -z $err ]]; chk sr.15

# sr.16 flags still bind (g is global)
run_sash 's="a-b-c"
s =~ s/-/x/g
echo $s'
[[ $rc = 0 && $out = "axbxc" && -z $err ]]; chk sr.16


# ══════ DD#99 stage 3: `map m = @{a:DISP}` — a map from an array ══════
# The array is consumed PAIRWISE (k1 v1 k2 v2 …). An odd count leaves a
# trailing element with no partner, and — as DD#69 requires of an
# external-command splat — the site must state what happens. A bare `@a` is
# rejected: a silently mis-paired map is the same hazard as a silently-empty
# argument list. (It used to compile and yield an EMPTY map, silently.)

# mp.1 a bare splat must state a disposition
run_sash 'array a=(k v)
map m = @a
echo "$(len m)"'
[[ $rc != 0 && -z $out && $err == *SB5058* ]]; chk mp.1

# mp.2 an even count pairs up; the disposition is simply unused
run_sash 'array a=(k1 v1 k2 v2)
map m = @{a:-X}
echo "$(len m):${m[k1]}:${m[k2]}"'
[[ $rc = 0 && $out = "2:v1:v2" && -z $err ]]; chk mp.2

# mp.3 `:-KEY` — the trailing element becomes the VALUE of KEY. DG: supplying
# the key gives a landmark (${m[KEY]}); supplying a value would force a scan.
run_sash 'array a=(k1 v1 k2)
map m = @{a:-EXTRA}
echo "$(len m):${m[k1]}:${m[EXTRA]}"'
[[ $rc = 0 && $out = "2:v1:k2" && -z $err ]]; chk mp.3

# mp.4 `:~` discards the trailing element
run_sash 'array a=(k1 v1 k2)
map m = @{a:~}
echo "$(len m):${m[k1]}"'
[[ $rc = 0 && $out = "1:v1" && -z $err ]]; chk mp.4

# mp.5 `:?MSG` aborts at runtime on an odd count, with the message
run_sash 'array a=(k1 v1 k2)
map m = @{a:?uneven}
echo "$(len m)"'
[[ -z $out && $rerr == *uneven* ]]; chk mp.5

# mp.6 …and does NOT abort on an even count
run_sash 'array a=(k1 v1)
map m = @{a:?uneven}
echo "$(len m):${m[k1]}"'
[[ $rc = 0 && $out = "1:v1" && -z $err ]]; chk mp.6

# mp.7 an EMPTY array is a valid empty map — zero is even, nothing is missing,
# so DD#69's empty-array reading does not apply in this position
run_sash 'array a=()
map m = @{a:~}
echo "$(len m)"'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk mp.7

# mp.8 the whole DD#99 map pipeline, in-process: k=v text → NUL → array → map
run_sash 'sep=$'"'"'\x00'"'"'
s=NAME:sash
s =~ s/:/$sep/g
read $s a
attest e in a [[ $e == $e ]]
map m = @{a:?uneven}
echo "${m[NAME]}"'
[[ $rc = 0 && $out = "sash" && -z $err ]]; chk mp.8

# mp.9 `:!~` discards like `:~` and additionally trusts the elements (DD#91).
# `!` is the modifier, `~` the disposition — a splat must still state one.
run_sash 'array a=(k1 v1 k2)
map m = @{a:!~}
echo "$(len m):${m[k1]}"'
[[ $rc = 0 && $out = "1:v1" && -z $err ]]; chk mp.9


# ── DD#99 stage 8: an array is not built from a splat ─────────────────────
# `array a = @m` was ACCEPTED and silently produced an EMPTY array — the
# "gives an undefined answer to input it accepted" shape. A map is built FROM
# an array (`map m = @{a:DISP}`); the reverse was never specified, and
# guessing a flatten order for the author is the same silent mis-pairing that
# DD#69/DD#99 make you state a disposition to avoid.

# af.1 the bare form
run_sash 'array p=(k1 v1)
map m = @{p:~}
array a = @m
echo "$(len a)"'
[[ $rc != 0 && -z $out && $err == *SB5061* ]]; chk af.1

# af.2 …and with a disposition, which does not make it meaningful
run_sash 'array p=(k1 v1)
map m = @{p:~}
array a = @{m:~}
echo "$(len a)"'
[[ $rc != 0 && -z $out && $err == *SB5061* ]]; chk af.2

# af.3 control: an ordinary array literal is untouched
run_sash 'array a=(x y)
echo "$(len a)"'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk af.3

# af.4 control: the SUPPORTED direction still works
run_sash 'array p=(k v)
map m = @{p:~}
echo "$(len m):${m[k]}"'
[[ $rc = 0 && $out = "1:v" && -z $err ]]; chk af.4

# ── DD#75 (2026-09-11, DG): a splat yields VALUES, not indices ──────────────
# `@a` does not produce `0 1 2`, so `@m` does not produce keys. Map keys are
# indices, not data — which is why "join m drops the keys" was the wrong
# reading, and `join-map-reject` was closed as not-a-bug. The behaviour below
# was already what the compiler did; it was pinned by nothing.
#

# af.5 `join` on a map joins its values, exactly as on an array
run_sash 'array p=(k1 v1 k2 v2)
map m = @{p:~}
echo "$(join m ,)"'
[[ $rc = 0 && $out = "v1,v2" && -z $err ]]; chk af.5

# af.6 the array form, for the symmetry the rule rests on: the elements, not
# the indices
run_sash 'array a=(x y z)
echo "$(join a ,)"'
[[ $rc = 0 && $out = "x,y,z" && -z $err ]]; chk af.6

# af.7 splatting a map into a for-in yields values too (already true, unpinned)
run_sash 'array p=(k1 v1 k2 v2)
map m = @{p:~}
for v in @m { echo "$v" }'
[[ $rc = 0 && $out = "v1
v2" && -z $err ]]; chk af.7

# af.8 `keys` is how you ask for the indices — the capability is not lost, it
# just has its own spelling (DD#75 section 3, map-only)
run_sash 'array p=(k1 v1 k2 v2)
map m = @{p:~}
for k in $(keys m) { echo "$k" }'
[[ $rc = 0 && $out = "k1
k2" && -z $err ]]; chk af.8

# af.9 `len` on a map counts entries
run_sash 'array p=(k1 v1 k2 v2)
map m = @{p:~}
echo "$(len m)"'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk af.9

# af.10 why SB5061 (af.1) survives the values ruling: the round trip is broken.
# `map m = @{a:~}` consumes alternating k/v, but `@m` is the VALUES — so
# re-mapping them cannot rebuild the map. `array a = @m` would sit opposite
# map-construction and read as its inverse while silently not being one.
# Shown here through the spellings that ARE legal.
run_sash 'array a=(k1 v1 k2 v2)
map m = @{a:~}
echo "m entries: $(len m)"
echo "values: $(join m ,)"
array b=(v1 v2)
map m2 = @{b:~}
echo "remapped entries: $(len m2)"
for k in $(keys m2) { echo "remapped key: $k" }'
[[ $rc = 0 && $out = "m entries: 2
values: v1,v2
remapped entries: 1
remapped key: v1" && -z $err ]]; chk af.10


# ── SB5070: an array splat in a scalar operand of a value intrinsic ───────
# DD#22/#69 give a splat one argument per element, so a splat in a scalar
# slot is a category error. It used to collapse into a single joined
# argument, so `$(gsub @a ":" "-")` silently ran `gsub "$(join a)" …` —
# arity checked out because a splat is one *syntactic* argument. All seven
# scalar-operand intrinsics shared the defect, not just `gsub`.

# 96.12 the filed case: `gsub` with a splat subject
run_sash 'array a=(x:1 y:2 z:3)
t=$(gsub @a ":" "-")
echo "[$t]"'
[[ $rc != 0 && -z $out && $err == *SB5070* ]]; chk 96.12

# 96.13 the advice names `join`, which is DD#75's explicit joined-string form
[[ $err == *join* ]]; chk 96.13

# 96.14 `sub` goes with it
run_sash 'array a=(x:1 y:2)
t=$(sub @a ":" "-")
echo "[$t]"'
[[ $rc != 0 && -z $out && $err == *SB5070* ]]; chk 96.14

# 96.15 so do the single-operand string intrinsics
run_sash 'array a=(x y)
t=$(toupper @a)
echo "[$t]"'
[[ $rc != 0 && -z $out && $err == *SB5070* ]]; chk 96.15

run_sash 'array a=(/p/q /r/s)
t=$(basename @a)
echo "[$t]"'
[[ $rc != 0 && -z $out && $err == *SB5070* ]]; chk 96.16

run_sash 'array a=(ab cd)
t=$(match @a "a")
echo "[$t]"'
[[ $rc != 0 && -z $out && $err == *SB5070* ]]; chk 96.17

# 96.18 a splat carrying a disposition is still a splat
run_sash 'array a=(x:1 y:2)
t=$(sub @{a:~} ":" "-")
echo "[$t]"'
[[ $rc != 0 && -z $out && $err == *SB5070* ]]; chk 96.18

# 96.19 `@args` is a collection splat too — no exemption here (the SB5032
# arity guarantee is about running an external command with too few
# arguments, which has no bearing on a scalar operand)
run_sash 'fn f {
  t=$(gsub @args ":" "-")
  echo "[$t]"
}
f a:1'
[[ $rc != 0 && -z $out && $err == *SB5070* ]]; chk 96.19

# 96.20 control: the advice actually works — joining first compiles and runs
run_sash 'array a=(x:1 y:2)
t=$(gsub "$(join a " ")" ":" "-")
echo "[$t]"'
[[ $rc = 0 && $out = "[x-1 y-2]" && -z $rerr ]]; chk 96.20

# 96.21 control: a scalar subject is untouched
run_sash 's="x:1:2"
t=$(gsub $s ":" "-")
echo "[$t]"'
[[ $rc = 0 && $out = "[x-1-2]" && -z $rerr ]]; chk 96.21

# 96.22 control: `len` and `join` take a collection by name, so a splat
# there is NOT this error — they stay as they were (DD#75 open questions)
run_sash 'array a=(x y z)
echo "$(len a):$(join a)"'
[[ $rc = 0 && $out = "3:x y z" && -z $rerr ]]; chk 96.22

# vm.bl24 DD#90 rule 5 reaches a DISPOSITION'S OPERAND, not just `"…"` (DG,
#         2026-09-27). The operand is parsed as an interpolating parts list, so
#         a brace-less disposition inside it stays a literal colon-run: the
#         `:!` here is text appended to the default, NOT a taint-off modifier.
#         DG chose this over extending brace-less into the operand — he would
#         reach for `attest` rather than lean on bypass ergonomics.
run_sash 'str y=abc
str x=""
str z=$x:-$y:!
echo "[$z]"'
[[ $rc = 0 && $out = "[abc:!]" && -z $err ]]; chk vm.bl24

# vm.bl25 …and the braced spelling is the one that actually dispositions.
#         Paired with vm.bl24 so the two readings cannot both drift.
run_sash 'str y=abc
str x=""
str z=$x:-${y:!}
echo "[$z]"'
[[ $rc = 0 && $out = "[abc]" && -z $err ]]; chk vm.bl25

set -e
tally
