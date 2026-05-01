title: DD#33 — Unified hash table for arrays
status: implemented

Single `SashTable` for both indexed and associative arrays. String keys always
(`arr[5]` → key `"5"`). Python compact dict layout: sparse indices + dense entries.
Integer keys hashed as raw i64, not converted to strings.
