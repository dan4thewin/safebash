title: DD#33 — Unified hash table for arrays
status: implemented

Single `SashTable` for both indexed and associative arrays. Python
compact dict layout: sparse index array + dense entry array. FNV-1a
hash on byte slices. Open addressing with linear probing.

Two key representations:
- **Indexed arrays** (`array`): integer keys hashed as raw 8-byte i64,
  not converted to strings. Codegen stores i64 on stack, passes
  `(&i64, 8)` to table_set/table_get.
- **Associative arrays** (`map`): string keys passed as `(ptr, len)`.

Table owns copies of keys (heap-allocated on insert). Entries store
`(key_ptr, key_len, val_ptr, val_len)`. The validator catches
"declared indexed, used string key" mismatches at compile time.
