---
title: "getenv: presence handling — capture-condition, or an optional default param"
status: ready
decided: 2026-09-12
priority: P3
kind: task
depends: []
gates:
  - "DG decides how a script tests whether an environment variable is present/absent. Two candidate forms: (a) allow `$(getenv NAME)` as a capture-condition whose status reflects presence (`if x=$(getenv NAME) { use $x }`), or (b) give getenv an optional second argument as a default (`$(getenv NAME DEFAULT)`, absent → DEFAULT), mirroring `$(arg N DEFAULT)`. Pick one (or both)."
---

# getenv presence handling

> **DECIDED (DG, 2026-09-12).** Presence is signalled by the **exit status of
> the lookup**, and there is **no default argument**. See "Decision" at the
> bottom; the two candidate designs above are kept for context only.

DG (2026-09-03), on the DD#88 capture-condition rejecting `$(getenv NAME)`:
"getenv might make sense as a way to handle the requested variable being
absent, or we could handle that with an optional second param to getenv."

Today `$(getenv NAME)` in a condition is SB4025 (DD#88 is external-command
only; getenv is a sash intrinsic). But testing env-var **presence** is a real
need. Two candidate designs:

## (a) getenv as a capture-condition (presence = status)

`if x=$(getenv NAME) { … }` would branch on whether NAME is set: present →
success (capture the value into x), absent → failure. This extends the DD#88
capture-condition to the getenv intrinsic specifically (its exit status becomes
"is it set"). Requires deciding getenv's failure semantics (does an unset var
make `$(getenv)` "fail"? today it is tainted and returns … what? — verify) and
wiring the status into the capture-condition path (currently external-only).

## (b) getenv optional default (`$(getenv NAME DEFAULT)`)

Mirror the DD#60 `$(arg N DEFAULT)` addendum: a second argument is the value
used when NAME is unset. `x=$(getenv HOME /root)` → `$HOME`, or `/root` if
unset. This sidesteps the condition entirely — absence is handled inline. It
does NOT tell you *whether* it was present (only gives a value), so it and (a)
are complementary, not mutually exclusive.

## Recommendation

(b) is the smaller, self-contained change (a new arity for the getenv
intrinsic, like `arg`), and covers the common "default if unset" case. (a) is
the more general "test presence" form but pulls getenv into the
capture-condition machinery. DG to choose one or both.

## Related

- DD#88 (capture-condition — external only; this is the intrinsic extension).
- DD#60 `$(arg N DEFAULT)` — the existing optional-default precedent.
- The taint model: `$(getenv NAME)` is tainted (env is untrusted); a default
  literal is clean — a `$(getenv NAME DEFAULT)` result stays tainted.

## Decision (DG, 2026-09-12)

> "the intrinsic isn't failing, but the lookup of a particular env var
> fails; no default arg; rather `home=$(getenv HOME) || home=/home/dg`"

This rejects the optional-default design outright and adopts status-based
presence. The framing matters: the objection that DD#88/SB4025 assume
intrinsics "can't fail" does not apply, because **the intrinsic is not
failing** — it performs a lookup correctly and reports that the lookup found
nothing. That is a result, not a malfunction.

Target form:

    home = $(getenv HOME) || home = /home/dg

### The machinery already exists

Measured on the release binary — the `||` fallback works today for external
commands and is blocked only for the intrinsic:

| source | result |
|---|---|
| `h = $(/bin/echo hi) \|\| h = /fallback` | `hi` |
| `h = $(/bin/false) \|\| h = /fallback` | `/fallback` |
| `h = $(getenv HOME) \|\| h = /fallback` | **SB4025** |
| `h = $(getenv SASH_NOPE) \|\| h = /fallback` | **SB4025** |

So the work is: give `getenv` a meaningful exit status (0 = present,
nonzero = absent) and let it through the capture-condition / `||` path that
SB4025 currently reserves for external commands.

### RESOLVED 2026-09-12 — see DD#101

DG settled both open points:

- **Uncaught is allowed** (option A: status-only). A bare
  `home=$(getenv NOPE)` compiles, binds `""`, sets a failure status nothing
  reads. Handling is *not* mandatory — taint already forces an `attest`
  before any use, and a second mandatory gate would break every existing
  `x=$(getenv HOME)` for no added safety.
- **Present-but-empty reports success.** `NAME=""` is a successful lookup
  (POSIX `getenv` semantics); only *unset* fails. Without this the
  unset/set-to-empty distinction — the entire point of the feature — stays
  unrecoverable.

No default argument (contrast DD#60's `$(arg N DEFAULT)`); no abort (which
would be incompatible with `||`). Written up as
**`notes/decisions/DD101-getenv-lookup-status.md`**.

### Correction 2026-09-12: abort is NOT forced (and is self-contradictory)

An earlier revision of this file claimed DD#42 forced uncaught absence to
abort. **That was a misreading.** DD#42's sentence —

> Out-of-range `$(argv N)` aborts with a clear error message, consistent
> with integer overflow behavior (DD#26). There is no silent empty return.

— is scoped to **`$(argv N)` out-of-range bounds**, the sentence it follows.
It is a statement about array bounds, not a general principle about intrinsic
results, and it says nothing about `getenv` presence.

Worse, abort-by-default **contradicts the shape DG chose**:

    home=$(getenv HOME) || home=/home/dg

`||` branches on an exit status. An abort tears down the program, so the
fallback could never fire. Picking the `||` form already rules out abort. No
decision needed — the two are incompatible.

### Measured: handling is ALREADY required, via taint

The interesting finding. `getenv` results are tainted (DD#24/#53), so the
value cannot be used at all without an `attest`:

    str home=$(getenv NOPE1)
    echo "[$home]"
    → SB5020 tainted variable $home used without attest

And when the attest predicate rejects the empty string, **absence already
aborts at runtime** — no getenv-specific rule required:

    str home=$(getenv NOPE1)
    attest home [[ -d $home ]]
    → attest: home failed the gate     (rc=1)

So `getenv` already sits much closer to the "handling required" model than to
the external-command model. The enforcement simply lives in the taint system
rather than in `getenv`.

**The residual hole is narrow.** An attest predicate that *admits* empty lets
absence through silently:

    attest home [[ $home != /root ]]
    → got:[]     (rc=0)

That is a general property of weak attest predicates, not a `getenv` defect.

**What no predicate can fix:** unset and set-to-empty are indistinguishable —
measured identical output for `NOPE1` unset and `NOPE1=''`. The information
never reaches the program. This, not safety, is the real argument for a
presence mechanism: taint already covers safety.

### Open sub-questions for implementation

1. **Set-but-empty vs unset.** `NAME=""` — does the lookup succeed (status 0,
   value `""`) or fail? Recommend **succeed**: the variable is present, and
   this matches `getenv(3)` returning a non-NULL pointer to an empty string.
2. **Taint across the join.** `$(getenv HOME)` is tainted; `/home/dg` is a
   clean literal. After `a || b`, `home` must be **tainted** — the
   conservative join. Needs a gate; verify the SSA phi does not clean it.
3. `getenv` arity becomes exactly 1 — see
   `notes/tasks/bug-getenv-status-unvalidated.md`, which must land the arity
   check either way.

### Follow-on

`notes/decisions/DD42-getenv-argv.md` needs an addendum recording
status-based presence. DD#42's getenv half is still current (only the
argv/argc half is superseded by DD#60), so the addendum belongs there.
