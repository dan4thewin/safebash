title: "Should a `$( )` body that taints an outer global be SB5067?"
status: done
priority: P2
kind: question
depends: []
gates:
  - "DG has chosen between extending SB5067 to bodies and merging taint outward"
  - "DONE 2026-09-30: DG chose (a); implemented, specs bfb.23-bfb.28"

# A `$( )` body can taint an outer global and nothing says so

Opened 2026-09-29 by the fix in journal 445, which closed the surrounding
escape but deliberately left this half unmodelled.

## Measured

A **function** body tainting a global is a located `[SB5067]`.

A **`$( )`** body doing the same reaches only the SSA backstop — no SB code,
no span. And the write really does take effect, per the DD#86 addendum:

```sash
str t=clean
str q=$(t=modified; /bin/echo x)
```

prints `modified`. So the value escapes and the taint does not follow it.

## Why it is not simply a bug to fix

`taint_cmdsub_bodies` clones the tainted set per body, copying
`taint_function_body`. Clone-and-discard is sound *for functions* precisely
because SB5067 makes the top level the only place a global's taint can arrive.
`taint_top_level` documents the consequence: "the set this leaves behind is
final — no fixpoint". That invariant does not hold for `$( )` bodies, so the
choice here is an architecture choice, not a patch.

This is strictly better than the previous state (the pass walked nothing at
all) and introduces no new hole, which is why it shipped.

## The options

**(a) Extend SB5067 to `$( )` bodies.** Preserves the no-fixpoint
architecture, and is what "a body is an anonymous function" implies — the same
rule, applied to the same construct. Cost: it would newly reject programs that
taint inside a body and attest after it, which compile today.

**(b) Merge taint back out for names the enclosing scope binds.** Models DD#86
exactly. Cost: needs a bound-name set threaded into the descent, and gives up
the no-fixpoint invariant that `taint_top_level` currently relies on.

Recommendation: (a), because it keeps one rule for one construct and the
architecture it protects is load-bearing. But it is a behaviour change that
rejects currently-valid programs, so it is DG's call.

## Update 2026-09-30 — (a) got much cheaper

DD#121's addendum relaxed SB5067 from "may not taint a global" to "may not
*leave* a global tainted" (no net insert at body exit). That changes this
question's arithmetic.

The stated cost of (a) was that it "would newly reject programs that taint
inside a body and attest after it". Under the net-effect rule an **in-body
attest is the repair** — a `$( )` body could taint an outer global and attest
it adjacently, exactly as a function body now can, and be accepted. What (a)
would still reject is the genuinely leaky shape: a body that taints an outer
global and leaves it that way.

The rationale for (a) also got stronger. The task's own argument is that
clone-and-discard is sound for functions "precisely because SB5067 makes the
top level the only place a global's taint can arrive" — and the net-effect
rule is what now carries that invariant, in the form
`entry ⊇ exit` for globals. Extending the *same* predicate to `$( )` bodies
is a smaller step than extending the old syntactic one was.

Status quo is preserved for now and deliberately: the addendum's rewrite moved
site collection to `walk_scoped_structural_stmts`, which does not descend into
`$( )` bodies, so this question is exactly where it was. Still DG's call.

## Measured 2026-09-30, after the net-effect rule landed

The filed example used a *clean* write (`t=modified`), which understates the
question. Re-probed with a real taint, and with `q` read so SB5050 does not
mask the result:

| # | shape | today |
|---|---|---|
| A | body writes a CLEAN value to an outer global | compiles, prints `modified` — no taint involved |
| B | body taints an outer global, global read outside | **SSA backstop only** — spanless, no SB code, off under `--no-taint` |
| C | same, read outside with `:!` | compiles, leaks the value |
| D | body taints + **adjacent in-body attest** | **compiles and works** |
| E | global read BEFORE, tainted in body, never read after | **compiles silently, nothing says so** |
| F | the function equivalent of E | located **`[SB5067]`** |

E and F are the whole question in two lines. Identical semantics — sash's
`$( )` body is inlined, not a subshell, so the write escapes either way — and
one is diagnosed while the other is silent.

### What this does to the cost of (a)

D is the finding. The stated cost of (a) was that it "would newly reject
programs that taint inside a body and attest after it"; under the net-effect
rule the in-body adjacent attest is already accepted, so that cost is gone.

B is already rejected today, just badly. So the only shape (a) would *newly*
reject is **E** — and E is exactly what SB5067 already rejects for a function.
(a) is therefore close to a no-op on accepted programs: it converts B from a
spanless backstop reject into a located diagnosis, and closes E.

### C is not an exception — corrected 2026-09-30

An earlier draft of this section listed C (`:!` on a global a `$( )` body
tainted) as something the anonymous-function model leaves open. That was
wrong, and DG caught it.

SB5067 under the net-effect rule fires at **body exit** and never examines the
read site. Measured on the function form: a body that taints a global with no
attest is SB5067 *even when the global's only read is `:!`*. The bypass is at a
later statement; the rule has already failed by then.

So under (a) case C becomes SB5067 for the same reason, before the `:!` read is
reached. The model settles the whole question.

This also retires the exception DG raised during the DD#121 discussion — "the
sole exception being a missing attest and a global only accessed by `:!` (an
exception we could disallow...)". The relaxation closed it as a side effect; no
separate decision is needed.

`:!` remains live exactly where SB5067 has no jurisdiction, neither of which is
a body exporting taint:

- a **top-level** taint read with `:!` (DD#91's sanctioned bypass)
- a function's **own local** read with `:!`

## Resolved 2026-09-30 — DG chose (a)

> ":! no longer can happen, since a function can't leave a global tainted, yes?"
> … "having :! work within the same scope preserves the intent sufficiently.
> go with (a)"

`taint_cmdsub_bodies` now runs the same gather -> walk -> verdict as
`taint_function_body`: sites collected before the body's walk, the entry set
snapshotted, and `report_exit_tainted_globals` passed against the exit set. The
subject in the diagnostic is the construct (`$( … )`) rather than a name, since
an anonymous body has none; the span already points inside it.

The diagnostic's rationale clause was generalised from "a function may not
export taint to its caller" to "taint may not escape the body that creates it",
which is true of both constructs. All existing SB5067 assertions match on
`leaves the global` / `assignment`, so none changed.

### What DG's `:!` observation settles

It is the reason (a) is complete rather than partial. SB5067 fires at **body
exit** and never examines the read site, so `:!` downstream cannot rescue a
leak — measured on the function form first, then confirmed for bodies
(`bfb.24`). `:!` keeps working exactly where the intent is local: a top-level
taint, and a body's own locals (`bfb.27`). That is "within the same scope".

### Measured cost: zero

The sweep went 7071 -> 7083, the full +12 being the six new dual-mode specs.
**No existing spec changed verdict.** The predicted near-no-op held exactly:
the only newly-rejected shape is E, which the function rule already rejected.

### Clone-and-discard is now sound rather than merely convenient

The task's argument was that cloning the tainted set per body is sound for
functions "precisely because SB5067 makes the top level the only place a
global's taint can arrive", and that the invariant did not hold for `$( )`
bodies. It does now, and for the same reason. The verdict is what makes the
discarded set provably equal to the one the enclosing scope keeps, so
`taint_top_level`'s "no fixpoint" claim covers bodies too.

Option (b) is closed with it.
