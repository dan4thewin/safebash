---
title: "`}` breaks an unquoted word — `/bin/echo a}b` is a compile error, bash prints `a}b`"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "`/bin/echo a}b` prints `a}b`, or `}` is rejected with a message that says why"
  - "`${s}` and `fn f { … }` are unaffected"
---

Found while fixing bug #7 (the mid-word `#`, DD#110), which had the same shape:
a byte listed in `scan_word_chars`' break set that should sometimes be part of
the word.

```
$ /bin/echo a}b      sash: error [SB4014] expected expression    bash: a}b
$ /bin/echo a}       sash: error [SB4014] expected expression    bash: a}
$ /bin/echo a{b      sash: error expected '}'                    bash: a{b
```

No `#` involved — this is independent of DD#110 and predates it.

Unlike the `#` case this is **not** silent: it is a compile error, so no binary
disagrees with bash. That is why it is P2 and not P1. But the message is wrong
twice over — `a{b` reports "expected `}`", which sends the author looking for a
missing brace that was never opened.

**This is a language question, not a scanner one.** The `#` fix had an obvious
right answer because the old behavior was silent data loss. Here the options are
real:

1. Make `{` and `}` ordinary word characters when they are not at a word start,
   matching bash. Cheap in `scan_word_chars`, mirrors DD#110's rule.
2. Keep rejecting, but with a message that names the actual problem ("`}` is
   reserved; quote it as `'}'` to use it as text").
3. Leave `{` alone (it opens a block after a command word) and only relax `}`.

Brace expansion (`{a,b}`) is a separate surface — check `spec/brace.sh` and
whatever DD covers it before touching this, since option 1 may collide.

DG decides.
