---
title: "`arity_too_many_msg` prints `min` where it means `max`"
status: open
priority: P4
kind: bug
depends: []
gates:
  - "a too-many-arguments diagnostic names the maximum, not the minimum"
---

# What

`arity_too_many_msg` formats the *minimum* arity into a message about having
supplied too many arguments. For any intrinsic whose min and max differ, the
number the author is told is the wrong one.

Cosmetic, but a diagnostic that states a false number is worse than one that
states none.

# Where

- `sash/src/validate.rs` — `arity_too_many_msg`

Filed 2026-09-29, journal 444.
