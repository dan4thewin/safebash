title: DD#45 — Source emitter architecture
status: implemented

1. Native binaries retain their native source for extraction (DD32/DD109).
   Bash conversion and annotated source emission belong to `sash-convert`.
2. Environment changes are explicit external delegation, not mutation of
   inherited environment through sash bindings (DD53).

---

~~Coerce in lowerer, emit from CST.~~ SourceEmitter tracks position in
original bytes, copies unchanged regions verbatim, substitutes coerced
text. ~~`--emit-source` and `.sash_source` ELF section produce identical output.
Declaration axes amendment: four orthogonal axes freely stackable.~~
DD57 separates converter emission from native compilation; DD35/DD86 retain
three declaration axes and positional lifetime.

Export rejection amendment (SB5001): `export` and `declare -x` cannot
be mechanically coerced — they are compile errors. ~~Use `FOO=bar cmd`
for per-command env vars.~~ DD53 requires `/usr/bin/env` delegation.
The validator rule catches both keyword and flag.
