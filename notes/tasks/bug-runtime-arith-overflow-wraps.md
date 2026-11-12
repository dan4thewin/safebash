title: "BUG (P0/security) — runtime integer overflow silently wraps (DD#25 violation); itoa(i64::MIN) panics"
status: done
priority: P0
kind: bug
depends: []
gates:
  - "[DONE] runtime (non-const) overflow on +, -, * traps with `<argv0>: arithmetic overflow` and rc != 0 (native + c)"
  - "[DONE] echo of a legitimate i64::MIN (-9223372036854775808) prints correctly, rc=0 (no panic)"
  - "[DONE] spec 25.2/25.3 pass for the RIGHT reason (trap at the arith step, not the masked itoa panic)"
  - "[DONE] new spec cases: runtime fn-arg overflow for +, -, * each trap; itoa(MIN) echoes (25.4-25.8)"
  - "[DONE] DD#25 frontmatter/implementation note corrected"
  - "[DONE] native spec 0-failing; c/janet baseline held"
  - "[DONE 2026-06-26b] Pow (was saturate→now traps); Div i64::MIN/-1 (was SIGFPE→now clean trap), native+c. Neg already traps (0-x→ssub_overflow); Mod MIN/-1 = 0 (srem, correct)."
  - "[RESOLVED 2026-06-27, DG] Shl left-shift: NO overflow check — shifts are bit manipulation, not arithmetic. `1 << 63` evaluates (the `<<` lexing bug was fixed 2026-06-26). Already correct in codegen (plain `ishl`, no trap). By design, not deferral."
  - "[RESOLVED 2026-06-27, DG] const-fold overflow: runtime trap STANDS. DG: runtime sufficient unless compile-time detection is essentially free — investigated, it is NOT free. The fold detection (`checked_*`→None, expr_handlers.rs) is immediately discarded into a runtime ArithBinOp; surfacing it as a compile error needs a mutable diagnostics sink threaded through the entire `&VersionState`-immutable arith recursion (arith_to_ssa/binop_to_ssa/unaryop_to_ssa), or a new constant-evaluator in the validator (which does NOT fold today). Neither is free. All-literal overflow (e.g. `(( 2**63 + 1 ))`) already traps correctly at runtime."
  - "[SPLIT OFF] out-of-range integer *literal* → 0 — see `bug-arith-literal-out-of-range`"

# BUG — runtime integer overflow silently wraps (DD#25 violation)

**Status 2026-06-26: +, -, *, Pow, Div, Neg FIXED on native + C.** Mod
MIN/-1 is correctly 0 (no trap needed). itoa i64::MIN fixed. Remaining:
Shl (blocked on the `<<` heredoc bug) and the const-fold-policy
ratification. See journals 120 & 122.




## Repro (native)

```bash
$ cat > t.sh <<'EOF'
fn mul { int a=$(($1)); int b=$(($2)); (( c = a * b )); echo $c; }
mul 3037000500 3037000500   # 3037000500^2 = 9.2234e18 > i64::MAX
EOF
$ sash t.sh -o t.bin && ./t.bin
-9223372036709301616        # WRAPPED, rc=0 — should have trapped
```

`+` and `-` wrap identically (confirmed: `add 9e18 1e18` → `-8.4e18`,
rc=0).

## Root cause

- `codegen/arithmetic.rs::emit_arith_op` emits plain `iadd`/`isub`/`imul`
  for Add/Sub/Mul — **no overflow check**. Cranelift wraps.
- `transform/expr_handlers.rs:415` const-folds with `wrapping_add` /
  `wrapping_sub` / `wrapping_mul` — the compile-time path wraps too.
- The C backend (`emit_c`) emits C `+`/`-`/`*` — signed overflow is UB,
  wraps in practice. Same bug.

## Why 25.2/25.3 "pass" today (accidental)

`int x=i64::MAX; (( x += 1 ))` wraps to **exactly i64::MIN**. Then
`echo $x` calls `sash_itoa` → `itoa_inner` (sash-rt/src/inner.rs:48):

```rust
n = n.checked_neg().expect("sash: integer overflow in itoa");
```

`checked_neg()` on i64::MIN is `None` → **panic** → rc=99. So the spec's
`rc != 0` holds, but for the wrong reason — it's the itoa panic, not
overflow detection. A non-MIN overflow (e.g. the `mul` above) wraps and
echoes fine with rc=0, exposing the real bug.

## Secondary bug (itoa)

`echo` of a *legitimate* i64::MIN panics:

```bash
$ printf 'int x=-9223372036854775808\necho $x\n' | sash -c ... # rc=99 "sash: panic"
```

i64::MIN is a valid int and must format as `-9223372036854775808`. Fix
`itoa_inner` to compute the magnitude via `unsigned_abs()` (u64), which
handles MIN without negating into i64 range. (`write_i64` at
sash-rt/src/lib.rs:534 and `itoa_buf` at :2883 have the same latent
defect — `wrapping_neg`/`-n` mis-handle MIN; fix all three.)

## Fix approach

1. **sash-rt:** `itoa_inner` (and `write_i64`, `itoa_buf`) handle i64::MIN
   via `unsigned_abs()`. Add `sash_arith_overflow() -> !` that writes
   `<argv0>: arithmetic overflow\n` and exits non-zero.
2. **Native codegen:** `emit_arith_op` Add/Sub/Mul → cranelift
   `sadd_overflow` / `ssub_overflow` / `smul_overflow` (0.130, returns
   `(result, of)`); `brif of` → abort block calling `sash_arith_overflow`.
3. **C backend:** emit `__builtin_{add,sub,mul}_overflow` guards that call
   the same abort (or a c-mode equivalent).
4. **Const-fold (expr_handlers.rs:415):** replace `wrapping_*` with
   `checked_*`; a compile-time-known overflow should be a **compile
   error** (a literal `(( 2**63 ))` is statically over-max) OR emit the
   runtime trap — DECISION for DG: reject at compile time vs. defer to the
   runtime trap. (File sub-decision; do not silently keep wrapping.)
5. Spec-first: add runtime fn-arg overflow cases for +, -, * and an
   itoa(MIN) echo case to `spec/arithmetic.sh`; assert the message in
   `rerr` (native).

## Scope notes

- **janet**: overflow is wontfix (`bug-janet-int-overflow` — janet uses
  float/bignum, no i64 trap). Keep overflow specs `in_modes native,c`.
- **Other ops**: `Pow` currently *saturates* to i64::MAX
  (sash-rt:1999) — also a DD#25 divergence (saturate ≠ trap); `Shl`,
  unary `Neg` (i64::MIN), and `Div` (i64::MIN / -1) are overflow-capable.
  Fix +/-/* first (confirmed), then sweep these (file follow-up if large).
- Supersedes the overflow gate of `abort-diagnostics-remaining`.
