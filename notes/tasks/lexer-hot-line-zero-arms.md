---
title: Two hot-line zero-arms in the lexer
status: open
depends: []
gates: []
---

# Two hot-line zero-arms in the lexer

Found by the branch-coverage asymmetry query (journal 311): a branch arm with
0 hits whose sibling arm ran tens of thousands of times. Twice now that shape
has been a real defect (`Program::end_block`, journal 309; SB4056, journal
311), so these two deserve a verdict rather than a shrug.

DG's rule for this campaign: *if it is unreachable, decide whether it is dead
code or a should-never-run assertion. Dead code goes. An assertion gets a unit
test proving it catches what it claims.*

## 1. `parse/lexer.rs:235` — `next()` never pulls a fresh token

```rust
pub fn next(&mut self) -> SpannedToken {
    let tok = if !self.peeked.is_empty() {
        self.peeked.remove(0)          // 86,406 hits
    } else {
        self.next_token_skip_comments() // 0 hits
    };
```

Every `next()` call across the whole spec suite drained the peek buffer. The
`else` is the lexer's actual token source and it was never the path taken,
because every caller peeks before it consumes.

Not dead in the "delete it" sense — it is what fills the stream when the
buffer is empty, and `peek` itself calls into the same machinery. The question
is whether the buffer is *structurally* always primed when `next()` runs, in
which case the branch is provably unreachable and the function could assert
it, or merely always primed *in practice*, in which case it needs a unit test
that calls `next()` on a fresh Scanner.

The second is almost certainly true and cheap to prove — construct a
`Scanner`, call `next()` without peeking, assert the first token. That single
test would cover the arm and document the contract.

## 2. `parse/lexer.rs:1581` — `scan_word_chars` never returns `Some`

```rust
if let Some(eq_tok) = self.scan_word_chars(start, &mut word) {
    return eq_tok;       // 0 hits
}                        // fallthrough: 43,028
```

In 43,028 calls the helper returned `None` every time. Its `Some` is an
early-out for a token shape (from the name, an `=`-bearing word) that nothing
in the suite produces.

This one may be genuinely dead — if the `=` case is handled earlier now, the
early-return is a leftover from a previous factoring. Needs a read of
`scan_word_chars` to see under what condition it yields `Some`, then an
attempt to construct that input. If no input reaches it, delete it and its
`Option` wrapper; the helper can return `()`.

## Method note

The query that surfaces these:

```sql
WITH pairs AS (
  SELECT a.file, a.line, a.count AS dead, MAX(b.count) AS live
  FROM cov_branches a JOIN cov_branches b
    ON a.file=b.file AND a.line=b.line AND a.block=b.block AND a.branch<>b.branch
  WHERE a.count=0
  GROUP BY a.file, a.line, a.block, a.branch
)
SELECT file, line, live FROM pairs WHERE live > 2000 ORDER BY live DESC;
```

Read both arms before concluding anything — a 0/N split is the signal, a
low-traffic function is not.
