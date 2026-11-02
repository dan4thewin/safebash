---
title: "DD#7 - Nonnegative array indices, population length, live traversal"
status: accepted
---

DG selected these three policies on 2026-11-01 for the next
collection/argument runtime unit. Together with
[DD#6](DD06-missing-cell-reads.md), they settle that unit's collection
questions, not the broader runtime roadmap.

## Array indices

Indexed-array indices must be nonnegative. Reject negative indices
uniformly for reads, writes, and deletion, including through collection
references. A negative index is invalid, not a missing cell: DD#6's
typed-zero read does not hide it.

There is no from-end interpretation, clamping, wrapping, or negative-key
storage. This restriction applies to integer indices, not associative-map
string keys such as `"-1"`. Invalid-index handling retains the ordinary
failure policy; this decision does not allocate a source diagnostic code.

## Length

Collection length is the number of present cells, for both arrays and
maps. It is not the highest array index plus one. Stored empty strings
and integer zeros count as present.

An array with indices `{2,5}` has length `2`. Reading a missing
nonnegative index does not change it; overwriting a present cell leaves
it unchanged, and deleting a present cell reduces it.
This retains [native DD#75's](../../notes/decisions/DD75-canonical-keyword-forms-uniform-collections.md)
element/entry count and matches `std::map::size()`.

## Live sorted traversal

Direct collection iteration follows the source container's live contents
in the already-selected sorted-key order. It does not snapshot the
collection or the initial set of keys.

Start at the smallest present key. After each visited key, advance to
the smallest currently present key strictly greater than it:

- Inserting a later key makes it eligible for this traversal.
- Inserting an earlier key, or reinserting the current key, does not
  cause a revisit.
- Deleting an unvisited key skips it; changing its value is visible when
  that key is reached.
- Deleting the current cell, clearing the collection, or mutating through
  an aliased reference must not invalidate traversal state.

For example, after visiting `2` in `{2,5}`, inserting `3` and deleting `5`
makes `3` the next key. The same rule applies to map keys and to values
traversed in their keys' order.

This requires mutation-safe traversal, not incrementing an iterator that
the body may have erased. Continually adding later keys can extend a loop
indefinitely; there is no hidden initial-size bound.

Live traversal does not turn explicit copies or snapshots into views.
Already-built command arguments remain ordinary owned arguments, prepared
before launch under the existing process contract.

## Scope

DG chose rejection over signed-key or from-end indexing, population over
extent length, and live iteration over either snapshot alternative.
The AI's snapshot recommendation was not adopted.

The next unit can implement these rules with ordinary C++ containers,
functions, references, and a harmless C++/sash report pair. This decision
adds no runtime implementation, source intrinsic, IR reader, emitter, or
native compiler change. See
[journal 013](../journal/013-collection-unit-policies.md).

Implemented by the bounded collection/argument runtime unit on 2026-11-02;
[journal 014](../journal/014-collection-argument-runtime.md) records its
helpers, paired example, and controls. Native behavior remains unchanged.
