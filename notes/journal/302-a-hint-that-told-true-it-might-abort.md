title: 302 — a hint that told `true` it might abort

`sash-convert-check-comment-true-false` was next. DD#69's addendum
externalized `true`/`false` — they resolve to `/usr/bin/true` and
`/usr/bin/false` under DD#23, not to inline non-forking builtins — and the
task asked for a `# CHECK:` hint so a bash author converting a script knows.

## The filing was right

Worth saying plainly, because the last three were not. Every claim held when
I put it against the live binary:

- A bare `false` aborts. `/tmp/t.bin: /usr/bin/false: command failed (exit 1)`,
  and the `echo` after it never runs. Bash would have set `$?` and carried on.
- `false || echo fallback` does *not* abort — prints `fallback`, exits 0. The
  status is consumed, so DD#55 has nothing to complain about.
- `while true` forks. I timed it rather than asserting it: **11.1 seconds for
  20,000 iterations**, about 0.55 ms a pass, against a bash builtin that costs
  nothing. That is a real consequence and not a theoretical one.

Re-measuring a filing that turns out to be correct still pays for itself. The
timing number is now in the hint text, and I would not have had it otherwise.

## Two hazards, two messages

The gate said "a `# CHECK:` hint". I emitted two, because the failure modes
are not the same size:

- **`false` with its status discarded** — the program *ends*. Semantic,
  surprising, and the thing you actually need to be told.
- **everything else** — the meaning is exactly bash's; it just costs a fork.

One undifferentiated warning on both would bury the first under the second,
and the second fires far more often (`while true` is everywhere). So the
abort wording is reserved for the case that can abort.

Deciding *which* case that is, is the whole implementation.
`status_is_consumed` walks out through `list` (`&&`/`||`),
`negated_command`, `command_substitution`, `pipeline`, and — the part that
matters — the **condition field** of `if`/`while`/`until`/`elif`, not those
statements wholesale. A command in a loop *body* is in discarded-status
position like any other statement. Matching on the node kind alone would have
told every `false` inside a `while` body that it was safe, which is precisely
backwards.

## I shipped the bug I was warning about, briefly

First version, on input `true`:

> CHECK: sash runs `true` as /usr/bin/true (externalized, DD#23) and its
> status is discarded here — so a bare `false` ABORTS the program…

A bare `true` cannot abort. I had branched on *position* and interpolated the
*name*, so the two came apart the moment they disagreed. The abort wording is
now gated on the command actually being `false`, and a bare `true` gets the
fork message — it is a no-op that costs a fork, which is the true statement
about it.

This is a small instance of the thing journal 301 spent itself on: a message
is a string, nothing checks it, and it can say something false about the very
program it is attached to. I caught this one only because I read the output of
my own battery instead of checking that the hint appeared.

## Verification

Specs 57.141–57.147, including a control asserting the hint does **not** fire
on the word `true` inside a string or in a comment — free, as it turns out,
because the hint hangs off `convert_command`'s `name` field and only real
command words ever reach it.

Fail-first predicted 57.141–57.146 and observed exactly those six. 57.147 was
predicted to pass *without* the change, since it asserts an absence, and did.

Sweep: **4500 passing, 9 skipped, 0 failing, 0 flips.** Predicted 4500
(4493 + 7 native-only) before running.
