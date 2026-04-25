# Brush Test Suite Checklist — 2026-04-25

Baseline: 622 total cases (161 PASS, 56 FAIL_OUTPUT, 75 FAIL_COMPILE,
300 NA, 27 SKIP, 3 TIMEOUT).

This file tracks every non-PASS case. Cases that PASS are omitted.
Cases marked `[SHOULD_FAIL]` are correctly rejected by sash; the test
passes when sash produces the expected diagnostic.

## Legend

- `[ ]` — needs work (bug, missing feature, or wrong output)
- `[x]` — done (passes or correctly rejected)
- `[SHOULD_FAIL: SBnnnn]` — correctly rejected by sash; verify diagnostic

---

## SHOULD_FAIL — Correctly Rejected (68 cases)

These scripts use features sash intentionally disallows. Each should
compile-fail with the listed diagnostic code.

### SB4020 — Tainted variable (47 cases)

- [x] arithmetic.yaml :: Basic quoted arithmetic
- [x] arithmetic.yaml :: Divide by zero
- [x] arithmetic.yaml :: Assignment with references
- [x] arrays.yaml :: Create array via index update
- [x] arrays.yaml :: Dump values from non-existent array
- [x] arrays.yaml :: Use non-existent var in indexed array index
- [x] arrays.yaml :: Use non-existent var in associative array index
- [x] builtins/cd.yaml :: Basic cd usage
- [x] builtins/cd.yaml :: cd -
- [x] builtins/command.yaml :: Basic command usage
- [x] builtins/command.yaml :: command -v
- [x] builtins/command.yaml :: command -v with full paths
- [x] builtins/let.yaml :: let with assignment
- [x] builtins/read.yaml :: read from process substitution
- [x] builtins/read.yaml :: read with custom IFS
- [x] builtins/read.yaml :: read text with tabs and custom IFS
- [x] command_substitution.yaml :: Ignore single quote in comment in command substitution
- [x] command_substitution.yaml :: Ignore double quote in comment in command substitution
- [x] command_substitution.yaml :: Ignore parantheses in comment in command substitution
- [x] command_substitution.yaml :: Ignore dollar in comment in command substitution
- [x] command_substitution.yaml :: Positional parameter count not mistaken for comment
- [x] command_substitution.yaml :: Ignore single quote in comment in command substitution (backticks)
- [x] command_substitution.yaml :: Ignore double quote in comment in command substitution (backticks)
- [x] command_substitution.yaml :: Ignore parantheses in comment in command substitution (backticks)
- [x] command_substitution.yaml :: Ignore dollar in comment in command substitution (backticks)
- [x] command_substitution.yaml :: Positional parameter count not mistaken for comment (backticks)
- [x] compound_cmds/case.yaml :: Basic catch-all pattern against multi-line input
- [x] functions.yaml :: Function invocation with empty arg
- [x] functions.yaml :: Function call with env variables
- [x] here.yaml :: Here doc in a command substitution
- [x] redirection.yaml :: Redirection in command substitution
- [x] redirection.yaml :: Redirection in command substitution to non-standard fd
- [x] redirection.yaml :: Redirection in command substitution in braces to non-standard fd
- [x] redirection.yaml :: Redirection in command substitution in subshell to non-standard fd
- [x] simple_commands.yaml :: Simple command with non-existent cwd
- [x] well_known_vars.yaml :: Basic defaulted PATH var
- [x] word_expansion.yaml :: Undefined variables
- [x] word_expansion.yaml :: Undefined positional params
- [x] word_expansion.yaml :: High-numbered positional params
- [x] word_expansion.yaml :: Command substitution
- [x] word_expansion.yaml :: Command substitution with embedded parens
- [x] word_expansion.yaml :: Command substitution with exec
- [x] word_expansion.yaml :: Backtick command substitution
- [x] word_expansion.yaml :: Backtick command substitution with escaping
- [x] word_expansion.yaml :: Parameter expression: default value
- [x] word_expansion.yaml :: Parameter expression: alternative value
- [x] word_expansion.yaml :: Parameter expression: error on condition (non-interactive)
- [x] word_expansion.yaml :: Parameter expression: expanded array as default value
- [x] word_expansion.yaml :: Substring operator on arrays
- [x] word_expansion.yaml :: Expansion with curly braces

### SB4014 — Parse error (5 cases)

- [x] arithmetic.yaml :: Empty expression
- [x] compound_cmds/for.yaml :: Empty for loop
- [x] compound_cmds/subshell.yaml :: Subshells in sequence
- [x] here.yaml :: Multiple here docs in a command substitution
- [x] here.yaml :: Complex here docs in a command substitution

### SB4004 — Dynamic command (3 cases)

- [x] assignments.yaml :: Quoted equals sign
- [x] here.yaml :: Multiple here docs
- [x] well_known_vars.yaml :: SHLVL

### SB4003 — Indirect expansion (5 cases)

- [x] word_expansion.yaml :: Indirect variable references
- [x] word_expansion.yaml :: Indirect variable references with special parameters
- [x] word_expansion.yaml :: Indirect variable references with array
- [x] word_expansion.yaml :: Variable prefix match
- [x] word_expansion.yaml :: Array keys: indexed array
- [x] word_expansion.yaml :: Array keys: empty array

### SB5001 — Export rejected (3 cases)

- [x] builtins/declare.yaml :: Declare and export variable
- [x] builtins/exec.yaml :: exec without -c
- [x] builtins/export.yaml :: Basic export usage

### SB5002 — Heredoc cmdsub (1 case)

- [x] here.yaml :: Here doc with expansions

---

## ICE / Compiler Crashes (7 cases)

These panic or produce ICE errors. All are bugs.

### ICE — SSA taint backstop fired (4 cases)

CST taint pass missed these; SSA backstop caught them.

- [ ] arithmetic.yaml :: Nested expressions — cmdsub inside arithmetic `$(($(echo 1) + 37))`
- [ ] arrays.yaml :: Fill associative array — shift+loop in function, local vars
- [ ] special_parameters.yaml :: $@ and $* — `for arg in $@` in function body
- [ ] word_expansion.yaml :: Passing array elements to func — `for arg in $@` in function

### ICE — Codegen panic (2 cases)

- [ ] word_expansion.yaml :: Substring with length (with nested expressions) — `${var:$offset:${length}}`; ICE: unsupported param expansion operator `:`
- [ ] word_expansion.yaml :: Parameter transformation: assignment — `${var@A}`; ICE: expected string var, got int

### COMPILE_BUG — Wrong rejection (1 case)

- [ ] compound_cmds/subshell.yaml :: Subshell env usage — `(subshell_var=value && echo ...)` → UNKNOWN STMT KIND: (

---

## WRONG_OUTPUT — Compiles but wrong result (56 cases)

### Arithmetic side effects (3 cases)

Bash treats `(( x = 1 ))` and `$(( x += 1 ))` as having side effects
on variables. Sash treats arithmetic as pure (DD#31).

- [ ] arithmetic.yaml :: Complex variable references — `$((z))` where z="x+y" (variable-as-expression)
- [ ] arithmetic.yaml :: Assignment arithmetic — `$((x = 1))`, `$((x += 1))`, `$((x++))` don't persist
- [ ] arithmetic.yaml :: Assignments in logical boolean expressions — `$(( 1 && (x+=1) ))`

### Compound command redirects (4 cases)

Redirects on compound commands (`for...done > file`, `(cmd) > file`,
`func() {} >> file`) apply to the whole compound, not individual cmds.

- [ ] compound_cmds/for.yaml :: For loop output redirection — `for...done > file`
- [ ] compound_cmds/for.yaml :: For loop piped — `for...done | grep`
- [ ] compound_cmds/subshell.yaml :: Subshell output redirection — `(echo; echo) > file`
- [ ] functions.yaml :: Function definition with output redirection — `f() {} >> file`

### Subshell/pipeline isolation (3 cases)

Pipeline components run in subshells; variable mutations don't propagate.

- [ ] compound_cmds/subshell.yaml :: Piped subshell usage — `(echo hi) | wc -l` → 0 instead of 1
- [ ] pipeline.yaml :: Side effects in pipe commands — `{ var=2; echo hi; } | cat` ordering
- [ ] pipeline.yaml :: pipeline extension — `|&` stderr redirect

### Array operations (10 cases)

Various array edge cases: per-element operations, IFS joining,
${arr} → ${arr[0]}, empty array iteration.

- [ ] arrays.yaml :: Basic associative array definition and access — `${var}` on assoc array
- [ ] arrays.yaml :: Array element expansion with non-standard IFS — `${arr[*]}` should join with IFS
- [ ] word_expansion.yaml :: Array access — `var2=${y[@]}` assignment semantics
- [ ] word_expansion.yaml :: Empty arrays — `for f in "${myarray[@]}"` on empty array
- [ ] word_expansion.yaml :: Mixing array elements and other words — `"${arr[@]} d e f"` quoting
- [ ] word_expansion.yaml :: Array length — `${#y}` should be `${#y[0]}` = 1
- [ ] word_expansion.yaml :: Array length: single item — `${#z}` = length of first element
- [ ] word_expansion.yaml :: Substring replacement — `${arr/pat/rep}` per-element
- [ ] word_expansion.yaml :: Prefix substring replacement — `${arr/#pat/rep}` per-element
- [ ] word_expansion.yaml :: Suffix substring replacement — `${arr/%pat/rep}` per-element
- [ ] word_expansion.yaml :: Global substring replacement — `${arr//pat/rep}` per-element
- [ ] word_expansion.yaml :: Substring operator on single-element array — `${arr[@]:1}` slice
- [ ] word_expansion.yaml :: Iterate through modified array — `${array[@]%a}` per-element strip

### Parameter transformations — @-syntax (6 cases)

DD#52: `${var@...}` is not in the safe subset. These should become
SHOULD_FAIL once a validator rule rejects `@` operator.

- [ ] word_expansion.yaml :: Parameter case transformation — uses @U/@u/@L
- [ ] word_expansion.yaml :: Parameter quote transformations - Q — uses @Q
- [ ] word_expansion.yaml :: Parameter quote transformations - K — uses @K
- [ ] word_expansion.yaml :: Parameter quote transformations - k — uses @k
- [ ] word_expansion.yaml :: Parameter transformations: expand escapes — uses @E
- [ ] word_expansion.yaml :: Parameter transformation: attributes — uses @a

### Extended test edge cases (7 cases)

- [ ] extended_tests.yaml :: File executable tests — `-x` on symlinks
- [ ] extended_tests.yaml :: Binary string matching with expansion — `+([$var])` extglob
- [ ] extended_tests.yaml :: Empty and space checks — `${var//[[:space:]]/}` + `-v` semantics
- [ ] extended_tests.yaml :: Variable set checks — `-v` should distinguish "declared but unset" from "set to empty"
- [ ] extended_tests.yaml :: Variables in extended tests — `[[ var -eq 10 ]]` bare name as variable
- [ ] extended_tests.yaml :: Regex with min/max counts — `^z{2,6}$` quantifier
- [ ] extended_tests.yaml :: Regex with newline — `$'\n' =~ .` should match

### Substring/expansion edge cases (3 cases)

- [ ] word_expansion.yaml :: Substring replacement with patterns — `${var/${pattern}/}` dynamic pattern
- [ ] word_expansion.yaml :: Substring on string with multi-byte chars — byte vs character indexing
- [ ] word_expansion.yaml :: Parameter expression: expanded array as alternate value — `${var+"${arr[@]}"}`

### Case statement (1 case)

- [ ] compound_cmds/case.yaml :: Case status values — `;&` fallthrough, `return $1` in case

### Redirections (2 cases)

- [ ] redirection.yaml :: Output redirection to file (append) — `>> file` appends duplicate
- [ ] redirection.yaml :: Redirection failure — `> /nonexistent` should suppress output

### Tilde expansion (1 case)

- [ ] assignments.yaml :: Assignment with tilde expansion — `~/a:~/b` colon-separated

### Command builtins (3 cases)

- [ ] builtins/command.yaml :: command -v -p — `-p` flag not handled
- [ ] builtins/printf.yaml :: printf %q — quoting style differs from bash
- [ ] builtins/printf.yaml :: printf ~%q — same quoting issue

### Return/unset (2 cases)

- [ ] builtins/return.yaml :: Return from nested clauses — `return` from inside `case` inside `while`
- [ ] builtins/unset.yaml :: Unset function — `unset funcname` should remove function

### Variable typing (1 case)

- [ ] variables.yaml :: Appending an integer to an integer variable — `declare -i y=value` → y=0 vs bash's y=value

### Well-known vars (2 cases)

- [ ] well_known_vars.yaml :: EPOCHSECONDS — `date +%s` comparison fails
- [ ] well_known_vars.yaml :: RANDOM — static between calls

### Command exit code (2 cases)

- [ ] word_expansion.yaml :: Command substitution exit code — `x=$(false)` should set $? to 1
- [ ] simple_commands.yaml :: Command that's a directory — should be exit 126, not 127

### Here doc (1 case)

- [ ] here.yaml :: Here doc with other tokens after tag — `<<EOF | wc -l` pipeline

### Pattern matching (1 case)

- [ ] patterns/patterns.yaml :: Patterns: quoting — `"a*"` should be literal, not glob

---

## TIMEOUT (3 cases)

- [ ] builtins/exec.yaml :: Exec a command — `exec $0 -c '...'` loops
- [ ] compound_cmds/arithmetic_for.yaml :: Continue in arithmetic for loop — infinite loop
- [ ] compound_cmds/until.yaml :: Single-line until loop — `until true` hangs

---

## Summary

| Category | Count | Status |
|----------|-------|--------|
| PASS | 161 | ✅ |
| SHOULD_FAIL (correct rejection) | 68 | ✅ verified |
| ICE / crash | 7 | ❌ bugs |
| Wrong output | 56 | ❌ needs work |
| Timeout | 3 | ❌ needs investigation |
| NA (out of scope) | 300 | — |
| SKIP (no stdin) | 27 | — |
| **Total actionable** | **66** | |

### Priority order for fixes

1. **@-syntax validator rule** — 6 WRONG_OUTPUT → SHOULD_FAIL (trivial)
2. **ICE: CST taint gaps** — 4 cases where SSA backstop fired
3. **Compound command redirects** — 4 cases, high value
4. **Array per-element operations** — 13 cases, one fix
5. **Arithmetic side effects** — 3 cases, design decision needed
6. **Extended test fixes** — 7 cases, mixed difficulty
7. **Remaining edge cases** — one-offs
