# 331 — the typo guard that guessed

Bug #11, and like the four before it, the coverage loop did not find it. What
found it was trying to write a spec for a cold branch and noticing the answer
was wrong.

The branch was `validate.rs:7485`, inside `is_negative_int_literal` — a
two-operand `&&` whose FALSE arms were cold. Flipping them needs a literal
that starts with `-` but is not a well-formed negative: `$(arg -)` and
`$(arg -x)`. Both compiled. Neither is a valid index. So I ran them:

```
$ ./t one two          # x=$(arg -); attest x [[ -n $x ]]; echo "[$x]"
[./t]
```

Argument 0. The program name. Exit 0.

## The table that made it a bug rather than a curiosity

| written | actually read | exit |
|---|---|---|
| `$(arg abc)` | argument 0 — the program name | 0 |
| `$(arg -)` | argument 0 | 0 |
| `$(arg 1x)` | argument 1 | 0 |
| `$(arg 2abc)` | argument 2 | 0 |
| `$(arg +1)` | argument 1 | 0 |

The runtime parses the index by taking leading digits and discarding the rest.
`$(arg 1x)` is the one that matters: it looks like it worked, and for a
single-digit index it even returns the argument you meant. Mistype the second
digit and you silently read a different argument.

I did not have to decide whether this was acceptable, because the project had
already decided. Spec `av.t6`, written long before:

> `$(arg N)` for N >= argc aborts at runtime (no silent empty)

An out-of-range index refuses to guess. A malformed index was guessing. Same
intrinsic, same class of mistake, opposite behavior.

## The shape, again

`arg_index_diag` already existed. It already had a typo guard — for *negative*
literals, with a DD#60 citation. It simply had no guard for "is this a number
at all". Fifth time this session that a rule was implemented for the spelling
someone reported and left open for the rest of its class:

- DD#72's numeric-literal fix covered arithmetic context only (bug #7)
- DD#64's line continuation was implemented in two of three scanner contexts
  (bug #8), then in two of two but not at end-of-input (bug #9)
- `bareword_shortcut_expansion` covered a single-statement `$( )` body
  (bug #10)
- and now an index typo guard that checked the sign but not the digits

There is a pattern to where these live: **next to an existing check that
handles the neighbouring case.** Not in unwritten code — in code that already
demonstrates the author knew the rule. If I were looking for bug #12 without a
coverage report, that is where I would look.

## Fix

A second guard beside the first, ordered after it so `-1` keeps SB5038 and its
DD#60 wording, and restricted to literals so `$(arg $i)` stays a runtime
concern. That literal/computed split is not my invention — the comment above
`is_negative_int_literal` already draws it, in those words. DD#112, SB5078,
specs `av.t8`–`av.t8i`.

`av.t8g` pins the ordering (a negative literal still gets SB5038, not the
broader rule) and `av.t8h`/`av.t8i` pin the two things that must *not* change:
`01` is a fine index, and a computed one is untouched.

## Filed, not fixed

Same probe turned up `${a[-1]}`, which is rejected — correctly, there is no
negative indexing — but reported as a *string key*, with the advice "use an
integer". `is_string_key` classifies a literal as a string when it is not all
digits, so a leading `-` makes `-1` a string. The behavior is right and even
memory-safe (a negative subscript is a distinct key, not an offset; `-100000`
harms nothing). Only the explanation is wrong, so it is a P2 in
`notes/tasks/diag-negative-subscript-called-string-key.md` rather than a
stop-everything.

Worth separating those two outcomes deliberately: one probe session produced a
silent-wrong-answer bug and a merely-misleading message, and they are not the
same severity even though both came out of the same wrong-looking diagnostic.

## Coverage

`cov.217`/`cov.218` cover the two cold branches in
`check_cmdsub_value_intrinsic` — the function I added yesterday for bug #10,
which I should have covered when I wrote it. A value intrinsic *with*
arguments takes a different formatting arm, and a non-literal head has to be
declined.

Sweep 5059/10/0, exactly the predicted 5037 + 11 specs × 2 modes.

## A sweep that lost tests and reported green

Separately: the first sweep of the bug-#10 change reported 5001 passing, 0
failing, and had silently dropped an entire file. `run.sh` takes its scratch
TSV from `mktemp`, `/tmp` on this host is being cleaned aggressively, and
`set -eu` plus an EXIT trap turns a vanished TSV into a partial load with no
complaint. Nothing in the output said so.

The only thing that caught it was predicting the total first — 4983 + 9×2 +
8×2 = 5017, not 5001. Re-running under `TMPDIR=~/sashscratch/tmp` gave exactly
5017. AGENTS.md already says to predict the total; it frames it as catching
collisions. It also catches a sweep that lost tests rather than failed them,
which is a strictly harder failure to notice.
