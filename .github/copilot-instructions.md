# Copilot Instructions for safebash

## About this project

safebash is a compiler for a safe, statically analyzable subset of
Bash, implemented in Rust. It produces signed native artifacts whose
integrity can be enforced at execution time. The CLI binary is `sash`.

Read the README.md for the full design document, especially the
Safety Model and chain of trust sections.

## Approach

DG directs the effort. The AI implements under his direction.

- **Small, conservative goals** — each experiment yields a working
  artifact with a test, collected in `exp/` directories
  (e.g., `exp/001-treesitter-parse/{mkfile,src/,...}`).
- **Compile and test at every step** — don't move on until the
  current experiment passes.
- **Surface contradictions immediately.** If stored facts, code
  comments, documentation, or architecture notes conflict with actual
  code behavior, flag the contradiction to the user at high priority
  before proceeding.

## Documentation

Two documents chronicle the work:

- **`exp/JOURNAL.md`** — each experiment with goals, actions, and
  reasoning. Written as a narrative, not just notes.
- **`exp/GUIDE.md`** — explains the system architecture for someone
  coming to the project fresh. Updated when new architecture is
  introduced.

Update both after every significant milestone.

## Development milestones

Each layer is independently testable before the next is built:

1. **Parse and Validate.** tree-sitter-bash front-end with safe
   subset validation.
2. **IR and SSA.** Structured intermediate representation with SSA
   transformation.
3. **Janet Oracle.** Emit Janet from SSA IR. Property-based tests
   comparing Janet execution to compiled output.
4. **Cranelift Backend.** Lower SSA IR to Cranelift. JIT for REPL,
   AOT for deployment.
5. **Signing and Verification.** Sign AOT artifacts. Integrate with
   IPE / fs-verity.

## Completion checklist

Every task ends with:
1. All tests pass
2. Update `exp/JOURNAL.md` (goals, actions, reasoning)
3. Update `exp/GUIDE.md` if new architecture introduced
4. Add deferred work to `exp/TODO.md`
5. Update this file if SOP changed
6. `git commit` with descriptive message
7. `git push` (use `source ~/.ssh/vars` for SSH agent on ROVE)

## Style

- Rust: follow `rustfmt` defaults, `clippy` clean
- Build: use `mk` (Plan 9 mk), not `make`. Each experiment has an
  `mkfile` with a `test` target. `cargo` is invoked by `mk` rules.
- Commit messages: short, descriptive
- Comments: only when code needs clarification, not boilerplate

## Reference files

- **README.md** — design document (safety model, architecture, goals)
- **exp/JOURNAL.md** — experiment chronicle
- **exp/GUIDE.md** — system architecture guide
- **exp/TODO.md** — backlog
- **exp/TIMELINE.md** — chronological milestones

## Bug policy

Zero tolerance. Finding a bug is a stop-everything event. Never work
around it. If you notice yourself routing around a behavior, STOP.
Investigate whether it's a bug.
