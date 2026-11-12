title: "`mk gate` does not automate the negative control (new specs red on the pre-change binary) or the running of reversion stubs"
status: open
priority: P3
kind: tooling
depends: []
gates:
  - "one command runs the last gate sweep's first-ever ids against a binary built from a named base revision, in both modes, and lists each id that passes there"
  - "that command is shown to fail: a spec that passes on both binaries appears in its list"
  - "AGENTS.md's `mk gate` paragraph names the command"

`mk gate` (journal 540) automates every spec.db check in AGENTS.md's Spec
tests section except two, and both are still done by hand each family:

- **Negative control.** Each new spec should fail on the binary from before
  the change, unless it is a deliberate control. F1+F4 ran it by hand (179 of
  201 red, journal 537:229) and so did F2 (51 of 56, journal 539:117).
- **Reversion stubs.** Writing a stub is judgment work. Running one is not:
  apply it, rebuild, run both modes, list the ids that went red, and check
  that the stub's target text occurs exactly once. F1+F4 had a throwaway
  harness for that (journal 537:235-238).

The obstacle for the negative control is that a spec file finds its
binaries and runtime relative to its own directory: common.sh:19 puts
`../target/release` on `PATH` and common.sh:25 sets `SASH_RT_LIB` to
`../sash-rt/target/release/libsash_rt.a`, overriding the environment. So the
current spec files must be copied into a built worktree of the base revision
and run from there. The hand-run version did exactly that.

The output is a list to review, not a pass/fail count, because some new
specs are controls that pass on both binaries by design (F2's stgarg.34 and
stgarg.36). A prediction like the gate's (`NEGC=<n>` ids expected to pass on
the base binary) would let it gate.
