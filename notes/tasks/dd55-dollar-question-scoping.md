title: DD#55 — $? scoping and restrictions
status: ready
depends:
  - dd55-suppress-prefix
severity: correctness
gates:
  - tests
  - commit

DD#55 restricts `$?` availability:

1. **After `- cmd`**: `$?` is a compile error. Suppression means
   you don't care about the exit status. Reading it is contradictory.

2. **Scoping**: `$?` is meaningful in handling position — `||` RHS,
   `if`/`while` body immediately after the tested command. Not as a
   general-purpose variable in subsequent unrelated statements.

### Implementation

Track "last command" state in a validation pass:
- After `- cmd`: mark `$?` as unavailable
- After `cmd || action` / `if cmd`: mark `$?` as available within
  the handling scope
- After bare `cmd` (with implicit check): `$?` is 0 (the command
  succeeded, otherwise we aborted). Reading it is valid but useless.

### `return $?`

Valid only when `$?` is available. `- cmd; return $?` → compile error.
`cmd || return $?` → valid (propagates failure to caller, makes
function non-void).
