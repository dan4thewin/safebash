---
title: "emit_c's `_ =>` arms emit a TODO *comment* for unimplemented builtins/insts (fail-open)"
status: ready
priority: P1
kind: bug
depends: []
gates:
  - "an unimplemented `BuiltinCmd` in the c backend fails at compile time rather than emitting `/* TODO command: … */` and a silently-missing statement"
  - "same for the `SsaInst` `_ =>` arm (`/* TODO inst: … */`)"
  - "whatever the chosen mechanism, a spec pins it: a program using an unimplemented surface must not compile-and-silently-do-nothing"
---

## Symptom

`sash/src/emit_c.rs`, `emit_command_body`:

```rust
_ => {
    if !emit_process_builtin(…) && !emit_builtin_extra(…) {
        writeln!(e.out, "    /* TODO command: {} ({:?}) */", name, kind).unwrap();
    }
}
```

An unimplemented builtin emits a **C comment**. The generated C compiles, the
binary links and runs, and the statement silently does nothing. `lower_inst`
has the identical pattern for `SsaInst` (`/* TODO inst: {:?} */`).

## Why this is P1

This is a fail-**open** default inside a compiler whose premise is failing
closed. It has produced three silently-wrong builtins found so far, all in one
sitting (journal 278):

| builtin | symptom before the fix |
|---|---|
| `Exec` | `exec CMD` ran nothing and fell through to unreachable statements; `exec > file` un-did its own redirect |
| `Close` | `close 9` on a bad fd exited 0 instead of aborting with EBADF |
| `TableDelete` | `unset m[k]` was a no-op — key present, `len` unchanged, value still readable |

None of these were caught by the suite, because the suites that would have
caught them were gated `native`-only *because* the backend was incomplete —
the gate and the gap kept each other alive.

## The measurement method that found them

Enumerating variants with no reference in `emit_c.rs` is misleading (21 of 41,
but most are expression intrinsics or control flow on other paths). The
behavioral test is the one that works: write a minimal program per builtin,
`--emit-c`, and grep the output for `TODO command`. Worth keeping as a
standing check — arguably it belongs in the suite rather than in a notebook.

## Design question for DG

The fix direction is a genuine fork and is **not** being taken unilaterally:

- **(A) hard compile error.** `_ =>` becomes a diagnostic ("builtin X is not
  supported by the c backend"). Loudest, and matches the project's failing-
  closed stance. Risk: if any currently-passing path legitimately relies on a
  benign TODO, it turns into a regression — needs a sweep to find out.
- **(B) emit an abort.** Generate a runtime `sash_emsg` + abort instead of a
  comment. Still compiles; fails loudly at the point of use rather than at
  build time. Weaker than (A) but zero risk of breaking a compile that works.
- **(C) exhaustive match.** Remove `_ =>` entirely so the *Rust* compiler
  refuses to build when a variant is added without a c arm. Strongest — the
  gap cannot recur — but forces an explicit arm (even if only an explicit
  "unsupported" one) for all 41 variants, and the same for `SsaInst`.

(C) is the one that prevents recurrence rather than reporting it. (A) and (C)
compose: exhaustive match, with the explicitly-unsupported arms emitting the
(A) diagnostic.

## Notes

Found while closing `c-exec-support`. The same audit direction should be run
against `SsaInst` — it was observed to have the pattern but was not probed.
