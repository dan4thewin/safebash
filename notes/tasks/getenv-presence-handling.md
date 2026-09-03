---
title: "getenv: presence handling — capture-condition, or an optional default param"
status: ready
priority: P3
kind: decision
depends: []
gates:
  - "DG decides how a script tests whether an environment variable is present/absent. Two candidate forms: (a) allow `$(getenv NAME)` as a capture-condition whose status reflects presence (`if x=$(getenv NAME) { use $x }`), or (b) give getenv an optional second argument as a default (`$(getenv NAME DEFAULT)`, absent → DEFAULT), mirroring `$(arg N DEFAULT)`. Pick one (or both)."
---

# getenv presence handling

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
