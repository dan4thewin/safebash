---
status: open
priority: P3
depends:
gates:
---

# SB5088 claims non-adjacency for attests that are adjacent

Split out of `negated-attest-not-a-caught-form` (2026-10-02), which fixed the
`if ! attest` case by translation. Two positions still report SB5088:

```sash
str x=$(arg 1)
! attest x [[ -n $x ]]          # SB5088
echo "got=$x"
```

```sash
str x=$(arg 1)
while ! attest x [[ -n $x ]] { break }   # SB5088
echo "got=$x"
```

Both are **correctly rejected**. The complaint is only the wording:

> attest of 'x' is not adjacent to the operation that tainted it. Move it to
> immediately after the statement that taints 'x', or read the value with
> `${x:!}` to take it on trust

The attest *is* immediately after the statement that taints `x`. The advice
sends the author to rearrange a program that is already arranged correctly,
or to reach for `${x:!}` — a trust escape — to work around a message that
does not describe the real objection.

## Not a negation defect

`while attest x [[ … ]] { break }` — **unnegated** — reports the same SB5088.
So this is not about `!`; it is about the attest appearing in a position that
is not one of DD#121's three caught forms. The negated spellings above are
just two more instances.

The real objections, which the diagnostic should state:

- **bare `! attest`** — an uncaught attest aborts on failure, so negating its
  status cannot mean anything. (Bare *unnegated* `attest` is legal: it is the
  uncaught form.)
- **`while attest`** — a loop condition is not a caught form. R2 is a
  statement about a *merge*, and a loop's back-edge is not one.

## Shape of the fix

`check_adjacency` reports SB5088 when it finds an attest whose subject is not
in `JustTainted`. It cannot currently tell "you put this in the wrong place"
from "this position is not a caught form at all". Detecting the second case
needs the position, not just the taint state — i.e. the caller has to say
which syntactic slot the attest came from.

Was filed as "worth doing with the `r47-validate-rs-per-rule-tests` work
rather than alone". **r47 was closed 2026-10-14** — it prescribed a Rust suite
under `sash/tests/validate/`, a surface deleted in journal 304 — so this no
longer has a carrier and stands alone. The standing reason survives: the
cold-arm ranking already wants `check_adjacency` broken up, so pair it with
that rather than with a retired task.
