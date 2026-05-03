title: DD#52 — not in SUBSET.md rejected constructs table
status: resolved
source: gpt-5.4
severity: accuracy

DD#52 rejects `${var@...}` transforms (SB4015) but SUBSET.md's
rejected constructs table doesn't list them. The string operations
section lists `${var^^}` etc. but doesn't mention the rejected
`${var@U}` etc. Add to the rejected table.
