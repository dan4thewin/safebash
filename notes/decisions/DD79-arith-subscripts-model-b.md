title: DD#79 — arithmetic subscripts (model B) + undeclared-read strictness (B2)
status: accepted

1. In arithmetic, a bare integer-array cell `a[EXPR]` is an integer
   rvalue or lvalue with a full arithmetic index. A bare integer-map cell
   uses a wholly quoted string key, such as `m["$prefix:foo"]`, for either
   role. Quoting the key does not convert the cell's value.
2. `${...}` imports a value and never denotes an assignable location.
   `$a[K]` desugars to `${a[K]}` even in arithmetic (DD#123), retaining
   DD#80's ordinary operand-word key grammar rather than bare arithmetic
   indexing. Partial quoting does not select a string key in a bare cell.
3. Missing integer cells read as zero; integer cells carry raw i64
   values. String cells require DD#77's conversion disposition.
4. A name read in arithmetic must have a binding in its lexical scope,
   just as outside arithmetic. Assignment and read-modify-write forms
   can introduce integer bindings; reading alone does not. Valid
   nullary integer-function calls remain calls, not undeclared variables.
5. An indexed write evaluates and retains its key once, before its RHS.
   A compound update also retains the old cell value before the RHS runs,
   following DD#86's evaluation-order rule.

---

Two decisions about names inside `(( ))` / `$(( ))`, settled with DG
(2026-08-16 model B; 2026-08-18 Stage 2 + B2). Formalizes the model the
`arith-subscript-values` task implemented across two stages.

## Part 1 — the arithmetic-subscript model (model B)

Inside `(( ))`, a **bare** identifier/subscript uses arithmetic rules
(int-valued); a `${…}` is a normal-parse rvalue that *imports a value*. `${…}`
NEVER denotes a location — it means "the value of" everywhere.
DG extended the bare-cell form on 2026-11-24: wholly quoting the bracket
contents selects an ordinary string key, not arithmetic. Single, double and
C quotes work; interpolation follows the chosen quoting form.

| form                 | in `(( ))`                       | notes |
|----------------------|----------------------------------|-------|
| bare `a[i]`          | int-array cell, **rvalue+lvalue**| index `i` is a full int arith expr; `a[i]++`, `a[i] op= …`, `a[i] = …`, `x = a[i]` |
| bare `m["K"]`        | int-map cell, **rvalue+lvalue**  | the entire key is one quoted word; interpolation is allowed |
| bare `m[k]`          | **error** (SB4030)               | an arithmetic key is not implicitly stringified |
| `${a[i]}`, `${m[k]}` | **rvalue only**                  | `${…}` parsed by NORMAL expansion (map string key works); value-import |
| bare scalar `f`      | rvalue+lvalue                    | `$` optional (`f` ≡ `$f` for reads) |

- The lvalue is always the bare form (`a[i]`, `m["K"]`) or the statement assign
  (`a[i]=`, `m[k]=`). `${…}` as an lvalue is redundant AND breaks the
  value-import invariant → rejected. `(( m["k"]++ ))` and
  `(( m["$key"] += 2 ))` update integer-map cells. Ordinary keyed writes
  remain available; their keys use DD#80's operand-word grammar.
- Out-of-range index / absent key reads the DD#53 zero-value (0), uniform with
  an unset variable — **no bounds check** (arrays and maps are sparse
  table-backed; a missing index == a missing key). Decided, not a bug.
- `int` cells read/write the raw i64 (no atoi). ~~A **string**-array cell in arith
  currently atoi's silently; whether it should require a disposition like a bare
  scalar string (DD#77 D1) is a filed follow-up
  (`arith-bare-subscript-followups`).~~ DD#77/DD#123 require the disposition
  in every spelling.

### Implementation

`${a[i]}` reads use `ArithExpr::Expansion(Box<Expr>)` through the normal
parameter parser and shared `expr_to_value`. A `BareSubscript` arith-lexer
token retains balanced `[...]` source. Its key is re-parsed as arithmetic,
or as an ordinary word when wholly quoted. `ArithExpr::Subscript{name,index}`
(rvalue) and `SubscriptAssign{name,index,op,value}` (lvalue → reuses the
statement subscript-assign write path) preserve that distinction.
Both backends consume the same checked SSA. Quoted tokens retain raw source
so interpolation and C escapes are not lost or decoded twice. The old
in-tree Janet backend is retired by DD104.

### Indexed updates (2026-11-13)

The implementation retains the compound operator until SSA lowering.
Expanding `a[index] op= rhs` to `a[index] = a[index] op rhs` duplicated
effects in `index`; merely reusing an unevaluated expression also let RHS
effects change the selected key or old value. Lowering now saves the key,
then the old cell value for an update, before evaluating the RHS.

Ordinary indexed `+=` uses the same path for integer addition and string
concatenation. Plain indexed `=` saves the key but does not read the old
cell. The compiler preserves the implicit read in semantic checking.
The same once-only ordering applies to the wholly quoted arithmetic map
keys accepted on 2026-11-24. [DD104](DD104-decoupled-janet-oracle.md)
records the corresponding inert-IR schema change.

Arithmetic `&&`, `||` and `?:` evaluate only their selected operands.
Lowering keeps calls and checked keys inside their owning branches and saves
an earlier operand before lifting later effects. Internal branch decisions
do not manufacture a source-visible command status. An integer-cell `:-`
evaluates its subject once and selects its literal fallback on zero.

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
