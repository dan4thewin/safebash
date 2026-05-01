# Experiment 007: Reject eval — first validator rule

**Goal:** Implement a CST validator that rejects constructs enabling
dynamic code execution: `eval`, `source`/`.`, `${!var}`, `declare -n`,
and variable-as-command-name (`$cmd args`).

**Actions:**
- Explored tree-sitter-bash CST output for each target construct to
  determine node kinds and structure (using 005's parser as a probe)
- Implemented `validate()` — a recursive CST walker that emits
  `Diagnostic` structs with line/col, error code, and message
- Defined five safebash diagnostic codes:
  - SB001: variable as command name (command_name contains expansion)
  - SB002: eval
  - SB003: source / dot-source
  - SB004: declare -n / local -n (nameref)
  - SB005: ${!var} indirect expansion
- Wrote 20 tests covering rejection of each construct (including
  nested contexts: inside functions, inside if), acceptance of safe
  constructs (normal commands, `$(cmd)`, arithmetic, `declare -i`,
  `declare -r`), multi-violation reporting, and line/column accuracy

**Key observations:**
- tree-sitter-bash parses `eval` as a regular `command` node with
  command_name "eval" — no special node kind. Detection is by
  matching the word text.
- `${!var}` indirect expansion appears as an `expansion` node with
  a `!` child node. This also catches `${!prefix*}` enumeration.
- `declare -n` uses `declaration_command` node kind, distinct from
  `command`. The `-n` flag appears as a `word` child.
- Variable-as-command: when `$cmd` is the command name, the
  `command_name` node's first child is `simple_expansion` instead
  of `word`.

**Conclusion:** First validator rule works. The `Diagnostic` struct
and `validate()` walker establish the pattern for all subsequent
rules (008–011).

---

