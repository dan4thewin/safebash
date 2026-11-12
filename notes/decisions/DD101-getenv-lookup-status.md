title: DD#101 — `$(getenv NAME)` reports a lookup status
status: implemented
depends: [DD53, DD88, DD24]

1. `$(getenv NAME)` returns the environment value and a lookup status:
   a present name succeeds even when empty; an absent name fails and
   returns an empty string. It takes one name, without a default operand.
2. Explicitly consume the lookup status through a legal capture-condition
   or discard it with `$(- getenv NAME)`. An unhandled lookup is a compile
   error; consuming its string alone does not handle its status.
3. Status handling and taint are independent, including under `:!` or taint
   bypass switches. Both outcomes remain tainted acquisitions; handling or
   discard does not validate them.
4. Preserve ordinary consumer, adjacency and body/operand boundaries.
   A discarded status cannot also be consumed; acquisition-failure fallback
   is not failed-attest resolution (DD55/DD88/DD121/DD127/DD128).

---

Lookup status settled with DG 2026-09-12. DG revised the handling obligation
on 2026-11-12: **"require explicit handling or discard"** (journal 629).
Enforcement of that revision is **pending** in
[`require-getenv-status-handling`](../tasks/require-getenv-status-handling.md).

`$(getenv NAME)` gains a **lookup status**. It does not gain a default
argument. Absence is not a fatal fault: a handled or explicitly discarded
lookup may continue. An unhandled lookup must be refused at compile time,
not silently accepted or turned into an implicit runtime assertion.

| environment | status | value |
|---|---|---|
| `NAME` set to `/home/dg` | success (0) | `/home/dg` |
| `NAME` set to `""` | **success (0)** | `""` |
| `NAME` unset | **failure (nonzero)** | `""` |

Present-but-empty is a **successful lookup**. That is the POSIX `getenv`
semantic — the variable exists — and it is the whole point of the feature:
without it, unset and set-to-empty stay indistinguishable, which is the one
thing no amount of validation can recover.

## The shape

    home=$(getenv HOME) || home=/home/dg

and, via DD#88's capture-condition:

    if ! home=$(getenv HOME) { … }

For intentional value-only use, explicitly discard presence:

    home=$(- getenv HOME)

This still binds the string, including `""` on absence. It discards only
the lookup status, not the value or its taint.

<del>

**Uncaught is allowed.** A bare `home=$(getenv NOPE)` compiles, binds `""`,
and sets a failure status that nothing reads — exactly like an uncaught
external command.

</del>

That original lookup permission remains in the compiler until enforcement
lands. Under the revised rule it is a compile error, even if a later attest
would reject empty text. Ordinary operational failures retain DD55/DD130's
separate abort-if-unchecked contract.

### Consumers and boundaries

Use the existing untyped capture-condition consumers: `if`/`elif`,
`while`/`until`, outer negation and logical-list decisions. Existing RHS
shape and declaration restrictions remain. A typed declaration or a capture
inside an argument/interpolation does not consume lookup status merely by
using the string; use explicit discard when no legal status consumer applies.
No new bare-command, redirect or anonymous-body result form is introduced.

Handling an enclosing command, test, capture or function does not handle
a `getenv` operand or a lookup inside its body. A later `$?` read is not a
consumer of an otherwise-unhandled lookup. Source `-` must be adjacent to
the lookup: `$(- getenv NAME)`, not `- home=$(getenv NAME)`. Conversely,
`if home=$(- getenv NAME)` cannot consume the status that was discarded.
Existing first-expression status-read and negation rules remain.

## Rationale

DG: *"the intrinsic isn't failing, but the lookup of a particular env var
fails."*

That sentence motivated adding lookup status. SB4025 originally refused to
let a capture drive a condition when the RHS is "a sash intrinsic/builtin
(**which can't fail**)".
The premise is true of `$(len …)` or `$(join …)`, whose result is a pure
function of their arguments. It is false of `getenv`, which interrogates
external state that may or may not contain the name. A not-found answer is a
**result**, not a malfunction — the intrinsic did its job and reported what it
found.

So `getenv` moves out of the can't-fail set. No new mechanism is required:
DD#88 already built capture-conditions and `||` composition for external
commands, and this lets one intrinsic in.

## Why not abort

`$(arg N)` out-of-range aborts (DD#60), so abort was the obvious symmetry to
consider. ~~It is **incompatible with the chosen shape**: `||` branches on an
exit status, and an abort tears the program down before any fallback can run.
Choosing `||` rules out abort by construction.~~

That argument rules out only an **unconditional** abort. A checked operation
can abort when unhandled and branch when handled, as external commands do.
The November 12 decision instead chooses a compile-time handling/discard
obligation for lookup status. It does not reclassify absence as a fatal fault.

An earlier revision of `getenv-presence-handling` claimed DD#42 *forced* the
abort, quoting "There is no silent empty return." **That was a misreading.**
The sentence is scoped to `$(argv N)` out-of-range — it is about array bounds,
and says nothing about environment lookups.

Note also the contrast with DD#60's `$(arg N DEFAULT)`: `arg` took an optional
**default argument**, `getenv` does not. DG chose status-and-`||` instead. An
absent env var is an ordinary, expected condition that the caller may want to
branch on, log, or fall back from — not a bounds violation.

## Presence and value validation are independent

An attest checks acceptable contents, not whether the name existed. A valid
predicate may accept `""` while the application still requires presence;
no value-only predicate can distinguish that from an absent lookup.
Taint resolution, an explicit trusted read and taint-disable switches therefore
cannot discharge the lookup-status obligation. Handling presence likewise
does not attest the value or permit an otherwise-illegal tainted read.

Intentional disregard remains expressible with `$(- getenv NAME)`.
The original convenience/compatibility rationale is retained below, but
does not govern the revised rule:

<del>

The alternative considered was making an uncaught `$(getenv X)` a compile
error. Rejected, because **handling is already required — by taint.**

`getenv` results are tainted (DD#24, DD#53), so the value cannot reach any use
without an `attest`:

    str home=$(getenv NOPE)
    echo "[$home]"
    → SB5020 tainted variable $home used without attest

and when the attest predicate rejects the empty string, absence already aborts
at runtime with no getenv-specific rule in play:

    str home=$(getenv NOPE)
    attest home [[ -d $home ]]
    → attest: home failed the gate    (rc=1)

Mandating `||` would stack a second gate behind the one taint already forces,
break every existing `x=$(getenv HOME)`, ~~and be unique in the language —
nothing else requires its status be consumed~~.
DD55 requires non-void function results to be consumed or explicitly
discarded. DD88's named-predicate capture extension does not inherit
`getenv`'s permission to leave lookup absence unhandled.

**The residual hole, stated honestly.** An attest predicate that *admits* the
empty string lets absence through silently:

    attest home [[ $home != /root ]]
    → got:[]    (rc=0)

This is a property of weak attest predicates in general, not a `getenv`
defect: a gate that does not test what you care about does not check what you
care about. The `||` form is what lets a script that *does* care express it.

</del>

The examples inside that superseded rationale are historical observations,
not current acceptance requirements. Explicit discard can keep a value-only
example focused on taint; it must not be inserted silently by the compiler.

## Consequences

- ~~**Not a breaking change.** Every program that compiles today still compiles
  and behaves identically; the new status is simply unread.~~ That described
  the original status addition. Mandatory handling is a source-contract
  change: formerly unhandled lookups need a real consumer or explicit discard.
- SB4025's capture-condition carve-out must admit `getenv` specifically —
  not intrinsics generally. `status` and the pure builtins stay can't-fail.
- Taint is unchanged: `$(getenv …)` remains a tainted source in both the
  success and failure paths. ~~The `||` fallback value follows the ordinary
  DD#78 read-position rules.~~ DD#128 requires legal adjacent resolution;
  replacing the value on acquisition failure does not itself clear taint.
- Distinguishing unset from set-to-empty requires the runtime to return a
  found/not-found flag, not merely a string — `getenv` cannot be implemented
  as "return the pointer, treat NULL as empty" at the boundary.
- Evaluate each lookup once. Do not replace presence with a nonempty-value
  test, rerun it to obtain status, or grant an exception for an expected
  environment value, caller condition or taint bypass.
- No diagnostic code, internal representation or converter migration strategy
  is selected by this recording. The implementation task owns enforcement
  and affected-fixture accounting.

## See also

- DD#88 — capture-condition; supplies the `if x=$(cmd)` / `||` machinery
- DD#53 — env var isolation; `$(getenv NAME)` is the only env read
- DD#24 — taint and legal value reads
- DD#60 — `$(arg N DEFAULT)`, the default-argument shape deliberately *not*
  taken here
- `notes/tasks/bug-getenv-status-unvalidated.md` — ~~`getenv`/`status` have no
  `type_check.rs` entry at all; fix that first, since this work adds a second
  arity to `getenv`~~. This is the completed historical arity/placement repair,
  not a prerequisite or a new arity change.
