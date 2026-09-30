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

## A separate bug, found as a control

```sh
array g=(a b)
for v in "@g" { /bin/echo "[$v]" }   # SB5050 dead variable
```

Clean array, no taint, and **identical on the pre-fix binary** — not a
regression. Add any other read of `g` and it compiles and runs correctly, so
the splat works; the dead-variable pass simply does not count a quoted splat
as a read. `detect_array_iter` deliberately recurses through a single-part
`DoubleQuoted`, so the transform intends the spelling and two passes disagree
about it. Filed as `bug-quoted-array-splat-not-a-read` — whether `"@g"` should
be a spelling at all is DG's call, and it is not this fix's business.

## Gates

Sweep **7093** (3649 native, 3431 c, 13 skipped), predicted exactly, zero
failing, no retirements. `mk test` and fmt green. Specs `91.34`–`91.38`.
