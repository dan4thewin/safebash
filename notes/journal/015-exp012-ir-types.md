# Experiment 012: IR type definitions

**Goal:** Define Rust types for the safebash intermediate
representation — the layer between tree-sitter CST and SSA.

**Actions:**
- Probed tree-sitter-bash CST for all construct types: commands,
  pipelines, assignments, declarations, if/elif/else, while/until,
  for, case, functions, redirections, lists (&&/||), subshells,
  groups, and all expression types
- Designed a type hierarchy in `src/ir.rs` covering:

**Statement types (StmtKind):**
  Assignment, Declaration, Command, Pipeline, If, While, For,
  Case, Function, Redirected, List, Subshell, Group, Comment

**Expression types (ExprKind):**
  Literal, DoubleQuoted, SingleQuoted, SimpleExpansion,
  ParameterExpansion, CommandSubstitution, ArithmeticExpansion,
  Concat, Array, Test

**Supporting types:**
  Span (source location), DeclKeyword (declare/local/export/readonly),
  RedirectOp (8 variants), ListOp (And/Or), TestExpr (unary/binary/
  not/and/or/paren), ParameterExpansion (name + operator + operand),
  ElifClause, CaseItem (with fallthrough flag)

**Design principles applied:**
- Every CST construct that survives the safe-subset validator has
  a corresponding IR node
- IR is a tree (SSA adds graph edges in phase 4)
- String data is owned — IR is independent of source text
- Spans preserved for error reporting
- Comments preserved for round-trip fidelity

**Tests:** 32 tests verifying every IR node type can be constructed,
all enum variants are distinct, Span::from_node works with real CST
nodes, and a complete Program models a multi-statement script.

**Conclusion:** The IR type system is complete enough to represent
any bash script that passes the safe-subset validator. Ready for
CST→IR lowering in experiments 013–018.

---

