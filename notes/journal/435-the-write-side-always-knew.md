# 435 — The write side always knew

DG asked a question that turned out to be the whole session:

> My model of taint and use are both simple. a variable/container has a
> flag. compiling a write from an external source sets the flag. compiling
> a read checks the flag. why is there some separate walk rather than a
> check every time a read is added?

The honest answer was that his model *is* what is implemented — and that the
separate walk is not the problem. The problem is that "every read" was a list
somebody typed out by hand.

## The measurement

There are two taint implementations: the front-end pass in `validate.rs` and
the SSA backstop in `ssa_taint.rs`. Both are structured exactly as DG
described. Both maintain a set, set on write, check on read.

`ssa_taint.rs` splits cleanly into those two halves, and they had very
different histories:

| half | function | shape | escapes found |
|---|---|---|---|
| write | `classify_inst_def` | exhaustive, all 19 `SsaInst` variants | none, ever |
| read | `check_inst_uses` | `_ => {}`, 4 of 19 | both live ones |

`SsaInst::Test` and `SsaInst::RedirectPush` are *listed by name* in
`classify_inst_def`, twenty lines above the wildcard that omitted them.

Widening the sample did not weaken it. Every taint escape this project has
recorded — journals 400, 430, 431, 432, and the two open at the start of this
session — sits behind a wildcard match arm. **Not one has been found behind an
exhaustive one.** The wildcard is not correlated with the bug; it is the bug's
entire mechanism.

## DG's answer

> what about interposing a "getter" as the only way to do a read?

The interesting part of checking this was finding that the getter pattern
already exists at every level of the codebase, and that its reliability tracks
exactly one property:

| getter | wildcard? | escapes |
|---|---|---|
| `Statement::child_bodies()` | no — 14/14 | none |
| `classify_inst_def` | no — 19/19 | none |
| `Statement::direct_exprs()` | yes `_ => {}` | — (taint never calls it) |
| `SsaValue::child_values()` | yes `other =>` | journals 430, 432 |
| `check_inst_uses` | yes `_ => {}`, 4/19 | **both live escapes** |

So the getter is worth having, but not for the reason it first appears. It does
not create totality — `_ => vec![]` puts us straight back. What it does is
collapse the number of places that have to *be* total from about twenty to one,
and put that one place where rustc's exhaustiveness check can hold it. The
enforcement was always the compiler. The getter just gives it something to
hold.

DG: "make it so."

## What was built

`SsaInst::taint_reads()` in `ssa.rs` — total over all 19 variants, no wildcard,
returning each read operand with a description of its sink. `check_inst_uses`
became a four-line loop over it. `check_value_use` became total over all 30
`SsaValue` variants; it had covered 8.

Two things about the shape were not obvious going in.

**Redirects are why a value-oriented walk could never have worked.** A
`Redirect`'s target is an `ir::Expr` and never becomes an `SsaValue` —
redirects ride through SSA unlowered. No amount of totality over `SsaValue`
reaches them. The getter has to yield a sum type, `TaintRead::Value` or
`TaintRead::RedirectTarget`, and the redirect half resolves by bare name
through `taint_of_name`. That is imprecise in the permissive direction (a
variable attested *after* the redirect reads clean), which is acceptable for a
backstop and is noted where it happens.

**`check_value_use` deliberately does not delegate its recursion to
`SsaValue::child_values()`**, even though that is what `value_is_tainted` does.
`child_values()` still ends in a wildcard, so borrowing it would have
reintroduced the same hole one level down — the exact defect, one call deeper.
Written out by hand until that getter is total too; filed as
`getters-still-ending-in-a-wildcard`.

## The sweep found what it was supposed to find

First run after the change: **14 failures**, 7 per mode — `24.13` and
`bfb.1`–`bfb.6`. Every one an `attest`.

The old code's comment had said, correctly, that every `attest` form routes
through `lower_if_attest` and "reaches SSA as branch machinery and never as a
Command." True, and irrelevant: it reaches SSA as a bare `SsaInst::Test`, in a
condition block, structurally identical to a user-written `[[ ]]`. Making
`Test` a sink therefore rejects every caught-attest program, because reading
the tainted subject is the entire point of the form.

That distinction had never needed to exist, because `Test` had never been
checked. It needs to exist now: `SsaInst::Test` carries a `TestOrigin`, `Plain`
or `AttestPredicate`, and the predicate gets the same exemption the bare
`SsaInst::Attest` form already had. Two of the seven construction sites are
attest; five are not.

Worth noting what did *not* fail. `lift_default_value` emits a `-z` test on a
temp holding `${x:-word}`'s source, and `try_test_arg_capture` emits one for
`[[ ]]` as a command argument. Both are compiler machinery reading a value the
user never wrote a test for, and both were left `Plain` on the theory that a
tainted value reaching them is a real read. 6729 specs agree.

## Result

Both escapes closed:

```sh
/bin/echo $(str h=$(getenv TARGET); echo pwned > $h)
# was: compiles, and the binary writes a file named by the environment
# now: tainted variable $h used as a redirect target of 'echo'

/bin/echo $(str h=$(getenv TARGET); if [[ -n $h ]] { echo yes })
# was: compiles, and the binary branches on external input
# now: tainted variable h_1 used as a test operand
```

Sweep **6716 passing, 13 skipped, zero failing** — 6729 total, predicted before
running. Four new specs, `bkstp.1`–`bkstp.4`: two proving the escapes are
closed, two proving the `TestOrigin` exemption holds, because those were the
tests the first sweep broke.

## The gate

`contrib/totality-check.py`, wired into `.githooks/pre-commit`. Any function
whose doc comment contains `do not add a wildcard` must have no wildcard arm.
Seven functions opt in today; the marker was already the house convention, so
the gate enforces something the codebase was already saying.

Verified it can fail before trusting it — replacing one arm with `_ => {}`
produces the rejection, restoring it goes green. A gate that cannot fail is not
a gate.

Unlike the complexity gate this is whole-tree rather than
grandfather-on-modify. It can afford to be: there is no backlog.

## Left open, deliberately

The plan DG approved had a fourth part: route the front-end taint pass through
`Statement::direct_exprs()` instead of its twenty hand-written call sites.
Measuring it changed the case for doing it *now*.

`check_expr_taint` is **already total** over `ExprKind` — it was made total in
the journal-400 work. Its remaining weakness is call-site completeness, not
variant completeness, and the fix touches the whole front end's diagnostic
ordering. That is a different size and a different risk from anything above,
and it is not what was closing the escapes. Filed with the evidence rather than
started at the end of a session. DG sees the diff shape before it begins.

Also left: the backstop still does not ask whether a value is tainted *as a
whole*, only whether it contains a tainted read. `SsaValue::CmdSubst { argv:
["getenv", "T"] }` holds nothing but literals, so `/bin/echo $(getenv T)` slips
past it — under `--no-taint-cst`. With the front-end on, which is what ships,
it is SB5020 everywhere it can be written, inside a `$( )` body included.
Measured, not assumed. `ssa-taint-full-backstop` carries it, now with the
diagnosis: the derivation half already computes the answer the reporting half
declines to ask for.
