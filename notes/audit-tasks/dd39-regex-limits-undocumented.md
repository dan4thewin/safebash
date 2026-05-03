title: DD#39 — regex engine limits not in SUBSET.md
status: resolved
source: gpt-5.4
severity: gap

DD#39 and SUBSET.md present regex support without documenting the
Plan 9 engine limitation: no `{n,m}` counted repetition, silent
no-match on unsupported syntax. Silent failure on unsupported regex
is exactly what safebash should not tolerate. Document the limitation
in SUBSET.md and consider whether unsupported syntax should be a
compile error rather than silent no-match.
