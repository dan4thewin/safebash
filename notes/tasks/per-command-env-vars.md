title: Per-command env vars — delegate to /usr/bin/env
status: done
depends:
  - dd23-compile-time-resolve
gates: [53.6]
  - tests
  - commit

DD#53 says sash never manipulates the environment. Per-command env
vars are delegated to `/usr/bin/env`:

    /usr/bin/env FOO=bar /usr/bin/grep pattern file

## Coercion from bash

    # bash:
    FOO=bar grep pattern file

    # sash (coerced):
    /usr/bin/env FOO=bar /usr/bin/grep pattern file

The lowerer recognizes prefix assignments on commands and rewrites
to an env invocation. No env-var manipulation in the compiler or
runtime. `sash_setenv` has no remaining callers.

## What this replaces

The original plan was to implement `FOO=bar cmd` directly in sash
with a modified envp array. That required per-command env building
in codegen and `sash_setenv` in the runtime. Delegating to env
eliminates all of that.
