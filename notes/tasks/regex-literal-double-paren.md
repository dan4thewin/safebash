---
status: ready
priority: P3
depends:
gates:
  - "`[[ $s =~ ^((a))$ ]]` either compiles or is rejected by a diagnostic that names the real cause"
---

# An unquoted regex containing `((` is rejected as "expected `]]`"

Measured 2026-09-13 while writing the manpage's regex section.

```sh
[[ $s =~ ^(a)$ ]]        # compiles
[[ $s =~ ^(a)(b)$ ]]     # compiles
[[ $s =~ ^(a(b))$ ]]     # compiles
[[ $s =~ ^((a))$ ]]      # error [SB4014] expected ']]'
[[ $s =~ ^((a)(b))$ ]]   # error [SB4014] expected ']]'
```

The trigger is exact: **a literal `((`**, i.e. an open paren immediately
followed by another. `^(a(b))$` has the same nesting depth as `^((a)b)$`
and compiles, because its two opens are not adjacent. The scanner takes
`((` as the arithmetic-command opener.

Both patterns are valid regexes and both work when quoted
(`=~ '^((a))$'`) or taken from a variable. So the language accepts the
construct; only one spelling of it is unreachable.

## Two separable defects

1. **The diagnostic misdiagnoses.** "expected `]]`" names a missing
   delimiter the input does not lack. This is the same shape as the
   SB5011 case fixed in journal 288: one error arm serving two
   populations. Even if the restriction stays, the message should say
   that `((` in an unquoted pattern is read as the arithmetic opener and
   point at quoting. That part is not a design question.

2. **Whether to accept it at all.** DD#63 says the pattern is "just an
   argument (a single word)", and single-word scanning is what makes `((`
   ambiguous. Accepting it means the `=~` right-hand side tracks paren
   depth like `scan_word_chars` already tracks `[`/`]` depth for
   subscripts — a small, local change, but it widens what a bare word may
   contain, and the ambiguity with `$(( ))` is real. **DG's call.**

## Documented meanwhile

`docs/sash.1`'s new `.SH REGULAR EXPRESSIONS` section states the trigger
and the workaround under "Quoting", so the behaviour is at least not a
surprise. That is a stopgap, not a resolution: the manpage currently
documents a misleading diagnostic as if it were a rule.
