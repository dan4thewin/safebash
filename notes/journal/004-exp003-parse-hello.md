# Experiment 003: parse hello — first successful parse

**Goal:** Parse `echo "hello world"` and inspect the resulting tree.

**Actions:**
- Called `parser.parse(source, None)` on a one-line script
- Printed root node kind and children

**Results:**
```
root: program [(0, 0)-(0, 18)]
  command: "echo \"hello world\""
```
- Root is always `program`
- `echo "hello world"` parses as a single `command` node
- The CST is concrete — it preserves quotes, whitespace positions

**Conclusion:** The parse API is straightforward. `parse()` returns a
`Tree` with a `root_node()`. Nodes have `.kind()` (string), `.child()`
accessors, and `.utf8_text()` to recover source text. The tree is
read-only and navigated via cursors or child iterators.

---

