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

## A flake that was not a flake

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

I wrote it up as unexplained, and filed a task saying the harness discards
failure evidence so the next occurrence would be equally undiagnosable.

**Both of those were wrong, and DG took three words to show it: "what about
the xtrace?"**

## The harness had already recorded it

`common.sh`'s `setup()` opens a per-file, per-mode trace and redirects the
whole spec file into it under `set -x`:

```sh
trace=.trace/$(date +%a)/"$id.$SASH_MODE"
exec 3>&1 >> "$trace" 2>&1
PS4='+$LINENO: '
set -x
```

`exec 3>&1` is exactly why `chk` writes `ok` / `not ok` to fd 3 — fd 1 is the
trace. Every command, expansion and assignment is there, line-numbered. The
file is appended to, so the failing sweep and the passing re-run were both
still sitting in `.trace/Wed/cli.c`; `spec.db` gave the timestamp
(`04:07:30Z`), which picked out the block.

I had concluded a facility was missing without reading the file that provides
it.

## What it actually was

```
+262: sr_err='sash: --sysroot=/tmp/tmp.nkqYLkSBp3/nosuch.squashfs: Could not open ...'
++263: grep -c SB sq.err
+263: [[ 1 = 0 ]]
```

`mktemp -d` had returned `/tmp/tmp.nkqYLkSBp3`. Ten random alphanumerics, and
these happened to contain the literal **`SB`**.

All five specs assert `$(grep -c SB <file>) = 0`. Every one of those messages
echoes the `--sysroot` path back, and the path is under this file's tmpdir. So
the grep counted the **directory name**, not a diagnostic code.

Every feature of the "flake" falls out of that: c mode only, because each file
*and mode* gets its own `mktemp -d` and native's name did not contain `SB`; all
five at once, because they share one dir; never reproducible, because the name
is fresh each run; once in 400+ runs, because the odds are about 0.2% — nine
adjacent positions at 1/62².

Not a transient. A latent test bug that had been in the suite since those specs
were written, firing roughly once every five hundred sweeps, and **rendering
identically to a real regression**.

Fixed by matching the full code shape, `SB[0-9]{4}`, which a random suffix
cannot fake. Reproduced deterministically first — recreate the directory by
name, watch the old assertion fail and the new one pass — because a fix for an
intermittent bug is worth nothing without a deterministic reproduction.

Every other `grep -c SB…` in the suite already names a complete code
(`SB5018`, `SB4030`), so the blast radius was exactly these five.

## Two lessons, neither the one I filed

1. **Read the harness before concluding it lacks something.** I proposed adding
   failure-context capture to a gate that has captured it all along.
2. **An assertion must not be able to match the scaffolding.** A bare `SB`
   tested against text that embeds a `mktemp` path is a test that fails at
   random, and the day it does, it looks like a regression.

The second is the more dangerous, because "unexplained, did not reproduce" is a
conclusion that closes an investigation. It almost closed this one.

