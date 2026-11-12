title: DD#16 — Integer Type System
status: implemented

1. `int` is a signed 64-bit value type, distinct from strings.
2. `fn name -> int` declares an integer result; an arrow requires the `int`
   annotation, and every returning path must supply that kind (DD55/DD58).
3. Type crossings are explicit; arithmetic string operands require their
   disposition under DD77.

---

`int` variables store machine-width integers. `fn name -> int { }` for
typed return. Strict type separation — no implicit conversion between
int and string (see DD#30).

## Enforcement correction (2026-10-16)

~~Omitting the annotation retains the default string return.~~ DD55 distinguishes
unannotated void and status functions; stdout capture is a separate channel.
Once `->` is
present, `int` is required: `fn f -> { ... }` must not silently select that
default. Missing or unsupported types after the arrow are SB4014, naming the
missing `int`. This applies to every function-definition spelling through the
shared parser tail. Specs `rty.1`-`rty.6`; journal 511.
