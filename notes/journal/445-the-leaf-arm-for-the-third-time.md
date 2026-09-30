# 445 — the leaf arm, for the third time

`check_taint` did not walk the statements inside a `$( … )` body. Closing that
was the third instance of one bug shape in a single session, and the third time
the fix was the same three lines in a different dispatcher.

## What was actually broken

The filing said the escape was already closed by the SSA backstop and this was
now a diagnostic-quality bug. Measured, that was too kind. Every one of these
reaches the backstop — no SB code, no span:

```sash
str q=$(str h=$(getenv HOME); echo "$h")          # builtin argument
str q=$(str h=$(getenv HOME); if [[ -n $h ]]; …)  # test operand
str q=$(str h=$(getenv HOME); /bin/echo x > $h)   # redirect target
str q=$(str h=$(getenv HOME); while [[ -n $h ]]; …)
str q=$(array a=(z); str h=$(getenv HOME); a[0]=$h; …)
```

The filing believed `/bin/echo "$h"` got a proper SB5020. It does — but only
because its probe put the capture in **argument** position, where
`check_bare_cmdsub_taint` rejects the whole capture as a bare tainted source
before anything looks inside. Put the same body in value position (`str q=$(…)`
plus an `attest`, which is the sanctioned way to obtain external data) and the
diagnostic is the backstop's. The probe was measuring a different check.

`check_cmdsub_inner_reads` was the only thing that looked inside a body:

```rust
for s in stmts {
    if let StmtKind::Command(c) = &s.kind { … }
}
```

Two holes. It matched `StmtKind::Command`, so a `Declaration`, `If`, `While` or
`Redirected` statement was skipped outright; and it measured reads against the
**outer** tainted set, which never contains a name the body itself defined.
`str h=$(getenv HOME)` inside a body is a Declaration — skipped — and `h` is
body-defined — invisible. Between them, nothing in a body was ever checked.

## The shape, three times in one session

`validate_stmt_ctx`, then `check_stmt`/`infer_stmt`, now `taint_stmt`. Each
dispatcher reached its body-walking helper from the `_` arm. But **the arms
that carry a `$( )` are the leaves** — `Command`, `Declaration`, `Assignment`
— so widening `_` reaches none of them. I confirmed that experimentally the
second time rather than assume it: widening `check_stmt_bodies` to
`child_bodies()` changed nothing at all.

The fix each time is an unconditional descent after the `match`, with the `_`
arm left pinned to `structural_bodies()` so a wrapper is not visited twice
(journal 308's duplication failure mode). Here it is `taint_cmdsub_bodies`,
extracted rather than inlined — inline it put `taint_stmt` at cc 13 against an
11.41 bar. Extracted, `taint_stmt` is cc 11 / cog 1 and the helper is 3 / 3.

Spec `cbt.8` exists solely to hold the no-duplication property: take a body
through a `while` wrapping an `if` and assert SB5020 appears **once**.

## The set is cloned, and that is not the whole answer

`taint_cmdsub_bodies` clones the tainted set per body, matching
`taint_function_body`. That is right for names the body introduces — they are
body-local per DD#86.

It is **not** right for a name the enclosing scope already binds. DD#86 says a
body is an anonymous function and writes to existing globals persist, and they
demonstrably do:

```
str t=clean; str q=$(t=modified; …)   → prints modified
```

So a body can taint an outer global and the clone discards that. This is not a
regression — today the pass walks nothing, so clone-and-discard is strictly
more checking than before and opens no new hole — but it is an unfinished
model, and the question it raises is DG's. See the note at the end.

## `check_cmdsub_inner_reads` is not redundant — measured

The descent routes through `taint_command`, which does strictly more than the
four-line walk, so the obvious tidy was to delete it. I stubbed its body and
swept: **12 specs × 2 modes failed** (`60.33`, `60.37`–`60.43`, `60.48`,
`60.50`, `60.52`, `av.n6`). It is reached from expression-walk positions
(`reject_scalar_rhs_read`, `check_value_cmdsub_reads`) that the statement-level
descent does not cover. Restored, kept, and now documented as load-bearing
rather than left looking like dead weight. The experiment cost one sweep and
replaced a plausible guess with a number.

## What the 20 sweep failures were

The fix broke 10 specs × 2 modes, exactly as the filing predicted, and every
one was a spec that had been using a `$( )` body as a *vehicle*.

`bkstp.1`–`bkstp.12` are the SSA backstop's own tests; their header names the
gap as the reason they can reach it. With the front end checking bodies they
are rejected earlier and never arrive. The right vehicle already existed:
**`--no-taint-cst`** disables only the front-end pass, and `spec/taint_switch.sh`
exists to host exactly that. The block moved there wholesale — dual-mode, so
the move is count-neutral. As front-end gaps keep closing, *all* backstop
coverage now depends on that flag.

`av.c5`, `av.c6` and `csc.16` were different: `argv N b` fills from program
arguments and the body then read `b` with no attest. I checked the identical
program at top level — SB5020. They passed only because bodies were not
checked. Attests added inside the body; they now assert what they meant to.

I checked `bkstp.12` specifically for a false positive — `array b=( @{a:-$t} )`
with tainted `t` — and it is SB5021 at top level too. Correct rejection.

## Two notes in passing

**The location assertion is mode-dependent, and I got it wrong twice.** First
`$err == *:*:*:` — which requires the string to *end* in a colon, so all five
specs failed. Then `.input.sash:[0-9]*:…`, which passes in c mode and fails in
native, because `_run_sash_native` compiles with `-c "$src"` and there is no
file: the prefix is `-c:1:36:`. The mode-agnostic form is `*:[0-9]*:[0-9]*:*`.
Asserting the span at all is the point of these specs — it is precisely what
distinguishes the new diagnostic from the backstop it replaces — so it was
worth getting right rather than dropping.

**SB5049 and bodies.** A name declared inside a body collides with a top-level
declaration of the same name, in either order, and two sibling bodies collide
with each other. That looks like it contradicts "names first introduced inside
are local" — but the function control behaves the same way (`cannot redeclare
global h inside a …`), so bodies are being consistent with the construct DD#86
models them on. Pre-existing, unrelated to taint, recorded here rather than
chased.

## Sweep

7027 (3616 native, 3411 c, 13 skipped) — predicted exactly before running.
`mk test`, fmt, clippy, totality all green.

## For DG

**Should a `$( )` body that taints an outer global be SB5067, like a function
body does?** Measured: a function body tainting a global gives a located
SB5067; a `$( )` body gives the backstop only; and the write persists at
runtime. `taint_function_body` gets away with clone-and-discard precisely
*because* SB5067 makes the top level the only place a global's taint can
arrive — `taint_top_level`'s "the set this leaves behind is final — no
fixpoint" depends on it. That invariant does not currently hold for bodies.

Extending SB5067 to bodies preserves the no-fixpoint architecture and is what
"a body is an anonymous function" implies, but it would newly reject programs
that attest after the body. Merging taint back out instead needs a bound-name
set threaded in and gives up the no-fixpoint property. I have not chosen; what
shipped is safe under either.
