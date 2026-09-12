#!/bin/bash
# spec/regex.sh — DD#63 regex builtin family (match / sub / gsub) + thin sugar.
# Regex is a builtin family taking ordinary string args; quoting/interpolation
# fall out of normal argument evaluation. The s/// and [[ =~ ]] syntaxes are
# thin sugar that desugar to these builtins.
set -eu
. common.sh
setup regex
plan=68
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
# 63.2f `- match S P` — `-` DISCARDS the result (DD#70 addendum 2026-09-09).
# `-` used to be rejected here on the grounds that a (B) construct has no
# runtime verdict to suppress; but `|| :` was always allowed, so the rule only
# dictated WHICH explicit discard you had to write, and mandated the oblique
# one. SB4018 still catches the SILENT discard, which is the real hazard.
# native+c: the SB40xx must-consume diagnostics are transform-time (before
# codegen), so they surface on --emit-c too. janet deferred.
run_sash 'match abc abc'
[[ $rc != 0 && -z $out && $err == *SB4018* ]]; chk 63.2e

run_sash '- match abc abc
echo after'
[[ $rc = 0 && $out = "after" && -z $err ]]; chk 63.2f

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
# 63.cap7 `- match S P ARR` — `-` discards the boolean and KEEPS the array,
# which is the point: the out-array form is side-effect-primary
# Native-only (transform diagnostics not surfaced on --emit-c/--janet).
run_sash 'array g=()
match abc abc g'
[[ $rc != 0 && -z $out && $err == *SB4018* ]]; chk 63.cap6

run_sash 'array g=()
- match abc b g
echo "$(len g):${g[0]}"'
[[ $rc = 0 && $out = "1:b" && -z $err ]]; chk 63.cap7

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
run_sash 'if match "a b" "a\x20b"; then echo y; else echo n; fi'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 63.e1

# 63.e2 `\x` + a non-hex digit is rejected (not silently literal)
run_sash 'if match "axb" "a\xZZb"; then echo y; else echo n; fi'
[[ $rc != 0 ]]; chk 63.e2

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

# 63.g3 a `{` byte in RE does not close the `${…}` (Path B). A literal brace
#       needs `\{` (a bare `{` opens a `{n,m}` interval) — DD#39 ruling
#       (2026-07-08): `{`/`[` require their escape for a literal, bash-faithful
#       (glibc regcomp errors on a bare `{` too). This exercises Path B: the
#       `{` byte in `a\{b` must not truncate the expansion.
run_sash "var='a{b'
echo \"\${var:s/a\{b/z/}\""
[[ $rc = 0 && $out = "z" && -z $err ]]; chk 63.g3

# 63.g4 literal text after a valid close concatenates (NOT an error)
run_sash 'var=axb
echo "${var:s/x/z/}tail"'
[[ $rc = 0 && $out = "azbtail" && -z $err ]]; chk 63.g4

# 63.g5 unterminated `:s` payload → SB4032 (not a silent $var + literal)
run_sash 'var=ab
echo "${var:s/a}"'
[[ $rc != 0 && -z $out && $err == *SB4032* ]]; chk 63.g5

# 63.g6 the delimiter `/` itself, escaped as `\x2f`, is a literal slash
run_sash "var='a/b'
echo \"\${var:s/a\x2fb/z/}\""
[[ $rc = 0 && $out = "z" && -z $err ]]; chk 63.g6

# 63.g7 no-brace baseline unchanged (Path B scans the 3 fields identically)
run_sash 'x="hello world"
echo "${x:s/world/earth/}"'
[[ $rc = 0 && $out = "hello earth" && -z $err ]]; chk 63.g7

# 63.g8 missing the mandatory trailing `/` (only two delimiters) → SB4032
run_sash 'var=ab
echo "${var:s/a/b}"'
[[ $rc != 0 && -z $out && $err == *SB4032* ]]; chk 63.g8

# ── H. a command argument that *looks* like sed is NOT a substitution ──
# Regression pin: the retired `try_regex_substitution` hijacked any command
# whose first arg matched `s/RE/SUB/[g]` (it desugared from the old `var =~ s/…`
# encoding). `/bin/echo s/a/b/` printed nothing. The `:s`/`=~` sugars now
# desugar at parse to `$(sub|gsub …)`, so a bare command arg is just a literal.

# 63.h1 `s/a/b/` as a command arg is a literal word, not a substitution
run_sash '/bin/echo s/a/b/'
[[ $rc = 0 && $out = "s/a/b/" && -z $err ]]; chk 63.h1

# 63.h2 the global-flag shape `s/x/y/g` is likewise a literal arg
run_sash '/bin/echo s/x/y/g'
[[ $rc = 0 && $out = "s/x/y/g" && -z $err ]]; chk 63.h2

# 63.h3 an UNRESOLVABLE command with a lone `s/…` arg still resolves normally →
# SB5011 (regression pin: the retired `is_regex_mutation` guard used to exempt
# any single-`s/`-arg command from DD#23 resolution, so `nosuchcmd s/a/b/`
# compiled and then hit a runtime `bug: bare command` abort instead).
run_sash 'nosuchcmd_xyz s/a/b/'
[[ $rc != 0 && $err == *SB5011* ]]; chk 63.h3


# ── E. `match` arity is enforced (bug: it was not) ────────────────────────
# `match S P ARR EXTRA` used to compile, report SUCCESS, and fill no array —
# so an `if` took the THEN branch on a match that never happened. The
# 3-arg out-array lowering simply declined the shape and the 2-arg boolean
# lowering picked it up, ignoring the surplus.

# 63.ar1 four arguments
run_sash 'array g=()
array h=()
if match "a=1" "(a)" g h; then echo "${g[0]}${h[0]}"; fi'
[[ $rc != 0 && -z $out && -n $err ]]; chk 63.ar1

# 63.ar2 five arguments
run_sash 'array g=()
if match "a=1" "(a)" g x y; then echo "${g[0]}"; fi'
[[ $rc != 0 && -z $out && -n $err ]]; chk 63.ar2

# 63.ar3 a mistaken flag is surplus, not silently swallowed — this is how the
# bug surfaced (probing `match … -g` for a global form looked like it worked)
run_sash 'array g=()
if match "a=1 b=2" "([a-z])=([0-9])" g -g; then echo "${g[1]}"; fi'
[[ $rc != 0 && -z $out && -n $err ]]; chk 63.ar3

# 63.ar4 one argument (no pattern)
run_sash 'if match "a=1"; then echo y; fi'
[[ $rc != 0 && -z $out && -n $err ]]; chk 63.ar4

# ── F. `matchall S P ARR` — every match, not just the first (DD#63) ───────
# The rule that lets ONE builtin serve both shapes: append each match's
# CAPTURE GROUPS, or the WHOLE match when the pattern has no groups.
#   no groups  -> the matched spans      (the `mapfile -t` shape)
#   N groups   -> g1..gN per match, flat (the `mapfile -m` shape, which lands
#                 exactly on the pairwise array `map m = @{a:DISP}` consumes)
# Unlike `match`, the whole match is NOT prepended when there are groups —
# it would break the pairing. Boolean: exit 0 when at least one match.

# 63.ma1 no groups → the whole matched spans
run_sash 'array a=()
if matchall "x
y
z" "[^\x0a]+" a; then echo "$(len a):${a[0]}:${a[2]}"; fi'
[[ $rc = 0 && $out = "3:x:z" && -z $err ]]; chk 63.ma1

# 63.ma2 groups → captures flattened, whole match omitted
run_sash 'array a=()
if matchall "k1=v1
k2=v2" "([^=\x0a]*)=([^\x0a]*)" a; then echo "$(len a):${a[0]}:${a[1]}:${a[2]}:${a[3]}"; fi'
[[ $rc = 0 && $out = "4:k1:v1:k2:v2" && -z $err ]]; chk 63.ma2

# 63.ma3 the `mapfile -m` replacement end to end. Note the value keeps its
# own `=` (only the FIRST is a separator), which the NUL-substitution route
# could not express at all. native,c: janet does not emit `map m = @{a:DISP}`
# at all (task `janet-map-from-array`).
run_sash 'array pairs=()
matchall "name=Alice
key=val=ue" "([^=\x0a]*)=([^\x0a]*)" pairs || :
map m = @{pairs:~}
echo "$(len m):${m[name]}:${m[key]}"'
[[ $rc = 0 && $out = "2:Alice:val=ue" && -z $err ]]; chk 63.ma3

# 63.ma4 no match → array cleared, non-zero (clear-on-entry like `match`)
run_sash 'array a=(x y z)
if matchall "abc" "zzz" a; then echo "y"; else echo "n $(len a)"; fi'
[[ $rc = 0 && $out = "n 0" && -z $err ]]; chk 63.ma4

# 63.ma5 a single match behaves like the plural case, not like `match`
run_sash 'array a=()
if matchall "abc" "b" a; then echo "$(len a):${a[0]}"; fi'
[[ $rc = 0 && $out = "1:b" && -z $err ]]; chk 63.ma5

# 63.ma6 the target auto-declares (argv / read / read precedent). native,c:
# janet emits the `var` inside the `||` block so the later read cannot see it
# — generic to every auto-declaring builtin there, not to `matchall` (`argv 1
# fresh || :` fails identically); task `janet-autodeclare-scoped-to-list-block`.
run_sash 'matchall "a b" "[a-z]" fresh || :
echo "$(len fresh)"'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk 63.ma6

# 63.ma7 the out-array is REQUIRED — there is no boolean/value 2-arg form,
# because "all matches" has no single value to be
run_sash 'if matchall "abc" "b"; then echo y; fi'
[[ $rc != 0 && -z $out && -n $err ]]; chk 63.ma7

# 63.ma8 …and surplus arguments are rejected, as for `match`
run_sash 'array a=()
if matchall "abc" "b" a extra; then echo y; fi'
[[ $rc != 0 && -z $out && -n $err ]]; chk 63.ma8

# 63.ma9 a tainted subject taints the result (it is external data reshaped,
# not declassified — same as `match`)
run_sash 'h=$(getenv HOME)
matchall "$h" "[a-z]+" parts || :
echo "${parts[0]}"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 63.ma9

# 63.ma10 the target is a BINDING, not a value (SB5063). A quoted target
# silently wrote nowhere and left the array's previous contents in place.
run_sash 'array out=(old)
if matchall "abc" "[a-c]" "out"; then echo "n=$(len out)"; fi'
[[ $rc != 0 && -z $out && $err == *SB5063* ]]; chk 63.ma10

# 63.ma11 `match S P ARR` has the same binding target, and a quoted one ICEd
# the compiler ("ICE: undefined array") rather than diagnosing.
run_sash 'array o=(old)
if match "abc" "(b)" "o"; then echo "n=$(len o)"; fi'
[[ $rc != 0 && -z $out && $err == *SB5063* ]]; chk 63.ma11

# 63.ma12 the two-arg boolean `match S P` has no target — still accepted, and
# a wrong `matchall` arity stays SB4045's to report, not SB5063's.
run_sash 'if match "abc" "b"; then echo y; fi'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 63.ma12

# 63.ma13 `matchall S P` (missing out-array) keeps its own arity diagnostic
run_sash 'if matchall "abc" "[a-c]"; then echo y; fi'
[[ $rc != 0 && -z $out && $err == *SB4045* ]]; chk 63.ma13


# ── DD#70 addendum (2026-09-09): `-` discards ANY result ──────────────────
# `- X` uniformly means "run X, discard its result", whether X is an opaque
# external (A) or in-process logic (B). The old split forbade `-` on (B) and
# required `|| :` — but both are explicit discards, so the rule protected
# nothing and merely mandated the more indirect spelling. This matters most
# for the side-effect-primary forms, where the boolean is incidental and the
# ARRAY is the point.

# 63.dash1 the DD#99 mapfile line, in its preferred spelling. native,c: janet
# scopes an auto-declared collection's `var` to the enclosing block
# (task `janet-autodeclare-scoped-to-list-block`, not specific to `-`).
run_sash '- matchall "a
b" "[^\x0a]+" arr
echo "$(len arr):${arr[1]}"'
[[ $rc = 0 && $out = "2:b" && -z $err ]]; chk 63.dash1

# 63.dash2 `-` on a NO-match matchall still proceeds (nothing to abort on —
# a false boolean is not a failure, DD#70)
run_sash '- matchall "abc" "zzz" arr
echo "n=$(len arr)"'
[[ $rc = 0 && $out = "n=0" && -z $err ]]; chk 63.dash2

# 63.dash3 bare (unmarked) is STILL an error — the protection that matters is
# against the SILENT discard, and `-` is not silent
run_sash 'matchall "a" "a" arr
echo "$(len arr)"'
[[ $rc != 0 && -z $out && $err == *SB4018* ]]; chk 63.dash3

# --- DD#63: the out-array form auto-declares, for `match` as for `matchall` ---
# `match S P ARR` fills ARR, so it introduces it — the same bulk-fill
# precedent as `matchall` / `read` / `argv`. It used to require a prior
# `array g=()`, and without one SB5039 claimed `g` was "introduced nowhere".

# 63.mi1-63.mi3 are native,c: janet emits the auto-declaring `var` INSIDE the
# if-condition expression, so the program does not compile. Pre-existing for
# `matchall`; task `janet-match-outarray-var-in-condition`.
# 63.mi1 `match` auto-declares its out-array — `len` resolves with no `array`
run_sash 'if match abc "(a)(b)" g { echo hit }
echo "n=$(len g)"'
[[ $rc = 0 && $out = "hit
n=3" && -z $err ]]; chk 63.mi1

# 63.mi2 the auto-declared array iterates (whole match + each group)
run_sash 'if match abc "(a)(b)" g { - echo hit }
for x in @g { echo "x=$x" }'
[[ $rc = 0 && $out = "hit
x=ab
x=a
x=b" && -z $err ]]; chk 63.mi2

# 63.mi3 on NO match the array still exists, empty — the declaration is
# static, not conditional on the match succeeding
run_sash 'if match qq "(z)" g { echo yes } else { echo no }
echo "n=$(len g)"'
[[ $rc = 0 && $out = "no
n=0" && -z $err ]]; chk 63.mi3

# 63.mi4 a PRE-declared array is not double-declared — auto-declare is a
# no-op when the name is already a collection
run_sash 'array g=(x y z)
if match ab "(a)" g { - echo hit }
echo "n=$(len g)"'
[[ $rc = 0 && $out = "hit
n=2" && -z $err ]]; chk 63.mi4

# 63.mi5 the two-arg boolean `match S P` still declares NOTHING — the
# pattern is not a binding, so a later read of it is SB5039
run_sash 'if match abc "a" { - echo hit }
echo "$(len a)"'
[[ $rc != 0 && -z $out && $err == *SB5039* ]]; chk 63.mi5

set -e
tally
