# Experiment 008: Unquoted expansion detection (SC2086, SC2046)

**Goal:** Detect unquoted parameter expansions and command
substitutions — the single most common source of Bash bugs.

**Actions:**
- Used a CST dumper to study tree-sitter-bash's representation of
  quoted vs unquoted expansions across all relevant contexts
- Key insight: an expansion node (`simple_expansion`, `expansion`,
  `command_substitution`) is "unquoted" when it is NOT inside a
  `string` node (double quotes) and NOT in a safe context
- Implemented `is_quoted_or_safe()` which walks the ancestor chain
  to classify context

**Safe contexts identified (no word splitting occurs):**
- Inside `string` node (double quotes)
- RHS of `variable_assignment` (x=$var — no splitting on RHS of =)
- Inside `test_command` with `[[` opener (but NOT `[`)
- Inside `arithmetic_expansion` $(( ))
- Inside `heredoc_body`
- `case_statement` discriminant (the [value] field)

**Unsafe despite being inside assignment:**
- Inside `array` node: `arr=($var)` — splitting occurs inside ( )

**Diagnostic codes:**
- SC2086 for unquoted `$var`, `${var}`, `$1`, `$@`, etc.
- SC2046 for unquoted `$(cmd)` command substitution

**Tests:** 29 tests covering:
- 12 unquoted patterns that should flag (simple, braced, positional,
  special, command sub, default, args, for-value, concat, array, [ ])
- 8 quoted patterns that should pass (same set, double-quoted)
- 7 safe-context patterns (assignment, local, export, [[ ]], $(()),
  heredoc, case)
- Multi-violation and position accuracy tests
- Complex safe script integration test

**Conclusion:** The ancestor-chain walk approach is clean and
handles all contexts correctly. The same pattern will work for
other context-dependent rules (e.g., [ ] vs [[ ]] detection in 010).

---

