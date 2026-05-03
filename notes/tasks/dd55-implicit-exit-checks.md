title: DD#55 — implicit exit checks in codegen
status: ready
depends: []
severity: correctness
gates:
  - tests
  - commit

Codegen stores exit status after `sash_run` (external commands) but
never checks it. DD#55 requires: every operation that can fail at
runtime gets an implicit nonzero → abort check.

### What needs checks

- `sash_run` return value (external commands): emit `brif nonzero → abort`
  after every call, using `sash_exit(status)` or a dedicated abort path
- `sash_wait` return value (foreground wait)
- `echo`/`printf` via `sash_write` — check return value
- `cd` via `sash_chdir` — check return value
- `regcomp9` on dynamic patterns — already returns null, but caller
  treats as no-match instead of abort

### What does NOT get checks

- Variable assignment
- Arithmetic evaluation (separate DD#25 overflow mechanism)
- String operations
- Literal regex (validated at compile time)

### Implementation

After each checked call, emit:
```
result = call sash_run(...)
zero = iconst 0
ok = icmp eq result, zero
brif ok → continue, abort_block
```

Abort block: write diagnostic to stderr, call `sash_exit(status)`.

### Condition position exemption

Commands in `if`/`while` condition and LHS of `&&`/`||` do NOT get
implicit checks — their exit status is the branch condition.
Transform.rs already handles this for `if`/`while`; need to verify
`&&`/`||` chains also exempt.
