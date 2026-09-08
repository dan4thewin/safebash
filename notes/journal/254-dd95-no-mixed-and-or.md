# 254 — DD#95: mixing `&&` and `||` is rejected

**Branch:** `hand-parser`.

## How it started

DG asked a question, not for a change: *"do `!`, `&&` and `||` behave
identically outside `[[ ]]` as within? e.g. `[[ -n $a ]] && [[ -n $b ]]` vs
`[[ -n $a && -n $b ]]`."*

Answer: almost. `!` (both spellings and its binding), pure `&&` chains, pure
`||` chains, and the statement-level consume rules are all identical. But a
**mixed** chain groups differently — inside, `&&` binds tighter; outside, the
two are equal-precedence and left-associative. The truth tables diverge in 2 of
8 rows, whenever `a` is true and `c` is false.

The same text meant two different things depending on where it sat.

## The survey DG asked for

He then asked what C, Go, Rust, Python "and perhaps other dominant languages"
could teach us. Verified against sources rather than memory, and it reframed
the problem:

- **Every mainstream expression language binds `&&` tighter** — so sash's
  *inside* was already right and needed no change.
- **Shell is the outlier** (equal precedence, left-assoc), and the outside
  inherited it. The divergence was never sash's invention.
- **C's own `-Wall` warns on exactly this shape**, and `-Werror` makes it
  fatal. The most precedence-confident language on the list does not consider
  its own precedence self-explanatory.
- **Ada makes the mixture a syntax error.** The safety-critical language
  resolves it by rejection — the precedent sash follows.
- The deepest lesson was **Perl/Ruby's**: they have sash's exact shape (an
  expression tier and a control-flow tier) and distinguish them by *spelling*
  (`&&`/`||` vs `and`/`or`), not by one spelling with two precedences. That
  reframed the defect: the precedence difference was the symptom; identical
  spelling for two tiers was the cause.

DG's call: reject the mixture in **both** contexts, with **different advice**
per context — parentheses inside, `if`/dispatch outside.

## Why rejection beats picking a winner

It **removes** the inconsistency rather than papering over it. Afterwards there
is no text whose meaning depends on context: the ambiguous form is illegal in
one and conventional in the other.

It also settles the missing-grouping question outside `[[ ]]`. `( … )` is
SB4014 there and `{ …; }` is SB4027 — under this rule that is not a gap to
fill but the point. A command list that needs grouping is asking to be an `if`
or a dispatch, which sash already has (DD#68 braces, DD#66/#71 dispatch).

The rule is provably minimal: pure chains group the same under either
associativity, so only mixing needs restricting.

## Implementation

- Inside: `parse_test_and_tracked` reports whether it combined a `&&` *at this
  level*; `parse_test_or` errors if it saw both. A parenthesised operand is a
  primary, so its own operators are checked independently and never count as
  mixing at the outer level — which is what makes `( )` the fix.
- Outside: the left-folding loop in `parse_trailing_list_op` remembers the
  first operator and rejects a differing one.
- One code (SB4053), two messages — the SB4020 precedent (same rule, two
  contexts, tailored advice).

## Migration, and three superseded specs

Measured first: 21 mixed sites in the whole corpus, 20 of them the single idiom
`X && echo Y || echo N`, and `contrib/*.sash` had none. The `suites/` hits were
vendored Haskell. So the cost was ~21 mechanical rewrites to
`if … { } else { }`.

Three were **superseded, not migrated**: `wordsplit.sh` 62.7, 62.9 and 62.10
existed specifically to pin the shell left-associativity ("`&&` / `||` are
left-associative: `(false && A) || B` runs B") that this DD removes. They now
assert SB4053, with the old claim recorded in the comment. Worth flagging: this
DD deliberately deletes documented, tested behavior — the specs said so, and
now they say why it changed.

## Verification

3-mode sweep: **0 regressions**, +36 passing (4084 → 4120; failing unchanged at
33). specs `errors.sh` 95.1–95.12, written first — covering both diagnostics,
both parenthesised resolutions, the motivating ternary idiom, both
replacements, pure chains in both contexts, the error-handling idiom, and `!`.
