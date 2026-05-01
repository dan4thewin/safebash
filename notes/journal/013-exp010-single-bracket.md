# Experiment 010: Reject single-bracket test / test builtin (SC2292)

**Goal:** Detect `[ ]` and the `test` builtin, flag for conversion
to `[[ ]]`.

**Actions:**
- Probed tree-sitter-bash CST: `[ ]` and `[[ ]]` both parse as
  `test_command` nodes, distinguished by first child (`[` vs `[[`).
  The `test` builtin parses as a regular `command` node with
  command_name "test".
- Implemented two checks: `check_test_command` (first child is `[`)
  and `check_test_builtin` (command name is "test")
- Both emit SC2292 with distinct messages

**Tests:** 21 tests covering:
- 6 single-bracket patterns (unary, binary, -a compound, in if,
  while, elif)
- 4 test-builtin patterns (unary, binary, in if, message check)
- 5 double-bracket patterns that should pass (unary, binary, &&
  compound, in if, regex)
- 2 non-test commands that should pass
- Multi-violation, position accuracy, complex safe script

**Conclusion:** Same pattern as 009 — detection by node kind and
first-child inspection. Simple and reliable.

---

