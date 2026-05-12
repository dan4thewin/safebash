title: BUG — EXIT trap fires on function returns, not before exit
status: done
severity: bug
gates: [56.1, 56.2a, 56.2b, 56.3, 56.4]
---

Fixed: END block model replaces trap-inline approach.
Dedicated end_block in SSA, all exit paths goto it.
DD#55 abort blocks jump to end_block instead of sash_exit.
