title: Bless failure semantics — DD#47 vs GUIDE.md
status: resolved
source: gpt-5.4
severity: contradiction

Resolved: DD#47 updated to document both forms. Bare bless → fatal
exit on failure. `if bless` → false branch, VAR remains tainted.
Both forms are intentional. DG confirmed.
