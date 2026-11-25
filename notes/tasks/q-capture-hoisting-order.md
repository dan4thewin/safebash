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

The accepted selector prerequisite repairs the shared
within-word `lift_concat` path and source ordering in explicit glob operands
and glob-containing mixed lists (journal 647). It materializes earlier
component values, including string conversion, before later lifted effects.
`globsel.3`, `.31`, `globsugar.8`, `.16` and their independent reversions
pin these routes. The ERR guard migration `errnul.39` and reverse-order
control `.40` distinguish an earlier safe read from a later NUL-bearing one.
This is partial delivery, not completion of general argument-list ordering,
the status rows above, or the independent redirect-timing task.

The collection-key delivery ([journal 670](../journal/670-operand-word-collection-keys.md),
2026-11-25) additionally repairs shared arithmetic lifting. `&&`, `||`
and ternary alternatives lift only on their reached paths; earlier
arithmetic operands are retained before a later lifted call can mutate
their storage. Integer defaults carry their subject type and evaluate that
subject once. Internal `TestValue` branches do not publish a new `$?`.
`keyeval.1`-`.6`, `maparith.10` and independent lazy/retention/default/status
reversions pin these routes in both backends. This does not close general
argument-list ordering, the two status rows above, capture entry checking
or attached-redirect timing.

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

DD55's 2026-11-09 single-write rule adds a downstream publication
requirement for `echo`/`write`, owned by
`bug-echo-partial-line-before-abort`. That task must consume stable,
source-ordered values, not merely collect pointers after captures have
already run. This task's ordering repair does not by itself prove one
output request; the two owners must preserve each other's contracts.

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
