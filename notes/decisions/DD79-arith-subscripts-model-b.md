title: DD#79 — arithmetic subscripts (model B) + undeclared-read strictness (B2)
status: accepted

1. In arithmetic, a bare integer-array cell `a[EXPR]` is an integer
   rvalue or lvalue with a full arithmetic index. Maps use explicit
   value imports for reads and ordinary keyed assignments for writes.
2. `${...}` imports a value and never denotes an assignable location.
   `$a[K]` desugars to `${a[K]}` even in arithmetic (DD#123), retaining
   DD#80's direct-index grammar rather than bare arithmetic indexing.
3. Missing integer cells read as zero; integer cells carry raw i64
   values. String cells require DD#77's conversion disposition.
4. A name read in arithmetic must have a binding in its lexical scope,
   just as outside arithmetic. Assignment and read-modify-write forms
   can introduce integer bindings; reading alone does not. Valid
   nullary integer-function calls remain calls, not undeclared variables.

---

Two decisions about names inside `(( ))` / `$(( ))`, settled with DG
(2026-08-16 model B; 2026-08-18 Stage 2 + B2). Formalizes the model the
`arith-subscript-values` task implemented across two stages.

## Part 1 — the arithmetic-subscript model (model B)

Inside `(( ))`, a **bare** identifier/subscript uses arithmetic rules
(int-valued); a `${…}` is a normal-parse rvalue that *imports a value*. `${…}`
NEVER denotes a location — it means "the value of" everywhere.

| form                 | in `(( ))`                       | notes |
|----------------------|----------------------------------|-------|
| bare `a[i]`          | int-array cell, **rvalue+lvalue**| index `i` is a full int arith expr; `a[i]++`, `a[i] op= …`, `a[i] = …`, `x = a[i]` |
| bare `m[k]`          | **error** (SB4030)               | a string key isn't expressible under arith rules |
| `${a[i]}`, `${m[k]}` | **rvalue only**                  | `${…}` parsed by NORMAL expansion (map string key works); value-import |
| bare scalar `f`      | rvalue+lvalue                    | `$` optional (`f` ≡ `$f` for reads) |

- The lvalue is always the bare form (`a[i]`) or the existing statement assign
  (`a[i]=`, `m[k]=`). `${…}` as an lvalue is redundant AND breaks the
  value-import invariant → rejected. Map arith-mutation is
  `m[k]=$(( ${m[k]} + 1 ))` (`${m[k]}` reads, `m[k]=` writes, `$(( ))` computes).
- Out-of-range index / absent key reads the DD#53 zero-value (0), uniform with
  an unset variable — **no bounds check** (arrays and maps are sparse
  table-backed; a missing index == a missing key). Decided, not a bug.
- `int` cells read/write the raw i64 (no atoi). ~~A **string**-array cell in arith
  currently atoi's silently; whether it should require a disposition like a bare
  scalar string (DD#77 D1) is a filed follow-up
  (`arith-bare-subscript-followups`).~~ DD#77/DD#123 require the disposition
  in every spelling.

### Implementation

Stage 1 (`${a[i]}` reads) — `ArithExpr::Expansion(Box<Expr>)` value-import via
the shared `expr_to_value`. Stage 2 (bare `a[i]`) — a `BareSubscript` arith-lexer
token (balanced `[…]`, re-parsed as arith), `ArithExpr::Subscript{name,index}`
(rvalue) and `SubscriptAssign{name,index,op,value}` (lvalue → reuses the
statement subscript-assign write path). Bare `m[k]` on an associative array is a
type error (`type_check`, both the read form and the lowered arith-indexed
assignment). native+c; janet's arith VALUE path is a filed gap
(`janet-arith-subscript-read`).

## Part 2 — B2: undeclared reads are strict in arithmetic (SB5031 consistency)

Previously an undefined bare name **read** 0 in arithmetic (`$(( y+1 ))` → 1),
while outside arithmetic `$y` is a typo error (SB5031, "assigned nowhere"). That
seam surprised: `(( a[i]++ ))` accepted an undefined index `i` as 0, yet the
later `$i` was rejected — same name, opposite treatment.

**Decision (DG, 2026-08-18):** extend SB5031 into arithmetic. A bare name
**read** in `(( ))` that is *assigned nowhere* is a typo error, exactly like a
read outside arithmetic. "Assigned nowhere" is ~~program-wide~~ and already counts
arithmetic assignments — `(( f++ ))`, `(( g = … ))`, and the `for (( i=0; … ))`
init all *assign* their name, so it is in scope and reading it is fine ("anything
you assign exists"). Only a pure read of an unassigned name errors:

DD#86 resolves this against each lexical binding; one body's private
assignment does not declare a name in another body.

- `$(( y + 1 ))` with undeclared `y` → SB5031.
- the index `i` in `(( a[i]++ ))` with undeclared `i` → SB5031 (declare `int
  i=0`).
- `(( f++ ))`, `for (( i=0; i<n; i++ ))`, `int i=1; $(( i+1 ))` → fine.

Chosen over the alternatives: **auto-vivify on read** (referencing creates the
name) masks index/operand typos, contradicting SB5031's purpose; **status quo**
(silent 0) keeps the surprising seam. B2 is the strictness sash applies
everywhere else (SB5031/SB5018/DD#77).

DD#58 nullary int-function calls (`$((f))` where `f` is a function) are valid
references, so function names are exempt from the B2 read check.

### Implementation

`validate::check_undeclared_reads` now descends `ExprKind::Arithmetic`
(`check_arith_reads`): a bare `Var`/`DollarVar` read, a bare-subscript array
name, and every subscript index / operand are checked against the in-scope set
(scope names + function names). A janet gap for an undeclared scalar counter
(`(( f++ ))`) is filed (`janet-undeclared-arith-counter`).

## See also

- DD#53 — zero-value (unset/out-of-range reads 0).
- DD#58 — arithmetic sexpr / nullary int-function calls.
- DD#76 — int-valued collections (the int cell read/write raw i64).
- DD#77 — arith string→int discipline (a bare scalar string in arith needs a
  disposition; the string-array-cell analogue is now implemented — bare string
  cell → SB4030, `${a[i]:-N}`/`${a[i]:?msg}` disposition parity with a scalar).
- notes/tasks/arith-subscript-values.md — the two-stage implementation tracker.
