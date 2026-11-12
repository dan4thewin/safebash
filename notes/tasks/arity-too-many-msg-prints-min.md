title: "`arity_too_many_msg` prints `min` where it means `max`"
status: done
priority: P4
kind: bug
depends: []
gates:
  - "a too-many-arguments diagnostic names the maximum, not the minimum"

# What

`arity_too_many_msg` formats the *minimum* arity into a message about having
supplied too many arguments. For any intrinsic whose min and max differ, the
number the author is told is the wrong one.

Cosmetic, but a diagnostic that states a false number is worse than one that
states none.

# Where

- `sash/src/validate.rs` — `arity_too_many_msg`

Filed 2026-09-29, journal 444.


# Done 2026-09-29 — journal 444b

`fixed_arity_msg` passed `min` into the "takes N arguments" slot. Invisible
while min == max, which is why it survived — it is wrong only for a DD#84
callee with trailing optionals:

```sash
fn f { /bin/echo ${args[0]} ${args[1]:-d} }
f a b c
→ SB5043 function 'f' takes 1 argument but was called with 3
```

`f` takes one argument or two, so the message denied that `f a b` is legal. It
now reports the range through the existing `arity_range`: "takes 1 to 2
arguments", or "takes exactly 1 argument" when they coincide.

Specs 81.30, 81.31.
