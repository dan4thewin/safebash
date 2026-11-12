title: "DD#123 — a cell is a variable: every spelling reads, every spelling disposes, every spelling owes the crossing"
status: accepted

# DD#123 — a cell is a variable

**DG, 2026-10-04.** Three changes, one rule. A cell — `a[i]` or `m[k]` — is a
variable that happens to be addressed by a subscript. It reads where a variable
reads, it takes the dispositions a variable takes, and it owes the same
string→int accountability a variable owes. Where the three spellings of a cell
(`a[i]` bare, `$a[i]`, `${a[i]}`) disagreed about any of that, the disagreement
was the defect.

This is the principle `spec/arithmetic.sh` has carried in a section header since
the braced cell dispositions were built — **"a string cell behaves like a string
variable" (DG)** — applied to the rest of its consequences rather than to the
case that happened to be asked about.

## 1. `$a[i]` reads a cell in arithmetic

`$(( $a[1] ))` is the cell, as `$(( a[1] ))` and `$(( ${a[1]} ))` already were.

It used to be SB4035, and the reject was *right when it was written*. The
`$(( … ))` body is collected as a reconstructed string — logos tokens back to
text via `tok_to_text`, re-lexed by `tokenize_arith` — and while `$a` and `[i]`
were **separate tokens**, that round trip re-spaced the index to `$a[ $i ]`,
which `subscript_index_expr` then mis-parsed as `a[0]`. A 2026-08-21 attempt to
accept the spelling produced exactly that: `$a[i]`, `$a[$i]` and `$a[i+1]` all
silently returned the element at index 0. It was reverted, correctly, and the
note recorded that a real fix had to be lexer-level.

That lexer-level fix then landed for a different reason. The 2026-10-03
brace-less desugar fuses `$name[…balanced…]` into one `Tok::DollarBrace`, so the
index survives the round trip verbatim. **The carve-out had outlived the defect
it was protecting against** — the `arith_depth > 0` guard in
`braceless_subscript` was the last thing holding it.

Removing that guard is the whole change, and because the desugar routes the
spelling onto the braced grammar, it inherits the braced *rejects* too:

| in `$(( … ))`   | `a[i]` bare | `$a[i]` | `${a[i]}` |
|-----------------|-------------|---------|-----------|
| `a[1]`          | reads       | reads   | reads     |
| `a[$i]`         | reads       | reads   | reads     |
| `a[i+1]`        | reads       | SB4041  | SB4041    |
| `a[a[0]]`       | reads       | SB4041  | SB4041    |
| as an lvalue    | yes         | **no**  | **no**    |

The last two rows are agreement, not gaps. `$a[i]` is now *exactly* `${a[i]}`,
including where `${a[i]}` says no — and `${…}` is never an lvalue (DD#79 model
B's invariant), so `$a[i]` is not either. The bare form keeps the wider index
grammar and the lvalue because that is the arith-native spelling; that
asymmetry is model B and is unchanged.

Specs `dsx.1`-`dsx.5`.

## 2. A cell takes a brace-less disposition

`$a[0]:-z` is `${a[0]:-z}`. DD#90's scope list (a scalar `$name`, a positional
`$N`, an array splat `@name`) takes a fourth entry rather than a cell-shaped
exception.

This is a **correctness** fix, not an ergonomic one. Brace-less, the three
disposition operators silently were not themselves:

| input                 | was        | now   |
|-----------------------|------------|-------|
| `echo $a[0]:-z`       | `p:-z`     | `p`   |
| `echo $a[9]:-z`       | `:-z`      | `z`   |
| `echo $a[9]:?m`       | `:?m`, **rc 0** | aborts |
| `echo x $a[9]:~ y`    | `x :~ y`   | `x y` |

`:?` is the guard operator. `$a[$i]:?missing` was a check that could not fail:
the cell read empty, `:?m` concatenated as literal bytes, and the program
continued holding the non-empty, truthy string `":?m"`. `:~` is the elide, and
brace-less it passed an argument where the braced form passes none — under
DD#84 an argument-count difference. This is the SB5068 shape again: a spelling
that looks like it invokes a rule and quietly does not.

### No cell-specific limit, because the limit already exists

DG asked whether the brace-less cell needed a legibility restriction — "only
where it's clear where the braces would be if present, such as when bordered by
a space char". Measured, that restriction is **already the shipped rule**, and
it is not cell-specific:

- **The `:` must be tight.** `try_braceless_disposition` is offered the position
  immediately after the subject and returns `None` on anything but `:`. So
  `$a[0]:-z` is a disposition and `$a[0] :-z` is a cell followed by a separate
  word — exactly as `$x:-z` and `$x :-z` already differ.
- **The operand ends at a word boundary**, by `scan_braceless_operand`, the same
  function and the same boundary set a scalar uses.
- **Only `:-`, `:?`, `:~` open one.** `$a[0]:/bin` stays literal concatenation,
  as `$x:/bin` does.

So the brace-less cell asks no question the brace-less scalar had not already
answered. Giving cells a *different* rule would create a third grammar and
re-open precisely the "all paths must agree" problem that DD#80's 2026-10-03
amendment closed.

**Where adjacency collides with something, it collides loudly.** In arithmetic
`:` is the ternary's second half, so `$(( c ? $a[0]:-5 ))` reads `:-5` as a
disposition and the ternary then has no `:` — **SB4020**. That is the existing
scalar behaviour (`$(( c ? $x:-5 ))` is SB4020 today, where bash answers `-5`),
and it is the acceptable half of the trade: a diverging *reject* costs a space,
a diverging *accept* costs a silent wrong answer.

**Interpolation is unchanged and still requires braces.** `"$a[9]:-z"` is the
literal `:-z`, because the double-quote scanner never reaches this path — DG's
earlier ruling, "err on the side of clarity", stands.

Specs `80.114`-`80.122`.

## 3. A string cell owes the crossing in every spelling

`$(( ${s[0]} ))` on `array s=( hi )` silently evaluated to **0**.

DD#77 D1 says a string→int crossing must carry a disposition, and since
2026-08-19 a *bare* string cell has been SB4030. The braced spelling never was.
It arrives as an `ArithExpr::Expansion`, and `walk_arith_subscripts` — the
walker the check runs on — yields only bare cells and swallowed `Expansion` in a
`_ => {}` arm. So the one spelling that skipped the check was also the only
spelling in which a **map** cell can be written at all (model B makes bare
`m[k]` inexpressible), which is why no map cell had ever been checked.

Now all of these are SB4030, with `${…:-N}` / `${…:?msg}` as the escape hatch:

```
array s=( hi );    $(( ${s[0]} ))   $(( $s[0] ))    # SB4030
map   m=( [k]=5 ); $(( ${m[k]} ))                   # SB4030
array a=( 10 20 ); $(( ${a[0]} ))                   # SB4030 — the type, not the contents
```

The last line is the visible cost: a *numeric-looking* string cell rejects too,
because what is checked is the declared element type, exactly as for a bare
`s`. Six existing specs (`asx.1`-`asx.6`) asserted the old reading and were
re-pointed at `int array` / `int map`, or given the disposition — each keeping
its original subject.

A sexpr argument is **not** an int operand and is not flagged: `(strlen
${a[0]})` passes a string to something that wants a string, and the intrinsic's
signature types it. The imported-operand walker skips `FuncCall` args for the
same reason `walk_arith_var_reads` always has.

Specs `icx.1`-`icx.10`.

## What this does not change

**`:-` still targets the additive identity** — `""` for a string, `0` for an
int. That is DD#65 §1 and journal 209, decided 2026-07; sash has no concept of
an unset variable, so `:-` fires on the identity element of the subject's type
rather than on "unset or empty". `int u=0; ${u:-5}` is `5` and `str s=0;
${s:-5}` is `0`; the first diverges from bash and is meant to. Restated here
only because this session re-derived it from behaviour and briefly mistook it
for an undocumented accident.

**Model B is untouched.** Bare `a[i]` keeps the arith index grammar and the
lvalue; bare `m[k]` is still inexpressible; `${…}` is still never an lvalue.

## Where it lives

- `sash/src/parse/lexer.rs` — `braceless_subscript`: the `arith_depth` guard is
  gone, and a trailing disposition is folded into the synthesized
  `DollarBrace`.
- `sash/src/parse/arith.rs` — `arith_bad_literal_error`: the bracket arm no
  longer claims `$a[i]` is unsupported. It is reached by a *stray* bracket now
  (`$(( $a[1 ))`, `$(( 1[2] ))`) and says so.
- `sash/src/ir.rs` — `walk_arith_int_operands`, the single traversal of the
  int-operand positions, with `walk_arith_var_reads` (bare names) and
  `walk_arith_imported_operands` (`${…}` roots) as its two views.
  `walk_arith_subscripts` is now total.
- `sash/src/type_check.rs` — `check_arith_expansion_string_cell`, and
  `bare_map_subscript_error` made type-aware so a string map's author is not
  sent to `${m[k]}`, which would reject in turn.

## Enforcement follow-on (2026-10-16)

The same principle exposed two remaining
parser differences: cell trust forms were rejected instead of taking the
scalar trust path, while unknown cell tails silently vanished. Both are fixed
(DD#91 / SB4047). Tracing the new trust path also exposed omitted cell-index
walks in read, taint, argv-splat and default-type checks. The index is a separate
read, not covered by the cell's trust flag. Bare trusted string values still
owe their string-to-int disposition, for cells and scalars alike. The existing
SB4039 rejection for unsupported cell colon operators is preserved; its text
no longer incorrectly says substring/sed on cells are unimplemented.
Journal 512 records the probes and the coverage measurement.

## Prior art

- `notes/decisions/DD77-*` D1 — the string→int crossing and its dispositions.
- `notes/decisions/DD79-arith-subscripts-model-b.md` — bare vs imported, and
  the `${…}`-is-never-an-lvalue invariant.
- `notes/decisions/DD90-braceless-dispositions.md` — the scope list this
  extends, and the tight-`:` disambiguation it already relied on.
- `notes/decisions/DD80-braced-key-discipline.md`, amendment 2026-10-03 — "all
  paths must agree", and the desugar that made this possible.
- `notes/decisions/DD65-var-modifier-prune.md` §1 — the additive-identity
  trigger, unchanged.
