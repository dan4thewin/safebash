# Experiment 015: IR printer

**Goal:** Pretty-print the safebash IR as indented, readable text
for visual verification of CST→IR lowering.

**Design:**
- `print_program()` → `String`: deterministic, readable, diffable
- 2-space indentation for nesting (if/while/for/case/function bodies)
- Expressions printed inline: `"hello $var"`, `${var:-default}`,
  `$((n + 1))`, `$(date)`, `('a' 'b')`, `[[ -f $x ]]`
- Redirections appended inline: `echo hello > out.txt`
- Pipelines: commands separated by `|` on separate lines
- Lists: `&&` / `||` operators on separate lines between operands
- Command substitution bodies printed inline with `;` separators
- Comments preserved verbatim (including shebang)

**Tests:** 41 tests covering:
- Every assignment form (literal, empty, quoted, single-quoted,
  expansion, param-expansion, cmd-sub, arithmetic, array, concat)
- Declarations (readonly, local -n, export)
- Commands (simple, quoted arg, no args)
- Pipeline and negated pipeline
- List (&& and ||)
- If/elif/else
- While and Until
- For
- Case with multiple items
- Function definition
- Redirected (write, append, fd)
- Subshell and Group
- Comment and shebang
- Expressions in various positions (param expansion, array subscript,
  command substitution in string)
- Full script integration roundtrip
- Determinism (same input → same output)
- Nested indentation depth verification

**Conclusion:** Phase 3 (IR construction) is complete. The pipeline
is: source → tree-sitter CST → IR → printed text. Every construct
that passes the safe-subset validator can be lowered and printed.
Ready for Phase 4 (SSA).

---

