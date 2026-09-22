---
title: "argv with a dynamic slice start (`argv $n a`)"
status: todo
priority: P3
kind: feature
depends: []
gates:
  - "DG rules whether a runtime slice start is in the safe subset at all"
  - "if yes: `SsaInst::ArgvFill` carries a constant `start: i64` — it needs an SsaValue"
  - "if yes: both backends (Cranelift + emit_c) fill from a runtime index"
  - "if no: SB5085's message should say so positively, not just 'must be a literal'"
---

# `argv $n a` — a dynamic slice start

`argv N a` (DD#60) takes a **literal** N. `SsaInst::ArgvFill { array, start:
i64 }` carries a compile-time constant, so there is nowhere to put a runtime
value.

Until 2026-09-22 a sigilled index was not rejected — it was *mislowered*.
`try_handle_argv` returned `None` for any shape it could not lower, and `None`
in `try_builtin` means "not a builtin", so `argv $n a` became an ordinary
command named `argv`, compiled clean, and the binary died with `sash: bug:
bare command passed to sash_exec`. SB5085 now rejects it at compile time,
which is correct but is a *narrowing by default*, not a considered answer.

The question for DG is whether a runtime start belongs in the subset. The
conservative reading of "the compiler should reject anything not fully
understood" says the current rejection is right and this task closes as
`wontfix`; the counter-argument is that `argv $n a` is the natural way to
write "skip the flags I already parsed", and today the only spelling is a
literal, which forces the count to be known at compile time.

Nothing depends on this: SB5085 makes the current behaviour safe either way.
