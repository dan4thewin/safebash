# Experiment 037: for, case, functions in codegen (2026-04-16)

Added native code generation for `for` loops (literal word lists,
single word, variable expansion), `case` statements (pattern matching
with wildcards, variable-derived discriminants), and user-defined
functions (basic call/return, function-scoped variables).

**Tests added:** 10 codegen tests (for_literal_words, for_single_word,
for_with_variable, case_basic, case_second_branch, case_wildcard,
case_from_arg, case_from_arg_wildcard, case_sets_variable,
function_basic). Total: 58 codegen + 59 oracle = 117.

## Experiment 038: dynamic for loops (2026-04-16)

Extended `for` loops to iterate over `$@` (positional parameters).
Handles empty argument lists and single-argument edge cases.

**Tests added:** 3 (for_dollar_at, for_dollar_at_empty,
for_dollar_at_single). Total: 61 codegen + 59 oracle = 120.

## Experiment 039: until loops, while read (2026-04-17)

Added `until` loop codegen and `while read line` for line-by-line
stdin processing. This enabled the first scripts that consume piped
input.

**Tests added:** 5 (until_basic, until_immediate_exit,
while_read_stdin, until_countdown, while_read_count). Total: 66
codegen + 59 oracle = 125.

## Experiment 040: arithmetic statements (2026-04-17)

Codegen for `(( ))` as a command (not just `$(( ))` expansion):
postfix increment/decrement, compound assignment (`+=`, `-=`),
bare assignment, `(( ))` as an `if` condition, and C-style
`for (( ; ; ))` loops.

**Tests added:** 6 (arith_cmd_postfix_increment,
arith_cmd_compound_assign, arith_cmd_assign, arith_cmd_if_condition,
arith_cmd_decrement, c_style_for_loop). Total: 72 codegen + 59
oracle = 131.

## Experiment 041: integer variables (2026-04-17)

Implemented `declare -i` / `int` variable support in codegen.
Integer variables store machine-width integers directly instead of
string pointers. Arithmetic on int vars avoids atoi/itoa round-trips.
Type checker tracks int vs string through the SSA graph.

**Tests added:** 7 (int_var_arithmetic, int_var_arith_cmd,
int_var_if_arith, int_var_if_arith_false, int_var_while_countdown,
int_var_c_style_for, int_var_dollar_arith_expansion). Total: 79
codegen + 59 oracle = 138.

## Experiment 042: exec, lists, source emission (2026-04-18)

Added `exec` (replaces process), `&&`/`||` lists with short-circuit
evaluation, `!` pipeline negation in codegen. Also the `--emit-source`
flag for source-to-source output (precursor to the full source emitter
built later in Phase 2).

**Tests added:** 23 (exec_replaces_process, list_and_success/failure,
list_or_success/failure, list_cmd_and, negation variants, plus
source emission tests). Total: 102 codegen + 59 oracle = 161.

## Experiment 043: printf, arrays (2026-04-18)

Codegen for `printf` (basic format strings, `%d`, multiple args)
and indexed arrays (declaration, variable indexing, loop iteration).
Associative arrays wired through the unified `SashTable` (DD#33).
Sash-native `array` and `map` keywords first supported here.

**Tests added:** 11 (printf_basic, printf_multiple_args,
printf_no_format, printf_percent_d, indexed_array_basic,
indexed_array_variable_index, indexed_array_loop,
assoc_array_with_table, indexed_array_from_args, keyword_map,
keyword_array). Total: 113 codegen + 59 oracle = 172.

## Experiment 044: pipe-while-read, final feature round (2026-04-19)

The last and largest experiment. Multi-stage pipelines with
`while read` (`echo | while read line`), the noop colon command
(`:`), `echo -n`, arithmetic comma operator, and many edge cases.
This experiment reached 247 codegen tests — the full set that
carried forward into the consolidated `sash/` crate.

**Tests added:** 134 (pipe_while_read_basic, pipe_while_read_count,
pipe_while_read_multi_stage, noop_colon, echo_n_no_newline,
arith_comma_operator, plus ~128 others covering all remaining
codegen edge cases). Total: 247 codegen + 59 oracle = 306.

## Experiment 045: validator with taint tracking and bless (2026-04-19)

Standalone validator experiment implementing flow-sensitive taint
analysis. All external input (command substitution, environment
variables, positional parameters, `read`) is tainted. The `bless`
command clears taint after validation. This experiment proved the
taint model before integrating it into the consolidated compiler.

**Tests added:** 68 integration tests covering eval/source rejection,
indirect expansion, taint propagation, bless semantics, and clean
scripts. This was the last numbered experiment.

---

# Phase 2: Consolidated compiler (2026-04-20 onward)

After experiment 045, all code was consolidated into the `sash/`
crate. The numbered experiment model was retired — further work
happens directly in `sash/`. The experiments were later split into
the `safebash-exp` repo (git submodule at `exp/`).

Key developments in Phase 2 (each supported by design decisions
documented below):

**Taint integration (DD#43, DD#44).** The taint tracker from exp/045
was integrated into the compiler pipeline. Taint analysis runs after
validation, before lowering. `bless var [[ condition ]]` is the
only way to clear taint. All use sites of tainted variables produce
SB4020 diagnostics. Arithmetic context `(( ))` is a read context
for taint purposes (DD#43 amendment).

**Source emitter (DD#45).** `--emit-source` outputs sash-native
source by recording coercions during the CST walk and applying them
to the original source bytes. Comments and formatting are preserved.
`--annotate` adds `# was: ...` comments showing original syntax.
The same coerced source is embedded in a `.sash_source` ELF section
in every compiled binary, recoverable via `sash --extract <binary>`.

**Declaration axes (DD#45 amendment).** Four orthogonal axes for
variable declarations: mutability (`const`), lifetime (`local`),
type (`int`), structure (`array`, `map`). Stacked freely:
`local const int x=5`. Coercions from bash syntax: `declare -i` →
`int`, `declare -a` → `array`, `declare -A` → `map`, `readonly` →
`const`, `let "expr"` → `(( expr ))`. Combined flags split:
`declare -ri` → `const int`.

**Export rejection (SB5001).** `export` and `declare -x` are compile
errors. The sash model uses `FOO=bar cmd` for per-command env vars.
The validator rule catches both the keyword and the flag.

**Heredoc command substitution rejection (SB5002).** `$(cmd)` inside
heredoc bodies feeding external commands is rejected — the tainted
data has no named variable to bless. The safe pattern: extract to
a variable, bless it, reference the blessed variable in the heredoc.

**Regex expression syntax (DD#39 amendments).** `${var:s/re/str/flags}`
is the sash-native expression form of regex replace — returns a value
without mutating. `var =~ s/re/str/flags` is the statement form that
mutates in place. Tree-sitter grammar extended with a new external
scanner token (`:s/`). The `${var/pat/rep}` family is accepted via
glob→regex coercion.

**Known externals auto-bless.** `known_externals.rs` defines
conservative regex patterns for 26 environment variables and 25
commands. When the taint tracker recognizes a known-safe source, it
auto-blesses the target variable. `--emit-source` inserts full
`bless` lines for transparency.

**Janet backend parity.** `emit_janet.rs` brought to functional
parity with the native backend: break/continue, function arguments
in arithmetic, string operations (length, case, substring, prefix/
suffix strip, replace), heredocs, `$?` exit status, `&&`/`||` lists,
modulo. 37 round-trip tests verify identical output between bash and
Janet execution.

**Function argument passing.** Fixed `$1`/`$2` in arithmetic
expressions inside functions. The lowerer now preserves `$` prefix
on positional parameters so the transform doesn't misidentify them
as numeric literals.

**Repo split.** `safebash-exp` created as a separate repo. History
rewritten with `git-filter-repo` to remove experiment directories
and test artifacts from all commits. `.git` shrunk from 16MB to
648KB. Experiments remain accessible as a git submodule at `exp/`.

**Test suite at end of Phase 2:** 457 tests (247 codegen, 114 e2e,
37 Janet, 59 oracle), zero warnings.

