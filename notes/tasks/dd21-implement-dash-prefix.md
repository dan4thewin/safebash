title: Implement - prefix error suppression (DD#21)
status: ready
severity: feature
depends:
  - dd21-grammar-dash-prefix
gates:
  - tests
  - commit

Implement `- cmd` syntax: suppress command failure (equivalent to
`cmd || true`). Needs lowerer recognition, IR representation,
transform/SSA handling, and codegen to discard exit status.

Also implement pipeline coercion for bash-mode input:
- Scripts with `set -o pipefail`: no `-` inserted, compiler flags
  unchecked segments
- Scripts without pipefail: insert `-` on non-terminal segments
  to preserve bash default semantics

`--emit-source` / `--annotate` must show the inserted `-` prefixes.
