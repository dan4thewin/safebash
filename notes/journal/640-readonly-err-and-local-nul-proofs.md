# 640 - Readonly ERR inputs and local NUL proofs

DG authorized the full DD135 package. The AI has implemented its second
unit: readonly inherited ERR bindings and demand-driven NUL proofs at
external argument uses. Journal 639 delivered the predicate/runtime
prerequisite. The independent `mkdtemp` constructor remains to be done.

## Imports and writes

The bound AST's canonical global identities, scope introductions and
transitive function footprints identify bindings accessed outside ERR.
Write-only accesses and identity-only collection forwarding count. A
declaration in ERR is not exclusive when main or EXIT also accesses it
through a helper. Conversely, `HandlerEntry.inputs` contains every global
and cannot serve as the readonly set.

The existing definition inventory moves to `ssa::instruction_target`.
The separate total `instruction_write` inventories logical storage changes:
assignments, readers/fills/captures, cells, append, match outputs and decoded
deletion. An attest retag is not a write; a conversion's assignment is.
Live same-process ERR instructions reject inherited writes with SB5102,
retaining both the outer call site and actual write location.

This remains structural under `--no-taint`. Main and EXIT stay mutable,
forked jobs affect their process copies, and ERR-only working state and
fresh private locals remain writable. There is no runtime freeze, public
readonly operation, new scope, handler-dispatch change or IR schema change.

## Actual values, not version labels

`terminal/nul.rs` answers only otherwise-unproved ERR argv queries.
Both backends reuse physical storage by logical binding name. Therefore a
real copy is a snapshot, but an older `VarRef` is not. Existing byte facts
remain a fast path only when ERR writes cannot replace their dependencies.
The backward query rewinds writes from the actual instruction position.

Test truth and status-zero truth are separate demands. This matters for
lazy AND/OR, whose merge can branch on a retained result without containing
a test itself. Polarity, negation, explicit status transfers and barriers
are preserved; every feasible predecessor must prove the obligation.
An unchanged loop obligation can be inductive only with its independent
entry paths checked too. Main's predicate history cannot cross ERR entry.

Mutation summaries retain bounds without pretending to identify a selected
cell. Stores and appends include new contents; deletions preserve existing
bounds; regex outputs are substrings of the subject. Map conversion includes
source-key bytes, and insertion includes the new string key. Internal
indexed keys are not assumed to be decimal C strings.

False `0?`, true content-shape predicates and true filename tests supply
witnesses for the same actual value. Runtime argv guards, taint clearance,
path ownership and other operational-failure obligations remain independent.

## Controls and corrections

`signals.sh` grows from 73 to 125 checks: `errro.1`-`.14` and
`errnul.1`-`.38`. Final unit-A before-controls have 29 red checks per mode;
the other new cases preserve existing refusals or acceptance. The final
compiler passes all 125 in native and C.

Two physical-storage fixtures needed refinement. `.38` now uses an integer
helper that overwrites an earlier interpolation's storage; it fails on the
unit-A baseline and independently pins the version-fact guard. `.28` uses
an arithmetic argument rather than an opaque capture, whose own unknown
NUL bound had masked the intended write-replay assertion. Direct NUL
concatenation in `.29` is not a storage-write case.

The first full gate correctly failed: 16 results, eight older fixtures in
both modes. Seven in `terminal.sh` write inherited ERR state, now forbidden
by DD135. Three late-attest checks retain their independent SSA diagnostic
using terminating failed paths; fresh acquisition moves into writable
ERR-only state. These four preserved obligations take `errstate.1`-`.4`.
The overwritten/partially refreshed inherited-input variants are superseded
by readonly enforcement, not recoded as duplicate SB5102 assertions.
The seven old IDs, `migratea.23`-`.28` and `.30`, retire; the file's plan
falls from 81 to 78. All 78 pass on both unit A and the final compiler.

The eighth failure, `exit_arg.sh`'s unchanged `migratea.56`, was a real bug
introduced by the new prover: replacing version facts lost the permanent
NUL-free guarantee of an integer copied from ERR's full status.
The prover now retains the checked scalar integer types, excluding table
storage. It does not reinterpret types or equate different value identities.
All eleven exit-argument cases pass on unit A and the repaired compiler.

Eleven independent reversion mechanisms were built and run in both modes:

| Disabled mechanism | Exact failing cases per mode |
|---|---|
| Readonly refusal | `errro.1`-`.9`, `.12`, `.13` |
| Predicate witnesses | `errnul.1`-`.5`, `.7`, `.8`, `.11`, `.14`, `.16`, `.18`, `.19`, `.22`, `.23`, `.25`, `.27`, `.34` |
| All-predecessor joining | `errnul.9`, `.10`, `.20`, `.24`, `.26` |
| Predicate polarity | `errnul.6`, `.9`, `.10`, `.24`, `.26` |
| Actual write replay, final tree | `errnul.12`, `.13`, `.15`, `.17`, `.20`, `.24`, `.28`, `.35`, `.36`, `.38` |
| Version-fact invalidation | `errnul.38` |
| Result-channel barrier | `errnul.30` |
| Collection mutation bounds | `errnul.32`, `.33`, `.37` |
| Map conversion's source-key bound | `errnul.35` |
| Newly inserted string-key bound | `errnul.36` |
| Checked scalar integer bound | `migratea.56` |

The first four runs preceded `.28`'s fixture repair. The original
write-replay expectation was too broad; reading its xtraces explained the
masked capture, direct concatenation and missing integer-result proof.
The final replay reversion includes `.38` after the integer correction
removes that independent refusal. All stubs are restored and the rebuilt
compiler is byte-identical to the frozen final positive.

## Accepted gate and remaining work

The first draft sweep recorded 104 new mode IDs at rows 6959045-6970555.
The corrected gate therefore predicts `NEW=8 FLIPS=2 RETIRED=14 TOTAL=11505`,
not another 104 new IDs. It passes at rows 6970556-6982060:
5864 native and 5641 C, 11492 passing and 13 existing skips, no failures,
ghosts or plan mismatches. The two flips repair `migratea.56` from the
failed draft sweep. Liveness reports 251 historical differences.

Against the accepted unit-A sweep, all 11393 retained statuses are unchanged:
112 IDs were added and fourteen retired. Rust unit/doc and static checks
pass. Source, compiler, runtime and both local lock identities are verified;
there is no runtime change or coverage run in this unit.

Session evidence is in `files/dd135-20261116/`: the failed `err-gate.log`
is preserved separately from `err-gate-final.log`, alongside before,
positive and reversion TSVs/logs, frozen compilers and `err-final.sha256`.
DD135, GUIDE, status and the readonly/proof task record this delivery.
The full authorized package is not complete until the constructor and tour
migration are delivered.
