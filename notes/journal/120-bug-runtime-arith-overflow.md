# 120 — P0 bug: runtime integer overflow silently wrapped (DD#25)

Unattended round (SOP: dependency-ordered task work, fix straightforward
blocking bugs, file decisions). Started on `abort-diagnostics-remaining`
(the DD#55 (A)-channel diagnostics follow-up); its DD#25 overflow gate led
to a **stop-everything bug**.

## The bug (P0, security)

DD#25 ("integer overflow traps at runtime, not silent wraparound", marked
SECURITY) was **not implemented** for runtime arithmetic. `emit_arith_op`
emitted plain `iadd`/`isub`/`imul`; cranelift wraps. Confirmed:

```
mul 3037000500 3037000500   # → -9223372036709301616, rc=0  (should trap)
```

`+` and `-` wrapped identically. The C backend (`emit_arith_op_c`) emitted
plain `+`/`-`/`*` — same silent wrap. The const-fold path
(`fold_arith_op`) used `wrapping_add`/`mul`/`sub` — folded overflow
silently too.

**Why spec 25.2/25.3 falsely "passed":** `int x=MAX; (( x += 1 ))` wraps
to *exactly* i64::MIN, and `echo $x` → `sash_itoa` → `itoa_inner` did
`n.checked_neg().expect(...)` which **panics** on i64::MIN. So the abort
was the itoa panic, not overflow detection — a non-MIN overflow (the `mul`
above) wrapped and echoed fine with rc=0. Two bugs masking each other.

## Fixes

Spec-first: `spec/arithmetic.sh` 25.4–25.6 (runtime fn-arg overflow for
*,+,−), 25.7 (i64::MIN echoes), 25.8 (native diagnostic). All red first.

1. **sash-rt**
   - `itoa_inner`, `write_i64`, `itoa_buf`: compute magnitude via
     `unsigned_abs()` (u64) — i64::MIN now formats as
     `-9223372036854775808` instead of panicking.
   - New `sash_arith_overflow() -> !`: writes `<argv0>: arithmetic
     overflow\n`, exits 2.
2. **Native codegen** (`arithmetic.rs`): new `emit_checked_arith` — Add/
   Sub/Mul use cranelift `sadd_overflow`/`ssub_overflow`/`smul_overflow`
   (0.130; returns `(result, of)`, `of` is i8); `brif of` → abort block
   calls `sash_arith_overflow`. Wired the extern through `ExternFuncs` +
   `setup.rs`.
3. **C backend** (`emit_c.rs`): `emit_checked_arith_c` uses
   `__builtin_{add,sub,mul}_overflow` → `sash_arith_overflow()`; prelude
   decl added.
4. **Const-fold** (`expr_handlers.rs`): `fold_arith_op` returns
   `Option<i64>` — `checked_{add,sub,mul}` + guarded shift/pow/div; on
   overflow returns `None`, so `try_fold_binop` falls back to a runtime
   `ArithBinOp` and the runtime trap fires. **Decision-free**: a
   compile-time-known overflow becomes a runtime trap, not a new
   compile-time error. (Whether it should instead be a compile error is a
   DG sub-decision, noted in the bug task — not taken here.)

## Behavior

- `mul/add/sub` overflow → `<argv0>: arithmetic overflow`, exit 2 (native
  and C).
- `-` (suppress) does **not** suppress overflow — it's an unconditional
  trap (DD#25), verified.
- Normal/negative/large-valid arithmetic unaffected; i64::MIN echoes.

## Verification

native spec **0 failing** (525 passing); c=58 / janet=106 unchanged —
**zero regressions** (authoritative spec.db diff). janet overflow stays
out of scope (bignum; `bug-janet-int-overflow` wontfix), so the overflow
specs are `in_modes native,c`.

## Left for follow-up

- **Pow** saturates to i64::MAX (sash-rt:1999) — also a DD#25 divergence
  (saturate ≠ trap). **Shl** (shift ≥ 64), unary **Neg** (i64::MIN), and
  **Div** (i64::MIN / -1, SIGFPE) are overflow-capable and not yet
  trap-with-message. Filed in `bug-runtime-arith-overflow-wraps` "Other
  ops".
- The original `abort-diagnostics-remaining` still has its 5 other gates
  (external/cmdsub/pipeline/bg-exec/regerror9) open — the overflow gate is
  now done via this bug fix.
