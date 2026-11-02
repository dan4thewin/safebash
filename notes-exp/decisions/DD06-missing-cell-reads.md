---
title: "DD#6 - Missing-cell reads return typed zero"
status: accepted
---

DG selected typed-zero reads for the independent C++ target on 2026-11-01.
The alternative presented was an operation failure on an absent cell.

## Rule

An ordinary read of a missing cell, addressed by an otherwise valid key,
returns the zero value of its element type:

| Element type | Missing-cell value |
|---|---|
| String | `""` |
| Integer | `0` |

The rule applies uniformly to indexed arrays and maps.
Reads do not create entries. A cell explicitly storing an empty string or
integer zero remains present; its ordinary value can match a missing
cell's value without collapsing their distinct container membership.

Use non-inserting lookup, not `std::map::operator[]`, for reads.
This decision chooses neither the accessor's C++ return representation nor
a new source presence operator. The container still preserves membership.

## Operators and failures

A cell keeps the applicable semantics of an ordinary scalar variable,
including its dispositions. In particular, `:-` tests the subject's typed
value (`""` for strings, `0` for integers), not whether a key exists.
Fallback evaluation remains lazy. Required-value and elision operators
follow their scalar rules rather than acquiring a separate cell policy.
An empty string remains an empty argument unless explicitly elided.

Absence alone is not an operation failure. This does not suppress failed
key evaluation, allocation or conversion errors, or an invalid index:
[DD#7](DD07-collection-indices-length-traversal.md) requires nonnegative
array indices. Existing failure checks remain active.

## Basis and limits

This retains the native zero-value read and cell-is-a-variable principles
in [DD#79](../../notes/decisions/DD79-arith-subscripts-model-b.md) and
[DD#123](../../notes/decisions/DD123-a-cell-is-a-variable.md).
Strict lookup would expose incorrect keys sooner, but would also make
ordinary sparse reads fallible and deliberately diverge from those rules.
DG chose typed zero.

[DD#7](DD07-collection-indices-length-traversal.md) settles the remaining
collection choices: reject negative indices, count present cells, and
traverse live in the already-selected sorted-key order.
This records policy, not implementation of collection helpers or an IR
consumer. See [journal 012](../journal/012-collection-missing-cell-policy.md).
