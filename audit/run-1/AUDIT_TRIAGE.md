# Adversarial Audit Triage — safebash run-1

**Date:** 2026-04-12
**Models:** gpt-4.1, gpt-5.2, gpt-5.3-codex, claude-sonnet-4, claude-opus-4.5
**Passes:** 3 (bypass, IR fidelity, spec coverage)

---

## Pass 1: Bypass Findings (Security)

### B1: `builtin eval` / `command eval` bypass
**Category:** PRODUCTION BUG
**Severity:** CRITICAL
**Found by:** 5/5 models (unanimous)
**Description:** `builtin eval "$cmd"` and `command eval "$cmd"` achieve
arbitrary code execution. SB4001 only checks if the command word is
literally `"eval"`. When prefixed with `builtin` or `command`, the
command word becomes `"builtin"` or `"command"`, and `eval` is just
an argument.
**Fix:** Extend SB4001 to check whether the first argument to
`builtin`/`command` is a rejected command name.

### B2: `bash -c` / `sh -c` arbitrary execution
**Category:** PRODUCTION BUG
**Severity:** CRITICAL
**Found by:** 4/5 (opus-4.5, sonnet-4, gpt-4.1, codex)
**Description:** `bash -c "$payload"` achieves arbitrary code execution.
No rule checks for shell interpreters invoked with `-c`. This is
semantically equivalent to `eval`.
**Fix:** New rule SB4005: reject `bash`, `sh`, `dash`, `zsh`, `ksh`
with `-c` argument.

### B3: `trap` with code string
**Category:** PRODUCTION BUG
**Severity:** CRITICAL
**Found by:** 3/5 (codex, opus-4.5, sonnet-4)
**Description:** `trap "$payload" EXIT` executes arbitrary code at
signal/exit time. The command string is eval'd by bash. No rule
checks `trap` arguments.
**Fix:** New rule: reject `trap` with non-literal first argument,
or reject `trap` with string arguments entirely (require function
references).

### B4: `$'\x65\x76\x61\x6c'` (ANSI-C quoting obfuscation)
**Category:** PRODUCTION BUG
**Severity:** CRITICAL
**Found by:** 1/5 (opus-4.5 only)
**Description:** `$'\x65\x76\x61\x6c'` decodes to `eval` at parse
time. tree-sitter parses this as `ansi_c_string`, not `word`, so
the literal-text comparison misses it.
**Fix:** Extend command-name extraction to decode ANSI-C strings
before comparison.
**Note:** Single-model find but clearly real and verifiable.

### B5: `alias e=eval; e` (alias-based bypass)
**Category:** SECURITY GAP
**Severity:** HIGH
**Found by:** 1/5 (opus-4.5)
**Description:** Define an alias expanding to a rejected command.
The validator sees the alias name, not the expansion. However:
aliases are typically not expanded in non-interactive scripts
unless `shopt -s expand_aliases` is set.
**Fix:** Reject `alias` assignments whose expansion starts with a
rejected command. Or reject `shopt -s expand_aliases` entirely.
**Note:** Mitigated by default bash behavior in scripts, but a
real gap if aliases are enabled.

### B6: `exec /bin/sh -c` / `exec` not checked
**Category:** SECURITY GAP
**Severity:** HIGH
**Found by:** 2/5 (opus-4.5, sonnet-4)
**Description:** `exec` can replace the process with arbitrary
commands. Not in the reject list.
**Fix:** Add `exec` to the set of commands checked for dangerous
arguments (same as B2).

### B7: `xargs ... sh -c` pattern
**Category:** SECURITY GAP
**Severity:** HIGH
**Found by:** 1/5 (opus-4.5)
**Description:** `echo 'payload' | xargs -I{} sh -c '{}'` achieves
code execution through xargs. Hard to catch generically — xargs
is just a command with arguments.
**Fix:** Specific pattern check for xargs + shell invocation, or
document as out-of-scope (external tool behavior).

### B8: `env eval` / `env bash -c`
**Category:** SECURITY GAP
**Severity:** HIGH
**Found by:** 1/5 (opus-4.5)
**Description:** `env` as a command prefix hides the real command
from the validator.
**Fix:** Same approach as B1 — check arguments to `env` for
rejected commands.

### B9: Nameref with untrusted target
**Category:** HARDENING
**Severity:** MEDIUM
**Found by:** 3/5 (gpt-4.1, codex, opus-4.5)
**Description:** `declare -n ref="$1"` allows writing to arbitrary
variables. By design (namerefs are allowed per design decision #3),
but risky when the target comes from untrusted input.
**Note:** Accepted risk per design review. Document in SUBSET.md
threat model.

### B10: Process substitution `<(cmd)` unscrutinized
**Category:** TEST GAP
**Severity:** MEDIUM
**Found by:** 2/5 (sonnet-4, opus-4.5)
**Description:** Commands inside `<(...)` execute without special
validation. Working as designed — the commands inside are validated
by the normal rules — but process substitution itself isn't in the
IR yet (see IR finding I2).

---

## Pass 2: IR Fidelity Findings

### I1: Heredoc expansion mode lost
**Category:** PRODUCTION BUG
**Severity:** HIGH
**Found by:** 3/5 (opus-4.5, gpt-4.1, gpt-5.2)
**Description:** `<<EOF` (expand variables) vs `<<'EOF'` (literal)
vs `<<-EOF` (strip tabs) — the IR doesn't distinguish these. The
lowering treats all heredocs the same.
**Fix:** Add `quoted: bool` and `strip_tabs: bool` fields to the
heredoc representation (or to Redirected statement).

### I2: Process substitution not handled
**Category:** PRODUCTION BUG
**Severity:** HIGH
**Found by:** 3/5 (opus-4.5, sonnet-4, gpt-4.1)
**Description:** `<(cmd)` and `>(cmd)` are valid bash constructs
parsed by tree-sitter but not represented in the IR. They silently
become literals or are dropped.
**Fix:** Add `ProcessSubstitution` to ExprKind.

### I3: Brace expansion not handled
**Category:** PRODUCTION BUG
**Severity:** HIGH
**Found by:** 2/5 (opus-4.5, gpt-5.2)
**Description:** `{a,b,c}` and `{1..10}` are not represented in
the IR. Brace expansion produces multiple arguments but the IR
treats it as a single literal.
**Fix:** Add `BraceExpansion` to ExprKind, or reject/document as
out of safe subset.

### I4: `select` loop not handled
**Category:** PRODUCTION BUG
**Severity:** HIGH
**Found by:** 2/5 (opus-4.5, gpt-5.2)
**Description:** `select x in a b c; do ... done` is a valid bash
construct but not in StmtKind. Silently dropped.
**Fix:** Add `StmtKind::Select` or reject as out of safe subset.

### I5: Coproc not handled
**Category:** PRODUCTION BUG
**Severity:** HIGH
**Found by:** 2/5 (opus-4.5, gpt-5.2)
**Description:** `coproc name { cmd; }` is parsed by tree-sitter
but not in the IR. Silently dropped.
**Fix:** Reject as out of safe subset (concurrency is excluded).

### I6: `trap` not semantically represented
**Category:** PRODUCTION BUG
**Severity:** MEDIUM
**Found by:** 3/5 (opus-4.5, gpt-4.1, codex)
**Description:** `trap 'cleanup' EXIT` is lowered as a generic
command, losing the signal-handler semantics. The compiler can't
reason about cleanup paths or signal behavior.
**Fix:** Add `StmtKind::Trap` or handle in the validator as a
special command (see B3).

### I7: Array subscript assignment `arr[0]=value` not preserved
**Category:** PRODUCTION BUG
**Severity:** HIGH
**Found by:** 2/5 (opus-4.5, gpt-4.1)
**Description:** `arr[0]=value` and `arr+=value` are not
distinguished from plain assignment in the IR.
**Fix:** Add index/subscript field to Assignment, add `+=`
operator variant.

### I8: `declare` flags not semantically interpreted
**Category:** TEST GAP
**Severity:** MEDIUM
**Found by:** 2/5 (opus-4.5, gpt-4.1)
**Description:** `declare -i`, `declare -A`, `declare -a` flags
are preserved as strings but not interpreted. The compiler can't
know that `-i` means integer arithmetic or `-A` means associative
array.
**Fix:** Future work — type system in SSA phase.

### I9: `[ ]` vs `[[ ]]` conflated in IR
**Category:** TEST GAP
**Severity:** MEDIUM
**Found by:** 1/5 (gpt-4.1)
**Description:** Both test forms lower to the same IR. The
validator catches `[ ]` (SC2292) but the IR doesn't preserve
which form was used.
**Fix:** Add a `bracket_form` field to the test expression, or
document as intentional (coerce mode transforms `[ ]` to `[[ ]]`
so the IR only needs to represent `[[ ]]`).

---

## Pass 3: Spec Coverage Findings

### S1: Most spec rules not implemented
**Category:** EXPECTED GAP (known)
**Severity:** N/A
**Found by:** 5/5 (unanimous)
**Description:** Only 8 rules are implemented (SB4001–4004, SC2086,
SC2046, SC2006, SC2292). The spec lists ~60 rules across 9
categories. This is expected — experiments 007–010 were foundational,
with remaining rules planned for future work.
**Action:** Not a finding. Track in TODO.md.

### S2: SC2206/SC2207 — spec says reject, code coerces
**Category:** INCONSISTENT
**Severity:** MEDIUM
**Found by:** 3/5 (opus-4.5, codex, gpt-4.1)
**Description:** Unquoted expansion in array assignment
`arr=($var)` — spec says reject (use mapfile), but SC2086 catches
it as a coerce. Severity mismatch.
**Fix:** Array-context check in unquoted_expansion to emit reject
instead of coerce.

### S3: `builtin`/`command` wrapper not in spec
**Category:** MISSING_SPEC
**Severity:** HIGH
**Found by:** 3/5 (gpt-5.2, codex, opus-4.5)
**Description:** The spec doesn't mention that `builtin eval` or
`command eval` should be rejected. The bypass exists (B1) and the
spec should explicitly address it.
**Fix:** Add to SUBSET.md under dynamic execution category.

### S4: Coerce/strict mode not implemented
**Category:** INCONSISTENT
**Severity:** MEDIUM
**Found by:** 1/5 (codex)
**Description:** Spec defines AOT-strict vs JIT-coerce modes
(design decision #8), but the CLI always treats coerce diagnostics
as causing nonzero exit. No mode toggle exists.
**Fix:** Future work — add `--coerce` flag to CLI.

---

## Priority Summary

**Fix now (CRITICAL bypass — blocks security claims):**
1. B1: `builtin eval` / `command eval` → extend SB4001
2. B2: `bash -c` / `sh -c` → new SB4005
3. B3: `trap` with code string → new rule
4. B4: ANSI-C quoting obfuscation → extend command name extraction

**Fix soon (HIGH — real gaps):**
5. B5: alias bypass → reject alias for rejected commands
6. B6: `exec` not checked → add to command list
7. B8: `env` wrapper → check arguments
8. S3: Update SUBSET.md for wrapper commands

**Track (IR — needed for correct compilation):**
9. I1: Heredoc modes
10. I2: Process substitution
11. I3: Brace expansion
12. I4: select loop
13. I7: Array subscript assignment

**Defer (medium/low):**
14. I5: Coproc → reject as out of subset
15. I6: trap semantics → covered by B3 rule
16. S2: Array reject vs coerce
17. S4: Mode toggle
