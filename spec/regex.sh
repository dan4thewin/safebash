#!/bin/bash
# spec/regex.sh — DD#63 regex builtin family (match / sub / gsub) + thin sugar.
# Regex is a builtin family taking ordinary string args; quoting/interpolation
# fall out of normal argument evaluation. The s/// and [[ =~ ]] syntaxes are
# thin sugar that desugar to these builtins.
set -eu
. common.sh
setup regex
plan=16
echo "1..$plan" >&3

set +e

# ── A. sub / gsub function forms (primitives) ────────────────────────

# 63.4 sub replaces the FIRST match, prints result
run_sash 'echo $(sub abab "b" "B")'
[[ $rc = 0 && $out = "aBab" && -z $err ]]; chk 63.4

# 63.5 gsub replaces ALL matches
run_sash 'echo $(gsub abab "b" "B")'
[[ $rc = 0 && $out = "aBaB" && -z $err ]]; chk 63.5

# 63.b3 a literal slash is just an argument byte — no delimiter parsing
run_sash 'echo $(sub "a/b/c" "/" "_")'
[[ $rc = 0 && $out = "a_b/c" && -z $err ]]; chk 63.b3

# 63.f1 empty replacement deletes the match
run_sash 'echo $(sub abc "b" "")'
[[ $rc = 0 && $out = "ac" && -z $err ]]; chk 63.f1

# ── B. match — value form $(match S P) = whole match (resub[0]) ───────

# 63.1a $(match S P) prints the whole match
run_sash 'echo $(match "hello world" "world")'
[[ $rc = 0 && $out = "world" && -z $err ]]; chk 63.1a

# 63.1b a character class match returns exactly the matched span
run_sash 'echo $(match "abc123def" "[0-9][0-9][0-9]")'
[[ $rc = 0 && $out = "123" && -z $err ]]; chk 63.1b

# 63.b1 subject and pattern are ordinary args — interpolation is normal
run_sash 's=hello
p=ell
echo $(match "$s" "$p")'
[[ $rc = 0 && $out = "ell" && -z $err ]]; chk 63.b1

# 63.f2 no match → empty whole match, no error (assignment doesn't abort)
run_sash 'x=$(match abc z)
echo "[$x]"'
[[ $rc = 0 && $out = "[]" && -z $err ]]; chk 63.f2

# ── C. match — boolean command form (DD#63 primitive; DD#70 (B)) ──────
# `match S P` is a boolean COMMAND: exit 0 if it matched, nonzero if not.
# It is a (B) must-consume result (DD#70) — it MUST be consumed by a
# condition (if/while/&&/||); a bare `match` or `- match` is a compile
# error.

# 63.1c match in an if-condition: matched → then branch
run_sash 'if match "hello world" "world"; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 63.1c

# 63.1d match in an if-condition: no match → else branch
run_sash 'if match "hello" "zzz"; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk 63.1d

# 63.2a match && action — runs the action only on a match
run_sash 'match "abc123" "[0-9]" && echo hit'
[[ $rc = 0 && $out = "hit" && -z $err ]]; chk 63.2a

# 63.2b match || action — runs the action only on no match
run_sash 'match "abc" "zzz" || echo miss'
[[ $rc = 0 && $out = "miss" && -z $err ]]; chk 63.2b

# 63.2c interpolated subject/pattern in a boolean match
run_sash 's="user@host"
if match "$s" "@"; then echo at; else echo none; fi'
[[ $rc = 0 && $out = "at" && -z $err ]]; chk 63.2c

# 63.2d `match S P || :` explicitly discards the boolean (no match here)
run_sash 'match abc zzz || :
echo done'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk 63.2d

# 63.2e bare `match S P` is a must-consume compile error (DD#70 SB4018)
# 63.2f `- match S P` — `-` on a (B) construct is a compile error (SB4019)
# Native-only: transform diagnostics (the SB40xx must-consume family,
# incl. the existing SB4016) are not yet surfaced on the --emit-c /
# --janet paths. See c-janet-dd55-abort-channel / diagnostic parity.
if in_modes native; then
    run_sash 'match abc abc'
    [[ $rc != 0 && -z $out && $err == *SB4018* ]]; chk 63.2e

    run_sash '- match abc abc'
    [[ $rc != 0 && -z $out && $err == *SB4019* ]]; chk 63.2f
else
    skip_chk 63.2e; skip_chk 63.2f
fi

set -e
tally
