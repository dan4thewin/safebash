# 330 — an exemption wider than its justification

Fourth bug out of the branch-coverage loop, and the fourth one where the
coverage measurement was not what found it. What found it, again, was
*collecting the behavior to write the spec with*.

The branch was `parse/expr.rs:678` — the `let [stmt] = stmts else { return
None }` that opens `bareword_shortcut_expansion`. Cold. To flip it I needed a
command substitution whose body is not exactly one statement, so I typed the
first thing that came to mind:

```sh
/bin/echo "$(echo hi; status)"
```

It compiled. Then it printed:

```
sash: bug: bare command passed to sash_exec (expected absolute path)
./t: status: command failed (exit 127)
```

into the program's own output, and exited 0.

## The comment that describes the bug

`bareword_shortcut_expansion`'s doc comment, eight lines above the branch I was
probing:

> the substitution survived as a `Command` no backend recognises as a builtin,
> was dispatched as an external, and the runtime's "bare command passed to
> `sash_exec`" complaint was substituted into the program's output with a 0
> exit. Every compile-time check passed, because `status` really is a
> registered builtin.

That is a description of what I had just reproduced, written by whoever fixed
it the first time. The fix handled `$(status)`. It did not handle `$(echo hi;
status)`, because it opens with `let [stmt] = stmts`.

This is the third time this session — DD#72's numeric-literal fix that only
covered arithmetic context, DD#64's line continuation implemented in two of
three scanner contexts, and now this. **A fix aimed at a reported spelling
rather than at the rule leaves the rest of the rule broken, and leaves behind a
comment claiming otherwise.** The comment is the tell. If it describes a class
of failure and the code handles an instance, look for the rest of the class.

## The part that made it DG's call

The validator would normally have caught it. SB5073 exists, fires for a bare
`status` at top level and in a pipeline, and its message states the rule
precisely. It was suppressed inside `$( )` — deliberately, with a comment
saying so:

> Only the head-must-be-literal rule carries over, NOT the rest of
> `check_command_head`: a value intrinsic is SB5073 in statement position and
> is exactly the right thing inside `$( )`.

So the fix meant narrowing something documented as intentional, which is the
one case AGENTS.md says to ask about regardless of how obvious it looks. I
filed it and asked.

The shape of the answer is worth recording: DG's first response was *"make this
concrete — what is output now, and what changes with the recommendation?"* I
had written two paragraphs of mechanism and one table row. What actually
settled it was five rows of real output, including the row that showed
`$(echo hi; echo $?)` already prints `hi` / `0` correctly. Once that was on
screen the "widen the rewrite" option was visibly redundant — it would add a
second spelling for something that already had one. **A design question between
two plausible options is usually decided by output, not by argument.** I should
have run the table before writing the prose.

## The framing I had backwards

I kept writing "`$(status)` is sugar for `$?`". DG corrected it, twice:

> you keep saying $(status) is sugar for $? when I keep saying we decided that
> variables aren't magic and only appear that way when they are sugar for
> intrinsics like status, getpid, etc.

It is the other way round. `status` is the intrinsic — the real thing. `$?` is
punctuation that reads it. sash has no special parameters; it has intrinsics,
and a little punctuation that happens to look like bash's magic variables.

The codebase already knew this. SB5012's message reads "*the punctuation sugar
`$?` reads the same value*". I had read that message during this very
investigation and still inverted it, because bash's framing is the one my hands
type. Worth flagging: when the project has redefined a relationship the rest of
the world states in the opposite direction, the old framing leaks back in
through phrasing even when the work is correct.

## The fix

The exemption is now exactly as wide as the rewrite that justifies it: a value
intrinsic is exempt only when it is the lone top-level statement of the
substitution body. `check_cmdsub_heads` computes `stmts.len() == 1` and threads
it down one level; child bodies always pass `false`, because an `if` arm inside
a substitution is length-1 at every level and still nothing rewrites it.

Same diagnostic code, different wording. The statement-position message says
the intrinsic is "only valid inside a command substitution" — true, and
actively misleading for a use that *is* inside one. The new message names the
constraint that was actually violated.

Eight specs, `23.25`–`23.32`, sitting directly under `23.24`, which pins the
exemption and still passes. That adjacency is deliberate: the two specs
together say "this far and no further", which is more durable than either
alone.

DD#111. And `cov.204`, the coverage spec for the branch that started all this,
uses `"$()"` — an empty body flips the same branch without encoding the bug.
