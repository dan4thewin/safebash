title: DD#25 — implement arithmetic overflow traps
status: ready
depends: []
gates: [25.1]

Replace wrapping `iadd`/`isub`/`imul` in `emit_arith_op` with
overflow-checked sequences. On overflow: print error to stderr,
set `$?=1`, exit (matching bash behavior for `((2**63))` etc.).

Cranelift doesn't have checked-add instructions, so the pattern is:
compare-and-branch around a trap or error path, similar to the
existing div-by-zero handling.

Also handle `pow` overflow in `sash_pow` runtime function.

Ref: DD#25, SUBSET.md arithmetic semantics paragraph.
