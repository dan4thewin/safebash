title: Provisional immutable scratch and validated regex results
date: 2026-11-09

## DG's provisional adoption

Following journal 603's selector/scratch package, DG proposed `r?`:
runtime regex values should pass it before use, with the compiler
guaranteeing that validation succeeded first. He suggested immutable
construction and boolean match/substitution results, then a scratch
instance that is never modified but starts afresh when its construction
site is revisited in a loop. A `readonly` operation was discussed as
an alternative; none was selected.

DG requested a unified use-case sketch, then supplied the caught-attest
rule:

> for the final case of a caught attest on an immutable scratch, we could
> allow use of the scratch with match/sub/gsub/... only in the true branch,
> and only allow :! use outside the true branch, but no :! with match et al.

After the consequences were discussed, DG directed:

> let's provisionally adopt this along with the revisions to existing DD -
> in the implementation task let's keep track of which DD were modified
> should any part prove untenable

This is provisional policy/backlog recording, not an implementation go.
The existing requirement stands: implement and evaluate on a **separate
branch**, then bring utility, costs and ramifications back to DG before
landing behavior on main. DG supplied the design directions and adoption;
the AI assembled the use-case model and reconciled the records.

## Two corrections to the preceding explanation

DG asked whether DD131's "retains previous batch" should have applied
only to non-scratch. The recorded text did apply it to scratch too,
but that was the AI's extrapolation from ordinary append-on-success /
unchanged-on-failure, not a separately selected DG scratch rule.
DD131 is corrected in place: ordinary existing destinations retain;
scratch construction begins a new instance and cannot expose an older
batch after failure. The stronger immutability/lifetime model is the
provisional direction now being evaluated.

Reading DD121 during recording also exposed an overstatement in the
AI's earlier discussion of failed attests. R2 already exempts paths
that do not fall through, including `continue` and `return`. A new
permission was not required merely to take one of those transfers.
The new scratch-specific rule permits unresolved **fall-through**
(including no-else) and confines ordinary/regex reads to the successful
region. It is deliberately stricter about post-construct permission:
even a failed arm that exits or continues does not export it.
The correction was reported to DG before completing the record.

## The aligned model

DD131 now covers fresh immutable **scalar and collection** scratch.
One source construction site may execute repeatedly; each execution
creates another instance rather than writing the preceding constant.
Positional visibility, no shadowing and single source introduction
remain. A skipped site does not reset a binding, and loop exit is not
automatic destruction. Meaningful zeros do not automatically carry
regex proof.

The use cases include scalar captures and initialization, `readline`,
for binders, conversion targets, existing array fillers, array/map
construction and snapshots after mutable building. Immutability covers
contents, append/unset and writes through A-E aliases. Read-only helpers
remain possible; identity, one signature per function and strict
non-aliasing are not weakened. Ordinary consts keep their own
declaration/lifetime rules.
The final scalar scratch spelling remains open; this is not general
const-collection syntax or a `readonly` command.

Array publication is whole-batch: an ordinary existing destination is
unchanged on operation error; a scratch attempt remains fresh and empty.
It is not rollback of input consumption or earlier effects, nor a
transaction for all scalar producers. DD88 retains this attempt's
partial stdout on external capture failure. Failed construction and
failed validation are different: a failed attest preserves the new
candidate just tested.

DD132 records `r?`, mandatory regex proof and result handling. The
predicate tests the whole candidate with the actual accepted regex
rules. Invalid data gives quiet false, including a literal being tested;
literal regex **use** retains static validation. Resource faults remain
fatal. Cleanliness, constness and source-text order do not establish
successful validation of a runtime value.

Every runtime regex consumer owes proof for its final immutable
pattern. Composition needs its own check; whole-value quoting does
not change bytes. Attest-all can certify present immutable elements,
not missing-index defaults. Calls, globals and handler roots cannot
fabricate proof. Neither trusted copies nor disabled taint checking
provide an escape.

Caught scratch attests permit ordinary reads only in the attest-success
arm; regex use additionally requires `r?`. Failure and post-construct
code may retain the candidate and read it explicitly as trusted data.
Pattern operands forbid `:!`, even in the successful arm. Subject and
replacement reads retain their ordinary trust rules. Negation swaps
the successful arm; standalone attest allows following uses because
failure cannot continue. The failed instance is not abandoned, changed
or globally cleaned, and signal/handler checks must honor that state.

`sub`/`gsub` supply text **plus** a boolean meaning a match occurred.
No match gives original-subject text and false; identity replacement
can still be true. Existing regex value forms can consume the boolean
through capture-conditions, or explicitly discard it for text-only use.
Substitution sugar projects text per edit; mutation remains resultless
and cannot write scratch. No chain-wide boolean or general assignment
status is invented. Other status-less intrinsic captures still reject
inert prefixes. `rglob` instead keeps an operational status, successful
even with zero matches.

## Ownership and the revision inventory

The AI kept `q-glob-semantics` as the single integrated P2 trial owner,
rather than creating competing implementations of the shared model.
`q-dynamic-regcomp-levers` is closed **as a provisionally answered
question only** and retains its F15 measurements as history. Its
implementation, diagnostic and old-spec migration obligations moved
to the integrated owner. Ordinary const validation remains independently
accepted work and a prerequisite, not silently completed here.

That implementation task contains the exact inventory DG requested for
this recording: **26 modified DD files and new DD132**, with baseline
`2a70e7ddf17d211c20b09ee961e1f3075072986c`, the changed section/contract
and what must be revisited if that part fails. Each trial outcome is
pending. Later implementation edits must extend the inventory and
record evidence, DG's retained/revised/withdrawn disposition and the
reconciliation commit.

The inventory also retains the original selector recording's provenance:
`2a70e7dd` against `bad3ff56`, with its 13 modified DDs and new DD131.
DD60 is the one earlier companion not edited again here; its argv-fill
trial pointer has its own carried-forward row. Closeout must account
for both recordings, not just this revision.

Withdrawal must reconcile text in place, not blindly restore whole
files over independent subsequent decisions. The baseline is provenance,
not authority to restore the AI's scratch-retention extrapolation or
consumer abort as a substitute for DD55/DD70's handling contract.
GUIDE, status, the two owners and the directly coupled failure,
const-validation, status-less-capture and portability tasks agree.
Journal 603 carries a correction/supersession pointer without erasing
the original discussion.

## Implementation boundary

No compiler, runtime, converter, spec or build code changed. No
implementation branch or new behavior probe, sweep or coverage run
accompanies this recording; journal 577 remains the accepted
implementation baseline. The historical F13/F15 tables were not rerun.

The implementation must supply real-CLI native/C evidence, before and
reversion controls, measured spec migration and gate predictions.
No diagnostic number, regex-cache representation, proof-pass architecture,
final scalar declaration grammar or publication mechanism was chosen
by the documentation work. Those decisions must preserve the selected
contract and follow the normal decision-authority rules.
