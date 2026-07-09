#!/bin/bash
# spec/regex.sh — DD#63 regex builtin family (match / sub / gsub) + thin sugar.
# Regex is a builtin family taking ordinary string args; quoting/interpolation
# fall out of normal argument evaluation. The s/// and [[ =~ ]] syntaxes are
# thin sugar that desugar to these builtins.
set -eu
. common.sh
setup regex
plan=40
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

# ── D. captures: out-array form `match S P ARR` (DD#63) ───────────────
# `match S P ARR` fills ARR with [whole, g1..gN] (array is the trailing
# positional, like read -a / mapfile) and returns the boolean. Bash-parity
# (DG, D1-B): ARR length = (#capturing groups in pattern)+1; an interior
# group that exists but didn't participate is "" (counted, not skipped).
# Clear-on-entry (D3): each call replaces ARR; no match → ARR is emptied.
# Still a (B) must-consume boolean (DG, D2): a bare 3-arg `match` is an
# error — consume it with if / && / `|| :`.

# 63.cap1 one group: ARR = [whole, g1], length 2 (pre-populated → stale
# entries are cleared on a successful match, not appended)
run_sash 'array g=(old1 old2 old3)
if match "aXXb" "a(X+)b" g; then echo "$(len g) ${g[0]} ${g[1]}"; fi'
[[ $rc = 0 && $out = "2 aXXb XX" && -z $err ]]; chk 63.cap1

# 63.cap2 no groups: ARR = [whole] only, length 1 (bash-parity, NOT 10)
run_sash 'array g=()
if match "abc" "b" g; then echo "$(len g) ${g[0]}"; fi'
[[ $rc = 0 && $out = "1 b" && -z $err ]]; chk 63.cap2

# 63.cap3 three groups
run_sash 'array g=()
if match "2026-06-25" "([0-9]+)-([0-9]+)-([0-9]+)" g; then
    echo "$(len g) ${g[1]} ${g[2]} ${g[3]}"
fi'
[[ $rc = 0 && $out = "4 2026 06 25" && -z $err ]]; chk 63.cap3

# 63.cap4 interior non-participating group: counted, value is ""
run_sash 'array g=()
if match "ac" "(a)(b)?(c)" g; then echo "$(len g) [${g[1]}][${g[2]}][${g[3]}]"; fi'
[[ $rc = 0 && $out = "4 [a][][c]" && -z $err ]]; chk 63.cap4

# 63.cap5 no match → ARR cleared (length 0) even when pre-populated;
# boolean consumed by if/else. Tests clear-on-entry on the no-match path.
run_sash 'array g=(x y z)
if match "abc" "zzz" g; then echo "y ${g[0]}"; else echo "n $(len g)"; fi'
[[ $rc = 0 && $out = "n 0" && -z $err ]]; chk 63.cap5

# 63.cap6 bare 3-arg `match S P ARR` is a must-consume compile error (SB4018)
# 63.cap7 `- match S P ARR` — `-` on a (B) construct is SB4019
# Native-only (transform diagnostics not surfaced on --emit-c/--janet).
if in_modes native; then
    run_sash 'array g=()
match abc abc g'
    [[ $rc != 0 && -z $out && $err == *SB4018* ]]; chk 63.cap6

    run_sash 'array g=()
- match abc abc g'
    [[ $rc != 0 && -z $out && $err == *SB4019* ]]; chk 63.cap7
else
    skip_chk 63.cap6; skip_chk 63.cap7
fi

# ── E. captures: replacement backrefs \0..\9 in sub/gsub (DD#63) ──────
# In a sub/gsub REPLACEMENT, `\0`=whole match, `\1`..`\9`=capture groups,
# `\\`=literal backslash. `\`+any other char is a compile error (SB4033;
# DG, D6). Under DD#64 the replacement is a verbatim string (no
# double-quote collapse): a literal backslash is `"\\"` (two bytes).

# 63.cap8 group reorder via \1 \2
run_sash 'echo $(sub "John Smith" "([A-Za-z]+) ([A-Za-z]+)" "\2 \1")'
[[ $rc = 0 && $out = "Smith John" && -z $err ]]; chk 63.cap8

# 63.cap9 whole-match \0 (per gsub match)
run_sash 'echo $(gsub "a1b2" "[0-9]" "<\0>")'
[[ $rc = 0 && $out = "a<1>b<2>" && -z $err ]]; chk 63.cap9

# 63.cap10 `\\` = one literal backslash (DD#64: source is "\\", two bytes)
run_sash 'echo $(sub "x" "x" "\\")'
[[ $rc = 0 && $out = '\' && -z $err ]]; chk 63.cap10

# 63.cap11 backref to a non-participating group → empty
run_sash 'echo $(sub "ac" "(a)(b)?(c)" "[\2]")'
[[ $rc = 0 && $out = "[]" && -z $err ]]; chk 63.cap11

# 63.cap12 invalid `\`-escape in a replacement (\ + non-digit) → SB4033
run_sash 'echo $(sub x x "\a")'
[[ $rc != 0 && -z $out && $err == *SB4033* ]]; chk 63.cap12

# ── D. sugar interpolation: the s/// fields are ordinary interpolating
#      words, so $var in the pattern or replacement is expanded (DD#63). ──

# 63.d7 `v =~ s/RE/SUB/` mutation sugar interpolates the PATTERN
run_sash 'x=b
v=abc
v =~ s/$x/Y/
echo $v'
[[ $rc = 0 && $out = "aYc" && -z $err ]]; chk 63.d7

# 63.d8 `${v:s/RE/SUB/}` expression sugar interpolates the REPLACEMENT
run_sash 'r=Y
v=abc
echo ${v:s/b/$r/}'
[[ $rc = 0 && $out = "aYc" && -z $err ]]; chk 63.d8

# ── E. `\xnn` hex escape in the regex pattern (vendored engine). ──────
# native,c only: the janet backend uses its own regex, not the vendored
# Plan 9 engine that grows \xnn (see janet regex parity).

# 63.e1 `\x20` matches a literal space
if in_modes native,c; then
    run_sash 'if match "a b" "a\x20b"; then echo y; else echo n; fi'
    [[ $rc = 0 && $out = "y" && -z $err ]]; chk 63.e1
else
    skip_chk 63.e1
fi

# 63.e2 `\x` + a non-hex digit is rejected (not silently literal)
if in_modes native,c; then
    run_sash 'if match "axb" "a\xZZb"; then echo y; else echo n; fi'
    [[ $rc != 0 ]]; chk 63.e2
else
    skip_chk 63.e2
fi

# ── G. `${var:s/RE/SUB/[g]}` brace handling (Path B) + malformed → err ─
# The lexer scans the three `/`-delimited fields, so a bare `{`/`}` inside
# RE/SUB is literal and does NOT close the `${…}`. A malformed `:s`
# payload (wrong delimiter count / unterminated) is a compile error
# (SB4032), never a silent fallback.

# 63.g1 bare `}` in RE is literal (does not close the expansion)
run_sash "var='a}b'
echo \"\${var:s/a}b/z/}\""
[[ $rc = 0 && $out = "z" && -z $err ]]; chk 63.g1

# 63.g2 bare `}` in SUB is literal
run_sash 'var=axb
echo "${var:s/x/}/}"'
[[ $rc = 0 && $out = "a}b" && -z $err ]]; chk 63.g2

# 63.g3 a `{` byte in RE does not close the `${…}` (Path B). The engine
#       requires `\{` for a literal brace (a bare `{` starts an interval);
#       whether bare `{` should be literal is a separate open decision
#       (decide-regex-bare-brace). This still exercises Path B: the `{`
#       byte in `a\{b` must not truncate the expansion.
if in_modes native,c; then
    run_sash "var='a{b'
echo \"\${var:s/a\{b/z/}\""
    [[ $rc = 0 && $out = "z" && -z $err ]]; chk 63.g3
else
    skip_chk 63.g3
fi

# 63.g4 literal text after a valid close concatenates (NOT an error)
run_sash 'var=axb
echo "${var:s/x/z/}tail"'
[[ $rc = 0 && $out = "azbtail" && -z $err ]]; chk 63.g4

# 63.g5 unterminated `:s` payload → SB4032 (not a silent $var + literal)
run_sash 'var=ab
echo "${var:s/a}"'
[[ $rc != 0 && -z $out && $err == *SB4032* ]]; chk 63.g5

# 63.g6 the delimiter `/` itself, escaped as `\x2f`, is a literal slash
if in_modes native,c; then
    run_sash "var='a/b'
echo \"\${var:s/a\x2fb/z/}\""
    [[ $rc = 0 && $out = "z" && -z $err ]]; chk 63.g6
else
    skip_chk 63.g6
fi

# 63.g7 no-brace baseline unchanged (Path B scans the 3 fields identically)
run_sash 'x="hello world"
echo "${x:s/world/earth/}"'
[[ $rc = 0 && $out = "hello earth" && -z $err ]]; chk 63.g7

# 63.g8 missing the mandatory trailing `/` (only two delimiters) → SB4032
run_sash 'var=ab
echo "${var:s/a/b}"'
[[ $rc != 0 && -z $out && $err == *SB4032* ]]; chk 63.g8

set -e
tally
