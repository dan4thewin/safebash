title: BUG — EXIT trap fires on function returns, not before exit
status: ready
severity: bug
depends: []
gates:
  - tests
  - commit

Two bugs in EXIT trap implementation:

1. **Fires on function returns.** transform.rs inlines the EXIT trap
   body before every `Return` terminator. In the trampoline model
   (DD#27), function "returns" are gotos — only `_start`'s final
   return is a real program exit. The trap body runs (and duplicates)
   at every function return.

2. **Does not fire before `exit` builtin.** codegen.rs emits
   `sash_exit(code)` directly with no trap body. EXIT trap should
   run before explicit `exit` calls.

### Fix

Emit the EXIT trap body as a single block (`exit_trap_block`).

- Program's final exit path: goto `exit_trap_block` instead of
  `Return`.
- `exit` builtin: goto `exit_trap_block` instead of calling
  `sash_exit` directly. Pass exit code via a variable.
- `exit_trap_block`: runs trap body, then calls `sash_exit(code)`.
- Function returns (trampoline gotos): unchanged — do NOT go
  through the trap block.

### Test cases needed

- EXIT trap with function calls (verify trap runs once at exit,
  not at each function return)
- EXIT trap with explicit `exit N` (verify trap runs)
- EXIT trap with normal program completion (verify trap runs)
- Multiple functions with returns (verify no trap duplication)

### Ref

transform.rs ~line 37 (EXIT trap inlining)
codegen.rs ~line 1552 (exit builtin)
DD#27 (trampoline model)
