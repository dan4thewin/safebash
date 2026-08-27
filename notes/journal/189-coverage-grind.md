# 189 — coverage grind: reliable method + first batches

Continuation of journal 188. DG directed: work through the release
missing-lines file by file — remove shadowed/unreachable code (unit-test
followup only if a security regression), add specs for reachable-untested,
re-measure in release only, commit in batches.

## Reliability settled (I was wrong, then right)

I briefly claimed release-mode coverage was "unreliable" (release vs debug
read 91% vs 48% on validate.rs). DG pushed back: why care about debug, and
what proves release wrong? Answer: **nothing** — the release build is what
ships and what the spec suite exercises, and every "missing" line I actually
verified was genuinely dead-or-untested:

- validate.rs:380 (export) → parser-shadowed dead (parser.rs:1679 rejects
  export first).
- validate.rs:743-752 (prefix `++` → SB5007) → shadowed: every `++x` yields
  SB4020 from the arith parser, never reaching the validate arm.
- validate.rs:648-659 (SB5023) → genuinely untested: the existing `=~` spec
  uses a *valid* pattern (`wordsplit.sh:79` asserts a match).

The 91%↔48% swing is just LLVM line-counting granularity (debug has no
inlining → larger denominator), not release being wrong. **Release coverage
is the correct, reliable measurement.** Retracted the debug detour and the
SB-code-proxy pivot.

## Harness note (finicky, not unreliable)

`cargo llvm-cov` only instruments if `cargo llvm-cov clean --workspace` runs
first (it forces a rebuild); otherwise a prior `cargo build --release`
fingerprint is reused and the specs run *uninstrumented* (0 profraws). And
the clean wipes sash-convert/sash-expand, so rebuild the whole workspace
before running convert/expand specs. After a measurement, force-rebuild
uninstrumented (`cargo clean -p sash --release`) or the shipped binary stays
instrumented (slower, drops profraws).

Reliable release baseline (native+c, compiler only, no janet): **87.69%
lines** (22337 lines, 2750 missed), 91.5% functions.

## Batches so far (all pushed, 0 regressions, native gate 0)

- `4d94eb9` remove 4 dead methods (predecessors/as_u8/from_str/deferred_at).
- `0f36b7a` remove parser-shadowed check_export_or_declare; coverage.sh cov.1-3
  (mutation/comma-in-condition → SB5007).
- `877d55e` coverage.sh cov.4-10: code-asserting tests for 7 reachable-untested
  diagnostics (SB5023/5019/4037/5016/4015/4029/5021).

## Realistic scope

The remaining ~2750 missing lines span ~20 files; each cluster needs per-item
reachability analysis (shadowed-dead → remove-and-verify-behaviour-preserved;
reachable → spec). It's a multi-session grind. Method is settled; continuing
in batches.
