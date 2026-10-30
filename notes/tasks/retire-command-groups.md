---
title: "Retire command groups; keep `cmd || { a; b }` as sugar for `if ! cmd { a; b }`"
status: open
priority: P3
kind: decision
depends: []
gates:
  - "DG rules on the open questions below; DD#55 records the `||` sugar and DD#70 the grammar"
  - "a group anywhere the ruling removes it is rejected with a diagnostic naming the replacement (an `if`, or a function), in both backends"
  - "`cmd || { a; b }` behaves exactly as `if ! cmd { a; b }`, `$?` included, in both backends, with specs"
  - "every spec that uses a group is rewritten or re-pinned, and the five related tasks below are updated or closed"
---

DG, 2026-10-30 (`q-suppression-on-user-functions`, "Rulings, 2026-10-30"):

> a multi-statement group is essentially a user function without a name -
> I don't think I see any reason to support the groups. the foo || { a; b }
> can be allowed as a sugaring of if ! foo { a; b }.

## Today

`parse_group` (parser.rs:1365–1380) accepts a `{ … }` group in five places:

- as a statement: bc.g4–bc.g8 (brace.sh:180–207);
- after `||` or `&&`: 55.51–55.56 (errors.sh:1475–1502) and 18.8
  (functions.sh:85);
- as a pipeline stage (`c-compound-pipeline-stage`);
- under `-`: p518.22/.23 and csfx.17. A `-` never prefixes a compound
  any more, which is `retire-suppression-on-compounds`;
- with a redirect: csfx.17 (`compound-redirects`).

Journal 453 has the history of the brace token.

## Open

1. Is `cmd && { a; b }` sugar for `if cmd { a; b }`, or rejected? DG named
   only `||`.
2. A statement group `{ a; b }` (bc.g5) is rejected. Which diagnostic,
   and does its advice say to inline the statements or to name a function?
3. A group as a stage or with a redirect: is it rejected, with the
   advice to write a function?
4. `compound-redirects`, `c-compound-pipeline-stage`,
   `q-compound-as-condition`, `block-falls-through-blind-to-group` and
   `compound-condition-abort-catch` all assume groups exist. Each is
   updated or closed by the ruling.

The census of specs that use a group is taken when the work starts, from
the parser rather than by grep. A grep for `|| {` / `&& {` also matches
dispatch bodies; it found 88 lines in 13 files on 2026-10-30, 41 of them in
dispatch.sh.
