title: DD#101 — Scalar input lookup results and literal defaults
status: implemented
depends: [DD53, DD88, DD24]

1. `$(arg N)` and `$(getenv NAME)` return a string plus lookup status:
   present succeeds, including empty; absent fails with empty text.
2. Explicitly consume that status through a legal capture-condition or
   discard it inside `$(- arg N)` / `$(- getenv NAME)`. An unhandled bare
   lookup is a compile error; using its string does not handle presence.
3. `$(arg N DEFAULT)` and `$(getenv NAME DEFAULT)` return only a string:
   the present value or literal DEFAULT on absence, never on present-empty.
   The default handles absence; no status remains to test or discard.
4. All four forms acquire tainted data, including absent/default outcomes.
   Presence handling, discard or a default does not validate contents;
   `:!` and taint bypass do not discharge a bare lookup's status obligation.
5. Evaluate each lookup once. DD65's literal-default and DD60/DD112's
   index rules apply; malformed or negative indices are not missing input
   and a default does not legalize them.
6. Preserve ordinary consumer, adjacency and body/operand boundaries.
   A discarded status cannot also be consumed; acquisition-failure fallback
   is not failed-attest resolution (DD55/DD88/DD121/DD127/DD128).

---

Lookup status settled with DG 2026-09-12. DG revised the handling obligation
on 2026-11-12: **"require explicit handling or discard"** (journal 629).
DG adopted the symmetric four-form contract on 2026-11-13:
**"yes to all 4, update decisions, outline, and backlog"** (journal 631).
The original `getenv` presence result is implemented. Mandatory bare-lookup
handling, `arg`'s presence result and the new `getenv` default form are
**pending** in the expanded
[`require-getenv-status-handling`](../tasks/require-getenv-status-handling.md).

## Shared lookup contract

Both operations look up caller-supplied program input: a zero-indexed program
argument or an environment name. Without DEFAULT they supply **text plus
presence status**. Absence is not a fatal fault: a handled or explicitly
discarded lookup may continue. An unhandled bare lookup must be refused at
compile time, not silently accepted or turned into an implicit runtime
assertion. This applies even when the particular input is expected to exist.

| input | bare lookup status | string |
|---|---|---|
| present, nonempty | success (0) | supplied value |
| present, empty | **success (0)** | `""` |
| absent | **failure (nonzero)** | `""` |

For `arg`, a valid nonnegative index at or beyond `argc` means absence;
malformed/negative indices remain errors, with or without a default.
`argc`, `argv` slices and function-local `args` retain their own contracts.

Present-but-empty is a **successful lookup** in both lookups. Presence cannot
be reconstructed from the text, because absence also supplies `""`.

### Literal-default forms

| lookup | without default | with default |
|---|---|---|
| program argument | `$(arg N)` | `$(arg N DEFAULT)` |
| environment | `$(getenv NAME)` | `$(getenv NAME DEFAULT)` |

With DEFAULT, a present input supplies its value unchanged, including `""`;
only absence selects DEFAULT. An explicitly empty DEFAULT is still a supplied
default and selects the value-only form. The default must satisfy DD65's
existing literal-only rule, not contain a variable read or other expansion.

Supplying DEFAULT **handles absence within the lookup**. The resulting
operation exposes only the selected string: it supplies neither a presence
status nor fresh lookup-status read permission. It does not claim the input
was present or synthesize success for a condition. Consequently,
`if x=$(arg N DEFAULT)` / `if x=$(getenv NAME DEFAULT)` and
`$(- arg N DEFAULT)` / `$(- getenv NAME DEFAULT)` are invalid result uses.
To distinguish supplied input from fallback, use the bare lookup and branch.

All four forms remain tainted acquisitions, including when DEFAULT is chosen.
Neither fallback nor explicit discard attests the value or clears an existing
binding's unresolved taint. Defaults do not handle operand failures, invalid
indices or fatal faults.

## Bare lookup handling

    home=$(getenv HOME) || home=/home/dg

The same capture-condition machinery applies to program arguments:

    if dir=$(arg 1) { … } else { … }

A required argument now uses an explicit failure branch, for example
`dir=$(arg 1) || fail 1`; value validation remains separate.

For intentional value-only use, explicitly discard presence:

    home=$(- getenv HOME)
    dir=$(- arg 1)

These still bind the string, including `""` on absence. They discard only
lookup status, not the value or its taint.

<del>

**Uncaught is allowed.** A bare `home=$(getenv NOPE)` compiles, binds `""`,
and sets a failure status that nothing reads — exactly like an uncaught
external command.

</del>

That original `getenv` permission and DD60's old bare-`arg` abort remain
implementation baselines, not the revised contract. An unhandled bare lookup
is a compile error even if a later attest would reject empty text. Ordinary
operational failures retain DD55/DD130's separate abort-if-unchecked contract.

### Consumers and boundaries

Use the existing untyped capture-condition consumers: `if`/`elif`,
`while`/`until`, outer negation and logical-list decisions. Existing RHS
shape and declaration restrictions remain. A typed declaration or a capture
inside an argument/interpolation does not consume a bare lookup's status merely
by using the string; use explicit discard or a literal default when no legal
status consumer applies.
No new bare-command, redirect or anonymous-body result form is introduced.

Handling an enclosing command, test, capture or function does not handle
an operand lookup or a lookup inside its body. A later `$?` read is not a
consumer of an otherwise-unhandled lookup. Source `-` must be inside the
bare lookup's capture, not on its assignment. Conversely, neither
`if home=$(- getenv NAME)` nor `if dir=$(- arg N)` can consume discarded
status. Existing first-expression status-read and negation rules remain.

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

That was the rationale for `getenv`'s original presence result. The same
distinction applies to `arg`: a valid argument index may have no supplied
value, just as an environment name may be absent. DD88's existing consumers
handle either bare lookup. A literal default is the complementary value-only
choice, not silent disregard of an unanswered presence result.

## Why not abort

~~`$(arg N)` out-of-range aborts (DD#60), so abort was the obvious symmetry to
consider.~~ That was the earlier argument-lookup contract, superseded by the
2026-11-13 shared presence rule. ~~It is **incompatible with the chosen shape**:
`||` branches on an exit status, and an abort tears the program down before any fallback can run.
Choosing `||` rules out abort by construction.~~

That argument rules out only an **unconditional** abort. A checked operation
can abort when unhandled and branch when handled, as external commands do.
The November 12 decision instead chose a compile-time handling/discard
obligation for bare `getenv`; November 13 extends it to bare `arg`.
Neither reclassifies absence as a fatal fault.

An earlier revision of `getenv-presence-handling` claimed DD#42 *forced* the
abort, quoting "There is no silent empty return." **That was a misreading.**
The sentence is scoped to `$(argv N)` out-of-range — it is about array bounds,
and says nothing about environment lookups.

~~Note also the contrast with DD#60's `$(arg N DEFAULT)`: `arg` took an optional
**default argument**, `getenv` does not. DG chose status-and-`||` instead.~~
The November 13 decision adopts both forms for both lookups. Missing optional
program arguments and missing environment names can both be expected; code
may branch on absence, discard presence or supply a literal default.

## Presence and value validation are independent

An attest checks acceptable contents, not whether the name existed. A valid
predicate may accept `""` while the application still requires presence;
no value-only predicate can distinguish that from an absent lookup.
Taint resolution, an explicit trusted read and taint-disable switches therefore
cannot discharge the lookup-status obligation. Handling presence likewise
does not attest the value or permit an otherwise-illegal tainted read.

Intentional disregard remains expressible with `$(- getenv NAME)` and
`$(- arg N)`; defaulted forms instead supply an explicit fallback.
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
  the original status addition. Formerly unhandled `getenv` and implicit-
  assertion `arg` calls need explicit handling, discard or a literal default.
- DD88 admits the two **bare lookup forms**, not intrinsics generally.
  Their defaulted forms remain value-only; classification must distinguish
  the legal forms rather than whitelist either builtin name wholesale.
- Taint is unchanged: all four forms remain tainted sources, including
  absent/default outcomes. ~~The `||` fallback value follows the ordinary
  DD#78 read-position rules.~~ DD#128 requires legal adjacent resolution;
  replacing the value on acquisition failure does not itself clear taint.
- Distinguish presence from text content for both lookups. A defaulted lookup
  resolves that distinction internally; it does not expose a status.
- Evaluate each lookup once. Do not replace presence with a nonempty-value
  test, rerun it to obtain status, or grant an exception for an expected
  environment value, caller condition or taint bypass.
- No diagnostic code, internal representation or converter migration strategy
  is selected by this recording. The implementation task owns enforcement
  and affected-fixture accounting.

## See also

- DD#88 — capture-condition; supplies the `if x=$(cmd)` / `||` machinery
- DD#53 — env var isolation; `getenv` is the explicit env read
- DD#24 — taint and legal value reads
- DD#60 — program arguments and the unchanged `argc`/`argv` forms
- DD#65 — literal-only defaults
- DD#112 — argument-index validation, independent of presence
- `notes/tasks/bug-getenv-status-unvalidated.md` — ~~`getenv`/`status` have no
  `type_check.rs` entry at all; fix that first, since this work adds a second
  arity to `getenv`~~. The historical arity/placement repair remains completed;
  the optional default belongs to the expanded lookup task.
