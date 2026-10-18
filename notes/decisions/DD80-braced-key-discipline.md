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

> **Superseded in part, 2026-09-26.** The lists below enumerate accepted
> *spellings*; the rule is now decided by **type** and applies to `a[K]=v` as
> well as `${a[K]}`. See the amendment at the end of this file. The intent is
> unchanged — the mechanism and the edge cases are not.

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

## Amendment (2026-09-26) — the key is a *type*, not a spelling; and both surfaces enforce it

Two changes, implemented in `761b3f4` and `0e23874`. The original rule was
written as a list of accepted **spellings** and was enforced on the read
surface only. Both of those turned out to be the wrong shape.

### 1. The assignment LHS accepts the same index grammar as a read

DD#80 was written about `${a[K]}` and said nothing about `a[K]=v`. Measured
**before either change** (both columns are the old state), the write surface
was strictly narrower — it took a bare `$var` and nothing else:

| index | `${a[K]}` (then) | `a[K]=v` (then) |
|---|---|---|
| `$(( i+1 ))` | ok | **SB5011** (`a[` became a command name) |
| `${s:-2}` | ok | **SB4014** |
| `"$i"` | ok | **SB4014** |

So `$(( … ))` — *the conversion this DD names as the remedy for a string
index* — could not be written on the left of an `=`. An author told to convert
had nowhere to put the conversion, and the two failures did not even agree on
a diagnostic.

`parse_split_subscript_index` now recovers the raw index text from the token
spans and hands it to **`expr::parse_subscript_index`, the same function the
read path calls**. There is one list of accepted index forms and both surfaces
call it, so the parity is structural rather than two lists kept in step by
hand. **Every rule in this DD now reads "in `${a[K]}` and in `a[K]=v` alike."**

Parity is the point, not permissiveness: it is what makes section 2 below
enforceable on the write side at all, and section 2 then *narrows* the read
column too — `${a[${s:-2}]}` on a string `s` is now SB4030 on **both**
surfaces. What survives on both is the accountable form,
`a[$(( ${s:?msg} ))]=Q`, which aborts at runtime with the author's own message
when the value is not an integer.

### 2. "must yield a number" is decided by type, not by spelling

The type-aware section above enumerates spellings — "an **integer literal**, a
**`$var`**, or an **expansion** (which must yield a number)". A `$var` is not
inherently a number and an expansion does not inherently yield one, so the
list both over- and under-accepts. It is replaced by a single question:

> **An indexed array's index must be int-typed. A map's key must not be.**

"Int-typed" means an integer literal, or an expression `expr_type` types as
`Int`. That one delegation inherits every typing rule already written
elsewhere, rather than restating any of them:

- `$(( … ))` is `Int` — the conversion this DD names, still the remedy.
- a cell of an `int array` is `Int` (DD#76) — so the nested `${a[${a[0]}]}`
  blessed in the table above keeps working, **and a cell of a *string* array
  does not**. Same spelling, different element type, different answer. An
  enumerated whitelist lost this case.
- a `:-` / `:?` disposition keeps its **subject's** type — so
  `${a[${i:-0}]}` on an `int i` indexes, while `${a[${s:-2}]}` on a string
  `s` does not. **A bare disposition is not the conversion**; that is the whole
  reason `$(( … ))` is required, and it now falls out instead of being listed.

Only the bare integer literal is stated separately, because `expr_type` types
`2` as String (bash does, and nothing else in the language wants a bare
literal to be an int by itself).

A bare literal array index must also fit the runtime's signed 64-bit domain:
unsigned decimal `0` through `9223372036854775807`. A longer sequence of
digits is not a valid index merely because it looks numeric. The map-side
spelling rule is separate: a digit-only key still needs quotes even when it
exceeds that range. Explicit arithmetic and int-typed indices retain their
existing rules. Coverage 526 found the missing range check reaching a
codegen invariant through `args`; `types.sh` `lit64.1`-`lit64.15` pin the
domain and the related scalar/int-collection literal checks.

The predicate this replaces asked the **negative** question — "is this a
*string* key?" — and could only name forms it already knew were strings,
defaulting everything else to accepted. That is not a fixable list:
`TypeEnv::var_type` ends in `unwrap_or(String)`, so **"declared a string" and
"never recorded at all" are the same answer** and no negative test can
separate them. The visible symptom was that an undeclared `i=1` indexed an
array while `str s=abc` did not, though neither is int-typed. Asking for `Int`
is a fact rather than an absence, so the ambiguity stops mattering.

### The map side, likewise

"Any string is a valid key" stands, and so does the DD#33 integer-literal
rule. What is added: **an int-*typed* key is rejected too, not merely an
integer literal.** `${m[$i]}` on an `int i`, `${m[$((1+1))]}`, and a cell of
an `int array` are all the same crossing — int→string, which DD#105 requires
to be written rather than inferred. Narrowing the rule to bare variables would
have left `${m[$((1+1))]}` crossing in silence while `${m[$i]}` was told to
quote.

**Quoting is the conversion**, and it compiles for every one of them on both
surfaces — `m["$i"]`, `m["$(( 1+1 ))"]`, `m["${n[0]}"]` — so the rule leaves
no form without a way to say what it means. This is the same convention as a
redirect target and `$(strlen "$n")`.

### Consequence: an ICE closed by construction

`${a[${s:-hello}]}` used to panic at `codegen/values.rs:1432`. The braced
subscript was the only int sink lacking a DD#77 D4 default-literal check.
Rejecting every bare disposition on a string subject makes that sink
unreachable, so no widening of `check_int_value_defaults` is owed.

### Specs

`spec/subscript.sh` 80.51–80.66 (LHS parity, the type rules, and both
remedies); the accept-side specs 80.60/80.62/80.63/80.65 are deliberate
regression guards proving the recommended remedy actually compiles — a rule
that rejects a form without a working alternative is worse than no rule.
Journals 413 and 414.

## Addendum (2026-10-03) — a capture is not an index, in every position

DG, on the last position where this DD was not enforced:

> I don't think `arr[$(f)]` is legal, but `arr[$((f))]` where `f` is an int
> returning fn seems fine.

This changes nothing in the rule above — it is section 2 applied. A `$( … )`
types as String, so `expr_type` already answered "not `Int`" and
`is_int_index` already returned false. The capture-as-index was accepted in
exactly one place, and for a reason that had nothing to do with indices:
**an assignment's and a declaration's VALUE were never walked by the subscript
rules.** `echo "${arr[$(f)]}"` was SB4030 throughout; `str z=${arr[$(f)]}`
was accepted and printed `q`. Walking the value makes the three positions
agree, and incidentally closes an ICE — `str v="${a[foo]}"` reached int
lowering and panicked with *"type_check (SB4030) should have rejected it"*,
the compiler naming the pass that never ran.

Worth recording because it is the second time this DD's enforcement has been
found to be **position-dependent rather than rule-dependent** (the first was
the write surface, in the 2026-09-26 amendment, fixed by making both surfaces
call one parser). The rule was never in doubt either time. What was missing
was a walk.

### Why the capture gets its own diagnostic

The generic index message advises "use an integer literal, an `int` variable,
or `$(( … ))`". For a capture that advice is a dead end: `${a[$(( $(f) ))]}`
is **SB4041** (a capture is not an arithmetic operand either) and `int i=$(f)`
is **SB4030**. An author holding a string-returning function has exactly one
migration — declare it `fn f -> int` — so the message names that form
directly, per `array_index_error`'s own standing rule that a diagnostic must
not send its reader from one SB4030 to the same SB4030.

## Amendment (2026-10-03) — one grammar, three spellings

> **DG:** I thought braceless array access was supported...
>
> **DG:** all paths must agree.

It was supported — but not with *this* grammar. A subscript can be written
three ways, and each was a separate parse path with its own idea of what `K`
may be:

| spelling | path | index grammar it accepted |
|---|---|---|
| `${a[K]}` | `try_subscript` (byte-level, balanced) | the full rule above |
| `$a[K]` | `try_subscript_arm` (token-level) | a fixed shape: `[`, `$ident`, `]` |
| `"$a[K]"` | `try_emit_array_subscript_in_string` | an integer literal, nothing else |

So `echo $a[$i]` printed an element while `echo "$a[$i]"` was **SB5064**, and
`$a[$(( i ))]` was SB5064 where the braced form computed the index. The
narrow paths did not *reject* a richer index — they declined to see a
subscript at all, leaving a bare `$a`, which is why the complaint that
surfaced was "you read the collection with the scalar sigil."

**The resolution is a desugar, not a third implementation.** The lexer now
rewrites `$a[K]` to `${a[K]}` (and the in-string form likewise) before
anything downstream sees it, so this DD's grammar is the *only* subscript
grammar in the compiler and the spellings cannot drift apart again. This is
the mechanism DD#90 already uses for `$x:-foo`, and it is the project's
standing "never fork a shared walker" convention applied to a parser.

### What the divergence was hiding

Not a style difference — **five ICEs and a hole in a safety diagnostic.**

`$#[0]`, `$?[0]`, `$$[0]`, `$![0]` and `$1[0]` each built a subscript on a
collection named `#`/`?`/`$`/`!`/`1`. No pass rejected it and codegen panicked
with *"ICE: undefined array"*. Every braced counterpart was already a clean
diagnostic (SB4038 / SB4034 / SB4047 / SB4003 / SB4047); the brace-less path
simply never reached one. SB5031 — `$@[0]` isn't subscriptable — was a
point-fix for the `@`/`*` members of this same family, and its own doc
describes the crash.

Worse, SB5068 (`s` is a scalar, so `s[…]` has no element) exists because
`${s[0]}` once **segfaulted**. The brace-less spelling only reached that fix
for the index forms its gather understood: `str s=z; echo $s[$(( 0 ))]`
compiled and printed `z[0]`. A safety diagnostic that one spelling can walk
around is not a fix, and that is the argument for agreement over codifying
the narrowing.

Pinned by `spec/subscript.sh` sections N and O (`80.70`–`80.96`), which spec
each index form in the two spellings that used to diverge, plus the five
closed ICEs.

### One decline remains, and it is the right one

`$arr[foo` — brackets that never close. No subscript is recognised, `$arr`
stands alone, and SB5064 is accurate (`80.33`). Bash would print `p[foo`.

### Carve-out: inside `(( ))` the desugar does not fire

`$(( $a[1] ))` stays **SB4035** by design (DD#79 model B: inside arithmetic
the subscript spelling is the bare `a[i]`). `collect_arith_content`
re-serialises the main lexer's tokens through `tok_to_text`, so an un-gated
desugar would have flipped that deliberate rejection to an accept silently.
The desugar is gated on `Scanner::arith_depth == 0`.

## Amendment 2026-10-04 — the rule is the index's, not the position's

The desugar above made the three *read* spellings agree. It did nothing for
the **write** side, and the write side was enforcing nothing at all: every
form section A of `spec/subscript.sh` rejects when read was accepted when
written.

| | read | write (before) |
|---|---|---|
| `m[ a b ]` | SB4041 | accepted, key `a b` |
| `m[ $s x ]` | SB4041 | accepted, key **literal `$s x`** — `$s` never expanded |
| `m[a-b]` | SB4041 | accepted |
| `m[a.b]` | SB4041 | accepted |

The cause is a shape, not an oversight in the rule: an assignment's LHS index
lives in the `Assignment.index: Option<Expr>` struct field as a **bare
`Expr`**, never wrapped in an `ExprKind::Subscript`. `flag_subscript_index`
keys on that wrapper, so the expression walk *reached* the index and could
not tell it was one — it saw a `Literal` like any other. It takes two
visitors to state one rule, because there are two IR shapes; `validate.rs`
now has `flag_assignment_index` beside it, both calling the same
`ambiguous_index_diag`.

**This is the third time DD#80's enforcement has proved position-dependent
rather than rule-dependent** — after the write surface (2026-09-26) and the
assignment/declaration *value*, which was never walked at all (journal 462).
The pattern is the standing one: a guard keyed on syntax rather than on the
thing being guarded. When a rule is added here, the question to ask is not
"does the check fire on `${a[K]}`" but "what are all the IR shapes an index
can inhabit".

SB4041's wording lost the word "braced" and its `${ }` reference with this
change; it is one rule for all positions, so it no longer names one.

### What the gap was hiding

A silent miscompile, not just a missing error. `m[$s ] x ]=9` **compiled
clean and discarded source**: it assigned the literal string `]=9` to key
`zz`, with ` x ` dropped without a word.

Two coupled parser defects, neither harmful alone. The shape probe
(`looks_like_dynamic_subscript_assign`) searched for a `]` *followed by*
`=`/`+=`; the consumer (`lhs_index_close`) searched for the *first* `]`. Two
searches, two predicates, one bracket. And `parse_assignment` then did
`scanner.next(); // consume =` **unconditionally**, consuming whatever token
stood there as if it were the operator. On a map — where a string key makes
the extra text legal to the lexer — the first `]` closed the index, ` x ` was
eaten as the `=`, and `]=9` became the value.

Both halves are closed: `lhs_index_close_from(scanner, base)` is now the one
search with the one predicate (the callers differ only by whether `name[` is
still unconsumed), and the operator is consumed by an explicit `match` that
returns SB4014 on anything else. That arm is unreachable while the probe and
the parser agree, which is now structural rather than a coincidence of two
hand-matched lookahead windows — it is checked instead of assumed precisely
because the previous version assumed and was wrong.

Pinned by `spec/subscript.sh` sections P (`80.97`–`80.106`, the rejections
and their read twins) and Q (`80.107`–`80.113`, the legal write forms that
must not move).
