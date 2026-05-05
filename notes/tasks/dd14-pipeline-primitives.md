title: Pipeline and command substitution primitives
status: ready
severity: correctness
gates: [14.1, 14.2, 14.3]
---

Verify pipelines, command substitution, and background commands
use fork/exec/wait/pipe primitives per DD#14.
