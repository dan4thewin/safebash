---
title: "DD#10 - Counted byte strings and ASCII operations"
status: accepted
---

DG authorized this bounded unit after numeric ingress: byte length/slicing,
ASCII case conversion, and the existing `p?`/`g?` predicates, shared by
scalar strings and string cells. Native DD#12, DD#93, DD#102, DD#123 and
the existing substring/case implementation settle those operations.
No new source policy, normalization framework, or producer change is added.

## Shared value operations

Keep ordinary `std::string` values and counted `std::string_view` operands.
`strlen` checks the byte count against signed-i64 capacity; its operand is
a value, never a variable name. `substring`, `toupper`, and `tolower`
return independent, terminated owned strings, including for partial views.
Interior NUL and high bytes remain data. Allocation failures escape without
publishing a replacement; empty/default views are valid.

Slice offsets and lengths count bytes, not Unicode characters. Positive
offset clamps to the end; negative offset counts from the end and clamps
to zero. Omitted length returns the remainder. Zero length returns empty;
positive length clamps to the remainder; negative length trims that many
bytes from its end, returning empty when the trim covers the remainder.
Both signed-i64 extremes are valid bounds.

The AI chose an optional signed length to distinguish omission from zero,
an unsigned bounded-magnitude helper that safely handles signed minima,
and shared small ASCII mapping/predicate helpers as reversible details.

Case conversion changes only ASCII letters. `is_print` (`p?`) requires
nonempty bytes in `0x20..0x7e`; `is_graph` (`g?`) requires `0x21..0x7e`.
Neither rewrites its operand or throws for ordinary false. These predicates
and length allocate nothing. Do not use locale-sensitive C classification
or case conversion, trim text, normalize Unicode, or add checkpoints inside
these pure helpers.

## Bounded tenth pair

DG clarified the example's application-specific handling: given `COUNT=08`,
slice key/separator/value, lowercase only the key to `count`, and strictly
convert unchanged `08` to integer eight. Do not repair malformed numbers.
The pair retains a real function, source-shaped branches and traversal,
and demonstrates omitted versus explicit-zero slice length and common
operations on string cells.

Top-level `g?` attestation precedes passing argv input into the function.
Only the key is folded; mixed-case hexadecimal value text stays unchanged.
Invalid input remains a failure. DD#9's existing C++ required-value renderer
and status 2 remain unchanged; the reference reports status 1 for rejected
required numeric text. This unit does not silently equate those statuses.

Regex replacement, bulk reads/splitting, remaining formatting, and the
other process/OS families remain separate work. There is still no IR reader,
emitter, native compiler modification, or producer implementation.
