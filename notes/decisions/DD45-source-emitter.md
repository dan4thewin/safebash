title: DD#45 — Source emitter architecture
status: implemented

Coerce in lowerer, emit from CST. SourceEmitter tracks position in
original bytes, copies unchanged regions verbatim, substitutes coerced
text. `--emit-source` and `.sash_source` ELF section produce identical output.
Declaration axes amendment: four orthogonal axes freely stackable.
