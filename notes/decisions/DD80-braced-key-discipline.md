title: DD#80 — braced subscript key/index discipline (`${a[K]}` is direct, not arithmetic)
status: accepted

Companion to DD#79 (arithmetic subscripts, model B). DD#79 governs a **bare**
subscript inside `(( ))` (arithmetic). This DD governs the **braced** form
`${a[K]}` / `${m[K]}` in every context: what a valid key/index is, and why it
is *not* interpreted by arithmetic rules.

Settled with DG (2026-08-22).

## The principle

`${…}` is the string-key / direct-access construct (it is how a map is read by
a string key). Its index/key is therefore a **direct, unambiguous** value —
never *implicitly* computed. Arithmetic and other computation are available,
but only through an **explicitly opened** nested context; there is no implicit
arithmetic inside `${ … }`.

The distinction is implicit-vs-explicit, not "no computation":

| bare (implicit) → **reject** | explicit (nested context) → **ok** |
|------------------------------|------------------------------------|
| `${a[i]}`                     | `${a[$i]}`                          |
| `${a[i+1]}`                  | `${a[$(( i+1 ))]}`                  |
| `${a[a[0]]}`                 | `${a[${a[0]}]}` / `${a[$(( a[0] ))]}` |
| `${m[a[0]]}`                 | `${m["a[0]"]}` (literal) / `${m[${a[0]}]}` (computed) |

## What a valid `${collection[K]}` key/index is

`K` is exactly one of:

1. **integer literal** — `${a[5]}` (array index). On a **map** a bare integer
   read is rejected (DD#33: it reads like an array index) — quote it: `${m["5"]}`.
2. **`$var` / `${var}`** — a dynamic index/key (the value at runtime).
3. **an expansion** — `${…}`, `$(…)`, `$(( … ))`. A value produced by an
   explicitly-opened nested context. This is the sanctioned way to put a
   *computed* index inside `${ }`.
4. **a quoted string** — `"…"` / `'…'`. An explicit literal key; a double-quoted
   string interpolates (`${m["p$s"]}` builds a key), so nested expansions are
   allowed inside it.
5. **a clean bareword** — `[A-Za-z_][A-Za-z0-9_]*` only. A literal string key
   (maps). An identifier with no `$`, no `[`/`]`, no operators, no whitespace.

Anything else — a bareword carrying `[`/`]`, arithmetic operators, an embedded
`$`, or whitespace — is an **implicit/ambiguous** form and is rejected
(**SB4041**). The fix is always to make it explicit: quote a literal
(`"a[0]"`), interpolate a var (`$i`), or open arithmetic (`$(( … ))`).

### Type-aware (array vs map)

Resolved at type-check, where the collection type is known:

- **int-indexed array** `${a[K]}` — `K` must be an **integer literal**, a
  **`$var`**, or an **expansion** (which must yield a number). A clean bareword
  or a quoted string is a *string* key and does not index an int array →
  **SB4030** (the existing type-check "string key on indexed array" — the same
  string→int discipline as DD#77; type_check emits a uniform SB4030). A quoted
  numeric (`${a["5"]}`) is likewise rejected — use the unquoted integer
  `${a[5]}`.
- **map** `${m[K]}` — `K` may be a clean bareword, a quoted string, a `$var`,
  or an expansion (string key building). Any string is a valid key.

## The unrolling invariant (governing implementation)

Any nested key/index is **semantically equivalent to its unrolled sequential
form** — a series of temporary assignments, one per nested context. The
compiler realizes this for free: AST→SSA lowering (`transform`) already lowers a
subscript index with the same recursive `expr_to_value` it uses for every other
operand, so each nested `${…}`/`$(…)`/`$(( … ))`/quoted-Concat becomes its own
SSA temporary. Example — the (deliberately extreme)

```
(( 5 > ${outermap[${innermap[":$s$(( (getpid) + 1)):"]}]} ))
```

unrolls to `t0=getpid(); t1=t0+1; t2=":"++$s++str(t1)++":"; t3=innermap[t2];
t4=outermap[t3]; 5 > t4`.

Consequences:
- **No nesting-depth cap.** Nesting falls out of recursive descent; a limit
  would be *added* logic to forbid what the pipeline handles uniformly.
- **No construct "needs disproportionate plumbing."** If one appears to, that
  is an implementation smell to **investigate** (a coupling that shouldn't
  exist), not an exception to carve out. Held to the bug policy.
- The only genuinely new logic is the **surface classification** (rule above):
  reject implicit/ambiguous bare `K` (SB4041) and type-check string-key-vs-int
  (SB4030). The lowering is not new.

## New diagnostics

- **SB4041** (parse, new) — an implicit/ambiguous braced index/key. Directs to
  the explicit form (quote / `$var` / `$(( ))`).
- **SB4030** (type-check, reused) — a string key (clean bareword, quoted, or a
  non-numeric expansion result) used to index an **int** array. type_check
  emits a uniform SB4030; the "string key on indexed array" check already lived
  there (it is why bare `${a[i]}` reported SB4030). This DD extends it to quoted
  keys and sharpens the message. (A distinct SB4042 was considered but deferred:
  it would require adding a per-error code to every `TypeError` site.)

## Migration

Behavior change: `${a[i]}` (bare non-numeric on an int array) and exotic
unquoted map keys previously "worked" (silently coercing to `0`, or mis-parsing
on the first `]`). The full 3-mode spec sweep showed **0 regressions** — existing
specs already used numeric / `$var` forms. `sash-convert` has no subscript
handling today (it passes them through) and no convert.sh coverage, so nothing
broke; mapping a bash arithmetic subscript `${a[i]}` → `${a[$i]}` / `$(( … ))`
is filed as a follow-up (`converter-braced-subscript-index`).

## See also

- DD#33 — subscript key typing (integer-key-on-map / string-key-on-array). DD#80
  defers to it: an integer read on a map is quoted (`["5"]`).
- DD#79 — bare arithmetic subscripts (model B). The two together fully specify
  sash subscripts: **bare in `(( ))` = arithmetic; `${ }` = direct key.**
- DD#77 — arith string→int discipline (a string cell/scalar in arith needs a
  disposition). A string *key* for an int-array index is the same crossing,
  here rejected at the braced surface (SB4030) rather than coerced.
- notes/tasks/bug-braced-nested-subscript-index.md — the bug that surfaced this
  (superseded by this DD's rule: `${a[a[0]]}` rejects, `${a[${a[0]}]}` works).
