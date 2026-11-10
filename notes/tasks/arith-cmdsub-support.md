---
title: "should a command substitution be allowed inside $(( ))?"
status: ready
priority: P3
kind: design
depends: []
gates:
  - "a DD (or an explicit no) covering `$(( $(cmd) + 1 ))`"
---

# `$(( $(cmd) ))` — rejected consistently, but never decided

## Where it stands

Bash evaluates a command substitution inside an arithmetic expansion:

```sh
$ bash -c 'echo "$(( $(/usr/bin/printf 5) + 1 ))"'
6
```

sash rejects it — **SB4020**, in every position, as of journal 286. Before
that fix four of the five positions rejected it and the fifth (inside a
double-quoted string) silently evaluated to `0`, so this was never a
decision, just an artefact of where the error happened to be swallowed.

Consistent rejection is strictly better than a silent wrong answer, but it
is still not a *decision*. Nothing in `notes/decisions/` covers the form.

## The question for DG

Three options:

1. **Keep rejecting** (status quo) — and say so in a DD, so the next person
   to find it does not read the rejection as an oversight. The diagnostic
   could then name the reason rather than being a generic syntax error.
2. **Support it.** `$(( $(f) + 1 ))` lowers the capture to an SSA temporary
   and coerces to int, exactly as `x=$(f); (( x + 1 ))` already does. Note
   the taint consequence: an external capture is tainted, so the arithmetic
   would be tainted, and `int n=$(( $(f) + 1 ))` would need an attest — a
   meaningful surface change, not just parser work.
3. **Support only the clean intrinsics** (`$(getpid)`, `$(argc)`, …) and keep
   rejecting external captures. Avoids the taint question entirely but
   introduces a split that the DD#23 model does not otherwise have.

## Relevant

- The sexpr spelling already covers the in-process case: `$(( (getpid) > 0 ))`
  works today (DD#58), which is an argument for option 1 — the capability is
  present, only the bash spelling is absent.
- DD#23 maps every external to an absolute path; there are no bare externals.
- The taint rule is operational: anything through `execve(2)` is tainted.

## Related file-read decision

`dollar-lt-file-policy` is resolved as a decision (2026-11-10): DD88
accepts direct file-read capture-conditions, with implementation queued in
`recoverable-file-failures`. Its old IFS question is already settled by
DD22/DD99's no-implicit-splitting rule. Neither resolves this task's
arithmetic-position question.
