title: Intermittent file-test failures with pinned timestamps
status: open
priority: P2
kind: bug
depends: []
gates:
  - Capture the actual file timestamps and executable identity during a failing run.
  - Establish the cause without weakening the whole-second >= contract or adding pass-until-green retries.
  - Prove any repair with a deterministic failing control and the ordinary spec gate.

# Intermittent file-test failures with pinned timestamps

The 2026-11-26 gate at rows 7491081-7504306 failed only `native:ft.2`,
`native:ft.3` and `native:ft.5`. All three compiled and exited successfully,
but printed `no` instead of `yes`. Their C counterparts passed.
The same three native failures were already observed before the lookup-default
work, as [journal 673](../journal/673-lexical-capture-redirect-proofs.md)
records.

`spec/filetest.sh:24-46` pins equal times, newer mtime, and equal whole seconds
with differing nanoseconds. The runtime's `sash_file_test` opcode 17 compares
`st_mtime >= st_atime`; none of these cases contains an input lookup.

During the lookup-default investigation, the frozen original and new compilers
produced byte-identical executables for all three programs. All six plain runs
and six runtime-traced runs printed `yes`; stat checks before compilation,
after compilation and after execution retained the requested timestamps.
The original compiler also passed the unmodified 115-case file. A new-compiler
whole-file run, tracing only the compiled programs' `newfstatat` calls, passed
all 115 and showed the expected kernel timestamp fields.

Those controls do not explain the failed sweep. Its xtrace records the
successful `touch` commands and wrong verdicts, but not the timestamps seen by
the failing binaries. No external reader, filesystem defect, runtime defect
or compiler fix is established. Preserve the assertion rather than attributing
the failure to an unmeasured mechanism.

The failed sweep, xtrace, executable comparisons and syscall controls are
retained with the private `lookup-defaults-20261126` evidence. The original
whole-second comparison repair remains complete; this is a separate failure
investigation, not a reopening of its semantics.

The unchanged replacement gate at rows 7504307-7517532 passes all three
cases and every other assertion (13213 passing, 13 existing skips).
That permits the independently verified lookup-default delivery; it does not
establish a cause or repair for these intermittent failures.
