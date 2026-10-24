---
title: "Question: may `attest` retag a const?"
status: open
priority: P3
kind: question
depends: []
gates:
  - "DG rules whether `attest NAME …` on a `const` NAME is legal"
  - "if legal: the retag sites bind the clean version without tripping SB4044, and a spec pins it in both backends"
  - "either way: a spec pins the ruling, and the DD#86 / DD#24 text states it"
---

## What

```sh
const h=$(getenv HOME)
attest h [[ $h =~ ^/ ]]
echo "$h"
```

This is SB4044 ("cannot reassign const") at `2:1`. Before the batch-531 port
(F13, journal 532) the transform's `define` backstop raised it, spanless at
`1:1`. F13 moved every SB4044 channel into validate's `check_const_writes`,
including the attest retag (`attest_retag_target`), so the behaviour is
unchanged and only the position is new. No DD rules on it either way; the
verdict comes from the backstop's mechanics, not from a decision.

A const used as a convert **target**, `attest s (( n = s ))` with `const n`,
is a real reassignment. That stays SB4044 whatever this question decides.

## Why it is a question

A retag writes a new SSA version of the name carrying the verdict, but the
value is unchanged. DD#86's SB4044 is about reassignment. Under today's rule,
a const initialised from a taint source can never be attested, so it can
only be read with `:!`.

## Options

1. **Legal.** A retag does not reassign. The scalar retag sites
   (`push_attest_retag`, `handle_attest_stmt`, and the convert sources in
   `lower_if_attest_convert` and `push_attest_arith_convert`) define the
   clean version through a const-aware path. `check_const_writes` drops
   the retag channel and keeps the convert target. This is the
   recommendation made during F13; DG deferred it to the backlog under the
   port's no-digression rule.
2. **SB4044, as today.** Document it: a tainted const is read with `:!`.
3. **Reject the source.** A `const` initialised from a taint source is an
   error, so the question never arises.

Found during the F13 port (probe c13).
