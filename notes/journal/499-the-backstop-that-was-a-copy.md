# 499 — The backstop that was a copy

Ranking the cold lines after the DD#81 work put a function I had written
myself three sessions earlier at the head of the queue:
`is_capture_condition_body`, three cold lines out of twelve.

That is an uncomfortable place to find one's own code, and the reason it was
cold turned out to be the interesting part.

## Three arms, all shadowed

The three cold lines were the three rejections:

| line | shape | why cold |
|---|---|---|
| 1393 | `let [stmt] = body else { return false }` | `$(a; b)` — multi-statement |
| 1399 | `_ => false` on the head | `$(${c} hi)` — non-literal head |
| 1401 | `_ => false` on the body | `$(if … { … })` — compound body |

Every one of them is category 2 — validate-shadowed. `run_semantic_checks`
ends by printing its diagnostics and calling `std::process::exit(1)`, and that
happens *before* `type_check_or_exit`, which happens before `transform_or_exit`.
So anything `validate` rejects cannot reach the transform at all. All three
shapes draw SB4025 from `validate::is_capture_condition_rhs` and die there.

No spec can warm them. That much is the usual answer, and the usual next step
is a unit test.

## Except they were not a backstop

Putting the two functions side by side is what changed the conclusion:

```
validate::is_capture_condition_rhs      transform::is_capture_condition_body
  let [stmt] = stmts.as_slice() else      let [stmt] = body else
  match peel_capture_wrappers(…)          match peel_capture_wrappers(…)
    Pipeline(p) => …                        Pipeline(_) => true
    Command(c) => …                         Command(c) => …
    _ => false                              _ => false
```

I had written a private fork of the gate. The stored rule about private
duplicates of shared walkers names exactly this hazard, and the three cold
lines were the fork's share of the duplication — the arms that exist only
because the original has them.

DG's rule for an unreachable line is the one that decides it:

> an unreachable line earns its place only if it backstops a regression
> **orthogonally**; one re-asking the gate's own question on the same data adds
> nothing.

These arms re-ask `validate`'s question on the *same AST*, one pass later.
They are not orthogonal in any degree. A unit test pinning them would have
pinned the duplication in place and called it coverage.

## What the transform actually needs to ask

Exactly one thing, and it is a thing `validate` genuinely cannot answer:
**does the head name a user function?** `validate` runs before resolution, so
`$(f 2>&1)` arrives looking exactly like an external command. A bare `$(f)`
never reaches the capture path at all — it lowers to `FuncCall` — so without
this check the redirected spelling would drift off DD#88's side of the line
from its unredirected twin. That is spec 88.28, and it is the function's whole
remaining job.

```rust
fn capture_condition_head_is_user_fn(body: &[Statement], state: &VersionState) -> bool {
    body.iter().any(|stmt| match peel_capture_wrappers(&stmt.kind) {
        StmtKind::Command(c) => {
            matches!(&c.name.kind, ExprKind::Literal(n)
                if state.function_bodies.contains_key(n.as_str()))
        }
        _ => false,
    })
}
```

The caller inverts the sense. Every line is now reachable: the `Command` arm
by `$(cmd 2>f)` and by 88.28, the `_` arm by every pipeline capture.

## The pipeline arm is not an oversight

The old code had `Pipeline(_) => true` with no per-stage check, and a user
function *is* legal as a pipeline stage. That looked like a hole next to the
single-command exclusion, so I checked it before touching anything:

```sh
fn myfn { /bin/cat }
if x=$(/bin/echo hi | myfn) { echo "got ${x:!}" }   # → got hi
fn myfn { /bin/echo hi }
if x=$(myfn | /bin/cat) { echo "got ${x:!}" }       # → got hi
```

Both compile and both are correct. DD#88 excludes capturing *a function's
output* as a condition because a function conveys status via `return`; a
pipeline's status is the pipeline's, and a function inside one is just a stage.
The asymmetry is the rule working, not a gap. The new function asks nothing
about pipelines for that reason, and the `_` arm covers them.

## Where the regression check went instead

Deleting a defensive arm is only defensible if something else catches the
regression it was nominally guarding. Nothing did: there was no spec anywhere
in the suite pinning *any* of the three rejections.

So that is what the three cold lines were really telling me — not "write a
unit test here", but "the gate that owns this question is untested". Specs
88.29-88.31 pin them on `validate`, which is where a reader looking for the
rule would go, and where a change to the rule would be made.

A copy of a classifier in a later pass is not a second opinion. It is a second
thing to keep in sync, and the copies in this very file have drifted away from
their originals rather than catching them — the `Redirected`-peeling
disagreements of journal 495 were six of those.

## Result

8325 checks, zero failures, zero flips, exactly the six new ids. `mk test` 118.
Three cold lines gone by deletion rather than by unit test, and three specs
added to the pass that actually decides the question.
