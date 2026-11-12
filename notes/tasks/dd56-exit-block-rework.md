title: DD#56 — END block implementation
status: done
depends: [bug-exit-trap-wrong-scope]
severity: feature
gates: [56.1, 56.2a, 56.2b, 56.3, 56.4]

Implement END { ... } as a top-level declaration.

### Parser
Recognize `END {` ... `}` at top level. Exactly one allowed.

### Transform
Emit end_block in SSA:
- Program's final exit: goto end_block (signum=0)
- `exit N`: set code, goto end_block
- Signal EXIT check: goto end_block (signum already set)

### end_block contents
1. User's END body
2. Final sash_exit(code)

If no END declared, end_block is just sash_exit(code).

### Exit status
- `exit N` in END → override exit status
- `return` in END → terminate (same as reaching end)
- Falling off end → exit with triggering status
