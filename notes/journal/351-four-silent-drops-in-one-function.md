# 351 — Four silent drops in one function

## How it surfaced

Coverage work, not a bug hunt. The functions queue had five entries where
every line was cold; three were ICE guards (kept, by standing rule), and one
was `transform/expressions.rs:opaque_form_to_string` — the helper that renders
`$(...)`, `$(( ... ))` and `[[ ... ]]` as placeholder text.

Its caller, `expr_to_string`, runs **14574 times** and has six cold arms.
Every caller passes a `Literal`. So the question was whether a non-literal can
reach it at all, and the cheapest way to answer that was to try one per call
site rather than read twenty-one of them.

`str c=/bin/echo; $c hi` → SB5011, as it should be. `array c=(…); ${c[0]} hi`
→ SB5011. Then:

```
$ str s=TERM; signal $s { echo got }
got
```

That printed. It was not supposed to print anything.

## The first reading was wrong

It looked like a signal handler firing with no signal delivered. It was not:
the real form is `signal <num> <disposition>` (`signal 2 IGN`), so my line
parsed as the command `signal TERM` followed by an inline group `{ echo got }`
that ran on the spot. My syntax was wrong, and the `got` was mine.

The interesting half was the other one. **`signal TERM` compiled.** So did
every other malformed spelling I could write:

```
signal                    → accepted, no-op
signal 2                  → accepted, no-op
signal TERM               → accepted, no-op
signal BOGUS IGN          → accepted, no-op
signal 2 BOGUS            → accepted, no-op
signal 2 IGN extra        → accepted, no-op
signal -1 IGN             → accepted, no-op
signal 9 IGN              → emitted a syscall that cannot succeed
signal 99 IGN             → emitted a syscall that cannot succeed
int n=2; signal $n EXIT   → accepted, no-op
```

`signal` had no entry in `validate.rs` or `type_check.rs`. The only mention of
it in the validator was **SB4043**, the rule that rejects `trap` — whose
message tells the user to write `signal N IGN|DFL|FLAG|EXIT`. The compiler
pointed at a form and then accepted every wrong spelling of it in silence.

## Four gates, one function

`handle_signal` (`transform/cmd_handlers.rs`) was 48 lines and dropped input
at four separate points:

```rust
if c.args.len() >= 2 {                      // <2 args: fall through
    let signum: i64 = sig_str.parse().unwrap_or(0);   // non-numeric → 0
    if signum > 0 {                         // 0 or negative: fall through
        match disposition.as_str() {
            …
            _ => {}                         // unknown disposition: nothing
        }
    }
}
```

Each is individually defensible and collectively they mean *nothing a user can
type is an error*. `>= 2` also silently ignores extra arguments. Worth noting
that none of these is a missing branch — the shape 344/347/348/350 kept
producing. This is the opposite shape: **branches that exist precisely to do
nothing**, which is why no omission detector would have flagged it and why the
`_ => {}` reads as ordinary defensive code.

The runtime had a fifth. `sash_trap` and `sash_signal_flag` both end

```rust
if ret < 0 { -1 } else { 0 }
```

and the generated code discards the result in both backends. A rejected
`rt_sigaction` left the program running without a disposition it believed it
had.

## What DG settled

DD#56 already fixed the surface — a signal **number** plus one of four
dispositions, and "the compiler does not know signal names" — so *rejecting*
was not a question. Three things were genuinely open, and DG took all three:

- **The "Supported signals" table is illustrative, not a whitelist.** Accept
  1-64, less 9 (KILL) and 19 (STOP). The test is whether the kernel will
  honour the registration, not whether the converter emits it.
- **A non-literal number is rejected for all four dispositions.** `EXIT` has
  no choice: `exiting_signals` is a compile-time set. Allowing expansions for
  the other three would have split the surface to no purpose.
- **The runtime aborts on a rejected `rt_sigaction`.** After the compile-time
  rules it should be unreachable — which is the reason to report it, not the
  reason to swallow it.

## The fix

Four rules in `validate.rs`, next to `check_trap` where the misleading message
lives: **SB4059** arity, **SB4060** non-literal or non-numeric number,
**SB4061** uncatchable or out-of-range number, **SB4062** unknown disposition.
One condition per code, each pointing at the offending argument's own span.

The diagnostic nearly shipped with a bug of its own. The first version read
``write the number, or give it a GPP macro (`#define {text} 15`)`` — correct
by accident for `signal TERM IGN`, since TERM really is 15, and wrong for
every other name. Pinned to a fixed correct example.

With the shape guaranteed, all four gates in `handle_signal` came out. The
two wrapper levels went with them, and the function fell from **cc 8 / cog 6**
to **cc 6 / cog 1** — the nesting was essentially all of what made it read as
complicated, and removing it cost only two lines, since the comment explaining
what the gates used to hide is nearly as long as the gates were. The
unknown-disposition arm is now an `unreachable!` naming the rule that
guarantees it, per the ICE exception to the purge rule.

## Verification

Twelve specs, `56.40`-`56.51` — ten rejections, one non-literal disposition,
and one that compiles all four valid dispositions together to guard the new
rules against over-reach. Dual-mode file, so +24. Predicted 5378, got 5378,
0 failing. Units 83/83.

`plan=` needed correcting from 39 to 40 before the run: I wrote twelve specs
and counted eleven. A wrong `plan` does not fail the file, it prints a note —
the predicted-total discipline is what catches it, and it is the second time
this stretch that predicting first has paid for itself.

## What this says about the method

The bug was found by asking a *coverage* question — "can a non-literal reach
this arm?" — and answering it by execution rather than by reading call sites.
Journal 349 was found the same way, and in both cases the reasoning-first
version of the answer was wrong. Six cold arms in `expr_to_string` and the
whole of `opaque_form_to_string` are still cold; the probe that was meant to
classify them turned up an unrelated defect two layers down before it got
there. Those arms remain the open item.
