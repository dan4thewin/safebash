# 208 — Coverage grind round: +3 never-run fns, +1 c/janet bug found

Returned to the grind (per DG). Re-measured per-function count first (the
authoritative unit): the bug#3/#4 work had already removed `incdec_to_ssa`,
`compound_assign_to_ssa`, `lower_redirected_function`, so the frame moved from
95.03% (73 never-run) to 95.30% (70) before any new specs.

## +3 named fns flipped 0→>0 (verified by re-measure)

`spec/coverage.sh` cov.76-78:
- `skip_balanced_squote` / `skip_balanced_dquote` (parse/lexer) — a `'`/`"`-quoted
  run inside a `$( )` inside `"…"`; the tainted cmdsub is then SB5020 but the
  lexer runs first (cov.76/77).
- `parameter_expansion_type` (type_check) — `int n=${x:-0}` `:-` type-preserve
  (cov.78, native-only — see the bug below).

Functions **95.30% → 95.50%** (67 never-run).

## The grind's payoff: a real c/janet bug (DG's thesis again)

cov.78 (`int x=5; int n=${x:-0}; echo $n`) passes native but **fails c AND
janet**: both mishandle `${int_var:-default}` — c emits `sash_atoi_or(x_ptr,
x_len, 0)` for `x_ptr`/`x_len` that don't exist on an int var (gcc:
`'x_ptr' undeclared`); janet emits `(length x)` on a number. Native is correct.
Filed `bug-cjanet-default-on-int-var.md` (P2); cov.78 guarded `in_modes native`
meanwhile.

## 4 specs missed their target (the classic caveat) → reclassified in accounting

Each was re-measured and confirmed to hit an adjacent path, not the named fn:
- `collection_reader_taint` + `bare_name_tainted` → **shadowed-dead**: `$(join …)`
  / `$(keys …)` lower to intrinsics (`try_native_intrinsic`), never a `join`/
  `keys` Command in SSA, so the ssa-taint join/keys arm is unreachable.
  Removable.
- `extract_from_literal` / `extract_leading_operator` → only reachable from the
  dispatch-glued operator form (`[[ $x==a { … } ]]`), which currently errors
  SB4014 (P3 `dispatch-compact-operator-spacing`). Compact `[[ a==b ]]` splits
  `==` into `Tok::Eq` (normal path). Deferred.
- `scan_base_hash_span` → reachability discrepancy: the path looks direct
  (`next_arith_token`→`scan_arith_number`→ `#` → it) yet direct `$(( 16#ff ))`
  rejects SB4035 without flipping it; the `$(( ))`/`(( ))` arith likely
  tokenizes base# elsewhere. Needs a trace. cov.79 keeps the direct-reject
  regression test regardless.

## Process notes

- Removed the 4 missed specs' transient ids (they were briefly cov.80-82); had
  to `DELETE FROM runs/tests` the orphaned `*:cov.8[012]` rows so `spec.db`
  wouldn't carry stale statuses (a removed test keeps its last row — the same
  masking hazard as the id-collision work). Re-swept: authoritative
  `test_status` 0 native / 0 c failing.
- Grind state tracked in the session `grind` table; accounting updated in
  `notes/coverage-accounting.md`.
