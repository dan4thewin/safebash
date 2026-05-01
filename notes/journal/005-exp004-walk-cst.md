# Experiment 004: walk CST — understanding tree structure

**Goal:** Write a recursive tree walker. Parse a script with assignment,
echo, and if/then. Print every node with indentation to reveal the
full tree structure.

**Actions:**
- Wrote `print_tree()` — recursive, prints kind + text for leaf nodes
- Parsed a script with: shebang, assignment, echo with expansion,
  if/test/-f/then/echo/fi

**Results (abridged):**
```
program
  comment "#!/bin/bash"
  variable_assignment
    variable_name "name"
    = "="
    string
      string_content "world"
  command
    command_name
      word "echo"
    string
      string_content "hello "
      simple_expansion
        variable_name "name"
  if_statement
    test_command
      unary_expression
        test_operator "-f"
        string
          simple_expansion
            variable_name "name"
    command
      command_name
        word "echo"
      string
        string_content "found"
```

**Key discoveries:**
- Variable expansions (`$name`) appear as `simple_expansion` nodes
  inside `string` nodes. This is where SC2086 (unquoted expansion)
  detection will happen — checking whether `simple_expansion` appears
  inside a `string` (quoted) or bare (unquoted).
- `command_name` wraps the command word. Arguments are sibling nodes.
- `test_command` wraps `[ ... ]`. The test operator (`-f`) and its
  argument are children of `unary_expression`.
- Punctuation tokens (`=`, `"`, `[`, `]`, `;`, `if`, `then`, `fi`)
  are all explicit CST nodes.

**Conclusion:** The CST is detailed enough for validation. Every
construct we need to check is visible as a named node type.

---

