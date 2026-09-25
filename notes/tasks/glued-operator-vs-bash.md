---
status: open
priority: P3
kind: question
---

# `[[ $x==a ]]` answers F where bash answers T

A **bash divergence**, pre-existing and independent of DD#118 — surfaced while
probing the `=`/`==` surface, not caused by it.

```sh
x=a
[[ $x==a ]]   # sash: false      bash: true
[[ $x=~a ]]   # sash: false      bash: true
```

`parse/parser.rs::extract_compact_operator` splits a glued operator off the end
of a word, so `$x==a` parses as the three-token comparison `$x == a`. bash does
not split: it sees one word, `a==a`, and applies its implicit-`-n` rule, which
is true. Under DD#118 sash has no implicit `-n`, so the two languages now
disagree about the *shape*, not just the answer.

## Why it is not simply a bug to fix

The no-space form is a **deliberate sash extension**. `dispatch.sh` specs
`[[a==a]]` and friends 28 times, and bash cannot parse that at all — there is
no `[[a` token for it. So the glued spelling is load-bearing in sash and
meaningless in bash, and "match bash" is not obviously the right answer.

The narrower question is whether the *variable* form `[[ $x==a ]]` should keep
splitting. Three options, none free:

1. **Keep splitting** (today). Consistent with `[[a==a]]`; diverges from bash
   on a shape bash accepts.
2. **Stop splitting when the left side is an expansion.** Matches bash's
   tokenization, but under DD#118 the result is now SB4068 (a bare word), so
   `[[ $x==a ]]` becomes an *error* rather than silently true — arguably the
   best reading, and it costs nothing in the dispatch specs, which are all
   literal-subject.
3. **Reject the glued form outright** outside dispatch. Cleanest to explain;
   costs the extension where someone may be relying on it.

Option 2 looks strongest precisely because DD#118 changed the landing site:
before, "don't split" meant "silently true"; now it means "rejected".

DG's call — this is a language-surface question, not an implementation detail.

## Where

- `sash/src/parse/parser.rs::extract_compact_operator`
- `spec/dispatch.sh` — 28 no-space specs that constrain any change
- `notes/decisions/DD118-explicit-tests.md` — "Not settled here"
