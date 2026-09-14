---
title: Three arms the dispatcher owns
date: 2026-09-14
---

# Three arms the dispatcher owns

`notes/tasks/lexer-hot-line-zero-arms.md` filed two hot-line zero-arms found
by the asymmetry query in journal 311. Both got a verdict. Chasing the second
one turned up a third that neither the query nor the filing had connected to
it, and that third one was the interesting one.

DG's rule for the campaign: an unreachable arm is either dead code, which
goes, or a should-never-run assertion, which earns a unit test proving it
catches what it claims. Two of the three were the second kind. One was the
first kind but had been dressed up as the second.

## 1. `scan_word_chars`'s bare leading `=` — dead

```rust
b'=' if word.is_empty() => {
    self.pos += 1;
    return Some(SpannedToken { token: Tok::Eq, ... });
}
b'=' => break,
```

Zero hits in 43,028 calls. The guard is `word.is_empty()`, which is only true
on the first loop iteration, because every non-breaking arm pushes a byte. So
the arm fires only when `scan_word` is entered with `self.pos` sitting on an
`=`.

It never is. `next_token` dispatches on the first byte, and `b'='` is claimed
at the top by `scan_eq_punct`, which already handles `=`, `==` and `=~`. The
only other routes into `scan_word` are `scan_dash_or_word`,
`scan_plus_or_word`, `scan_lbracket`, `scan_rbracket` and the `scan_via_logos`
fall-back, and not one of them advances `self.pos` before calling. The first
byte `scan_word_chars` sees is always the byte the dispatcher looked at.

So the arm is a leftover from a factoring where `=` was not dispatched
separately. Removing it lets the helper drop its `Option<SpannedToken>` return
and its now-unused `start` parameter, and lets `scan_word` drop the `if let
Some(eq_tok)` wrapper. Four lines of caller boilerplate for a branch that
could not fire.

## 2. `Scanner::next`'s direct pull — a real path, just never taken

```rust
let tok = if !self.peeked.is_empty() {
    self.peeked.remove(0)           // 86,406
} else {
    self.next_token_skip_comments() // 0
};
```

Not dead. This is the scanner's actual token source; the buffer arm only ever
returns what this arm produced earlier through `peek`. It reads zero because
every caller in the parser peeks before it consumes, which is a property of
the parser, not a property of `next`.

Two unit tests, then. One calls `next` on a fresh `Scanner` and walks
`echo hi` to `Eof` without ever peeking. The other runs two scanners over the
same source, one peeking before each `next` and one not, and asserts they
agree token-for-token including spans. That pins the contract that the two
paths are interchangeable, which is the thing the 0/86,406 split made
impossible to trust.

## 3. The silent truncation nobody asked about

Item 1's proof needed the full list of routes into `scan_word`, and one of
them was this:

```rust
_ => {
    // Logos didn't match the dispatched byte. Fall back to
    // word scan so the byte still becomes part of a token.
    self.scan_word(start)
}
```

`scan_via_logos` runs the DFA on one of ten structural bytes — newline, `#`,
`;`, `|`, `&`, `(`, `)`, `{`, `}`, `>`. If the DFA does not match, this hands
the byte to `scan_word`. But all ten are word-break bytes. `scan_word_chars`
breaks on the first byte, the word comes back empty, and `scan_word` returned:

```rust
if word.is_empty() {
    return SpannedToken { token: Tok::Eof, ... };
}
```

`Tok::Eof`. In the middle of the source. The parser would stop, the program
would compile, and everything after that byte would be gone without a word.
Same failure family as SB4056 the day before: a path that answers a question
it cannot answer by inventing a plausible-looking token.

Both arms read zero, which is the only reason nobody has hit it. The comment
on the fall-back describes an intent ("so the byte still becomes part of a
token") that the code does not carry out — the byte becomes part of nothing.

Two lines above it, the same function already says what to do with a broken
DFA invariant:

```rust
to_tok(lt).expect("logos token without payload mapping")
```

So the pattern was sitting right there. Both arms are now explicit panics
naming the invariant, and both have a `#[should_panic]` test in a
`invariant_tests` module inside `lexer.rs` — the test module has to live in
the module because the scan methods are private, and the tests deliberately
reach past the dispatcher to hand them input the dispatcher never would.

A third test, in `parse/tests.rs`, runs the DFA directly against all ten
dispatched bytes plus the six doubled forms (`;;`, `||`, `&&`, `>>`, `>&`,
`))`) and asserts every one matches. That is what makes the panics
unreachable rather than merely untested: if someone edits `logos_tokens.rs`
and drops a rule, that test fails before the panic ever gets the chance to.

## What the filing got wrong

The task file guessed that item 2 was "almost certainly" just needing a test,
and it was. It guessed item 1 "may be genuinely dead", and it was. Both
guesses held. What it missed is that item 1 could not be settled without
enumerating every caller of `scan_word`, and that enumeration is what surfaced
item 3 — which no coverage query would have ranked, because its two arms are
in different functions and the asymmetry query works line by line.

The query finds candidates. The proof obligation finds bugs.

## State

35 unit tests (was 30 at the start of the campaign). Sweep 4627 passing, 10
skipped, 0 failing.
