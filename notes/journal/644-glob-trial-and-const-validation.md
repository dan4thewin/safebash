# 644 - Begin the glob trial; validate const values without writes

DG authorized starting the planned glob features. The integrated owner is
`q-glob-semantics`: DD131's selectors, accumulating fills and immutable
scratch belong with DD132's regex proof and result model. The AI created
`glob-scratch-trial` from clean main
`6c7b899ff9f8e1bfe18a5746efd23d8c549003ea`. Main stays there; trial
implementation does not constitute DG's adoption of the experiment.

## Three remaining source choices

DG accepted all three recommendations together:

1. A leading underscore alone requests scratch construction, including
   `_x=...`, `str _x=...`, readers and loop binders. No extra `const` is
   required. `if _out=$(...)` remains untyped; DD88's typed-declaration
   restriction is unchanged.
2. An initializer cannot read the scratch binding it is constructing,
   directly or through a helper. It receives neither an old instance nor
   the new instance's meaningful zero as its own input.
3. Only bare literal list words activate glob sugar. `sub/*.sb` expands;
   quoted stars (including C-quoted ones) and computed strings remain data.
   Mixed words such as `$dir/*.sb` reject with explicit-glob guidance:
   `glob files "$dir/*.sb"`.

DD131 and the corresponding DD35/DD86/DD98 passages now state those choices.
The draft outline agrees. The owner records their exact sections and
pre-change baseline alongside the earlier revision inventory. These remain
trial rules, not a claim that scratch or selectors are implemented yet.

## Independent const-validation prerequisite

DG had already accepted "separate immutability from validation" on
2026-11-09. The implementation therefore begins with that small independent
prerequisite, not a new const/scratch policy choice.

The old validator grouped a scalar attest retag with real bindings. Removing
that refusal alone would reach `VersionState::define`'s deliberate const-write
panic. The repair separates both boundaries:

- `stmt_written_names` enumerates real writes, including conversion targets,
  but not validation retags. DD100's strict global-access footprint retains
  the attest identity access separately; no alias exception was introduced.
- `define_attested` allocates a validation version without passing through
  the write guard or changing constness. Standalone scalar predicates,
  standalone conversion sources and the shared caught-retag producer use it.
  The existing SSA instructions still alias the unchanged value; no runtime
  copy, storage change or new ABI is required.
- Actual writes continue to use `define`. The direct invariant test checks
  refusal both before and after a successful validation-version allocation.
  The retag is outside the panic catch, so its failure cannot falsely satisfy
  the test.

Existing taint classification already distinguishes attest success from
ordinary writes. The new path grants no adjacency, ownership, failed-path,
loop-placement, collection-const or regex-proof exemption. A conversion's
source keeps its original bytes and string type; its target is a real write.

## Before controls and independent reversions

Seventeen dual-mode `constval` cases replace the obsolete `constw.4`
rejection and add focused acceptance/refusal boundaries. They cover full and
shorthand predicates, standalone and caught/negated/logical forms, conversion
sources, repeated/self-tail function locals, sibling capture locals, runtime
failure, trusted failure reads, real writes and conversion targets, late and
received attests, unresolved fall-through and failed-arm tainted reads.

All seventeen fail on the frozen ordinary before compiler in both modes;
the remaining 470 cases pass. All 487 pass with the repair. The before
artifact was built normally before source edits; neither coverage nor a taint
bypass was used.

| Independent reversion | Exact failing new cases, each mode |
|---|---|
| Put retags back in the validator's write set | `constval.1`-`.17` |
| Use the real-write allocator for standalone scalar attests | `.1`, `.2`, `.7`, `.8`, `.9` |
| Use it for standalone conversion sources | `.3` |
| Use it in the shared caught-retag producer | `.4`, `.5`, `.6`, `.10` |

A fifth reversion makes validation remove the readonly flag. The API invariant
then fails specifically because the subsequent real write succeeds; source
validation would reject such a write before lowering, so this is not a rival
source test. Every reversion was restored. Source, spec, compiler, runtime and
lock identities match the saved positive before and after the full gate.

## Ordinary acceptance

```sh
NEW=34 FLIPS=0 RETIRED=2 TOTAL=11663 timeout 1200 mk -p 1 gate
```

The first sweep passes at rows **7028525-7040187**: **11650 passing, 13
existing skips**, native **5943** / C **5720**. Exactly 34 mode IDs are
first-ever; only `native:constw.4` and `c:constw.4` retire. All **11629**
retained preceding statuses are unchanged. No failures, flips, ghosts or
plan mismatches; liveness is 251. Rust tests pass at **239 / 27 / 20**.
Formatting, Clippy, totality and live touched-function complexity pass.

The const-validation owner is done on this branch. The integrated trial
owner remains open: source-identity scratch lifetimes, atomic batches,
selectors, regex proofs/results and utility/migration/memory evaluation
remain. No independent backlog item or coverage work was resumed.

Session evidence is in `files/glob-trial-20261117/`, with the frozen ordinary
baseline and `const/` holding before/positive/reversion TSVs and logs, the
readonly invariant failure, positive identities and the ordinary gate log.
