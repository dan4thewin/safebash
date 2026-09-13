# 277 — The buffer DG asked for, three corrections later

DG's original question was *"why isn't `s=$(userfn)` just `userfn` copying
bytes into a local memory?"* Journal 276 answered that it couldn't be, and
the answer was right about the **deadlock** and wrong about the **general
case**. This entry closes the gap: for a body the compiler can prove never
forks onto its own stdout, the capture now *is* a local memory copy, and it
costs zero syscalls.

## The measurement

A 1000-line captured function, `strace -c`:

| path | total syscalls |
|---|---|
| pipe (original) | 3014 |
| memfd (276) | 3014 |
| **buffer (this)** | **4** |

Four, for the whole program: `execve`, one `mmap` for the heap, and the two
`write`s of the program's own final `echo`. The capture itself is **zero**.
The 3002 body writes that dominated both earlier numbers do not become
cheaper — they stop being syscalls at all.

## Why 276's objection didn't generalise

276 argued the sink must stay an fd, on two grounds: a forked child has only
an fd to inherit, and `echo hidden > f` rebinds fd 1 underneath a buffer
keyed on the number 1. Both are true. Neither is an argument against a
buffer — they are arguments against deciding *at runtime*, on the fd number,
with no idea what the body contains.

Decide at **emission** time and both evaporate. The compiler knows whether a
body forks and whether it redirects. It declines the fast path for those and
takes it for everything else. The runtime check stays exactly as cheap
(`if fd == 1`) because the compiler has already done the proving.

That is the shape of the correction: the objection was sound about the
mechanism 276 had in mind, and I let it stand as an objection to the *idea*.

## DG's purity rule

DG drew the line, and it is sharper than the one I would have drawn:

> if the function simply wraps an external command or pipeline, it's
> disqualified. if it captures external output, but ultimately uses echo or
> write to yield the value, those echo and write ops can be buffer appends.

So `fn emit { /bin/cat f; }` is out — the child inherits *our* stdout. But
`fn wrap { str t=$(/bin/printf ext); echo "got-$t"; }` is **in**, because the
inner `$( )` hands its child a pipe of its own and nothing but our `echo`
ever reaches our stdout. The disqualifying condition is not "contains an
external"; it is **"a fork inherits *this* capture's stdout."**

`spec/misc.sh` `cap.15` was already exactly that shape, written as a
non-regression guard two commits earlier with a comment predicting it would
matter to the fast path. It did.

`transform/capture_purity.rs` implements the walk. A body is impure if it
contains a pipeline (every stage forks), a redirect (may rebind fd 1
invisibly), or a command whose head contains `/` (DD#23: only a resolved
external has a slash). A call to a user function recurses into its body;
recursion cannot occur because SB5037 rejects it, but the walk carries a
visiting set so a malformed program falls back to "impure" rather than
hanging the compiler.

## The runtime is a stack, not a flag

The part that took a second pass. A flag cannot be right, because an impure
capture can nest inside a pure one — `fn outer { str d=$(deep); echo …; }`
where `deep` has an external. The inner site *does* redirect fd 1 to a
memfd, so for its duration the outer buffer must stop diverting, or the
inner body's in-process writes land in the outer string.

So `sash_capture_start` (the memfd path) pushes a **transparent** frame that
suspends diversion, and pops it on end. `sash_write` appends only when the
top frame is buffered. Depth is static — sash inlines functions and rejects
recursion — so a fixed 64-entry array is sufficient and an overflow is a
compiler bug, not user input.

## The vacuous test

`cap.18` pins the suspend. Its first draft used `/bin/cat cap_big.txt` as the
whole inner body, and **passed with the suspend deleted**. I only found that
because I ran the fail-first check on a test I was fairly sure was a
formality.

The reason is worth keeping: an external's writes go straight to fd 1 from a
forked child and never pass through `sash_write` at all. They are immune to
the buffer in both directions. Only an **in-process** write can be
mis-diverted, so the test needs `/bin/true` to make the body impure *and* an
`echo` to give the mechanism something to get wrong. With both, deleting the
suspend fails it.

Generalising: *fail-first is not a formality even when the test looks like
one.* A guard that cannot fail is worse than no guard, because it reads as
coverage.

## The split, made visible

One observable changed, and only one — the full sweep moved exactly two rows
(`c:cap.12`, `native:cap.12`) out of 4132.

A buffered site never redirects fd 1. So a body probing its own stdout sees
the caller's real stdout, where a memfd site sees a regular file:

| body | before | now | bash |
|---|---|---|---|
| pure | NOTPIPE | **PIPE** | PIPE |
| impure | NOTPIPE | NOTPIPE | PIPE |

I proposed hiding this — disqualify any body mentioning `/dev/stdout` or
using `-t`, and the optimization becomes unobservable. I recommended it on
the principle that an optimization shouldn't be observable.

DG rejected it:

> it should be observable rather than a taboo we only do when we won't be
> caught

Which is the better reading. The two paths genuinely differ. A rule that
conceals the difference *only where someone might look* isn't uniformity, it
is a cover-up with a lookup table. `cap.12` and `cap.17` now pin both halves
side by side, with the `/bin/true` in `cap.17` marked load-bearing, so the
thing that moves the answer is stated rather than discovered.

Note that sash now *agrees* with bash on the pure case. That is a side
effect, not a goal — matching bash byte-for-byte was retired long ago, and
276 records me wrongly invoking it as though it hadn't been.

## Cost

`emit_capture_inst` went to cognitive 6 against a 3× gate of 5.49. Extracted
`emit_capture_end_c`; it is 5/1 and 4/3 now. The gate did its job — the
function had grown a second axis of branching and was hiding it inside a
match arm.

## State

Sweep **4107 passing, 29 skipped, 0 failing**. Zero flips, zero lost, four
gained (`cap.17`, `cap.18` in both modes). `cap.12` passes with its new
expectation.
