title: bug: `$(int x=1)` is reported as an unterminated `$(`
status: next
priority: P3
kind: bug
depends: []

A declaration inside a command substitution is rejected — correctly — but
with a diagnostic that describes a different problem:

```
$ echo '/bin/echo $(int x=1)' | sash /dev/stdin
error [SB4057] unterminated `$(` — it is never closed before the end of
the source, so everything after it was read as part of it
```

The `)` is right there. What actually happens: `collect_cmd_sub_body`
hands the body to `parse_statement`, which parses `int x=1` as a
declaration and consumes through the `)` looking for the rest of it. The
loop then runs out of input with `closed` still `false`, and the caller
turns that into `unterminated_cmd_sub_diag`.

Verified pre-existing on the unquoted path at `8f7052a`, before the
quoted/unquoted parser convergence (journal 422). The convergence made the
quoted spelling agree with it rather than compiling silently, which is a
strict improvement, but it also doubled the number of spellings that
produce the wrong message.

Not a soundness issue — the program is rejected either way. It is a
diagnostic-quality issue, and a sharp one, because the message actively
misdirects: it tells the author to go looking for a missing paren.

The fix is presumably to detect a declaration as the first statement of a
`$( )` body and say so ("a declaration is not a command; `$( )` captures
the output of a command"). Worth checking what other statement kinds reach
the same end — `collect_cmd_sub_body`'s `closed == false` exit is the only
route to SB4057, so anything that over-consumes lands here.

A spec should pin whichever message is chosen; there is none today.
