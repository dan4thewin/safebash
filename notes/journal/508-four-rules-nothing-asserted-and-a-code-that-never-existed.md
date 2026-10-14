# 508 — Four more rules nothing asserted, and a code that never existed

Journal 507 found one rule — SB5027 — that the compiler raised, the specs
reached, and nothing asserted. This session ran the same census over the rest
of the compiler. It found four more, two stale cross-references, and a
measurement pattern that was wrong twice before it was right.

## The census pattern was wrong twice

The first version scanned for the code as a quoted literal, `"SB[0-9]{4}"`.
That is how most diagnostics are written, so it looked complete. It is not:
**17 of 153 codes are interpolated into a hand-rolled format string** —
`format!("{}:{}: error [SB5055] …")` — and the quoted-literal pattern cannot
see any of them. A rule emitted that way, asserted by nothing, would never
appear in the census at all. This is the same blind spot AGENTS.md already
records for message *text*; it applies to the code as well.

The second version matched the code anywhere, which fixed the source side and
broke the spec side: a code named only in a `#` comment counted as an
assertion. Filtering comments on **both** sides is what surfaced the four
holes below. Unfiltered, the forward direction also carries a permanent
three-code baseline — retired codes mentioned in explanatory comments — which
is the kind of standing noise that trains you to ignore the output.

Running both directions matters independently. `comm -13` (asserted, raised by
nothing) is what exposed the format-string blind spot, and it is what found
SB4042.

## SB4042 never existed

`check_subscript_index`'s doc comment named "(SB4042, type_check)" as the rule
that catches a non-integer subscript. DD#80 lists SB4042 under *considered but
deferred*. It was never implemented. The real rule is SB4030, confirmed live:

```
array a=(1 2); x=${a["k"]}   →  SB4030
```

The comment had been asserting a fact about a sibling pass that was false when
it was written. Fixed to name SB4030 and to record that SB4042 was deferred,
so the next reader does not go looking for it.

## The compound-suffix family

`parse/parser.rs` rejects four suffixes on a compound command, all with the
same remedy ("wrap the compound in a function"):

| code | suffix |
|------|--------|
| SB4021 | redirect |
| SB4022 | pipe |
| SB4023 | `&&` / `\|\|` |
| SB4024 | `&` |

Only SB4022 was asserted. The other three were raised by live code and named
only in spec comments. Each reproduces as the sole diagnostic for its program,
so there was no masking here — simply nothing asserting them. Added `csfx.1`
–`csfx.3`.

The second stale cross-reference turned up in the same neighbourhood:
`errors.sh`'s `rdr.47` comment said a backgrounded compound is SB4022. It is
SB4024.

## SB4017, and the rival the reversion proof caught

SB4017 fires from `analyze_function_returns` when an `fn f -> int` returns
only literal zero. Two spellings share one arm — `return 0` and a bare
`return` — and the bare half was unreached.

I added positives for both. The reversion proof — stub all four rules, rebuild,
count the red — turned **six** specs red where I predicted five. The sixth was
`55.14`, which I had not realised was already SB4017's positive; it asserted
only `-n $err`, so it had never named the code. My `0.5p` was a verbatim rival
of it.

So the proof earned its keep twice over: it confirmed the four new assertions
bite, and it found a duplicate before I committed it. Dropped `0.5p`, kept
`0.5q` (the bare-`return` spelling, genuinely distinct), and tightened
`55.13`/`55.14` to name SB4016/SB4017 rather than merely requiring *some*
error. Final reversion: five red, against a baseline of one.

That is the general shape: **before writing a positive, stub the rule and see
what already fails.** A green run proves nothing about an assertion that was
never capable of failing.

## A flake I could not explain

The first sweep after these changes reported 8375 — exactly the predicted total
— with zero new-id surprises but **five flips**: `c:cli.31`, `32`, `40`, `41`,
`42`, all passing→failing.

Treated as stop-everything. Ruled out, in order: not a code regression (only a
doc comment changed, and the binary was verified still raising all four rules);
not mode-dependent test bodies (these call `$SASH` directly rather than
`run_sash`, so the native and c bodies are identical — yet only `c:` failed);
not historical flakiness (each had exactly one failing run against 400+);
not shared state (`common.sh` gives every file its own `mktemp -d`, and the
`EXIT` trap that removes it was tested and does **not** fire in subshells);
not disk or inode pressure.

It did not reproduce: standalone c-mode `cli.sh` passed 64/64, a 40-iteration
loop of `cli.31`'s exact body gave zero failures, and a second full sweep came
back 8375 with zero failures and the five flipped back.

I am leaving it unexplained rather than inventing a cause.

## The reason it stays unexplained

`chk` records a status and nothing else. On failure it prints `not ok <index>`
and discards `$out`, `$err`, and `$rerr`. For a reproducible failure that costs
nothing — you re-run it. For a transient it is fatal: the evidence is gone at
the moment it existed, and the only artefact is a row in `runs` saying
something was wrong once.

These five make it worse by asserting `$(grep -c SB f) = 0` alongside an exact
message match, a shape that cannot distinguish *wrong output* from *no output
at all*. A regression and a transient look identical.

Whether the harness should capture failure context is DG's call — it touches
the authoritative gate — so it is raised rather than changed.
