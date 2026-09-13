title: "docs/GUIDE.md still describes the tree-sitter/CST pipeline"
status: done
priority: P3
kind: task
depends: []
gates:
  - "no section of docs/GUIDE.md refers to a CST, `lower.rs`, `rule.rs`, `rules/*.rs`, `taint.rs`, or `tree-sitter-bash-safebash` as part of the main compiler"
  - "the file/module tour matches the crate layout that actually exists"

# GUIDE.md describes a pipeline that no longer exists

Commit `1c0e64a` deleted `lower/`, `rules/`, the standalone taint pass, the
logos-CST glue, and the tree-sitter dependency from the main compiler. The
parser now yields the AST directly. `docs/GUIDE.md` was never updated to
match.

Fixed opportunistically while landing `drop-removed-builtin-table` (the
Phase 1 / Phase 2 sections and the pipeline diagram, because that task
rewrote the SB-code table inside Phase 2). The rest is still stale:

- **line ~93** — the module tour lists `lower.rs — CST → IR lowering with
  coercions`. No such file.
- **line ~117** — `convert.rs — CST-walking conversion engine`. This one is
  *half* right: `sash-convert` genuinely does walk a tree-sitter CST, so
  check whether the surrounding text implies the main compiler does too.
- **line ~267** — a whole subsection, "`lower.rs` converts CST → IR. This is
  where coercions happen…". Coercion is now spread across the parser and
  the transform; this needs rewriting, not deleting.

Worth a single pass over the whole document rather than more spot fixes: a
reader currently cannot tell which statements are current. Check the crate
tour against `sash/src/` as it stands, not against the file names.

## Done 2026-09-13 (journal 295)

Both gates met, verified mechanically: every `*.rs` named in the
repository-structure block exists, and every file in `sash/src` is named.

Beyond the three sites the filing lists, the pass found the document stale
in ways a reader could not have detected:

- The crate tour was **a pipeline behind** — it listed `transform.rs`,
  `codegen.rs`, `ssa_printer.rs`, `printer.rs`, `emit_source.rs` and
  `known_externals.rs` as if they were files, and omitted `parse/`,
  `resolve.rs`, `ssa_taint.rs`, `regex_validate.rs` and `emit_c.rs`
  entirely. `transform/` and `codegen/` are directories of 13 and 12 files.
- **The C backend was not in the Backends section at all** — the thing
  half the spec suite runs through, and the reason a codegen regression
  usually shows up as one failing spec in one mode. Added, framed as the
  differential oracle it is rather than as a portability feature.
- The tree-sitter note pointed at `~/src/tree-sitter-bash-safebash`, a fork
  that drove the deleted pipeline.
- "Phase 4: Lower" described a pass that no longer exists. Rewritten to say
  where the coercions went (parser for the single-form ones, transform for
  the type- and resolution-dependent ones) rather than deleted, since a
  reader looking for "where do coercions happen" needs an answer.
- "For New Contributors" told people to run `mk test` (which does not pass
  at HEAD) and pick a task from `TODO.md` (which does not exist).
  Rewritten around `notes/status.md`, `notes/decisions/`, the spec suite,
  and — explicitly — **re-measure the filing before acting on it**.
- The regex section gained the two boundaries that actually bite: the
  byte-oriented engine (and *why* — the `lib9.h` ASCII-only `chartorune`
  stub) and the interior-NUL truncation, both cross-referenced rather than
  restated.

**`AGENTS.md` was stale in the same places** and was corrected alongside:
it listed `emit_janet.rs`, `emit_source.rs` and `oracle.rs` as key source
files and `oracle_tests.rs` / `janet_tests.rs` as test files — none of
which exist — gave wrong test counts for the files that do, said "57 design
decisions … up to DD#69" when there are 95 up to DD#107, and hard-coded
complexity means (cc 12.46 / cog 6.58) that have since ratcheted to
**4.04 / 2.05**, making the quoted 3× bar nearly three times too loose. The
hard-coded numbers are replaced with the query that produces them.
