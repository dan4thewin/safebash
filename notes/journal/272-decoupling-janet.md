title: "Decoupling janet: --emit-ir, and the removal"
date: 2026-09-13

## The question DG asked

*"Put a pin in that. Can the IR that janet is supposed to express be
captured to a file (serde?) so that the janet oracle could be a separate
program?"*

Then, before I'd finished answering: *"every change seems to yield more
janet bugs/gaps. if we split off janet entirely via serde, then iteration
on native/c doesn't have to 'pay the janet tax', and janet can come along
as time permits... discuss"*

## Measuring the tax before agreeing with it

The premise was worth testing rather than accepting, because "every
change yields more janet bugs" and "the tax is growing" are different
claims and only one of them turned out to be true.

| mode | passing | failing | skipped |
|---|---|---|---|
| native | 2095 | 0 | 0 |
| c | 1920 | 0 | 27 |
| janet | 1366 | **41** | **540** |

The skip ratio sat at **27.7–28.0% on every commit I sampled**. So the
tax was *constant*, not accelerating — the weaker version of DG's claim.
I said so.

But the constancy is what makes the case, not what weakens it. A cost
that grows eventually argues for itself; a flat ~28% surcharge never
does, and never goes away either. And underneath it sat the fact that
settles the matter:

**janet was not gating.** "41 failing (all janet, unchanged)" appears in
every sweep report going back months. The pre-commit hook checks fmt,
clippy and complexity — never janet. So the project was paying the full
coupling cost and collecting none of the oracle benefit. That asymmetry,
not the failure count, is the decision.

One honest correction I had to make to my own framing: serde changes
*when* you pay, not *what you owe*. The 41 failures don't evaporate on
being moved out of tree. They are a real diagnosis — 13 are DD#56
signals, `cf.f9/f10/f12/i2` the genuine control-flow gaps — and they
confirm the 2026-08-16 analysis in `janet-rt-primitive-library.md`:
the backend re-implemented leaf primitives inline instead of calling a
library. That task file even names the trigger for this decision, which
is a strange thing to find already written down.

## What a "janet data literal" is

DG: *"what is a janet data literal? I don't understand --janet-map vs.
--janet, is one the older more idiomatic janet and the other the more
IR-faithful?"*

Exactly right. I ran both on one program and showed the three forms side
by side:

- `--janet` — **semantics**: what the program *does*. Idiomatic,
  readable beside the sash. Also a permanent debt: correct semantics for
  34 node types, forever.
- `--janet-map` — **structure**: how the program *parsed*, via `sash/*`
  macros. Better, but a macro is a program — the consumer's preamble has
  to match the compiler's vocabulary exactly. And it still had semantics
  to defer, so it still emitted `(sash/TODO stmt:arith-test)`. It could
  not render a 7-line program.

The finding that decided it: `--janet-map`'s output and a data literal
carry **the same information**. They differ only in whether the `sash/*`
names are macros or inert tags. In a homoiconic language the two look
nearly identical on the page, which is exactly why the distinction is
easy to miss and load-bearing.

DG picked **C**: emit it inert.

## Why inertness is the whole mechanism

There is nothing to execute and nothing to version-match, so a consumer
running weeks behind the compiler can still read a capture. That is what
"decoupled" actually cashes out to.

It also made the capture **complete on day one**. With no semantic
decisions to make there is no staging, no `TODO`, no partial coverage —
the property that distinguishes it from both predecessors. Cheapest of
the three options *and* the most complete, which does not happen often.

serde/JSON was the other candidate and would have given mechanical
anti-drift, but it destroys DD#15's reviewability goal — you can no
longer read the capture beside the sash. A structural sexpr map already
*is* a serialization.

## Two properties I made load-bearing

**Exhaustive, no `_` arm anywhere.** A capture that silently drops a node
is worse than no capture, because it fails quietly and at a distance.
Every match names every variant, so adding an IR variant is a compile
error at each site that must learn about it.

This survived the complexity refactor, which is where it nearly didn't.
`ExprKind`'s 12 variants sit in the gate's dead zone — too few for the
pure-dispatcher delta exemption, too many for the 3× bar — so `expr` had
to split. The lazy split gives the second half a `_` arm. Instead each
half names the *other's* variants explicitly and routes to
`unreachable!`, so a new variant still breaks the build in both places.

**Tags are IR node names, never an interpretation.** `attest` renders as
`{:kind :command :name "attest"}` — no `:attest` tag, no run/ok?
distinction. That distinction is position-based interpretation and
belongs to the consumer. Keeping this layer judgement-free is precisely
what lets a consumer lag safely: it has nothing to disagree with.

(DG asked for `:attest` over `:bless`. Checking first: `bless` is dead
vocabulary, zero source hits — DD#15's text simply predates the rename.
Nothing to rename, only something not to resurrect.)

## Verifying, and one near-miss

I instrumented the spec harness to emit and janet-parse every program in
the corpus: **1974 emissions, zero parse failures.**

The near-miss is worth recording. The first run came back zero — and it
was meaningless, because I had not checked the probe fired. It hadn't,
much. Adding a counter and re-running turned a vacuous zero into a real
one. A zero-failure result from an unverified probe is indistinguishable
from a zero-failure result from a working one, and only one of them is
evidence.

I re-ran the whole corpus again after the refactor rather than assuming
output survived it.

## The removal

`emit_janet.rs` (2400), `emit_janet_map.rs` (667), `sash-rt.janet`,
`janet_tests.rs`, the flags, the `SASH_MODE`.

Two things the grep found that the directive didn't name. `oracle.rs`
and `oracle_tests.rs` have neutral names and are entirely janet — the
file compiles to janet, runs it, compares to bash. Nothing else used
them, so they went too; otherwise `clippy --tests` breaks.

And `spec/cli.sh`'s `cli.13` — *"the usage text covers every advertised
flag"* — failed on the removal. Which is exactly what it is for. Nice to
have a test fail for the right reason.

## What I did not do

Two cleanups I stopped short of and took to DG rather than deciding:

- the ~300 `in_modes native,c` guard sites, now harmless no-ops but
  stale conditionals referencing a dead mode;
- the stale rows in `spec.db`. `test_status` **never forgets a removed
  test** — `native:15.1` still reads `passing` at commit `2c850ae`, and
  565k janet rows still report 41 phantom failures. That is a gate
  integrity problem rather than cosmetics, and it is general: it is not
  about janet, it is about what the view does with any deleted test.
  The stale rows do carry an old `commit_hash`, so they are detectable.

Sweep after removal: **4019 passing, 0 failing, 31 skipped**, zero flips.

## Addendum — the two cleanups

DG: strip the guards now; purge the DB (option C, over the
self-cleaning view).

**The guards.** Behaviour-neutral by construction, and I verified it
rather than asserting it: byte-identical sweep, zero status diffs, zero
rows gained or lost. What made it interesting was the failure modes. My
first pass asserted that every `else` branch was pure `skip_chk` before
dropping it, and 109 of 300 failed that assertion — `;`-chained skips,
`for n in …; do skip_chk cov.$n; done` loops, and inline
`else skip_chk X; fi`.

The one worth recording: **`run_sash '…'` payloads contain `if`, `else`
and `fi` as string content.** A depth counter that reads lines
positionally sees those as real block structure and closes the guard in
the wrong place — silently, producing a file that still passes
`bash -n`. Quote-parity tracking fixed most; the last 13 I did by hand
with line-number assertions. Strict assertions turned what would have
been a silent corruption into a list.

**The comments.** 221 orphaned lines. Mostly they mix a real
explanation with a janet aside — `# ivc.9 an int-valued collection cell
renders as decimal in string context. native,c — janet treats the
collection variable as a scalar int (filed …)` — so a line-granular
delete would have destroyed the test descriptions. Rule: excise the
clause, keep the sentence. Verified two ways: every added/removed diff
line is a comment or blank, and all 47 touched ids still have a
description line.

**The DB.** 565k janet rows, plus 8 for tests deleted earlier. Those 8
are the part I'd flag: `native:15.1` was still reporting `passing` at
`2c850ae` — two commits after I deleted the test. The view has no notion
of a deleted test. We purged rather than fixed that, so it will recur;
stale rows are detectable by a `commit_hash` older than the newest in
`runs`.

Final: **4013 passing, 29 skipped, 0 failing, 0 stale rows.**
