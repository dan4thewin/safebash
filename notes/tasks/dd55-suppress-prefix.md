title: DD#55 — implement `-` error suppression prefix
status: ready
depends:
  - dd55-implicit-exit-checks
severity: feature
gates: [55.4, 55.9, 55.11]
  - tests
  - commit

DD#21 defines `- cmd` syntax. DD#55 uses it for error suppression.
Currently not implemented — no grammar, lowerer, or codegen support.

### Grammar

tree-sitter-bash-safebash needs to recognize `- cmd` as a valid
command form. The `-` prefix on a command is a sash-native construct.

### Lowerer

Recognize `-` prefix on commands. Set a flag on the IR Command node
indicating suppression.

### Transform

Propagate suppression flag to SSA instructions.

### Codegen

When suppression flag is set: skip the implicit exit check after
`sash_run`. Store exit status as 0 (or discard).

### $? scoping

After `- cmd`, `$?` reads are compile errors. The taint pass or a
new validation pass must track which commands were suppressed and
reject `$?` references that follow them.

### Interactions

- `- cmd` in pipeline: per-segment suppression (DD#21)
- `x=$(- cmd)`: suppressed capture — output captured, status discarded
- `- cmd` before `return $?`: compile error ($? unavailable)
