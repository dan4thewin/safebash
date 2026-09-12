title: "docs/GUIDE.md still describes the tree-sitter/CST pipeline"
status: ready
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
