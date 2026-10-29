---
title: "Question: should `END { }` admit function definitions?"
status: open
priority: P4
kind: question
depends: []
gates:
  - "DG rules (a) keep or (b) reject; the ruling is recorded in DD#56"
  - "under (b), SB5071 rejects a `fn` in END with advice to define it at the top level, and 56.94-56.100 are rewritten to match"
---

SB5071 admits a `fn` definition only as a direct statement of the top level
or of `END { }`. Since batch-531 F3 (journal 541) an END definition behaves
exactly like a top-level one (DD#56 addendum 2026-10-29, specs
56.94-56.100). The port plan listed one simplification for DG:

- **(a) keep** — today's surface.
- **(b) reject** — SB5071 admits the top level only. An END-only helper
  moves to the top level, and END can still call it: END runs after the
  top level, so the function is defined before END uses it (DD#81 Part A).

What (b) removes:
- the two-list scan `ir::Program::function_defs` (it collapses to
  `statements`);
- the SB5041 case where a top-level use precedes an END definition (56.98):

  ```
  echo $(( (f) + 1 ))
  END { fn f -> int { return 7; } }
  ```
- the END-function half of B5 (the type checker scans the top level only).

What (b) costs: a narrower language surface. Programs that define helpers
inside END are rejected. The affected specs and contrib scripts are not
counted yet; count them when ruling.
