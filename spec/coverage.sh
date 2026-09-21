#!/bin/bash
# spec/coverage.sh — tests for reachable compiler paths that no feature-
# oriented spec happens to exercise, found via LLVM coverage (journal 188+).
# Only genuinely-new (0 prior run_sash) reachable rejections go here; shadowed/
# unreachable code is deleted instead of tested (DG 2026-08-26).
set -eu
. common.sh
setup coverage
plan=337
echo "1..$plan" >&3

set +e

# cov.1 a mutation in a `(( ))` CONDITION is rejected (validate SB5007) — no
# prior spec exercises a bare mutation in condition position
run_sash 'if (( x=1 )); then echo hi; fi'
[[ $rc != 0 && -z $out && $err == *SB5007* ]]; chk cov.1
# cov.2 DD#31 comma-in-condition: a non-final part that isn't a mutation → SB5007
run_sash 'int a=0; int b=0; while (( a==1, b=2 )); do echo x; done'
[[ $rc != 0 && -z $out && $err == *SB5007* ]]; chk cov.2
# cov.3 DD#31 comma-in-condition: a final part that IS a mutation → SB5007
run_sash 'int a=0; while (( a=1, a=2 )); do echo x; done'
[[ $rc != 0 && -z $out && $err == *SB5007* ]]; chk cov.3
# cov.4 a static (compile-constant) invalid regex in `=~` → SB5023
run_sash '[[ abc =~ ( ]] && echo y'
[[ $rc != 0 && -z $out && $err == *SB5023* ]]; chk cov.4
# cov.5 a numeric test op with a non-numeric operand → SB5019
run_sash '[[ 5 -lt abc ]] && echo y'
[[ $rc != 0 && -z $out && $err == *SB5019* ]]; chk cov.5
# cov.6 a boolean-context predicate that does not return on all paths → SB4037
run_sash 'p() { if true; then return 0; fi }; if p; then echo y; fi'
[[ $rc != 0 && -z $out && $err == *SB4037* ]]; chk cov.6
# cov.7 `read -f` is retired with every other read flag (DD#99) → SB5057
run_sash 'read -f x < /etc/hostname'
[[ $rc != 0 && -z $out && $err == *SB5057* ]]; chk cov.7
# cov.8 a `${x@OP}` transform is rejected → SB4015
run_sash 'x=hi; echo ${x@Q}'
[[ $rc != 0 && -z $out && $err == *SB4015* ]]; chk cov.8
# cov.9 the removed `case` construct → SB4029
run_sash 'case $x in a) echo m;; esac'
[[ $rc != 0 && -z $out && $err == *SB4029* ]]; chk cov.9
# cov.10 a tainted value placed into an array via subscript assign → SB5021
run_sash 'array a=(); a[0]=$(getenv HOME)'
[[ $rc != 0 && -z $out && $err == *SB5021* ]]; chk cov.10
# cov.11 `source` / `.` is rejected as dynamic execution → SB4001. (Was SB4002
# from the parser's removed-builtin table; that table is gone and SB4001 from
# validate::check_dynexec — which always fired too — is now the only answer.)
run_sash 'source f'
[[ $rc != 0 && -z $out && $err == *SB4001* ]]; chk cov.11
# cov.12 indirect expansion `${!x}` is rejected → SB4003
run_sash 'x=y; echo ${!x}'
[[ $rc != 0 && -z $out && $err == *SB4003* ]]; chk cov.12
# cov.13 a `{ }` group cannot be a condition → SB4027
run_sash 'if { echo a; }; then echo hi; fi'
[[ $rc != 0 && -z $out && $err == *SB4027* ]]; chk cov.13
# cov.14 heredocs are rejected (use a herestring) → SB5002
run_sash 'cat <<EOF'
[[ $rc != 0 && -z $out && $err == *SB5002* ]]; chk cov.14
# cov.101 a heredoc as the whole command (no command name) is also SB5002.
# This is the `check_heredoc` name arm; cov.14 above covers the args arm.
run_sash '<<EOF'
[[ $rc != 0 && -z $out && $err == *SB5002* ]]; chk cov.101
# cov.102 the SB5002 cascade is suppressed: `__HEREDOC__` is an internal
# lexer sentinel and must never reach a user-facing message, and the heredoc
# body lines must not be re-reported as unknown commands (SB5011).
run_sash '<<EOF'
[[ $err != *__HEREDOC__* && $err != *SB5011* ]]; chk cov.102
# cov.103 a backtick inside a $( ) inside a double-quoted string. command_sub_to_expr
# used to carve out content starting with a backtick and keep the WHOLE
# substitution as a verbatim Literal — a holdover from when backticks were their
# own token. That dodged both the backtick rejection and the taint check, and
# diverged from bash, which runs the command. Found while dropping the sugar.
run_sash 'echo "$(`/bin/echo hi`)"'
[[ $rc != 0 && -z $out && $err == *SB5076* ]]; chk cov.103
# cov.104 a `$` as the very last byte of the source is a literal `$`, as in
# bash. This is scan_dollar's end-of-source guard; `-c` passes no trailing
# newline, so it is the only way to reach it.
run_sash 'echo $'
[[ $rc = 0 && $out = '$' ]]; chk cov.104
# cov.105 the same `$` with a word in front of it.
run_sash 'echo a$'
[[ $rc = 0 && $out = 'a$' ]]; chk cov.105
# cov.15 `$(status)` with no preceding command → SB5012
run_sash 'echo $(status)'
[[ $rc != 0 && -z $out && $err == *SB5012* ]]; chk cov.15
# cov.16 an inline `*` glob in a command argument → SB5029
run_sash 'echo *'
[[ $rc != 0 && -z $out && $err == *SB5029* ]]; chk cov.16
# cov.17 a malformed `@{…}` splat disposition → SB5033
run_sash 'array a=(1); echo @{a:x}'
[[ $rc != 0 && -z $out && $err == *SB5033* ]]; chk cov.17
# cov.18 a `$var` in a for-loop source (no word-split by design) → SB5009
run_sash 'y=a; for x in $y; do echo $x; done'
[[ $rc != 0 && -z $out && $err == *SB5009* ]]; chk cov.18
# cov.19 the `**` globstar is always rejected (even quoted) → SB5006
run_sash 'echo "**"'
[[ $rc != 0 && -z $out && $err == *SB5006* ]]; chk cov.19
# cov.20 `read` takes NO flag, so a bogus one is the same error as a real one
# — there is no unknown-flag category left (DD#99) → SB5057
run_sash 'read -z x < /etc/hostname'
[[ $rc != 0 && -z $out && $err == *SB5057* ]]; chk cov.20
# cov.21 quoting an array splat is not allowed → SB5026
run_sash 'echo "$(argv)"'
[[ $rc != 0 && -z $out && $err == *SB5026* ]]; chk cov.21
# cov.22 `eval` (dynamic execution) is rejected → SB4001
run_sash 'eval "echo hi"'
[[ $rc != 0 && -z $out && $err == *SB4001* ]]; chk cov.22
# cov.23 a prefix `++`/`--` in `(( ))` is an arith syntax error → SB4020
run_sash 'int x=0; (( ++x ))'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk cov.23
# cov.24 `export` is not a reserved word — it is simply an unknown command, so
# it gets the ordinary SB5011 every other unknown name gets (was a bespoke
# SB5001 from the parser's removed-builtin table).
run_sash 'export A=1'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk cov.24
# cov.25 compact dispatch with the `=~` operator (no spaces) — existing specs
# only exercise compact `==`; this covers the `=~` compact-parse path
run_sash '[[abc=~b]] && echo match'
[[ $rc = 0 && $out = match && -z $err ]]; chk cov.25
# cov.26 `>&N` dup redirect (emit_dup_output) — only the harness/converter use
# it; no spec exercises the compiled codegen path. `echo hi >&2` → stderr.
run_sash 'echo hi >&2'
[[ $rc = 0 && -z $out && $rerr = hi ]]; chk cov.26
# cov.86-88 a `>&`/`<&` fd-dup target that isn't a plain fd number is rejected
# SB4048 (DG 2026-08-29) — was a silent dup-to-fd-2 fallback
# (bug-redirect-dup-target-unvalidated). All modes (parse reject).
run_sash 'echo hi >&foo'
[[ $rc != 0 && -z $out && $err == *SB4048* ]]; chk cov.86
run_sash 'echo hi 2>&1x'
[[ $rc != 0 && -z $out && $err == *SB4048* ]]; chk cov.87
run_sash 'echo hi >&-'
[[ $rc != 0 && -z $out && $err == *SB4048* ]]; chk cov.88
# cov.27 a file redirect whose TARGET is a variable expansion (`> $f`) — the
# only `> $var` specs are taint rejections, so this clean codegen path (redirect
# target as a runtime expr) is untested.
run_sash 'f=rvfile; echo hi > $f; cat $f'
[[ $rc = 0 && $out = hi && -z $err ]]; chk cov.27
# cov.28 append (`>>`) to a variable-expansion target
run_sash 'f=rvfile2; echo a > $f; echo b >> $f; cat $f'
[[ $rc = 0 && $out = $'a\nb' && -z $err ]]; chk cov.28
# cov.29-48 arithmetic-operator coverage: no feature spec exercises these
# operators with a variable operand, leaving transform/expr_handlers.rs
# (unaryop_to_ssa / try_fold_unop / incdec_to_ssa / compound_assign_to_ssa /
# comma_to_ssa) and validate_arith_comma_in_condition's passing path
# untested. All compile+run identically in native and c.
# unary ops in $(( )) with a runtime (variable) operand — unaryop_to_ssa
run_sash 'int x=5; echo $(( -x ))'
[[ $rc = 0 && $out = -5 && -z $err ]]; chk cov.29
run_sash 'int x=5; echo $(( ~x ))'
[[ $rc = 0 && $out = -6 && -z $err ]]; chk cov.30
run_sash 'int x=5; echo $(( !x ))'
[[ $rc = 0 && $out = 0 && -z $err ]]; chk cov.31
run_sash 'int x=0; echo $(( !x ))'
[[ $rc = 0 && $out = 1 && -z $err ]]; chk cov.32
# unary ops on a constant operand — try_fold_unop (compile-time fold)
run_sash 'echo $(( ~5 ))'
[[ $rc = 0 && $out = -6 && -z $err ]]; chk cov.33
run_sash 'echo $(( !0 ))'
[[ $rc = 0 && $out = 1 && -z $err ]]; chk cov.34
# postfix inc/dec as a (( )) statement — incdec_to_ssa
run_sash 'int x=5; (( x++ )); echo $x'
[[ $rc = 0 && $out = 6 && -z $err ]]; chk cov.35
run_sash 'int x=5; (( x-- )); echo $x'
[[ $rc = 0 && $out = 4 && -z $err ]]; chk cov.36
# compound-assign statements — compound_assign_to_ssa + each arith op
run_sash 'int x=5; (( x += 3 )); echo $x'
[[ $rc = 0 && $out = 8 && -z $err ]]; chk cov.37
run_sash 'int x=5; (( x -= 3 )); echo $x'
[[ $rc = 0 && $out = 2 && -z $err ]]; chk cov.38
run_sash 'int x=5; (( x *= 3 )); echo $x'
[[ $rc = 0 && $out = 15 && -z $err ]]; chk cov.39
run_sash 'int x=6; (( x /= 3 )); echo $x'
[[ $rc = 0 && $out = 2 && -z $err ]]; chk cov.40
run_sash 'int x=7; (( x %= 3 )); echo $x'
[[ $rc = 0 && $out = 1 && -z $err ]]; chk cov.41
run_sash 'int x=5; (( x <<= 1 )); echo $x'
[[ $rc = 0 && $out = 10 && -z $err ]]; chk cov.42
run_sash 'int x=20; (( x >>= 2 )); echo $x'
[[ $rc = 0 && $out = 5 && -z $err ]]; chk cov.43
run_sash 'int x=5; (( x &= 3 )); echo $x'
[[ $rc = 0 && $out = 1 && -z $err ]]; chk cov.44
run_sash 'int x=5; (( x |= 2 )); echo $x'
[[ $rc = 0 && $out = 7 && -z $err ]]; chk cov.45
run_sash 'int x=5; (( x ^= 1 )); echo $x'
[[ $rc = 0 && $out = 4 && -z $err ]]; chk cov.46
# comma expression in $(( )) — comma_to_ssa (value is the last part)
run_sash 'int a=1; int b=2; echo $(( a, b ))'
[[ $rc = 0 && $out = 2 && -z $err ]]; chk cov.47
# comma-in-condition PASSING path: a non-final mutation then a final
# test — validate_arith_comma_in_condition accepts (cov.1-3 reject)
run_sash 'int a=0; if (( a += 1, a )); then echo y; fi'
[[ $rc = 0 && $out = y && -z $err ]]; chk cov.48
# cov.49-57 arithmetic ternary `cond ? a : b` — no feature spec exercises it,
# leaving native transform/codegen (SsaValue::Ternary / lower_ternary_value /
# lower_ternary_int) and the emit_c str/int ternary path untested. cov.49-56
# use simple branches; cov.57 pins lazy evaluation (both backends short-circuit
# like bash — the fix for the eager-eval bug).
run_sash 'int x=5; echo $(( x>0 ? 1 : 2 ))'
[[ $rc = 0 && $out = 1 && -z $err ]]; chk cov.49
run_sash 'int x=-5; echo $(( x>0 ? 1 : 2 ))'
[[ $rc = 0 && $out = 2 && -z $err ]]; chk cov.50
# expression branches (arith on each side)
run_sash 'int x=5; int y=$(( x>3 ? x*2 : x+1 )); echo $y'
[[ $rc = 0 && $out = 10 && -z $err ]]; chk cov.51
# falsy condition (0) selects the else branch
run_sash 'int x=0; echo $(( x ? 10 : 20 ))'
[[ $rc = 0 && $out = 20 && -z $err ]]; chk cov.52
# truthy (non-zero) condition selects the then branch
run_sash 'int x=7; echo $(( x ? 10 : 20 ))'
[[ $rc = 0 && $out = 10 && -z $err ]]; chk cov.53
# nested ternary in the then branch
run_sash 'int x=7; echo $(( x>5 ? (x>10 ? 3 : 2) : 1 ))'
[[ $rc = 0 && $out = 2 && -z $err ]]; chk cov.54
# ternary in an int-assignment (max of two)
run_sash 'int a=1; int b=2; int m=$(( a>b ? a : b )); echo $m'
[[ $rc = 0 && $out = 2 && -z $err ]]; chk cov.55
# `!=` condition (spaced) selecting the else branch
run_sash 'int x=0; echo $(( x != 0 ? 5 : 9 ))'
[[ $rc = 0 && $out = 9 && -z $err ]]; chk cov.56
# lazy eval: a div-by-zero in the UNTAKEN branch must NOT fire (bash
# short-circuits `?:`). x=0 → else → 42, no `division by 0` on stderr.
run_sash 'int x=0; echo $(( x>0 ? 10/x : 42 ))'
[[ $rc = 0 && $out = 42 && -z $err ]]; chk cov.57
# cov.58 the `command` builtin is not in sash-native (DD#23 — "command -v" is
# `[[ -x /path ]]`); any `command …` is rejected as dynamic execution (SB4001,
# check_dynexec), same class as eval/source. Pins the removal of the dead
# emit_command_builtin codegen (validation shadows it).
run_sash 'command /bin/echo hi'
[[ $rc != 0 && -z $out && $err == *SB4001* ]]; chk cov.58
# cov.59 the `$(< file)` slurp form (parse/expr.rs parse_slurp_form) parses,
# then reads a file → tainted → SB5020 (no feature spec uses `$(< …)`; the
# other `$(<…)` in the tree are bash harness code, not sash source).
run_sash 'echo $(< /etc/hostname)'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk cov.59
# cov.61 `for x in $(cmd)` → the DD#28/#40 pipe-while-read transform
# (control_flow.rs emit_cmdsubst_pipe_while_read / build_cmdsubst_stage /
# build_pipeline_stages_from_cmdsubst). `read` is line-based (one iteration for
# `echo a b c`, no word-split — a deliberate divergence from bash), and the
# tainted loop var is attested with a predicate. Native-only: the c backend does
# an extra empty EOF iteration here (task c-pipe-while-read-eof-extra-iter).
run_sash 'for x in $(echo a b c); do attest x [[ -n $x ]]; echo "$x"; done'
[[ $rc = 0 && $out = "a b c" && -z $err ]]; chk cov.61
# cov.62 a backtick inside "…" is a LITERAL char (lexer scan_dq_backtick emits a
# Literal, not a CommandSub): backticks are only top-level `$( )` sugar and have
# no special meaning in quotes (DG 2026-08-28). The command is never run — the
# backticks and their contents render verbatim. Works in both backends.
run_sash 'echo "x`/bin/echo BOOM`y"'
[[ $rc = 0 && $out = 'x`/bin/echo BOOM`y' && -z $err ]]; chk cov.62
# cov.63 ANSI-C octal escapes are REJECTED (DD#72 model, journal 371): bash's
# `\nnn`/`\0nnn` are archaic and ambiguous, and sash's own `\0NNN` counted
# digits differently from bash. Hex says it exactly. All backends.
run_sash "echo \$'\\x31\\x32\\x33'"
[[ $rc = 0 && $out = "123" && -z $err ]]; chk cov.63
# cov.64 a parenthesized `[[ ( … ) ]]` grouped test (parser parse_test_group).
# All backends.
run_sash '[[ ( -n x ) ]] && echo y'
[[ $rc = 0 && $out = y && -z $err ]]; chk cov.64
# cov.65-67 binary bitwise operators in $(( )) with runtime operands
# (codegen/arithmetic.rs emit_arith_op BitAnd/BitOr/BitXor → band/bor/bxor).
# The compound-assign forms (&=,|=,^=) are cov.44-46; the BINARY forms were
# untested. a=6,b=3 → &=2, |=7, ^=5.
run_sash 'int a=6; int b=3; echo $(( a & b ))'
[[ $rc = 0 && $out = 2 && -z $err ]]; chk cov.65
run_sash 'int a=6; int b=3; echo $(( a | b ))'
[[ $rc = 0 && $out = 7 && -z $err ]]; chk cov.66
run_sash 'int a=6; int b=3; echo $(( a ^ b ))'
[[ $rc = 0 && $out = 5 && -z $err ]]; chk cov.67
# cov.68-69 RUNTIME divide/modulo by zero — DD#25 (amended): every arithmetic
# fault aborts with `<argv0>: <what>` and exit 2. This used to write bash's
# "division by 0" to stderr, yield 0 and keep going, which made it the only
# arithmetic fault in the language that produced a value and continued. The
# divisor is a variable, so the folder cannot prove it and SB5077 does not
# apply; the check in codegen fires instead. (cov.57 only tested the
# ternary-UNTAKEN, non-firing case.)
run_sash 'int x=0; echo $(( 5 / x ))'
[[ $rc = 2 && -z $out && $rerr == *"division by zero"* ]]; chk cov.68
run_sash 'int x=0; echo $(( 5 % x ))'
[[ $rc = 2 && -z $out && $rerr == *"division by zero"* ]]; chk cov.69
# cov.70 non-final comma in a condition that is NOT a mutation → SB5007 via
# validate_arith_comma_in_condition (a bare name `a` as a non-final comma part).
# Distinct from cov.2 (`a==1, b=2`) whose final `b=2` mutation trips the earlier
# mutation-in-condition check first; here the non-final `a` reaches the comma
# validator's "must be a mutation" arm. Front-end reject — all modes.
run_sash 'int a=1; if (( a, a>0 )); then echo y; fi'
[[ $rc != 0 && -z $out && $err == *SB5007* ]]; chk cov.70
# cov.71 malformed `=~ s/…` substitution (two delimiters) → SB4032 via
# parse_regex_mutation/sed_malformed_err. Front-end reject — all modes.
run_sash 's="x"; s=~ s/a/b'
[[ $rc != 0 && -z $out && $err == *SB4032* ]]; chk cov.71
# cov.72 substring `${var:off:len}` → native codegen emit_string_op_expansion
# (the only ParamExpansion operator that still reaches codegen; DD#65). native.
run_sash 's="hello"; echo ${s:1:3}'
[[ $rc = 0 && $out = ell && -z $err ]]; chk cov.72
# cov.73 `keys` as the sole source of a for-in loop (the array-splat for-in
# path). native.
run_sash 'map m=([x]=1 [y]=2); for k in $(keys m); do echo $k; done'
[[ $rc = 0 && $out = $'x\ny' && -z $err ]]; chk cov.73
# cov.74 a compact (glued) dispatch operator `[[ $x ==b { … } ]]` → parser
# extract_compact_operator (operator-adjacent-to-pattern split). native.
run_sash 'x=b; [[ $x ==b { echo m } ]]'
[[ $rc = 0 && $out = m && -z $err ]]; chk cov.74
# cov.75 a predicate function whose body is a dispatch with a default arm where
# every arm returns → validate case_always_returns (definite-return, DD#74).
# native.
run_sash 'f() { [[ $1 == a { return 0 } || { return 1 } ]] }; if f a; then echo yes; fi'
[[ $rc = 0 && $out = yes && -z $err ]]; chk cov.75

# --- coverage grind (2026-08-29): drive never-run named fns to count>0 ---
# (Verified via cargo-llvm-cov per-function count: each spec below flips its
# named target 0->>0, except cov.79 which documents a reject path.)

# cov.76 a single-quoted run inside a `$( )` inside double quotes → the lexer's
# skip_balanced_squote skips it so an inner `'` can't miscount the `)`. The
# tainted cmdsub is then rejected (SB5020), but the lexer path is exercised.
run_sash 'echo "$(/bin/echo '\''q'\'')"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk cov.76

# cov.77 a nested double-quoted run inside the same → skip_balanced_dquote
# (honours `\` escapes).
run_sash 'echo "$(/bin/echo "q")"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk cov.77

# cov.78 `${x:-0}` on an int `x` triggers the default on the additive identity
# `0` (DG's identity-element semantic — the int analogue of "" for strings), not
# on "unset". So a non-zero `x` yields the value.
run_sash 'int x=5; int n=${x:-0}; echo $n'
[[ $rc = 0 && $out = 5 && -z $err ]]; chk cov.78

# cov.80 the same `:-` with `x == 0` (the identity element) → the default fires.
run_sash 'int x=0; echo ${x:-99}'
[[ $rc = 0 && $out = 99 && -z $err ]]; chk cov.80

# cov.81 identity-element default in a string context (`echo`), non-zero int →
# the value, not the default.
run_sash 'int x=5; echo ${x:-99}'
[[ $rc = 0 && $out = 5 && -z $err ]]; chk cov.81

# cov.82 an int `:-` default must be an int literal (DG 2026-08-29) — a string
# default is rejected at type_check (SB4030), so the disposition stays
# homogeneously int. All modes (front-end reject).
run_sash 'int x=5; echo ${x:-hello}'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk cov.82

# cov.83 an *empty* int default (`${x:-}`) is likewise rejected — 0 is the
# identity element (not "empty"), so there is no empty-string fallback for an int.
run_sash 'int x=5; echo ${x:-}'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk cov.83

# cov.84 the int-literal-default rule extends to an int-array *cell*
# (`${a[i]:-D}`): a non-literal default is rejected (SB4030). All modes.
run_sash 'int array a=(1 2 3); echo ${a[0]:-hi}'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk cov.84

# cov.85 an int-array cell with a literal default is accepted and lowers to the
# identity-element ternary.
run_sash 'int array a=(1 2 3); echo ${a[0]:-0}'
[[ $rc = 0 && $out = 1 && -z $err ]]; chk cov.85

# cov.79 a direct `base#digits` arith literal (`16#ff`) is rejected SB4035 (bash
# arbitrary base, DD#72-dropped) — the direct-arith path, distinct from the
# `${s:-16#ff}` default path (arithmetic.sh d4.7).
run_sash 'echo $(( 16#ff ))'
[[ $rc != 0 && -z $out && $err == *SB4035* ]]; chk cov.79

# cov.89 `[[ -t N ]]` isatty test (native emit_isatty_test).
# Captured stdout in the harness is not a tty → notty.
run_sash 'if [[ -t 1 ]] { echo tty } else { echo notty }'
[[ $rc = 0 && $out = notty && -z $err ]]; chk cov.89
# cov.90 a full-line comment inside a compiled program — comments are lexical
# trivia dropped by the Scanner (see spec/comments.sh); this pins that a
# standalone comment line still compiles and runs the following statement.
run_sash '# a comment
echo hi'
[[ $rc = 0 && $out = hi && -z $err ]]; chk cov.90

# cov.91 `$?` / `$(status)` with no preceding command → SB5012
# (find_dollar_question) — no feature spec reads status at program start.
run_sash 'echo $?'
[[ $rc != 0 && -z $out && $err == *SB5012* ]]; chk cov.91

# cov.92 a `$(random)` disposition default → SB5035 (expr_invokes_random,
# DD#78) — the PRNG side effect under eager default eval is rejected.
run_sash 'echo ${y:-$(random)}'
[[ $rc != 0 && -z $out && $err == *SB5035* ]]; chk cov.92

# cov.93 a tainted `$(arg N DEFAULT)` default in an assignment RHS → SB5020
# (check_arg_default_taint, DD#60/78). The `$(arg …)` source assignment is
# allowed, but the nested default is a read; `x=$(arg 1 cleanlit)` compiles.
run_sash 'argv 1 a; t=$(getenv HOME); x=$(arg 1 $t)' one
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk cov.93

# ── The removed-builtin table is gone (DG 2026-09-12) ───────────────
# Six bash words used to be intercepted in command position with bespoke
# diagnostics, which both reserved the names and made the unknown-command
# story non-uniform (`shift`, also removed, always got the generic SB5011).
# Now every one of them lands on SB5011, and all are usable as function names.

# cov.94 every removed builtin gives the SAME unknown-command error
for w in export declare typeset readonly select shift; do
    run_sash "$w x"
    [[ $rc != 0 && -z $out && $err == *SB5011* && $err == *"'$w'"* ]] || break
done
[[ $rc != 0 && $err == *SB5011* && $err == *"'shift'"* ]]; chk cov.94

# cov.95 ...and `select` is now usable as a user-defined function — the
# capability the table took away (it parsed, but was unusable at the call site)
run_sash 'select() {
  /bin/echo picked
}
select'
[[ $rc = 0 && $out = "picked" && -z $err ]]; chk cov.95

# cov.96 ...same for `export`
run_sash 'export() {
  /bin/echo exported
}
export'
[[ $rc = 0 && $out = "exported" && -z $err ]]; chk cov.96

# cov.97 ...and `readonly`
run_sash 'readonly() {
  /bin/echo ro
}
readonly'
[[ $rc = 0 && $out = "ro" && -z $err ]]; chk cov.97

# cov.98 `eval` is still rejected — SB4001, dynamic execution (validate, not
# the parser), and the message keeps the "no runtime interpreter" clause the
# parser table used to carry
run_sash 'eval x'
[[ $rc != 0 && -z $out && $err == *SB4001* && $err == *"runtime interpreter"* ]]; chk cov.98

# cov.99 `.` is still rejected too
run_sash '. f'
[[ $rc != 0 && -z $out && $err == *SB4001* ]]; chk cov.99

# cov.100 `eval` is a reject, not a reserved word: it cannot be redefined away
run_sash 'eval() {
  /bin/echo nope
}
eval'
[[ $rc != 0 && -z $out && $err == *SB4001* ]]; chk cov.100

# ── 2026-09-15 branch-coverage batch (journal 321). Each entry names the
# arm it reaches; all were 0-hit across a full native+c sweep. ──

# cov.106 absorb_text (parse/expr.rs) in OPERAND mode: `&` is NOT a connector
# inside `[[ ]]`, and unlike an assignment value it is not absorbed either —
# it terminates the word, so the `]]` never arrives.
run_sash '[[ a&b == x ]] && echo y'
[[ $rc != 0 && -z $out && $err == *SB4014* ]]; chk cov.106
# cov.107 a command substitution holding TWO statements — the `let [stmt]`
# single-statement bareword shortcut must decline.
run_sash 'x=$(echo a; echo b); echo "$x"'
[[ $rc = 0 && $out = "a
b" && -z $err ]]; chk cov.107
# cov.108 an EMPTY command substitution (zero statements) — the same shortcut
# declines from the other side.
run_sash 'x=$( ); echo "[$x]"'
[[ $rc = 0 && $out = "[]" && -z $err ]]; chk cov.108
# cov.109 an unbalanced `${a[0}` — capture_balanced_subscript finds no close.
run_sash 'int array a=(1 2); echo ${a[0}'
[[ $rc != 0 && -z $out && $err == *SB4047* ]]; chk cov.109
# cov.110 a non-identifier dynamic subscript `$a[$i-]` — dynamic_subscript_var
# declines, so the split-subscript gather backs off and `$a` stands alone.
run_sash 'int array a=(1 2); int i=1; echo $a[$i-]'
[[ $rc != 0 && -z $out && $err == *SB5064* ]]; chk cov.110
# cov.111 a BARE `@` is an ordinary word, not a splat (the `w.len() > 1` guard).
run_sash 'echo @'
[[ $rc = 0 && $out = "@" && -z $err ]]; chk cov.111
# cov.112 `@{…}` splat disposition on a non-identifier name.
run_sash 'int array a=(1 2); echo @{1bad:-x}'
[[ $rc != 0 && -z $out && $err == *SB5033* ]]; chk cov.112
# cov.113 the `[*]` half of the retired splat (the `[@]` half is specced; the
# `idx == "*"` arm was not).
run_sash 'int array a=(1 2); echo $a[*]'
[[ $rc != 0 && -z $out && $err == *SB4038* ]]; chk cov.113
# cov.114 trailing literal text after an unbraced subscript, unquoted.
run_sash 'int array a=(1 2); echo $a[0]tail'
[[ $rc = 0 && $out = "1tail" && -z $err ]]; chk cov.114
# cov.115 …and the same inside a double-quoted string (a separate scanner arm).
run_sash 'int array a=(1 2); echo "$a[0]tail"'
[[ $rc = 0 && $out = "1tail" && -z $err ]]; chk cov.115
# cov.116 the retired `[*]` splat inside a string.
run_sash 'int array a=(1 2); echo "$a[*]"'
[[ $rc != 0 && -z $out && $err == *SB4038* ]]; chk cov.116
# cov.117 a dynamic subscript whose variable starts with `_`.
run_sash 'int array a=(1 2); int _i=1; echo $a[$_i]'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk cov.117
# cov.118 the retired keys form spelled with `[*]`.
run_sash 'int array a=(1 2); echo ${!a[*]}'
[[ $rc != 0 && -z $out && $err == *SB4038* ]]; chk cov.118
# cov.119 postfix DECREMENT as a statement. Every `if *inc {"++"} else {"--"}`
# in parse/arith.rs had only ever taken the `++` side.
run_sash 'int i=5; (( i-- )); echo $i'
[[ $rc = 0 && $out = "4" && -z $err ]]; chk cov.119
# cov.120 prefix `--i` is rejected (the `--` spelling of the SB4020 message).
run_sash 'int i=5; (( --i )); echo $i'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk cov.120
# cov.121 a mutation inside `$(( ))` expression context, `--` spelling.
run_sash 'int i=5; echo $(( i-- ))'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk cov.121
# cov.122 an integer literal one past i64::MAX.
run_sash 'echo $(( 9223372036854775808 ))'
[[ $rc != 0 && -z $out && $err == *SB4026* ]]; chk cov.122
# cov.123 a malformed `0x` literal (DD#72 unsigned-prefix digit check).
run_sash 'echo $(( 0xzz ))'
[[ $rc != 0 && -z $out && $err == *SB4035* ]]; chk cov.123
# cov.124 a braced SUBSCRIPT inside arithmetic (try_braced_subscript).
run_sash 'int array a=(7 8); echo $(( ${a[0]} + 1 ))'
[[ $rc = 0 && $out = "8" && -z $err ]]; chk cov.124
# cov.125 …and an empty index there.
run_sash 'int array a=(7 8); echo $(( ${a[]} ))'
[[ $rc != 0 && -z $out && $err == *SB4041* ]]; chk cov.125
# cov.126 a comma sequence in VALUE context folds to its last part.
run_sash 'int i=1; echo $(( i , 2 ))'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk cov.126
# cov.127 a wholly empty `$(( ))` is zero (the all-separators early return).
run_sash 'echo $(( ))'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk cov.127
# cov.128 a C-style for loop that counts DOWN (the `--` update arm).
run_sash 'for (( j=2; j>0; j-- )); do echo $j; done'
[[ $rc = 0 && $out = "2
1" && -z $err ]]; chk cov.128
# cov.129 an UNQUOTED backtick is rejected (scan_backtick / skip_backtick_body);
# cov.62 covers the quoted form, which is verbatim text and never scanned.
run_sash 'echo `echo hi`'
[[ $rc != 0 && -z $out && $err == *SB5076* ]]; chk cov.129

# cov.130-133 BUG (consume_dq_arith_sub returned its partial text): an
# UNTERMINATED `$((` inside a double-quoted string ran off the end of the
# source and was then evaluated anyway — `echo "$(( 1 + 2"` silently printed
# 3. Every sibling construct already rejected this (bare `$(( 1 + 2`, `"${ s"`
# and `"$( cmd"` all raise SB4057/SB4020), so the in-`"…"` arith scanner was
# the one hole. It now records the construct as unterminated; the span points
# at the `$`. The valid forms below are the no-regression half: the `depth`
# bookkeeping (not a length test) now decides whether `))` gets skipped.
# cov.130 the bug: unterminated `$((` inside `"…"` is rejected
run_sash 'echo "$(( 1 + 2"'
[[ $rc != 0 && -z $out && $err == *SB4057* ]]; chk cov.130
# cov.131 a nested `(…)` group inside a quoted `$((…))` still closes correctly
run_sash 'echo "$(( (1 + 2) * 3 ))"'
[[ $rc = 0 && $out = 9 && -z $err ]]; chk cov.131
# cov.132 a nested `((…))` — depth really is counted, not just paired
run_sash 'echo "$(( ((1)) + 1 ))"'
[[ $rc = 0 && $out = 2 && -z $err ]]; chk cov.132
# cov.133 two quoted arith subs in one word — the second must start clean
run_sash 'echo "$(( 1 + 2 ))x$(( 3 ))"'
[[ $rc = 0 && $out = "3x3" && -z $err ]]; chk cov.133

# cov.134-139 fold_arith_op's logical and zero-divisor arms. `&&`/`||` over
# two literals had never been constant-folded at all (both outcomes of both
# operators were cold), and the div/mod-by-zero guards fold to 0 — matching
# the runtime's value for cov.68/69, though the fold emits no diagnostic.
run_sash 'echo $(( 1 && 1 ))'
[[ $rc = 0 && $out = 1 && -z $err ]]; chk cov.134
run_sash 'echo $(( 1 && 0 ))'
[[ $rc = 0 && $out = 0 && -z $err ]]; chk cov.135
run_sash 'echo $(( 0 || 1 ))'
[[ $rc = 0 && $out = 1 && -z $err ]]; chk cov.136
run_sash 'echo $(( 0 || 0 ))'
[[ $rc = 0 && $out = 0 && -z $err ]]; chk cov.137
# cov.138-139 a divisor the FOLDER can prove is zero is a compile-time error
# (SB5077), not a runtime abort: the compiler already knows the program cannot
# run, so it says so before emitting a binary. Reach is exactly what the folder
# proves — literals and operators over literals, through parens (cov.157) — and
# NOT const-propagation through variables, which is why cov.68/69 above still
# reach the runtime. Keeping a literal `/0` out of the emitted C also matters
# for `--emit-c`: gcc replaces the whole enclosing function with `ud2`.
run_sash 'echo $(( 1 / 0 ))'
[[ $rc != 0 && -z $out && $err == *SB5077* ]]; chk cov.138
run_sash 'echo $(( 1 % 0 ))'
[[ $rc != 0 && -z $out && $err == *SB5077* ]]; chk cov.139
# cov.139a-b the non-zero divisors still fold normally
run_sash 'echo $(( 9 / 2 ))'
[[ $rc = 0 && $out = 4 && -z $err ]]; chk cov.155
run_sash 'echo $(( 9 % 2 ))'
[[ $rc = 0 && $out = 1 && -z $err ]]; chk cov.156
# cov.157 the folder proves through parens and nested operators, so a zero
# divisor spelled `(2 - 2)` is the same error as one spelled `0`.
run_sash 'echo $(( 1 + 8 / (4 - 4) ))'
[[ $rc != 0 && -z $out && $err == *SB5077* ]]; chk cov.157
# cov.158 the `(( ))` STATEMENT context reaches the check too — it is wired
# into validate_arith_context as well as the `$(( ))` value path, and a rule
# that answered to only one of the two spellings would be the same
# spelling-dependent inconsistency SB5077 exists to remove.
run_sash 'int x=1; (( x = x / 0 )); echo $x'
[[ $rc != 0 && -z $out && $err == *SB5077* ]]; chk cov.158
# cov.159 a modulo by a proven-zero divisor inside a ternary BRANCH. The check
# descends into both arms, so this is rejected even though the arm is not
# taken at runtime — a literal fault written in dead code is still a fault
# (compare SB5050, which rejects a dead variable).
run_sash 'echo $(( 1 ? 5 : 10 % 0 ))'
[[ $rc != 0 && -z $out && $err == *SB5077* ]]; chk cov.159

# cov.309-313 the folder's reach includes UNARY operators over constants.
# It did not: `const_eval_arith` had arms for Literal/Paren/BinOp and nothing
# else, so `$(( 5 / (1 + -1) ))` compiled and faulted at runtime while
# `$(( 5 / (2 - 2) ))` was SB5077 -- the same fault at two different times,
# decided by whether a minus sign appeared. That is precisely the
# spelling-dependent inconsistency cov.158 says SB5077 exists to remove.
# `UnaryOp` carries no side effect (`x++`/`++x` are separate IR variants), so
# folding it is safe.
run_sash 'echo $(( 5 / (1 + -1) ))'
[[ $rc != 0 && -z $out && $err == *SB5077* ]]; chk cov.309
# cov.310 a negated zero is still zero
run_sash 'echo $(( 5 / -0 ))'
[[ $rc != 0 && -z $out && $err == *SB5077* ]]; chk cov.310
# cov.311 logical not of a non-zero constant is zero
run_sash 'echo $(( 5 / !5 ))'
[[ $rc != 0 && -z $out && $err == *SB5077* ]]; chk cov.311
# cov.312 bitwise not of -1 is zero
run_sash 'echo $(( 5 / ~-1 ))'
[[ $rc != 0 && -z $out && $err == *SB5077* ]]; chk cov.312
# cov.313 the converse must hold: a unary divisor that is NOT zero still
# compiles and folds, so the wider reach does not over-reject
run_sash 'echo $(( 5 / -1 ))'
[[ $rc = 0 && $out = "-5" && -z $err ]]; chk cov.313

# cov.314-324 constant folding of the comparison, bitwise and power
# operators. `fold_arith_op` has an arm for each, and not one of them had a
# literal-operand spec -- the suite exercised these operators only with
# variable operands, which take the runtime path instead. Values checked
# against bash.
run_sash 'echo $(( 1 != 2 ))'
[[ $rc = 0 && $out = 1 && -z $err ]]; chk cov.314
run_sash 'echo $(( 1 < 2 ))'
[[ $rc = 0 && $out = 1 && -z $err ]]; chk cov.315
run_sash 'echo $(( 2 <= 2 ))'
[[ $rc = 0 && $out = 1 && -z $err ]]; chk cov.316
run_sash 'echo $(( 3 > 1 ))'
[[ $rc = 0 && $out = 1 && -z $err ]]; chk cov.317
run_sash 'echo $(( 3 >= 4 ))'
[[ $rc = 0 && $out = 0 && -z $err ]]; chk cov.318
run_sash 'echo $(( 1 << 3 ))'
[[ $rc = 0 && $out = 8 && -z $err ]]; chk cov.319
run_sash 'echo $(( 16 >> 2 ))'
[[ $rc = 0 && $out = 4 && -z $err ]]; chk cov.320
run_sash 'echo $(( 12 & 10 ))'
[[ $rc = 0 && $out = 8 && -z $err ]]; chk cov.321
run_sash 'echo $(( 12 | 3 ))'
[[ $rc = 0 && $out = 15 && -z $err ]]; chk cov.322
run_sash 'echo $(( 12 ^ 10 ))'
[[ $rc = 0 && $out = 6 && -z $err ]]; chk cov.323
run_sash 'echo $(( 2 ** 10 ))'
[[ $rc = 0 && $out = 1024 && -z $err ]]; chk cov.324

# cov.140-142 check_len_operand's rejection arms (type_check.rs)
# cov.140 `len` with no argument at all → arity, not the operand check
run_sash 'echo $(len)'
[[ $rc != 0 && -z $out && $err == *SB4045* ]]; chk cov.140
# cov.141 `args` is the function-local argument array, absent at top level
run_sash 'echo $(len args)'
[[ $rc != 0 && -z $out && $err == *SB5024* ]]; chk cov.141
# cov.142 a scalar operand is redirected to `strlen`
run_sash 'str s=abc; echo $(len s)'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk cov.142

# cov.143-145 join_intrinsic's argument-shape rejections
# cov.143 the separator is fixed at compile time, so an expansion is refused
run_sash 'array a=(p q); str s=-; echo $(join a $s)'
[[ $rc != 0 && -z $out && $err == *SB5074* ]]; chk cov.143
# cov.144 too many arguments
run_sash 'array a=(p q); echo $(join a , x)'
[[ $rc != 0 && -z $out && $err == *SB5040* ]]; chk cov.144
# cov.145 none at all
run_sash 'array a=(p q); echo $(join)'
[[ $rc != 0 && -z $out && $err == *SB5040* ]]; chk cov.145

# cov.146-147 is_identifier_or_subscript's two rejection arms — a malformed
# assignment target is neither an assignment nor a runnable command (SB5011).
# cov.146 a name that is not an identifier, with a well-formed subscript
run_sash '9x[0]=1; echo done'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk cov.146
# cov.147 a valid name with an EMPTY subscript
run_sash 'a[]=1; echo done'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk cov.147

# cov.148-149 check_unset_args: `unset` is collection-only (DD#30), so a
# scalar — declared or not — is redirected to `name=""`.
run_sash 'str s=a; unset s; echo done'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk cov.148
run_sash 'unset nosuchvar'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk cov.149

# cov.150-153 arith_brief's PostIncDec arms (validate.rs) — the text SB5007
# quotes back at you. A BARE `if (( x++ ))` reports `x = ...` (the Assign
# arm), so these had to be reached through a BinOp, where the inc/dec node
# survives into the message. Both directions were cold: neither `++` nor `--`
# had ever been rendered by this function.
run_sash 'int x=0; while (( x++ < 3 )); do echo $x; done'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"x++"* ]]; chk cov.150
run_sash 'int x=0; if (( 1 + x-- )); then echo y; fi'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *"x--"* ]]; chk cov.151
# cov.152 a mutation is legal in the STATEMENT form — the decrement half,
# which is the side every `if *inc` in the tree had been skipping
run_sash 'int x=0; (( x-- )); echo $x'
[[ $rc = 0 && $out = "-1" && -z $err ]]; chk cov.152
# cov.153 a comma expression whose FINAL part is a pure test is accepted:
# the mutation is hoisted, so validate_arith_comma_in_condition allows it
run_sash 'int x=0; if (( x++, x < 5 )); then echo y; fi'
[[ $rc = 0 && $out = y && -z $err ]]; chk cov.153

# cov.154 `$$` as a `[[ ]]` operand — skip_dollar_var's special-character set
# (`@ * # ? ! $`) after a `$`. `$?`/`$#` are rejected there (SB5012/SB5024),
# so `$$` is the spelling that actually reaches the scan.
run_sash 'if [[ $$ == 0 ]]; then echo y; fi'
[[ $rc = 0 && -z $out && -z $err ]]; chk cov.154

# cov.160-167 the C-style `for` header is an arithmetic STATEMENT context and
# must answer to the same rules as `(( … ))`. It did not: `lower_c_style_for`
# called `parse_arith_str` for each of the three slots and then DISCARDED the
# `Err` with `if let Ok(...)`. That did not merely lose a diagnostic — it built
# a different program. A malformed condition fell back to `true_cmd`, so
# `for (( i=0; i <* 3; i++ ))` compiled to an INFINITE LOOP; a malformed update
# was dropped, so the counter never advanced (also infinite); a malformed init
# was dropped, so the loop silently started from the variable's default. The
# identical typo in a bare `(( … ))` has always been a hard SB4020 (cov.166).
# All three discarded-Err branches were cold in the branch-coverage measure at
# `b9ca092` — the cold branch WAS the bug.
run_sash 'for (( i=0; i <* 3; i++ )); do echo $i; done'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk cov.160
run_sash 'for (( i=0; i < 3; i +*+ )); do echo $i; done'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk cov.161
run_sash 'for (( i=*0; i < 3; i++ )); do echo $i; done'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk cov.162
# cov.163-165 DD#31 is categorical — "Only postfix `x++` and `x--`. No prefix
# forms." — and names no exemption for a `for` header. Prefix was nonetheless
# accepted in the init and step slots (it compiled and even ran correctly),
# while the SAME spelling in `$(( ))` or `(( ))` was SB4020 and in the
# condition slot was SB5007 for an unrelated reason. Three slots, three
# dispositions, one construct. All three are SB4020 now, the same code and the
# same message as everywhere else.
run_sash 'for (( i=0; i < 3; ++i )); do echo $i; done'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"prefix ++i"* ]]; chk cov.163
run_sash 'for (( ++i; i < 3; i++ )); do echo $i; done'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"prefix ++i"* ]]; chk cov.164
run_sash 'for (( i=0; --i < 3; )); do echo $i; done'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"prefix --i"* ]]; chk cov.165
# cov.166 the control: the same garbage in a bare `(( … ))` always errored.
run_sash 'int i=0; (( i <* 3 ))'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk cov.166
# cov.167 and the legitimate headers must still lower — an empty slot is the
# deliberate `for (( ;; ))` spelling and is the ONLY thing that may still
# become `true`. Comma slots (DD#31) keep working too.
run_sash 'for (( i=0, j=10; i < 3; i++, j-- )); do echo "$i $j"; done'
[[ $rc = 0 && $out = "0 10
1 9
2 8" && -z $err ]]; chk cov.167

# ── validate.rs cold-branch batch: for-words and ternary ────────

# cov.168 `$@` in a for-loop word. `check_for_word` rejects every
# `SimpleExpansion` except `@` and `*`, which get the DD#42 message naming
# `$(argv 1)` — the two arms of that guard had never both been reached.
run_sash 'for x in $@; do echo "$x"; done'
[[ $rc != 0 && -z $out && $err == *SB5045* && $err == *'$@'* ]]; chk cov.168

# cov.169 `$*` takes the other arm of the same guard
run_sash 'for x in $*; do echo "$x"; done'
[[ $rc != 0 && -z $out && $err == *SB5045* && $err == *'$*'* ]]; chk cov.169

# cov.170 sash globs are `*`-only: a bracket class is SB5006
run_sash 'for x in a[bc]d; do echo "$x"; done'
[[ $rc != 0 && -z $out && $err == *SB5006* ]]; chk cov.170

# cov.171 …and so is `?`
run_sash 'for x in a?d; do echo "$x"; done'
[[ $rc != 0 && -z $out && $err == *SB5006* ]]; chk cov.171

# cov.172 an UNBALANCED bracket is not a class and must stay a literal — the
# check is `contains('[') && contains(']')`, and only this shape reaches the
# second half with the first already true
run_sash 'for x in a[bc; do echo "$x"; done'
[[ $rc = 0 && $out = 'a[bc' && -z $err ]]; chk cov.172

# cov.173 a for-word that CONCATENATES a literal and an expansion: the glob
# scan has a separate `Concat` arm that walks the parts, and it only sees a
# non-Literal part in this shape
run_sash 's="x"; for w in pre$s*post; do echo "$w"; done'
[[ $rc = 0 && $out = 'prex*post' && -z $err ]]; chk cov.173

# cov.174 a mutation under a ternary in a condition — the TRUE arm. It cannot
# take effect reliably (the arm may go unevaluated), so SB5007.
run_sash 'int x=0; if (( 1 ? (x=2) : 3 )); then echo "$x"; fi'
[[ $rc != 0 && -z $out && $err == *SB5007* ]]; chk cov.174

# cov.175 …and the FALSE arm, which is the other operand of the same `||`
run_sash 'int x=0; if (( 1 ? 3 : (x=2) )); then echo "$x"; fi'
[[ $rc != 0 && -z $out && $err == *SB5007* ]]; chk cov.175

# cov.176 prefix `++` in `$(( … ))`. `(( ++x ))` is pinned at cov.23 and the
# three `for` slots at cov.163-165; this is the fourth and last parse entry
# point, and together they are the only thing enforcing DD#31's "no prefix
# forms" — the validator's duplicate of the rule was unreachable and removed.
run_sash 'int x=0; echo $(( ++x ))'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"prefix ++x"* ]]; chk cov.176

# cov.177 the predicate-return scan (SB4037) also walks the END block, so a
# program needs both a predicate use and an `END { }` for that arm to run
run_sash 'f() { return 0; }
if f; then echo y; fi
END { echo bye; }'
[[ $rc = 0 && $out == $'y\nbye' && -z $err ]]; chk cov.177

# cov.178 a function that is NOT used as a predicate sits in the same fn_defs
# list and must be skipped — `g` never returns and that is fine
run_sash 'g() { echo hi; }
f() { return 0; }
if f; then g; fi'
[[ $rc = 0 && $out = 'hi' && -z $err ]]; chk cov.178

# cov.179 a predicate whose body is an `if` with BOTH arms returning is
# definite-returning — the else-arm check passes and the then-arm is reached
run_sash 'f() { if true; then return 0; else return 1; fi }
if f; then echo y; fi'
[[ $rc = 0 && $out = 'y' && -z $err ]]; chk cov.179

# cov.180 …and the same shape with a then-arm that does NOT return. The else
# arm alone is not enough; this is the other half of that `&&`.
run_sash 'f() { if true; then echo x; else return 1; fi }
if f; then echo y; fi'
[[ $rc != 0 && -z $out && $err == *SB4037* ]]; chk cov.180

# cov.181 a predicate whose body is a dispatch with NO default arm: the
# uncovered subject falls through to no return at all
run_sash 'f() { s=$(argv 1); [[ $s == a { return 0; } ]]; }
if f; then echo y; fi'
[[ $rc != 0 && -z $out && $err == *SB4037* ]]; chk cov.181

# cov.182 SB5069 names the FIRST `read` in the statement, once, even when the
# condition holds two — the walk latches on the first hit
run_sash 'while read A | read B; do echo x; done'
[[ $rc != 0 && -z $out && $err == *SB5069* && $(grep -c SB5069 <<<"$err") = 1 ]]; chk cov.182

# cov.183 an inline `*` glob in COMMAND-argument position (DD#69/SB5029) when
# the word is a concatenation — the bare-literal form is the common one, this
# reaches the `Concat` arm of the same scan
run_sash 's="x"; echo $s*'
[[ $rc != 0 && -z $out && $err == *SB5029* ]]; chk cov.183

# cov.184 a non-numeric `int` initializer is not an out-of-range literal — the
# range check must decline it and let the type error stand
run_sash 'int x=abc'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err != *SB4026* ]]; chk cov.184

# cov.185 an embedded subscript assignment in a condition `(( … ))` — the
# array analogue of the embedded `++` case, and a separate check
run_sash 'array a=(1 2); if (( (a[0]=1) )); then echo y; fi'
[[ $rc != 0 && -z $out && $err == *SB5007* && $err == *SB4020* ]]; chk cov.185

# cov.186 a mutation in a ternary CONDITION (not either arm) — cov.174/175
# cover the arms, and the three tests are the three operands of one `||`
run_sash 'int x=0; if (( (x=2) ? 1 : 3 )); then echo y; fi'
[[ $rc != 0 && -z $out && $err == *SB5007* ]]; chk cov.186

# cov.187 SB5012 collects spans by walking every expansion in the statement,
# so a statement mixing an ordinary `$s` with an unavailable `$?` must skip
# the one and flag the other
run_sash 's="x"; echo $s $?'
[[ $rc != 0 && -z $out && $err == *SB5012* ]]; chk cov.187

# cov.188 `$*` inside `(( … ))` — the arith walker has its own `$@`/`$*` arm,
# separate from the expression one, and the `*` half is a distinct branch
run_sash 'if (( $* )); then echo y; fi'
[[ $rc != 0 && -z $out && $err == *SB5045* ]]; chk cov.188

# cov.189 `args` written by a DECLARATION rather than a plain assignment —
# SB5052 covers both spellings and they are separate arms
run_sash 'array args=(1 2)'
[[ $rc != 0 && -z $out && $err == *SB5052* ]]; chk cov.189

# cov.190 DD#78: a `:-` default is part of the expansion's value, so a CLEAN
# name with a TAINTED default is still tainted. Without this the array would
# take external data without a `read`.
run_sash 's=$(getenv HOME)
c="q"
array a=("${c:-$s}")
echo "${a[0]}"'
[[ $rc != 0 && -z $out && $err == *SB5021* ]]; chk cov.190

# cov.191 a variable written TWICE and never read reports once, not twice —
# the dead-variable pass dedups by name
run_sash 'x="a"
x="b"
echo hi'
[[ $rc != 0 && -z $out && $err == *SB5050* && $(grep -c SB5050 <<<"$err") = 1 ]]; chk cov.191

# cov.192 `$(arg)` with no index at all — the arity guard is `is_empty() ||
# len > 2`, and every existing spec exercises the upper half
run_sash 'echo $(arg)'
[[ $rc != 0 && -z $out && $err == *SB5036* ]]; chk cov.192

# cov.193 the tab-stripping heredoc `<<-` is a distinct scanner path from
# `<<` (cov.14) and takes the same SB5002 rejection.
run_sash '/bin/cat <<-EOF
	hi
	EOF'
[[ $rc != 0 && -z $out && $err == *SB5002* ]]; chk cov.193
# cov.194 a backtick that is never closed. 67.4/67.5 cover well-formed
# backticks; this is the scanner's unterminated path, which must still reach
# the DD#67 rejection rather than swallowing the rest of the source.
run_sash '/bin/echo `abc'
[[ $rc != 0 && -z $out && $err == *SB5076* ]]; chk cov.194
# cov.195 a backtick with nothing after it but a newline — the shortest
# unterminated form, and a separate arm from cov.194's non-empty body.
run_sash '/bin/echo `
'
[[ $rc != 0 && -z $out && $err == *SB5076* ]]; chk cov.195

# cov.196 `<<` as the very last bytes of the source — the heredoc sentinel
# scanner's end-of-input guard, reached before it can look for a `-`.
run_sash '/bin/cat <<'
[[ $rc != 0 && -z $out && $err == *SB5002* ]]; chk cov.196
# cov.197 a `\` as the last byte of a backtick body. The body scanner steps
# over `\`` so an escaped inner backtick does not end the scan early; this is
# the same check with nothing left to escape. Still SB5076.
run_sash '/bin/echo `\'
[[ $rc != 0 && -z $out && $err == *SB5076* ]]; chk cov.197

# cov.198 `${#}` — the `#` operator with an empty operand. The length-sugar
# scanner's `len <= 1` guard; `${#x}` (cov-covered) takes the other path.
run_sash '/bin/echo ${#}'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk cov.198
# cov.199 `${##}` — a doubled `#`, which is the prefix-strip operator, not
# length sugar. Same scanner, the other early-out.
run_sash '/bin/echo ${##}'
[[ $rc != 0 && -z $out && $err == *SB4034* ]]; chk cov.199
# cov.200 `${s@Uu}` — a two-character `@` transform. `try_at_transform` only
# recognizes a single letter, so this falls through to the generic
# unrecognized-content reject rather than SB4015.
run_sash 's=a
/bin/echo ${s@Uu}'
[[ $rc != 0 && -z $out && $err == *SB4047* ]]; chk cov.200
# cov.201 a bare `${a[@]}` with no trailing operator — the DD#75 §4 retired
# splat. The operator-bearing form (`${a[@]/x/y}`) takes the other branch.
run_sash 'array a=(1 2)
/bin/echo ${a[@]}'
[[ $rc != 0 && -z $out && $err == *SB4038* ]]; chk cov.201
# cov.202 a map element word that opens `[` but never closes it.
run_sash 'map m=([a=1)'
[[ $rc != 0 && -z $out && $err == *SB4031* ]]; chk cov.202
# cov.203 an empty map key `[]=` — closes, but there is nothing between the
# brackets, so it is not the `[key]=value` form either.
run_sash 'map m=([]=1)'
[[ $rc != 0 && -z $out && $err == *SB4031* ]]; chk cov.203
# cov.204 an empty `"$()"` — zero statements, so the `$(status)`/`$(getcpid)`
# bareword rewrite declines. (It requires exactly one.)
run_sash '/bin/echo "$()"'
[[ $rc = 0 && -z $out && -z $err ]]; chk cov.204
# cov.205 a `$( )` body that is a declaration, not a command: the rewrite's
# second guard. Quoted, because the unquoted path rewrites while scanning and
# never reaches this one.
run_sash '/bin/echo "$(int x=1)"'
[[ $rc = 0 && -z $out && -z $err ]]; chk cov.205
# cov.206 a `$( )` whose command name is an expansion rather than a literal —
# DD#23 rejects it, and the rewrite declines before that.
run_sash 's=/bin/true
/bin/echo "$($s)"'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk cov.206

# --- parse/parser.rs cold branches: error arms that the suite never reached.
# Each was probe-verified with contrib/cov-probe.sh. ---

# cov.207 DD#68 brace form: an `elif` with NO condition at all. The `{ … }`
# that follows is read as the condition, and a brace group is not one. This is
# the elif site specifically — the `if` site has its own copy.
run_sash 'x=1
if [[ $x == 1 ]] { echo one } elif { echo two }'
[[ $rc != 0 && -z $out && $err == *SB4027* ]]; chk cov.207

# cov.208 the same shape, well-formed, so the empty-condition arm is the only
# difference between passing and failing.
run_sash 'x=2
if [[ $x == 1 ]] { echo one } elif [[ $x == 2 ]] { echo two }'
[[ $rc = 0 && $out = "two" && -z $err ]]; chk cov.208

# cov.209 DD#66: the `[[ ]]` dispatch form is a STATEMENT. Used where a
# boolean is wanted — a `while` condition — it is rejected rather than
# silently evaluated for its exit status.
run_sash 'x=hi
while [[ $x == y { echo a } ]]; do echo z; done'
[[ $rc != 0 && -z $out && $err == *SB4028* ]]; chk cov.209

# cov.210 dispatch needs `SUBJECT ==|=~ PATTERN`. A unary test is not a
# subject/pattern pair, so there is nothing to dispatch on.
run_sash 'x=hi
[[ -n $x { echo a } ]]'
[[ $rc != 0 && -z $out && $err == *SB4028* ]]; chk cov.210

# cov.211 and a binary test with the wrong operator. `!=` has no dispatch
# meaning — the diagnostic names the operator it found.
run_sash 'x=hi
[[ $x != y { echo a } ]]'
[[ $rc != 0 && -z $out && $err == *SB4028* ]]; chk cov.211

# cov.212 DD#61 attest-all lead: every token before `in` must be a binder
# name. A `;` there ends the statement, so `in` never arrives.
run_sash 'map m=([a]=1)
attest k ; in m [[ -n $k ]]'
[[ $rc != 0 && -z $out && $err == *SB4014* ]]; chk cov.212

# cov.213 `!` binds to the final stage of a pipeline only. Negating the whole
# pipeline would negate a plumbing status a reviewer cannot trace back to a
# command, so it is rejected with the predicate-wrapping escape hatch.
#
# This reaches SB4040 through the FIRST operand of the `pipeline_operand` test
# (a trailing `|`). The second operand — an already-folded multi-stage
# pipeline — short-circuits away and appears unreachable: `!` parses its
# operand with `dispatch_statement`, which by contract does not fold `|`, so
# the statement it returns is never a multi-command pipeline. Left in place as
# a defensive check; see notes/status.md.
run_sash '! echo hi | /bin/cat'
[[ $rc != 0 && -z $out && $err == *SB4040* ]]; chk cov.213

# cov.214 a `for … in` word list that runs into end-of-input before any body.
# The word-collecting loop has an `at_eof()` guard that the suite never
# reached — every other `for` in the corpus is newline- or `do`-terminated.
# (Only the native harness reproduces true EOF: the c harness appends a
# newline. The diagnostic is the same either way.)
run_sash 'for x in a'
[[ $rc != 0 && -z $out && $err == *SB4014* ]]; chk cov.214

# cov.215 a bare word where an optional `-> int` return type would go. The
# annotation parser has to decline on a word that is neither `->` nor `-`,
# and then the body check reports the real problem.
run_sash 'fn n() foo { return 7 }'
[[ $rc != 0 && -z $out && $err == *SB4014* ]]; chk cov.215

# cov.216 `<&` with a non-fd target. `>&foo` was covered; the input-dup
# direction picks a different arrow for the message and was not.
run_sash '/bin/cat <& foo'
[[ $rc != 0 && -z $out && $err == *SB4048* ]]; chk cov.216

# cov.217 a value intrinsic WITH arguments, mid-`$( )` body (DD#111). The
# diagnostic elides the argument list, so this is a different formatting arm
# from the no-argument case in 23.25.
run_sash 'echo hi
x=$(echo a; getenv HOME)'
[[ $rc != 0 && -z $out && $err == *SB5073* ]]; chk cov.217

# cov.218 a NON-literal command head in a multi-statement `$( )` body. DD#23
# rejects it first; the value-intrinsic check has to decline on a head it
# cannot read as a name.
run_sash 'v=/bin/true
echo hi
x=$(echo a; $v arg)'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk cov.218

# cov.219–cov.222 cover `reject_tainted_into_array`'s value walk
# (`expr_uses_tainted`), which nothing reached with a `${…}` default or a
# subscript. cov.221/cov.222 cover the `Subscript` arm added by DD#114.

# cov.219 a tainted name inside a `${x:-D}` default, as a collection element.
# DD#78 makes the default part of the value, so the read taints the element.
run_sash 'x=$(printf 1)
array b=("${x:-d}")
echo "${b[0]}"'
[[ $rc != 0 && -z $out && $err == *SB5021* ]]; chk cov.219

# cov.220 …and the mirror: a CLEAN name whose default holds the tainted read,
# so the walk has to look past the name it first tests.
run_sash 'int y=1
x=$(printf 1)
array b=("${y:-$x}")
echo "${b[0]}"'
[[ $rc != 0 && -z $out && $err == *SB5021* ]]; chk cov.220

# cov.221 a CLEAN subscript read as a collection element — the container is
# not tainted, so the walk goes on to check the index (DD#114).
run_sash 'array a=(1 2)
array b=("${a[0]}")
echo "${b[0]}"'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk cov.221

# cov.222 …and a TAINTED container, which stops at the name. `read ARR` is
# the one bulk external read left after DD#99.
run_sash '/bin/echo a | read ARR
array b=("${ARR[0]}")
echo "${b[0]}"'
[[ $rc != 0 && -z $out && $err == *SB5021* ]]; chk cov.222

# ── brace-less disposition operand: the `$…` scan (parse/lexer.rs
# `skip_dollar_var`, reached only from `scan_braceless_operand`). Every
# spec below drives a DIFFERENT arm of that scan; the compile outcome is
# incidental, the lexer path is the point.

# cov.223 a multi-char identifier after `$` — the scan's inner run loop has
# to advance past the first byte, which a 1-char name never makes it do.
run_sash 'array a=()
str xy=q
/bin/echo @{a:-$xy}'
[[ $rc = 0 && $out = "q" && -z $err ]]; chk cov.223

# cov.224 …and a `_`-led identifier, which reaches the scan's second
# alternative (`c == b'_'`); an alphabetic lead short-circuits past it.
run_sash 'array a=()
str _z=q
/bin/echo @{a:-$_z}'
[[ $rc = 0 && $out = "q" && -z $err ]]; chk cov.224

# cov.225 a single-char SPECIAL (`$#`) in the same position. The scan
# consumes it before the `#` word-terminator check ever runs, so the
# operand is `$#` and not an empty operand plus a comment.
run_sash 'array a=()
/bin/echo @{a:-$#}'
[[ $rc != 0 && -z $out && $err == *SB5024* ]]; chk cov.225

# ── for-loop iterable classification (transform/control_flow.rs
# `detect_array_iter`). Nothing but a splat had ever been iterated, so the
# non-splat paths through the classifier were untravelled.

# cov.226 a double-quoted single-part word: the classifier must unwrap the
# quotes and re-test the inside, then find a plain expansion there.
run_sash 'str v=a
for x in "$v"; do echo $x; done'
[[ $rc = 0 && $out = "a" && -z $err ]]; chk cov.226

# cov.227 …and a double-quoted word with TWO parts, which is not
# unwrappable — the concatenation is the single iteration item.
run_sash 'str v=a
str w=b
for x in "$v$w"; do echo $x; done'
[[ $rc = 0 && $out = "ab" && -z $err ]]; chk cov.227

# cov.228 an expansion carrying a non-splat operator (`:-`). It is a
# parameter expansion, so it reaches the splat tests, and fails both.
run_sash 'str v=a
for x in ${v:-d}; do echo $x; done'
[[ $rc = 0 && $out = "a" && -z $err ]]; chk cov.228

# ── positional / special-variable rejections, each from a distinct rule.
# These are the one-line programs the feature specs never bother to write.

# cov.229 `$0` is rejected everywhere (SB5017), not folded into the
# function-local SB5024 family below.
run_sash 'echo "$0"'
[[ $rc != 0 && -z $out && $err == *SB5017* ]]; chk cov.229

# cov.230 `$@` at TOP LEVEL — SB5045, and the directive names `$(argv 1)`.
run_sash 'echo "${@}"'
[[ $rc != 0 && -z $out && $err == *SB5045* && $err == *'argv 1'* ]]; chk cov.230

# cov.231 the same rule inside a FUNCTION gives a different directive
# (`@args`), because the fix differs by scope.
run_sash 'fn f() {
  echo "${@}"
}
f'
[[ $rc != 0 && -z $out && $err == *SB5045* && $err == *@args* ]]; chk cov.231

# cov.232 `$*` takes the same path as `$@` and names itself correctly.
run_sash 'echo "${*}"'
[[ $rc != 0 && -z $out && $err == *SB5045* && $err == *'$*'* ]]; chk cov.232

# cov.233 a numeric positional at top level → SB5024, directing to $(arg N).
run_sash 'echo "$1"'
[[ $rc != 0 && -z $out && $err == *SB5024* && $err == *'arg 1'* ]]; chk cov.233

# cov.234 `$#` takes the SAME rule but a different directive ($(argc)).
run_sash 'echo "$#"'
[[ $rc != 0 && -z $out && $err == *SB5024* && $err == *argc* ]]; chk cov.234

# ── command-head resolution (validate `is_valid_command` / resolve.rs).

# cov.235 a bare relative path is SB5051, not a generic not-found — the
# head contains `/`, so `is_valid_command` defers and `resolve` owns it.
run_sash 'foo/bar'
[[ $rc != 0 && -z $out && $err == *SB5051* ]]; chk cov.235

# cov.236 …and the `./x` spelling reaches the same rule.
run_sash './x'
[[ $rc != 0 && -z $out && $err == *SB5051* ]]; chk cov.236

# cov.237 a head containing `*` is likewise deferred by `is_valid_command`,
# and then fails PATH resolution with SB5011. Two passes emit this exact
# message; this one comes from resolve, which is why it is not SB5051.
run_sash 'echo*'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk cov.237

# ── subscript spellings that are accepted. Quoting a literal map key is
# legal both ways, and an arithmetic index is evaluated, not stringified.

# cov.238 a double-quoted literal map key.
run_sash 'map m=([k]=v)
echo "${m["k"]}"'
[[ $rc = 0 && $out = "v" && -z $err ]]; chk cov.238

# cov.239 a single-quoted literal map key reaches the same lookup.
run_sash 'map m=([k]=v)
echo "${m['\''k'\'']}"'
[[ $rc = 0 && $out = "v" && -z $err ]]; chk cov.239

# cov.240 an arithmetic subscript is folded to its value, so `$((1))`
# selects element 1 rather than being read as the key "$((1))".
run_sash 'array a=(x y)
echo "${a[$((1))]}"'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk cov.240

# cov.241 `$(len args)` inside a function — `check_len_operand`'s slot branch
# (DD#75 §5: `args` is in scope in every function body).
run_sash 'f() {
  echo "n=$(len args)"
}
f a b'
[[ $rc = 0 && $out = "n=2" && -z $err ]]; chk cov.241

# cov.242 `len` with no operand — the arity check, not the operand check
run_sash 'echo "$(len)"'
[[ $rc != 0 && -z $out && $err == *SB4045* ]]; chk cov.242

# cov.243 `len` of a literal — it resolves a *variable name*, so a quoted
# string names nothing
run_sash 'echo "$(len "abc")"'
[[ $rc != 0 && -z $out && $err == *SB5062* ]]; chk cov.243

# cov.244 `unset` on an int is the scalar reject (DD#30) — the branch that
# evaluates the second half of the vtype test
run_sash 'int i=5
echo $i
unset i'
[[ $rc != 0 && $err == *SB4030* ]]; chk cov.244

# cov.245-246 pre-increment/decrement requires a variable, and the operator
# named in the diagnostic follows the `inc` flag
run_sash 'int x=1
echo $(( ++5 ))'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"after ++"* ]]; chk cov.245
run_sash 'int x=1
echo $(( --5 ))'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"after --"* ]]; chk cov.246

# cov.247 the `--` side of the nested-mutation diagnostic. `brief_arith`
# renders the offending sub-expression; only the `++` spelling was ever
# exercised, so the `--` arm of its PostIncDec case never ran.
run_sash 'int x=0
int y=1
(( x = y-- ))'
[[ $rc != 0 && -z $out && $err == *SB4020* && $err == *"mutation y--"* ]]; chk cov.247

# cov.248-249 `$(status …)` where `status` is the first word but `)` does
# not immediately follow. `parse_status_word` only ever saw the bare
# `$(status)` spelling, so its whole argument loop was unreached.
run_sash 'echo $(status /bin/true)'
[[ $rc != 0 && -z $out && $err == *SB4045* ]]; chk cov.248
run_sash '/bin/true
echo $(status; echo b)'
[[ $rc != 0 && -z $out && $err == *SB5073* ]]; chk cov.249

# cov.250-252 a redirect target that is served by intrinsic codegen rather
# than a VarSlot. `$$` (pid) and `$!` (last background child) are readable
# first-class values — `str s=$$; /bin/echo hi > $s` always worked — but the
# slot-based redirect-target lowering could not see them, so using them
# DIRECTLY as a target went wrong in two different ways: `> $$` was skipped by
# the `var_name == "$"` guard in init_positional_params, got no slot at all and
# raised the unresolved-target ICE, while `> $!` had no guard, got an EMPTY
# slot, and silently redirected to the empty filename that
# `spill_scalar_redirect_targets` exists to prevent. Both now spill to a
# synthetic scalar like any other non-slot target.
run_sash '/bin/echo hi > $$
/bin/echo "pid=$$"'
rpid=${out#pid=}
[[ $rc == 0 && -n $rpid && -f $rpid && $(cat "$rpid") == hi ]]; chk cov.250
# the same target inside a concatenation — target_needs_spill recurses into
# DoubleQuoted parts, so `"$$.log"` takes the spill path too
run_sash '/bin/echo hi > "$$.log"
/bin/echo "pid=$$"'
rpid=${out#pid=}
[[ $rc == 0 && -n $rpid && -f $rpid.log && $(cat "$rpid.log") == hi ]]; chk cov.251
# `$!` names the background child, matching `str s=$!; > $s`
run_sash '/bin/sleep 0.2 &
/bin/echo hi > $!
/bin/echo "child=$!"'
rchild=${out#child=}
[[ $rc == 0 && -n $rchild && -f $rchild && $(cat "$rchild") == hi ]]; chk cov.252

# cov.253-255 the prefix `++`/`--` rejection lives in `parse_arith_str`, the one
# funnel every route into arithmetic passes through. It used to sit in the two
# entry points instead, which these three routes bypass: a `$(( … ))` inside a
# double-quoted string, the same nested in `${x:-…}`, and the bare-subscript
# index re-parse. All three still raised SB4020 — but from `validate`, where the
# prefix form was caught only by riding along in the *generic* mutation arm, so
# the identical construct got a vaguer message inside quotes than outside. These
# assert the specific message, not just the code, so a slide back to the generic
# one fails here.
run_sash 'int x=1; /bin/echo "$(( ++x ))"'
[[ $rc != 0 && -z $out && $err == *SB4020*prefix*++x*not\ allowed* ]]; chk cov.253
run_sash 'int x=1; /bin/echo "${x:-$(( ++x ))}"'
[[ $rc != 0 && -z $out && $err == *SB4020*prefix*++x*not\ allowed* ]]; chk cov.254
run_sash 'int a[3]; int x=1; (( y = a[++x] ))'
[[ $rc != 0 && -z $out && $err == *SB4020*prefix*++x*not\ allowed* ]]; chk cov.255

# cov.256-261 a `(( … ))` statement whose value embeds a *compound* assignment.
# `hoist_mutation_node` applies the store and leaves a read of the target
# behind; only its plain-`Assign` arm had coverage, so every line of the
# `CompoundAssign` arm was cold despite the form working.
run_sash 'int a=1; (( (a+=2) )); echo $a'
[[ $rc == 0 && $out == 3 ]]; chk cov.256
run_sash 'int a=1; (( 1 + (a+=2) )); echo $a'
[[ $rc == 0 && $out == 3 ]]; chk cov.257
run_sash 'int a=10; (( (a-=3) )); echo $a'
[[ $rc == 0 && $out == 7 ]]; chk cov.258
run_sash 'int a=1; (( (a+=2), 1 )); echo $a'
[[ $rc == 0 && $out == 3 ]]; chk cov.259
run_sash 'int a=1; int b=2; (( (a+=2) + (b+=3) )); echo "$a $b"'
[[ $rc == 0 && $out == "3 5" ]]; chk cov.260
run_sash 'int a=1; (( -(a+=2) )); echo $a'
[[ $rc == 0 && $out == 3 ]]; chk cov.261

# cov.262-263 the two forms the hoist deliberately cannot apply — an embedded
# `++`/`--` and an embedded `a[i]` assignment — are rejected by
# `check_arith_test_placement` before the transform sees them. These are what
# keep `ARITH_TEST_HOISTABLE_INVARIANT` true, i.e. what makes the ICE arm in
# `hoist_mutation_node` unreachable rather than merely untested.
run_sash 'int a=1; (( 1 + (a++) ))'
[[ $rc != 0 && -z $out && $err == *SB4020*embedded*++* ]]; chk cov.262
run_sash 'int array v=(1 2 3); (( 1 + (v[0]=9) ))'
[[ $rc != 0 && -z $out && $err == *SB4020*embedded*'a[i]'* ]]; chk cov.263

# cov.264-270 SB5067: a function may not taint a global. Both paths that
# raise it — a bulk fill (`read`/`readline`/`argv`/`mapfile`) whose target is
# a global, and a plain assignment of a taint source to one — had **no spec
# at all**, so the entire diagnostic was uncovered. Taint is the safety
# property the language exists to enforce; it should not rest on an untested
# check.
run_sash 'str g=""; f() { read g; }; f'
[[ $rc != 0 && $err == *SB5067*taints\ the\ global* ]]; chk cov.264
run_sash 'str g=""; f() { readline g; }; f'
[[ $rc != 0 && $err == *SB5067* ]]; chk cov.265
run_sash 'argv g; f() { argv g; }; f'
[[ $rc != 0 && $err == *SB5067* ]]; chk cov.266
run_sash 'str g=""; f() { mapfile g; }; f'
[[ $rc != 0 && $err == *SB5067* ]]; chk cov.267
run_sash 'str g=""; f() { g=$(getenv HOME); }; f'
[[ $rc != 0 && $err == *SB5067*assignment* ]]; chk cov.268
run_sash 'str g=""; f() { g=$(/bin/date); }; f'
[[ $rc != 0 && $err == *SB5067*assignment* ]]; chk cov.269
# The same taint into a *local* is the allowed shape — attest it and use it.
run_sash 'f() { str l=$(getenv HOME); attest l [[ -n $l ]]; echo ok; }; f'
[[ $rc == 0 && $out == ok && $err != *SB5067* ]]; chk cov.270

# cov.271-275 SB4020 through the DD#93 attest carve-out. `validate` exempts an
# attest's `(( TARGET = EXPR ))` predicate from `check_arith_value_mutation`,
# and the predicate is an `ExprKind::Arithmetic`, not a `StmtKind::ArithTest`,
# so `check_arith_test_placement` does not see it either. That leaves the
# parser's `reject_nested_mutation` as the only gate on a mutation nested in
# the RHS. It used to ask a local copy of the mutation predicate that stopped
# at `BinOp`/`Paren` and omitted `SubscriptAssign`, so each shape below parsed
# clean and then hit the `unreachable!()` in `transform::expr_handlers` — an
# ICE, not a diagnostic. Every node kind the canonical walker descends into
# gets a spec so a future partial re-derivation cannot pass.
run_sash 'str s=$(arg 1); int n=$(( ${s:-0} )); int c=0; int d=1; if attest n (( c = n ? (d++) : 1 )); then echo $c $d; fi'
[[ $rc != 0 && -z $out && $err == *SB4020*mutation* ]]; chk cov.271
run_sash 'str s=$(arg 1); int n=$(( ${s:-0} )); int c=0; int d=1; if attest n (( c = n + -(d++) )); then echo $c $d; fi'
[[ $rc != 0 && -z $out && $err == *SB4020*mutation* ]]; chk cov.272
run_sash 'str s=$(arg 1); int n=$(( ${s:-0} )); int c=0; int d=1; int array v=(1 2); if attest n (( c = n + v[d++] )); then echo $c $d; fi'
[[ $rc != 0 && -z $out && $err == *SB4020*mutation* ]]; chk cov.273
run_sash 'str s=$(arg 1); int n=$(( ${s:-0} )); int c=0; int d=1; int array v=(1 2); if attest n (( c = n + (v[0] = 3) )); then echo $c $d; fi'
[[ $rc != 0 && -z $out && $err == *SB4020*mutation* ]]; chk cov.274
# The shape the old local copy did catch — a plain `BinOp` operand. Keeps the
# regression guard honest: the fix must not narrow what already worked.
run_sash 'str s=$(arg 1); int n=$(( ${s:-0} )); int c=0; int d=1; if attest n (( c = n + (d++) )); then echo $c $d; fi'
[[ $rc != 0 && -z $out && $err == *SB4020*mutation* ]]; chk cov.275
# Widening the walk into `Ternary`/`Subscript`/`UnaryOp` must not turn a
# mutation-free operand under those nodes into a false positive.
run_sash 'str s=$(arg 1); int n=$(( ${s:-0} )); int c=0; int d=1; int array v=(5 6); if attest n (( c = n ? v[d] : -d )); then echo $c; fi' 7
[[ $rc == 0 && $out == 6 && $err != *SB4020* ]]; chk cov.276
run_sash 'str s=$(arg 1); int n=$(( ${s:-0} )); int c=0; int d=1; if attest n (( c = n + -(d) )); then echo $c; fi' 7
[[ $rc == 0 && $out == 6 && $err != *SB4020* ]]; chk cov.277

# cov.278-282: reads inside an attest's `(( … ))` predicate. The arm used to
# collect them with `ir::walk_arith_var_reads`, which answers DD#77 D1's
# "which bare int operands cross from string?" — not "which names does this
# read?". It skips a subscript's subject name and a sexpr callee name, and
# checks no name for definedness. So a predicate that plainly reads `d` or `v`
# was rejected with a false SB5050 (cov.278-279), an undefined name raised
# nothing and silently read as 0 (cov.280-281), and an undefined array walked
# through validate into codegen and ICEd (cov.282). The arm now uses
# `check_arith_reads`, the same collector a plain `(( … ))` statement uses.
run_sash 'str s=$(arg 1); int n=$(( ${s:-0} )); int c=0; int d=1; if attest n (( c = n + (d) )); then echo $c; fi' 7
[[ $rc == 0 && $out == 8 && $err != *SB5050* ]]; chk cov.278
run_sash 'str s=$(arg 1); int n=$(( ${s:-0} )); int c=0; int d=1; int array v=(5 6); if attest n (( c = n + v[d] )); then echo $c; fi' 7
[[ $rc == 0 && $out == 13 && $err != *SB5050* ]]; chk cov.279
run_sash 'str s=$(arg 1); int n=$(( ${s:-0} )); int c=0; if attest n (( c = n + zz )); then echo $c; fi' 7
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk cov.280
run_sash 'str s=$(arg 1); int n=$(( ${s:-0} )); int c=0; int array v=(5 6); if attest n (( c = n + v[zz] )); then echo $c; fi' 7
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk cov.281
run_sash 'str s=$(arg 1); int n=$(( ${s:-0} )); int c=0; if attest n (( c = n + vv[0] )); then echo $c; fi' 7
[[ $rc != 0 && -z $out && $err == *SB5031* && $err != *ICE* ]]; chk cov.282
# The attest target is introduced by the attest, not read — it must not be
# reported undefined now that the predicate goes through the real collector.
run_sash 'str s=$(arg 1); int n=$(( ${s:-0} )); int c=0; if attest n (( c = n )); then echo $c; fi' 7
[[ $rc == 0 && $out == 7 && $err != *SB5031* ]]; chk cov.283

# cov.284-297: the lexing that made deleting 27 `LogosTok` variants safe.
# `Scanner::next_token` hands logos exactly ten bytes (newline # ; | & ( ) { }
# >), so a pattern starting with any other byte could never fire. The 22
# keyword variants and `[[`, `]]`, `<<<`, `+=`, `!` were therefore dead — a
# silent second answer to "what is a keyword?" beside `classify_word_token`'s
# real one. These pin the hand-rolled paths that actually produce them, so a
# future migration of keyword promotion under logos cannot quietly lose one.
run_sash 'if (( 1 )); then echo a; elif (( 1 )); then echo b; else echo c; fi'
[[ $rc == 0 && $out == a ]]; chk cov.284
run_sash 'if (( 0 )); then echo a; elif (( 1 )); then echo b; else echo c; fi'
[[ $rc == 0 && $out == b ]]; chk cov.285
run_sash 'int i=0; int s=0; while (( i < 3 )); do (( s = s + i )); (( i = i + 1 )); done; echo $s'
[[ $rc == 0 && $out == 3 ]]; chk cov.286
run_sash 'int i=0; int s=0; until (( i >= 3 )); do (( s = s + i )); (( i = i + 1 )); done; echo $s'
[[ $rc == 0 && $out == 3 ]]; chk cov.287
run_sash 'str r=""; for x in a b c; do r+=$x; done; echo $r'
[[ $rc == 0 && $out == abc ]]; chk cov.288
# `case`/`esac` are retired (DD, SB4029) but must still LEX, or the diagnostic
# that retires them could not fire at all.
run_sash 'str s=hi; case $s in hi) echo y;; *) echo n;; esac'
[[ $rc != 0 && -z $out && $err == *SB4029* ]]; chk cov.289
run_sash 'fn g() { echo AB; }; g'
[[ $rc == 0 && $out == AB ]]; chk cov.290
run_sash 'function h() { echo CD; }; h'
[[ $rc == 0 && $out == CD ]]; chk cov.291
run_sash 'int n=5; str s=abc; const int K=7; echo $n $s $K'
[[ $rc == 0 && $out == "5 abc 7" ]]; chk cov.292
run_sash 'array a=(x y); map m; m[k]=v; echo ${a[1]}${m[k]}'
[[ $rc == 0 && $out == yv ]]; chk cov.293
run_sash 'str s=$(arg 1); int n=$(( ${s:-0} )); int c=0; if attest n (( c = n )); then echo $c; fi' 7
[[ $rc == 0 && $out == 7 ]]; chk cov.294
# DD#56 `END { }` — the one keyword the parser intercepts before dispatch.
run_sash 'END { echo bye; }'
[[ $rc == 0 && $out == bye ]]; chk cov.295
# `[[` / `]]` (scan_lbracket/scan_rbracket) and `!` (scan_bang_or_neq).
run_sash '[[ abc == abc ]] && ! [[ x == y ]] && echo ok'
[[ $rc == 0 && $out == ok ]]; chk cov.296
# `+=` (scan_plus_or_word).
run_sash 'str s=ab; s+=cd; echo $s'
[[ $rc == 0 && $out == abcd ]]; chk cov.297
# `<<<` (scan_lt). Retired like `case` (SB4050), and for the same reason it
# must still lex: the diagnostic that retires it cannot fire otherwise.
run_sash 'str s=abc; cat <<< $s'
[[ $rc != 0 && -z $out && $err == *SB4050* ]]; chk cov.298

# cov.299-304: every reserved word must lower to a literal when used as a bare
# word. `literal_token_text` carried a duplicate of `keyword_to_str`'s token
# list and the copy was one short — `Tok::Str` was in neither, so the bare word
# `str` was an SB4014 parse error in every word context while all 21 of its
# siblings worked. The list now lives in `keyword_to_str` alone. cov.299-301
# sweep all 22 spellings so a future omission cannot hide.
run_sash 'echo if then else elif fi while until do done'
[[ $rc == 0 && $out == "if then else elif fi while until do done" ]]; chk cov.299
run_sash 'echo for in case esac fn function str int const'
[[ $rc == 0 && $out == "for in case esac fn function str int const" ]]; chk cov.300
run_sash 'echo array map attest END'
[[ $rc == 0 && $out == "array map attest END" ]]; chk cov.301
# The three non-argument word contexts the omission also broke.
run_sash 'str s=str; echo $s'
[[ $rc == 0 && $out == str ]]; chk cov.302
run_sash '[[ str == str ]] && echo y'
[[ $rc == 0 && $out == y ]]; chk cov.303
run_sash 'for x in str; do echo $x; done'
[[ $rc == 0 && $out == str ]]; chk cov.304
# A reserved word being usable as a bare word must not stop it introducing its
# own construct — the parser decides that before word parsing is reached.
run_sash 'if (( 1 )); then echo if; fi'
[[ $rc == 0 && $out == if ]]; chk cov.305

# cov.306-308: transform-time compile errors must gate EVERY backend, not just
# native. The transform used to run three times per compile — once purely for
# these diagnostics, once in `compile_native` for the taint gate, and a third
# time inside `compile_to_object`, which re-parsed the source from scratch.
# Collapsing that to one run moved the diagnostic gate, so pin that the
# alt-output flags still reject what native rejects. Invoked directly rather
# than through run_sash because the flags are the point (journal 348).
for _f in --emit-ir --manifest --emit-c; do
    ${SASH:-sash} "$_f" -c 'const int K=1; K=2; echo $K' >/dev/null 2>alt.err
    _arc=$?
    _aerr=$(cat alt.err); rm -f alt.err
    case $_f in
        --emit-ir)  _id=cov.306 ;;
        --manifest) _id=cov.307 ;;
        *)          _id=cov.308 ;;
    esac
    [[ $_arc != 0 && $_aerr == *SB4044* ]]; chk $_id
done

# cov.325 `${v@1}` — a single-character `@` transform whose character is not a
# letter. The transform reader requires an ASCII letter, so this falls through
# to the unrecognized-content reject, exactly as the two-character `${s@Uu}`
# does in cov.200. Bash calls it a bad substitution too.
run_sash 'str v=abc
echo ${v@1}'
[[ $rc != 0 && -z $out && $err == *SB4047* ]]; chk cov.325

# cov.326 an unterminated subscript in the body. The `${…}` scanner leaves
# `[@U` as the operator remainder, whose last `@` is followed by a lone
# letter — so without the `[@` guard it would read as an `@U` transform on a
# name that never carried one. Bash: bad substitution.
run_sash 'array arr=(p q)
echo ${arr[@U}'
[[ $rc != 0 && -z $out && $err == *SB4047* ]]; chk cov.326

# cov.327-330 the compiler-internal SIGIL namespace vs. user names.
# `BuiltinCmd::from_name` maps nine `__`-prefixed names to internal opcodes
# (`__negate_test_result` for `!`, `__table_delete` for `unset m[k]`, …). The
# compiler BUILDS those by enum and spells them with `sigil_str`, so those nine
# match arms were reachable only from SOURCE — a user writing the name — and no
# spec did. That is a namespace-collision surface: if the classification won
# over the user's own definition, `__table_delete() { … }` would lower to a
# table delete. It does not; the function wins. Pin it.
# cov.327 all nine sigil names are usable as ordinary function names
run_sash '__negate_test_result() { echo a; }
__set_test_from_status() { echo b; }
__let_exit() { echo c; }
__subshell() { echo d; }
__store_bg_pid() { echo e; }
__trap_signal() { echo f; }
__signal_flag() { echo g; }
__table_delete() { echo h; }
__table_delete_idx() { echo i; }
__negate_test_result; __set_test_from_status; __let_exit
__subshell; __store_bg_pid; __trap_signal
__signal_flag; __table_delete; __table_delete_idx'
[[ $rc = 0 && $out = "a
b
c
d
e
f
g
h
i" && -z $err ]]; chk cov.327
# cov.328 a sigil name with NO definition is an ordinary unknown command
# (SB5011) — the classification never makes one callable by itself
run_sash '__table_delete x'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk cov.328
# cov.329 a sigil-named function survives a pipeline stage, where the C
# backend re-classifies the name on its own path (emit_pipeline_stage)
run_sash '__subshell() { echo s; }
__subshell | /usr/bin/tr s S'
[[ $rc = 0 && $out = "S" && -z $err ]]; chk cov.329
# cov.330 args and exit status propagate normally through a sigil-named fn
run_sash '__let_exit() { echo "got $1"; return 3; }
__let_exit hi; echo $?'
[[ $rc = 0 && $out = "got hi
3" && -z $err ]]; chk cov.330

# cov.331-334 `$(getppid)` and `$(random)` captured to a variable. NOTE: these
# were written to close the cold `getppid`/`random` arms of
# `try_nullary_intrinsic`, and they do NOT reach them — measured 2026-09-21.
# An assignment `a=$(getppid)` is int-lowered exactly like the arithmetic
# spellings, so it lands in `int_intrinsic_i64`, not in the `emit_int_to_str`
# string path. The hypothesis in the original comment was right about *which*
# two lowerings exist and wrong about which one this spelling picks. The
# spelling that actually reaches the string arm is the UNASSIGNED one —
# `echo $(getppid)` / `"$(getppid)"` — which is cov.335-338 below.
# They are kept because the semantics they pin (stability, positivity,
# variation, non-negativity) are worth pinning on the int path too.
# Assert SEMANTICS, not just "it printed digits" — nondeterminism is why these
# went unspec'd, and the deterministic properties are what pin a real bug.
# cov.331 getppid is STABLE within a run, and is not getpid
run_sash 'a=$(getppid)
b=$(getppid)
c=$(getpid)
[[ $a = $b ]] && echo stable
[[ $a = $c ]] || echo distinct'
[[ $rc = 0 && $out = "stable
distinct" && -z $err ]]; chk cov.331
# cov.332 getppid is a positive integer (string form round-trips to int)
run_sash 'a=$(getppid)
int n=${a:-0}
(( n > 0 )) && echo positive'
[[ $rc = 0 && $out = "positive" && -z $err ]]; chk cov.332
# cov.333 random actually varies — two draws differ. A stuck generator
# returning a constant is the failure this catches.
run_sash 'a=$(random)
b=$(random)
[[ $a = $b ]] || echo differ'
[[ $rc = 0 && $out = "differ" && -z $err ]]; chk cov.333
# cov.334 random is NON-NEGATIVE. Guaranteed, not luck: sash_random masks
# with `& i64::MAX` to clear the sign bit (DG 2026-07-07, `$(random)` range),
# so a leading `-` here would be a real regression in the runtime.
run_sash 'r=$(random)
[[ $r != -* ]] && echo nonneg'
[[ $rc = 0 && $out = "nonneg" && -z $err ]]; chk cov.334

# cov.335-338 the SAME two intrinsics in true string context — an unassigned
# `$(getppid)` / `$(random)`, which is the spelling that reaches the
# `emit_int_to_str` arms of `try_nullary_intrinsic` (measured: cov.331-334
# above reach `int_intrinsic_i64` instead). `getpid` and `epochseconds` were
# hot here only because other specs spell them this way.
# cov.335 the two lowerings must AGREE. `$(getppid)` through the string path
# and through the int path are the same syscall; a divergence here would be a
# backend bug of exactly the shape that the int/string split invites, and it
# is deterministic despite the value being unpredictable.
run_sash 'a=$(getppid)
echo $(getppid)
echo $a'
[[ $rc = 0 && -z $err ]] && [[ $(echo "$out"|sed -n 1p) = $(echo "$out"|sed -n 2p) ]] \
  && [[ $out =~ ^[0-9]+$'\n'[0-9]+$ ]]; chk cov.335
# cov.336 same agreement inside an interpolation, where the capture is spliced
# into a larger string rather than being the whole word.
run_sash 'a=$(getppid)
echo "p=$a"
echo "q=$(getppid)"'
[[ $rc = 0 && -z $err && $out =~ ^p=([0-9]+)$'\n'q=([0-9]+)$ ]] \
  && [[ ${BASH_REMATCH[1]} = ${BASH_REMATCH[2]} ]]; chk cov.336
# cov.337 string-path random is NON-NEGATIVE — same `& i64::MAX` guarantee as
# cov.334, asserted on the other lowering.
run_sash 'echo "r=$(random)"'
[[ $rc = 0 && -z $err && $out =~ ^r=[0-9]+$ ]]; chk cov.337
# cov.338 string-path random varies between draws (stuck-generator guard).
run_sash 'echo $(random)
echo $(random)'
[[ $rc = 0 && -z $err ]] && [[ $(echo "$out"|sed -n 1p) != $(echo "$out"|sed -n 2p) ]]; chk cov.338

set -e
tally
