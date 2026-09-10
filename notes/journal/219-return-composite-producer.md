# 219 — `return <composite producer>` (SB4037 false-positive fix)

DG, at the demo: `fn f { return echo abc | ! grep -q z }` reported **SB4037**
("predicate must return on all paths") — but it looks clean. It is.

## Root cause — the producer parse stopped at the first command

`parse_return` dispatched on the producer's leading token and parsed exactly one
unit — a command, a `[[ ]]`, or a `(( ))`. It never folded a trailing pipeline /
`&&`-`||` list / redirect. So

```bash
return echo abc | grep -q z
```

parsed as `Pipeline([ ReturnProducer(echo abc), grep -q z ])` — the `return` was
trapped in **stage 0**, and `grep` ran as an ordinary, uncaught stage after it.

Two symptoms fell out of that one mis-parse:

- **In a predicate** (`fn f { return … | … }`): the function body's only
  top-level statement is the *pipeline*, not the `return`. `stmt_always_returns`
  sees a bare `Pipeline`, not a `ReturnProducer`, so `check_predicate_definite_
  return` fires **SB4037**. That's the report DG hit.
- **As a statement** (`return … | …` at top level): worse — the trailing stages
  run uncaught, so the `return` doesn't actually govern the pipeline and a
  non-zero later stage aborts (`grep: command failed`).

The whole class was broken: `return A | B`, `return A && B`, `return A || B`,
`return A > f`.

## Fix — fold the composite into the producer

`parse_return` now routes the parsed producer through the same
`maybe_wrap_redirects` that a normal statement gets, before wrapping it in
`ReturnProducer`:

```rust
let folded = maybe_wrap_redirects(scanner, p)?;
Ok(Some(Statement { kind: StmtKind::ReturnProducer(Box::new(folded)), span }))
```

Now `return A | B` parses as `ReturnProducer(Pipeline([A, B]))` — the whole
composite is the producer. The lowering was already correct:
`lower_return_producer` lowers the inner statement in condition context and
un-checks the last condition inst, so a `Pipeline`/`List` producer runs its
stages caught, the DD#55 aggregate becomes the status, and the `return`
terminates. Only the parse was wrong.

`ReturnProducer` is not `is_compound`, so the *outer* `maybe_wrap_redirects`
(the one `dispatch_statement` applies after `parse_return` returns) still runs —
but since `parse_return` has already consumed the pipeline, that outer call is a
harmless no-op.

## Verified (vs bash)

| input | sash | bash |
|---|---|---|
| `return echo abc \| grep -q a` | yes | yes |
| `return echo abc \| grep -q z` | no  | no  |
| `return [[1=1]] && [[2=2]]`     | yes | yes |
| `return [[1=1]] && [[2=3]]`     | no  | no  |
| `return [[1=2]] \|\| [[3=3]]`   | yes | yes |

Regression-checked the single-unit producers still parse: `return grep`,
`return [[ ]]`, `return (( ))` unchanged. `parse_return` cc 12 / cog 4 — clears
the gate, no split.

`&&`/`||` test note: validate a list producer with **non-stdin** operands
(`[[ ]]`), not two greps — `return grep -q a && grep -q b` reads misleadingly
because the first grep consumes stdin and the second sees EOF.

## Specs — `spec/functions.sh` 74.19–74.23 (plan 63 → 68)

- 74.19/74.20 `return <pipeline>` match / no-match — **all three modes**.
- 74.21/74.22/74.23 `return A && B` (both-true / second-false) and
  `return A || B` — **native,c only**.

janet gated: janet emits return-producers from the AST and **collapses** an
`&&`/`||` list to a single operand — it emits `(if true 0 1)` regardless of the
operands, passing some cases by luck and failing 74.22. Pipelines are genuine in
janet (74.19/74.20 run there). Filed `notes/tasks/janet-return-composite-
producer.md`.

Native + c: 0 regressions (authoritative latest-run-per-test diff vs HEAD
`071fbb7`); 12 new passing (5 native + 5 c + 2 janet), 0 new failing.

## Also observed — `X | ! Y` (a separate, pre-existing bug; NOT fixed here)

DG's original line negates the **final** stage: `… | ! grep -q z`. A negated
**non-first** pipeline stage currently parses but **silently drops the `!`**
(result inverted) and aborts in statement context. It's pre-existing (it doesn't
go through `parse_return`) and inconsistent: `! X | Y` (negated *whole*
pipeline) is already SB4040, but `X | ! Y` is silently accepted-but-wrong. Left
for a DG decision — reject (SB4040-consistent, DD#70-aligned) vs support (negated
final stage governs pipeline status). Raised separately; not in this commit.
