# 470 — The counter that went negative

Twenty-eight specs, thirty-three cold lines, one wrong comment, and a
measurement fault that can make a warm line read cold.

## The contradiction

The cold-line queue listed `sash/src/lib.rs:216` — `set_sysroot(path.to_string())`,
the body of `apply_and_check_flags`'s `--sysroot=` arm. That should have been
impossible. `spec/cli.sh` passes `--sysroot=` three times, one of them
(`cli.6`) a compile that succeeds, and `test_status` says all three pass.

Everything cheap said the spec was running. A PATH wrapper logging every
invocation confirmed 45 sash processes, three of them carrying `--sysroot=`.
The binary was the instrumented one. Forty-five processes wrote forty-five
profraws, none lost. And running `cli.6`'s exact command by hand, under the
same instrumentation, warmed line 216 with a count of 1.

Then merging all forty-five reported **zero**.

## The finding

Merge the profraws one at a time and the count walks downward:

| profraws | `DA:216` |
|---|---|
| 1 (just cli.6) | 1 |
| 17 | 1 |
| 18 | 0 |
| 19 | 18446744073709551615 |
| 20 | 18446744073709551614 |

That is not a small number. `18446744073709551614` is **−2** in an unsigned
64-bit counter. The three profraws that each subtracted one were
`--heap-size=abc`, `--heap-size=0` and `--heap-size=99999999999999999999` —
the three operands `parse_heap_size` rejects by calling `std::process::exit`.

LLVM does not give every region a physical counter. It allocates the minimum
and derives the rest as *expressions* over the ones it kept, and a loop body's
count is routinely derived from the back-edge. A process that exits from
inside the loop increments the counters on the way in and never the one on the
way round, so its contribution to the derived expression is inconsistent.
Summed across runs the error accumulates, and once it exceeds the true count
the line reports zero. A line that is genuinely warm reads cold.

All twenty-two `process::exit` sites in the compiler are in `lib.rs`, and
nearly all of them fire at top level after a pass has returned, which is why
this is rare rather than pervasive. `parse_heap_size` is the exception: it
exits with `apply_and_check_flags` still on the stack, mid-loop.

## How big is it

Measured, not estimated. Exporting each of the forty-five profraws separately
and taking the union of warm lines gives the set of lines *some* run executed;
intersecting that with the lines the merged report calls cold names every line
the merge got wrong. Against the real `target/coverage/union.lcov`, the whole
470-line cold cohort, the answer is **one line**: `lib.rs:216`.

So the queue was very nearly right, and the existing rule — *a cold line is a
hypothesis, not a finding; confirm it by execution* — is exactly what caught
this. What was missing was the reason. An hour went into "the spec provably
runs and the line provably does not", which is not a shape prose can warn you
about; it needs the mechanism named.

The detection is mechanical and cheap, and it is now written down in AGENTS.md:
when a cold line contradicts a spec that demonstrably exercises it, export a
single profraw. One process cannot underflow.

## The thirty-three lines

Twenty-one specs in `coverage.sh` (`cov.354`-`cov.374`), seven in `cli.sh`
(`cli.33`-`cli.39`). What they cover:

- **Unary `+`** through all three stages — the arith parser's `ATok::Plus`
  arm, the SSA transform's `IrUnaryArithOp::Pos`, and the IR printer's. Every
  existing arith spec spells `-`, `!` or `~`. Both signs are asserted, because
  `Pos` lowers to its operand unchanged and an arm that folded to the absolute
  value would pass the positive case alone.
- **A `$((` inside arithmetic**, which the collector nests rather than
  reopening, and the double-quote scanner's **nested `$( … $( … ) … )`** — one
  level never exercises the depth counter.
- **Four unterminated-operand scanners** behind a brace-less disposition:
  `$(`, a backtick, `${`, and a `${…:s/…}` edit whose fields run off the end.
- **DD#94's three terminator outcomes**: a trailing `;` ends the sequence, a
  trailing `,` still owes an operand, and a run of nothing but terminators is
  an empty expression worth 0.
- **`$'\u'` with no hex digits**, and a **`{ … }` group as a `while`
  condition** (`if` had this spec; `while` has its own call site).
- **The `<=` and `<<` entries of the operator table** SB5007 renders with. The
  assertions name the rendered text — a table with wrong entries still
  produces SB5007, so asserting the code alone would not notice.
- **Both remaining SB4030 operand descriptions**, for a command substitution
  and for everything else.
- **The K and G heap-size multipliers**. Accepting the suffix is not the claim
  worth making; a multiplier stuck at 1 would satisfy it too. Each size is
  paired with an operand that overflows `usize` *only after* scaling, so the
  bare number compiles and the suffixed one is rejected. That is the
  multiplication, observed.
- **Four CLI paths** that report an unreadable path or a usage error.

Two of them, `cov.373` and `cov.374`, needed end-of-source to arrive mid-token.
`run_sash` passes the program with `-c`, so the last byte of the source really
is the last byte and there is no trailing newline to stop the scan first. No
harness change was needed, which was worth checking before writing one.

## The comment that was wrong

`parse_arith_str` returns 0 for a token run that is nothing but separators,
and its comment said the C-style `for (( ;; ))` header is what reaches it. So
did `spec/arithmetic.sh`'s `31.51`, which has asserted that reading since it
was written.

It is not true. The header splits into three *empty* slots, and an empty slot
takes the `trimmed.is_empty()` return one branch earlier. Measured in
isolation, `for (( ;; ))` warms nothing on that line; `$(( ; ))` warms it.

This cost a spec: `cov.362` was written as `for (( ;; ))` on the comment's
authority, passed, and warmed nothing. The verification step is what reported
it — the snippet that warmed the line during probing and the spec finally
written are different programs, and only the second one counts. Both comments
now say what actually happens, and `cov.362` is `$(( ; ))`.

A second-order note worth keeping: the comment was not stale, it was wrong
from the start, and a passing spec carried the same wrong claim in its own
comment for its whole life. A spec pins behaviour. It does not pin the
*explanation* attached to it.

## State

`mk test` green (99 + 27 + 20). Full sweep green at the predicted total:
native 3935, c 3717 + 13 skipped, **7665** — 7609 plus 28 in each mode. Zero
flips against the previous sweep, with the gate's self-test returning 248 so
the comparison is capable of reporting one. Fifty-six new ids, which is the 28
specs in both modes, and no retirements.
