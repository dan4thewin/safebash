title: DD#30 — missing bare numeric assignment amendment
status: resolved
source: claude-opus-4.6
severity: accuracy

DD#30 note omits the amendment that `x=5` without type keyword is a
compile error (ambiguous — use `int x=5` or `x="5"`). This is a
significant behavioral rule absent from the decision note.
