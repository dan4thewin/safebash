---
title: "implement getpid-class internal builtins (getpid/getppid/getcwd/epochseconds/random/argc); $$ = sugar for getpid"
status: ready
priority: P3
kind: feature
depends: []
gates:
  - "$(getpid), $(getppid), $(getcwd), $(epochseconds), $(random), $(argc) work as in-process internal builtins (no execve) and capture CLEAN"
  - "$$ is sugar for getpid (same value, clean)"
  - "getcwd is a builtin (not the external /usr/bin/pwd, which would be tainted)"
  - "native + c + janet; spec tests; no net regression"
---

# Implement the getpid-class internal builtins

These names sit in `cmd_subst_taint`'s clean list but are implemented
nowhere — they fail as `$(getpid)` and as direct commands. DG: build them
as real **internal builtins** (in-process, no execve → clean), and make
**`$$` sugar for `getpid`**. `getcwd` is the builtin form (use it instead
of the external `pwd`, which execs → tainted).

Runtime support likely needs `sash_getpid`/`sash_getppid`/`sash_getcwd`/
… (rustix syscalls); wire each into the cmdsub-capture / intrinsic path
(where getenv/argv/arg are handled) and the direct-command path.
