# 327 — half a fix, scoped to the place it was found

The branch-coverage loop pointed at `parse/lexer.rs:432` and I went to write
a spec for it. The line is DD#72's arith guard, and its own comment says what
it prevents:

> Without this the `#` would start a comment and silently truncate the literal
> to `16`.

The probe said the branch never ran for `/bin/echo 16#ff`. Which is correct —
the guard is scoped to `arith_depth > 0`. Outside arithmetic there was nothing.
`/bin/echo 16#ff` printed `16`. `/bin/echo a#b` printed `a`. `/bin/echo ab#cd ef`
printed `ab` and threw the rest of the line away as a comment.

Bug #7, and the seventh found this way.

## The shape of it

`scan_word_chars` listed `b'#'` in its break set, and the byte dispatcher routed
a leading `#` to logos' `#[regex(r"#[^\n]*")]`. Two sites, one rule, and the
2026-07-05 fix touched neither — it added a guard at the *arith* entry instead.
That is the failure worth naming: the fix was scoped to the place the bug was
found rather than to the rule it violated. `notes/decisions/DD72` now says so.

## Two holes, not one

The first candidate was one line: break on `#` only when `arith_depth > 0`.
It fixed the word cases and the suite stayed at 4923. Then I checked the
expansion-adjacent forms and they were still wrong — `$s#y`, `${s}#y`,
`$((1+1))#z`, `$(p)#z` all truncated, because the dispatcher never asks whether
the `#` is mid-word.

The bash-faithful repair is a word-boundary model: a `#` is a comment only when
it begins a token. That needs a produce-time "previous token was word-like"
flag, because `last_end` is updated at consume time and goes stale across the
peek buffer. And it is wrong anyway: `$( … )` closes on an `RParen`, which is
not word-like, so `$(p)#z` would still break.

I brought DG three options. DG proposed a fourth: **require a space before the
`#`.** Look at `source[pos-1]`. No scanner state, no peek-ordering problem, and
it covers every expansion case for free. All fifteen shapes I had measured
matched bash, including the ones the one-line fix left broken.

That is the second thing worth naming. I had been reasoning inside the
constraint "match bash", and the state I needed was expensive because bash's
rule is stated in terms of tokens. DG changed the rule instead of paying for
the state. The new rule is *stricter* than bash's — it accepts a subset — so it
diverges only in the safe direction.

## The divergence, made loud

bash also comments after an operator: `echo a;# c` prints `a`. Under the new
rule sash reads `#` as a word, and the author would have gotten
`SB5011 command '#' not found`, which explains nothing.

DG ruled (b): give it a diagnostic. The scanner records the first `#` glued to
one of `; & | < > (` and `parse` reports SB4058 first and alone, on the same
precedent as the `unterminated` field — once the tokenization is wrong, every
later diagnostic is a consequence.

`)` and `}` are deliberately out of that set. They close an expansion far more
often than they end an operator, and bash agrees that `${s}#y` is one word.

DD#110 records the rule. `spec/comments.sh` cmt.12–cmt.26: eleven shapes that
are now byte-identical to bash, three SB4058 rejections, and a guard that
DD#72's SB4035 still fires for `(( n = 16#ff ))`.

## Found on the way, not fixed

`/bin/echo a}b` is a compile error ("expected expression"); bash prints `a}b`.
No `#` involved — `}` is in `scan_word_chars`' break set the same way `#` was.
It is a separate bug with the same shape, filed as
`notes/tasks/bug-brace-breaks-unquoted-word.md` rather than folded into this
commit. Whether a bare `}` should be a word character at all is a language
question, not a scanner one, and it is DG's.
