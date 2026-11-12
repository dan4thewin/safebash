title: "The two structural getters that still end in a wildcard"
status: done
priority: P2
kind: hardening
depends: []
gates:
  - "`SsaValue::child_values()` is total over `SsaValue` with no `other =>` / `_ => return None`"
  - "`Statement::direct_exprs()` is total over `StmtKind` with no `_ => {}`"
  - "both carry the `do not add a wildcard` marker, so `contrib/totality-check.py` holds them"
  - "every behaviour change from newly-yielded children is read, not silenced"

> **Half done, 2026-09-29 (journal 436).** `SsaValue::child_values()` is now
> total over all 30 variants with the `do not add a wildcard` marker; the
> three hand-compensations in `intern_value`, `contains_capture` and (via
> `splat_taint`) `value_is_tainted` are gone. One of the four dropped children
> was a **live taint bug**: `ArraySplat`'s `:-WORD` never reached
> `value_is_tainted`, so `array b=( @{a:-$tainted} )` derived `b` CLEAN. Fixed,
> with specs `bkstp.5`-`bkstp.8`. The other three were real gaps but
> unreachable — probed, no panic, correct output.
>
> **`Statement::direct_exprs()` is now total too, 2026-10-01 (journal 456)** —
> the nine `Statement`-only variants spelled out, `ArithTest` given its own arm
> and a documented reason, and the marker added. Verified the gate fires on it
> by putting a wildcard back.
>
> **Two claims in the body below are stale and were corrected by measurement:**
>
> - The explicit lifetime it names as a blocker **is already present**; it was
>   added when `child_values()` was done. Not a blocker.
> - "exactly one consumer" is wrong — there are **thirteen**: `ir::walk_own_exprs`,
>   `type_check::check_int_value_defaults`, and eleven in `validate.rs`.
>
> **`ArithTest` still yields nothing, deliberately.** Of the ten unlisted
> variants, nine own only `Statement`s; `ArithTest(Box<ArithExpr>)` is the only
> one that can own an `Expr` (via `ArithExpr::Expansion`), and that is reachable
> — confirmed through `--emit-ir`. It is not a live gap today because **eight of
> the thirteen callers carry a hand-written `if let StmtKind::ArithTest`
> compensation right beside the call** (the shape journal 436 removed for
> `child_values`), and the other four are covered by separate arith passes.
>
> **The remaining fork, for DG.** Making `ArithTest` yield its `Expansion`
> payloads would let those eight compensations be audited and trimmed — but two
> of them (`check_positionals_arith`, `dollar_at_arith`) also inspect
> `ArithExpr::DollarVar`, which is **not an `Expr`**, so they cannot simply be
> deleted; and both already handle `Expansion` themselves, so yielding it here
> would make them **double-report**. That is a refactor across eight validators,
> the same character as the taint-routing piece below, and not a getter change.
>
> **Working this task found a P1.** Reading those eight compensations surfaced a
> *private duplicate* of `walk_arith_expansions` in `type_check.rs` whose
> `_ => {}` dropped `Subscript`/`SubscriptAssign` — an ICE natively and a silent
> `sash_atoi("abc")` under `--emit-c`. See
> `bug-arith-default-blind-under-subscript`.


# Two getters still hand out an incomplete answer

Filed 2026-09-28 (journal 435), after making the taint read path total.

The correlation this project keeps re-measuring: **every taint escape it has
found sat behind a wildcard match arm; none has been found behind an exhaustive
one.** The A/B is inside a single file — `classify_inst_def` is exhaustive over
all 19 `SsaInst` variants and has produced no escapes; `check_inst_uses` ended
in `_ => {}` at 4 of 19 and produced both of the escapes closed in journal 435.

The taint path is now total end to end. Two structural getters are not.

## 1. `SsaValue::child_values()` — `sash/src/ssa.rs`

```rust
SsaValue::Concat(parts) | SsaValue::IndexedArray(parts) => parts.iter().collect(),
SsaValue::CmdSubst { argv, .. } => argv.iter().collect(),
SsaValue::AssocArray(pairs) => pairs.iter().map(|(_, v)| v).collect(),
other => other.operand_child_values(),   // → subject_child_values()  `_ => return None`
                                         // → operator_child_values() `_ => return None`
                                         // → unwrap_or_default()
```

A variant nobody listed yields **no children, silently**. Journals 430 and 432
are both this. Audited 2026-09-28; four variants that own `SsaValue` children
are unlisted today:

| variant | unreached child |
|---|---|
| `FuncCall { args }` | every argument |
| `IntFuncCall { args }` | every argument |
| `IntCell(inner)` | the wrapped value (DD#76) |
| `ArraySplat { disposition: Some(d) }` | `d.word` — the `@{a:-word}` default |

Three callers, so this is **not** a taint-local change:

- `value_is_tainted` (ssa_taint.rs) — taint derivation
- `Interner::intern_value` (codegen/setup.rs) — literal interning. Under-
  interning is the failure that *panics* codegen, so newly-yielded children may
  be a latent fix here.
- `contains_capture` (transform/functions.rs) — capture detection

Each new child is a behaviour change for all three. Expect new taint
propagation, new interned literals, new capture detections; read each one.

`check_value_use` deliberately writes its recursion out by hand rather than
delegating here, precisely because this wildcard would put the hole back one
level down. Once this is total, that duplication can go.

## 2. `Statement::direct_exprs()` — `sash/src/ir.rs`

Ends in `_ => {}`. Note it **already** yields redirect targets
(`StmtKind::Redirected => redirect_target_exprs`) — so the AST taint pass could
have seen the redirect escape for free. It never calls it: `direct_exprs` has
exactly one consumer, `walk_own_exprs_in_stmt` → `validate_stmt_ctx`, which is
not taint. The taint pass instead reaches `check_expr_taint` from **20
hand-written call sites**, and that function's own comments are a history of
missing ones.

So there are two pieces here, and the second is the consequential one:

- make `direct_exprs` total (contained);
- route the AST taint pass through it, replacing the 20 call sites (**not**
  contained — it changes diagnostic ordering and possibly messages across the
  whole front end, and every spec asserting an SB code in a specific order is
  exposed). `check_expr_taint` is already total over `ExprKind`, so this buys
  call-site completeness, not variant completeness. Worth doing, worth doing
  on its own, and worth DG seeing the diff shape before it starts.

## Guard already in place

`contrib/totality-check.py`, run by `.githooks/pre-commit`: any function whose
doc comment says `do not add a wildcard` must have no wildcard arm. **Both
getters above now opt in** — adding the marker is the last step of fixing each,
not the first.
