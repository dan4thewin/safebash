# 413 — The left side that could not say it

DG's directive after journal 412 was to enforce the strictest subscript
forms: an array index must be an integer, a map key must be a string, and
**read and assignment must accept the same thing**. This entry covers the
last clause, which turned out to be a precondition for the other two.

## The asymmetry

Measured, before anything changed:

| index form | `${a[K]}` read | `a[K]=` write |
|---|---|---|
| `2` | ok | ok |
| `$i`, `${i}` | ok | ok |
| `$(( 1+1 ))` | ok | **SB5011** |
| `${s:-2}`, `${s:?m}` | ok | **SB4014** |
| `$s:-2` | ok | **SB5011** |
| `"$i"` | ok | **SB4014** |

The write side took exactly one token. `parse_split_subscript_index`
matched `Tok::Dollar(v)` or a `Tok::DollarBrace` holding a bare
identifier, and everything else fell to an `_ =>` arm emitting SB4014,
"unsupported dynamic subscript index in assignment".

That is not a small gap. DD#80's own remedy for a string index is to
convert it with `$(( … ))` — and `$(( … ))` was precisely what the left
of an `=` would not take. An author told to convert had nowhere to put
the conversion. Worse, the two failures did not even agree on a
diagnostic: `a[${s:-2}]=` reached the parser and got SB4014, while
`a[$((1+1))]=` never looked like an assignment at all, so `a[` became a
command name and the error was SB5011 — a *command not found* message
for a typed-index problem, naming neither the array nor the index.

## Why it was one token

The lexer breaks a word on `$`, so `a[$i]=v` arrives as four tokens:
`a[`, `$i`, `]`, `=`. `looks_like_dynamic_subscript_assign` committed to
the assignment parse only when `peek_n(2)` was exactly `]`, which pins
the index at one token by construction. The closed form `a[lit]=` never
splits and goes down a different path entirely, which is why `a[2]=Q`
always worked.

## Three attempts

**Widening the lexer** — teach `scan_word` to keep `$` inside a
subscript — was rejected without trying it: the same word scanner runs
on every command argument, so `echo a[$i]` and every other bracketed
word would move with it.

**Running `parse_expr` on the live scanner** was tried, and failed in an
instructive way. It compiled, and then *every* subscript assignment
broke, including `a[$i]=Q` which had worked for months. With no space
before it, the word parser glues the closing `]` onto the last operand:
the index came out as `1]`. The read side never hit this because its
sub-scanner is handed the index text alone, with no `]` in it.

**Re-parsing the text** is what landed, and it is the version that
should have been obvious first. The token spans carry byte offsets, so
the raw index text can be recovered from the source and handed to
`expr::parse_subscript_index` — *the same function the read path calls*.
The only new machinery is a `Scanner::source_slice` accessor and a
bounded look-ahead for the `]`.

The point is not that it is less code. It is that the two surfaces can
no longer disagree: there is one list of accepted index forms and both
sides call it. A parity that has to be maintained by hand is a parity
that drifts, and this one had drifted far enough that the language's
documented remedy did not compile.

## What fell out for free

Three existing specs changed behavior, all of them improvements, and
none of them anticipated:

- **80.35** — `a[ x ]=5` is now a subscript assignment. It used to be
  SB5011 (`a[` as a command name) because a bareword index was not a
  `$`-token. `parse_subscript_index` trims, so the whitespace is
  immaterial and the real fault is reported: `x` is a string key on an
  array (SB4030). The read side already accepted `${a[ 1 ]}`; the write
  side now does too.
- **80.38** — `m[${k:-z}]=1` is honoured rather than rejected.
- **80.39** — `m[${#k}]=1` now reports SB4038, the `${#…}` retirement,
  instead of the blanket SB4014. Byte for byte the message the read side
  gives for the same text.

That last one is the clearest evidence the change is structural rather
than a widening. The LHS did not learn about `${#…}`; it stopped having
an opinion of its own.

## What did not move

`a[$i` with no `]`, and `a[$i; echo hi`, still back out and leave `a[` a
command name. The look-ahead is bounded at 32 tokens so a bare `name[`
cannot walk the rest of the file; spec 80.58 pins that, and passes both
before and after the change — a deliberate regression guard rather than
a new assertion.

## Verification

8 new specs (80.51–80.58) plus the 3 rewritten. Predicted total 6422;
the sweep returned exactly 6422, zero failing, **zero flips** against
`aa8fad2` (the pre-change state `ff9b857` committed). Non-vacuity proved
by reverting the three source files and rebuilding: exactly 80.35,
80.38, 80.39 and 80.51–80.57 fail, and 80.58 does not.

`parse_subscript_index` sat at cognitive 6 against a 5.61 bar, and
making it `pub(super)` counts as touching it. Rather than bypass the
gate, its nesting — `if !bare { if let Ok { if at_eof { … } } }`, which
is exactly the shape cognitive complexity exists to catch — was lifted
into a `parse_whole_index` helper returning `Option`. Both functions are
now 3/1.

## Still owed on DG's directive

This entry is the parser precondition only. The two type rules are not
implemented: an array index is still not required to be int-typed (an
undeclared `i=1` and a positional `$1` both still index), and a map key
is still not required to be string-typed (a bare `${m[$i]}` on an
int-typed `i` is still accepted, against DD#105). The subscript int sink
still has no DD#77 D4 default-literal check, so `a[${s:-hello}]` still
ICEs at `codegen/values.rs:1432`.

Those now have somewhere to land. They did not before: a rule enforced
on the read side and unenforceable on the write side would have had to
be two rules with two messages, and the message on the write side could
not have named the remedy.
