title: DD#17 — "universal call syntax" overstated
status: ready
source: gpt-5.4
severity: accuracy

DD#17 says `$(func args)` is the universal call syntax. But functions
are also called as bare commands (side-effect-only). The note describes
capture invocation as if it's the only form. Clarify that `$(func)` is
for value-returning calls; bare `func args` is for side-effect calls.
