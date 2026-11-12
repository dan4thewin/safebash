title: "audit: a single report file listing every bless / taint bypass site"
status: blocked
priority: P2
kind: hardening
depends: []
gates:
  - "one generated report enumerates every taint-relevant construct: bless / bless-all / map-bless, the `:!` taint-off operator (DD#91), --no-taint / --no-taint-cst usage, and (for context) every external-capture taint source"
  - "each entry carries file:line and the variable/subject, so a security reviewer can audit every bypass without grepping"

## Why (DG 2026-09-04)

Filed while adding the `:!` operator (DD#91) — a sanctioned per-use taint bypass
(`$foo:!` = `:~` elide + taint guard off, the middle ground between `bless` and
`--no-taint`). DG: prefer, over recording `:!` sites in `--manifest`, "a new
audit file with all bless/taint related hits" — a broader audit surface than
just `:!`.

The adversarial audit's highest-priority target is validator bypass (AGENTS.md).
A single generated report of every place taint is deliberately cleared or
suppressed makes that review mechanical:

- `bless v [[…]]` / `bless x in a [[…]]` / `bless k v in m [[…]]` (predicate blesses)
- **`:!`** taint-off expansions (DD#91) — the predicate-less bless
- `--no-taint` / `--no-taint-cst` invocations (whole-program bypass, DD#82)
- external capture taint SOURCES (`$(getenv)`, `$(argv)`, `$(external)`, read
  from fd) for context — where tainted data enters

## Shape (to design with DG)

- A compiler mode (e.g. `--audit-taint`) or a standalone pass over the IR that
  emits the report (text / json), OR a section appended to `--manifest`.
- Columns: kind (bless/bless-all/map-bless/taint-off/no-taint/source), file:line,
  subject variable/name, and the predicate text for blesses.
- Possibly a repo-committed snapshot the audit diffs against (like spec.db) so a
  new bypass shows up in review.

## Found

2026-09-04, DD#91 (`:!` operator), journal 243.

## Re-measured 2026-09-13

The filing is written in the retired **`bless`** vocabulary; the rename to
**`attest`** happened after it was filed. Read every `bless` below as
`attest`. Two of its factual claims no longer hold:

- **`--no-taint-cst` does not exist.** `sash --help` lists only
  `--no-taint`. Passing `--no-taint-cst` is *silently ignored* and the
  program compiles — which is the separate, already-filed
  `cli-usage-and-unknown-flags` (P2), not part of this task. The CST it
  named is gone too (commit 1c0e64a).
- **`--manifest` has nothing to append to, taint-wise.** Measured: it
  emits only the external-command manifest (absolute paths, one per
  line), and prints *nothing at all* for a program with taint sources and
  an `attest` but no external commands. The "append a section to
  `--manifest`" option is therefore a bigger change than the filing
  implies: it would give `--manifest` a second, differently-shaped
  payload.

**The surface to enumerate, in current vocabulary:**

| kind | spelling | where it lives |
|---|---|---|
| scalar attest | `attest v [[…]]` | `AttestForm::Scalar`, `ir.rs:589` |
| indexed attest | `attest x in a [[…]]` | `AttestForm::Indexed` |
| map attest | `attest k v in m [[…]]` | `AttestForm::Map` |
| bulk attest | — | `SsaInst::AttestAll` |
| taint-off | `$v:!` (DD#91) | expansion modifier |
| whole-program | `--no-taint` (DD#82) | CLI |
| sources (context) | `$(getenv)`, `$(argv)`, `$(external)`, read from fd | — |

`AttestPredicate` (`ir.rs:712`) carries the evidence text, so the
"predicate text for blesses" column is available without re-parsing.

## The fork, sharpened — for DG

The filing's "Shape (to design with DG)" is still open and is **not**
being decided unilaterally. Three options, re-costed against the tree as
it stands:

1. **A new `--audit-taint` mode.** Cleanest separation: one flag, one
   payload, no existing output changes shape. Costs a CLI flag and a new
   emitter. Composes with `--manifest` rather than colliding with it.
2. **A section appended to `--manifest`.** No new flag, but measured
   above: `--manifest` currently emits a bare list of paths and nothing
   else. Adding a keyed section makes its output polymorphic, and every
   existing consumer parsing it as one-path-per-line has to change.
3. **A standalone pass over the IR** invoked by the audit harness rather
   than the compiler CLI. Keeps the compiler surface unchanged, but the
   report is then only reachable from the harness — a user auditing their
   own program cannot run it.

**Recommendation: option 1.** The gate says "so a security reviewer can
audit every bypass without grepping", and a reviewer is not always the
person running the test harness. A dedicated flag keeps `--manifest`'s
contract intact and makes the report a first-class artifact rather than a
harness side effect. The committed-snapshot idea from the filing (diff a
checked-in report so a new bypass shows up in review) works with any of
the three, and is worth doing whichever is chosen — note that `spec.db`,
the precedent the filing cites, has since been **untracked** on purpose,
so the snapshot should be a small text file, not a database.

**Blocked on DG for the shape.** Everything else above is measured and
ready to implement once the shape is chosen.
