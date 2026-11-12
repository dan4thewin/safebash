title: DD#32 — Honest trace + embedded source
status: partial

1. Native binaries embed their native source, retrievable with `--extract`.
2. Runtime tracing reports branch/loop outcomes and evaluated values.
3. Trace machinery is a compile-time opt-in; the runtime environment may
   activate included sites but cannot add tracing to a binary built without it.

---

`.sash_source` ELF section embeds sash-native source (implemented).
`sash --extract <binary>` dumps the embedded source.

Runtime trace, in two halves:

- **Branch and loop outcomes — implemented** (native backend only).
  Compile with `--trace`, then run with `SASH_TRACE=1`; each branch and
  loop terminator writes `+ <kind> → <label>` to stderr. Specs
  `tr.1`-`tr.5` in `spec/trace.sh`. `--trace` is a no-op for `--emit-c`.
- **Evaluated values — designed, not built.** This is what keeps the
  status `partial`.

Tracing is opt-in **at compile time**: a binary built without `--trace`
carries no trace sites, so `SASH_TRACE` cannot make it emit anything.
That ordering is deliberate — the environment can only activate machinery
the build already chose to include, never introduce it (DD#107 applies
the same rule to the heap size).

> Corrected 2026-09-13. This file previously said the whole runtime trace
> was "designed, not built", which was false from the moment `--trace`
> landed and had already been repeated back as a gap by the adversarial
> audit (`audit/dd-audit/audit-gemini-pro.md`).

> 2026-10-31 (journal 550). A `&&`/`||` junction inside `[[ ]]` is now a
> branch of its own (DD#95 addendum), so it traces like any other branch:
> `+ branch → test.and.right`, `test.or.merge`, and so on (tr.5).
