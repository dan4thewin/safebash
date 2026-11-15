title: Add 0? and reject interior NUL in filename predicates
status: open
priority: P2
kind: bug
depends: []
gates:
  - "0? is true exactly when counted content contains NUL; empty and terminator-only strings are false"
  - "the new unary test follows ordinary condition, negation, attest shorthand and collection-binder rules without implicit taint clearance"
  - "every unary filename test rejects NUL before lookup; -nt/-ot/-ef reject either NUL-bearing operand before lookup or missing-file logic"
  - "operands evaluate once and retain their bytes across the check and lookup; NUL-free filename and timestamp semantics remain unchanged"
  - "native/C CLI specs pin content boundaries and both binary operands; syscall evidence distinguishes no lookup from a coincidentally false lookup"
  - "focused before-controls and independent reversions expose the old truncation and predicate faults; the predicted ordinary gate passes"

## Contract and existing defect

[DD135](../decisions/DD135-err-readonly-inputs-nul-and-mkdtemp.md) adopts `0?`
as **contains NUL**, not NUL-free. The storage terminator is excluded.
`[[ ! 0? $s ]]` is the NUL-free test; `attest s 0?` instead validates the
presence of NUL. Existing taint, predicate-shape and result-use rules apply.

A native control on 2026-11-14 compiled and ran:

```sash
str p=$'/tmp\x00not-a-path'
if [[ -d $p ]] { echo directory } else { echo not-directory }
```

It printed `directory`: the runtime tested `/tmp`, not the counted value.
The ordinary `/tmp` control also printed `directory`. Existing evidence is
in the session's `files/tour-cleanup-directory-proof-20261114.json`; this
recording did not rerun it or measure C.

`sash_file_test`, `sash_file_newer` and `sash_file_same_dev` in
`sash-rt/src/lib.rs` copy counted bytes and construct C strings without
rejecting interior NUL. In `sash_file_newer`, first-present/second-missing
is true. Rejecting NUL only inside a lookup helper as "missing" therefore
fails to repair `-nt`; reject either invalid operand at the predicate boundary.
`-ot` swaps the arguments to the same runtime operation.

## Scope and proof

Wire `0?` through the canonical unary operator, parsing, typing, validation,
IR and native/C paths; reuse shared dispatch rather than a private predicate
list. Preserve DD12/DD87's counted-content/terminator distinction and the
existing definitions of `i?`, `p?` and `g?`. DD132's provisional `r?` remains
separate.

Repair all filename predicates, not only `-d`. `-t` takes a descriptor,
not a pathname. Test leading, interior and final counted NUL, empty strings,
ordinary strings, and the terminator immediately after valid content.
For binary predicates, cover each operand, existing-prefix/missing-tail
values and the ordinary first-present/second-missing comparison.
Keep timestamp resolution, symlink and file-identity rules unchanged.

The dependent [ERR proof task](err-readonly-inputs-and-nul-proofs.md) must not
trust a true filename result until this runtime contract is enforced.
This task does not globally clear `may_nul`, resolve inherited taint, change
the external argv guard or expand filesystem failure handling.
