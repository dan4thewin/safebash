---
title: "`[[ ]]` does not explain a comment that ate its `]]`"
status: todo
priority: P4
kind: diagnostic
depends: []
gates:
  - "must not fire for an ordinary missing `]]` with no comment on the line"
---

# A comment that eats `]]` gets no explanation

DD#94 makes comments trivia inside both `(( ))` and `[[ ]]`, and a comment
runs to end of line in both. So the closing delimiter must not share a
commented line. The two constructs report that very differently:

```
$ (( a = 1 # x ))
error [SB4020] unterminated arithmetic — expected `))`. Note a `#` comment
runs to the end of its line, so a `))` after one is part of the comment

$ if [[ $s == a # hi ]]; then echo yes; fi
error [SB4014] expected ']]'
```

The arithmetic message names the cause. The `[[ ]]` one leaves the reader to
work out that their `]]` is inside a comment — and it is not obvious: an
earlier draft of journal 391 got this rule wrong for `(( ))`, where the
message *does* explain it.

## Why this is not a one-line fix

Appending the hint unconditionally to `expect(… DblBracketClose …)` would put
comment advice on every ordinary missing-`]]`, which is the common case and
has nothing to do with comments. That is the SB4064 failure mode — advice that
is actively wrong for the input it is shown to (journal 389).

Doing it properly means knowing whether a comment was consumed on the current
line before the delimiter was expected, which the scanner does not currently
expose. Three call sites: `parser.rs:1200`, `:1333`, and `:1714` (attest).

Shields: `94.29` (the trap), `94.30` (the working newline form). They assert
SB4014 only, so adding a hint to the message will not break them.
