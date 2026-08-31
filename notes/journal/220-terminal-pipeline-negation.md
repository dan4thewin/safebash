# 220 — `!` on the terminal pipeline segment (`a | ! b`)

DG's `return echo abc | ! grep -q z` (journal 219) uses `| ! grep` — a `!` on a
**non-first** pipeline stage. After the return-composite fix that line compiled
and ran, but returned the wrong answer: the `!` was silently dropped. This
session designed the semantics with DG and implemented it across all three
backends.

## The design (DG, discussed → ratified)

`-` and `!` are both *per-segment* pipeline operators. `-Si` suppresses a
segment from the DD#55 aggregate; the **terminal** segment supplies the
pipeline's result, so it may take the same `!` a standalone simple command
takes. Four rulings:

1. **Terminal-only.** `a | ! b` (negate the final stage) is allowed; a `!` on a
   non-terminal stage (`! a | b`, `a | ! b | c`) is SB4040. A non-terminal
   status is plumbing — negating it and folding it into the aggregate is
   illegible.
2. **Per-segment, not whole-pipeline.** `! b`'s status is inverted (0↔nonzero)
   *before* the DD#55 worst-of fold — it does NOT negate the whole aggregate
   (that would revive the "how far does `!` reach?" ambiguity DD#70 forbids).
   `echo abc | ! grep -q z` is true exactly when grep fails.
3. **No `-`/`!` combo on one segment.** `- ! cmd` (SB4019, existing) and its
   mirror `! - cmd` (SB4019, new) are both rejected — suppress discards a
   status, `!` inverts it; both at once is contradictory.
4. **`return ! X | Y` → SB4040** (was a confusing bare-`!` SB4014).

The elegant part: "terminal-only" makes the old leading-`!` rejection fall out
for free. `! a | b` is `(!a) | b` — a negation of segment 0, a non-terminal — so
it's rejected by the *same* rule, now with a precise message ("`!` may only
negate the final stage") instead of the vaguer "can't tell how far `!` reaches."

## The bug it fixed

`append_pipeline_stage` flattened a parsed `! b` stage
(`Pipeline{[b], negated:true}`) via `commands.extend(p.commands)` — **dropping
the `negated` flag**. The parser already rejected non-terminal `!` for free (a
`!`-stage with a trailing `|` hits `parse_negated`'s `pipeline_operand` check →
SB4040), so the ONLY defect was the terminal stage's silently-dropped negation.

## Representation — a per-segment `Negated` wrapper (symmetric with `Suppressed`)

- `StmtKind::Negated(Box<Statement>)` — a per-segment negation marker, parallel
  to `Suppressed`. `append_pipeline_stage` converts the `! b` stage into
  `Negated(b)` instead of flattening it away.
- SSA `Pipeline` gains `negated: Vec<bool>` (aligned with `stages`, parallel to
  `suppressed`). `handle_regular_pipeline` peels `Negated` → inner + sets the
  mask bit.
- Native `wait_for_children` and c `emit_pipeline_wait` invert a negated stage's
  status (`(st==0)?1:0`) before the worst-of update. janet mirrors it in
  `emit_pipeline_do`.
- The standalone `! cmd` path (`Pipeline.negated: bool` → `NegateTestResult`) is
  **untouched** — `Negated` appears only inside a multi-stage pipeline. Chose
  the wrapper over reusing `Pipeline.negated` so rustc's exhaustiveness forces
  every emitter to handle it (no silent drops — the exact bug class here), and
  the standalone path carries zero regression risk.

## Secondary fixes (DG approved)

- `parse_return_producer_head` now dispatches a leading `Tok::Bang` →
  `parse_negated`, so `return ! X` is a valid negated producer and
  `return ! X | Y` reports SB4040 (was SB4014). Extracted the producer head into
  its own fn — `parse_return` cc 14→7 (cleared the gate).
- `parse_negated` rejects a `Suppressed` unit (`! - cmd`) → SB4019, mirroring
  `- ! cmd`.
- SB4040 message rewritten to the terminal-only framing.

## Verified (vs bash)

| input | sash | bash |
|---|---|---|
| `echo abc \| ! grep -q z` (z absent → !fail → true) | T | T |
| `echo xyz \| ! grep -q y` (y present → !ok → false)  | F | F |
| `echo abc \| cat \| ! grep -q z` (3-stage terminal)  | T | T |
| `! a \| b`, `a \| ! b \| c` (non-terminal)           | SB4040 | syntax err |
| `- ! cmd`, `! - cmd` (combo)                          | SB4019 | — |
| `return ! [[ 1 = 2 ]]` → !false → true               | T | T |

bash rejects `| ! cmd` as a syntax error entirely; sash is deliberately more
permissive on the terminal (its `!`/`-` pipeline operators already diverge —
DD#70, DD#55).

## Specs — `spec/errors.sh` bng.7–bng.12 (plan 91 → 97), all three modes

Runtime terminal negation (bng.7/8/9), non-terminal reject (bng.10), `!-` combo
(bng.11), `return ! X | Y` (bng.12). Full 3-mode sweep vs HEAD `25d7d66`: **0
regressions, +18 passing** (bng.7-12 × 3), 0 new failing. Rust cargo suites:
failure set identical at baseline (stash-compared — 8/8 both sides; pre-existing
`case`/`echo -e` rot, not the gate). DD#70 addendum + DD#55 pointer.
