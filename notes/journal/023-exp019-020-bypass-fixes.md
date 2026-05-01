# Experiment 019: Critical bypass fixes (B1–B4)

**Date:** 2026-04-12

**Goal:** Close the four CRITICAL vulnerabilities identified by the
adversarial audit (run-1).

**Actions:**

Rewrote `dynamic_execution.rs` from a simple command-name matcher to a
multi-layer command analyzer:

1. **B1: Wrapper unwrapping** — `builtin eval`, `command eval` now
   detected. The rule extracts the effective command by looking past
   `builtin`/`command` wrappers. Same treatment for `source`/`.`
   through wrappers.

2. **B2: Bourne-lineage `-c`** — `bash -c`, `sh -c`, `dash -c`,
   `ksh -c`, `zsh -c` detected. Literal string argument → Marker
   (will be compiled as inline subshell per decision #10). Variable
   argument → Error (compiler can't see the code). Works through
   `command` and `env` wrappers.

3. **B3: `trap` code strings** — `trap 'literal code' SIG` → Marker
   (will be compiled inline, same principle as B2). `trap "$var" SIG`
   → Error. `trap funcname SIG` → allowed (function reference).
   `trap -` and `trap -p` → allowed.

4. **B4: ANSI-C obfuscation** — `$'\x65\x76\x61\x6c'` (decodes to
   `eval`) now caught. Added `decode_ansi_c_string()` handling hex,
   octal, unicode, and named escapes. Applied to all command-name
   extraction paths.

5. **B8: `env` wrapper** — `env [-i] [-u X] [K=V]... cmd` now
   resolved. Skips env flags and assignments, finds the effective
   command, applies all checks. `env FOO=1 bash -c "$cmd"` caught.

**Architecture:**

- `extract_command_word()` — resolves word or ANSI-C string to literal,
  returns None for expansions
- `collect_args()` — gathers argument nodes after command name
- `check_command()` — dispatches to eval/source/shell-c/trap checks
- `check_wrapper()` — unwraps builtin/command, recurses to check_command
- `check_env()` — unwraps env flags/assignments, recurses to check_command
- `check_shell_c()` — detects -c flag, classifies literal vs variable
- `check_trap()` — classifies function ref / literal / variable
- `is_literal_string()` — checks raw_string, ansi_c_string, or
  expansion-free double-quoted string
- `decode_ansi_c_string()` — full ANSI-C escape decoder

**New diagnostic codes:**
- SB4005: Bourne-lineage shell with -c (error if variable, marker if literal)
- SB4006: trap with code string (error if variable, marker if literal)

**Refinement of SB4001:** `eval` with all-literal arguments now emits
Marker instead of Error (will be compiled inline, recursive parse
pending). `eval` with variable arguments remains Error.

**Tests:** 37 integration tests covering all four bypasses, wrapper
combinations, env resolution, regression on original rules, complex
script with all bypasses, and safe script with no false positives.

**Bug found:** `check_trap` was calling `utf8_text(&[])` (empty source
slice) instead of passing the actual source bytes, causing a panic on
any trap with a bare word argument. Fixed by threading `source`
parameter through.

---

## Experiment 020: HIGH gap fixes (B5, B8, coproc)

**Date:** 2026-04-12

**Goal:** Close remaining HIGH findings from adversarial audit run-1.

**Actions:**

1. **B5: Alias restriction (Decision #11)** — New rule `alias_restriction.rs`
   with `AliasRestriction` struct. Handles three commands:
   - `alias name='expansion'` — tree-sitter parses as a `concatenation`
     argument (word "name=" + raw_string "'expansion'"). Dangerous
     expansions (eval, source, bash, sh, etc.) → Error. Safe expansions
     → Coerce with function conversion in the message.
   - `unalias` → Coerce (implies aliases exist).
   - `shopt -s expand_aliases` → Error SB4009 (enables text-macro
     expansion that defeats static analysis).

2. **Coproc rejection** — `UnsupportedConstructs` rule rejects `coproc`
   with SB4013 (concurrency is out of safe subset permanently).

3. **Dropped B6 (exec)** — `exec` with a command is just running a
   program; all existing rules already apply to the command being exec'd.
   Nothing to check that isn't already covered.

4. **Dismissed B7 (xargs)** — Out of scope. safebash doesn't analyze
   external program arguments. To be documented in SUBSET.md.

**Tree-sitter discovery:** `alias ll='ls -la'` parses as:
```
command
  command_name [name]: word "alias"
  concatenation [argument]: word "ll=" + raw_string "'ls -la'"
```
Not inside `command_name` as initially assumed. The assignment is a
regular command argument, which required rewriting the check logic.

**New diagnostic codes:**
- SB4008: alias rejected (Error for dangerous, Coerce for safe)
- SB4009: shopt -s expand_aliases rejected (Error)
- SB4012: select rejected (not yet implemented — needs node kind check)
- SB4013: coproc rejected (Error)

**Tests:** 21 integration tests covering alias classification,
dangerous vs safe alias detection, unalias, shopt, coproc, regression
on all 019 rules, complex script integration, and false positive check.

---

