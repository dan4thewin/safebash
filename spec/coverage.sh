#!/bin/bash
# spec/coverage.sh — tests for reachable compiler paths that no feature-
# oriented spec happens to exercise, found via LLVM coverage (journal 188+).
# Only genuinely-new (0 prior run_sash) reachable rejections go here; shadowed/
# unreachable code is deleted instead of tested (DG 2026-08-26).
set -eu
. common.sh
setup coverage
plan=89
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
# cov.7 `read -f` (field separator) with a single target var → SB5016
run_sash 'read -f x < /etc/hostname'
[[ $rc != 0 && -z $out && $err == *SB5016* ]]; chk cov.7
# cov.8 a `${x@OP}` transform is rejected → SB4015
run_sash 'x=hi; echo ${x@Q}'
[[ $rc != 0 && -z $out && $err == *SB4015* ]]; chk cov.8
# cov.9 the removed `case` construct → SB4029
run_sash 'case $x in a) echo m;; esac'
[[ $rc != 0 && -z $out && $err == *SB4029* ]]; chk cov.9
# cov.10 a tainted value placed into an array via subscript assign → SB5021
run_sash 'array a=(); a[0]=$(getenv HOME)'
[[ $rc != 0 && -z $out && $err == *SB5021* ]]; chk cov.10
# cov.11 `source` / `.` is rejected (fixed-at-compile-time) → SB4002
run_sash 'source f'
[[ $rc != 0 && -z $out && $err == *SB4002* ]]; chk cov.11
# cov.12 indirect expansion `${!x}` is rejected → SB4003
run_sash 'x=y; echo ${!x}'
[[ $rc != 0 && -z $out && $err == *SB4003* ]]; chk cov.12
# cov.13 a `{ }` group cannot be a condition → SB4027
run_sash 'if { echo a; }; then echo hi; fi'
[[ $rc != 0 && -z $out && $err == *SB4027* ]]; chk cov.13
# cov.14 heredocs are rejected (use a herestring) → SB5002
run_sash 'cat <<EOF'
[[ $rc != 0 && -z $out && $err == *SB5002* ]]; chk cov.14
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
# cov.20 `read` with an unknown flag → SB5015
run_sash 'read -z x < /etc/hostname'
[[ $rc != 0 && -z $out && $err == *SB5015* ]]; chk cov.20
# cov.21 quoting an array splat is not allowed → SB5026
run_sash 'echo "$(argv)"'
[[ $rc != 0 && -z $out && $err == *SB5026* ]]; chk cov.21
# cov.22 `eval` (dynamic execution) is rejected → SB4001
run_sash 'eval "echo hi"'
[[ $rc != 0 && -z $out && $err == *SB4001* ]]; chk cov.22
# cov.23 a prefix `++`/`--` in `(( ))` is an arith syntax error → SB4020
run_sash 'int x=0; (( ++x ))'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk cov.23
# cov.24 `export` is rejected (parser) → SB5001
run_sash 'export A=1'
[[ $rc != 0 && -z $out && $err == *SB5001* ]]; chk cov.24
# cov.25 compact dispatch with the `=~` operator (no spaces) — existing specs
# only exercise compact `==`; this covers the `=~` compact-parse path
if in_modes native,c; then
    run_sash '[[abc=~b]] && echo match'
    [[ $rc = 0 && $out = match && -z $err ]]; chk cov.25
else
    skip_chk cov.25
fi
# cov.26 `>&N` dup redirect (emit_dup_output) — only the harness/converter use
# it; no spec exercises the compiled codegen path. `echo hi >&2` → stderr.
if in_modes native; then
    run_sash 'echo hi >&2'
    [[ $rc = 0 && -z $out && $rerr = hi ]]; chk cov.26
else
    skip_chk cov.26
fi
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
if in_modes native; then
    run_sash 'f=rvfile; echo hi > $f; cat $f'
    [[ $rc = 0 && $out = hi && -z $err ]]; chk cov.27
else
    skip_chk cov.27
fi
# cov.28 append (`>>`) to a variable-expansion target
if in_modes native; then
    run_sash 'f=rvfile2; echo a > $f; echo b >> $f; cat $f'
    [[ $rc = 0 && $out = $'a\nb' && -z $err ]]; chk cov.28
else
    skip_chk cov.28
fi
# cov.29-48 arithmetic-operator coverage: no feature spec exercises these
# operators with a variable operand, leaving transform/expr_handlers.rs
# (unaryop_to_ssa / try_fold_unop / incdec_to_ssa / compound_assign_to_ssa /
# comma_to_ssa) and validate_arith_comma_in_condition's passing path
# untested. All compile+run identically in native and c.
if in_modes native,c; then
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
else
    for n in 29 30 31 32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48; do
        skip_chk cov.$n
    done
fi
# cov.49-57 arithmetic ternary `cond ? a : b` — no feature spec exercises it,
# leaving native transform/codegen (SsaValue::Ternary / lower_ternary_value /
# lower_ternary_int) and the emit_c str/int ternary path untested. cov.49-56
# use simple branches; cov.57 pins lazy evaluation (both backends short-circuit
# like bash — the fix for the eager-eval bug).
if in_modes native,c; then
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
else
    for n in 49 50 51 52 53 54 55 56 57; do
        skip_chk cov.$n
    done
fi
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
# tainted loop var is blessed with a predicate. Native-only: the c backend does
# an extra empty EOF iteration here (task c-pipe-while-read-eof-extra-iter).
if in_modes native; then
    run_sash 'for x in $(echo a b c); do bless x [[ -n $x ]]; echo "$x"; done'
    [[ $rc = 0 && $out = "a b c" && -z $err ]]; chk cov.61
else
    skip_chk cov.61
fi
# cov.62 a backtick inside "…" is a LITERAL char (lexer scan_dq_backtick emits a
# Literal, not a CommandSub): backticks are only top-level `$( )` sugar and have
# no special meaning in quotes (DG 2026-08-28). The command is never run — the
# backticks and their contents render verbatim. Works in all three backends.
run_sash 'echo "x`/bin/echo BOOM`y"'
[[ $rc = 0 && $out = 'x`/bin/echo BOOM`y' && -z $err ]]; chk cov.62
# cov.63 ANSI-C `$'\nnn'` octal escapes (lexer scan_ansi_c_octal): each `\ooo`
# is decoded to its byte. `$'\061\062\063'` → "123". All backends.
run_sash "echo \$'\\061\\062\\063'"
[[ $rc = 0 && $out = "123" && -z $err ]]; chk cov.63
# cov.64 a parenthesized `[[ ( … ) ]]` grouped test (parser parse_test_group).
# All backends.
run_sash '[[ ( -n x ) ]] && echo y'
[[ $rc = 0 && $out = y && -z $err ]]; chk cov.64
# cov.65-67 binary bitwise operators in $(( )) with runtime operands
# (codegen/arithmetic.rs emit_arith_op BitAnd/BitOr/BitXor → band/bor/bxor).
# The compound-assign forms (&=,|=,^=) are cov.44-46; the BINARY forms were
# untested. a=6,b=3 → &=2, |=7, ^=5. native+c.
if in_modes native,c; then
    run_sash 'int a=6; int b=3; echo $(( a & b ))'
    [[ $rc = 0 && $out = 2 && -z $err ]]; chk cov.65
    run_sash 'int a=6; int b=3; echo $(( a | b ))'
    [[ $rc = 0 && $out = 7 && -z $err ]]; chk cov.66
    run_sash 'int a=6; int b=3; echo $(( a ^ b ))'
    [[ $rc = 0 && $out = 5 && -z $err ]]; chk cov.67
else
    skip_chk cov.65; skip_chk cov.66; skip_chk cov.67
fi
# cov.68-69 runtime divide/modulo by zero (codegen/arithmetic.rs
# emit_div_by_zero_error) — the FIRING path: bash's "division by 0" to stderr,
# result 0, rc 0 (cov.57 only tested the ternary-UNTAKEN, non-firing case).
if in_modes native; then
    run_sash 'int x=0; echo $(( 5 / x ))'
    [[ $rc = 0 && $out = 0 && $rerr == *"division by 0"* ]]; chk cov.68
    run_sash 'int x=0; echo $(( 5 % x ))'
    [[ $rc = 0 && $out = 0 && $rerr == *"division by 0"* ]]; chk cov.69
else
    skip_chk cov.68; skip_chk cov.69
fi
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
if in_modes native; then
    run_sash 's="hello"; echo ${s:1:3}'
    [[ $rc = 0 && $out = ell && -z $err ]]; chk cov.72
else
    skip_chk cov.72
fi
# cov.73 `keys` as the sole source of a for-in loop (the array-splat for-in
# path). native.
if in_modes native; then
    run_sash 'map m=([x]=1 [y]=2); for k in $(keys m); do echo $k; done'
    [[ $rc = 0 && $out = $'x\ny' && -z $err ]]; chk cov.73
else
    skip_chk cov.73
fi
# cov.74 a compact (glued) dispatch operator `[[ $x ==b { … } ]]` → parser
# extract_compact_operator (operator-adjacent-to-pattern split). native.
if in_modes native; then
    run_sash 'x=b; [[ $x ==b { echo m } ]]'
    [[ $rc = 0 && $out = m && -z $err ]]; chk cov.74
else
    skip_chk cov.74
fi
# cov.75 a predicate function whose body is a dispatch with a default arm where
# every arm returns → validate case_always_returns (definite-return, DD#74).
# native.
if in_modes native; then
    run_sash 'f() { [[ $1 == a { return 0 } || { return 1 } ]] }; if f a; then echo yes; fi'
    [[ $rc = 0 && $out = yes && -z $err ]]; chk cov.75
else
    skip_chk cov.75
fi

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
# on "unset". So a non-zero `x` yields the value. All three backends now
# (bug-cjanet-default-on-int-var, fixed).
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
# identity-element ternary — native/c only (int arrays are a pre-existing janet
# gap: `scan-number` on the array literal; janet-int-array-and-cell-default).
if in_modes native,c; then
    run_sash 'int array a=(1 2 3); echo ${a[0]:-0}'
    [[ $rc = 0 && $out = 1 && -z $err ]]; chk cov.85
else
    skip_chk cov.85
fi

# cov.79 a direct `base#digits` arith literal (`16#ff`) is rejected SB4035 (bash
# arbitrary base, DD#72-dropped) — the direct-arith path, distinct from the
# `${s:-16#ff}` default path (arithmetic.sh d4.7).
run_sash 'echo $(( 16#ff ))'
[[ $rc != 0 && -z $out && $err == *SB4035* ]]; chk cov.79

# cov.89 `[[ -t N ]]` isatty test (native emit_isatty_test) — native,c only
# (janet has no `-t`). Captured stdout in the harness is not a tty → notty.
if in_modes native,c; then
    run_sash '[[ -t 1 ]] && echo tty || echo notty'
    [[ $rc = 0 && $out = notty && -z $err ]]; chk cov.89
else
    skip_chk cov.89
fi
# cov.90 a full-line comment inside a compiled program (parse_comment_stmt) —
# no feature spec has a standalone comment line in the source.
run_sash '# a comment
echo hi'
[[ $rc = 0 && $out = hi && -z $err ]]; chk cov.90

set -e
tally
