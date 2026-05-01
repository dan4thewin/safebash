# Experiment 005: parse script — real-world construct coverage

**Goal:** Parse a script resembling a real deployment tool: shebang,
`set -euo pipefail`, `readonly`, functions, `trap`, `for` loop with
glob, pipeline. Verify tree-sitter handles all of these without errors.

**Actions:**
- Wrote a 25-line script with: set, readonly, function definition,
  trap, mkdir, mount, for/in/do/done, cp, command substitution
- Tests verify: `function_definition`, `for_statement`, `pipeline`,
  `variable_assignment`, `if_statement` all appear in the CST
- Test: `complex_script_has_no_errors()` — tree-sitter reports no
  parse errors for a shellcheck-clean script

**Results:**
- All 6 tests pass
- tree-sitter-bash handles: shebang, set, readonly, functions,
  command substitution `$(...)`, trap, for/glob, pipelines, if/then/fi
- `has_error()` correctly returns false for clean scripts

**Conclusion:** tree-sitter-bash parses everything we need. Phase 1
is complete. The parser handles the full range of constructs that
appear in production deployment scripts. Ready to move to safe subset
validation in phase 2.

---

