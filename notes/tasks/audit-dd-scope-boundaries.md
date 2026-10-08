---
title: Audit DDs for scope boundaries stated out loud but never enforced
status: next
depends: []
gates:
  - every DD grepped for a sentence that grants a form in another position
  - each one checked by probing that position against the live binary
  - findings filed as bugs, not as notes
---

DD#117 fixed the declaration form of an assignment shape bug and wrote, in the
sentence that *granted* the statement form:

> Append remains available as a **statement**, once the name is declared.

Nothing checked the statement form. Sixteen spellings were broken, seven of
them segfaulting, and the fix for the first site had been in the tree for a
fortnight (journal 477, DD#124).

The shape to look for is a DD sentence of the form "X remains available as Y"
/ "X is still legal in Z" / "this applies only to W". Each one is a scope
boundary the author stated deliberately, which means they had the other
position in mind and did not check it. The reasoning that makes the rule right
inside the boundary usually applies just outside it.

Mechanically: grep `notes/decisions/` for `remains`, `still legal`, `only to`,
`does not apply`, `outside`, `as a statement`, `as a declaration`. For each
hit, write the smallest program that uses the form in the granted position and
run it. A green compile is not a pass — check the binary's behaviour, because
both of DD#117's misses compiled clean.

Not a documentation exercise. Anything found is a bug under the standing
policy, and gets fixed rather than recorded.
