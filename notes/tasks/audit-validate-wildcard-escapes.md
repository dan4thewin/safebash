title: audit: sweep validate.rs for wildcard arms that silently skip checks
status: next
priority: P1
kind: audit
depends: []

The same defect shape has now produced five separate escapes in the taint
pass alone, four of them user-reachable and one of them a shipped taint
bypass (journal 400):

| site | variant that fell through | effect |
|---|---|---|
| `check_expr_taint` | `ExprKind::Test` | `[[ ]]` operands unchecked |
| `check_expr_taint` | `ExprKind::SlurpFile` | `[[ $(<f) = x ]]` silently legal |
| `check_expr_taint` | `ExprKind::Arithmetic` | **taint bypass** — `echo "$(( ${x:-0} ))"` |
| `taint_stmt` | `StmtKind::ArithTest` | `if (( … ))` condition unchecked |
| `reject_scalar_rhs_read` | `ExprKind::Arithmetic` | RHS copy not rejected; target tainted, then attest-able |

`check_expr_taint` and `reject_scalar_rhs_read` are now total over
`ExprKind` and cannot regress this way again. Nothing has audited the rest.

The task: enumerate every `match` in `validate.rs` (and the other
per-statement passes) whose wildcard arm is a **silent no-op** over an AST
enum, and classify each one — "no reads by construction" or "unclassified".
A wildcard that returns a *diagnostic* is fine; a wildcard that returns
`{}` on a node type that can contain reads is the bug shape.

Two known ones deliberately left alone by journal 400, both needing a
verdict rather than a reflex:

- `ir::walk_arith_expansion_roots`'s `_ => {}`. Its remaining variants
  (`Literal`, `Var`, `DollarVar`, …) genuinely have no `Expr` children, but
  it is shared by several passes, so making it total is a wider change than
  that journal's scope.
- Bare `ArithExpr::Var`/`DollarVar` taint. Unreachable today — a *string*
  name in arithmetic is SB4030 (DD#77 wants a disposition) and an *int* name
  cannot be tainted once journal 400's fix is in (every construction route is
  SB4030, SB5020, or an array via DD#99 `read`). `check_arith_taint` names
  itself in a comment as the place that needs the arm if that ever changes.
  Worth a spec that pins the SB4030 shield rather than relying on the note.

Method: this is exactly the "validator bypass testing is the highest
priority" item from the run-1 audit. Prefer probing behaviour over reading
call sites — Bug A in journal 400 was invisible from the code and obvious
from one execution.
