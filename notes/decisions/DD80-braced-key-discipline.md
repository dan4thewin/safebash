title: DD#80 — operand-word collection keys (`${a[K]}` is direct, not arithmetic)
status: accepted

1. A direct subscript key is one ordinary operand word. Literal punctuation,
   adjacent quoted/unquoted fragments, interpolation and nested expansions
   follow normal word rules. Brackets delimit the key; they do not enable
   implicit arithmetic or implicit variable/cell reads.
2. Array indices must be integer-typed; a bare decimal index must be in
   `0..9223372036854775807`. Map keys must be strings; quote digit-only
   keys and integer values. A string disposition alone does not convert
   it to an integer index.
3. Reads, assignments and `unset NAME[K]` share the key grammar and type checks.
   `$a[K]` and its quoted spelling desugar to `${a[K]}`, including in
   arithmetic (DD#123); bare arithmetic subscripts retain DD#79's
   separate rvalue/lvalue grammar: arithmetic array indices, wholly quoted
   string map keys. Quoting the key does not convert the cell value.
4. Nested explicit contexts obey sequential value evaluation without an
   arbitrary nesting cap, retaining earlier values in DD#86 order.
   A nested expression's parse failure reports its own error. An empty
   unquoted key or more than one word is SB4041; quote spaces or an empty key.
5. `unset` names its collection statically; only the key/index is evaluated.
   Evaluate and retain all keys once, left to right, before applying any
   deletion or whole-collection clear. A key-producing helper may mutate the
   collection; deletion then acts on its current contents. Missing entries
   are a no-op. Mutation has no result. Element deletion does not resolve
   collection taint; whole clearing retains DD128's existing failed-attest
   replacement rule.
   Scratch, immutable arguments, readonly ERR imports and collection
   non-aliasing retain their ordinary restrictions.

---

Companion to DD#79 (arithmetic subscripts, model B). DD#79 governs a **bare**
subscript inside `(( ))` (arithmetic). This DD governs the **braced** form
`${a[K]}` / `${m[K]}` in every context: what a valid key/index is, and why it
is *not* interpreted by arithmetic rules.

DG requested runtime-key/index `unset` on 2026-11-22. Its target uses this
same direct-key grammar, including nested explicit expressions; a quoted
whole target or a runtime collection name is still invalid. Target storage
identity is not a read of its contents, but evaluating the key is an ordinary
checked read. This extends the earlier literal-only `unset` implementation,
not the scope of arithmetic subscripts.

Originally settled with DG on 2026-08-22; DG replaced the special key-shape
whitelist with ordinary operand words on 2026-11-24. This does not change
the separate grammar of `[KEY]=VALUE` entries in map literals or add dynamic
initializer keys.

## The principle

The contents of a direct cell's brackets are an **operand word**, just as in
an ordinary value position. `${m[$prefix:foo]}` and `$m[$prefix:foo]` build the
same key as the word `$prefix:foo`; enclosing the whole word in quotes is
optional unless its contents require them. Bare text remains text:
`${m[i+1]}` selects the literal key `i+1`, not a sum.

Arithmetic and variable/cell reads require their ordinary explicit syntax.
Only a **bare cell in arithmetic** has DD79's arithmetic-or-wholly-quoted
key distinction; dollar/braced value imports keep this DD's word grammar
even inside `$(( ... ))`.

| word | meaning |
|---|---|
| `${m[$prefix:foo]}` | interpolate a string key |
| `${m[a[0]]}` | literal key `a[0]` |
| `${m[$a[0]]}` / `${m[${a[0]}]}` | use the explicitly read cell value as a key |
| `${a[i+1]}` | string key on an array: SB4030 |
| `${a[$(( i+1 ))]}` | explicitly computed integer index |
| `${m["two words"]}` / `${m[""]}` | a key containing space / an empty key |

## What a valid `${collection[K]}` key/index is

`K` must parse completely as one ordinary operand word. Surrounding whitespace
does not become key data. Adjacent fragments concatenate; whitespace between
unquoted fragments instead supplies a second word and is SB4041.

Single, double and C quotes retain their ordinary meanings. Nested `${...}`,
`$name[...]`, `$(...)` and `$((...))` retain their own grammar and checks.
Punctuation such as `a-b`, `a.b`, `a|b` and `(a+b)` is literal word data, not
an operator evaluated by the brackets. A malformed nested expression is not
recovered as literal key text.

### Type-aware (array vs map)

Resolved at type-check, where the collection type is known:

- **int-indexed array** `${a[K]}` — `K` must have integer type, except for
  the existing bare decimal-literal index spelling. A bare text word
  or a quoted string is a *string* key and does not index an int array →
  **SB4030** (the existing type-check "string key on indexed array" — the same
  string→int discipline as DD#77; type_check emits a uniform SB4030). A quoted
  numeric (`${a["5"]}`) is likewise rejected — use the unquoted integer
  `${a[5]}`.
- **map** `${m[K]}` — `K` must be a string. Quote a digit-only literal or
  an integer-typed value to request its string form.

## The unrolling invariant (governing implementation)

Any nested key/index is **semantically equivalent to its unrolled sequential
form** — a series of temporary assignments, one per nested context. Shared
AST-to-SSA lowering evaluates and retains values in that order; merely
visiting a read before hoisting a later call is not sufficient. Lazy
arithmetic operands remain in their selected branches. For example:

```
if (( 5 > ${outermap[${innermap[":$s$(( (getpid) + 1)):"]}]} )) { echo yes }
```

unrolls to `t0=getpid(); t1=t0+1; t2=":"++$s++str(t1)++":"; t3=innermap[t2];
t4=outermap[t3]; 5 > t4`.

Consequences:
- **No nesting-depth cap.** Nesting falls out of recursive descent; a limit
  would be *added* logic to forbid what the pipeline handles uniformly.
- **No construct "needs disproportionate plumbing."** If one appears to, that
  is an implementation smell to **investigate** (a coupling that shouldn't
  exist), not an exception to carve out. Held to the bug policy.
- One operand-word parser handles direct keys on reads, writes and unset.
  One balanced bracket scan supplies their extents, including quotes and
  nested contexts; assignment recognition has no arbitrary token limit.
- Key types are checked through the shared own-expression walk, after stable
  inference, in each lexical body's actual collection-slot environment.
  Target identities remain distinct from content reads.

## Diagnostics

- **SB4041** (parse) — an empty unquoted direct key, more than one operand
  word, or partial quoting where a bare arithmetic map cell requires a wholly
  quoted key. Malformed nested constructs retain their own diagnostics.
- **SB4030** (type-check) — the wrong key type for the collection, including
  string array indices and integer map keys. Cell-value typing is independent.

## Original migration (2026-08-22, historical)

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
  sash subscripts: **bare in `(( ))` = arithmetic or a wholly quoted map key;
  dollar/braced forms = operand-word key.**
- DD#77 — arith string→int discipline (a string cell/scalar in arith needs a
  disposition). A string *key* for an int-array index is the same crossing,
  here rejected at the braced surface (SB4030) rather than coerced.
- notes/tasks/bug-braced-nested-subscript-index.md — the bug that surfaced this
  (superseded by this DD's rule: `${a[a[0]]}` rejects, `${a[${a[0]}]}` works).

## Amendment (2026-09-26) — the key is a *type*, not a spelling; and both surfaces enforce it

The measurements below are historical. The type and read/write parity rules
remain; the 2026-11-24 word parser replaces the old shape whitelist and
`parse_split_subscript_index` mechanism.

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

`parse_split_subscript_index` then recovered the raw index text from the token
spans and hands it to **`expr::parse_subscript_index`, the same function the
read path called**. Both surfaces used that parser, making parity structural
rather than two lists kept in step by
hand. **Every rule in this DD now reads "in `${a[K]}` and in `a[K]=v` alike."**

Parity is the point, not permissiveness: it is what makes section 2 below
enforceable on the write side at all, and section 2 then *narrows* the read
column too — `${a[${s:-2}]}` on a string `s` is now SB4030 on **both**
surfaces. What survives on both is the accountable form,
`a[$(( ${s:?msg} ))]=Q`, which aborts at runtime with the author's own message
when the value is not an integer.

### 2. "must yield a number" is decided by type, not by spelling

The original type-aware section enumerated spellings — "an **integer literal**, a
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
`2` as String (ordinary word literals are strings, and nothing else wants a bare
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

This records the original stdout-capture finding. The rule is type-based:
DD58 integer-result captures are integer values and may index arrays;
string-producing captures are not integer indices.

DG, on the last position where this DD was not enforced:

> I don't think `arr[$(f)]` is legal, but `arr[$((f))]` where `f` is an int
> returning fn seems fine.

This applied section 2 to a string-producing `$( ... )`, which types as
String, so `expr_type` already answered "not `Int`" and
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
is **SB4035** (a capture is not an arithmetic operand either) and `int i=$(f)`
is **SB4030**. (When this was written it was SB4041, the index's fallback
swallowing the arithmetic's own error; see the 2026-10-24 addendum.) An
author holding a string-returning function has exactly one migration —
declare it `fn f -> int` — so the message names that form directly, per
`array_index_error`'s own standing rule that a diagnostic must not send its
reader from one SB4030 to the same SB4030.

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
anything downstream sees it, so those spellings share this DD's direct-key
grammar. Bare arithmetic cells retain DD79's separate rule. This is
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

~~`$(( $a[1] ))` stays **SB4035** by design (DD#79 model B: inside arithmetic
the subscript spelling is the bare `a[i]`). `collect_arith_content`
re-serialises the main lexer's tokens through `tok_to_text`, so an un-gated
desugar would have flipped that deliberate rejection to an accept silently.
The desugar is gated on `Scanner::arith_depth == 0`.~~ DD#123 removes this
carve-out: the braceless spelling is the braced value import in arithmetic too.

## Amendment 2026-10-04 — the rule is the index's, not the position's

Historical shape-check repair: the table records the then-current whitelist,
not the current operand-word contract. `a-b` and `a.b` are now valid map keys;
unquoted multiple words still reject. The private key-type walker was a
separate remaining omission, repaired on 2026-11-24.

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
then gained `flag_assignment_index` beside it, both calling the same
`ambiguous_index_diag`. Those whitelist visitors are now retired; the shared
parser rejects invalid word boundaries, and the scoped type pass checks keys.

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

The October repair used `lhs_index_close_from(scanner, base)` as the one
search with the one predicate (the callers differ only by whether `name[` is
still unconsumed), and the operator is consumed by an explicit `match` that
returns SB4014 on anything else. That arm is unreachable while the probe and
the parser agree, which is now structural rather than a coincidence of two
hand-matched lookahead windows — it is checked instead of assumed precisely
because the previous version assumed and was wrong.

The 2026-11-24 implementation replaces that token search with the shared raw
balanced extent. A later `]=` cannot turn an already closed bracket word
into an assignment: the malformed examples now receive ordinary command-name
SB5011, rather than swallowing intervening source as an index.
Current regressions are `wordkey.18`-`.20`; unchanged valid writes remain
in `80.107`-`80.113`.

## Addendum (2026-10-24, batch-531 port step F14) — an explicit index that fails to parse reports its own error

The parse-error preservation rule survives. The implementation and
implicit/explicit fallback described below are historical: since
2026-11-24 every key is parsed as an operand word, with no literal fallback
after a failed parse.

An index was re-parsed as a value expression (`parse_whole_index`). When that
parse failed, the text fell back to a literal, and a literal that is not a
clean bareword was the ambiguous form SB4041 rejected. For an **implicit**
form that was the old policy. Today `${a[(1+2)]}` is a string-key type error
(SB4030), while the empty unquoted `${a[]}` is SB4041.

For an **explicit** form it was the wrong one. The old forms 2–4 (`$var`, an
expansion, a quoted key) open with `$`, `"` or `'`, and when one of them fails
to parse, the failure is that form's own error. The fallback discarded it,
and SB4041 then told the author to write an explicit form — sending them back
to the one they wrote. `${a[$(( 1 + ))]}` was advised to open arithmetic
explicitly; its real error is the missing operand (batch-531 B39).

**Current rule.**

- An index that opens with `$`, `"` or `'` and fails to parse reports that
  parse error: `${a[$(( 1 + ))]}`, `a[$(( 1 + ))]=x` and `${a["$(( 1 + ))"]}`
  are SB4020, "ends where an operand is expected".
- An index containing more than one operand word (`${m[$s x]}`) is SB4041.
  Adjacent fragments such as `${m[$s:tail]}` instead form one legal word.
- An empty unquoted index is SB4041.

It composes with DD#72's addendum of the same date, which makes a `$` that
starts no variable a stray byte: `${a[$(( $ ))]}` used to read `a[0]`
natively and emit C that gcc rejects, and is now SB4035. The capture case in
the 2026-10-03 addendum is corrected above: `${a[$(( $(f) ))]}` is SB4035,
not SB4041.

Specs: `80.131`–`80.134` (subscript.sh) and `75.164` (types.sh, re-coded
from SB4041 to SB4035). (`80.135`, `${a[${}]}`, sits beside them but pins
DD#65's rule: `${}` parses to that rule's SB4047 sentinel, so it never reaches
this route.) The implicit boundary is `cov.125`'s (an empty index in
arithmetic, still SB4041): a stub that sent every failing index down the
explicit route turned it red. Journal 533.
