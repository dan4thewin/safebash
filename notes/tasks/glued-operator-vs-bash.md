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

## The generality behind it (DG's question, 2026-09-25)

Measured across every context, the rule that actually governs whether glue
splits into operators is **lexical**, not per-construct:

> Glue is split into operators exactly where the operand's own lexical class
> **cannot contain** the operator's characters. Where it can, glue concatenates.

| regime | operand class | `a+b` / `a==b` means | contexts |
|---|---|---|---|
| **expression** | names and numbers | three tokens — `+` is lexically impossible inside a name | `(( ))`, `$(( ))`, subscripts |
| **word** | arbitrary bytes | one token — `+` and `==` are ordinary data | command args, assignment RHS, `for` lists, test operands, dispatch patterns |

Verified both directions: `int ab=1; (( c = ab+1 ))` → 2 (split is
*determined*), and `str x=a; [[ $x == a==b ]]` → F, comparing against the
literal `a==b` (glue is *data*). bash agrees on the second.

`[[ ]]` is **not** a third regime. It is word-regime tokenization plus a
**recovery rescan**: whitespace splits first, and only if that yields a single
operator-less token does `extract_compact_operator` rescan that token. Proof
that it is a fallback, not always-on — `[[ a==b == $r ]]` with `r=b` answers F
in sash *and* bash, because a spaced operator was found and no rescan
happened. In the source it sits in the `_` arm of `parse_test_binary_tail`.

## Why DD#118 reframes the choice

The rescan and **SB4068 are alternatives on the same code path** — the rescan
fires, and if it fails, SB4068 does.

So DD#118 changed what the rescan *competes with*. Before, the losing reading
was bash's implicit `-n` — a legal, if useless, test. The rescan was
overriding a valid parse, which is precisely why sash and bash disagreed.
Now the losing reading is an **error**. A recovery heuristic that fires only
on otherwise-invalid input is a materially different proposition, and a more
defensible one.

That strengthens **option 1** (keep splitting) relative to how it looked when
this was filed: nothing legal is being overridden any more. It also weakens
option 2 — "stop splitting on expansions" would leave `[[a==a]]` splitting for
literals but not variables, a rule with no lexical justification.

Against option 1: the split is still a *guess* by the principle above, since a
word genuinely can contain `==`. The honest framing is that `[[ ]]` trades a
little lexical purity for the DD#66 dispatch spelling, and DD#118 lowered the
price.

DG's call.

## Where

- `sash/src/parse/parser.rs::extract_compact_operator`
- `spec/dispatch.sh` — 28 no-space specs that constrain any change
- `notes/decisions/DD118-explicit-tests.md` — "Not settled here"
