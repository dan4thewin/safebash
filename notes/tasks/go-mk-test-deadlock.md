title: Investigate the installed Go mk dependency scheduler deadlock
status: open
priority: P2
kind: tooling
source: journal 511 (2026-10-16)
depends: []
gates:
  - reproduce and identify the runner failure independently of Rust assertions
  - default mk test completes without a runner deadlock
  - cov-reset completes its recursive build without a runner deadlock

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

Journal 528 (2026-10-19) reproduced the same stack shape in `mk cov-reset`'s
recursive `mk build`, immediately after building `sash-getopt`. The reset had
already removed the instrumented release tree and profiles; its rebuild
failed before any Rust assertion or ordinary spec sweep ran. This broadens
the observed trigger beyond the `test` target. The full coverage exports,
merged profiles and database had already been saved outside `target`.
Completing the clean rebuild with `mk -p 1 build`, followed by
`mk -p 1 test`, passed all 219/27/20 units; the full ordinary 9255-check
sweep and no-profile canaries then passed too. The runner remains open;
serialization is not a root-cause claim.

Journal 532 (2026-10-24) hit it again on plain `timeout 1800 mk test`:
`fatal error: all goroutines are asleep - deadlock!` after the regex and
sash-getopt checks, before any cargo test, with goroutines blocked in
`main.mkNode` chan sends under `main.mkNodePrereqs`. `mk -p 1 test` then
passed 226/27/20. Still no root-cause claim.
