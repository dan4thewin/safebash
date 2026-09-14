# 316 — No meaningless redirects

`notes/tasks/bug-redirect-silently-dropped-on-noncommand.md` had been sitting
on DG's desk since journal 313. The finding: `wrap_with_redirects`'s non-empty
arm took 0 of 18,467 calls, and chasing why turned up a silent divergence.

```
$ cat t.sh
(( 1 + 1 )) > /tmp/f

$ bash t.sh   ; ls /tmp/f   →  /tmp/f          (created, empty)
$ ./t         ; ls /tmp/f   →  No such file
```

The parser built a correct `Redirected` node — `--emit-ir` showed it — and
`attach_redirects_to_last_inst` threw the list away, because only four SSA
inst kinds can carry redirects and an arithmetic test lowers to none of them.
The `_ => {}` did what that arm always does: handled a case it could not
handle, by doing nothing.

The fork was reject-vs-implement, and it was a language-surface question, so
it waited. **DG: no meaningless redirects — should throw an error.**

## What that means precisely

bash's behaviour here is to create and truncate the file and then write
nothing into it, because `(( … ))` and `[[ … ]]` produce no output. That is a
wart, not a feature, and sash already declines to follow bash into this corner
twice — SB4021 for compounds, SB4046 for declarations, assignments and
function definitions. So the fix is not new machinery, it is two more nouns in
a list that already exists:

```rust
StmtKind::ArithTest(_) => Some("an arithmetic test"),
StmtKind::Command(c) if is_bracket_test(c) => Some("a `[[ … ]]` test"),
```

`[[ … ]]` needed the predicate because `parse_test_command` builds it as a
*command* whose name is the literal `[[` — so it is `StmtKind::Command` and
would otherwise have looked like a perfectly good redirect target.

The message tail grew four words: "redirects attach to a command **that
produces output**". The old wording was accurate for a function definition but
would have read oddly against `[[ … ]]`, which is a command in every structural
sense and still cannot take one.

## What the five forms had in common

The task's table listed five spellings, and the useful discovery while fixing
it was that they are all one case. `--emit-ir` shows the wrapping runs the
other way round from what the spellings suggest:

```
- (( 1 + 1 )) > f     →  suppressed { redirected { arith-test } }
return (( 1 + 1 )) > f →  return-producer { redirected { arith-test } }
```

The `-` and the `return` wrap the *redirect*, not the other way round, so the
redirect always lands directly on the arithmetic test. One match arm covers
all five. No recursion through `Suppressed`/`Negated`/`ReturnProducer` was
needed, and writing that recursion — which was the obvious first move — would
have been dead code.

## One diagnostic moved

A bare `[[ -f /etc/passwd ]] > f` used to report SB4018 (unused result) and
now reports SB4046, because the redirect is caught at parse time and SB4018 is
a later rule. That is the better error of the two: the redirect is the
specific mistake, and removing it still leaves SB4018 waiting. `rdr.6` and
`rdr.7` pin both halves so neither rule can quietly swallow the other.

Ten specs, `rdr.1`–`rdr.10`: the five forms, the bare-test pair, an assertion
that the file is never created, and two controls proving a redirect on a real
command and inside an `if` body are untouched. Sweep 4691 passing, 0 failing.

The task file is deleted. It was the last item gated on DG from the coverage
campaign.
