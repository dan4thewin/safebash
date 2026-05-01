title: DD#15 — Janet Oracle Defines Semantics
status: implemented

The Janet backend is the reference implementation for what sash programs
should do. Discrepancies between native and Janet output are bugs.

Janet externalizes the compiler's reasoning in a legible, inspectable
form. `(string/slice x 0 (string/find "/" x))` is a readable claim
about what `${x%%/*}` does. If that claim is wrong, you can see it by
reading, even without running.

Two independent implementations agreeing is epistemically stronger than
one implementation matching a reference. If codegen has a bug that
happens to produce correct output for test inputs, the oracle catches
it — because it would need the same bug independently. Janet can't
share Cranelift bugs because it doesn't use Cranelift at all.

When the oracle and bash disagree, the question is not "which is
right?" but "which behavior does safebash want, and why?" Every such
disagreement is a design decision. Janet implements *sash* semantics —
when sash deliberately diverges from bash (e.g., DD#31 arithmetic),
Janet must reflect the sash behavior, not the bash behavior.
