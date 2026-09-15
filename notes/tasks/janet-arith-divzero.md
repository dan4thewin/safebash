---
title: "janet backend: disposition for a zero divisor (DD#25 amendment)"
status: open
priority: P4
kind: feature
depends: []
gates:
  - "janet `$(( 1 / z ))` with z=0 has a decided, documented disposition"
  - "DD#25's janet paragraph is updated to state it"
---

DD#25's amendment (2026-09-14) made a zero divisor abort with
`<argv0>: division by zero`, exit 2, in the **native and c** backends,
and made a folder-proven zero divisor a compile-time error (SB5077).
The SB5077 half is backend-independent and already applies to janet —
it fires in `validate.rs`, before any backend runs.

The runtime half does not. Janet uses float/bignum, so it cannot take
an x86 `#DE` and has no `sash_arith_overflow` equivalent; `(/ 1 0)` in
janet is an error but not sash's error, and `(/ 1.0 0)` is `inf`. This
is the same shape as `bug-janet-int-overflow` (wontfix), and the
natural disposition is probably the same.

**DG deferred this explicitly: "TBD much later."** Do not decide it
unilaterally — the choice is whether janet joins the abort contract or
is documented as a known divergence, which is a language-surface
question, not an implementation detail.

No spec cost either way at present: arithmetic-fault specs are
`in_modes native,c` and janet is not in the sweep.
