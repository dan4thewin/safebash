---
title: "DD#81 Part B: uniform exact arity from `args` usage (SB5042/43/44)"
status: done
priority: P2
kind: feature
depends: []
gates:
  - "per-function signature inferred from args usage: literal index → fixed contiguous {0..max}; @args / $(len args) (sugar ${#args}/$#) / dynamic ${args[$i]} → variadic (floor = max literal index+1)"
  - "hole (non-contiguous literal indices, rule (a): full {0..max} must be read) → SB5044 at fn definition"
  - "every call site checked: fixed → exactly N (SB5042 too-few, SB5043 too-many); variadic → ≥ floor (SB5042)"
  - "applied on all three call paths: plain (inline_function_call), pipeline (build_function_pipeline_stage), capture ($() — now carries arg count after 50474e9)"
  - "existing un-coded 'reads $N but called with only M' diagnostic folds into SB5042"
  - "0 regressions native/c/janet; native gate 0"
---

## Context

DD#81 Part A (shadowing/name resolution), the func_constants removal, and the
`$(f arg)` arg-passing fix are all shipped (commits d73c69e, 56e4c17, 50474e9).
Part B — the *arity* half of DD#81 — remains. See
`notes/decisions/DD81-function-call-discipline.md` §"Part B" for the full rule
(hole rule (a) is decided).

Today: `collect_max_positional` (functions.rs) gives a **min**-arity from `$N`
reads only, checked on the plain + pipeline paths (`check_callee_arity`). It does
**not** understand the `args`-array forms, does **not** reject too-many, does
**not** detect holes, and does **not** run on the `$()` capture path.

## The model (from DD#81 Part B, DD#75 args)

`args` is 0-indexed; `$K` ≡ `${args[K-1]}`; `@args` ≡ `$@` is the splat;
`$(len args)` (sugar `${#args}`/`$#`) is the count.

- **literal index** `${args[k]}` / `$K` → contributes index `k`; signature =
  contiguous `{0..max}`, a gap is a **hole** (SB5044, rule (a): the *full* set
  must be read). Fixed arity = max+1.
- **whole-args access** — `@args`, `$(len args)`/`${#args}`/`$#`, or a
  **dynamic** index `${args[$i]}` → **variadic** (floor = highest literal
  index+1 if any).

Checking: fixed → exactly N (SB5042 `<`, SB5043 `>`); variadic → ≥ floor (SB5042).

## Implementation sketch

1. **Signature inference** — a new analysis (replace/extend
   `collect_max_positional`). Walk the body's exprs (`helpers.rs`
   `scan_expr_positionals_*`) recognizing:
   - `$N` → literal index N-1 (note the `$1`≡`${args[0]}` 0-index shift).
   - `${args[LIT]}` → `ExprKind::Subscript { name: "args", index: <int literal> }`
     → literal index.
   - `${args[$var]}` / non-literal index → dynamic → variadic.
   - `@args` splat, `$(len args)`, `${#args}`, `$#` → variadic markers. (No
     `Splat` IR node — `@args` lowers via `current_args_name`; find how it
     appears pre-transform: check `expressions.rs:170-193` and the `@` word
     form in the parser/IR. `$#`/`${#args}` and `$(len args)` are distinct
     surface forms to detect.)
   Return `enum Arity { Fixed(usize), Variadic { floor: usize } }` plus a
   `Vec<usize>` of read indices for the hole check.

2. **Hole check** at `lower_function_stmt` (statements.rs) — if fixed and the
   read-index set ≠ `{0..max}`, push SB5044.

3. **Rewrite `check_callee_arity`** to take the inferred `Arity` and emit SB5042
   (too-few) / SB5043 (too-many, fixed only). Fold the current un-coded message
   into SB5042.

4. **Wire the capture path** — `capture_func_into` (stmt_handlers.rs) now
   receives `arg_values`, so call `check_callee_arity` there too (it already
   runs on inline_function_call + build_function_pipeline_stage).

5. Specs in `spec/functions.sh` (81.x block): too-few, too-many (fixed), a
   variadic `@args` accepting any count, a hole → SB5044, and the same on a
   `$()` call. Mind the janet gating (function-call runtime).

## Watch-items

- `$1`≡`${args[0]}` **0-index shift**: `$N` reads index N-1; a body reading `$2`
  only ⇒ indices {1} ⇒ hole at 0 (rule (a)) ⇒ SB5044. Verify this is the
  intended surface (it is, per the DD).
- `shift` is unimplemented (SB5011) — the `+ shifts` term in the old inference
  is moot; drop it.
- `$@`≡`@args` desugaring is a **DD#75 §5 migration gap** (see DD#81 notes: `$@`
  currently reads empty inside a function while `@args` works). The variadic
  detection should key off the canonical `@args`/`current_args_name` form; the
  `$@` sugar repair is separate but related.
