title: DD#128 - EXIT, ERR, and adjacent attest resolution
status: accepted

Drafted 2026-11-03 at DG's request. Section 10 records DG's original rulings
and the 2026-11-06 revision to explicit termination, incorporated below.
The handler infrastructure was accepted and implemented 2026-11-05.
Implementation was authorized after those choices. The typing prerequisite shipped
in journal 565; journal 566 restores uniform SB5071; journal 567 implements
process-static globals and their meaningful zero values. Journal 568
implements strict adjacent resolution and fresh private lifetimes, with
definition-convergent, versioned SSA taint checks. Journal 569 implements
generated handlers, their distinct entry/failure contracts, cause-based
runtime dispatch and shared signal protection, replacing END.

On 2026-11-06 DG separated explicit orderly termination from explicit failure:
`exit [N]` selects EXIT for any integer; `fail [N]` selects ERR, including
`fail 0`. Journal 575 records the decision; journal 576 implements it in
`explicit-exit-and-fail`, with independent source/SSA checks and native/C gates.

DD130's later 2026-11-06 ruling distinguishes recoverable failures from
fatal faults and makes fork failure fatal. Sections 7-8 incorporate that
contract; its classification/fork/file-error implementation tasks remain
open. This does not claim that the shipped handler checks already enforce
the new classification in every form.

DG supplied the design principles and decisions. The AI assembled the
writeup and implemented the handler/termination work described above.

## The model

Separate orderly termination from failure cleanup:

- **EXIT is ordinary checked code**, entered on normal fall-through,
  explicit `exit [N]` with any status, or shutdown requested by a configured
  EXIT signal.
- **ERR is failure cleanup**, entered on an uncaught operational failure,
  explicit `fail [N]`, or a first fatal fault before ERR. It makes no
  optimistic assumption that incoming data was attested. Its own operational
  failures must be caught or suppressed, not implicitly abort the handler.
- **Taint is resolved beside acquisition.** A legal adjacent attest, or
  clean replacement of its subject on the failed-attest branch, can clear
  outstanding taint. An unrelated later assignment cannot.
- **Fatal runtime faults remain fatal.** A first fault attempts ERR once;
  a fault inside ERR terminates cleanup. Arithmetic faults, heap
  exhaustion and fork failure are not made recoverable by entering an
  error handler (DD130).

These are rules for reasoning about control flow and trust. They do not
promise that cleanup always finishes, that external commands terminate, or
that a process survives an uncatchable signal.

## Why change END

The former END combined successful completion, early exits, aborts and
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
They share top-level globals, but neither body is a function-definition
root. **SB5071 applies uniformly:** a `fn` is legal only as a direct element
of the program's top-level statement list, never inside EXIT or ERR.
Helpers are defined at program top level. Every whole-program pass must
still visit both handler bodies for their executable statements and calls.

`exit` remains the lowercase orderly-termination builtin; `fail` is the
lowercase failure-termination builtin. `signal N EXIT` keeps its existing
disposition spelling and dispatches to the EXIT body. Handlers remain blocks:
this change does not add source-level handler functions, arguments or returns.

Both handlers belong to the **main process only**. Forked stages and
background jobs do not run them. A child failure can become a parent-side
failure through the existing pipeline or wait/status mechanisms; that is
not the child executing the parent's handler.

Reject the `END` spelling with a migration diagnostic; there is no
compatibility alias. An old END body is not automatically equivalent to
either new body: its routing and error-checking contracts were different.

## 2. Selecting and leaving a handler

Selection is by **termination cause**, not merely by the eventual process
exit byte.

| Event while executing ordinary main-process code | Destination |
|---|---|
| Normal fall-through | EXIT |
| Explicit `exit [N]`, for any integer N | EXIT |
| A checkpoint dispatches a configured EXIT signal | EXIT |
| Explicit `fail [N]`, for any integer N | ERR |
| An uncaught operational failure | ERR |
| A caught or suppressed failure followed by continuation | Neither |
| A first fatal runtime fault | ERR once |

If the selected handler is absent, terminate without substituting the other
handler. An absent ERR does not make an abort run EXIT.

`exit` defaults to 0; `fail` defaults to 1. Each takes at most one argument,
with DD89's existing integer-sink rules. `fail 0` is legal: the operation
selects failure cleanup, while the integer independently selects the reported
status. Neither builtin prints an implicit command-failure message merely
because it was called.

Both operations terminate the current execution path rather than returning a
catchable status. An `if`, `||` or `-` cannot make a reached `exit` or `fail`
resume the interrupted program. Operand evaluation happens before transfer;
an uncaught failure during that evaluation retains its normal ERR behavior.
Existing process-placement restrictions apply to `fail` as they do to `exit`.

DD55 additionally rejects the source spellings `- exit` and `- fail`
(DG, 2026-11-08), with or without a status operand or attached redirect.
Remove the prefix; it is not permission to discard a transfer or an
operand failure. Enforcement is pending in
`reject-control-transfer-suppression`; the routing above is unchanged.

The last predicate's truth value does not turn normal fall-through into an
abort. Likewise, a signal-triggered EXIT normally carries `128 + signum`;
that nonzero status does not select ERR.

DD89 remains in force for both builtins: the argument is a full i64
internally, with no new range restriction or early truncation. `exit 256`
selects EXIT and supplies 256 to it; `fail 256` selects ERR and supplies 256
to it. In either case an unchanged final status is observed as 0 by the
parent. Neither the full integer's zero/nonzero value nor its eventual low
byte selects the handler. Signal and fall-through selection remains
cause-based too.

### No re-entry or return to the interrupted program

The permitted handler progression is:

```text
main -> EXIT -> termination
main -> ERR  -> termination
main -> EXIT -> ERR -> termination
```

An uncaught operational failure, explicit `fail` or first fatal fault in
EXIT enters ERR. EXIT is ordinary checked code, not the old unchecked
END region. Completing EXIT does not run ERR simply because its saved entry
status was nonzero, for example after a signal.

An explicit `exit [N]` inside either handler terminates directly with N,
including nonzero values; it never restarts EXIT or transfers to the other
handler. An explicit `fail [N]` inside ERR also terminates directly with N,
including zero; it does not restart ERR. Defaults remain 0 for `exit` and
1 for `fail`. Deliberate source-level termination remains visible control
flow, distinct from an implicit failure exit.

Neither handler returns to the interrupted code. Neither handler can
re-enter itself, and ERR never transfers back to EXIT. Configured EXIT
signals received during either handler may still update the signal state,
but do not dispatch another handler. IGN, DFL and FLAG retain their meanings.

### Status and values

Save the status that caused handler entry. Falling off the handler preserves
it; successful cleanup commands must not accidentally replace a failure
status with zero. Explicit `exit N` or `fail N` replaces it as described
above. If EXIT fails into ERR, ERR receives the new failure's status.

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
assigned in an `else` block. Replacement must also be a legal write:
DD86 permits validation of a const, not replacement of its value on
failure. That allowance does not relax adjacency, ownership or the
requirement to resolve every continuing path. Replacement must cover the
whole value. A whole-collection unset can supply an empty replacement there. Scalars
require a clean assignment; this does not legalize scalar unset (DD30).
A cell write, cell unset or append does not resolve a tainted collection
or scalar.
Existing refusals of reads or cell writes on tainted data remain in force.

Outside that resolution, a clean assignment changes the runtime value but
does not remove the binding's outstanding taint. This replaces DD121's
general "assignment kill" rule.

Under this rule:

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
The failed-attest replacement must be written directly in that branch.
Calling a reset helper does not count, even if its independent summary
proves a clean whole-value replacement: the helper does not inherit the
branch's permission to resolve its subject.

### What the rule does not mean

It is not a permanent blacklist of names. A clean binding remains clean
after a clean write. Once a valid resolution has cleared its taint, it is
ordinarily usable until another operation taints it.

The binding's lifetime matters. A new function/capture entry initializes its
newly owned locals to meaningful zeros and starts fresh private taint state
(DD86). That is a real value initialization, not a waiver for a still-live
binding. Globals, collection slots and captured enclosing locals retain their
existing values and unresolved state.

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

DD71's binding lists do not make several acquisitions a joint adjacency
unit (DG, 2026-11-09). DD121 measures each plain or declared binding
separately: a later binding intervenes, and a sequence of later attests
cannot resolve the list as a batch. Signal planning must preserve each
legal acquisition/resolution pair rather than extending permission or
deferring dispatch across such a batch. The grammar/adjacency work is
pending in `q-statement-form-residuals`; existing handler ownership,
failed-attest replacement and `:!` contracts are unchanged.

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
`exit N` path, whether N is zero, nonzero or not statically known.

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

The failed branch can resolve v before `exit`, use `fail` to enter ERR,
or leave it unresolved when EXIT does not require it clean. An
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
necessarily enter EXIT. An ERR-bound transfer need not satisfy EXIT's contract.

There is an acknowledged nonlocal dependency: adding an ordinary use in
EXIT can invalidate an earlier explicit exit of any status. A diagnostic
must identify both the unresolved source/transfer and the requiring use in EXIT.

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
failures, explicit `exit`/`fail` or fatal faults.

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

A failure or explicit `exit`/`fail` encountered before deferred signal dispatch
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
attest or literal assignment. Genuinely fresh input acquired inside ERR
may use ordinary adjacent, caught attestation and then ordinary reads.
Its failure branch must comply with section 7, and any replacement that
resolves the subject must be written directly in that branch.

## 7. ERR must handle recoverable operational failures explicitly

An operation whose recoverable failure could implicitly abandon ERR must
be caught or suppressed by a form that actually handles that failure.
Otherwise it is a compile-time error. Do not implement this by marking
the entire handler unchecked.

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

DD130 classifies setup and execution failures by recoverability, not by
their position. A recoverable failure of the command's own attached
redirect belongs to its handling context; a prefix still does not reach
into an operand capture or a helper. Fatal faults cannot be caught or
suppressed, including pipeline construction's failed fork.

Where no real handling form exists for a recoverable failure, the
operation remains unavailable in ERR until that support is implemented.
A missing handling form does not make an ordinary error fatal. Conversely,
the possibility of a genuinely fatal fault is not itself a reason to ban
the operation, as arithmetic and allocation already demonstrate.

Implementation remains coupled to `runtime-failure-classification` and
`recoverable-file-failures`: today's statement-level `-` still misses a
redirect-open failure, and some backup/restore failures have no handling
form. Do not loosen SB5101 merely on the strength of the new terminology;
the runtime path and its checked effect model must agree.

## 8. Fatal runtime faults

The named exceptions to ERR's operational-failure guarantee are:

- DD25 arithmetic faults: overflow, division by zero and negative shift
  counts.
- Exhaustion of the runtime heap.
- Fork failure (DD130; uniform by-form implementation pending).

They remain fatal, including inside ERR. Neither `if` nor `-` promises
recovery from them. Do not ban an operation merely because those fatal
outcomes are possible. Other resource/I/O failure paths follow DD130's
cause-and-state classification; an operation's name or an unavailable
handling form is not sufficient evidence to add a fatal exception.

A first fatal fault in the main process, including inside EXIT, attempts
ERR once if it is declared. Without ERR, terminate directly. Entering ERR
must not itself require another heap allocation. The failed operation is
not retried, and the interrupted program is not resumed.

A fatal fault inside ERR terminates cleanup. Its diagnostic must be
best-effort and must not require another heap allocation; it must not
re-enter ERR or transfer to EXIT. This also applies when ERR was originally
entered for an ordinary failure rather than a fatal fault. No substitute
successful value is invented. This does not promise that diagnostic I/O
succeeds or that the existing runtime shutdown work cannot block.

Arithmetic and OOM paths call `sash_abort`, which attempts ERR before final
runtime shutdown. Startup heap-mapping failure uses that same path, before
main has initialized its locals. A fatal fault in a forked child runs
neither parent handler.

Uncatchable signals and external process termination are outside the
language's cleanup guarantee. Compiler/runtime defects are not a license
to enlarge the ordinary fatal category.

## 9. Implementation and acceptance requirements

This is wider than renaming END or fixing the six W5 findings.

### Storage and terminal dispatch

DG settled the representation on 2026-11-04: **sash globals use actual
process-global storage, and ERR is a generated function.** C emits
file-scope static objects; native code emits writable data symbols.
Function-local bindings and compiler temporaries remain local. The storage
is the backing representation of the existing sash scopes, not a new
language-visible scope or a second copy of the values.

Completed assignments update that storage, and handler reads observe it.
The compiler must not substitute values from a hypothetical normal path
or postpone publishing a completed write past a possible fatal transfer.
The runtime calls the generated handler and terminates; it does not jump
back into the generated main frame or return to the interrupted operation.
The entry path needs no heap allocation. The existing once-only and
main-process-only requirements apply to these calls.

DG explicitly puts optimization below this reasoning model: shell-like
programs making heavy use of external programs are unlikely to be limited
by local memory accesses. Register-only globals are not a reason to retain
nonlocal jumps or complicate handler-visible state.

The storage prerequisite is implemented in journal 567. Both backends use
the same checked global inventory, with a distinct valid empty header for
each global collection and lazy backing allocation. Statement attestation
aliases its source storage rather than creating a future-only copy.
The migrated `h128.gstate.1`/`.2` retain actual completed-value assertions;
the handler-entry taint checks have separate controls.

Journal 568 implements strict resolution in the existing independent
summaries and SSA backstop. The latter converges full definition
classifications, uses actual-current predecessor links and carries reaching
versions on ordinary collection operands. Fresh body-local initialization
closes B14, which otherwise made a subsequent call inherit both stale
values and expired private taint. Neither this prerequisite nor the static
storage work by itself exposed the new handler surface.

Journal 569 lowers EXIT and ERR as independent SSA roots and emits private
functions referenced by optional runtime callbacks. Handler input versions
alias the real global cells; they are not copies of main's final SSA state.
The runtime records terminal progress before cleanup or callback invocation.
Dispatch closes existing readers, restores the invoker's descriptors and
selects a separate capture head without allocating. Actual handler work can
still allocate and fail.

Capture frames are heap-backed linked stacks, separately owned by main,
EXIT and ERR. They have no undocumented fixed 64-frame ceiling. A fault
while appending to a live main/EXIT capture cannot make ERR reuse that
borrowed frame. Capture memfd creation, descriptor duplication and rewind
failures abort explicitly; they never return fabricated empty success.

### Pipeline and acceptance

Keep DD127's one independent summary per function, DD100's non-aliasing
contract, centralized lexical identities and W4's process isolation.
Extend the facts needed for cleanup entry and uncaught-failure effects;
do not reintroduce caller-context body walks. Distinguish runtime value
replacement from permission to resolve outstanding taint.

Both handlers must participate in parsing, binding, resolution, validation,
typing, summaries, lowering, emission and artifacts. The B5 typing
prerequisite closed in journal 565. Both handlers and all capture/function
bodies are checked, and lowering uses the checked type map. Collection
signatures include program-level definitions, calls in every executable
root and shared global facts. No second definition root or blanket
handler-unchecking pass remains.

The source checker derives handler requirements using the existing
independent summaries and incoming-origin ownership obligations. A fresh
acquisition on only one branch does not permit an inherited-input attest
on the other. The SSA backstop independently checks incoming versions and
fresh acquisitions, including mapped collection slots.

`terminal::check_err` reports SB5101 from the live same-process lowered
instructions, including called/inlined bodies, with the call and failure
locations. It does not re-walk a function under caller-specific taint.
Conservative value facts distinguish potentially empty/NUL-bearing values,
dynamic regexes and unknown collection sizes from proven-safe forms.
Collection mutation invalidates the relevant proofs. This currently makes
descriptor captures, bulk/slurp reads and redirects unavailable in ERR:
their uncatchable read or backup/restore failures remain operational
failures even where an enclosing status is caught. Compiler-bounded argv
iteration, known nonempty dispositions, even-length map conversion and
valid static regexes need no fictitious extra failure handling.

Artifact schema 2 has optional `:exit-block` and `:err-block` fields and no
`:end-block`; empty handlers remain distinguishable from absent handlers.

The compiler and SSA backstop must agree on the new taint contract. Handler
entry must preserve actual values on implicit and explicit transfers in
both backends. Signal protection must cover every poll path, not just the
ones represented by an explicit checkpoint instruction.

Acceptance needs focused real-CLI specs and reversion controls for:

- The routing matrix, missing handlers, status preservation and explicit
  overrides, including full-i64 `exit`/`fail`, `fail 0` and signal status.
- EXIT-to-ERR transfer, no handler re-entry and no child execution of either
  handler.
- Ordinary overwrite/unset/rebinding not clearing taint, failed-attest
  replacement resolving only its subject, and helper/slot equivalents.
- Caught-attest exits of every status with and without EXIT clean-use
  requirements, and conservative ERR reads even after normal-path attestation.
- Signals pending between acquisition and resolution, function return,
  reader completion, and the false-reader path; no timing-only evidence.
- Real values after early aborts and reader-producer failure, not merely
  matching diagnostics or matching empty output between backends.
- ERR fallback failures, same-process helpers, argument captures and setup
  failures; no suppression-reach regression.
- First arithmetic and heap faults in main or EXIT attempting ERR once,
  with direct termination when ERR is absent.
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

## 10. DG's rulings

The initial five choices were settled on 2026-11-03. A-D remain in force;
E was revised on 2026-11-06 rather than retained as a conflicting rule:

| Choice | Decision |
|---|---|
| A - END migration | Reject END with migration guidance; no compatibility form. |
| B - First fatal fault | Attempt ERR once. A fatal fault inside ERR terminates without re-entry. |
| C - Failed-attest replacement | Write the replacement directly in the failed-attest branch; a helper does not inherit clearing permission. |
| D - Fresh input in ERR | Allow ordinary adjacent, caught attestation of freshly acquired input, followed by ordinary reads. |
| E - Handler selection | Revised 2026-11-06: explicit `exit` selects EXIT and explicit `fail` selects ERR, independently of status. Signal and fall-through select EXIT. Section 2 is the current rule. |

On 2026-11-04 DG also confirmed: retain SB5071 uniformly for EXIT/ERR,
discard support solely for END-local function definitions, and correct
DD56. This restores DD86's original rule rather than introducing a new
placement restriction.

On 2026-11-06 DG authorized recording and implementing orderly `exit`
and explicit `fail`, then explicitly chose to allow `fail 0`. The
function-shaped EXIT idea was set aside. The implementation is authorized
as an unattended session on a new branch alongside the already-recorded
return-kind work; this does not authorize unrelated backlog or coverage work.

## 11. Companion documents

The affected operative sections are rewritten coherently, rather than
retaining contradictory policies under later exceptions. History remains in
git:

- DD56: END surface, cause-based routing, handler status and checkpoints.
- DD55: checked EXIT and explicit operational-failure handling in ERR.
- DD121: replace general assignment kills with adjacent resolution and
  state the EXIT-entry qualification on non-fall-through paths.
- DD127: summary transfer rules, cleanup inputs and preserved ownership.
- DD24 and DD89: align terminology and references without changing the
  retained source/escape or full-i64 exit rules.
- GUIDE, the manual, converter guidance and AGENTS: generated handlers and
  strict resolution replace obsolete END and general-overwrite descriptions.

DD104 also records artifact schema 2; DD120 puts descriptor restoration at
the common terminal callback entry. The remaining scope, typing, function,
reader, keyword and error-taxonomy references use the same model.
