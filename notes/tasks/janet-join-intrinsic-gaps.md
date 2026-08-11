---
title: "janet: join intrinsic gaps — bare-name join a, and separator join @a SEP"
status: ready
priority: P3
kind: bug
depends: []
effort: effort-janet-backend-parity
gates:
  - "janet `$(join a)` (bare-name array) space-joins, instead of exec'ing /usr/bin/join"
  - "janet `$(join @a SEP)` / `$(join a SEP)` joins with SEP, instead of erroring (`bad slot, expected string, got array`)"
  - "un-guard spec/types.sh 33.19–33.21 to include janet once both work"
---

# janet join intrinsic gaps

Surfaced 2026-08-11 while implementing DD#75 join (`join arr SEP`, array-first).
janet's join only handles the `@a` **space**-join (`$(join @a)` → `(join a " ")`,
works). Two gaps:

- **bare-name `join a`** → janet emits an external-command exec (`join a`),
  running `/usr/bin/join` instead of the intrinsic. native/c resolve bare `a`
  to the array; janet's emit doesn't.
- **separator `join @a SEP`** → janet emits a call that passes the array to a
  janet builtin expecting a string → `error: bad slot #1, expected string, got
  <array>`. The separator isn't wired through janet's join emission.

Guarded `spec/types.sh` 33.19–33.21 to `native,c` meanwhile. The arg-order and
the native non-array reject (SB4030) are backend-independent and already land.
