# Experiment 011: Validator framework

**Goal:** Unify the individual rule implementations from experiments
007–010 into a composable validation framework. Single CST walk,
multiple rules, collected diagnostics.

**Architecture:**

```
src/
  lib.rs            — parse(), module declarations
  diagnostic.rs     — Diagnostic struct, Severity enum, constructors
  rule.rs           — Rule trait (name, node_kinds, check)
  validator.rs      — validate() — single walk, dispatch to rules
  rules/
    mod.rs          — all_rules() registry
    dynamic_execution.rs   — SB4001 eval, SB4002 source, SB4004 $cmd
    indirect_expansion.rs  — SB4003 ${!var}
    unquoted_expansion.rs  — SC2086/SC2046
    backtick_substitution.rs — SC2006
    single_bracket_test.rs — SC2292
  main.rs           — CLI entry point
tests/
  integration.rs    — 37 cross-rule integration tests
```

**Key design decisions:**

- **Rule trait:** `name()`, `node_kinds()`, `check()`. Rules are
  stateless. The framework calls `check()` only for nodes matching
  the rule's declared `node_kinds()`. Empty `node_kinds()` means
  receive all nodes.

- **Single walk:** The framework walks the CST once and dispatches
  each node to all rules whose `node_kinds()` match. This is O(n)
  in CST size regardless of rule count.

- **Severity enum expanded:** Error, Coerce, Deferred, Marker.
  The Coerce level distinguishes rules that are errors in AOT/strict
  but auto-fixable in JIT/coerce.

- **Diagnostic constructors:** `Diagnostic::error()`, `::coerce()`,
  `::deferred()`, `::marker()` — convenience methods that extract
  line/col from the node.

- **Rule interaction bug found and fixed:** `$cmd arg` was flagged
  by both SB4004 (variable-as-command, correct) and SC2086
  (unquoted expansion, incorrect — command-name position doesn't
  undergo word splitting). Fixed by adding `command_name` to the
  safe-context list in `is_quoted_or_safe()`.

- **Output sorted by position:** Diagnostics are sorted by
  (line, col) regardless of rule firing order.

**Tests:** 37 integration tests covering all 5 rules individually,
cross-rule interaction (all rules fire together), diagnostic sorting,
rule registration, selective rule loading, safe script integration,
and position accuracy.

**Conclusion:** Phase 2 is complete. The framework makes adding a
new rule a matter of: write a struct implementing `Rule`, add it to
`all_rules()`, write tests. No changes to the walker or framework.

---

