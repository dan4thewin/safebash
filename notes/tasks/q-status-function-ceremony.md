---
title: "Question: status handling for anonymous capture bodies"
status: open
priority: P3
kind: question
depends: []
gates:
  - "DG rules on how an anonymous capture consumes or exposes a status result; DD55/DD86 record the contract"
  - "the specs the chosen option flips are rewritten, and new specs pin it in both modes"
---

## Return-kind and ceremony questions resolved, 2026-11-06

DG separated bare return from explicit zero and declined all constant-result
checking, including the existing int-only SB4017 rule. The current contract
is in DD55/DD58/DD74; journal 577 implements
`enforce-function-return-kinds` and `retire-return-ceremony-check`.

The body contract applies to anonymous function bodies as well (DD86).
The remaining question is caller-side status consumption, not the result
kind or whether a constant result is legal.

This slug is retained for incoming links. Its former eight-flip estimate
assumed that bare return would be rejected along with explicit zero; that
is not the chosen rule and is not an implementation prediction.

## Remaining question: a status in an anonymous capture body

`$( …; return 3 )`: DD86 makes this a function body, and its explicit
return supplies a status under the settled return-kind rule. How does the
capture consume or expose that status? Named status-function captures
require explicit handling/discard (`$(- status_f)`); the corresponding
anonymous-body handling surface remains to be settled. The original F6
filing observed that the status was ignored and found no dedicated spec;
rec.7/.8 contained `return g …` and asserted SB5037 instead. Re-measure
that behavior and coverage before choosing or implementing the handling
contract.

`dd55-nonvoid-functions` is the historical completion record, not authority
for the retired bare-return/zero conflation.
