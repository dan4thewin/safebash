---
title: "Question: does DD#55's `return 0` ceremony rule cover status functions, and a `return` in a capture body?"
status: open
priority: P3
kind: question
depends: []
gates:
  - "DG rules on both questions; DD#55 records the ruling"
  - "the specs the chosen option flips are rewritten, and new specs pin it in both modes"
---

From `q-suppression-on-user-functions`, "Rulings, 2026-10-30". F6 enforces
B15: a bare call of a status function is an error unless it is caught or
discarded with `-`. It classifies a bare `return` as `return 0`, which is
what the code does (`handle_return_stmt`) and what spec 0.5q pins. So a
function whose returns are all zero is void. Two questions remain.

## 1. The ceremony rule

DD#55 ("Non-void functions"): "`return 0` alone is a compile error — if the
only possible return is zero, the `return` is ceremony. Omit it (void
function)." Today only SB4017 enforces this, and only for `-> int`.

Enforcing it for status functions, with a bare `return` counted as
`return 0`, rejects every early-exit `return` in a void function (w21:
`fn f { if [[ $1 == a ]] { return }; echo "$1" }`, which prints `b` for
`f a; f b`). The census in that task's Q3 counts 8 flips: 27.4, fr.11,
fr.12, cf.u4, tmpcap.19–21, 88.11. w18 (`fn f { echo x; return 0 }`) is
the silent ceremony case today.

- (a) Leave the rule `-> int`-only, and amend DD#55's text to say so.
- (b) Reject an explicit `return 0` in a function with no other status,
  but keep a bare `return` as the early-exit spelling.
- (c) Enforce it fully, bare `return` included (8 flips).

## 2. A `return` in an anonymous capture body

`$( …; return 3 )`: DD#86 (2026-09-29) makes an anonymous capture body a
function body. Does such a capture carry a status that must be handled,
written `$(- …)`, as F6 requires for `$(status_f)`? Today the status is
ignored. No spec has one: rec.7/.8 (functions.sh:817, :822) contain
`return g …` and assert SB5037.

Pointers: `dd55-nonvoid-functions` holds the original design (a bare
`return` defaults to 0; the ceremony rule).
