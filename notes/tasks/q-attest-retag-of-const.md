title: "Allow validation of const values without permitting writes"
status: open
priority: P3
kind: work
depends: []
gates:
  - "native and C accept otherwise-legal const attests: full predicates, unary shorthands, standalone and caught forms, and arithmetic-conversion sources; values and types remain unchanged"
  - "success permits ordinary reads; failure retains unresolved taint and follows the existing failure/continuation contract, without falsely cleaning another path"
  - "real writes remain SB4044, including conversion targets and failed-attest replacements; preserve type, redeclaration, loop-placement and const-scalar restrictions"
  - "preserve adjacency, per-binding locality, shared/inherited-taint ownership, explicit :! reads and EXIT/ERR requirements; const grants no exception to any of them"
  - "cover global, function-local and capture-local subjects without losing immutability across retags or merges; preserve the real-write lowering invariant and non-const attest behavior"
  - "drive the real CLI in both modes, prove before/reversion controls, migrate any changed existing contracts at fresh ids and run the gate with measured predictions"

## Accepted policy (DG, 2026-11-09)

> yes, separate immutability from validation

DD86 is authoritative: a const freezes its value, not its validation
state. An otherwise legal taint-source initializer is allowed, and an
eligible attest may validate the unchanged value. A new SSA version
carrying the verdict is not a source-level assignment.

```sash
const home=$(getenv HOME)
attest home -n
echo "$home"
```

Apply the principle uniformly to full predicates and unary shorthands,
standalone and legal caught attests, and a const used as DD93's arithmetic
conversion **source**. Conversion preserves the source's string bytes
and type; its distinct target receives the integer.

Actual writes are still forbidden. In particular, a const conversion
**target** stays SB4044, as does replacing a const subject with a default
on its failed-attest path. No same-name conversion, collection const,
redeclaration, const-in-loop or implicit-clean-input exception follows.
Nor may an implementation temporarily mark a const mutable and let an
unrelated write through.

Existing adjacency, per-binding locality, ownership and failed-path
resolution remain. A late attest or an in-body attest of received taint
does not become legal because its subject is const. A failed gate does
not authorize a clean read or unresolved fall-through at the caught
construct's merge; DD121 already exempts non-fall-through transfers.
Explicit `:!` reads, handler entry requirements and signal deferral
retain their contracts.

The later DD131/DD132 **provisional** model is distinct: immutable
scratch has fresh per-construction instances and a restricted
caught-attest exception. `q-glob-semantics` owns that integrated branch
trial and its DD revision inventory, with this accepted const-validation
work as a prerequisite. Do not silently extend scratch lifetimes or
failed-attest permission to ordinary consts, or treat successful taint
validation alone as regex proof.

The policy question is closed; **implementation remains open** here,
with the existing slug and P3 priority. DD24/DD35/DD93/DD128, GUIDE and
journal 602 agree. DG did not select either retaining the retag refusal
or banning tainted const initializers.

## Implementation boundary

Source inspection at `59489a40` confirms the current mechanisms:

- `check_const_writes` (`sash/src/validate.rs:134-144`) checks the names
  enumerated by `stmt_bound_names`. Its command arm includes
  `attest_retag_target` (183-222), alongside real command-bound names.
- `push_attest_retag` (`sash/src/transform/stmt_handlers.rs:510-522`)
  looks up the old version and calls `state.define` for the validation
  version. The emitted `AttestRetag` aliases the unchanged source value.
- The standalone and caught paths, including conversion sources, must
  agree. Removing only the validator refusal would expose the lowering
  invariant rather than implement the policy.

Separate a value-preserving validation version from a real value write.
Keep the ordinary const-write guard and its invariant, and keep the
conversion target on the real-write path. Reuse shared attest machinery;
do not make different forms disagree or duplicate a walker privately.
No new diagnostic code, source syntax or lowering architecture was selected
by the policy recording.

Before implementation, identify the affected `constw`/taint contracts and
take fresh ids for changed assertions, above the database high-water mark.
Do not treat a generic refusal, an unrelated diagnostic or an ICE as proof
that a const-write or taint boundary still holds. Preserve the existing
mutable-subject controls as well as proving const acceptance and refusal
on the appropriate paths. Predict gate counts from that measured set.

## Historical evidence (not rerun for this recording)

Found during F13 (probe c13, journal 532):

```sash
const h=$(getenv HOME)
attest h [[ $h =~ ^/ ]]
echo "$h"
```

F13 retained SB4044 ("cannot reassign const") but moved it from the
transform backstop's spanless `1:1` to the attest at `2:1`. DG's
no-digression rule deferred reconsideration. DD86's F13 addendum did
explicitly record that retained rejection; the old task's claim that
no DD mentioned it was inaccurate. The 2026-11-09 ruling now supersedes
that part of the rule, not the real-write guard.

No compiler, runtime, converter, spec or build code changed in the
policy recording. No new runtime probe, sweep or coverage run was made.
