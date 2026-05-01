# Experiment 013: CST→IR lowering — assignments, declarations, commands

**Goal:** Build the `lower` module that converts tree-sitter CST
nodes into the IR types defined in experiment 012.

**Scope:** This experiment covers three statement types plus the
full expression lowering pipeline:

**Statements lowered:**
- `variable_assignment` → `StmtKind::Assignment`
- `declaration_command` → `StmtKind::Declaration`
  (declare/local/export/readonly with flags and assignments)
- `command` → `StmtKind::Command` (name + arguments)
- `comment` → `StmtKind::Comment`

**Expressions lowered (all RHS/argument positions):**
- `word` → `ExprKind::Literal`
- `string` (double-quoted) → `ExprKind::DoubleQuoted` with parts
- `raw_string` (single-quoted) → `ExprKind::SingleQuoted`
- `simple_expansion` ($var) → `ExprKind::SimpleExpansion`
- `expansion` (${var:-...}) → `ExprKind::ParameterExpansion`
- `command_substitution` → `ExprKind::CommandSubstitution`
- `arithmetic_expansion` → `ExprKind::ArithmeticExpansion`
- `concatenation` → `ExprKind::Concat`
- `array` → `ExprKind::Array`
- `command_name` → unwrap to inner expression
- Subscript expansion (${arr[@]}) handled

**Architecture:**
- `lower_program()` — entry point, walks top-level children
- `lower_statement()` — dispatches by CST node kind
- `lower_expr()` — recursive expression lowering
- `lower_string()` — double-quoted string decomposition
- `lower_parameter_expansion()` — ${...} with operators

**Tests:** 27 integration tests covering every assignment form
(literal, empty, quoted, expansion, param-expansion, cmd-sub,
arithmetic, array, concatenation, interpolated string),
declarations (readonly, local, export, declare with flags, local -n),
commands (simple, multi-arg, expansion arg, no args), comments,
multi-statement programs, span accuracy, and a realistic script.

**Conclusion:** The expression lowering is complete enough to
handle all value positions in assignments and command arguments.
This foundation is reused by experiments 014–018 for pipeline,
if, while, for, and function lowering.

---

