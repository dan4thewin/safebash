# 274 — The heap gets a ceiling worth having (DD#107)

The last open half of `bug-heap-exhaustion-silent-segv`. The fail-loudly
half shipped 2026-09-12: exhausting the heap stopped being a silent
SIGSEGV and became `sash: out of memory (requested N bytes)` with exit 1.
That made the failure honest without making it rare — the heap was still
1 MB, and a 10 000-element array still could not be built.

## Measuring instead of quoting

The filing listed options A (grow on demand), B (reserve big with
`NORESERVE`), C (fail loudly, done). Before presenting the fork I checked
both claims against the tree, which turned out to matter:

- The ceiling. Not "1 MB of payload": **~5 000 30-byte elements**. The
  binding cost is *transient* — growing the entry array needs the old
  array, the new array, and the payload live at once, so usable capacity
  is well under the nominal size. The abort at 10 000 requests 640 KiB in
  one go.
- **Option A's premise was false.** The filing said heap_4 "supports
  multiple regions via `vPortDefineHeapRegions`-style init". The vendored
  `csrc/heap4.c` has a single `xStart`/`pxEnd` sentinel pair and a
  one-region `sash_heap_init(void*, size_t)`. heap_5 is the multi-region
  variant upstream. A is allocator surgery, not configuration — and the
  estimate in the filing had been written against the false claim.

That is three sessions running where a filing was wrong in a way that
would have changed the decision. A filing is a snapshot; nothing ages it.

I also had to retract a line from my own tradeoffs: I listed "a 32-bit
target would not tolerate 256 MB", and DG asked why 32-bit was called
out. `_start` is cfg'd for x86_64 and aarch64 only. There is no 32-bit
target. Generic caution dressed up as a specific cost.

## The decision, and the part I would not have guessed

DG took **B at 256 MB**, plus a switch to set it — and then answered the
switch's design before I could get the question out properly:

> we go out of our way to prevent things like PATH from altering runtime
> behavior so compiled-in seems better

That is the taint system's premise applied somewhere I had not thought to
apply it. I had been weighing an env var as the cheap option — `_start`
stores `sash_envp` before `sash_init_heap`, so reading `$SASH_HEAP` there
would have been nearly free, and I had noticed that and taken it as an
argument *for*. It is an argument against: cheap is exactly what makes it
attractive to reach for. A heap size in the environment lets anything
that controls the environment shrink a binary's heap until it aborts,
without touching the program. The whole point of rejecting bare `$HOME`
in favour of `$(getenv HOME)` is that the environment is untrusted input;
letting it size the allocator hands that input a lever on every program.

So: compiled in. `--heap-size=<size>`, `K`/`M`/`G` suffixes, no
`$SASH_HEAP`.

## Mechanism: weak default, strong override

`heap4.c` defines `sash_heap_size` **weakly** at 256 MB. `--heap-size`
makes the compiler emit a **strong** definition of the same symbol into
the object it generates — a Cranelift data object natively, a C global
under `--emit-c` — and the linker prefers it. An ordinary build emits no
such symbol, picks up the weak default, and carries no runtime branch.

The direction is the reverse of the project's usual one (`sash_trace_flag`
and friends are defined in the runtime and *imported* by codegen), which
is why weak linkage earns its keep here: it lets the runtime state a
default that the generated object may override, without every object
being obliged to define the symbol.

`NORESERVE` is what makes 256 MB affordable. Pages are committed when
touched, so hello-world's peak RSS is **340 kB — the same as under the
1 MB heap**. The reservation costs address space.

## What `--heap-size` did to the tests

`hp.1`-`hp.3` pinned "exhaustion aborts cleanly" by building a
30 000-element array. Under the new default that *succeeds*, so the tests
would have kept passing while testing nothing — a green check on a
failure path that no longer ran.

They now compile with `--heap-size=1M`. Exhaustion is pinned against a
**stated** ceiling instead of whatever the default happens to be, so the
default can move again without quietly hollowing them out. The flag made
the OOM tests cheaper and more deterministic than they were before it
existed, which was not a motivation for adding it.

`hp.6` (the default absorbs 50 000) and `hp.7` (`--heap-size=16M` lifts
what 1 MB refuses) pin the decision from both sides.

The first cut of `cli.20`-`cli.22` was worse than useless. They asserted
that `--heap-size=abc` exits non-zero — which it does *without the flag
implemented at all*, via the unknown-flag path. Fail-first caught it:
they passed against the stashed build. They now assert the size
diagnostic by name. A test that cannot fail for the reason it was written
is not a test, and only running it against the broken tree reveals that.

## `SASH_TRACE`, since DG asked

> SASH_TRACE should also be opt-in ... does trace actually do anything?
> (i don't recall it being implemented)

It does, and it already is opt-in — in exactly the shape DG was asking
for. `--trace` compiles the trace sites in; `SASH_TRACE=1` activates
them; a binary built without `--trace` ignores the variable completely.
The environment can only switch on machinery the *build* chose to
include, never introduce it. That is the same rule DD#107 now applies to
the heap.

What it emits is branch and loop outcomes (`+ loop → while.body`), native
backend only, pinned by `tr.1`-`tr.3`. Evaluated values are still
unbuilt, which is what keeps DD#32 `partial`.

DG's doubt was well-founded even though the feature works, because
**DD#32 said it did not**: "`SASH_TRACE=1` runtime trace ... (designed,
not built)", long after the branch half shipped. The adversarial audit
read that line and reported the gap back to us as a finding
(`audit-gemini-pro.md`). A decision record that understates what is built
manufactures phantom findings; both files are corrected.

Reading `_start` to answer the question turned up a real bug: **aarch64's
`_start` never calls `sash_trace_init`**, so `sash_trace_flag` stays 0 and
every compiled-in trace site tests false. On aarch64 `--trace` produces a
binary that is bigger, slower, and silent. Filed as
`bug-aarch64-trace-init-missing`; not folded in here, since it is a
different defect with a different fix and the startup sequence being
written twice per architecture is its real cause.

## Landed

256 MB default with `NORESERVE`; `--heap-size=<size>` in both backends,
operands validated at compile time (junk, zero, and overflow rejected
rather than clamped). 200 000 elements where 5 000 was the wall.

Sweep **4071 passing, 29 skipped, 0 failing**, zero flips, +12 specs.

Still a ceiling, and documented as one. `heap-multi-region-growth` (P3)
carries option A with its premise corrected.
