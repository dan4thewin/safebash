## 209 — int `:-` identity-element default (all 3 backends) + literal-only rule

**On `hand-parser`.** DG-directed, growing out of the open bug
`bug-cjanet-default-on-int-var` (int `${x:-D}` broken in c/janet).

### The semantic (DG-ratified)

`:-` triggers the default on the **identity element** of the subject's type:
`""` for strings (existing), **`0` for ints** (new). So `${int_x:-D}` ≡
`(x == 0 ? D : x)`. DD#62's "meaningful zero" is preserved — `:-` merely *picks*
0 as its trigger (the additive identity, the int analogue of the empty string),
op-locally; it does **not** treat 0 as "unset".

This exposed that **all three backends had been applying a *string-emptiness*
test to an int**. Native was itself wrong: `int x=0; echo ${x:-99}` returned
`0` (int→string "0" has length 1 → "non-empty" → default never fired); c/janet
failed to compile (`x_ptr` / `(length x)` on an int slot). A native behavior
change, not just a c/janet fix.

### Second refinement (DG, same day): literal-only default

When the subject is int, `D` must be an **int literal** (decimal / `0x`/`0o`/`0b`)
— not a string, an interpolation, or empty. Rejected at type_check (**SB4030**).
DG chose literal-only over "any int-typed default" ("disregard my `$(( ))`; go
with literal only"): it unifies with the arithmetic-context default rule
(DD#77 D4) — one mental model, "an int `:-` default is an int literal" — and
keeps the disposition homogeneously int.

### Implementation

- **transform** (`transform/expressions.rs`): the `:-` arm lowers an int source
  to `SsaValue::Ternary { cond: ArithBinOp{src, Eq, Expr("0")}, true_val:
  default, false_val: src }`. Reuses the ternary lowering, so **native + c**
  (which emit from SSA) get it for free — no `DefaultValue` int-awareness needed.
  `source_is_int` detects an int scalar (`type_map`) or int cell
  (`Subscript.is_integer_value`).
- **janet** (`emit_janet.rs`, `emit_pe_default`): janet emits from the IR, not
  SSA, so the ternary never reaches it. Its scalar branch now emits
  `(if (= name 0) (string default) (string name))` — both arms stringified so
  the value consumes uniformly under the int-context `scan-number` wrapper and a
  bare string context (a harmless, exact round-trip, janet-only).
- **type_check** (`type_check.rs`): new fourth pass `check_int_value_defaults`
  → `walk_value_default` (a flat dispatcher; arm bodies extracted to
  `walk_pe_default` / `walk_value_slice` / `walk_value_pairs` /
  `walk_value_cmdsub` to stay under the cognitive gate) → `check_int_pe_default`
  (`:-` + int subject + non-literal default → SB4030). Treats `$(( … ))` as
  **opaque** — `check_arith_default_literals` owns arith defaults, so no
  duplicate diagnostic (verified: `$(( ${x:-hello} ))` = exactly one SB4030).

### Scope notes

- **Bare `-`** (default-on-*unset*) is unaffected: sash vars are always set, so
  its default is dead code and its type is irrelevant. Native `${x-4}`/`${s-hi}`
  already match bash. (A phantom "miscompile" I chased was my own test error —
  `sash -c` takes a **code string** à la `bash -c`, not a file path.)
- **Janet int arrays** are broken independent of `:-` (`int array a=(4 5 6)` →
  `(scan-number @["4" "5" "6"])` → nil), so an int-array **cell** `:-` also
  fails in janet; native + c handle it. Filed →
  `janet-int-array-and-cell-default` (P3). cov.85 guarded `in_modes native,c`.

### Verification

Specs coverage.sh cov.78/80/81 (positives, all modes), cov.82/83/84 (rejects,
all modes), cov.85 (int-cell positive, native+c). Full 3-mode sweep, latest-
run-per-test diff vs HEAD `328e529`: **0 regressions** native / c / janet;
native+6, c+6, janet+5 new passing, c:cov.78 skip→pass. fmt + clippy clean;
new functions pass the cog/cc gate (`walk_value_default` cc=9, cog=1).

Docs: `bug-cjanet-default-on-int-var` → done (identity-element resolution),
DD#65 addendum, `janet-int-array-and-cell-default` filed.
