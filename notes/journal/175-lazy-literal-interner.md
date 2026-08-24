# 175 — Lazy literal interner (P2 of eliminate-ice-arms-via-types)

Continued the ICE-arms cohort. **P2**: the five codegen sites that looked
up a string in the `literals` map and `panic!`'d on a miss
(`"ICE: missing literal"` / `"ICE: assoc array key not interned"`).

## The invariant they guarded

`setup::intern_literals` pre-interns every string the SSA references, so a
miss is only reachable if a codegen path *drifts* from that pre-pass — a
latent-bug backstop, not a user-reachable error. But a bare `panic!` is a
sharp edge: any future codegen path that synthesizes a string the pre-pass
didn't see becomes an ICE instead of just working.

## Fix — `intern_or_declare` (values.rs)

A total read-or-declare helper:

```rust
pub(super) fn intern_or_declare(
    literals: &HashMap<String, DataId>,
    module: &mut ObjectModule,
    s: &str,
) -> DataId {
    if let Some(&id) = literals.get(s) { return id; }
    let data_id = module.declare_anonymous_data(false, false).expect("declare data");
    // …define bytes + NUL, define_data…
    data_id
}
```

Key design point (why this needed **no signature churn**): `module` is
already `&mut ObjectModule` at all five call sites, and Cranelift's
`declare_anonymous_data` needs no unique `__str_N` name (unlike
`declare_data`). So the miss path declares fresh anonymous data with zero
plumbing and no `&mut literals`. The un-deduplicated data on the rare drift
path is harmless. The `.expect("declare/define data")` matches the existing
`Interner` convention — a genuine module-builder failure, not an
ICE-invariant.

## Sites converted (5)

- `value_handlers.rs` `emit_glob_expansion` — dir_str `"."` + operand (2)
- `value_handlers.rs` `emit_array_join_expansion` — join separator
- `values.rs` `lower_func_call_value` — const-folded string
- `values.rs` assoc/indexed-array lowering loop — the array **key**

## Verification

- `cargo build`/`clippy` clean, no orphaned imports.
- Behavior spot-check: `for f in *.md`, `$(join a -)`, a func-constant, and
  `map m=([k]=v [j]=w)` all compile+run.
- Full `SPEC_MODES="native c janet"` sweep, latest-run-per-test diff vs base
  `d92f992`: **0 passing→failing, 0 gone, 0 new**. Native gate 0 failing.

No new specs: P2 is behavior-preserving — the panics were unreachable
backstops, so removing the drift-ICE risk changes no observable output.

## Cohort status

Done this session: P8 (join arity, a real bug → SB5040), P4 (CheckedArithOp),
P6 (case→bool), P1 Pattern-B (index-var is_int-total + `$@[i]`), **P2 (this)**.
Remaining: P1 Pattern-A `match` conversions (types.rs flag-checked sites);
keep+cite passes for the table / P3 / P5 / P7 sites.
