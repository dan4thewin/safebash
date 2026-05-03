title: DD#56 — EXIT block rework
status: ready
depends:
  - bug-exit-trap-wrong-scope
  - dd56-signum-keyword
severity: feature
gates:
  - tests
  - commit

Rework EXIT trap from "inlined before every Return" to a single
exit block in _start.

### Current (broken)

transform.rs ~line 37: EXIT trap body inlined before every Return
terminator. Fires on function returns (wrong) and misses `exit`
builtin.

### New design

Single `exit_block` in the SSA:
- Program's natural end: goto exit_block (signum=0)
- `exit N` builtin: set exit code, goto exit_block
- Exiting trap check: goto exit_block (signum already set by
  signal handler)

exit_block contains:
1. The programmer's `trap EXIT { ... }` body
2. Final `sash_exit(code)` call

If no EXIT trap declared, exit_block is just `sash_exit(code)`.

### Exit code passing

Need a mechanism to pass the exit code to the exit block:
- Normal exit: 0 (or last command's status)
- `exit N`: N
- Signal exit: conventional 128 + signum (programmer chooses in
  their case/esac)

Use a global or variable for the intended exit code, set before
jumping to exit_block.

### Function returns

Function returns are trampoline gotos (DD#27). They do NOT go
through exit_block. Remove all current Return-before-trap logic.
