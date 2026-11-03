title: DD#128 - EXIT, ERR, and adjacent attest resolution
status: draft

Drafted 2026-11-03 at DG's request. **Not accepted or implemented.** The
compiler still has DD56's `END`, and DD121 still permits general clean
assignment kills. This document proposes their replacement; it does not
silently change the authority of the accepted DDs.

DG supplied the design principles. The AI has assembled them into this
proposal. Five concrete draft choices are identified at the end for DG's
review.

## The model

Separate orderly termination from failure cleanup:

- **EXIT is ordinary checked code**, entered on normal fall-through,
  explicit `exit 0`, or shutdown requested by a configured EXIT signal.
- **ERR is failure cleanup**, entered on an uncaught operational failure or
  explicit nonzero exit. It makes no optimistic assumption that incoming
  data was attested. Its own operational failures must be caught or
  suppressed, not implicitly abort the handler.
- **Taint is resolved beside acquisition.** A legal adjacent attest, or
  clean replacement of its subject on the failed-attest branch, can clear
  outstanding taint. An unrelated later assignment cannot.
- **Fatal runtime faults remain fatal.** Arithmetic faults and heap
  exhaustion are not made recoverable by entering an error handler.

These are rules for reasoning about control flow and trust. They do not
promise that cleanup always finishes, that external commands terminate, or
that a process survives an uncatchable signal.

## Why change END

The existing END combines successful completion, early exits, aborts and
signals in one block. Its incoming values can therefore be observed before
the adjacent attest, and its blanket unchecked treatment hides which
failures the author intended to ignore.

Reconstructing every abort-time taint state would make ordinary cleanup
depend on many distant program points. The split gives the two bodies
different contracts instead: EXIT requires the trust its ordinary uses
need; ERR explicitly accepts potentially unresolved input.

DG also tightened the acquisition rule:

> a later assignment, even of a literal, that isn't the branch off a
> failed attest, should not clear taint.

And the failure-handler rule:

> the ERR block should require anything that could abort to instead be
> caught or suppressed.

The latter has a deliberate limit, discussed with DG: there is no general
recovery policy for arithmetic faults or an exhausted heap. The guarantee
is explicit handling of operational failures, not an infallible handler.

## 1. Surface and scope

```sash
EXIT {
    # ordinary, checked cleanup
}

ERR {
    # explicit failure handling
}
```

Each declaration is optional, appears at top level, and occurs at most once.
They share top-level globals and retain the existing END scope rules,
including the existing permission for direct function definitions. This
does not settle the separate question of retiring handler-local function
definitions. Every whole-program pass must visit both bodies.

`exit` remains the lowercase builtin. `signal N EXIT` keeps its existing
disposition spelling, but dispatches to the new EXIT body.

Both handlers belong to the **main process only**. Forked stages and
background jobs do not run them. A child failure can become a parent-side
failure through the existing pipeline or wait/status mechanisms; that is
not the child executing the parent's handler.

**Draft choice A:** replace the `END` spelling, with a migration diagnostic
rather than a compatibility alias. An old END body is not automatically
equivalent to either new body: its routing and error-checking contracts
were different.

## 2. Selecting and leaving a handler

Selection is by **termination cause**, not merely by the eventual process
exit byte.

| Event while executing ordinary main-process code | Destination |
|---|---|
| Normal fall-through | EXIT |
| Explicit `exit 0`, including argumentless `exit` | EXIT |
| A checkpoint dispatches a configured EXIT signal | EXIT |
| Explicit nonzero `exit N` | ERR |
| An uncaught operational failure | ERR |
| A caught or suppressed failure followed by continuation | Neither |
| A fatal runtime fault | Terminal runtime path; draft choice B |

If the selected handler is absent, terminate without substituting the other
handler. An absent ERR does not make an abort run EXIT.

The last predicate's truth value does not turn normal fall-through into an
abort. Likewise, a signal-triggered EXIT normally carries `128 + signum`;
that nonzero status does not select ERR.

DD89 remains in force: an explicit exit argument is a full i64 internally,
with no new range restriction or early truncation of the stored value.
**Draft choice E:** zero/nonzero selection uses that full value. Thus
`exit 256` selects ERR and supplies 256 to it, even though an unchanged
final status is observed as 0 by the parent. DD89 settles value preservation,
not this new selection rule: selecting by the eventual low byte while
still delivering the full value to the handler is a distinct alternative.

### No re-entry or return to the interrupted program

The permitted handler progression is:

```text
main -> EXIT -> termination
main -> ERR  -> termination
main -> EXIT -> ERR -> termination
```

An uncaught operational failure or explicit nonzero exit in EXIT enters
ERR. EXIT is ordinary checked code, not the old unchecked END region.
Completing EXIT does not run ERR simply because its saved entry status was
nonzero, for example after a signal.

An explicit `exit 0` inside EXIT terminates; it does not restart EXIT.
An explicit `exit N` inside ERR terminates with that status, including
zero; it does not enter EXIT. Deliberate source-level termination remains
visible control flow, distinct from an implicit failure exit.

Neither handler returns to the interrupted code. Neither handler can
re-enter itself, and ERR never transfers back to EXIT. Configured EXIT
signals received during either handler may still update the signal state,
but do not dispatch another handler. IGN, DFL and FLAG retain their meanings.

### Status and values

Save the status that caused handler entry. Falling off the handler preserves
it; successful cleanup commands must not accidentally replace a failure
status with zero. Explicit `exit N` overrides it as described above.
If EXIT fails into ERR, ERR receives the new failure's status.

The incoming status is available through `$?` as the first evaluated
expression, under DD55's handling-position rules, for example `int code=$?`.
This does not make `$?` a generally readable latch after arbitrary commands.
Compiler-managed status and signal values are clean.

The handler observes the values established by **actual execution before
the transfer**, not a hypothetical normal-completion SSA version. An
assignment that never completed does not install its result. A parent pipe
reader's completed writes remain visible if producer failure is discovered
later, as DD122 already requires. Native and C must agree on those values.

## 3. Strict adjacent attest resolution

### What may remove taint

Within a live binding, only these operations may discharge outstanding taint:

1. A successful attest at a position permitted by DD121 and DD127.
2. Clean replacement of that attest's subject on its failed branch.

The second permission belongs to **that subject**, not to every binding
assigned in an `else` block. Replacement must cover the whole value. A
whole-variable unset can supply the empty/zero replacement there; a cell
write, cell unset or append does not resolve a tainted collection or scalar.
Existing refusals of reads or cell writes on tainted data remain in force.

Outside that resolution, a clean assignment changes the runtime value but
does not remove the binding's outstanding taint. This replaces DD121's
general "assignment kill" rule.

Under this proposal:

```sash
v=$(getenv INPUT)
v=known
echo "$v"                       # rejected: v remains unresolved
```

But:

```sash
v=$(getenv INPUT)
attest v [[ $v == allowed ]] || v=known
echo "$v"                       # permitted: both outcomes resolve v
```

An acquisition-failure branch is not an attest-failure branch.
`v=$(cmd) || v=known` does not itself discharge taint under this rule.
The compound source can still have its legal adjacent attest, as DD121
already allows.

This applies equally to scalars, whole collections, implicit rebinding,
globals and collection slots, and writes performed by helpers. Ordinary
`unset`, a clean loop binding, a buffer copy or a helper's literal assignment
must not become an alternative spelling for clearing outstanding taint.
**Draft choice C** defines the locality of helper-based failure cleanup.

### What the rule does not mean

It is not a permanent blacklist of names. A clean binding remains clean
after a clean write. Once a valid resolution has cleared its taint, it is
ordinarily usable until another operation taints it.

A genuine new acquisition has its own adjacency opportunity, even if its
target was already tainted. The acquisition itself does not clear anything;
its legal attest can resolve the newly acquired value. A literal overwrite
is not a new taint source and cannot manufacture permission for a late
attest.

The deliberate cost is that "tainted" means **unresolved under the required
protocol**, not necessarily that the current bytes are externally supplied.
Replacing those bytes with a literal is no longer sufficient evidence of
resolution.

### Existing locality and escape rules remain

Adjacency retains DD121's structural meanings: ordinary statement order,
condition-to-body/branch positions, for-source-to-body positions, and the
DD127 single-use-call rule. Shared-function ownership and the prohibition
on attesting received taint inside another body remain.

`:!` authorizes a particular read; it does not clear the source binding.
In particular, `v=${v:!}` is still an unrelated assignment when written
outside the permitted failed-attest resolution, and leaves v tainted.
Reading through `:!` into a different clean binding remains the existing
explicit trust override; this DD does not revoke the escape hatch.

Both R2 and R3 continue to apply. A caught attest must resolve its subject
on every continuing path, even if no later read happens to need it. A
deliberately unattested value is read only through the existing permitted
forms, principally `:!` and its eligible attest predicate.

## 4. EXIT's entry contract

EXIT is checked as ordinary code. Its ordinary uses of incoming data must
be valid on every entry that can reach those uses. A successful attest on
one path cannot justify an ordinary read also reached by an uncleared
`exit 0` path.

```sash
v=$(getenv INPUT)
if attest v [[ $v == allowed ]] {
    :
} else {
    exit 0
}
EXIT {
    echo "$v"                   # rejected: the failure path enters here too
}
```

The failed branch can resolve v before `exit 0`, take a nonzero exit to
ERR, or leave it unresolved when EXIT does not require it clean. An
explicit `:!` use remains available. A literal assignment inside EXIT
cannot repair the unresolved incoming binding, and a first-statement
attest in EXIT is not adjacent to the interrupted source in main.

The requirement includes uses through helpers and mapped collection slots,
not just text in EXIT. It includes other operations requiring a clean
binding, such as a collection-cell write, not just argument reads. Uses of
a fresh value after its own legal acquisition/resolution are not uses of
the incoming value.

This narrows the old non-fall-through exemption **at actual EXIT entries**,
not indiscriminately at every `return`, `break` or `continue`. Those
transfers retain their taint state at their real continuation; they do not
necessarily enter EXIT. An ERR-bound exit need not satisfy EXIT's contract.

There is an acknowledged nonlocal dependency: adding an ordinary use in
EXIT can invalidate an earlier zero exit. A diagnostic must identify both
the unresolved source/transfer and the requiring use in EXIT.

## 5. Signals must not split acquisition from resolution

An acquisition with a legal adjacent attest is protected against
**signal dispatch into EXIT** while its required resolution is incomplete:

```text
check pending signal
acquire
attest; on caught failure, resolve or take the explicit transfer
resume ordinary signal checkpoints
```

Signal receipt still records the pending flag immediately. Only dispatch
into EXIT is deferred. This is not a signal mask and does not defer ordinary
failures, explicit exits or fatal faults.

The protection follows semantic adjacency, including reader conditions,
captures, and a single-use function returning taint to its caller's
adjacent attest. A function-return poll must not split that pair.
Pipeline-reader completion includes producer-status handling; failure
there goes to ERR rather than exposing the unvalidated reader value to
ordinary EXIT.

On a path with no applicable attest, no fictitious resolution is assumed.
For example, a false reader condition can bypass the body's attest. Its
actual taint state still has to satisfy any EXIT entry from that path.
Deliberately unattested data continues to require `:!`.

All checkpoint producers must honor the protection, including polls emitted
directly by either backend. Closing only the explicit SSA checkpoint path
would leave the same hole elsewhere. Reader completion must have its
appropriate post-operation checkpoint.

A failure or explicit exit encountered before deferred signal dispatch
takes its normal transfer. A pending signal does not convert an abort into
EXIT. Resolution can include a compound source or work in a failed-attest
branch, so this policy does **not** promise bounded shutdown latency.

## 6. ERR's incoming data

ERR does not assume that execution reached an adjacent attest, or that a
successful fall-through state describes an earlier abort.

Its entry rule is deliberately conservative: an incoming binding that can
have received tainted input in the main process before ERR requires `:!`
for a value read. A successful main-path attest does not exempt that
incoming binding. This is a potential-source rule, not reconstruction of
every abort edge's validation state.

The potential sources include temporary taint inside called functions,
global and slot writes, and work done in EXIT before it fails into ERR.
Looking only at a function's final outgoing generators is insufficient:
it can acquire, attest and return clean, yet abort before the attest.
Uncalled templates and writes confined to forked children do not themselves
taint main-process bindings. Actual parent-side reads still do.

Never-tainted values and compiler-managed state remain ordinarily usable.
Neither literals nor the saved failure status require escape punctuation
merely because they appear in ERR.

This entry rule is not permission to repair inherited taint with a late
attest or literal assignment. **Draft choice D** retains ordinary adjacent
resolution for genuinely fresh input acquired inside ERR.

## 7. ERR must handle operational failures explicitly

An operation whose failure could implicitly abandon ERR must be caught or
suppressed by a form that actually handles that failure. Otherwise it is a
compile-time error. Do not implement this by marking the entire handler
unchecked.

The requirement applies to every branch, including failure reporting and
fallbacks. In `cmd || fallback`, fallback must itself comply. A deliberate
`exit N` in a handling branch is an explicit choice to stop cleanup, not
an implicit failure exit.

DD55's no-reach rule remains intact:

- Catching or suppressing a same-process function call handles its returned
  result, not an abort inside its body.
- An operand capture or value call is checked where it executes. A prefix
  on the outer command does not catch its failures.
- A called body's possible uncaught operational failure therefore matters
  at an ERR call site, including through further calls.
- A forked child's failure can instead be handled through its process
  status. Requiring that handling does not turn the child into an ERR body.

A helper may remain usable in main while being refused from ERR because
its body has an uncaught operational failure. Report the call and the
internal failure site; do not silently change the helper's behavior based
on who called it.

Setup and operand failures count where they can terminate the handler.
For example, a statement-level `-` does not currently suppress a redirect's
open failure, whereas a condition can catch it. A `-` on every pipeline
stage is not proof that pipeline construction cannot abort.

This DD does not settle the separate global question of which setup
failures `-` should discard. Use a valid existing handling form; where none
exists, the operation is unavailable in ERR unless it belongs to the
explicit fatal category below. A missing handling form does not itself
make an ordinary error fatal.

## 8. Fatal runtime faults

The named exceptions to ERR's operational-failure guarantee are:

- DD25 arithmetic faults: overflow, division by zero and negative shift
  counts.
- Exhaustion of the runtime heap.

They remain fatal, including inside ERR. Neither `if` nor `-` promises
recovery from them. Do not ban arithmetic or allocation merely because
those fatal outcomes are possible.

A fatal fault inside ERR may terminate cleanup. Its diagnostic must be
best-effort and must not require another heap allocation; it must not
re-enter ERR or transfer to EXIT. No substitute successful value is
invented. This does not promise that diagnostic I/O succeeds or that the
existing runtime shutdown work cannot block.

**Draft choice B:** retain direct runtime termination for these faults
before handler entry too. The current arithmetic and OOM paths call
`sash_exit`/`sash_shutdown`, which perform runtime teardown and leave the
process without dispatching the source END body. Trying ERR once after
such a fault would be a separate policy, not implied by non-recoverability.

Uncatchable signals and external process termination are outside the
language's cleanup guarantee. Compiler/runtime defects are not a license
to enlarge the ordinary fatal category.

## 9. Implementation and acceptance requirements

This is wider than renaming END or fixing the six W5 findings.

Keep DD127's one independent summary per function, DD100's non-aliasing
contract, centralized lexical identities and W4's process isolation.
Extend the facts needed for cleanup entry and uncaught-failure effects;
do not reintroduce caller-context body walks. Distinguish runtime value
replacement from permission to resolve outstanding taint.

Both handlers must participate in parsing, binding, resolution, validation,
typing, summaries, lowering, emission and artifacts. The existing B5
type-check traversal gap cannot be retained while claiming either handler
is checked as ordinary code. The current blanket END unchecking must not
be inherited by EXIT or used to disguise ERR's explicit-handling rule.

The compiler and SSA backstop must agree on the new taint contract. Handler
entry must preserve actual values on implicit and explicit transfers in
both backends. Signal protection must cover every poll path, not just the
ones represented by an explicit checkpoint instruction.

Acceptance needs focused real-CLI specs and reversion controls for:

- The routing matrix, missing handlers, status preservation and explicit
  overrides, including full-i64 exits and signal status.
- EXIT-to-ERR transfer, no handler re-entry and no child execution of either
  handler.
- Ordinary overwrite/unset/rebinding not clearing taint, failed-attest
  replacement resolving only its subject, and helper/slot equivalents.
- Caught-attest zero exits with and without EXIT clean-use requirements,
  and conservative ERR reads even after normal-path attestation.
- Signals pending between acquisition and resolution, function return,
  reader completion, and the false-reader path; no timing-only evidence.
- Real values after early aborts and reader-producer failure, not merely
  matching diagnostics or matching empty output between backends.
- ERR fallback failures, same-process helpers, argument captures and setup
  failures; no suppression-reach regression.
- Arithmetic and heap faults inside ERR remaining terminal rather than
  re-entering handlers or being converted to success.

The W5 evidence remains relevant: B96 (entry state), B97 (actual values),
B109 (function-return checkpoint), B110 (reader effects before producer
failure), B111 (reader-completion poll) and e7 (later clean overwrite).
e7 is now settled conservatively: that unrelated overwrite cannot clear
the taint. Policy changes do not by themselves fix B97 or B111.

For new handler syntax, a pre-change parser rejection is not evidence that
the new semantic checks work. Use targeted reversion controls once parsing
exists, along with the ordinary spec gate and its predicted counts.

## 10. Draft choices for DG

These defaults make the proposal concrete; they are not recorded rulings.

| Choice | Draft default | Alternative and tradeoff |
|---|---|---|
| A - END migration | Reject END with migration guidance. | A temporary compatibility form would need its own old/new routing and checking contract; it cannot safely be an unexplained alias. |
| B - First fatal fault | Keep the current direct runtime termination, with no first attempt to run ERR. | Attempt ERR once before termination; potentially useful cleanup, but the handler may immediately encounter the same exhausted resource. A fatal fault already inside ERR never re-enters it either way. |
| C - Failed-attest cleanup through a helper | Clearing permission is lexical to the failed-attest arm and its subject. Calling a generic reset helper does not grant that helper permission to clear received taint. | Admit a call as resolution when its independent summary proves a clean whole-value replacement. This needs a distinct replacement fact, not restoration of general helper assignment kills or caller-specific walks. |
| D - Fresh input in ERR | The conservative `:!` rule governs incoming data. A genuinely new source inside ERR may use ordinary adjacent, caught attestation and then ordinary reads. | Require `:!` for such fresh input too, even after local validation. That is a blanket restriction on ERR reads, rather than a conservative entry contract. |
| E - Zero/nonzero exit selection | Test the full i64 exit value, so `exit 256` selects ERR. | Select by the eventual process-status byte, so it selects EXIT. Either policy can preserve the full value for the handler as DD89 requires; the difference is which notion of success chooses the handler. |

## 11. Documents to replace on acceptance

Do not append contradictory exceptions to the old policies. Rewrite the
affected sections coherently, with history remaining in git:

- DD56: END surface, cause-based routing, handler status and checkpoints.
- DD55: checked EXIT and explicit operational-failure handling in ERR.
- DD121: replace general assignment kills with adjacent resolution and
  state the EXIT-entry qualification on non-fall-through paths.
- DD127: summary transfer rules, cleanup inputs and preserved ownership.
- DD24 and DD89: align terminology and references without changing the
  retained source/escape or full-i64 exit rules.
- GUIDE, the manual, converter guidance and AGENTS: remove obsolete END
  and general-overwrite descriptions when the implementation lands.

The draft alone changes none of those accepted contracts.
