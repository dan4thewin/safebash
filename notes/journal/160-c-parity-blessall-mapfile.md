# 160 — c parity (batch 3): bless-all, mapfile-from-file, array append

Un-guarding the DD#61 bless-all runtime tests (taint.sh 60.9–60.36, ad.1–3)
for c required three emit_c gaps, plus a bonus one:

## RedirectPush / RedirectPop

`mapfile -t a < file` lowers to a `RedirectPush [< file]` … `Mapfile` …
`RedirectPop` triple. Both push/pop were `/* TODO command */`, so the file
was never opened on fd 0 and mapfile read from the real (empty) stdin.

emit_c already had `emit_redirects` / `emit_restore` (per-command save/apply/
restore) but no persistent stack across the two commands. Added
`Emit.redirect_stack: Vec<Vec<SavedFdC>>`; `RedirectPush` applies the
redirects and pushes the saved-fd list, `RedirectPop` pops and restores. These
are handled at the **top** of `emit_command`, before the per-command
auto-save/restore that would otherwise undo the redirect immediately (that
double-application was visible as two `sash_redirect(0,…)` calls).

## BlessAll / BlessRetag

`bless x in a [[ p ]]` (array) and `bless k v in m [[ p ]]` (map) were a
`/* TODO inst */` no-op. Added `emit_bless_all_c` mirroring native
`emit_bless_all`: loop `i` over `sash_table_len(tbl)`, bind the binder(s) from
`sash_table_value`/`sash_table_key`, evaluate the predicate via the existing
`lower_test_expr`; non-if-form aborts (`sash_bless_fail`) on the first failing
cell, if-form ANDs results into `__test_result`.

Shadowing (60.22: `x=outer; … bless x in a …; echo $x` → `outer`) works
because `c_var_name` collapses the binder's unique SSA version onto its name,
so I **save and restore** the binder's `_ptr`/`_len` slots around the loop.
The binders also needed C declarations (`collect_inst_var_types` gained a
`BlessAll` arm). `BlessRetag` is a no-op in c (versions collapse).

## ArrayAppend (bonus)

Un-guarding surfaced a pre-existing gap: `a+=( two )` (SsaInst::ArrayAppend,
discriminant 6) was `/* TODO inst */`, so 60.11 read empty. Added
`emit_array_append_c`: set the value at integer key `sash_table_len(a)` (the
next index), mirroring native `emit_array_append`.

## Un-guarded

- taint.sh bless-all block `in_modes native` → `native,c` (60.9-60.36, ad.1-3).
  24.20 stays native-only (checks `$rerr`, which the c harness discards).
- errors.sh e.b70.fndisc `native` → `native,c` (already worked in c — a
  `-> int` fn in `|| :` discard; janet still deferred).

## Result

Full c sweep: **0 regressions, +19** (`c:60.9-60.36`, `c:ad.1-3`,
`c:e.b70.fndisc`). Only `c:38.5` / `c:55.19` (`$!` bg pid) remain failing.

Complexity: kept `emit_inst` lean by moving the bless family into an
`emit_bless_inst` sub-dispatcher (alongside the existing `emit_read_inst`).

## Files

- `sash/src/emit_c.rs` — `Emit.redirect_stack`; RedirectPush/Pop in
  `emit_command`; `emit_bless_inst` (Bless/BlessAll/BlessRetag) +
  `emit_bless_all_c`; `emit_array_append_c` in `emit_read_inst`; BlessAll +
  ArrayAppend arms in `collect_inst_var_types`.
- `spec/taint.sh`, `spec/errors.sh` — guard widenings.
