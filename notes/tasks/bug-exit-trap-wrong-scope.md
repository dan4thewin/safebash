title: BUG — EXIT trap fires on function returns, not before exit
status: ready
severity: bug
gates: [56.1, 56.2a, 56.2b, 56.3, 56.4]
---

Two bugs in current EXIT/trap implementation:

1. transform.rs inlines EXIT trap body before every `Return`
   terminator — fires on function returns (wrong).
2. `exit` builtin bypasses trap body entirely.

### Fix (DD#56 redesign)

Replace with END block model:
- Single `end_block` in SSA, reached from: final exit path,
  `exit` builtin, exiting signal checks.
- Function returns (trampoline gotos) do NOT enter end_block.
- END block always terminates the process.

### Ref

transform.rs ~line 37, codegen.rs ~line 1552, DD#56
