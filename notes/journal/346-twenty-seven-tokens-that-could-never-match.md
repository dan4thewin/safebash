# 346 — Twenty-seven tokens that could never match

`to_tok` came up in the branch-coverage loop with 28 cold lines at cc 46 /
cog 1. The shape of the miss was the tell: every *operator* arm was covered
and every *keyword* arm was cold. That is not the signature of a thin test
suite. It is the signature of a function whose inputs are constrained.

## The constraint

`Scanner::next_token` (`lexer.rs:389`) dispatches on the first byte and hands
logos exactly ten of them:

```
\n  #  ;  |  &  (  )  {  }  >
```

Everything else is hand-rolled, because it needs stateful scanning or
word-boundary semantics the surrounding tokeniser already owns:

| byte | scanner | produces |
|---|---|---|
| alphabetic | `scan_word` → `classify_word_token` (1836) | **every keyword** |
| `!` | `scan_bang_or_neq` | `Bang` |
| `+` | `scan_plus_or_word` | `PlusAssign` |
| `[` | `scan_lbracket` | `DblBracketOpen` |
| `]` | `scan_rbracket` | `DblBracketClose` |
| `<` | `scan_lt` | `HereString` |

logos matches from position 0 of a slice that always starts with a dispatched
byte. A pattern beginning with `i` therefore cannot fire — not "does not
currently fire", *cannot*. The cold-line set matched that constraint exactly,
which is what turned a coverage observation into a reachability proof. The
arithmetic closed too: 22 keywords + 22 operators + `Comment` = 45 arms = cc 46.

`LogosTok::lexer` (`lexer.rs:412`) is the sole producer outside the enum
definition and the unit tests. So 27 of the 45 variants were unproducible.

## Why this needed a ruling rather than a deletion

The module doc said so itself, at the top of the file:

> The keyword variants below are not reached through the dispatcher … but are
> retained because the equivalence tests pin logos' longest-match behaviour for
> them — useful guard rails if we ever move keyword promotion under logos.

That is a documented deliberate choice, and the first thing to check was
whether its justification had rotted — the 2026-09-14 test redux deleted 546
integration tests and could easily have taken the cited tests with it. It had
not: `logos_equivalence::keywords_recognized` and `to_tok_maps_all_variants`
were both still live and both still drove every keyword variant.

So the two sources of truth genuinely disagreed. "Purge dead code" said delete;
an in-file comment said keep, for a stated reason that still held. That is the
definition of a fork that is consequential and underdetermined, so it went to
DG rather than being resolved by judgement.

It also meant the *coverage* question was already answered: the cold lines were
covered by unit tests, which is precisely the "modulo unit tested regression
guards" carve-out. `to_tok` was never a coverage gap. It was a dead-code
finding wearing a coverage gap's clothes.

**DG ruled: purge all 27, and tighten the signature.**

## What made the retention worse than inert

The keyword list was not merely unused. It was a *second answer to a question
that already had one*. `classify_word_token` holds the real keyword table; the
logos enum held a copy that nothing consulted and nothing checked against it.
Any drift between them would have been invisible — the guard rails were not
guarding, because the thing they mirrored could change without them.

That is the same shape as both bugs this session (journal 344): a canonical
answer with a narrower duplicate written beside it. `has_mutation_inner` was a
partial re-derivation of `arith_has_mutation`; `walk_arith_var_reads` was
pressed into service as a read collector when `check_arith_reads` was the
collector. Here the duplicate had not caused a bug yet. Speculative retention
for a migration that may never happen is a standing invitation to one.

## The `Comment` arm

`Comment` is the one live variant whose `to_tok` arm was still dead, for a
different and narrower reason: `emit_logos_token` (`lexer.rs:485`) intercepts
it *before* calling `to_tok`, because it needs the matched slice and a
payload-free mapping cannot carry one. So `to_tok` never saw one, its `Option`
return existed for exactly that arm, and the sole caller answered it with
`.expect("logos token without payload mapping")`.

`to_tok` now returns `Tok`. The `Comment` arm is an `unreachable!` naming what
it means — the interception was removed and a comment is about to lose its
payload — and cites `LOGOS_DISPATCH_INVARIANT`. That is the sanctioned
exception to the purge rule: an ICE that prevents a regression, pinned by
`to_tok_comment_is_loud_ice`.

## Tests

`keywords_recognized` was deleted; it tested logos' recognition of tokens logos
can never be handed. `operators_longest_match` lost its five purged entries.
`whitespace_skipped_continuations_swallowed` used `if … \<newline> then` to
demonstrate continuation-swallowing — rewritten with `(` and `)`, which
preserves the property using bytes that actually reach logos.

`to_tok_maps_all_variants` became `to_tok_maps_every_reachable_variant`, and
got stronger in the process. The old one asserted `to_tok(t).is_some()` for
every variant — it would not have caught a variant mapped to the *wrong* `Tok`.
The replacement pins the mapping pair by pair.

`logos_matches_every_dispatched_structural_byte` survives untouched. It pins
the ten-byte contract that the whole argument rests on, and it is now the
test that makes the deletion safe rather than merely a coverage guard.

Fifteen specs, `cov.284-298`, pin that the hand-rolled paths still lex every
deleted token: the keywords through their constructs, `[[`/`]]`, `+=`, `!`, and
— for `case`/`esac` and `<<<`, both retired surfaces — that they still lex well
enough for the diagnostic retiring them to fire at all. A lexer that stopped
recognising `case` would not report SB4029; it would report something
confusing about an unexpected word.

## A prediction that missed

I predicted +15 on the sweep and got +30. The collision check was clean and
all fifteen ids appeared under both `c:` and `native:`, so nothing had
collided or vanished — `spec/coverage.sh` is a **dual-mode** file and I had it
recorded as native-only. 15 × 2 = 30 exactly.

Worth writing down because the prediction discipline did its job in reverse
here: the number that did not match was not the tree, it was my model of the
tree. An unexplained sweep total is a defect either way, and the fix is the
same — go and find out which.

**5318 passing / 13 skipped / 0 failing. 83 units. `to_tok` cc 46 → 19,
cog 1 — delta −18, dispatcher-exempt under the gate DG ruled on yesterday.**
