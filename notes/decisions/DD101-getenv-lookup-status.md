title: DD#101 — `$(getenv NAME)` reports a lookup status
status: implemented
depends: [DD53, DD88, DD24]

1. `$(getenv NAME)` returns the environment value and a lookup status:
   a present name succeeds even when empty; an absent name fails and
   returns an empty string. It takes one name, without a default operand.
2. Lookup absence does not itself abort or require a handler.
   DD#88's capture conditions and status-handling forms can distinguish
   absent from present-but-empty input.
3. Both outcomes remain tainted acquisitions. DD#121/DD#127/DD#128 govern
   their validation and reads; a fallback assignment on acquisition
   failure is not failed-attest resolution.

---

Settled with DG 2026-09-12.

`$(getenv NAME)` gains a **lookup status**. It does not gain a default
argument, and it does not abort.

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

**Uncaught is allowed.** A bare `home=$(getenv NOPE)` compiles, binds `""`,
and sets a failure status that nothing reads — ~~exactly like an uncaught
external command~~. See "Why handling is not mandatory" below.
Ordinary unhandled operational failures remain checked under DD#55/DD#130.

## Rationale

DG: *"the intrinsic isn't failing, but the lookup of a particular env var
fails."*

That sentence is the decision. SB4025 refuses to let a capture drive a
condition when the RHS is "a sash intrinsic/builtin (**which can't fail**)".
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
consider. It is **incompatible with the chosen shape**: `||` branches on an
exit status, and an abort tears the program down before any fallback can run.
Choosing `||` rules out abort by construction.

An earlier revision of `getenv-presence-handling` claimed DD#42 *forced* the
abort, quoting "There is no silent empty return." **That was a misreading.**
The sentence is scoped to `$(argv N)` out-of-range — it is about array bounds,
and says nothing about environment lookups.

Note also the contrast with DD#60's `$(arg N DEFAULT)`: `arg` took an optional
**default argument**, `getenv` does not. DG chose status-and-`||` instead. An
absent env var is an ordinary, expected condition that the caller may want to
branch on, log, or fall back from — not a bounds violation.

## Why handling is not mandatory

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

## Consequences

- **Not a breaking change.** Every program that compiles today still compiles
  and behaves identically; the new status is simply unread.
- SB4025's capture-condition carve-out must admit `getenv` specifically —
  not intrinsics generally. `status` and the pure builtins stay can't-fail.
- Taint is unchanged: `$(getenv …)` remains a tainted source in both the
  success and failure paths, and ~~the `||` fallback value follows the ordinary
  DD#78 read-position rules~~. DD#128 requires legal adjacent resolution;
  replacing the value on acquisition failure does not itself clear taint.
- Distinguishing unset from set-to-empty requires the runtime to return a
  found/not-found flag, not merely a string — `getenv` cannot be implemented
  as "return the pointer, treat NULL as empty" at the boundary.

## See also

- DD#88 — capture-condition; supplies the `if x=$(cmd)` / `||` machinery
- DD#53 — env var isolation; `$(getenv NAME)` is the only env read
- DD#24 — taint; why the value still needs an attest
- DD#60 — `$(arg N DEFAULT)`, the default-argument shape deliberately *not*
  taken here
- `notes/tasks/bug-getenv-status-unvalidated.md` — `getenv`/`status` have no
  `type_check.rs` entry at all; ~~fix that first, since this work adds a second
  arity to `getenv`~~
