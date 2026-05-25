---
title: DD Candidates
modified: 2026-05-24
---

# DD Candidates

Design decisions made or surfaced during the parser migration
(2026-05-24 session). Some are implemented, some need formal DDs.

---

## Compound-command postfix operators (IMPLEMENTED)

**Decision**: All postfix operators on compound commands are rejected
(SB4021-4024): redirects, pipes, &&, ||, &.

**Sash idiom**: Wrap in a function for composition.

**Error codes**: SB4021 (redirects), SB4022 (pipes), SB4023 (&&/||),
SB4024 (&).

**Spec tests**: 14.4-14.6 rewritten to expect rejection.

**Converter task**: `notes/tasks/converter-compound-postfix.md` —
converter must rewrite these patterns to function-wrapped form.

---

## Heredocs removed from language (IMPLEMENTED)

**Decision**: Heredocs (`<<` / `<<-`) are not part of sash. Rejected
at scanner level with SB5002.

**Alternative**: Use herestrings (`<<<`) for single-line input, or
variables for multi-line content.

**Rationale**: Heredocs complicate parsing (multi-line delimiter
tracking), encourage embedding untrusted data inline, and the
content undergoes expansion which is a taint risk.

---

## Backslash is literal in unquoted context (IMPLEMENTED)

**Decision**: Backslash (`\`) in unquoted words is a literal character,
not an escape. Escape sequences only apply inside double-quoted strings
and `$'...'` ANSI-C strings.

**Rationale**: Sash eliminates word splitting and restricts globbing.
The cases where unquoted backslash escaping is needed in bash (escaping
spaces, `$`, glob characters) don't exist in sash. Making `\` literal
simplifies the language and allows regex patterns (`\d`, `\w`, `\s`) to
pass through naturally.

**Impact**: `\n` in unquoted context is the two characters `\` and `n`,
not a newline. Use `$'\n'` or `"...\n..."` for actual escape sequences.

---

## Braces only meaningful at statement boundaries

**Question**: Where are `{` and `}` meaningful in sash?

**Current answer**: Function bodies (`fn name { ... }`) and groups
in error-handler context (`cmd || { ... }`). No brace expansion.

**Open question**: Are groups (`{ ... }`) needed at all? They serve
as multi-statement error handlers in `cmd || { s=$(status); handle; }`.
Without groups, every handler would need a named function. DG
described them as "conceptually anonymous functions immediately invoked."

**Status**: Groups remain. The scanner emits `Tok::LBrace` and
`Tok::RBrace` unconditionally. Inside `[[ =~ regex ]]`, braces in
patterns are handled by `parse_regex_rhs` which collects them as
literal characters.

---

## Taint rule: source determines taint, not command (IMPLEMENTED)

**Decision**: Taint is determined by the data source. External sources
(wherever there is an exec, a file read, or an env var read) are
ALWAYS tainted.

- Pipe from builtin (echo, true — no exec) → clean
- Pipe from external command (printf, /bin/anything — execs) → tainted
- `$(getenv NAME)` → tainted (env var read)
- `$(argv N)` → tainted (external input)
- `$(/bin/cmd)` → tainted (exec)
- `$(user_func)` → clean (no exec, sash-internal)
- `read x <<< "literal"` → clean (no external source)
- `read x` (from stdin) → tainted (external source)

**Key principle**: A pipe from a forked child that does NOT exec
would not be tainted. But any command that execs produces tainted
data.

**Builtin list**: echo, true, false, :, read, mapfile, bless, getenv,
argv, argc, shift, unset, [[, trap, signal, wait, kill, signum, cd,
status, getpid, getppid, getcpid, getcwd, epochseconds, dtoi, itod.

**NOT builtins** (require exec): printf, cat, grep, sed, awk, wc, etc.

---

## $? state machine (IMPLEMENTED)

**Decision**: `$?` / `$(status)` follows a state machine:

- **Undefined**: no preceding command, or consumed → use is error
- **Available**: set after any non-suppressed command executes
- **Consumed**: after a statement uses `$?`, resets to undefined

In `||` branches: inherited status available only for the first
statement. After any other statement executes in the branch, the
original status is gone.

**Rationale**: Prevents stale-status bugs where `$?` refers to
something other than the intended command.

---

## Bless: variable must be LHS (IMPLEMENTED)

**Decision**: In `bless x [[ condition ]]`, the blessed variable must
appear as the LHS (subject) of the binary test expression.

- Valid: `bless x [[ $x =~ ^pattern$ ]]`
- Valid: `bless x [[ -n $x ]]`
- Invalid: `bless x [[ "foo" = $x ]]` (variable on RHS = pattern injection risk)
- Invalid: `bless x [[ 1 -eq 1 ]]` (condition doesn't reference variable)

---

## printf is not a builtin (IMPLEMENTED)

**Decision**: `printf` is not a sash builtin. It resolves to
`/usr/bin/printf` via PATH and requires exec.

**Impact**: `printf ... | read x` produces tainted data (external
source). Use `echo` for clean pipe sources.

---

## DD#31 comma expressions in conditions (IMPLEMENTED)

**Decision**: `(( x=1, x > 0 ))` in if/while is valid per DD#31.
Non-final parts are mutations (validated as statement context), final
part is comparison (validated as condition context).

**Implementation**: When a Group in condition context has a trailing
ArithTest, earlier statements are validated as non-condition (the
mutation part of the desugared comma expression).

---

## Runtime regex abort per DD#55 (IMPLEMENTED)

**Decision**: Invalid regex at runtime (dynamic pattern that fails
regcomp) causes program abort with exit code 2. Per DD#55: "Runtime
calls that can fail on data abort."

Literal regex patterns are validated at compile time (SB5023).
Dynamic patterns (containing $variables) defer validation to runtime.
