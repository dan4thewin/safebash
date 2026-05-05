title: Verify tree-sitter grammar supports - prefix syntax
status: ready
severity: design
depends: []
gates: [21.1]
  - commit

DD#21 introduces `- cmd` as error suppression syntax. Check whether
the tree-sitter-bash-safebash fork already has grammar support for
the `-` prefix on commands. If not, add it.

This blocks all DD#21 implementation work.
