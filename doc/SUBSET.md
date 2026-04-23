# safebash — Safe Subset Specification

*Derived from ShellCheck's ~520 SC codes and design decisions
recorded in `exp/JOURNAL.md`. Each rule is classified as
**reject** (error), **defer** (resolved by SSA in phase 4),
or **marker** (compiler transforms the construct).*

*Per DD#40, coercion rules have been removed from the validator.
Sash handles bash idioms silently — backticks, `[ ]`, unquoted
variables all compile without diagnostics. The validator is now
a pure subset gate (4 rules).*

*Rules use ShellCheck's SC codes where the same rule applies.
Safebash-specific rules use SB4xxx codes (a new category beyond
ShellCheck's 1xxx/2xxx/3xxx scheme). All codes are permanent
once assigned.*

**A note on authorship:** This document was written by an AI (Claude,
Anthropic) under the direction of DG. The categorization reflects
DG's design: safety means the script's intent is visible, its data
boundaries are explicit, and its binary is tamper-evident.

---

## Design Principles

1. **No implicit behavior.** If a construct's effect depends on
   runtime state (IFS value, file existence for globbing, environment
   variables), the compiler handles it safely by construction. Sash
   has no word splitting and no implicit globbing — these entire
   classes of bugs are eliminated by the value model, not by linting.

2. **No dynamic code generation.** `eval` and indirect variable
   expansion via `${!var}` prevent static analysis and are rejected.

3. **Explicit data boundaries.** External data (env vars, positional
   params, read input, command output) is tainted by default (DD#24).
   Tainted data cannot be used until validated via `bless`. Environment
   variables are accessed via `$(getenv NAME)`, positional params via
   `$(argv N)`. No "variables just appearing."

4. **Silent coercion for bash idioms.** Backticks → `$()`, `[ ]` →
   `[[ ]]`, `$[..]` → `$((..))`), `for x in $(cmd)` → pipe-while-read.
   The compiler handles these silently — no diagnostics, no user
   action required. This is not linting; it's compilation.

5. **No minimum bash version.** The compiler accepts whatever syntax
   tree-sitter-bash can parse. Since safebash compiles to a native
   binary (not a bash script), the target system's bash version is
   irrelevant.

---

## Compilation Modes

Per DD#40, sash has two modes:

**`sash` (bash compatibility):** Shellcheck runs as a pre-compilation
lint gate (warning if shellcheck not installed). The compiler silently
coerces bash idioms: backticks → `$()`, `[ ]` → `[[ ]]`, `for x in $(cmd)`
→ pipe-while-read, `$HOME` → `$(getenv HOME)`, `$1` → `$(argv 1)`.
Taint quarantine applies — external data must be blessed.

**`sashn` / `sash --nolint` (sash-native):** No shellcheck. Sash-native
syntax available (`@arr`, `$arr[i]`, `len`, `keys`, `join`). Bare `$HOME`
and `$1` at program entry are compile errors — use `$(getenv HOME)` and
`$(argv 1)`. Quotes are optional everywhere (no word splitting in sash).

---

## Diagnostic Severity

| Severity | Meaning | Blocks compilation |
|----------|---------|--------------------|
| **error** | Unconditionally rejected | Always |
| **deferred** | Cannot resolve at CST level; SSA must prove safe | Yes, unless SSA resolves |
| **marker** | Signals compiler to transform the construct | No |

The `coerce` and `inform` severities were removed per DD#40. Coercions
are handled silently by the compiler — they're compilation, not linting.

---

## Category 1: Quoting and Word Splitting

The single largest source of Bash bugs. Unquoted expansions undergo
word splitting (by IFS) and globbing — both are implicit, context-
dependent, and produce surprising results with whitespace or special
characters in data.

| SC | Description | Mode | Action |
|----|-------------|------|--------|
| SC2086 | Unquoted `$var`, `$1`, etc. | coerce | Add double quotes |
| SC2046 | Unquoted `$(cmd)` | coerce | Add double quotes |
| SC2048 | Unquoted `$*` / `${array[*]}` | coerce | Change to `"$@"` / `"${array[@]}"` |
| SC2068 | Unquoted `$@` / `${array[@]}` | coerce | Add double quotes |
| SC2206 | Unquoted expansion in array assignment | reject | Ambiguous intent — use `mapfile` or `read -a` |
| SC2207 | Unquoted `$(cmd)` in array assignment | reject | Use `mapfile` or `read -a` |
| SC2231 | Unquoted expansion in for-loop glob | coerce | Quote the variable part |
| SB4020 | `for x in $(cmd)` — word-split loop | coerce | Rewrite to `cmd \| while read -r x` (stream, no word-split) |
| SB4021 | `for x in *` / `for x in *.ext` — glob loop | coerce | Rewrite to `find ... \| while read -r x` or `sash_readdir` |
| SB4022 | `printf -v var fmt args` — builtin sprintf | coerce | Rewrite to `var=$(printf fmt args)` — use external `/usr/bin/printf` |
| SB4023 | `getopts` — builtin option parser | coerce | Rewrite to `while/case/shift` or external `getopt` |
| SC2254 | Unquoted expansion in case pattern | coerce | Add double quotes for literal match |
| SC2291 | Unquoted repeated spaces | inform | |
| SC2248 | Unquoted variable without metacharacters | coerce | Add double quotes (optional check) |

### Rationale

SC2086 alone accounts for more real-world Bash bugs than any other
single issue. The safe subset requires all parameter expansions and
command substitutions to be double-quoted unless they appear in a
context where splitting/globbing is suppressed (inside `[[ ]]`,
inside `$(( ))`, on the RHS of `=` in assignment).

---

## Category 2: Eval and Dynamic Execution

Constructs that generate or execute code at runtime defeat static
analysis. The compiler's stance: **if I can see the code, I'll
compile it; if I can't, I won't run it.**

| Code | Description | Mode | Action |
|------|-------------|------|--------|
| SB4001 | `eval` command | reject/marker | Literal string args → marker (will be compiled inline); variable args → reject |
| SC2294 | `eval` negates benefit of arrays | reject | Drop eval |
| SB4003 | `${!var}` indirect expansion | reject | Variable name is opaque at compile time |
| SC2089 | Quotes/backslashes treated literally in variables used as commands | reject | Intent unclear — use an array |
| SC2090 | Quotes/backslashes in variable won't be respected | reject | Intent unclear — use an array |
| SB4004 | `$cmd` as command name (variable as command) | defer | Allow if `readonly cmd=literal`; defer to SSA otherwise; hard error if SSA can't resolve |
| SB4005 | Bourne-lineage shell with `-c` (`bash -c`, `sh -c`, etc.) | reject/marker | Literal string → marker (compiled as inline subshell); variable → reject |
| SB4006 | `trap` with code string | reject/marker | Literal code → marker (compiled inline); variable → reject; bare function name → allow |
| SB4008 | `alias` definition | coerce/error | Dangerous expansion (eval, source, bash) → error; safe expansion → coerce (convert to function) |
| SB4009 | `shopt -s expand_aliases` | reject | Text-macro expansion defeats static analysis |

### Wrapper command resolution

The validator unwraps command wrappers before applying checks:

| Wrapper | Resolution |
|---------|-----------|
| `builtin cmd` | Check the effective command (`cmd`) |
| `command cmd` | Check the effective command (`cmd`) |
| `env [-i] [-u X] [K=V]... cmd` | Skip env flags/assignments, check effective command |

`builtin eval`, `command eval`, `env bash -c "$var"` — all detected.

### ANSI-C string obfuscation

Command names in `$'...'` form (e.g., `$'\x65\x76\x61\x6c'` = `eval`)
are decoded before comparison. This prevents obfuscation of rejected
command names.

### Out-of-scope: external program arguments

safebash does not analyze arguments to external programs like `xargs`,
`find -exec`, `parallel`, etc. If `xargs sh -c '...'` is a concern,
it must be addressed at the OS/policy level, not the compiler level.
The number of programs that can execute arbitrary code is unbounded.

### Allowed: namerefs (declare -n / local -n)

Namerefs are **not** rejected. Unlike `${!var}`, where the variable
name is an opaque string at runtime, `local -n ref=target` declares
the referent at the point of creation. The compiler sees exactly
which variable `ref` points to. Even `local -n ref=$1` is analyzable
at the call site — the compiler knows what argument was passed.

Namerefs provide the same indirection capability as `${!var}` but
in a structured form the compiler can reason about. Rejecting them
would push users toward eval.

**Threat model caveat:** When the nameref target comes from untrusted
input (`declare -n ref="$1"`), an attacker can read or write arbitrary
shell variables (including `PATH`, `LD_PRELOAD` in child processes,
etc.). This is an accepted risk — the script source is trusted, and
namerefs are too useful to reject. Authors must ensure variable-name
provenance is trusted at call sites.

### source / . — compiled away, not rejected

`source` and `.` are not rejected. They are compiled to a safe
runtime mechanism:

| Code | Description | Mode | Action |
|------|-------------|------|--------|
| SB4002 | `source` / `.` | marker | Compiled to safe kvp loader |

At runtime, the compiler replaces `source`/`.` with:

1. Fork a Rust helper (`sash-expand`) on the file path.
2. Helper validates every line is `KEY=VALUE`, comment, or blank.
   Dies on the first non-conforming line with the offending line
   and line number.
3. Helper expands simple `$VAR` / `${VAR}` references in values
   against its inherited environment. Dies on any other expansion
   form (`${VAR:-...}`, `$(cmd)`, backticks, etc.).
4. Helper outputs trivial format: `KEY=value-to-newline`. No
   quoting, no escaping. Values cannot contain newlines.
5. Compiled binary reads helper stdout, sets variables. If helper
   exits nonzero, the binary dies too.

Both `source` and `.` are synonyms in bash and compile identically.

---

## Category 3: Deprecated and Legacy Syntax

Old syntax that has a modern, safer equivalent. Coerce mode
transforms mechanically; strict mode rejects.

| SC | Description | Mode | Action |
|----|-------------|------|--------|
| SC2006 | Backtick `` `cmd` `` substitution | coerce | Convert to `$(cmd)` |
| SC2007 | `$[expr]` arithmetic | coerce | Convert to `$((expr))` |
| SC2003 | `expr` for arithmetic | coerce | Convert to `$(( ))` |
| SC2219 | `let expr` | coerce | Convert to `(( expr ))` |
| SC2196 | `egrep` | coerce | Convert to `grep -E` |
| SC2197 | `fgrep` | coerce | Convert to `grep -F` |
| SC2230 | `which` | coerce | Convert to `command -v` |
| SC2112 | `function` keyword (non-POSIX) | coerce | Convert to `fname()` form |
| SC2268 | `x`-prefix in comparisons | coerce | Remove prefix |

---

## Category 4: Test Constructs

`[ ]` is a command with complex quoting requirements. `[[ ]]` is a
Bash keyword with built-in word splitting/globbing suppression.

| SC | Description | Mode | Action |
|----|-------------|------|--------|
| SC2292 | `[ ]` instead of `[[ ]]` | coerce | Convert to `[[ ]]` |
| SC2070 | `-n` doesn't work with unquoted args in `[ ]` | coerce | Convert to `[[ ]]` |
| SC2074 | `=~` not supported in `[ ]` | coerce | Convert to `[[ ]]` |
| SC2081 | `[ ]` can't match globs | coerce | Convert to `[[ ]]` |
| SC2107 | `[ a && b ]` | coerce | Convert to `[[ a && b ]]` |
| SC2109 | `[ a \|\| b ]` | coerce | Convert to `[[ a \|\| b ]]` |
| SC2166 | `[ p -a q ]` / `[ p -o q ]` | coerce | Convert to `[[ ]]` with `&&` / `\|\|` |
| SC2130 | `-eq` for string comparison | reject | Type error |
| SC2071 | `>` for numeric comparison | reject | Type error |

### Rationale

`[[ ]]` eliminates an entire class of bugs related to empty
variables, glob characters in data, and operator confusion.
The safe subset requires `[[ ]]` for all conditionals.

The `test` builtin is equivalent to `[ ]` and is also rejected.

---

## Category 5: Subshell and Scope

Bash pipelines create subshells. Variable modifications inside a
pipeline are invisible to the parent. This is the most common source
of "it works on my machine" bugs.

| SC | Description | Mode | Action |
|----|-------------|------|--------|
| SC2030 | Variable modified in subshell (pipeline) | reject | Use `while read` with process substitution or `lastpipe` |
| SC2031 | Variable was modified in subshell — change lost | reject | Same |
| SC2033 | Shell functions can't be passed to external commands | inform | |
| SC2106 | `exit` only exits subshell in pipeline | inform | |

### Rationale

The compiler can detect pipeline subshell scope at the CST level.
When a variable is assigned inside a pipeline's subshell (the
non-last segment, or all segments without `lastpipe`), the safe
subset rejects the script.

---

## Category 6: Safety and Robustness

Constructs that are technically valid but produce dangerous behavior
in common scenarios.

| SC | Description | Mode | Action |
|----|-------------|------|--------|
| SC2164 | `cd` without `|| exit` / `|| return` | coerce | JIT: append `|| exit 1` and warn. AOT strict: error — author chooses recovery |
| SC2115 | `rm -rf $dir/` may expand to `rm -rf /` | reject | Require `"${var:?}"` guard |
| SC2155 | `export foo=$(cmd)` masks return value | coerce | Separate declaration and assignment |
| SC2064 | Trap string expands at definition, not signal | coerce | Use single quotes |
| SC2162 | `read` without `-r` mangles backslashes | coerce | Add `-r` |
| SC2172 | Trap signals by number | coerce | Use signal names |
| SC2181 | Checking `$?` instead of `if cmd` | inform | |
| SC2310 | `set -e` disabled in `if` condition function call | inform | |
| SC2311 | `set -e` disabled in command substitution | inform | |
| SC2251 | `!` skips errexit | inform | |

### SC2164 rationale

`cd` without error handling is the canonical "delete everything" bug:
`cd tmpdir; rm -rf *` becomes `rm -rf *` in the original directory
if `cd` fails silently. In JIT/coerce mode, `|| exit 1` is appended
automatically. In AOT/strict mode, the compiler reports the error
and lets the author choose the fix: `|| exit`, `|| return`, or
`|| { handle_error; }`. The compiler identifies the problem but
doesn't choose the recovery strategy for release artifacts.

---

## Category 7: Arrays

Bash arrays have non-obvious semantics. The safe subset enforces
correct usage patterns.

| SC | Description | Mode | Action |
|----|-------------|------|--------|
| SC2128 | `$array` only gives first element | reject | Must use `"${array[@]}"` |
| SC2199 | Arrays implicitly concatenate in `[[ ]]` | reject | Use a loop |
| SC2198 | Arrays don't work as `[ ]` operands | reject | Use a loop |
| SC2124 | Assigning array to string | reject | Use `*` or keep as array |
| SC2178 | Variable used as array then assigned string | reject | Type inconsistency |
| SC2179 | Use `array+=("item")` to append | inform | |
| SC2180 | No multidimensional arrays | inform | |
| SC2190 | Associative array needs index | reject | Syntax error |
| SC2313 | Quote array indices (glob expansion) | coerce | Add quotes |

---

## Category 8: Command and Process Safety

| SC | Description | Mode | Action |
|----|-------------|------|--------|
| SC2045 | Iterating over `ls` output | reject | Use globs |
| SC2010 | `ls \| grep` | inform | Use globs |
| SC2044 | For loops over `find` output | reject | Use `find -exec` or `while read` |
| SC2156 | Injecting filenames into `-exec sh -c` | reject | Use parameters |
| SC2035 | Glob as first argument (becomes option) | coerce | Prefix with `./` |
| SC2012 | `ls` for non-alphanumeric filenames | inform | |

---

## Category 9: Arithmetic

| SC | Description | Mode | Action |
|----|-------------|------|--------|
| SC2004 | Unnecessary `$` in `$(( ))` | coerce | Remove `$` |
| SC2080 | Leading zero = octal | inform | |
| SC2079 | `(( ))` doesn't support decimals | inform | |
| SC2255 | `[ ]` doesn't do arithmetic | coerce | Use `$(( ))` |
| SC2304 | Unescaped `*` in `expr` | reject | Use `$(( ))` instead |

---

## Out of Scope

The following categories are deliberately excluded from the safe
subset specification:

**SC1xxx (parse errors):** tree-sitter-bash handles these. If the
CST contains ERROR nodes, the script is rejected before validation.

**SC3xxx (portability):** safebash accepts whatever tree-sitter-bash
can parse. POSIX sh compatibility is not a goal.

**Style suggestions** (SC2002 useless cat, SC2005 useless echo,
SC2126 grep|wc vs grep -c, etc.): These don't affect safety.
The compiler may emit warnings but will not reject.

**Unused variables** (SC2034, SC2154): The compiler may warn but
these are not safety issues. SSA analysis in phase 4 will handle
variable tracking more precisely than ShellCheck.

**`select` loops (SB4012):** Interactive construct. Permanently
excluded from the safe subset.

**Coproc (SB4013):** Concurrency construct. Permanently excluded
from the safe subset.

**External program argument analysis:** safebash does not inspect
arguments to programs like `xargs`, `find -exec`, `parallel`, or
`awk '{system(...)}'`. The set of programs capable of executing
arbitrary code is unbounded. This is an OS/policy concern, not a
compiler concern.

---

## Implementation Priority

Phase 2 experiments implement these rules in order:

| Exp | Rule | Category |
|-----|------|----------|
| 007 | Reject `eval`, mark `source`, reject `${!var}`, defer `$cmd` | Dynamic execution (SB4001–SB4004) |
| 008 | Reject/coerce unquoted expansions | Quoting (SC2086) |
| 009 | Reject/coerce backtick substitution | Deprecated (SC2006) |
| 010 | Reject/coerce `[ ]` / `test` | Test constructs (SC2292) |
| 011 | Validator framework + remaining rules | All categories |

After 011, individual rules from this spec will be added
incrementally. The framework makes adding a new rule a matter of
writing one function and one test.
