#!/bin/bash
# spec/regex.sh — DD#63 regex builtin family (match / sub / gsub) + thin sugar.
# Regex is a builtin family taking ordinary string args; quoting/interpolation
# fall out of normal argument evaluation. The s/// and [[ =~ ]] syntaxes are
# thin sugar that desugar to these builtins.
set -eu
. common.sh
setup regex
plan=321
echo "1..$plan" >&3

set +e

# ── A. sub / gsub function forms (primitives) ────────────────────────

# sub replaces the FIRST match; discard only its boolean (formerly 63.4).
run_sash 'echo $(- sub abab "b" "B")'
[[ $rc = 0 && $out = "aBab" && -z $err ]]; chk migratea.65

# gsub replaces ALL matches (formerly 63.5).
run_sash 'echo $(- gsub abab "b" "B")'
[[ $rc = 0 && $out = "aBaB" && -z $err ]]; chk migratea.66

# A literal slash is just an argument byte, not a delimiter (formerly 63.b3).
run_sash 'echo $(- sub "a/b/c" "/" "_")'
[[ $rc = 0 && $out = "a_b/c" && -z $err ]]; chk migratea.67

# An empty replacement deletes the match (formerly 63.f1).
run_sash 'echo $(- sub abc "b" "")'
[[ $rc = 0 && $out = "ac" && -z $err ]]; chk migratea.68

# ── B. match — value form $(match S P) = whole match (resub[0]) ───────

# A scalar match capture returns the whole match (formerly 63.1a).
run_sash 'echo $(- match "hello world" "world")'
[[ $rc = 0 && $out = "world" && -z $err ]]; chk migratea.69

# A character class returns exactly the matched span (formerly 63.1b).
run_sash 'echo $(- match "abc123def" "[0-9][0-9][0-9]")'
[[ $rc = 0 && $out = "123" && -z $err ]]; chk migratea.70

# Interpolation preserves a validated immutable pattern (formerly regexproof.41).
run_sash 's=hello
const p=ell
attest p r?
echo $(- match "$s" "$p")'
[[ $rc = 0 && $out = "ell" && -z $err ]]; chk migratea.71

# Discarding no-match retains its empty text without aborting (formerly 63.f2).
run_sash 'x=$(- match abc z)
echo "[$x]"'
[[ $rc = 0 && $out = "[]" && -z $err ]]; chk migratea.72

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
# The SB40xx must-consume diagnostics are transform-time (before codegen), so
# they surface on --emit-c too.
run_sash 'match abc abc'
[[ $rc != 0 && -z $out && $err == *SB4018* ]]; chk 63.2e

run_sash '- match abc abc
echo after'
[[ $rc = 0 && $out = "after" && -z $err ]]; chk 63.2f

# ── D. captures: out-array form `match S P ARR` (DD#63) ───────────────
# `match S P ARR` appends [whole, g1..gN] and returns the boolean.
# Batch length = (#capturing groups in pattern)+1; an interior
# group that exists but didn't participate is "" (counted, not skipped).
# DD131: existing entries survive; no match appends nothing.
# Still a (B) must-consume boolean (DG, D2): a bare 3-arg `match` is an
# error — consume it with if / && / `|| :`.

# One group appends the whole match and capture after existing entries.
run_sash 'array g=(old1 old2 old3)
if match "aXXb" "a(X+)b" g; then echo "$(len g) $(join g ,)"; fi'
[[ $rc = 0 && $out = "5 old1,old2,old3,aXXb,XX" && -z $err ]]; chk fillmig.2

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

# No match retains the pre-populated array and returns false.
run_sash 'array g=(x y z)
if match "abc" "zzz" g; then echo "y ${g[0]}"; else echo "n $(len g)"; fi'
[[ $rc = 0 && $out = "n 3" && -z $err ]]; chk fillmig.3

# 63.cap6 bare 3-arg `match S P ARR` is a must-consume compile error (SB4018)
# 63.cap7 `- match S P ARR` — `-` discards the boolean and KEEPS the array,
# which is the point: the out-array form is side-effect-primary
# Native-only (transform diagnostics not surfaced on --emit-c).
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

# Group reorder via \1 \2 (formerly 63.cap8).
run_sash 'echo $(- sub "John Smith" "([A-Za-z]+) ([A-Za-z]+)" "\2 \1")'
[[ $rc = 0 && $out = "Smith John" && -z $err ]]; chk migratea.73

# Whole-match \0 (per gsub match, formerly 63.cap9).
run_sash 'echo $(- gsub "a1b2" "[0-9]" "<\0>")'
[[ $rc = 0 && $out = "a<1>b<2>" && -z $err ]]; chk migratea.74

# `\\` = one literal backslash (source is two bytes, formerly 63.cap10).
run_sash 'echo $(- sub "x" "x" "\\")'
[[ $rc = 0 && $out = '\' && -z $err ]]; chk migratea.75

# Backref to a non-participating group is empty (formerly 63.cap11).
run_sash 'echo $(- sub "ac" "(a)(b)?(c)" "[\2]")'
[[ $rc = 0 && $out = "[]" && -z $err ]]; chk migratea.76

# 63.cap12 invalid `\`-escape in a replacement (\ + non-digit) → SB4033
run_sash 'echo $(sub x x "\a")'
[[ $rc != 0 && -z $out && $err == *SB4033* ]]; chk 63.cap12

# ── D. sugar interpolation: the s/// fields are ordinary interpolating
#      words, so $var in the pattern or replacement is expanded (DD#63). ──

# 63.d7 `v =~ s/RE/SUB/` mutation sugar interpolates the PATTERN
run_sash 'const x=b
attest x r?
v=abc
v =~ s/$x/Y/
echo $v'
[[ $rc = 0 && $out = "aYc" && -z $err ]]; chk regexproof.42

# 63.d8 `${v:s/RE/SUB/}` expression sugar interpolates the REPLACEMENT
run_sash 'r=Y
v=abc
echo ${v:s/b/$r/}'
[[ $rc = 0 && $out = "aYc" && -z $err ]]; chk 63.d8

# ── E. `\xnn` hex escape in the regex pattern (vendored engine). ──────

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
# could not express at all.
run_sash 'array pairs=()
matchall "name=Alice
key=val=ue" "([^=\x0a]*)=([^\x0a]*)" pairs || :
map m = @{pairs:~}
echo "$(len m):${m[name]}:${m[key]}"'
[[ $rc = 0 && $out = "2:Alice:val=ue" && -z $err ]]; chk 63.ma3

# No match retains the array and returns false, like `match`.
run_sash 'array a=(x y z)
if matchall "abc" "zzz" a; then echo "y"; else echo "n $(len a)"; fi'
[[ $rc = 0 && $out = "n 3" && -z $err ]]; chk fillmig.4

# 63.ma5 a single match behaves like the plural case, not like `match`
run_sash 'array a=()
if matchall "abc" "b" a; then echo "$(len a):${a[0]}"; fi'
[[ $rc = 0 && $out = "1:b" && -z $err ]]; chk 63.ma5

# 63.ma6 the target auto-declares (argv / read / read precedent).
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

# 63.dash1 the DD#99 mapfile line, in its preferred spelling.
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
n=5" && -z $err ]]; chk fillmig.5

# 63.mi5 the two-arg boolean `match S P` still declares NOTHING — the
# pattern is not a binding, so a later read of it is SB5039
run_sash 'if match abc "a" { - echo hit }
echo "$(len a)"'
[[ $rc != 0 && -z $out && $err == *SB5039* ]]; chk 63.mi5

# ── 39.20-39.31 DD#39 counted repetition: the instruction buffer ──────────
#
# `regcomp1` sized its instruction buffer at 6 slots per pattern CHARACTER —
# Pike's budget, correct for every operator he wrote. `{n,m}` is a sash
# addition and expands by the BOUND, not the length: `a{1,32}` is seven
# characters that emit thirty-two copies of the preceding atom. Past
# `a{1,16}` the expansion wrote off the end of the allocation.
#
# What that looked like: the compiler died in glibc malloc; a binary built
# with a smaller bound and handed a larger one at RUNTIME answered `a{1,17}`
# with "no match" against a string of 34 a's, or was killed outright. A
# pattern can come from `$(getenv …)`, so the input was not even trusted.

# 39.20 the first bound past the old budget
run_sash 's=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
if [[ $s =~ a{1,17} ]]; then echo Y; else echo N; fi'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk 39.20

# 39.21 the largest bound parsecounted accepts
run_sash 's=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
if [[ $s =~ a{1,32} ]]; then echo Y; else echo N; fi'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk 39.21

# 39.22 a non-zero minimum takes the other expansion branch
run_sash 's=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
if [[ $s =~ a{2,32} ]]; then echo Y; else echo N; fi'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk 39.22

# 39.23 a zero minimum takes the third (QUEST-the-original) branch
run_sash 's=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
if [[ $s =~ a{0,32} ]]; then echo Y; else echo N; fi'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk 39.23

# 39.24 exact count at the cap
run_sash 's=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
if [[ $s =~ ^a{32} ]]; then echo Y; else echo N; fi'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk 39.24

# 39.25 the cap itself still rejects, and says so
run_sash 's=aaa
if [[ $s =~ a{33} ]]; then echo Y; else echo N; fi'
[[ $rc != 0 && -z $out && $err == *'repetition count too large'* ]]; chk 39.25

# 39.26 many repetitions in one pattern — the budget is per-repetition, so a
#       pattern full of them must still compile
run_sash 's=aaa
if [[ $s =~ a{32}a{32}a{32}a{32}a{32}a{32}a{32}a{32} ]]; then echo Y; else echo N; fi'
[[ $rc = 0 && $out = "N" && -z $err ]]; chk 39.26

# A validated runtime pattern with a large bound (formerly 39.27).
#       This is the one that matters:
#       the pattern is tainted input, and before the fix it silently answered
#       the wrong question or killed the process.
export RE='a{1,32}'
run_sash 'const p=$(getenv RE)
attest p [[ -n $p && r? $p ]]
s=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
if [[ $s =~ $p ]]; then echo Y; else echo N; fi'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk regexproof.43
unset RE

# ── 39.28-39.31 counted repetition of a group is rejected, not mis-answered ──
#
# The re-lex reads exactly ONE token per copy, so after a group
# `prev_atom_start` pointed at the group's LAST atom: `(ab){2}` repeated `b`
# and built `(ab)b`. That reported no match on "ababababab", where bash
# matches — a wrong answer with no diagnostic. Whether to support it (which
# means replaying a token span) is a language-surface question; until then it
# must fail loudly. See notes/tasks/regex-counted-group.md.

# 39.28 the reported shape
run_sash 's=ababababab
if [[ $s =~ (ab){2} ]]; then echo Y; else echo N; fi'
[[ $rc != 0 && -z $out && $err == *'group is not supported'* ]]; chk 39.28

# 39.29 an alternation group too
run_sash 's=ababababab
if [[ $s =~ (a|b){4} ]]; then echo Y; else echo N; fi'
[[ $rc != 0 && -z $out && $err == *'group is not supported'* ]]; chk 39.29

# 39.30 a BRACKET class is a single token and keeps working — the reject is
#       scoped to `( )`, not to "anything before a brace"
run_sash 's=ababababab
if [[ $s =~ ^[ab]{4} ]]; then echo Y; else echo N; fi'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk 39.30

# 39.31 a group NOT followed by a count is unaffected
run_sash 's=ababababab
if [[ $s =~ (ab)(ab) ]]; then echo Y; else echo N; fi'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk 39.31

# ── a literal `((` in an unquoted pattern ────────────────────
# The lexer takes `((` as the arithmetic opener, so an unquoted pattern ends
# there. That is a real restriction; it used to be reported as "expected `]]`",
# which names a delimiter the input does not lack.

# 39.32 the diagnostic names the actual cause, not a missing `]]`
run_sash 's=abc
if [[ $s =~ ^((a))$ ]]; then echo Y; else echo N; fi'
[[ $rc != 0 && $err == *'SB4014'* && $err == *'arithmetic opener'* \
   && $err != *"expected \`]]\`"* ]]; chk 39.32

# 39.33 the trigger is adjacency, not nesting depth — this compiles
run_sash 's=ab
if [[ $s =~ ^(a(b))$ ]]; then echo Y; else echo N; fi'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk 39.33

# 39.34 the quoted spelling of the same pattern is accepted and matches
run_sash "s=a
if [[ \$s =~ '^((a))\$' ]]; then echo Y; else echo N; fi"
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk 39.34

# 39.35 the dispatch form reports the same cause rather than a parse cascade
run_sash 's=abc
[[ $s =~ ^((a)) { echo HIT } || { echo MISS } ]]'
[[ $rc != 0 && $err == *'SB4014'* && $err == *'arithmetic opener'* ]]; chk 39.35

# ── 63.fl*: the substitution flags slot ───────────────────────────────────
# DD#63: "`g` is the `sub`/`gsub` choice, not a flag argument. There are **no
# other flags**" — the vendored Plan 9 engine has no REG_ICASE. The slot was
# read as a bare `flags.contains('g')`, so EVERY other flag was silently
# ignored: `s/a/b/i` promised a case-insensitive substitution and delivered a
# case-sensitive one, and a typo'd `s/a/b/x` did nothing at all (real `sed`
# rejects `x`). Same shape as the tail-junk drop DD#96 fixed — junk accepted
# because no spec pinned the rejection.

# 63.fl1 `i` is the one people will actually reach for, and it was the lie:
#        silently case-SENSITIVE. Rejected, and the message names the remedy.
run_sash 'str x=aAaA
x =~ s/a/b/i
echo "[$x]"'
[[ $rc != 0 && -z $out && $err == *SB4032* && $err == *tolower* ]]; chk 63.fl1

# 63.fl2 a typo'd flag is an error, not a no-op
run_sash 'str x=aAaA
x =~ s/a/b/x
echo "[$x]"'
[[ $rc != 0 && -z $out && $err == *SB4032* && $err == *'`x`'* ]]; chk 63.fl2

# 63.fl3 the `${v:s/…}` expansion form agrees (shared builder)
run_sash 'str x=aAaA
str y=${x:s/a/b/i}
echo "[$y]"'
[[ $rc != 0 && -z $out && $err == *SB4032* ]]; chk 63.fl3

# 63.fl4 exactly `g` — a doubled `gg` is junk, not emphasis
run_sash 'str x=aAaA
x =~ s/a/b/gg
echo "[$x]"'
[[ $rc != 0 && -z $out && $err == *SB4032* ]]; chk 63.fl4

# 63.fl5 …and case matters: `G` is not `g`
run_sash 'str x=aAaA
x =~ s/a/b/G
echo "[$x]"'
[[ $rc != 0 && -z $out && $err == *SB4032* ]]; chk 63.fl5

# 63.fl6 control: bare `g` still selects gsub
run_sash 'str x=aAaA
x =~ s/a/b/g
echo "[$x]"'
[[ $rc = 0 && $out = "[bAbA]" && -z $err ]]; chk 63.fl6

# 63.fl7 control: no flag still selects sub (first match only), and the DD#96
#        chain with a per-edit `g` is unaffected by the new validation
run_sash 'str x=aAaA
x =~ s/a/b/
str v=ab
str w=${v:s/a/1/,s/b/2/g}
echo "[$x][$w]"'
[[ $rc = 0 && $out = "[bAaA][12]" && -z $err ]]; chk 63.fl7

# 63.fl8 the rejection reads as one sentence. Its source lines lost their `\`
#        continuations, so each line break became a run of indentation spaces
#        inside the message (each fragment below spans one of the three).
run_sash 'str x=aAaA
x =~ s/a/b/z
echo "[$x]"'
[[ $rc != 0 && -z $out && $err == *SB4032* && $err == *'(replace every match)'* \
   && $err == *'no case-insensitive mode'* && $err == *'both sides for `i`'* ]]; chk 63.fl8

run_sash 'str subject=abc
if [[ $subject =~ $'"'"'a\x00b'"'"' ]] { echo matched }'
[[ $rc != 0 && -z $out && $err == *SB5023* \
   && $err == *'pattern contains null byte'* ]]; chk rxcompile.1

# ── 39.36-39.56 DD#39 (2026-10-25): every static regex is compiled now ────────
# SB5023 used to cover only a lone top-level `[[ =~ ]]` and the FIRST edit of
# an `x =~ s/…` statement (and skipped any RE containing `$`). Everything else
# compiled and died at runtime with `regexp error`. One constructor serves
# three sites (operand / `[[ =~ ]]` / dispatch pattern), so each spec pins the
# code, the text AND the position.

# 39.36 `sub`'s static regex operand
run_sash 'str x=$(sub abc "(" z)
echo "[$x]"'
[[ $rc != 0 && -z $out \
   && $err == *':1:17: error [SB5023] invalid regex in substitution: missing operand for ('* ]]; chk 39.36

# 39.37 `gsub`'s static regex operand
run_sash 'str x=abc
str y=$(gsub "$x" "[" z)
echo "[$y]"'
[[ $rc != 0 && -z $out \
   && $err == *":2:19: error [SB5023] invalid regex in substitution: malformed '[]'"* ]]; chk 39.37

# 39.38 `match`'s static regex operand, as a condition
run_sash 'str x=abc
if match "$x" "(" { echo y } else { echo n }'
[[ $rc != 0 && -z $out \
   && $err == *':2:15: error [SB5023] invalid regex pattern: missing operand for ('* ]]; chk 39.38

# 39.39 `matchall`'s static regex operand, under `-`
run_sash 'array a=()
- matchall abc "(" a
echo $(len a)'
[[ $rc != 0 && -z $out \
   && $err == *':2:16: error [SB5023] invalid regex pattern: missing operand for ('* ]]; chk 39.39

# 39.40 the `${x:s/RE/SUB/}` sugar desugars to `sub` and is checked with it
run_sash 'str x=abc
echo "${x:s/b)/1/}"'
[[ $rc != 0 && -z $out \
   && $err == *':2:6: error [SB5023] invalid regex in substitution: unmatched right paren'* ]]; chk 39.40

# 39.41 every edit of an expansion chain, here the second (a `gsub`)
run_sash 'str x=abc
echo "${x:s/a/1/,s/c)/2/g}"'
[[ $rc != 0 && -z $out \
   && $err == *':2:6: error [SB5023] invalid regex in substitution: unmatched right paren'* ]]; chk 39.41

# 39.42 every edit of an `x =~ s/…` chain: the old check saw only the first
run_sash 'str x=abc
x =~ s/a/1/,s/c)/2/
echo "$x"'
[[ $rc != 0 && -z $out \
   && $err == *':2:1: error [SB5023] invalid regex in substitution: unmatched right paren'* ]]; chk 39.42

# 39.43 an invalid RE containing a `$` anchor: the old check skipped any `$`
run_sash 'str x=abc
x =~ s/(c$/Z/
echo "$x"'
[[ $rc != 0 && -z $out \
   && $err == *':2:1: error [SB5023] invalid regex in substitution: unmatched left paren'* ]]; chk 39.43

# 39.44 the cell sugar `${a[i]:s/…/}`
run_sash 'array a=(abc)
echo "${a[0]:s/(/x/}"'
[[ $rc != 0 && -z $out \
   && $err == *':2:6: error [SB5023] invalid regex in substitution: missing operand for ('* ]]; chk 39.44

# 39.45 positive: a valid `$`/`^` anchor is static and compiles in every form
run_sash 'str x=abc
array a=(abc)
x =~ s/c$/Z/
echo "$x ${x:s/^a/Q/} ${a[0]:s/^a(b)/Q/}"'
[[ $rc = 0 && $out = "abZ QbZ Qc" && -z $rerr ]]; chk 39.45

# A validated dynamic regex works in both forms (formerly 39.46).
run_sash 'const p=b
attest p r?
str x=abcb
str y=$(- sub "$x" "$p" z)
echo "[$y][${x:s/$p/Q/g}]"'
[[ $rc = 0 && $out = "[azcb][aQcQ]" && -z $rerr ]]; chk migratea.77

# 39.47 a regex dispatch's pattern (DD#66)
run_sash 'str x=abc
[[ $x =~ ( { echo a } || { echo b } ]]'
[[ $rc != 0 && -z $out \
   && $err == *':2:10: error [SB5023] invalid regex pattern: missing operand for ('* ]]; chk 39.47

# 39.48 every arm's pattern, not only the first
run_sash 'str x=abc
[[ $x =~ q { echo one } ( { echo two } || { echo dflt } ]]'
[[ $rc != 0 && -z $out \
   && $err == *':2:25: error [SB5023] invalid regex pattern: missing operand for ('* ]]; chk 39.48

# 39.49 an `=~` under `&&`: the check used to stop at the top-level operator
run_sash 'str x=abc
[[ $x =~ a && $x =~ ( ]] && echo m'
[[ $rc != 0 && -z $out \
   && $err == *':2:1: error [SB5023] invalid regex pattern: missing operand for ('* ]]; chk 39.49

# 39.50 under `||`
run_sash 'str x=abc
[[ $x == q || $x =~ ( ]] && echo m'
[[ $rc != 0 && -z $out \
   && $err == *':2:1: error [SB5023] invalid regex pattern: missing operand for ('* ]]; chk 39.50

# 39.51 under `!`
run_sash 'str x=abc
[[ ! $x =~ ( ]] && echo m'
[[ $rc != 0 && -z $out \
   && $err == *':2:1: error [SB5023] invalid regex pattern: missing operand for ('* ]]; chk 39.51

# 39.52 an attest predicate's `[[ ]]` is checked the same way
run_sash 'for x in $(argv 1); do attest x [[ $x =~ a && $x =~ ( ]]; echo "got:$x"; done' a
[[ $rc != 0 && -z $out \
   && $err == *':1:24: error [SB5023] invalid regex pattern: missing operand for ('* ]]; chk 39.52

# Unchecked malformed candidates cannot reach a consumer (formerly 39.53-.56).
run_sash 'str p="("
str x=$(- sub abc "$p" z)
echo "[$x]"'
[[ $rc != 0 && -z $out && $err == *SB5106* ]]; chk migratea.78

# Through global substitution sugar too.
run_sash 'str p="("
str x=abc
str y=${x:s/$p/z/g}
echo "[$y]"'
[[ $rc != 0 && -z $out && $err == *SB5106* ]]; chk regexproof.47

# Mutation remains subject to the same prerequisite.
run_sash 'str p="("
str x=abc
x =~ s/$p/z/
echo "[$x]"'
[[ $rc != 0 && -z $out && $err == *SB5106* ]]; chk regexproof.48

# Discarding a result does not supply a pattern proof.
run_sash 'str p="("
str x=$(- sub abc "$p" z)
echo "after [$x]"'
[[ $rc != 0 && -z $out && $err == *SB5106* ]]; chk regexproof.49

# Whole-subject anchors must not attest just one acceptable line.
run_sash 'x=$(arg 1)
attest x [[ $x =~ ^[a-z]{1,32}$ ]]
echo "$x"' $'abc\nadmin=true'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'attest: x = '* \
   && $rerr == *'failed the gate'* ]]; chk 39.57

run_sash 'x=$(arg 1)
attest x [[ $x =~ ^[a-z]{1,32}$ ]]
echo "$x"' $'admin=true\nabc'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'attest: x = '* \
   && $rerr == *'failed the gate'* ]]; chk 39.58

# No line boundaries or trailing-newline exception; ordinary search stays unanchored.
run_sash 's="ab
cd"
trailing="ab
"
plain=ab
empty=""
if [[ $s =~ ab$ ]] { echo wrong } else { echo end }
if [[ $s =~ ^cd ]] { echo wrong } else { echo start }
if [[ $trailing =~ ^ab$ ]] { echo wrong } else { echo trailing }
if [[ $plain =~ ^ab$ ]] { echo whole }
if [[ $empty =~ ^$ ]] { echo empty }
if [[ $s =~ cd ]] { echo search }
if [[ $s =~ ^ab.cd$ || $s =~ ^ab[^x]cd$ ]] { echo wrong }
if [[ $s =~ ^ab\x0acd$ ]] { echo newline }'
[[ $rc = 0 && $out = $'end\nstart\ntrailing\nwhole\nempty\nsearch\nnewline' \
   && -z $err && -z $rerr ]]; chk 39.59

# A grouped anchor bypasses the engine's first-instruction fast path.
run_sash 's="ab
cd"
const p="(^cd)"
attest p r?
echo "[$(- match "$s" "$p")]"'
[[ $rc = 0 && $out = '[]' && -z $err && -z $rerr ]]; chk migratea.79

# Whole-subject anchors still return false, retaining the old destination.
run_sash 'array g=(stale)
s="ab
cd"
if match "$s" "(ab$)" g { echo wrong } else { echo "miss $(len g)" }'
[[ $rc = 0 && $out = 'miss 1' && -z $err && -z $rerr ]]; chk fillmig.6

run_sash 's="ab
cd"
[[ $s =~
    ^cd { echo wrong-start }
    ab$ { echo wrong-end }
    ^ab\x0acd$ { echo whole }
    || { echo wrong-default }
]]'
[[ $rc = 0 && $out = whole && -z $err && -z $rerr ]]; chk 39.62

run_sash 's="ab
cd"
first=$(- sub "$s" "(^cd)" X)
all=$(- gsub "$s" "(ab$)" X)
if [[ $first == $s && $all == $s ]] { echo unchanged } else { echo wrong }'
[[ $rc = 0 && $out = unchanged && -z $err && -z $rerr ]]; chk migratea.80

# Advancing a global search does not create another beginning of subject.
run_sash 'x=aaa
echo "$(- gsub "$x" "^a" X) ${x:s/^a/X/g}"
w=a-a-a
w =~ s/^a-/X/g
echo "$w"'
[[ $rc = 0 && $out = $'Xaa Xaa\nXa-a' && -z $err && -z $rerr ]]; chk migratea.81

# Resumed captures and unmatched spans retain original-subject offsets.
run_sash 'echo "$(- gsub abab "(^a)|(b)" "<\1\2>")"'
[[ $rc = 0 && $out = '<a><b>a<b>' && -z $err && -z $rerr ]]; chk migratea.82

run_sash '- matchall aaa "^a" m
echo "$(len m):${m[0]}"'
[[ $rc = 0 && $out = '1:a' && -z $err && -z $rerr ]]; chk 39.66

run_sash '- matchall abab "(^a)|(b)" m
echo "$(len m):[$(join m ,)]"'
[[ $rc = 0 && $out = '6:[a,,,b,,b]' && -z $err && -z $rerr ]]; chk 39.67

# Zero-width anchors still advance, without re-anchoring at each byte.
run_sash 's="ab
cd"
echo "[$(- gsub "$s" "^" ">")]"'
[[ $rc = 0 && $out = $'[>ab\ncd]' && -z $err && -z $rerr ]]; chk migratea.83

run_sash 's="ab
cd"
- matchall "$s" "^" m
echo "$(len m):[${m[0]}]"'
[[ $rc = 0 && $out = '1:[]' && -z $err && -z $rerr ]]; chk 39.69

# The empty subject and terminal position are real match positions.
run_sash 'echo "[$(- sub "" "^$" X)][$(- gsub "" "^$" Y)]"'
[[ $rc = 0 && $out = '[X][Y]' && -z $err && -z $rerr ]]; chk migratea.84

run_sash 'echo "[$(- gsub ab "" ".")]"
echo "$(- gsub a "a|$" X)"
- matchall a "a|$" m
echo "$(len m):[${m[0]}][${m[1]}]"'
[[ $rc = 0 && $out = $'[.a.b.]\nXX\n2:[a][]' \
   && -z $err && -z $rerr ]]; chk migratea.85

# The retained first-NUL boundary also bounds resumed and zero-width searches.
run_sash 's=$'"'"'a\x00b'"'"'
want=$'"'"'aX\x00b'"'"'
got=$(- gsub "$s" "$" X)
if [[ $got == $want ]] { echo retained } else { echo wrong }'
[[ $rc = 0 && $out = retained && -z $err && -z $rerr ]]; chk migratea.86

run_sash 's=$'"'"'a\x00b'"'"'
- matchall "$s" "$" m
echo "$(len m)"
- matchall "$s" "" m
echo "$(len m)"'
[[ $rc = 0 && $out = $'1\n3' && -z $err && -z $rerr ]]; chk fillmig.7

run_sash 's=$'"'"'\x00b'"'"'
want=$'"'"'X\x00b'"'"'
got=$(- gsub "$s" "" X)
if [[ $got == $want ]] { echo retained } else { echo wrong }
- matchall "$s" "" m
echo "$(len m)"'
[[ $rc = 0 && $out = $'retained\n1' && -z $err && -z $rerr ]]; chk migratea.87

# The still-built standalone helper must use the same subject/cursor contract.
run_sash "sash-regex captures-all aaa '^a' | /usr/bin/od -An -tx1
sash-regex captures-all abab '(^a)|(b)' | /usr/bin/od -An -tx1"
[[ $rc = 0 && $out = $' 61\n 61 00 00 00 62 00 00 62' \
   && -z $err && -z $rerr ]]; chk 39.75

run_sash "sash-regex gsub aaa '^a' X
echo .
sash-regex gsub abab '(^a)|(b)' '<\1\2>'
echo ."
[[ $rc = 0 && $out = $'Xaa.\n<a><b>a<b>.' \
   && -z $err && -z $rerr ]]; chk 39.76

run_sash "sash-regex sub '' '^$' X
echo .
sash-regex gsub '' '^$' Y
echo .
sash-regex gsub ab '' .
echo /
sash-regex gsub a 'a|$' X
echo ."
[[ $rc = 0 && $out = $'X.\nY.\n.a.b./\nXX.' \
   && -z $err && -z $rerr ]]; chk 39.77

# DD132 trial: validity is a quiet, counted, actual-engine predicate.
run_sash 'if [[ r? "" && r? "a{32}" && r? "\x00" && r? "\\" && r? "\q" ]] { echo valid }'
[[ $rc = 0 && $out = valid && -z $err && -z $rerr ]]; chk regexvalid.1

run_sash 'if [[ r? "(" ]] { echo wrong } else { echo invalid }'
[[ $rc = 0 && $out = invalid && -z $err && -z $rerr ]]; chk regexvalid.2

run_sash 'int accepted=0; int rejected=0
for p in "" "a{1,32}" "\d+" "(" "[" "\" "\x" "a{33}" "(ab){2}" "[\D]" "a{5,3}" "a{4294967297}" "a{1,4294967297}" {
    if [[ r? $p ]] { (( accepted += 1 )) } else { (( rejected += 1 )) }
}
echo "$accepted:$rejected"'
[[ $rc = 0 && $out = 3:10 && -z $err && -z $rerr ]]; chk regexvalid.3

run_sash "int rejected=0
for p in \$'\\x00a' \$'a\\x00b' \$'a\\x00' {
    if [[ r? \$p ]] { echo wrong } else { (( rejected += 1 )) }
}
echo \$rejected"
[[ $rc = 0 && $out = 3 && -z $err && -z $rerr ]]; chk regexvalid.4

run_sash 'str text="a("; str prefix=$(- match "$text" a)
if [[ r? $prefix ]] { echo prefix } else { echo wrong }
if [[ r? $text ]] { echo wrong } else { echo whole }'
[[ $rc = 0 && $out = $'prefix\nwhole' && -z $err && -z $rerr ]]; chk migratea.88

run_sash 'const p=$(arg 1); attest p r?; echo $p' '[a-z]+'
[[ $rc = 0 && $out = '[a-z]+' && -z $err && -z $rerr ]]; chk regexvalid.6

run_sash '_p=$(arg 1); attest _p [[ r? $_p ]]; echo $_p' '^x$'
[[ $rc = 0 && $out = '^x$' && -z $err && -z $rerr ]]; chk regexvalid.7

run_sash '_p=$(arg 1); attest _p r?; echo after' '('
[[ $rc = 1 && -z $out && -z $err && $rerr == *"attest: _p = '(' failed the gate"* \
   && $rerr != *'regexp error'* ]]; chk regexvalid.8

run_sash 'p=$(arg 1)
if attest p r? { echo $p } else { p=fallback; echo $p }' '('
[[ $rc = 0 && $out = fallback && -z $err && -z $rerr ]]; chk regexvalid.9

run_sash 'argv 1 _patterns; attest p in _patterns r?; echo "$(join _patterns ,)"' '[a-z]+' '^x$'
[[ $rc = 0 && $out = '[a-z]+,^x$' && -z $err && -z $rerr ]]; chk regexvalid.10

run_sash 'map _patterns=([a]=b [+]=good)
if attest k v in _patterns r? { echo wrong } else { echo invalid }'
[[ $rc = 0 && $out = invalid && -z $err && -z $rerr ]]; chk regexvalid.11

run_sash 'argv 1 _patterns; attest p in _patterns r?; echo after' a '(' b
[[ $rc = 1 && -z $out && -z $err && $rerr == *'failed the gate'* \
   && $rerr != *'regexp error'* ]]; chk regexvalid.12

run_sash 'p=$(arg 1); if [[ r? ${p:!} ]] { echo $p }' a
[[ $rc != 0 && $err == *SB5020* ]]; chk regexvalid.13

run_sash 'int n=1; if [[ r? $n ]] { echo wrong }'
[[ $rc != 0 && $err == *SB4030* ]]; chk regexvalid.14

SASH_CFLAGS=--heap-size=16K run_sash 'str choice=yes
if [[ $choice == no && r? a ]] { echo wrong } else { echo and }
if [[ $choice == yes || r? a ]] { echo or }'
[[ $rc = 0 && $out = $'and\nor' && -z $err && -z $rerr ]]; chk regexvalid.15

SASH_CFLAGS=--heap-size=64K run_sash 'int i=0
while (( i < 500 )) {
    if [[ r? "[a-z]{1,3}" && ! r? "(" && r? "" ]] { (( i += 1 )) } else { fail 7 }
}
echo $i'
[[ $rc = 0 && $out = 500 && -z $err && -z $rerr ]]; chk regexvalid.16

SASH_CFLAGS=--heap-size=16K run_sash 'ERR { - echo fatal }
if [[ r? a ]] { echo valid } else { echo invalid }
echo after'
[[ $rc = 1 && $out = fatal && -z $err && $rerr == *'out of memory'* \
   && $rerr != *'regexp error'* ]]; chk regexvalid.17

SASH_CFLAGS=--heap-size=16K run_sash 'ERR { - echo fatal }
if ! [[ r? a ]] { echo invalid } else { echo valid }
echo after'
[[ $rc = 1 && $out = fatal && -z $err && $rerr == *'out of memory'* \
   && $rerr != *'regexp error'* ]]; chk regexvalid.18

SASH_CFLAGS=--heap-size=16K run_sash 'ERR { - echo fatal }
fn candidate -> int { echo entered >&2; return 1 }
- [[ r? "$(candidate)" ]]
echo after'
[[ $rc = 1 && $out = fatal && -z $err && $rerr == *entered* \
   && $rerr == *'out of memory'* && $rerr != *'regexp error'* ]]; chk regexvalid.19

printf -v _regex_long '%20000s' ''
_regex_long=${_regex_long// /a}
SASH_CFLAGS=--heap-size=16K run_sash "ERR { - echo fatal }
if [[ r? \"$_regex_long\" ]] { echo valid } else { echo invalid }"
[[ $rc = 1 && $out = fatal && -z $err \
   && $rerr == *'out of memory (requested 20001 bytes)'* ]]; chk regexvalid.20

# These were accepted by both static validation and runtime compilation.
run_sash 'str s=abc; if [[ $s =~ "\" ]] { echo wrong }'
[[ $rc != 0 && $err == *SB5023* && $err == *'trailing backslash in regex'* ]]; chk regexvalid.21

run_sash 'str s=a; if [[ $s =~ "^a{4294967297}$" ]] { echo wrong }'
[[ $rc != 0 && $err == *SB5023* && $err == *'repetition count too large'* ]]; chk regexvalid.22

run_sash 'str s=a; if [[ $s =~ "^a{1,4294967297}$" ]] { echo wrong }'
[[ $rc != 0 && $err == *SB5023* && $err == *'invalid repetition range'* ]]; chk regexvalid.23

# DD132 trial: a forced read is not a pattern proof, in any consumer or operand depth.
run_sash 'const p=a; if match a ${p:!} { echo matched }'
[[ $rc != 0 && $err == *SB5105* && $err == *'regex-pattern operand'* ]]; chk regextrust.1

run_sash 'const p=a
if match a ${p:!} hits { echo "$(len hits)" } else { echo no }'
[[ $rc != 0 && $err == *SB5105* ]]; chk regextrust.2

run_sash 'const p=a
if matchall aa ${p:!} hits { echo "$(len hits)" } else { echo no }'
[[ $rc != 0 && $err == *SB5105* ]]; chk regextrust.3

run_sash 'const p=a; echo $(sub aba ${p:!} X)'
[[ $rc != 0 && $err == *SB5105* ]]; chk regextrust.4

run_sash 'const p=a; echo $(gsub aba ${p:!} X)'
[[ $rc != 0 && $err == *SB5105* ]]; chk regextrust.5

run_sash 'const p=a; str s=a
if [[ $s == a || $s =~ ${p:!} ]] { echo matched }'
[[ $rc != 0 && $err == *SB5105* ]]; chk regextrust.6

run_sash 'const p=a
[[ a =~ a { echo first } ${p:!} { echo second } || { echo no } ]]'
[[ $rc != 0 && $err == *SB5105* ]]; chk regextrust.7

run_sash 'str s=aba; const p=a; echo "${s:s/${p:!}/X/}"'
[[ $rc != 0 && $err == *SB5105* ]]; chk regextrust.8

run_sash 'str s=aba; const p=b
s =~ s/a/X/,s/${p:!}/Y/
echo $s'
[[ $rc != 0 && $err == *SB5105* ]]; chk regextrust.9

run_sash 'array s=(aba); const p=a; echo "${s[0]:s/${p:!}/X/}"'
[[ $rc != 0 && $err == *SB5105* ]]; chk regextrust.10

run_sash 'const p=a
if match aa "${p:!-x}${p:!?missing}" { echo matched }'
[[ $rc != 0 && $err == *SB5105* ]]; chk regextrust.11

run_sash 'map patterns=([key]=a); str key=key
if match a "${patterns[${key:!}]}" { echo matched }'
[[ $rc != 0 && $err == *SB5105* ]]; chk regextrust.12

run_sash 'str n=1
if match 1 "^$(( ${n:!-0} ))$" { echo matched }'
[[ $rc != 0 && $err == *SB5105* ]]; chk regextrust.13

run_sash 'const p=a
if match a "$(echo "$(echo ${p:!})")" { echo matched }'
[[ $rc != 0 && $err == *SB5105* ]]; chk regextrust.14

SASH_CFLAGS=--no-taint run_sash '_p=$(arg 1)
if attest _p r? { if match a ${_p:!} { echo matched } } else { exit 1 }' a
[[ $rc != 0 && $err == *SB5105* ]]; chk regextrust.15

SASH_CFLAGS=--no-taint-cst run_sash 'const p=a; if match a ${p:!} { echo matched }'
[[ $rc != 0 && $err == *SB5105* ]]; chk regextrust.16

run_sash '_p=$(arg 1)
if attest _p r? { if match a ${_p:!} { echo matched } } else { exit 1 }' a
[[ $rc != 0 && $err == *SB5105* ]]; chk regextrust.17

run_sash 'fn select { if match a ${args[0]:!} { echo matched } }; select a'
[[ $rc != 0 && $err == *SB5105* ]]; chk regextrust.18

run_sash 'str s=$(arg 1); str replacement=$(arg 2)
if match ${s:!} a { echo match }
if [[ ${s:!} =~ a ]] { echo test }
[[ ${s:!} =~ a { echo dispatch } || { echo wrong } ]]
echo $(- sub ${s:!} a ${replacement:!})
echo $(- gsub ${s:!} a ${replacement:!})' aba X
[[ $rc = 0 && $out = $'match\ntest\ndispatch\nXba\nXbX' \
   && -z $err && -z $rerr ]]; chk migratea.89

run_sash 'str s=":!"
if match "$s" ":!" { echo literal }
if [[ "$s" =~ ":!" ]] { echo test }
echo $(- sub ":!" ":!" yes)'
[[ $rc = 0 && $out = $'literal\ntest\nyes' && -z $err && -z $rerr ]]; chk migratea.90

run_sash 'str p=$(arg 1)
if [[ r? ${p:!} ]] { echo candidate } else { echo invalid }
echo ${p:!}' a
[[ $rc = 0 && $out = $'candidate\na' && -z $err && -z $rerr ]]; chk regextrust.21

# DD132 trial: validity is a separate, immutable-value, all-path proof.
run_sash 'const p=a; if match a "$p" { echo matched }'
[[ $rc != 0 && $err == *SB5106* && $err == *'immutable value'* ]]; chk regexproof.1

run_sash 'const p=a; if matchall aa "$p" hits { echo "$(len hits)" }'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.2

run_sash 'const p=a; echo $(- sub aba "$p" X)'
[[ $rc != 0 && $err == *SB5106* ]]; chk migratea.91

run_sash 'const p=a; echo $(- gsub aba "$p" X)'
[[ $rc != 0 && $err == *SB5106* ]]; chk migratea.92

run_sash 'const p=a; if [[ a =~ "$p" ]] { echo matched }'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.5

run_sash 'const p=a; [[ a =~ "$p" { echo matched } || { echo no } ]]'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.6

run_sash 'str s=aba; const p=a; echo "${s:s/$p/X/}"'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.7

run_sash 'array s=(aba); const p=a; echo "${s[0]:s/$p/X/}"'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.8

run_sash 'str s=aba; const p=b; s =~ s/a/X/,s/$p/Y/; echo $s'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.9

run_sash 'str p=a; attest p r?; if match a "$p" { echo matched }'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.10

SASH_CFLAGS=--no-taint run_sash 'const p=$(arg 1)
if match a "$p" { echo matched }' a
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.11

SASH_CFLAGS=--no-taint-cst run_sash 'const p=a; if match a "$p" { echo matched }'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.12

run_sash 'const p=a; attest p r?; str _copy=${p:!}
if match a "$_copy" { echo matched }'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.13

run_sash 'const p=a; attest p r?; if match aa "${p}a" { echo matched }'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.14

run_sash 'const p=a; str choice=yes
if [[ $choice == yes ]] { attest p r? }
if match a "$p" { echo matched }'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.15

run_sash 'int n=0
for _p in a b {
    if (( n == 0 )) { attest _p r? }
    if match ab "$_p" { echo matched }
    (( n += 1 ))
}'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.16

run_sash 'str _p=a
if attest _p r? { : } else { exit 1 }
if match a "$_p" { echo matched }'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.17

run_sash 'str _p=a
if attest _p r? { str _copy=$_p } else { exit 1 }
if match a "$_copy" { echo matched }'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.18

run_sash 'const p=a; attest p r?
ERR { if match a "$p" { - echo handler } }
fail 1'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.19

run_sash 'const p=a
fn select { if match a "$1" { echo matched } }
select "$p"'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.20

run_sash 'const p=$(arg 1); attest p r?
if match aba "$p" { echo match }
if matchall aba "$p" hits { echo "$(len hits)" }
if [[ aba =~ "$p" ]] { echo test }
[[ aba =~ "$p" { echo dispatch } || { echo no } ]]
str s=aba; array a=(aba)
echo $(- sub aba "$p" X) $(- gsub aba "$p" X)
echo "${s:s/$p/X/}" "${a[0]:s/$p/X/}"' a
[[ $rc = 0 && $out = $'match\n2\ntest\ndispatch\nXba XbX\nXba Xba' \
   && -z $err && -z $rerr ]]; chk migratea.93

run_sash 'const p=$(toupper a)
if [[ r? "$p" && A =~ "$p" ]] { echo matched }'
[[ $rc = 0 && $out = matched && -z $err && -z $rerr ]]; chk regexproof.22

run_sash 'const p=$(toupper a)
if ! [[ r? "$p" ]] { exit 1 }
if match A "$p" { echo matched }'
[[ $rc = 0 && $out = matched && -z $err && -z $rerr ]]; chk regexproof.23

run_sash 'const p=a; attest p r?; const q="$p"
if match a "$q" { echo matched }'
[[ $rc = 0 && $out = matched && -z $err && -z $rerr ]]; chk regexproof.24

run_sash 'const p=a; attest p r?
fn select { if match a "$1" { echo matched } }
fn forward { select "$1" }
forward "$p"'
[[ $rc = 0 && $out = matched && -z $err && -z $rerr ]]; chk regexproof.25

run_sash 'str p=a
fn select { str _p=$1; attest _p r?; if match a "$_p" { echo matched } }
select "$p"'
[[ $rc = 0 && $out = matched && -z $err && -z $rerr ]]; chk regexproof.26

run_sash 'array _patterns=(a b)
attest p in _patterns [[ r? $p ]]
for _p in @_patterns { if match ab "$_p" { echo $_p } }'
[[ $rc = 0 && $out = $'a\nb' && -z $err && -z $rerr ]]; chk regexproof.27

run_sash 'array _patterns=(a b)
attest p in _patterns [[ r? $p ]]
fn select { for _p in @A { if match ab "$_p" { echo $_p } } }
fn forward { select =A }
forward =_patterns'
[[ $rc = 0 && $out = $'a\nb' && -z $err && -z $rerr ]]; chk regexproof.28

run_sash 'const p=a; str choice=yes
if [[ $choice == yes ]] { attest p r? } else { attest p r? }
if match a "$p" { echo matched }'
[[ $rc = 0 && $out = matched && -z $err && -z $rerr ]]; chk regexproof.29

run_sash 'str result=$(str _p=$(toupper a); attest _p r?
if match A "$_p" { echo matched })
echo $result'
[[ $rc = 0 && $out = matched && -z $err && -z $rerr ]]; chk regexproof.30

run_sash 'const p=a; str choice=yes
if [[ r? "$p" || $choice == yes ]] {
    if match a "$p" { echo matched }
}'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.31

run_sash 'str _p=a
fn select { if match a "$1" { echo matched } }
if attest _p r? { select "$_p" } else { exit 1 }'
[[ $rc = 0 && $out = matched && -z $err && -z $rerr ]]; chk regexproof.32

run_sash 'for _p in a b {
    attest _p r?
    if match ab "$_p" { echo $_p }
}'
[[ $rc = 0 && $out = $'a\nb' && -z $err && -z $rerr ]]; chk regexproof.33

run_sash 'array _patterns=(a); attest p in _patterns [[ r? $p ]]
if match a "${_patterns[5]:-a}" { echo matched }'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.34

run_sash 'array _patterns=(a); attest p in _patterns [[ r? $p ]]
if match a "${_patterns[0]:?missing}" { echo matched }'
[[ $rc = 0 && $out = matched && -z $err && -z $rerr ]]; chk regexproof.35

run_sash 'const p=a
fn select { if match a "$p" { echo matched } }
select
attest p r?'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.36

run_sash 'const p=a
fn select { if match a "$p" { echo matched } }
select &
attest p r?
- wait'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.37

run_sash 'str _p=a; attest _p r?
if attest _p [[ $_p == no ]] { echo no } else {
    if match a "$_p" { echo matched }
}'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.38

run_sash 'const p=a; attest p r?
if attest p [[ $p == no ]] { echo no } else {
    if match a "$p" { echo matched }
}'
[[ $rc = 0 && $out = matched && -z $err && -z $rerr ]]; chk regexproof.39

run_sash 'ERR {
    str _p=$(toupper a)
    if attest _p r? { if match A "$_p" { - echo handler } } else { exit 2 }
}
fail 1'
[[ $rc = 1 && $out = handler && -z $err && -z $rerr ]]; chk regexproof.40

run_sash 'fn select { if match a "$1" { echo matched } }; select a'
[[ $rc = 0 && $out = matched && -z $err && -z $rerr ]]; chk regexproof.51

run_sash 'str p=a
fn select {
    if [[ r? "$1" ]] { if match a "$1" { echo matched } } else { exit 1 }
}
select "$p"'
[[ $rc = 0 && $out = matched && -z $err && -z $rerr ]]; chk regexproof.52

run_sash 'str p=a
fn validate { if ! [[ r? "$1" ]] { exit 1 } }
fn select { if match a "$1" { echo matched } }
validate "$p"; select "$p"'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.53

run_sash 'const p=a; attest p r?; str _copy=${p:!}; attest _copy r?
if match a "$_copy" { echo matched }'
[[ $rc = 0 && $out = matched && -z $err && -z $rerr ]]; chk regexproof.54

run_sash 'const p=a; attest p [[ r? $p && a =~ $p ]]
if match a "$p" { echo matched }'
[[ $rc = 0 && $out = matched && -z $err && -z $rerr ]]; chk regexproof.55

run_sash 'const p=a; attest p [[ -n $p || r? $p ]]
if match a "$p" { echo matched }'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.56

run_sash 'array patterns=(a); attest p in patterns [[ r? $p ]]
if match a "${patterns[0]}" { echo matched }'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.57

run_sash 'for _p in a {
    if attest _p r? { : } else { continue }
    if match a "$_p" { echo matched }
}'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.58

run_sash 'const p=a
fn validate { attest p r? }
validate &
- wait
if match a "$p" { echo matched }'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.59

run_sash 'const p=a; attest p r?
fn select { if match a "$p" { echo matched } }
select &
- wait'
[[ $rc = 0 && $out = matched && -z $err && -z $rerr ]]; chk regexproof.60

run_sash 'const p=a; attest p r?
fn select { if match a "$p" { echo matched } }
select | /bin/cat'
[[ $rc = 0 && $out = matched && -z $err && -z $rerr ]]; chk regexproof.61

run_sash 'const p=a
fn validate { attest p r?; echo input }
fn select {
    if readline _input {
        attest _input [[ -n $_input ]]
        if match a "$p" { echo matched }
    }
}
validate | select'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.62

run_sash 'map _patterns=([a]=a [b]=b); str key=a
if [[ r? "${_patterns[$key]}" ]] {
    if match a "${_patterns[$key]}" { echo matched }
}'
[[ $rc = 0 && $out = matched && -z $err && -z $rerr ]]; chk regexproof.63

run_sash 'map _patterns=([a]=a [b]=b); str key=a
if [[ r? "${_patterns[$key]}" ]] {
    key=b
    if match b "${_patterns[$key]}" { echo matched }
}'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.64

run_sash 'array _patterns=(a b); int i=0
if [[ r? "${_patterns[$(( i + 0 ))]}" ]] {
    (( i += 1 ))
    if match b "${_patterns[$(( i + 0 ))]}" { echo matched }
}'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.65

run_sash 'map _patterns=([a]=a [b]=b)
attest key value in _patterns [[ r? $key && r? $value ]]
for _key in $(keys _patterns) {
    if match ab "$_key" { echo $_key }
    if match ab "${_patterns[$_key]}" { echo value }
}'
[[ $rc = 0 && $out = $'a\nvalue\nb\nvalue' && -z $err && -z $rerr ]]; chk regexproof.66

run_sash 'map _patterns=([a]=a [b]=b)
if [[ r? "${_patterns[a]}" ]] {
    if match b "${_patterns[b]}" { echo matched }
}'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.67

run_sash 'const p=a
fn validate { const q=a; attest q r? }
validate
if match a "$p" { echo matched }'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.68

run_sash 'const p=a; array _patterns=()
attest item in _patterns [[ -n $item && r? $p ]]
if match a "$p" { echo matched }'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.69

run_sash 'const p=a
fn validate { const q=$1; attest q r? }
validate "$p"
if match a "$p" { echo matched }'
[[ $rc = 0 && $out = matched && -z $err && -z $rerr ]]; chk regexproof.70

printf '%s\n' 'const p=a; if match a "$p" { echo matched }' > proof.sb
timeout "$SPEC_TIMEOUT" "${SASH:-sash}" --emit-ir proof.sb > proof.ir 2> proof.err
rc=$?; err=$(<proof.err)
[[ $rc != 0 && $err == *SB5106* && ! -s proof.ir ]]; chk regexproof.71

timeout "$SPEC_TIMEOUT" "${SASH:-sash}" --manifest proof.sb > proof.manifest 2> proof.err
rc=$?; err=$(<proof.err)
[[ $rc != 0 && $err == *SB5106* && ! -s proof.manifest ]]; chk regexproof.72

run_sash 'const p=a
fn valid { return [[ r? "$1" ]] }
if valid "$p" { if match a "$p" { echo matched } }'
[[ $rc = 0 && $out = matched && -z $err && -z $rerr ]]; chk regexproof.73

run_sash 'const p=a
fn valid { return [[ r? "$1" ]] }
fn relay { return valid "$1" 2>/dev/null }
if relay "$p" { if match a "$p" { echo matched } }'
[[ $rc = 0 && $out = matched && -z $err && -z $rerr ]]; chk regexproof.74

run_sash 'const p="("
fn invalid { if [[ r? "$1" ]] { return 1 }; return 0 }
if invalid "$p" { if match a "$p" { echo matched } }'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.75

run_sash 'const p=a; const copy=$p; attest p r?
if match a "$copy" { echo matched }'
[[ $rc = 0 && $out = matched && -z $err && -z $rerr ]]; chk regexproof.76

run_sash 'array _patterns=(a b); attest p in _patterns [[ r? $p ]]
array _copy=(@_patterns)
if match b "${_copy[1]}" { echo matched }'
[[ $rc = 0 && $out = matched && -z $err && -z $rerr ]]; chk regexproof.77

run_sash 'const p=a; attest p r?
EXIT { if match a "$p" { echo matched } }'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.78

run_sash 'const p=a
EXIT { if [[ r? $p ]] { if match a "$p" { echo matched } } }
exit 0'
[[ $rc = 0 && $out = matched && -z $err && -z $rerr ]]; chk regexproof.79

run_sash 'const p=a; str copy=$p; attest p r?
if match a "$copy" { echo matched }'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.80

run_sash 'const p="("
if [[ r? $p ]] || :; _directory=$(mkdtemp) {
    /bin/rmdir "$_directory"
    if match a "$p" { echo matched }
}'
[[ $rc != 0 && $err == *SB5106* ]]; chk migratea.102

run_sash 'array _patterns=("(" a)
if /bin/false { exit 1 } else {
    if [[ r? "${_patterns[$?]}" ]] {
        if match a "${_patterns[$?]}" { echo matched }
    }
}'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.88

run_sash 'array _patterns=("(" a)
if /bin/false { exit 1 } else {
    const first="${_patterns[$?]}"
    if [[ r? $first ]] {
        const second="${_patterns[$?]}"
        if [[ r? $first ]] {
            if match a "$second" { echo matched }
        }
    }
}'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.89

run_sash 'array _patterns=("(" a)
if /bin/false { exit 1 } else {
    int _index=$(status)
    if [[ r? "${_patterns[$_index]}" ]] {
        if match a "${_patterns[$_index]}" { echo matched }
    }
}'
[[ $rc == 0 && $out == matched ]]; chk regexproof.90

run_sash 'array _patterns=(a b)
attest p in _patterns r?
if /bin/false { exit 1 } else {
    if match b "${_patterns[$?]:?missing}" { echo matched }
}'
[[ $rc == 0 && $out == matched ]]; chk regexproof.91

run_sash 'fn select {
    const p=$1
    if match a "$p" { echo matched }
}
select a
select "("'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.92

run_sash 'fn select {
    const p=$1
    if [[ "$2" == again ]] { return select "(" done }
    if match a "$p" { echo matched }
    return 0
}
if select a again { : }'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.93

run_sash 'fn emit { echo payload }
map paths=([Xba]=filename); const p=a
emit > "${paths[$(- sub aba "$p" X)]:?missing}"'
[[ $rc != 0 && $err == *SB5106* ]]; chk migratea.94

run_sash 'fn emit { echo payload }
map paths=([Xba]=filename); const p=a; attest p r?
emit > "${paths[$(- sub aba "$p" X)]:?missing}"
/bin/cat filename'
[[ $rc == 0 && $out == payload && -z $err && -z $rerr ]]; chk migratea.95

run_sash 'str _p=a
if ! attest _p r? { exit 1 } else { : }
if match a "$_p" { echo escaped }'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.96

run_sash 'str _p=a
if ! attest _p r? { exit 1 } else {
    if match a "$_p" { echo matched }
}'
[[ $rc == 0 && $out == matched && -z $err && -z $rerr ]]; chk regexproof.97

run_sash 'str _p=a; attest _p r? || exit 1
if match a "$_p" { echo escaped }'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.98

run_sash 'array _patterns=(a); attest p in _patterns [[ r? $p ]]
if match b "${_patterns[5]}" { echo matched }'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexproof.99

# DD132: regex captures carry text and a separately consumed Boolean.
run_sash 'if out=$(sub abc b B) { echo "$?:[$out]" } else { echo wrong }'
[[ $rc = 0 && $out = '0:[aBc]' && -z $err && -z $rerr ]]; chk regexresult.1

run_sash 'str out=old
if out=$(sub abc z X) { echo wrong } else { echo "$?:[$out]" }'
[[ $rc = 0 && $out = '1:[abc]' && -z $err && -z $rerr ]]; chk regexresult.2

run_sash 'if out=$(sub abc b b) { echo "matched:[$out]" } else { echo wrong }'
[[ $rc = 0 && $out = 'matched:[abc]' && -z $err && -z $rerr ]]; chk regexresult.3

run_sash 'if out=$(gsub abab b B) { echo "$?:[$out]" } else { echo wrong }'
[[ $rc = 0 && $out = '0:[aBaB]' && -z $err && -z $rerr ]]; chk regexresult.4

run_sash 'if out=$(gsub abc z X) { echo wrong } else { echo "$?:[$out]" }'
[[ $rc = 0 && $out = '1:[abc]' && -z $err && -z $rerr ]]; chk regexresult.5

run_sash 'if out=$(sub "" "^$" "") { echo "matched:[$out]" } else { echo wrong }'
[[ $rc = 0 && $out = 'matched:[]' && -z $err && -z $rerr ]]; chk regexresult.6

run_sash 'if out=$(gsub ab "^" X) { echo "matched:[$out]" } else { echo wrong }'
[[ $rc = 0 && $out = 'matched:[Xab]' && -z $err && -z $rerr ]]; chk regexresult.7

run_sash 'if out=$(match abc123 "[0-9]+") { echo "$?:[$out]" } else { echo wrong }'
[[ $rc = 0 && $out = '0:[123]' && -z $err && -z $rerr ]]; chk regexresult.8

run_sash 'if out=$(match abc "^") { echo "matched:[$out]" } else { echo wrong }'
[[ $rc = 0 && $out = 'matched:[]' && -z $err && -z $rerr ]]; chk regexresult.9

run_sash 'str out=old
if out=$(match abc z) { echo wrong } else { echo "$?:[$out]" }'
[[ $rc = 0 && $out = '1:[]' && -z $err && -z $rerr ]]; chk regexresult.10

run_sash 'str out=$(sub abc a X); echo "$out"'
[[ $rc != 0 && $err == *SB4018* && $err == *'`sub`'* && $err == *boolean* ]]; chk regexresult.11

run_sash 'echo "[$(gsub aba a A)]"'
[[ $rc != 0 && $err == *SB4018* && $err == *'`gsub`'* && $err == *boolean* ]]; chk regexresult.12

run_sash 'out=$(match abc b); echo "$out"'
[[ $rc != 0 && $err == *SB4018* && $err == *'`match`'* && $err == *boolean* ]]; chk regexresult.13

run_sash 'if [[ $(sub abc a X) == Xbc ]] { echo wrong }'
[[ $rc != 0 && $err == *SB4018* && $err == *'`sub`'* && $err == *boolean* ]]; chk regexresult.14

run_sash 'echo $(- sub $(gsub aba a A) A X)'
[[ $rc != 0 && $err == *SB4018* && $err == *'`gsub`'* && $err == *boolean* ]]; chk regexresult.15

run_sash 'fn f { echo "$(sub abc a X)" }
echo $(- sub "$(f)" X Y)'
[[ $rc != 0 && $err == *SB4018* && $err == *'`sub`'* && $err == *boolean* ]]; chk regexresult.16

run_sash '_out=$(- sub abc z X); echo "[$_out]"'
[[ $rc = 0 && $out = '[abc]' && -z $err && -z $rerr ]]; chk regexresult.17

run_sash '_out=$(- match abc z); echo "[$_out]"'
[[ $rc = 0 && $out = '[]' && -z $err && -z $rerr ]]; chk regexresult.18

run_sash 'if out=$(- sub abc a X) { echo "$out" }'
[[ $rc != 0 && $err == *SB4025* ]]; chk regexresult.19

run_sash 'if str out=$(sub abc a X) { echo "$out" }'
[[ $rc != 0 && $err == *SB4025* ]]; chk regexresult.20

run_sash 'out=$(sub abc z X) || echo "miss:[$out]"'
[[ $rc = 0 && $out = 'miss:[abc]' && -z $err && -z $rerr ]]; chk regexresult.21

run_sash 'if ! out=$(match abc z) { echo "$?:[$out]" } else { echo wrong }'
[[ $rc = 0 && $out = '0:[]' && -z $err && -z $rerr ]]; chk regexresult.22

run_sash 'str text=aa
while _out=$(sub "$text" a "") { echo "[$_out]"; text=$_out }
echo "done:[$_out]"'
[[ $rc = 0 && $out = $'[a]\n[]\ndone:[]' && -z $err && -z $rerr ]]; chk regexresult.23

run_sash 'const re="^a"
attest re r?
if _out=$(sub abc "$re" X) { echo "$_out" } else { echo wrong }'
[[ $rc = 0 && $out = Xbc && -z $err && -z $rerr ]]; chk regexresult.24

SASH_CFLAGS='--no-taint --no-taint-cst' run_sash 'str re=a
if out=$(sub abc "$re" X) { echo "$out" }'
[[ $rc != 0 && $err == *SB5106* ]]; chk regexresult.25

SASH_CFLAGS=--no-taint run_sash 'if out=$(sub "$(/bin/false)" a X) { echo "$out" } else { echo caught }'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed'* ]]; chk regexresult.26

SASH_CFLAGS=--no-taint run_sash 'echo $(- sub "$(/bin/false)" a X)'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed'* ]]; chk regexresult.27

# A text projection must not replace the program's completion status.
run_sash 'str text=abc
EXIT { echo "$text" }
text=${text:s/z/X/}'
[[ $rc = 0 && $out = abc && -z $err && -z $rerr ]]; chk regexresult.28

run_sash 'str name=abc parent=abc
EXIT { echo "$name,$parent" }
name=$(basename "$name")
parent=$(dirname "$parent")'
[[ $rc = 0 && $out = abc,abc && -z $err && -z $rerr ]]; chk regexresult.29

run_sash 'str text=abc
_out=${text:s/a/A/,s/z/Z/}
echo "$_out"'
[[ $rc = 0 && $out = Abc && -z $err && -z $rerr ]]; chk regexresult.30

run_sash $'if out=$(sub $\'ab\\x00tail\' tail X) { echo wrong } else {\nif [[ $out == $\'ab\\x00tail\' ]] { echo "retained:$(strlen $out)" }\n}'
[[ $rc = 0 && $out = retained:7 && -z $err && -z $rerr ]]; chk regexresult.31

printf -v _regex_large '%32768s' ''
SASH_CFLAGS='--heap-size=4096' run_sash "str text='$_regex_large'
echo ready
if out=\$(sub \"\$text\" x X) { echo \"\$out\" } else { echo caught }"
[[ $rc = 2 && $out = ready && -z $err && $rerr == *'regexp error: out of memory'* ]]; chk regexresult.32

SASH_CFLAGS=--no-taint run_sash 'fn f { /bin/false; echo data }
if out=$(sub "$(f)" a X) { echo "$out" } else { echo caught }'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed'* ]]; chk regexresult.33

run_sash 'if out=$(sub abc z X); true { echo "$out" }'
[[ $rc != 0 && $err == *SB4018* && $err == *sub* && $err == *boolean* ]]; chk regexresult.34

run_sash 'if out=$(/bin/false); true { echo "wrong:${out:!}" }'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed'* ]]; chk regexresult.35

run_sash 'sub abc a X'
[[ $rc != 0 && $err == *SB5073* && $err == *'x=$(- sub …)'* ]]; chk regexresult.36

run_sash 'array m=(); x=$(match abc a m); echo "$x:${m[0]}"'
[[ $rc != 0 && $err == *SB4045* && $err == *'x=$(- match S P)'* ]]; chk regexresult.37

# Index captures must finish lowering before the redirected operation, including
# the three pipeline consumers and a capture that creates additional SSA blocks.
run_sash 'fn key { echo aba }
map paths=([Xba]=filename)
echo direct > "${paths[$(- sub "$(key)" a X)]:?missing}"
/bin/cat filename
/bin/echo regular | /bin/cat > "${paths[$(- sub "$(key)" a X)]:?missing}"
/bin/cat filename
echo lastpipe > filename
/bin/cat < "${paths[$(- sub "$(key)" a X)]:?missing}" | readline line
attest line p?
echo "$line"
echo loop > filename
/bin/cat < "${paths[$(- sub "$(key)" a X)]:?missing}" | while readline entry {
attest entry p?
echo "$entry"
}'
[[ $rc = 0 && $out = $'direct\nregular\nlastpipe\nloop' && -z $err && -z $rerr ]]; chk regexresult.38

run_sash 'str text=aba; map paths=([Xba]=filename)
echo sugar > "${paths[${text:s/a/X/}]:?missing}"
/bin/cat filename'
[[ $rc = 0 && $out = sugar && -z $err && -z $rerr ]]; chk regexresult.39

SASH_CFLAGS=--no-taint run_sash 'map paths=([X]=filename)
- echo wrong > "${paths[$(- sub "$(/bin/false)" a X)]:?missing}"
echo after'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed'* ]]; chk regexresult.40

# Guarded index captures follow the arguments and each preceding redirect.
run_sash 'str key=before
map paths=([before]=arg-before [after]=arg-after)
fn argument { key=after; echo argument }
fn index { echo "$key" }
echo "$(argument)" > "${paths[$(index)]:?missing}"
/bin/cat arg-after'
[[ $rc = 0 && $out = argument && -z $err && -z $rerr ]]; chk regexresult.41

run_sash 'str where=parent
fn index { where=child; echo key }
fn emit { echo function }
map paths=([key]=stage-index)
/bin/echo external | /bin/cat > "${paths[$(index)]:?missing}"
echo "$where"
/bin/cat stage-index
emit > "${paths[$(index)]:?missing}" | /bin/cat
echo "$where"
/bin/cat stage-index'
[[ $rc = 0 && $out = $'parent\nexternal\nparent\nfunction' && -z $err && -z $rerr ]]; chk regexresult.42

run_sash 'str marker=old
fn index { marker=new; echo key }
map paths=([key]=skipped-index)
if /bin/echo wrong > missing/first > "${paths[$(index)]:?missing}" {
    echo wrong
} else { echo caught }
echo "$marker"'
[[ $rc = 0 && $out = $'caught\nold' && -z $err && -z $rerr && ! -e skipped-index ]]; chk regexresult.43

run_sash 'str text=before optional=""
str trusted=$(arg 99 "trusted")
array words=("one two" "" last)
fn index { text=after; words[0]=changed; optional=present; trusted=changed; echo key }
map paths=([key]=retained-words)
/usr/bin/printf "<%s>\n" "$text" @{words:?empty} ${optional:~} ${trusted:!~} tail > "${paths[$(index)]:?missing}"
/bin/cat retained-words
echo "$text:$optional"'
[[ $rc = 0 && $out = $'<before>\n<one two>\n<>\n<last>\n<trusted>\n<tail>\nafter:present' && -z $err && -z $rerr ]]; chk regexresult.44

run_sash 'str text=before
map values=([data]=before)
fn index { text=after; values[data]=after; echo key }
fn emit { echo "$1:${A[data]}" }
map paths=([key]=retained-call)
emit "$text" =values > "${paths[$(index)]:?missing}"
/bin/cat retained-call'
[[ $rc = 0 && $out = before:after && -z $err && -z $rerr ]]; chk regexresult.45

run_sash 'fn emit { echo "$1:${A[data]}" }
fn outer {
    emit payload =B > "${A[$(echo "$1")]:?missing}"
}
map paths=([key]=caller-index)
map values=([data]=from-slot)
outer key =paths =values
/bin/cat caller-index'
[[ $rc = 0 && $out = payload:from-slot && -z $err && -z $rerr ]]; chk regexresult.46

run_sash 'fn index { echo key }
map paths=([key]=permanent-index)
exec 7>&1
exec > "${paths[$(index)]:?missing}"
echo permanent
exec 1>&7
close 7
/bin/cat permanent-index'
[[ $rc = 0 && $out = permanent && -z $err && -z $rerr ]]; chk regexresult.47

run_sash 'str marker=old
fn index { marker=new; echo key }
map paths=([key]=skipped-exec-index)
if exec > missing/first > "${paths[$(index)]:?missing}" {
    echo wrong
} else { echo caught }
echo "$marker"'
[[ $rc = 0 && $out = $'caught\nold' && -z $err && -z $rerr && ! -e skipped-exec-index ]]; chk regexresult.48

run_sash 'echo 0 > index-input
array paths=(ordered-index)
fn index -> int {
    if readline value { attest value (( number = value )); return $number }
    return 1
}
/bin/echo ordered < index-input > "${paths[$(( (index) ))]:?missing}"
/bin/cat ordered-index'
[[ $rc = 0 && $out = ordered && -z $err && -z $rerr ]]; chk regexresult.49

run_sash 'array paths=(index-output)
fn argument -> int { echo argument >&2; return 7 }
fn index -> int { echo index >&2; return 0 }
echo "$(argument)" 2> index-stderr > "${paths[$(( (index) ))]:?missing}"
/bin/cat index-output index-stderr'
[[ $rc = 0 && $out = $'7\nindex' && -z $err && $rerr = argument ]]; chk regexresult.50

run_sash 'write line > reader-input
map paths=([key]=reader-input)
fn index { echo key }
if readline line < "${paths[$(index)]:?missing}" {
    attest line p?
    echo "$line"
}
array lines=()
if read lines < "${paths[$(index)]:?missing}" {
    attest item in lines p?
    echo "${lines[0]}"
}'
[[ $rc = 0 && $out = $'line\nline' && -z $err && -z $rerr ]]; chk regexresult.51

run_sash 'map paths=([key]=external-index)
echo payload > "${paths[$(/bin/echo key)]:?missing}"'
[[ $rc != 0 && $err == *SB5020* ]]; chk regexresult.52

SASH_CFLAGS=--no-taint run_sash 'map paths=([key]=external-index)
echo payload > "${paths[$(/bin/echo key)]:?missing}"
/bin/cat external-index'
[[ $rc = 0 && $out = payload && -z $err && -z $rerr ]]; chk regexresult.53

run_sash 'fn key { echo key }
map paths=([key]=concat-index)
echo default > "${paths[$(echo absent)]:-default-index}"
echo concat > "./${paths[$(key)]:?missing}.suffix"
/bin/cat default-index concat-index.suffix'
[[ $rc = 0 && $out = $'default\nconcat' && -z $err && -z $rerr ]]; chk regexresult.54

SASH_CFLAGS=--no-taint run_sash 'map paths=([key]=operand-index)
if echo wrong > "${paths[$(/bin/false)]:-operand-index}" {
    echo wrong
} else { echo caught }'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed'* ]]; chk regexresult.55

run_sash 'fn index { echo key }
fn verdict { return 3 }
map paths=([key]=condition-index)
if /bin/false > "${paths[$(index)]:?missing}" {
    echo wrong
} else { echo "external:$?" }
if verdict > "${paths[$(index)]:?missing}" {
    echo wrong
} else { echo "function:$?" }'
[[ $rc = 0 && $out = $'external:1\nfunction:3' && -z $err && -z $rerr ]]; chk regexresult.56

run_sash 'map paths=([key]=unchecked-exec-index)
echo before
exec > missing/first > "${paths[$(echo key)]:?missing}"
echo after'
[[ $rc = 1 && $out = before && -z $err && $rerr == *missing/first* && ! -e unchecked-exec-index ]]; chk regexresult.57

run_sash 'fn index { echo key }
map paths=([key]=failed-exec-stdout)
if exec > "${paths[$(index)]:?missing}" 2> failed-exec-stderr > missing/last {
    echo wrong
} else { echo caught }
/bin/false'
[[ $rc = 1 && $out = caught && -z $err && $rerr == *'/bin/false: command failed'* && ! -s failed-exec-stdout && ! -s failed-exec-stderr ]]; chk regexresult.58

run_sash 'map paths=([key]=exec-diagnostics)
exec 2> "${paths[$(echo key)]:?missing}"
/bin/false'
[[ $rc = 1 && -z $out && -z $err && -z $rerr ]] && grep -qF '/bin/false: command failed' exec-diagnostics; chk regexresult.59

set -e
tally
