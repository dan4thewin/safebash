title: Reject extglob patterns (DD#20/DD#39)
status: ready
severity: correctness
depends: []
gates: [20.4, 39.1a, 39.1b]
example: check (sash/src/rules/process_substitution.rs)
index_keyword: extglob
  - tests
  - commit

DD#20 (amended by DD#39) says extglob is rejected — `@()`, `+()`,
`!()`, `?()` are not in the safe subset. But no validator rule
catches them. tree-sitter emits `extglob_pattern` nodes for these;
`lower.rs:1067` parses them without complaint.

Add a validator rule that rejects `extglob_pattern` nodes with a
clear diagnostic (suggest regex form per DD#39).
