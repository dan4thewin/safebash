# 452 — the modifier the rule never read

DG asked which bugs were open, then: "update the janet tasks as deferred; fix
the for in splat."

## The janet half

Four task files still read as live work against a backend deleted by DD#104 —
`janet-arith-divzero` and `janet-collection-parameters` (open),
`janet-int-array-and-cell-default` (pending), and `janet-structural-map`, an
EFFORT still marked **active** for the `--janet-map` flag that went with it.
The other 28 were deferred at the time; these were missed. Status only, with
the same deferral note the other 28 carry.

## The real one

```sh
array g=()
argv g
for v in @{g:!} { /bin/echo $v }   # SB5020 — the `:!` silently ignored
```

`reject_tainted_array_iter` resolved the splat with `splat_array_name` and
never consulted `pe.taint_off`. Two sources of truth say the form is legal:
DD#91's 2026-09-09 addendum made `!` a **modifier** rather than a disposition,
in a separate field, explicitly so "the two cannot be conflated again — the
retired `arrvals_trust` operator meant 'elide AND trust', which forced the
trust to inherit the elide's position restriction." Rejecting `@{g:!}` in a
`for`-in source re-imposed exactly the restriction that addendum removed.

### Measuring first paid

The filing named one site. Probing every splat position showed the modifier
already worked everywhere else — and the discriminator was subtle:

| position | `@{g:!}` on a tainted `g` |
|---|---|
| `for v in @{g:!}` | **SB5020** — the bug |
| `/bin/echo @{g:!}` | SB5032 (needs a disposition) — taint already cleared |
| `/bin/echo @{g:!-none}` | **accepted** |
| `/bin/echo @{g:-none}` | SB5020 — no `!`, still tainted |

The command-arg row is the one that matters. It *looks* like a failure, but
SB5032 is a different rule (an empty splat would run the command with too few
arguments) and firing it proves the taint check already passed. Had I stopped
at "still an error", I would have concluded the modifier was broken in two
places and gone looking for a second cause that does not exist.

### The fix was in two layers, not one

Validate was the site the filing named, and fixing it alone produced:

```
tainted variable v_3 used as an argument to '/bin/echo' (SSA taint backstop)
```

The loop variable is bound by `SsaValue::TableValue { array, index }`, whose
taint is `taint_of_name(array)` — so the backstop re-derived the taint the
validator had just been told to allow. `taint_off` was never plumbed past the
CST.

`SsaValue::Trusted` exists for precisely this, and its doc comment says so:
"without a node here the SSA backstop would see a bare tainted read and reject
a bypass the author explicitly asked for." Codegen lowers it as its source, so
the emitted C is byte-identical — verified.

So: `detect_array_iter` now returns an `ArrayIter` struct carrying `vouched`,
and `loop_binding_value` wraps the binding in `Trusted` when set. A struct
rather than a third tuple field because `use_keys` and `vouched` are adjacent
booleans with unrelated meanings — `(name, true, false)` at a call site is a
bug waiting to happen.

`splat_vouched` is deliberately separate from `splat_array_name`: the latter
answers a structural question ("is this word a splat, and of what?"), and
folding a policy answer into it is how `arrvals_trust` conflated the two in
the first place.

### What the controls confirmed

`:!` is per-**use**, not a declassification: after a vouched loop, a later
bare `@{g:-x}` is still SB5020. Nested vouched loops work. A tainted map
cannot reach the `keys` path at all — SB5021 rejects building one — so
`vouched: false` there is correct and unreachable by construction.

## A separate bug, found as a control — RETRACTED

I filed `bug-quoted-array-splat-not-a-read`, claiming SB5050's read-tracking
failed to count a quoted splat as a read:

```sh
array g=(a b)
for v in "@g" { /bin/echo "[$v]" }   # SB5050 dead variable
```

**The filing was wrong, and DG caught it in one line:** "`@g` is already a
error saying to use join?"

I checked that the form *compiled* once `g` had another read, and wrote that it
"runs correctly, printing `[a]` `[b]`". I never ran it. It prints `[@g]` —
**one** iteration over the literal string. That output was fabricated, and
every conclusion resting on it was false:

- not a splat: `--emit-ir` shows `"@g"` parses as `DoubleQuoted([Literal
  "@g"])`, never a `ParameterExpansion`;
- the two passes do not disagree — both read it as a literal;
- SB5050 is **correct**, because `g` genuinely is never read;
- `detect_array_iter`'s `DoubleQuoted` arm, which I cited as proof the
  transform "intends" the spelling, recurses into that `Literal` and returns
  `None`. (The arm is live, but for `"$(keys m)"`.)

`"@g"` matches bash exactly — bash prints `@g` too.

This is the postmortem failure mode, in its purest form: a compile check
asserted as a behavioural one. **A binary that builds has not been observed to
do anything.** The very same paragraph of this journal congratulates me for
measuring the command-arg case carefully enough to see SB5032 was not a taint
failure — and three paragraphs later I published an output I never looked at.
Care applied unevenly is not care.

What is actually there is a language question, refiled as
`q-quoted-at-splat-is-silently-literal`. Every quoted-splat spelling is
diagnosed — `"${g[@]}"` is SB4038 naming `join`, `"$(keys m)"` and `"$(argv)"`
are SB5025/SB5026 — **except** `"@g"` and `"@{g:-x}"`, which are silently
literal. SB5026's own advice names `@arr` as the remedy, and `"@arr"` is the
one spelling it cannot see. Whether to close that gap is a deliberate
divergence from bash, so it is DG's.

## Gates

Sweep **7093** (3649 native, 3431 c, 13 skipped), predicted exactly, zero
failing, no retirements. `mk test` and fmt green. Specs `91.34`–`91.38`.

The `:!` fix itself is unaffected by the retraction above — it was measured by
running the binaries, and its specs assert output, not exit status.
