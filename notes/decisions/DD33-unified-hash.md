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

## Amendment (2026-09-26) — the validator catches *both* mismatches

The line above records only one direction: "declared indexed, used string
key". The implementation has always had the other one too, and DD#80 cites
*this* DD for it — "on a map a bare integer read is rejected (DD#33: it reads
like an array index)" — while the text here never said so. Recording it, so
the citation lands somewhere real:

- **string key on an indexed array** → SB4030. An `array` is keyed by i64; a
  string is not one.
- **integer key on a map** → SB4030. Not because a map cannot *hold* `"5"` as
  a key — it can, and `${m["5"]}` is how you say it — but because an unquoted
  `${m[5]}` reads like an array index at a glance. The rejection is about the
  reader, not the table.

As of `761b3f4` the second rule is about the key's **type**, not its spelling:
any int-typed key is rejected on a map, so `${m[$i]}` on an `int i` and
`${m[$((1+1))]}` join the bare literal. Quoting is the conversion. See DD#80's
2026-09-26 amendment for the full rule and DD#105 for why the crossing must be
written rather than inferred.

Nothing about the `SashTable` layout changes — both key representations are
exactly as described above. This amendment is about which mismatches the
compile-time check names.
