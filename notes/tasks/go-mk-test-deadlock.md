---
title: Investigate the installed Go mk deadlock during the test gate
status: open
priority: P2
kind: tooling
source: journal 511 (2026-10-16)
depends: []
gates:
  - reproduce and identify the runner failure independently of Rust assertions
  - default mk test completes without a runner deadlock
---

During journal 511's parser work, plain `timeout 900 mk test` exited 2 with
`fatal error: all goroutines are asleep - deadlock!` after the regex checks.
The Go stack names `main.mkNodePrereqs`, `main.mkNode` and blocked mutex/channel
operations. The runner is github.com/sgrankin/mk at
`v0.0.0-20260226143916-183147f76d6f`; no runner source was changed.

`timeout 900 mk -p 1 test` completed with all tests passing. That establishes
a usable serialized gate, not the root cause or that serialization repairs
the runner. Investigate scheduling/dependency handling and obtain a minimal
reproducer before proposing an upstream issue or a runner change. No claim
that this is deterministic, a compiler regression, or a missing dependency.
