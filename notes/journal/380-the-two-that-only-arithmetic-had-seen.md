# 380 — The two that only arithmetic had seen

`try_nullary_intrinsic` lowers the argument-less intrinsics. Five arms:
`epochseconds`, `getpid`, `getcwd` — hot. `getppid` and `random` — cold.

Two neighbours in one `match`, identical in shape, and only two of them
uncovered. That asymmetry is the whole lead.

## Why only those two

Both *are* spec'd. `sx.3` is `$(( (getppid) > 0 ))`, `sx.8` and `sx.11` draw
`random` into an `int`, `cov.92` and `av.d7` pin its rejection as a
disposition default. Every one of those reaches the intrinsic through
**arithmetic**, which takes a different lowering than the string path —
`emit_int_to_str` — that these arms sit on.

So the gap was not "nobody thought about `random`". It was that a value used
as a number had been spec'd and the same value used as a *string* had not.
`getpid` and `epochseconds` were hot precisely because something did
`echo $(getpid)` somewhere.

## Probed, correct

`$(getppid)` returned `429698`. The shell I ran it from was PID `429698`.
That is the assertion you want from a probe: not "it printed digits" but a
number I can independently predict.

`$(random)`: 200 draws, 200 distinct, none negative, all 17+ digits. Both
backends agree. No bug.

## Specs that assert semantics, not shape

Nondeterminism is *why* these went unspec'd — there is no fixed value to
compare against. But "nondeterministic" is not "unassertable", and the
deterministic properties are the ones that catch a real regression:

- **`getppid` is stable within a run** (two calls, same answer) and **is not
  `getpid`**. A stable value that is nonetheless distinct from your own PID is
  a much sharper claim than "positive integer".
- **`random` varies** — two draws differ. This catches a stuck generator
  returning a constant, which is exactly what a broken CSPRNG read looks like
  and what "it printed digits" would happily accept.
- **`random` is non-negative.** I checked whether that was guaranteed before
  asserting it: `sash_random` ends `i64::from_ne_bytes(buf) & i64::MAX`,
  clearing the sign bit, per DG's 2026-07-07 range decision. 200/200
  non-negative draws would have been weak evidence on their own — an
  assertion that holds by luck is a flake waiting to happen. It holds by
  construction, so it is safe to pin.

`cov.331`–`cov.334`. Sweep 5823, zero failing, zero flips.

## The transferable bit

When a `match` has hot and cold arms side by side and the arms are
structurally identical, the difference is never in the code — it is in which
*context* the tests reached it through. Ask what the hot neighbours have that
the cold ones don't. Here it was one word: `echo`.
