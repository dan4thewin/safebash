---
title: "Evaluate words and expressions left to right"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "DD86's left-to-right rule holds across argument lists and within words and reached operands, for captures and value calls, in both backends"
  - "earlier reads retain their values before a later capture mutates the source; visiting an expression without materializing its value is not enough"
  - "the two `$?` rows below obey DD55, with specs in both modes"
  - "existing lazy evaluation and function/capture/process boundaries are preserved; an unreached operand or pattern never runs"
  - "status and taint analysis do not apply a later capture's effects to an earlier read"
---

## Decision, 2026-11-06

DG chose **left-to-right**, for the principle of least surprise. DD86,
DD55 and journal 580 record the rule. It covers both the order of words
and the component reads/captures inside one word or reached operand;
retaining capture hoisting is no longer an option.

The existing task slug is retained for its links, but this is now an open
implementation bug, not an unanswered language question. No implementation
is included in the decision recording.

## Evidence

Rechecked 2026-11-06 at `51af4312`: the first global-mutation example below
still compiles and prints `new X new` with the ordinary native compiler.
The `$?` examples were not re-probed in this check.

Within a lowered argument list or word, captures run before its other
reads, so source order is not evaluation order. This is batch-531's
"Capture-hoisting order" (`batch-531-findings.md`, section 12).
It was found during F1+F4, exists on base, and is the same on the branch
wherever the branch compiles the program.

Measured at ab133dd2; native and C are identical:

```
str g=old
fn f { g=new; echo X }
echo "$g" "$(f)" "$g"      # current: new X new; required: old X new
```

That the write happens is settled: a capture body writes globals like any
other code (DD#86 addendum 2026-09-28, DG: "changes to globals … survive
unless a fork is required"). The timing is now settled too: evaluating in
text order must print `old X new`.

```
fn t { /bin/true }
/bin/sh -c "exit 3" || echo "[$?]" "$(t)"   # prints "[0] "; DD#55 requires status 3
/bin/sh -c "exit 3" || echo "$(t)" "[$?]"   # prints " [0]" and compiles
```

**The `$?` rows already contradicted DD55 before this ruling.** In the first, `$?` is the
handling branch's first expression, yet it reads the status that `$(t)` left
behind. In the second, `$?` is not the first expression, and DD#55 makes
that SB5012. Under left-to-right evaluation:
- the first row must read the original status 3;
- the second row must be SB5012.

## Implementation boundaries

`[[ && || ]]` operands and DD66 dispatch patterns are already lazy, since
F8 (B87, journal 550) and F9 (B86, journal 551): one that is never reached
never runs. Preserve that while repairing the order inside each reached
operand, just as inside an argument list or interpolated word. Preserve
the once-only dispatch subject value too.

Keep evaluation in its owning body/process, and preserve collection
identity arguments. Use the existing shared lowering rather than a
private capture walker or a backend-specific order. Both backends must
consume the ordered values; a later load from mutable storage would undo
an earlier visitor-order fix.

Capture entry is separately settled: DG chose no caller-status inheritance
(DD55/DD86, 2026-11-07; journal 586). The body starts fresh, and its own
handling flow can establish status. `bug-sb5012-sb5094-nested-body-flow`
owns that pending checker repair; word order cannot authorize an inherited
read. Direct intrinsic value expressions retain their surrounding context.

Attached-redirect timing is settled too: arguments first in both standalone
commands and stages (DD86, 2026-11-07; journal 585).
`q-stage-capture-redirect-order` owns that pending phase-order repair.
Neither task permits captures-first evaluation of the words themselves or
moving stage arguments back to the parent.

## Pointers

- The findings index entry has ev3/ev10 (a pure `echo` capture leaves `$?`
  alone). ev12-ev14's inherited body reads now require refusal, not an
  ordering-dependent captured status.
- The probes are in the session archive `files/port-f1f4-probes.tar.gz`
  (`cb/ev1`-`ev14`), and the name rows are under `files/p531-ledger.md`.
  The ab133dd2 re-probes of the rows above are in `files/port-f5-probes.tar.gz`
  (`q/hn1.sh`, `q/ev5.sh`, `q/ev6.sh`).
- Related: `bug-sb5012-sb5094-nested-body-flow` owns SB5012 for ev12-ev14
  under the no-inheritance ruling.
