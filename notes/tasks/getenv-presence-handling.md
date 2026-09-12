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

### Forced consequence: uncaught absence must abort

Today `h = $(getenv NOPE)` silently yields `""`. Once absence is a *failure*,
DD#55's error model makes an uncaught failure an abort, and DD#42 already
states the principle for the sibling intrinsic:

> Out-of-range `$(argv N)` aborts with a clear error message, consistent
> with integer overflow behavior (DD#26). **There is no silent empty
> return.**

`getenv` returning silent-empty for an absent variable is precisely the
behaviour DD#42 forbids for `argv`. Applying the decision consistently:

- `h = $(getenv NOPE)` — uncaught → **abort** with a diagnostic naming the
  variable.
- `h = $(getenv NOPE) || h = /default` — caught → `h` is `/default`.

This is a **breaking change** for any script that reads an optional env var
without a fallback, and it is the direct consequence of the decision rather
than a separate choice. Flagged rather than deferred; DG to veto if the
silent-empty case should somehow survive.

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
