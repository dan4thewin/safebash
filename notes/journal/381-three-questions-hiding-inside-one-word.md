# 381 — Three questions hiding inside one word

DG, on the last open item in DD#25:

> shl strikes me as dependent on the internal representation and thus not
> truly mathematical. I think shifting in zeroes on left or right are
> desireable in practice.

Measuring against that framing showed "shift" was hiding **three** separate
questions, and only two of them were really about representation. Separating
them is the whole of this entry.

## 1. Count masking — representation, straightforwardly

`1 << 64` was `1`. `1 << 200` was `256`. `8 >> 64` was `8`.

The hardware masks the shift count to 6 bits, so a count of 200 is a count of
8. That is a property of the register width, not of the arithmetic. And the
failure mode is the one DD#25 exists to reject: a dynamically computed mask
width of 64 silently returned the *unshifted* value. Wrong, and it looks
right.

Changed. A count ≥ 64 now shifts every bit out.

## 2. Negative counts — the one nobody had named

This was worse, and I only found it by building the full matrix rather than
testing the cases DD#25 mentioned.

    8 >> -1   →  0
    8 >> -64  →  8

`-64 & 63` is `0`, so a negative shift count was silently an **identity**. A
negative count is meaningless in every model of what a shift is. It now
aborts — `sash_arith_negshift`, `<argv0>: negative shift count`, exit 2 —
consistent with the amendment that made every arithmetic fault abort.

## 3. Sign fill on `>>` — where I argued the other way

"Shifting in zeroes on left or right" reads naturally as: `<<` fills zeros on
the right, `>>` should fill zeros on the left. I put the opposite case to DG,
because on a *signed* type the zero fill is the **more**
representation-dependent choice, not the less:

    -8 >> 1   arithmetic → -4   = floor(-8 / 2)
    -8 >> 1   logical    → 9223372036854775804

The second answer means something only if you reinterpret the sign bit as a
value bit — which is precisely reaching *into* the representation. Zeros-in is
the right rule for an unsigned type; `int` is `i64`.

DG took the argument. `>>` keeps filling with the sign, so a count ≥ 64
saturates: `0` for a non-negative value, `-1` for a negative one.

Worth recording that the floor/truncate split is pre-existing and unrelated:
`-9 >> 1` is `-5` and `-9 / 2` is `-4`, in bash and sash alike. Sign fill was
never buying an identity with division.

## The shape of the answer

Unbounded non-negative counts; large counts shift everything out; negative
counts are a fault. That is **exactly Go's rule**, arrived at from DG's
framing rather than from Go. Finding that the answer you reasoned to is one
somebody already shipped is a good sign, and I checked it *after* rather than
reaching for it as an authority.

## Four sites had to agree

`emit_shift` (native), `emit_shift_c` (C), and `fold_arith_op` — which
`validate.rs`'s `const_eval_arith` shares, so that is one place, not two. The
folder **declines** to fold a negative count and defers to the runtime trap
rather than producing a value, which is the rule DD#25 already states for a
proven overflow.

Spec `25.17` exists to pin exactly that agreement: a literal `1 << 64` must
fold to the same `0` the variable path computes. A constant folder that
disagrees with the code it is folding *for* is the worst kind of bug, because
the program's meaning then depends on whether a value happened to be known at
compile time.

The C guard turned out to carry a second job. In C a shift by ≥ 64 or by a
negative count is **undefined behaviour**, so clamping the count before it
reaches the shift is also what keeps the emitted program well-defined. The
semantic fix and the UB fix are the same edit.

## The other ruling: brace-less stays literal

Separately, DG settled the `y@a:-x` asymmetry left over from journal 377.
`y@{a:-x}` reports SB5083; `y@a:-x` is literal text and matches bash.

The case for closing it was consistency — gs.30 asserts "brace-less ≡ braced"
and this is the one place they diverge. What decided it the other way was a
measurement:

    user@host:~/dir

`@name` followed by `:~` **is** the brace-less trigger. That is the
scp/rsync/ssh remote-path spelling, not a corner case, and a prefix rule would
try to splat an array named `host`.

So `@` opens a splat only at the start of a word; mid-word it is an ordinary
byte, as in bash. The asymmetry is principled rather than an oversight: braces
are an *explicit request* for splat machinery, and the strictness belongs on
the spelling that asked for it.

I had flagged this one as "would break a currently-accepted program" for
several sessions without ever asking *which* program. The answer took one
probe and changed the recommendation from a shrug into a reason.

## Result

Sweep 5839, zero failing. Specs `25.13`–`25.17` and `gs.52`–`gs.55`.
`25.13` is a **rewrite** — it used to pin the bash-matching answers, which is
what made this a visible decision rather than a silent drift.
