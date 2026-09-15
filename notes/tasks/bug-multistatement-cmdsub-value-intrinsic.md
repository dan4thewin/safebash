---
title: "`$( )` with more than one statement leaks `sash: bug: bare command passed to sash_exec` for `status`/`getcpid`"
status: open
priority: P1
kind: bug
depends: []
gates:
  - "`/bin/echo \"$(echo hi; status)\"` either works or is a compile error — it must not emit `sash: bug:` into program output"
  - "single-statement `$(status)` / `$(getcpid)` keep working unchanged"
---

```sh
/bin/echo "$(echo hi; status)"
```

compiles clean and prints:

```
sash: bug: bare command passed to sash_exec (expected absolute path)
./t: status: command failed (exit 127)
```

Same for `getcpid`, and for any order (`$(status; echo hi)`).

## Why it happens

`bareword_shortcut_expansion` (`parse/expr.rs:677`) rewrites `$(status)` to
`$?`, and its doc comment describes precisely this failure as the thing it
exists to prevent:

> the substitution survived as a `Command` no backend recognises as a builtin,
> was dispatched as an external, and the runtime's "bare command passed to
> `sash_exec`" complaint was substituted into the program's output with a 0
> exit. Every compile-time check passed, because `status` really is a
> registered builtin.

But it opens with `let [stmt] = stmts else { return None }` — it only rewrites
a **single-statement** body. Two statements and the original bug is back.

The validator would normally catch it: `check_command_head` raises SB5073
("`status` is a value intrinsic … only valid inside a command substitution")
for a bare `status` in statement position, and that fires correctly at top
level and in a pipeline. It is **deliberately suppressed** inside `$( )` —
`check_cmd_in_stmt` (`validate.rs:2696`) says:

> Only the head-must-be-literal rule carries over, NOT the rest of
> `check_command_head`: a value intrinsic is SB5073 in statement position and
> is exactly the right thing inside `$( )`.

That exemption is right for `$(status)` and too broad for `$(echo hi; status)`.
It exempts the intrinsic **anywhere in the body**, while the rewrite that makes
the exemption safe only covers it as the **whole** body.

## Why it needs DG

The exemption is deliberate and documented, so narrowing it is a change to
something intentional. Two shapes:

1. **Narrow the exemption to match the rewrite** — a value intrinsic is exempt
   only when it is the sole statement of the body. `$(echo hi; status)` becomes
   SB5073, and the existing message already tells the author what to write.
   Small, conservative, and makes the exemption exactly as wide as the thing
   that justifies it.
2. **Widen the rewrite** — make a bare `status` statement anywhere in a body
   lower to something that emits `$?`, so `$(echo hi; status)` prints `hi\n0`
   like the bash spelling `$(echo hi; echo $?)`. More useful, but it invents a
   statement-position meaning for an intrinsic the language currently defines
   as value-only, which reads like a language change.

(1) matches the existing SB5073 message and the "value intrinsic" framing.
(2) contradicts it.

## Notes

Only bodies whose other statements do not taint reach this: with a real
external command (`$(/bin/true; status)`) SB5020 rejects first. Builtin-only
bodies (`echo`, `:`) are the live path.

Found while probing `parse/expr.rs:678` for branch coverage. The coverage spec
for that branch (cov.204) deliberately uses `"$()"` instead, so it does not
encode the buggy behavior.
