# 568 - Strict resolution and fresh private lifetimes

2026-11-04. DG's DD128 rule is explicit: an unrelated later assignment,
even a literal, cannot clear taint. Only a legal adjacent attest or direct
clean replacement of its subject on the failed path resolves it. Helpers
do not inherit that permission.

I implemented that prerequisite on 45e92ce7, retaining the independent
function summaries and both taint layers. Coupled SSA ordering/timing
repairs and B14's real private initialization belong in the same unit.
The compiler still accepts END. Generated EXIT/ERR handlers, routing,
entry contracts and signal protection remain subsequent work.

## Values and unresolved bindings

The source checker carries a subject-specific failed-attest context.
Ordinary assignments, whole unset, clean splitting and loop rebinding
preserve unresolved taint and its owners. Direct whole-subject replacement
on the failed path clears both. Appends and cell mutations cannot resolve
the whole binding. A genuine acquisition replaces provenance and gets its
own adjacency; a literal or buffer overwrite does not manufacture one.

Named summaries, anonymous bodies and forked children reset replacement
permission. Existing global/slot summaries still apply at calls: a helper
may acquire and resolve fresh input, but an ordinary reset helper cannot
resolve its caller's failed subject. R2 is enforced without requiring a
subsequent read.

This deliberately supersedes B106's requested ordinary clean-split
declassification and the two eager-clean-overwrite examples in the
global-summary follow-up. Runtime replacement remains correct under DD99;
those bindings are now intentionally unresolved. The remaining precision
questions stay filed, without the obsolete repair requirement.

## Definition continuity, convergence and timing

SSA definitions record their actual active predecessor before allocating
a new version. It is not `version - 1`: branch snapshots and discarded
template lowering leave counter gaps. Ordinary writes retain unresolved
predecessor taint. A permitted replacement can remove that dependency,
but the new value is still independently classified. Phi dependencies
remain separate.

The complete definition classifier converges before uses are checked,
within each executable process view. Converging only phis is insufficient:
an assignment that copies a forward phi must be revisited too. There is no
iteration cap. Phi predecessors outside the executable view cannot
contribute, nor can the existing terminally diverging predecessors.

This fixes B94. Its exact nested-branch program still prints `[dirty]`
with the source gate disabled on the frozen original compiler; both
backends now refuse it in the SSA backstop.

Full classification exposed the old bare-name collection lookup in both
directions. Cell reads, splats, table iteration and map-fill sources now
carry reaching `VarRef`s. A future fill cannot taint an earlier clean read;
a future attest cannot clean an earlier read or derived value. Backends
still address the same physical table. `sflow.19` now accepts the unchanged
source formerly pinned as the intermediate rejection `sflow.5`.

Unlowered redirect operands retain the bare-name completed view. Children
still inherit a completed parent view, not a precise fork-time snapshot.
Neither boundary is claimed fixed here.

## A new call is not an old binding

After the first successful full gate, this valid program exposed a coupled
regression:

```sash
fn f {
    str x=clean
    echo "$x"
    x=$(getenv DD128_LOCAL_INPUT)
}
f
f
```

The new predecessor links made the second call inherit the first call's
expired private taint. Merely dropping that link would have concealed B14:
both backends also retained stale private values when a conditional
assignment did not execute on the next call.

One `lower_body` hook now initializes newly owned locals at entry and starts
their fresh taint lifetime. It uses the canonical lexical scope inventory,
not another resolver or a generated-name heuristic. Names are sorted for
stable allocation. Named executable bodies, templates and anonymous
captures share it; self-tail back edges execute the initialization again.

`CheckedTypes` exports the type checker's actual scalar/element types and
array/map kinds, including implicit reader targets. This avoids a second
collection inference walk. Initialization emits real empty string, integer
zero and empty collection assignments. Globals, arguments, slots and
active enclosing locals are preserved. Captures can still modify inherited
bindings without resetting their earlier values or taint.

The extra definitions change diagnostic version numbers. `bindscope.41`
replaces `.38`; `.42` replaces `.40` using a real local named `t_4`, so its
collision is with the first binding's live version, not an unreachable
template. Restoring a joined `name_version` token instead of the base name
still fails exactly these two cases.

## Source evidence

There are 54 fresh dual-mode IDs and 12 retired IDs: 108 additions,
24 retirements, net 84 checks over HEAD. The new groups are
`adjpolicy.1`-`.9`, `adjresolve.1`-`.18`, `adjssa.1`-`.16`,
`flife.1`-`.8`, `sflow.19` and `bindscope.41`/`.42`.

The original compiler fails 19 cases in final `taint.sh`, twelve of the
initial fifteen `adjssa` cases and `sflow.19`, in each mode. The intermediate
fixed-point compiler, before collection versions, fails exactly
`adjssa.11`-`.14`. Those separate controls matter: `.12`-`.14` happened to
pass with the old one-pass classifier.

The strict-resolution compiler before private initialization fails exactly
`flife.1`-`.5`, `.7` and `.8`; `.6` is a preservation control. Final focused
runs pass functions 253, taint 556, taint-switch 78 and collection-parameter
158 checks per mode.

Two early lifetime fixtures were invalid evidence: an unset getenv left
status 1, and shell-level `left-=1` is not the supported arithmetic form.
The final tests supply `DD128_LOCAL_INPUT=dirty` and use `(( left -= 1 ))`.
The discarded results are retained, not counted as compiler failures.

`adjresolve.18` isolates anonymous source permission at the inner read,
asserting located SB5020. The outer direct replacement satisfies R2.
Without it, an outer R2 rejection would mask the broken inner barrier.
Likewise `adjpolicy.9` retains its tainted `continue 2` deposit; moving an
attest just before that continue would erase the claim.

## Independent reversions

Sixteen controls were built and run independently, restoring and
checksum-verifying the source between them. Red sets agree between native
and C; the captured-local control runs two spec files.

| Reversion | Red cases |
|---|---|
| Allow every source replacement | adjpolicy.1-.5; adjresolve.1-.5, .7-.11, .16 |
| Allow no source replacement | adjresolve.12-.15 plus 17 existing cases |
| Clean for-binding removes taint | adjpolicy.6; adjresolve.6 |
| Anonymous source body inherits permission | adjresolve.18 |
| SSA writes ignore predecessors | adjssa.1-.4, .6-.10, .16 |
| SSA replacements never drop predecessors | adjresolve.12-.15 plus 14 existing cases |
| Named SSA body inherits permission | adjssa.6 |
| Anonymous SSA body inherits permission | adjssa.10 |
| Stop full classification after one pass | adjssa.5, .6, .10, .15 |
| Include unreachable phi predecessors | adjpolicy.9 |
| Omit body initialization | flife.1-.5, .7, .8 |
| Keep expired private predecessor taint | flife.1 |
| Reset captured enclosing locals | flife.6; adjssa.16 |
| Restore joined diagnostic tokens | bindscope.41, .42 |
| Reset globals at private entry | flife.6; tmpcap.3, .4, .7, .8, .13, .15, .18 |
| Use counter-minus-one predecessors | adjssa.6, .10 |

For the two no-resolution controls, the authored prediction required
`adjresolve.12`-`.15`, not the complete observed old-case set. The complete
sets are recorded and agree between modes. The global-reset prediction
likewise named the new lifetime witness; seven existing temporary/capture
checks also fail. The first three source controls used the 555-check file
before `.18` was added. Its own barrier control uses the final 556 checks.

All stubs are restored. The normal compiler was byte-identical to the
frozen positive before final comment cleanup. The final code/compiler
hashes are recorded separately. No runtime source changed.

## Gates and persistence

The first accepted sweep, before the lifetime repair and final barrier
witness, was `NEW=84 FLIPS=0 RETIRED=20 TOTAL=10974`, rows
6778683-6789656. The final gate is:

```text
NEW=24 FLIPS=0 RETIRED=4 TOTAL=10994 timeout 900 mk -p 1 gate
```

It passes: native 5608/C 5386, 10981 passing and 13 existing skips,
rows 6789657-6800650, 246 seconds. Unit counts remain 238/27/20;
there are no ghosts, plan mismatches or unexpected flips. Liveness is 260.
All 10970 retained statuses from the first sweep are unchanged. Against
HEAD's block 6767773-6778682, all 10886 retained statuses are unchanged,
with exactly the intended 108 additions and 24 retirements.

Only two obsolete timing comments changed after the final full gate.
The final compiler unit/doc run and ordinary rebuild pass; `gate-code.sha256`
and `final-code.sha256` distinguish the source identities. No coverage run,
new runtime ABI, new Rust test suite or persistent harness script was added.

DD121/DD127 are rewritten in place rather than contradicted by appendices.
DD86/DD128, GUIDE, AGENTS, findings, relevant tasks and status agree.
Six ledger notes change; all 330 rows retain their first fifteen fields,
including status. W5's two pending rows remain pending.

The evidence archive is the session's `port-dd128-adjacent-probes.tar.gz`.
It retains before/after results, actual xtraces, independent control records,
the exact gate row blocks and source identities: 350 regular members,
47037422 uncompressed bytes, every member byte-compared. All 88 trace/TSV
pairs have identical check order; all 34 reversion records match their TSVs
and recorded predictions, with full native/C parity. Archive SHA256:
`f45dc128f74ba74e089b023416dbc5d4a58a796979f8902c61faaf9b6410dfe6`.

Frozen compilers remain separate. The next unit is the complete checked
EXIT/ERR surface and generated terminal callbacks, not a claim that W5 is
already done.
