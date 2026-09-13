# 294 — Two documented limitations, one of which was lying

Both items in this entry came out of the same observation: the man-page
section I wrote in journal 292 describes what the compiler *does*, and in two
places what it does is worth writing down only if you also say it is a
boundary rather than a rule.

## The NUL limitation had a gate nobody had closed

`limitation-regex-stops-at-interior-nul` was decided by DG on 2026-09-10 —
YAGNI, document it, do not fix it. That is a perfectly good outcome for a
limitation that nothing is blocked on. But the task carried a gate:

> the limitation is stated in the DD#63 regex documentation

and it was not stated anywhere. I wrote the regex man-page section three days
later without that task in view, so the section shipped covering every
*rejected* construct and omitting the one that silently returns a wrong
answer.

Re-measured before writing anything, because filings rot:

```
strlen "a\0bcd"       → 5      correct; the scalar is byte-transparent
[[ $s =~ cd ]]        → false  the bytes are right there
$(gsub $s "cd" "Z")   → unchanged
```

Still live, still silent. The filing's own repro no longer compiled — it used
`$(len s)`, and `len` became collection-only under DD#75 — which is the third
time this month a task's example has aged out from under it.

It is now in two places: a DD#63 addendum with the runtime detail (the length
survives the `sash-rt` boundary intact; `regcomp9`/`regexec9` take a `char *`
with no length parameter, so the truncation is one level below anything sash
owns) and a man-page subsection that puts it next to the POSIX bracket
classes, since those two are the only things in the whole regex surface that
fail *quietly*. Everything else is SB5023.

## The man page was documenting a lie

While closing the double-paren task I reread what 292 had written under
Quoting:

> `=~ ^((a))$` is rejected with "expected `]]`"

That sentence is accurate and also embarrassing. The input is not missing a
`]]`. The `]]` is right there. What happened is that the lexer took `((` as
the arithmetic opener, so the pattern word ended two characters in, and by the
time `parse_test_command` asked for a closing delimiter the scanner was
staring at an arithmetic token.

I had documented the diagnostic's *text* as if it were the language's rule,
which is exactly the failure the writeup gate was supposed to prevent, done by
the person who had just written the gate.

The fix is four lines in `parse_binary_rhs`: after the `=~` operand word, if
the scanner is sitting on `Tok::DblParen`, say what actually happened.

```
error [SB4014] `((` in an unquoted regex is read as the arithmetic opener
`$(( … ))`, so the pattern ends before it — quote the pattern
(`=~ '^((a))$'`) or separate the parens (`^(a(b))$`)
```

Both the plain test and the `[[ … { … } ]]` dispatch form go through
`parse_binary_rhs`, so both improved from the one edit. I had expected to need
a second arm for dispatch and was pleased to be wrong; that is what the DD#71
decision to make the `=~` RHS an ordinary operand word bought, and it is the
first time I have seen it pay.

This is the same shape as the SB5011 fix in journal 288 and the SB4046 one
before that: **one error arm serving two populations**, where the message was
written for the population that arrives more often. The tell is a diagnostic
that names something the input demonstrably has.

## What I did not do

The restriction itself stands. Accepting `^((a))$` unquoted means the `=~`
right-hand side has to track paren depth the way `scan_word_chars` already
tracks bracket depth for subscripts. That is small and local, and it is also a
decision about what a bare word may contain, against a genuine ambiguity with
`$(( ))`. DD#63 says the pattern is "just an argument"; widening what an
argument is, is not mine to do.

It is filed, and the fix above makes leaving it in place cheaper — a
restriction that announces itself and names the workaround costs the user one
reading, where a restriction that misdiagnoses costs them an investigation.

## The common thread

A limitation is only a limitation once it is written down. Before that it is a
bug that happens to be known to one person. Both items here were "known" —
one in a task file marked decided, one in a man page — and both were still
going to cost someone an afternoon, because what was written down was either
in the wrong place or not true.
