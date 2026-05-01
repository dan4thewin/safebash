# Experiment 009: Reject backtick substitution (SC2006)

**Goal:** Detect legacy backtick command substitution and flag it
for conversion to the modern $(...) form.

**Actions:**
- Probed tree-sitter-bash CST to confirm both backtick and $()
  parse as `command_substitution` nodes, distinguished by first
  child: `` ` `` for backticks, `$(` for modern form
- Implemented validator: walk the CST, find `command_substitution`
  nodes whose first child is `` ` ``, emit SC2006

**Simplest experiment so far:** The detection is a single condition
on the first child of `command_substitution`. No context analysis
needed (unlike SC2086 which requires ancestor-chain walking).
Backticks are always wrong regardless of context.

**Tests:** 14 tests covering:
- 7 backtick patterns (assignment, arg, double-quoted, if condition,
  variable, multiple, nested inside modern $())
- 5 modern $() patterns that should pass
- Position accuracy test
- Complex safe script integration test

**Conclusion:** SC2006 detection works. In coerce mode (future),
the transformation is mechanical: replace `` ` `` delimiters with
`$(` and `)`, remove backslash escaping of inner backticks.

---

