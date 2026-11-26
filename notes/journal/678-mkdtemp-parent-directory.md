title: Optional mkdtemp parent directories
date: 2026-11-26

# Optional mkdtemp parent directories

DG approved an optional directory operand for `mkdtemp`, preserving the
existing direct value/status contract. This implementation is on
`slice-trial`, after the provisional array-slice delivery. Main remains at
`727fe371`; this extension neither lands that branch nor adopts array slices.

## Contract

[DD135](../decisions/DD135-err-readonly-inputs-nul-and-mkdtemp.md) now admits
`$(mkdtemp [DIR])`. Omission selects `/tmp`; a supplied operand is an ordinary
checked string naming an existing parent, not a template. Variables, cells,
interpolation and legal computed values work. Empty input is not omission,
and interior NUL is rejected before pathname work. There is no `TMPDIR`
lookup, fallback directory or implicit parent creation.

The operand runs once before creation and before resolving a relative
parent against the resulting cwd. The returned pathname is absolute.
Concatenation preserves the parent's spelling: lexical collapse of `..`
would change symlink resolution, so neither directory-selector normalization
nor `realpath` is appropriate. No implicit `cpath?`, ownership or containment
certification is granted.

The result remains direct, clean and NUL-free, not captured stdout. A clean
result does not resolve old destination taint, and its input still needs
ordinary taint resolution or a legal forced read. Unhandled creation failure
aborts before assignment; handled/discarded recoverable failure supplies
terminated empty text and failure. Outer handling cannot catch failures in
operand/helper evaluation, and a failed creation does not roll back completed
operand effects. Fatal allocation remains fatal, including in ERR attempts.
Placement, readonly destinations and result-use restrictions are unchanged.

## Implementation

The AI extended canonical arity and string-operand typing. Shared lowering
inserts literal `/tmp` only for omission, retaining the existing typed private
constructor assignment. Both backends require that normalized parent and
pass its pointer/length, the checking flag and length/status outputs to the
same runtime entry point.

The runtime validates counted parent bytes before cwd, entropy or creation.
Absolute parents require no cwd lookup; relative parents use the shared Linux
syscall shim. A nonabsolute kernel cwd such as Linux's `"(unreachable)/..."`
is rejected rather than returned as a supposedly absolute path. Checked
length arithmetic reserves the complete terminated pathname before any
`mkdirat`. A failed attempt frees that reservation.

The existing public-rustix creation machinery remains: complete entropy
reads, 128-bit hexadecimal suffixes, exclusive owner-only creation, and at
most 128 collision attempts. Only collisions retry. Invalid parent bytes
use `EINVAL`; required cwd-resolution failures and returned creation errors
are contained failures under the existing constructor-local DD130 rule.
Heap exhaustion is not converted into a failed empty result.

The initial focused run found a real SSA omission. Ordinary assignment RHSs
propagate taint rather than escape it, while `mkdtemp` intentionally produces
a clean result. The new filesystem input therefore escaped the backstop.
The repair exposes parent operands through the canonical `SsaInst::reads`
inventory used by `taint_reads`, independently of result classification.
There is no private SSA walker or false taint attached to the generated result.

## Evidence

`spec/mkdtemp.sh` grows from 40 to 66 cases per mode. The former one-parent
refusal `mkd.13` retires; fresh `mkd.41` rejects two supplied parents, and
`mkd.42`-`.67` cover the extension. The existing `errmig.41` remains unchanged.
The cases include once-only operand/cwd effects, symlink/`..`, cells with
spaces/newlines, nested values, independent result storage, empty/NUL input,
missing/non-directory parents, cwd failures, root separators, allocation
before creation, reservation reclamation, both taint layers and ERR use.

All 66 pass in native and C. Against the frozen original compiler/runtime,
40 pass and exactly `mkd.42`-`.67` fail in each mode. A private linker wrapper
keeps the old compiler paired with its matching archive; actual archive-open
traces prove selection of this task's frozen old runtime, not the changed ABI.
The two existing native/C malformed-carrier unit matrices also pass with
missing/excess normalized-parent cases.

Nine independent reversions produce these exact red sets:

| Reversion | Failing cases |
| --- | --- |
| Omit SSA parent read | `mkd.57`, both modes |
| Omit string-operand typing | `mkd.60`, both modes |
| Native parent length forced to zero | Native: new `mkd.42`-`.47`, `.50`, `.52`-`.55`, `.58`, `.59`, `.63`, `.65`-`.67`; C stays 66/66 |
| C parent length forced to zero | The same 17 new cases in C; native stays 66/66 |
| Omit relative anchoring | `mkd.43`-`.46`, `.52`, `.53`, `.67`, both modes |
| Omit empty-parent rejection | `mkd.48`, both modes |
| Omit NUL rejection | `mkd.51`, both modes |
| Omit failed-reservation reclamation | `mkd.55`, both modes |
| Omit nonabsolute-cwd rejection | `mkd.67`, both modes |

Each backend-specific zero-length mutation also fails 26 retained cases:
`mkd.1`-`.12`, `.26`-`.38` and `errmig.41`, for 43 failures total in its
affected mode. Every other campaign compares the entire file's red set.
Replayable patches reproduce all eighteen recorded mutant source identities
and expected results. All mutations are restored; final source/spec,
compiler/runtime and both lockfile hashes match the frozen positive.
Restored focused runs again pass 66/66 in each mode.

Two control corrections are explicit. The initial NUL fixture used unsupported
`\0`; the final `\x00` fixture passes positively and fails both the original
compiler and the NUL-guard reversion. The private reversion driver initially
assumed every ID was `mkd.*`, overlooking retained `errmig.41`. It now checks
the frozen positive TSV's exact ordered IDs. Neither faulty control is
counted as compiler evidence.

The predicted ordinary gate passes:
`NEW=54 FLIPS=0 RETIRED=2 TOTAL=13416 timeout 900 mk -p 1 gate`.
The sweep takes 343 seconds. Rows 7,557,377-7,570,792 contain 13,416 distinct
checks: 6,824 native and 6,592 C, with 13,403 passing and 13 existing skips.
There are no failures, flips, ghosts or plan mismatches. All 13,362 retained
baseline statuses are unchanged; the 54 additions and two `mkd.13` retirements
match exactly. Liveness is 246. Rust unit totals are 240/27/20.

Private evidence is in the session's `files/mkdtemp-dir-20261126/`: matched
before/positive binaries, final identities, per-mode TSVs and logs, exact
reversion patches/results, linkage traces, saved weekday xtraces and gate
verification. Standalone controls do not write spec history.

## Closeout

The existing [constructor owner](../tasks/mkdtemp-value-intrinsic.md) records
this extension separately from its original November 16 delivery. DD135's
companion rules, GUIDE, outline and current status agree. No dependency,
converter, tour or coverage change is included; broader file-failure work
and the separate provisional slice evaluation remain independently owned.
