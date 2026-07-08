---
title: "Excise (or revive) the dormant Coercion machinery in emit_source.rs"
status: done
priority: P6
kind: work
depends: []
gates:
  - "decide: remove the Coercion path, or keep it as a documented hook"
  - "if removed: embed_sash_source stores the raw source directly; Coercion/COERCIONS/annotate plumbing deleted; .sash_source section byte-identical to today (verbatim source)"
  - "if kept: a recorder actually populates COERCIONS, or the dead code carries a clear 'reserved' note + a test pinning the verbatim behavior"
  - "no spec/rust regression; --extract still round-trips the source"
---

> **DONE / already excised — verified 2026-07-08.** Commit `4e87034`
> ("dead code: remove vestigial emit_source module — Coercion never
> constructed") already removed exactly the option-1 (excise) scope. There
> is no `emit_source.rs` and zero `Coercion`/`COERCIONS` references in the
> tree; `.sash_source` is embedded verbatim by
> `codegen/entry.rs::embed_sash_source` (DD#45), `--extract` round-trips it
> (`lib.rs::dump_sash_source`), and `--emit-source`/`--annotate` now
> reject-and-direct-to-sash-convert. DG (2026-07-08): no reason to keep dead
> code. Task closed as moot.


# Excise (or revive) the dormant Coercion machinery

## Background

`emit_source.rs` defines a `Coercion { start, end, replacement, original }`
patch type and a thread-local `COERCIONS` list. `emit_source(source,
annotate)` sorts the patches and splices them into the original source
bytes (verbatim outside patched ranges), and `codegen/entry.rs::
embed_sash_source` stores the result in the `.sash_source` ELF Note
section (DD#45; retrievable via `--extract`).

**The recorder is gone.** The code that pushed `Coercion`s lived in the
old CST→IR `lower/`, deleted with tree-sitter in commit `73caecf`.
Nothing now writes to `COERCIONS`, so:

- the list is always empty,
- the splice loop never runs,
- `.sash_source` embeds the source **verbatim** (confirmed empirically:
  `int x=5\necho $x` → extracts `int x=5\necho $x`).

The current model rejects non-conforming bash with diagnostics (e.g.
SB5018 → `$(getenv HOME)`) rather than silently coercing; structural
bash→sash-native rewriting lives in the separate `sash-convert` crate.
So the coercion path is dormant infrastructure / effectively dead code.

## Decision for DG

1. **Excise** — simplest: `embed_sash_source` stores `bash_source`
   bytes directly; delete `Coercion`, `COERCIONS`, the `annotate`
   parameter, and the splice loop. `.sash_source` is unchanged (already
   verbatim). One less dead-code surface; `pub(crate)` already flags it.
2. **Keep as a hook** — leave it, but add a "reserved for a future
   in-compiler coercion pass" note and a test pinning the verbatim
   behavior so it doesn't silently rot further.
3. **Revive** — re-introduce a recorder (e.g. emit a coercion alongside
   each SB-coercible rejection) so `--extract` yields sash-native source
   again. Overlaps `sash-convert`'s job — probably not worth it; the
   reject-and-direct-to-convert model is the current design.

Recommend (1) unless there's an intent to make the in-binary
`.sash_source` section native-rendered rather than verbatim.

## Discovery

2026-06-21, while correcting the stale "emit from CST" docs (commit
`c15b430`). The doc fix documented the dormancy; this task tracks the
cleanup decision.
