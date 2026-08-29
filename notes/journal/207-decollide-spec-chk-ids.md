# 207 — De-collide spec chk ids (suite-wide)

Follow-up to the 14.7/14.8 discovery (journal 206 / the taint reframe): `chk`
ids are a flat GLOBAL namespace, and a duplicate — cross-file or two executing
`chk` lines in one file — makes the later write silently overwrite the earlier
in `spec.db`, **masking failures**. DG: "there are more id collisions? let's fix
that." Audited the whole suite and fixed every one.

## Audit

Two checks (see `notes/tasks/decollide-spec-chk-ids.md` for the script):
- **cross-file**: any id emitted (chk or skip_chk) by ≥2 files.
- **within-file**: any id with ≥2 *executing* `chk` lines (the common
  `chk N … else skip_chk N` mode-guard is one `chk` + skip, so it's fine).

Found: 4 cross-file pairs (12 ids) + a tangled types.sh (2-3 blocks reusing
`30.8-11` / `33.11-18`) + errors.sh `55.30`.

## Fixes (renumber the intruder / later occurrence to a free same-DD sub)

Cross-file:
- arithmetic.sh ↔ types.sh `58.10-58.15` → types.sh `58.30-58.35` (arithmetic
  owns DD#58 arith-sexpr; types.sh's are the Option-C int-func-capture tests).
- convert.sh ↔ types.sh `30.7` → convert.sh `30.30`.
- misc.sh ↔ wordsplit.sh `20.3-20.6` → misc.sh `20.30-20.33` (wordsplit owns
  DD#20).
- types.sh ↔ wordsplit.sh `62.11` → wordsplit.sh `62.30` (types owns DD#62).

Within-file (types.sh had been assembled from blocks that reused ids):
- second block `30.8-30.11` (unset semantics) → `30.40-30.43`.
- late block `33.11,33.12,33.14-33.18` → `33.40-33.46`.
- errors.sh second `55.30` (open-failure-catchable) → `55.50`.

Applied with word-boundary, line-range-scoped replacement (cross-file ids are
unique per file → file-global; within-file dups → range-scoped to the later
occurrence). Comment + `chk` + any `skip_chk` for each moved test were updated
together.

## Verification

Re-audit: **0 cross-file collisions, 0 within-file duplicate `chk`**. Affected
files (arithmetic, errors, convert, misc, wordsplit, types) all pass with
unchanged plan counts. Full `run.sh` (native janet c): authoritative
`test_status` **0 native / 0 c failing**; all renumbered ids
(58.30, 30.30, 20.30, 62.30, 30.40, 33.40, 55.50, …) present and passing. No
de-collided pair unmasked a hidden failure — every colliding pair was passing on
both sides (the masking was benign here, but the hazard is real: it hid
commands.sh 14.7 last session).

No source changes — spec-only. `decollide-spec-chk-ids.md` (done) carries the
re-runnable audit; keep it empty.
