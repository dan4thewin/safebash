---
title: "Question: a statement's captures run before its other words (capture-hoisting order)"
status: open
priority: P2
kind: question
depends: []
gates:
  - "DG rules (a) keep hoisting and document it, or (b) evaluate left to right, word by word; the ruling is recorded in DD#55 and DD#86"
  - "under either ruling the two `$?` rows below are fixed (they contradict DD#55 whichever option is chosen), with specs in both modes"
---

Every `$( )` in a statement runs before the statement's other reads, so the
order of the words in the text is not the order they are evaluated in. This
is batch-531's "Capture-hoisting order" (`batch-531-findings.md`, section 12).
It was found during F1+F4, exists on base, and is the same on the branch
wherever the branch compiles the program.

Measured at ab133dd2; native and C are identical:

```
str g=old
fn f { g=new; echo X }
echo "$g" "$(f)" "$g"      # new X new   (bash's subshell: old X old)
```

That the write happens is settled: a capture body writes globals like any
other code (DD#86 addendum 2026-09-28, DG: "changes to globals … survive
unless a fork is required"). What is open is *when* it happens. Evaluating in
text order would print `old X new`.

```
fn t { /bin/true }
/bin/sh -c "exit 3" || echo "[$?]" "$(t)"   # prints "[0] "; DD#55 and bash say [3]
/bin/sh -c "exit 3" || echo "$(t)" "[$?]"   # prints " [0]" and compiles
```

**The `$?` rows are bugs under either option.** In the first, `$?` is the
handling branch's first expression, yet it reads the status that `$(t)` left
behind. In the second, `$?` is not the first expression, and DD#55 makes
that SB5012. The ruling decides only how the fix is built:
- the first row must print `[3]`;
- the second row must be SB5012.

## Options

- **(a) Keep hoisting, and document it.** The name row stays `new X new`.
  `$?` has to be read before the hoisted captures run, and SB5012 has to
  count a capture to the left of `$?` as an earlier expression.
- **(b) Evaluate left to right, word by word.** The name row prints
  `old X new`, and the first `$?` row prints `[3]` with no special case.
  On hand-parser, `[[ && || ]]` operands and DD#66 dispatch patterns are
  evaluated eagerly too: every capture in them runs before the test. The
  branch evaluates them lazily, as F8 (B87) and F9 (B86), which are not
  ported yet. Under (b), argument lists would follow the same left-to-right
  rule.

## Pointers

- The findings index entry has the remaining rows: ev3/ev10 (a pure `echo`
  capture leaves `$?` alone) and ev12-ev14 (a body's own `$?`).
- The probes are in the session archive `files/port-f1f4-probes.tar.gz`
  (`cb/ev1`-`ev14`), and the name rows are under `files/p531-ledger.md`.
  The ab133dd2 re-probes of the rows above are in `files/port-f5-probes.tar.gz`
  (`q/hn1.sh`, `q/ev5.sh`, `q/ev6.sh`).
- Related: `bug-sb5012-sb5094-nested-body-flow` asks whether a body inherits
  `$?` at all. If it does not, ev13 and ev14 become SB5012.
