# 639 - Counted NUL and filename guards

DG authorized the full DD135 package on 2026-11-16. The AI is implementing
three coherent units: the predicate/filename prerequisite, readonly ERR
imports with local use-site proofs, and the independent `mkdtemp` constructor.
This entry delivers only the first; it does not reopen DG's accepted policy
or claim the remaining units are implemented.

## Implementation

`0?` follows the existing unary-test AST, parsing and attest shorthand.
Both backends call `sash_has_nul`, which inspects exactly the counted bytes:
empty is false, a leading/interior/final counted NUL is true, and the storage
terminator is excluded. No IR schema change or separate validation walker
is needed. Whole integers retain their normal quoting requirement.

The three shared runtime filename boundaries now reject NUL before lookup.
`sash_file_test` covers every supported unary spelling; `sash_file_newer`
covers `-nt` and reversed `-ot`; `sash_file_same_dev` covers `-ef`.
The binary guards precede both lookups. Mapping a NUL-bearing second operand
to a missing-file result would be wrong: the ordinary `-nt` comparison can
then return true. Existing timestamp, symlink and identity semantics stay.

Ordinary predicates neither attest a value nor globally clear `may_nul`.
The external argv guard is unchanged. Predicate-derived ERR proof is the
next unit and may rely on true filename results only now that the runtime
guarantee holds.

## Evidence

Ten dual-mode cases, `nulcheck.1`-`.10`, extend `filetest.sh` from 47 to 57.
They cover counted-byte boundaries and Unicode, lazy/negated tests, scalar
and array/map attests, single evaluation, every unary filename spelling,
both operands of all binary filename tests, and independent taint/typing.

The NUL-bearing filename cases trace the compiled program and assert that
no file lookup occurs, rather than accepting any false answer. The positive
control observes real `newfstatat` calls, retained operand evaluations,
ordinary missing-file comparisons and hard-link identity. Its syscall lines
are also retained by the harness's xtrace; the temporary raw syscall files
themselves are cleaned by the harness.

Final before-controls use both the original compiler and original runtime
archive from `1aee5543`. Exactly `.1`-`.7`, `.9` and `.10` fail in each mode;
`.8` and all 47 existing cases pass. Early fixture spellings were corrected
to the accepted Unicode escape, explicit nonempty operand and map-element
assignment forms before these final controls. The repaired compiler/runtime
pass all 57 cases in both modes.

Independent reversions, each rebuilt and run in native and C:

| Disabled mechanism | Exact failing new cases per mode |
|---|---|
| Unary filename guard | `.6` |
| Timestamp-comparison guard | `.7` |
| File-identity guard | `.7` |
| Counted-byte predicate | `.1`-`.7` |

Every stub is restored. The compiler, runtime and six production source
files match the saved positive identities; the final spec additionally
retains the positive syscall lines in its trace. Session evidence lives in
`files/dd135-20261116/`, including final before/positive logs, TSVs, reversion
xtraces, artifact identities and `nul-gate.log`.

The predicted ordinary gate passes: `NEW=20 FLIPS=0 RETIRED=0 TOTAL=11407`.
Rows 6947638-6959044 contain 5815 native and 5592 C results:
11394 passing, 13 existing skips, no failures, ghosts or plan mismatches.
All retained statuses are unchanged; the liveness comparison finds 251
historical differences. Rust unit/doc tests and the static gate pass.

DD12/DD87/DD93/DD135, GUIDE and status distinguish this delivery from the
remaining ERR and constructor work. The predicate task closes; the other
two DD135 task records remain open. Coverage stays paused.
