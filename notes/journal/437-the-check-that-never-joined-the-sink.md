# 437 — The check that never joined the sink

Journal 436 filed a bug and called it DG's call. DG asked me to explain the
decision, and explaining it is what showed there wasn't one.

## What I had measured, and what I hadn't

The symptom was a false rejection: an array declared and splatted inside the
same `$( )` body was reported "introduced nowhere". I traced it to
`check_scalar_splat` building its `introduced` set from a program-wide
`program_scopes` union, which walks `child_bodies()` — total over `StmtKind`,
so a body hanging off an **expression** structurally cannot appear.

From there I reasoned that widening the union would trade the false rejection
for a false acceptance, because DD#86 makes a name first introduced in a
`$( )` body local to it. A real tradeoff, so: DG's call.

I never checked whether the union was *already* false-accepting.

```sh
fn f { array a=(1 2); /bin/echo @{a:-z}; }
f
/bin/echo @{a:-z}          # accepted, and printed `1 2`
```

DD#86 makes that `a` local to `f`. The `$y` and `$a[0]` spellings of the same
read both reject, correctly. Only the splat leaked, and it leaked at runtime —
a function-local array read from top level.

So the flat union was not a conservative choice being traded against a strict
one. It was wrong in both directions at once, and the tradeoff I had described
did not exist.

## Why the other spellings were right

The main SB5031 check is scope-aware. `check_reads_in_scope` threads a `known`
set per scope, and `SinkMode::Flag` is documented as *"flag a read whose name
is not in the **current scope's** `known`"*. It descends into `$( )` bodies via
`check_cmdsub_reads`, which is why `$(int y=5; echo $y)` accepts and
`$(int y=5; …); echo $y` — DD#86's own worked example — rejects.

The file states the design intent plainly: *"the one traversal below drives a
single `record` per variable read; the sink decides what to do — so the two
passes can never diverge on 'what counts as a read'."*

`check_scalar_splat` was added later, for a good reason its own comment
records: a splat is not a scalar read, so `@nosuch` was reaching codegen and
ICE-ing. But it was added *beside* that traversal rather than *into* it, with
its own flat union. It is the one read-check that did not join the shared sink
the file was designed around, and it is the one that diverged.

## The fix

A splat is a `ParameterExpansion` whose operator starts with `arrvals`. The
reason it fell through the existing sink is subtle and worth recording: a
disposition is an **operator**, and `record_param` treats any operator as
SB5031's opt-out — `${x:-…}` is exactly how you say "this may not exist".

For a splat that inference is wrong. `@{a:-z}` says the array may be *empty*,
not that it may be *undeclared*; `a` must still be a collection in scope. So
splats route to a new `record_splat` that does not take the opt-out, and
reports with the splat's own wording — "write `${a:-DEFAULT}`" is not advice
that makes sense for an array.

`check_scalar_splat` keeps SB5079 (`@x` on a scalar), which is a type question
rather than an existence one, and its union is sound for that under
no-shadowing. `subscript_subject_is_scalar` already requires
`introduced.contains(name)`, so removing the undefined branch could not turn an
unknown name into a spurious SB5079.

Nothing needed widening. The scope-aware traversal already went everywhere the
answer lives.

## Verified

Both directions, against the pre-fix binary and the post-fix one: the in-body
splat compiled only after, the function-local splat at top level compiled only
before. Specs 75.108–75.111 in the existing 75.x splat cohort. Predicted 6745,
swept 6745, zero failing.

## The habit worth keeping

The first filing was not careless about evidence — it had a measurement, a
diagnosis, and a DD citation. It was careless about *symmetry*. Having found a
check that was wrong in one direction, I described the other direction from the
code's apparent intent instead of running it.

One probe, three lines long, turned a scoping question for DG into a bug with a
settled fix.
