---
title: "Consider fd manipulation (dup, close, …) as explicit builtins rather than `>&`/`<&` syntax"
status: ready
kind: design
depends: []
gates:
  - "DG decision on whether fd dup/close is builtin functions vs redirect syntax"
---

## Context

`bug-redirect-dup-target-unvalidated` was fixed **reject-only** (2026-08-29,
DG): a `>&`/`<&` target must be a plain fd number (SB4048), and the close form
`>&-`, non-numeric targets, and trailing garbage are all rejected — no silent
dup-to-fd-2. That deliberately left a gap: sash now has **no** way to close an
fd, and only bare-numeric `N>&M` dup.

DG (2026-08-29): consider exposing fd manipulation as **builtins** instead of
(or alongside) redirect syntax — e.g. `dup`, `close`, … as first-class
value/void functions, the way the language already prefers explicit builtins
(toupper, len, join, …) over terse shell punctuation.

## Why builtins may be the better surface

- **Explicit + typed.** `close 2` / `dup 1 2` read clearly and validate like any
  other builtin call, vs. the `>&-` / `>&N` punctuation whose target parsing was
  exactly the silent-degrade hole just closed.
- **Consistent with sash's direction.** The language keeps trimming terse shell
  forms (DD#65 var-modifier prune, DD#75 bracket-subscript retirement) in favour
  of named builtins. fd juggling is the same shape.
- **Ties into the exec/trap purge.** The exec rework (see status.md / the "exec
  has no replacement for manipulate-file-handles + replace-the-running-program"
  thread) needs a sanctioned way to manipulate the running program's fds. A
  `dup`/`close` builtin family is a natural home for the "manipulate file
  handles" half.

## Open questions for DG

- Which primitives: `dup`, `dup2`, `close`, `open`? Just `dup`/`close`?
- Builtins **replace** `N>&M` syntax, or coexist with it?
- Value vs void semantics (does `dup` return the new fd?); taint of a returned fd.
- Interaction with the DD#55 abort model on a failed `dup`/`close`.

## Found

2026-08-29, fixing the redirect fd-dup silent-degrade
(`bug-redirect-dup-target-unvalidated`), per DG "file followup task to consider
dup, close, etc. as builtins."
