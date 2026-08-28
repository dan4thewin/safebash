---
title: "Malformed value-intrinsic arity falls through to sash_exec (runtime `bug:` abort)"
status: pending
priority: P1
kind: bug
depends: []
gates: []
---

## Symptom

A value intrinsic (`$(sub …)`, `$(toupper …)`, etc.) called with the **wrong
argument count** compiles cleanly, then aborts at **runtime** with an
internal-invariant message:

```
$ sash -c 'echo $(sub a)'      # sub needs 3 args (RE SUB STR)
sash: bug: bare command passed to sash_exec (expected absolute path)
[exit 127]
```

Reproduces with:
- `$(sub a)` (1 arg), `$(sub a b)` (2 args) — `sub` needs 3
- `$(toupper)` (0 args) — needs 1

`$(len a b)` (extra arg) does NOT abort — it silently ignores the extra arg
and returns `len a`. So the fallthrough is per-intrinsic and inconsistent.

## Why it's a bug

The `bug:` prefix is sash's internal-invariant-violation channel — it must
never be reachable from user source. Here the path is:

1. `try_native_intrinsic` (`transform/expr_handlers.rs`) matches the builtin
   name, calls e.g. `regex_replace_intrinsic`, which returns `None` when the
   arg count doesn't fit.
2. `None` makes the caller fall through to the **regular `$( )` argv path** —
   i.e. `sub`/`toupper` is now treated as an external command name.
3. DD#23 resolution does not reject the bare name at compile time (it slips
   through as a bare command), so codegen emits a command whose argv[0] is a
   bare `sub`, and `sash_exec` aborts at runtime (`bare command passed to
   sash_exec`).

Expected: a malformed intrinsic call is a **compile-time** error (a new SB
code, e.g. "SB40xx: `sub` expects 3 arguments (RE SUB STR), got 1"), never a
runtime `bug:` abort and never an attempt to exec a bare `sub`.

## Fix sketch (present options to DG)

- **A. Arity-check in `try_native_intrinsic`**: when the name is a known
  intrinsic but the arg count is wrong, emit a spanned diagnostic instead of
  returning `None`. Enumerate each intrinsic's arity (len=1, keys=1, sub=3,
  gsub=3, match=2, toupper/tolower/basename/dirname=1, join=1+). Cleanest and
  most specific message.
- **B. Backstop at resolution/codegen**: any bare (non-path, non-language)
  command name that survives to the exec funnel is SB5011 at compile time,
  not a runtime abort. Catches this class generally (and would have caught
  the `$x`-as-command family) but gives a less specific message.
- **C. Both** — A for the good message, B as a defense-in-depth invariant so
  no bare name can ever reach `sash_exec`.

Note the sibling inconsistency: `$(len a b)` ignores extra args rather than
rejecting. A complete fix should make arity handling uniform (reject extra
args too, or document the ignore).

## Found

2026 coverage grind — surfaced while probing inputs to reach the never-run
`try_keys_intrinsic`/intrinsic paths. Same class of "unexamined path hides a
bug" as the const-reassign panic.
