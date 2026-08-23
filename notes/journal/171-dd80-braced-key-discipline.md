# 171 — DD#80 braced subscript key/index discipline (`${a[K]}`)

Implements DD#80 (companion to DD#79): `${a[K]}` / `${m[K]}` take a **direct,
unambiguous** key — no *implicit* arithmetic. Computation uses an explicitly
opened nested context. Fixes `bug-braced-nested-subscript-index` (`${a[a[0]]}`
silently read the inner cell) and the wider silent-`0` footgun (`${a[foo]}`,
bare `${a[i]}`), in both word and arith contexts.

## The model (spec/subscript.sh 80.1–80.19, DD#80)

`K` is one of: integer literal · `$var` · an expansion (`${…}`/`$(…)`/`$(( … ))`)
· a quoted string · a clean-identifier bareword (maps). Implicit/ambiguous bare
forms reject (**SB4041**); a string key on an int-indexed array rejects
(**SB4030**, the existing type-check string→int discipline). The line with
DD#79: **bare in `(( ))` = arithmetic; `${ }` = direct key.**

| bare (implicit) → reject | explicit (nested) → ok |
|---|---|
| `${a[a[0]]}` (SB4041) | `${a[${a[0]}]}`, `${a[$(( a[0] ))]}` |
| `${a[i+1]}` (SB4041) | `${a[$(( i+1 ))]}` |
| `${a[$b[0]]}` (SB4041) | `${a[${b[0]}]}` |
| bare `${a[i]}`, `${a[foo]}`, `${a["5"]}` (SB4030 on int array) | `${a[$i]}`, `${a[5]}` |
| `${m[a[0]]}` (SB4041) | `${m["a[0]"]}`, `${m[${a[0]}]}` |

## The unrolling invariant did the heavy lifting

Per DG's steer, nesting is equivalent to sequential temporaries and the
transform already realizes it. The whole enabling change was on the **parse**
side; lowering was untouched:

- **`parse/expr.rs`** — `try_subscript` no longer splits on the first `]`
  (`content[..].find(']')`). `capture_balanced_subscript` captures the index
  balanced across `[] {} ()` and skipping quotes, so a `]` inside a nested
  `${…}`/`$(( … ))`/quote doesn't close early. `parse_subscript_index` re-parses
  the captured index through the existing `parse_expr`, so an explicit nested
  context becomes a normal expansion node that lowers to SSA temporaries like
  any other operand. `${a[${a[0]}]}` (was SB4035) now works; `$(( (getpid)+1 ))`
  and quoted keys nest for free.
- A bare dollar-subscript `$name[…]` (which parses as a clean cell read) is kept
  a `Literal` by `is_bare_dollar_subscript` so it rejects — the explicit
  cell-read index form is the braced `${name[…]}`.

## The only new logic: surface classification

- **`validate.rs::check_subscript_index`** (SB4041, run in `run_structural_checks`)
  — walks `Subscript` exprs and cell-`ParameterExpansion` indices; an index that
  is an explicit expansion or a quoted string is fine, a `Literal` is fine only
  if it is an integer literal or a clean identifier, and anything else (a
  `Literal` with `[`/operators, or a `Concat` of mixed parts) is ambiguous.
  Because it walks all exprs, it catches the arith-context `$(( ${a[a[0]]} ))`
  too.
- **`type_check.rs`** — SB4030 reused: the "string key on indexed array" check
  already lived here (why bare `${a[i]}` reported SB4030). Extended `is_string_key`
  to quoted keys (`${a["5"]}`) and sharpened the message. A distinct SB4042 was
  considered but deferred — it would need a per-error code on all 21 `TypeError`
  sites.

## DD#33 alignment

A bare integer key on a map (`${m[5]}`) is already SB4033/SB4030 per DD#33 — my
initial DD#80 table wrongly allowed it. Corrected: an integer literal indexes an
array; a map reads a numeric key quoted (`${m["5"]}`). Spec 80.16.

## Verification

Full `SPEC_MODES="native c janet"` sweep, latest-run-per-test diff vs HEAD
(`5fc9e05`): **0 passing→failing, 0 gone, 51 new passing**; 57 subscript specs
(19×3) green; native gate 0 failing. clippy/fmt clean. Arith-context asx.1/asx.2
intact. Filed `converter-braced-subscript-index` (bash arithmetic subscripts →
sash explicit forms — the converter has no subscript handling today, nothing
regressed).
