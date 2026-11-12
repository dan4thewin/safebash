title: "janet backend: disposition for a zero divisor (DD#25 amendment)"
status: deferred
priority: P4
kind: feature
depends: []
gates:
  - "janet `$(( 1 / z ))` with z=0 has a decided, documented disposition"
  - "DD#25's janet paragraph is updated to state it"

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

## Deferred to post-beta (DD#104, 2026-09-30)

The in-tree janet backend is removed; `--emit-ir` replaces it with an
inert IR capture that an out-of-tree oracle can consume. This gap is
**not fixed** -- it is recorded here as part of the diagnosis a restart
should read. The code it describes lives at tag `janet-babel`.

This file was still marked live (`open`/`pending`/`active`) long after the
backend it targets was deleted; the other 28 janet tasks were deferred at the
time and these four were missed. Deferred 2026-09-30 for consistency -- no
assessment of the underlying gap has changed.
