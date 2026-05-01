# Experiment 014: CST→IR complete lowering

**Goal:** Bundle experiments 014–018 into a single experiment that
lowers all remaining CST constructs to IR: pipeline, list (&&/||),
if/elif/else, while/until, for, case, function, redirected, negated,
subshell, group, and test expressions.

**Actions:**
- Probed tree-sitter CST for every remaining construct type
- Extended `lower.rs` from 013's foundation (assignments, declarations,
  commands, expressions) to handle all 14 statement types
- Extracted `lower_body()` as shared helper for compound bodies
  (program, function, do_group, compound_statement, else_clause)
- Added test_command lowering → `TestExpr` (unary, binary)

**Bug found and fixed:** Negated pipelines. Tree-sitter puts `!`
inside the pipeline as a `negated_command` wrapping the first element,
not outside the pipeline. Initial code treated `!` as always wrapping
the entire pipeline — this missed cases like `! echo a | grep b`
where `negated_command` is a child of `pipeline`. Fixed by detecting
`negated_command` inside `lower_pipeline()` and setting the negated
flag there.

**Tests:** 33 integration tests covering:
- Pipeline (2, 3 commands, negated command, negated pipeline)
- List (&& and ||)
- If (simple, else, elif+else, condition-is-test)
- While and Until loops
- For (single and multiple words)
- Case (multi-item, wildcard pattern)
- Function (simple, with local declaration)
- Redirected (write, append, read, fd, multiple)
- Subshell and Group
- Test expressions (unary and binary in conditions)
- Carry-forward (assignment, expansion, declaration, command)
- Full script integration and statement count verification

**Conclusion:** CST→IR lowering is complete. Every bash construct
that passes the safe-subset validator can now be represented in
the IR. Experiments 015–018 from the original roadmap are absorbed
into this single experiment. Phase 3 is effectively one experiment
away from completion (019: IR printer).

---

