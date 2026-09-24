# 401 — the type was the retired surface

Coverage work, resumed. The queue from the union run at `b4981d7` was four
commits stale, and the first thing it cost was a false lead: `read_arg_value`'s
"uncovered" lines in `ir.rs` resolved to doc-comment text, because the DD#99
work had shifted the file underneath the line numbers. `coverage.db` is keyed
by `(file, line)` and nothing in it records the commit it was measured at, so a
stale queue does not announce itself — it just points at the wrong lines.

Re-ran `mk cov-union`. Fresh baseline:

| axis      | total | missed | covered |
|-----------|------:|-------:|--------:|
| functions |  2245 |     27 | 98.80%  |
| lines     | 27459 |    607 | 97.79%  |
| regions   | 30633 |    885 | 97.11%  |
| branches  |  3894 |    349 | 91.04%  |

Top of the re-ranked queue: `ir.rs::read_arg_value`, 7 uncovered lines out of
11 executable ones — the densest cold function in the tree.

## What was cold

Everything except the first arm:

```rust
fn read_arg_value(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Literal(s) | ExprKind::SingleQuoted(s) => s.clone(),   // hot
        ExprKind::DoubleQuoted(parts) => match parts.as_slice() {        // cold
            [] => String::new(),
            [p] => match &p.kind {
                ExprKind::Literal(s) => s.clone(),
                _ => String::new(),
            },
            _ => String::new(),
        },
        _ => String::new(),                                              // cold
    }
}
```

The doc comment said what it was for: "the literal VALUE of a `read` argument
word (unwrapping one layer of quotes)… never valid read targets **or
delimiters**". Delimiters. That is `read -d ''`, and DD#99 retired the entire
flag surface. The quote-unwrapping was there to read a delimiter that can no
longer be spelled.

## Proving it dead, by execution

The function's own doc claims SB5057 guarantees the input. That claim is worth
exactly nothing until it is tested, so I tested the gate rather than read it.

SB5057 accepts an argument iff `is_plain_identifier(expr_literal_str(arg))`,
and `expr_literal_str` returns `s.as_str()` for `ExprKind::Literal` and `""`
for every other variant, while `is_plain_identifier("")` is false. So the gate
admits exactly one shape. That is a type-level argument, and it holds only if
every `readline` actually reaches the gate — which is a question about
`child_bodies()`, the same totality question as journal 400's wildcards. So I
probed every nesting context I could construct:

top level · function body · `if` · `else` · `while` · `until` · `for` ·
pipeline · `&&` · `||` · brace group · negation · DD#66 dispatch exact body ·
dispatch `||` body · dispatch regex body · function containing a dispatch ·
nested `if`/`while`

All sixteen rejected with SB5057. (`case` and `( )` "escaped" only by being
rejected first as removed constructs — SB4029 and a parse error.)

So `readline 'x'` is rejected too, which means the *`SingleQuoted` half of the
covered arm is dead as well*. Coverage could not see that: the two patterns
share a line, so one hot variant reports the whole arm hot. Line coverage
cannot distinguish alternatives in a `|`-pattern, and this is a clean example
of it hiding dead code inside a live line.

## The actual finding

`read_arg_value` is a near-exact copy of `ir::literal_word_text` — same four
arms, same quote-unwrapping — differing only in collapsing `None` to `""`.
And `literal_word_text`'s doc comment already names that as the defect it was
written to prevent:

> `None` is the whole point. The predecessor of this function fell back to
> rendering the expression's *source text*, so `$(join a "$sep")` silently
> joined with the four characters `"$sep"`. A slot that requires a
> compile-time constant must be able to say "this is not one" rather than
> invent a plausible string.

`read_arg_value` invented `""` instead of `"$sep"`. Same defect, different
invented value, in a function sitting twenty lines away from the warning.

This is the third rival-renderer in this campaign (journal 385's brief
renderers, 390's duplicate, now this). The pattern each time: two functions
compute the same thing by different rules, the strict one gates and the loose
one consumes, and the divergence is invisible while the gate holds.

## The root cause was the type

The deeper problem was not the function but its return type:

```rust
pub struct ReadTarget { pub var_names: Vec<String> }
```

A `Vec` of names — the shape of the multi-target form DD#99 retired. Its own
doc comment said the form was retired and that SB5057 guarantees exactly one
name, and then modelled many anyway. Because the type could hold any count,
both call sites had to handle counts SB5057 forbids, and both chose to do it
silently:

```rust
let [name] = flags.var_names.as_slice() else { return; };   // cmd_handlers.rs
if let [name] = flags.var_names.as_slice() { … }            // stmt_handlers.rs
```

Both of those lines were cold too. A `readline` with the wrong arity would
have emitted **no `ReadLine` instruction at all** — the program would read
nothing and say nothing about why. Four lines below the first one, in the same
file, `handle_signal` records the codebase's position on exactly this:

> the gates that used to stand here … each turned a malformed `signal` into a
> silent no-op; the validator rejects those spellings now

So: make the type say what the validator guarantees.

```rust
pub fn readline_target(args: &[Expr]) -> String {
    let [target] = args else {
        panic!("SB5057 validated `readline` takes exactly one target");
    };
    literal_word_text(target).expect("SB5057 validated the target is a bare name")
}
```

`ReadTarget` and `read_arg_value` are gone; both call sites lost their dead
guards; the rival renderer is gone. Ten uncovered lines removed by deletion
rather than by writing specs for inputs that cannot exist.

The two remaining guard lines are should-never-run assertions, and per DG's
rule for this campaign they earn unit tests that prove they catch what they
claim — reaching past the validator and calling the accessor directly, the
same shape as `parse/lexer.rs`'s `invariant_tests`. Unit tests are how the
union target measures these at all; no source text can reach them.

## Gates

Sweep **6237** (native 3214 / c 3010 + 13 skipped), predicted 6237 — no spec
changes, this is a refactor — **0 failing, 0 flips** against `8b63c77`.
`mk test` 54/27/20, up from 52 by the two new invariant tests. `readline x`
and `echo … | readline v` verified byte-identical at runtime; `readline "x"`
still SB5057.

## Method note

The stale queue is the lesson worth keeping. I nearly triaged a function by
reading `coverage.db` line numbers against a tree they were never measured
against; the first sign was that "uncovered line 883" was a doc comment, which
is impossible. **A coverage row that points at a non-executable line means the
database is stale, not that the line is cold.** `mk cov-union` before trusting
a queue that has survived a commit.
