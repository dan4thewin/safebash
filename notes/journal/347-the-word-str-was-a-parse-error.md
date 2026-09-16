# 347 — The word `str` was a parse error

`keyword_to_str` was next on the branch-coverage list: 15 cold lines, cc 23 /
cog 1. Journal 346 had just finished proving 27 lexer variants unreachable, so
the obvious guess was that this was the same shape again — another dispatcher
whose inputs were constrained.

I wrote the opposite into `plan.md` before looking, and it is the reason this
was found:

> `Tok` (the real token type, produced by `classify_word_token`) is **live**
> where `LogosTok` was not — so a cold arm here is more likely to be a
> genuinely untested path than an unproducible one. Do not assume symmetry.

## What the call graph showed

One caller: `literal_token_text` (`expr.rs:170`). Its guard was a 21-token
alternation listing exactly the tokens `keyword_to_str` maps:

```rust
Tok::If | Tok::Then | … | Tok::End => Some(keyword_to_str(tok).to_string()),
_ => absorb_text(tok, mode).map(str::to_string),
```

Two lists of the same set, in the same file, forty lines apart. By now that
shape has its own reflex — journal 344 was two bugs of it, journal 346 was a
third instance that had not yet become one. So I counted.

`lexer::classify_word_token` promotes **22** spellings. Both lists held **21**.
`Tok::Str` was in neither.

## The bug

`str` is a reserved word in sash. Every other reserved word can be used as a
bare word, which is what `literal_token_text` exists for — its own doc comment
says "reserved words used as bare arguments". `Tok::Str` fell through to
`absorb_text`, which returns `None` for it in all three modes. So the token
terminated the word and the parser reported "expected expression".

```
echo int      →  int
echo str      →  error [SB4014] expected expression
```

Not confined to arguments — it broke every word context:

| context | before |
|---|---|
| `echo str` | SB4014 |
| `str s=str` | SB4014 |
| `[[ str == str ]]` | SB4014 |
| `for x in str` | SB4014 |

Only the exact word. `echo str2` and `echo strs` were always fine, because
promotion is word-boundary exact. That is what let it survive: `str` appears
constantly in the suite and in every doc example as a *declaration*, which is
handled at `parser.rs:231` long before word parsing, and essentially never as
a piece of data.

## The fix, and not repeating it

Adding `Tok::Str => "str"` to both lists would have fixed the symptom and left
the cause — two lists that must agree, with nothing making them.

`keyword_to_str` is now the only list. It returns `Option<&'static str>`, so
`literal_token_text` collapses to:

```rust
Tok::Eq => Some("=".to_string()),
Tok::Dash => Some("-".to_string()),
_ => keyword_to_str(tok)
    .or_else(|| absorb_text(tok, mode))
    .map(str::to_string),
```

The `_ => ""` sentinel is gone with it. That arm was unreachable under the old
caller's guard and would have returned an *empty word* had anything reached it
— a keyword added to one list but not the other would have produced a silently
empty argument rather than an error. The `Option` makes that state
unrepresentable instead of merely unlikely.

Two lists remain in principle — this one and `classify_word_token`'s promotion
table — but they are not interchangeable: one maps spelling→token, the other
token→spelling. cov.299-301 pin the round trip across all 22 spellings, so a
future addition to either that misses the other fails a spec.

## What the coverage number was actually telling me

15 cold lines in a 21-arm dispatcher meant six spellings had ever been used as
bare words and fifteen had not. That is not a reachability constraint; it is a
gap in the suite, and the gap was hiding a token that could not be used as a
bare word *at all*. The cold count could not distinguish "untested" from
"impossible" — only reading the caller could.

Worth stating plainly, because journal 346 drew the opposite conclusion from a
superficially identical signal, and the difference was not cleverness. It was
declining to reuse yesterday's answer.

## Tally

Specs `cov.299-305`: the 22 spellings in three sweeps, the three broken
non-argument contexts, and one guard that a reserved word remaining usable as a
bare word does not stop it introducing its own construct (`if (( 1 )); then
echo if; fi`) — the regression the fix could plausibly have caused.

Coverage baseline refreshed before the fix: **95.85% lines / 89.63% branches**,
up from 95.52 / 89.53 at the last measurement four commits earlier.

**5332 passing / 13 skipped / 0 failing, predicted exactly. 83 units.**
