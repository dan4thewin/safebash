title: "BUG: sash-convert emits programs that fail to compile, without a `# MANUAL:` marker that says why"
status: open
priority: P3
kind: bug
depends: []
gates:
  - "every row under What converts to a program that compiles and prints what bash prints, or the failing line carries a `# MANUAL:` marker naming the change the user must make"
  - "a `# MANUAL:` marker's reason is true: g01, g04 and x10 today say \"operands too complex for auto-convert\" about one variable, `$?` and `$((n))`"
  - "m9 converts to one statement, not two"
  - "specs in spec/convert.sh pin one row from each group"

## What

A catalogue. Each bash program converts without complaint, and the output
fails to compile. All are pre-existing. HEAD before D1 (bf48bec) gives the
same first error for every row but cf6, where it also wrote
`for ${x:?x is not an integer} in a b` inside a C-style `for` body (SB4014,
then SB4072); D1's `in_arithmetic` boundaries removed that.

**Int typing.** Bash treats the variable as an integer; the conversion leaves
it a string.

| Id | Bash | Converted | First error |
|---|---|---|---|
| c1 | `declare -i i=3` / … `i=i-1` in a `while` | `int i=3` / … `i=i-1` | SB4030 "cannot assign string value to int variable `i`" |
| x11 | `f() { local -i x=$1; [[ $x == 1 ]] && echo one; }` / `f 1` | `int x=$1` | SB4030 "cannot assign string value to int variable `x`" |
| m1 | `count=0` / `count=$((count+1))` | `count=$((${count:?count is not an integer}+1))` | SB4030 "cannot assign int value to string variable `count`" |
| p6 | `k=0` / `(( k++ ))` | unchanged | SB4030, the same for `k` |
| x04 | `declare -a a` / `let a[1]=5` | `array a` / `(( a[1]=5 ))` | SB4030, the same for `a` |
| s12 | `declare -a a=(0)` / `let "a[0] += 4"` | `array a=(0)` / `(( a[0] += 4 ))` | SB4030 twice, then "string cell `a[…]` in arithmetic" |
| g05 | `x=2; y=$(( -x ))` | unchanged | SB4030 "bare string `x` in arithmetic" |

g05 is the odd one: m5's `m=$(( n * 2 ))` gets its `${n:?…}`, but an operand
after a unary `-` does not.

**Numeric test operators.** sash compares numbers in `(( ))`; `-eq` and its
kin in `[[ ]]` are SB5019.

| Id | Bash | Converted | First error |
|---|---|---|---|
| k2 | `if [[ $# -eq 0 ]]; then echo none; fi` | `if [[ $(( (argc) - 1 )) -eq 0 ]]; …` | SB5019 "'-eq' is a numeric operator" |
| k3 | `if [ $# -eq 0 ]; …` | the same | SB5019 |
| g06 | `declare -i n=3` / `[[ $n -eq 3 && $n -lt 5 ]] && echo both` | `int n=3` / unchanged | SB5019 twice |
| g01 | `/bin/true` / `[ $? -eq 0 ] && echo ok` | `[[ $(status) -eq 0 ]]`, under a MANUAL marker | SB5019 |
| g04 | `n="3"` / `[ "$n" -gt 2 ] && echo big` | `[[ $n -gt 2 ]]`, under a MANUAL marker | SB5019 |
| x10 | `declare -i n=4` / `[[ $((n)) -gt 3 ]] && echo big` | unchanged, under a MANUAL marker | SB5019 |
| g03 | `declare -i n=3` / `[ $n -eq 3 -a $n -lt 5 ] && echo both` | `[[ $n -eq 3 -a $n -lt 5 ]]` | SB4014 "expected ']]'" |

**`$#` in a function.** A bash function takes any number of arguments; a
sash function's arity comes from the arguments it reads.

| Id | Bash | Converted | First error |
|---|---|---|---|
| sp1 | `f() { echo "${#} $#"; }` / `f a b` | `f() { echo "$# $#"; }` | SB5043 "function 'f' takes exactly 0 arguments but was called with 2" |
| g02 | `f() { [ $# ] && echo has; }` / `f a` | `f() { [[ -n "$#" ]] && echo has; }` | SB5043 |
| g08 | `f() { echo "n=$#"; }` / `f a b` | unchanged | SB5043 |

**Singletons.**

| Id | Bash | Converted | First error |
|---|---|---|---|
| x06 | `/bin/true` / `[[ ${?} == 0 ]] && echo ok` | `[[ "${?}" == 0 ]]` (`$?` becomes `$(status)`; `${?}` does not) | SB4034 "the `?` parameter operator (in ${…}) isn't part of sash" |
| x05 | `x="a  b"` / `[ "$x" = "a  b" ] && echo same \|\| echo collapsed` | `[[ $x == "a b" ]] && echo same \|\| echo collapsed` | SB4053 "`&&` and `\|\|` in one list are ambiguous" |
| m5 | `x=$RANDOM` | unchanged | SB5017 "$RANDOM does not exist in sash" |
| cf4 | `x=$(( $(echo 3) + 1 ))` | unchanged | SB4035 "a nested `$( … )` or `$(( … ))` is not an arithmetic operand" |
| m9 | `/bin/true` / `x=$(( $( [[ $? == 0 ]] && echo 1 ) + 1 ))` | two statements: a spurious `(( $( [[ $? == 0 ]] && echo 1 ) + 1 ))`, then `x=$(( $( [[ "$(status)" == 0 ]] && echo 1 ) + 1 ))` | SB4035 on each |

m9 is the one that is more than a missing conversion. The extra statement
repeats the capture, with its `$?` unconverted. It is loud today only because
SB4035 rejects both lines. If they ever compiled, the capture would run
twice. cf6 and s16 have the same shape. Without the `$?` (m10), without the
arithmetic (m11), or as `echo $((…))` (m12), there is no extra statement.

## Fix

Recommendation (made during D1, not yet ruled), by group:

- **Int typing:** when bash treats a name as an integer (`declare -i`,
  `local -i`, `let`, `(( ))`, or a later `$(( ))` assignment), declare it
  `int` or an int array, and convert its other assignments to arithmetic.
- **Numeric operators:** rewrite `-eq` … `-ge` (with `-a` / `-o` inside
  `[ ]`) to `(( ))` when every operand is a literal, a variable, `$?`, `$#`
  or an arithmetic expansion. Keep `# MANUAL:` for operands that really are
  complex.
- **`$#` in a function:** add `: @args` to a converted function that reads
  `$#` or `${#}` and has no variadic access. That is DD#81's own migration:
  its addendum of 2026-09-28 makes counting contribute nothing to arity.
  `$(len args)` would not help, because it is SB5043 the same way. The doc
  table at sash-convert/src/convert.rs:1898 still says `$#` becomes
  `$(len args)`; the code (1935) keeps `$#` per DD#46, so the table is stale.
- **Singletons:** `${?}` like `$?`; m9 emits one statement; x05, m5 and cf4
  get a `# MANUAL:` marker naming the change.

## Related

- No open converter task covers a row here. The nearest are
  `converter-var-modifier-migration` (bash's own `${x:?}` and friends),
  `bug-arith-operator-expansion-unconverted`, `bug-convert-dollar-ten` and
  `convert-brace-forms-passthrough`.
- `bug-convert-test-whitespace-collapse`: x05's first line is its w2.

Found during the batch-531 port (step D1, converting the D1 corpus and
checking that each output compiles). Filed under DG's rule for the port that
questions and uncovered cases become backlog tasks.
