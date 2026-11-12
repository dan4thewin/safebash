# 624 - Companion command manuals and the legacy grep fixture

**Date:** 2026-11-12

DG requested descriptions of `sash-getopt`, `sash-expand` and
`sash-convert`, and suggested that `contrib/sash-grep.sash` might be
discarded. The AI added three companion command manuals and linked them
from `sash(1)`, keeping the compiler reference focused.

After being told that the grep sample still served a legacy test target,
DG explicitly chose to retire the sample, its harness and that target,
while retaining the current regex suites.

## The shipped interfaces

The new pages document invocation, output, errors, examples and material
limitations from the current implementations, rather than copying the
older design examples:

- `sash-getopt(1)` describes unquoted, NUL-terminated fields, including
  optional empty arguments, ordering, compatibility environment variables
  and the nonzero `--test` result. `--unknown` retains option material and
  suppresses parser diagnostics but still returns failure. The direct C
  build command makes clear that this helper is not a Cargo workspace
  artifact.
- `sash-expand(1)` describes one filename, sequential KEY=value assignments,
  inherited variables, the restricted nonrecursive expansions and raw
  newline output. It does not source or export anything. Both outer quote
  forms still allow expansion. There is no flag interface or stdin mode.
- `sash-convert(1)` describes file/stdin input, stdout/`-o` output,
  `--annotate`, help, the four annotation categories and optional
  ShellCheck analysis. Successful conversion is not compiler acceptance,
  equivalence, or approval of generated validation predicates. The
  ordinary example is converted, reviewed and compiled from a `.sb` file.

The format distinction matters: getopt's output belongs with a NUL-aware
array reader; expand's KEY=value text is not that protocol and does not
become a map merely by passing it to `read`. DD99 supersedes DD54's old
`mapfile -m` integration. No retired reader flag, obsolete converter flag
or queued annotation policy is advertised as implemented.

The new pages use safebash for the language and `.sb` for generated source.
They introduce no language policy, installation mechanism or public
language specification. README and GUIDE retain their separate roles.

## Retiring the grep fixture

At baseline `c15d14b3`, `mk test` depended on `sash-grep-test`, which ran
`contrib/test-regex.sh` and compiled the 14-line `contrib/sash-grep.sash`.
That harness was ignored by the broad `test-*` rule and was not tracked:
a fresh tracked-source checkout lacked a live test prerequisite. This
was established by Git membership and the mk rule, not by claiming a new
fresh-clone execution.

The harness's 26 checks are deliberately retired. Its matching assertions
normalized output through `xargs`; its malformed-pattern checks accepted
empty stdout without asserting runtime status or the diagnostic. This
filing does not claim a one-for-one equivalence proof with the retained
tests.

Removed the sample, the ignored harness, the mk target and its prerequisite
edge. The ignored harness was copied into private session evidence and
byte-compared before deletion; the tracked sample remains retrievable at
the baseline commit. No unrelated ignored files or ignore rules changed.

The independent C-engine tests and authoritative native/C regex specs
remain byte-identical. With both retired files absent, the mk dry run
contains the retained C/getopt/Cargo routes and no legacy grep invocation.
The ordinary gate then completed successfully through that graph.

## Defects discovered while documenting

Minimal actual-CLI probes found two independent `sash-expand` input
problems:

- Literal UTF-8 bytes are converted individually to characters, corrupting
  text; the four stripping helpers also slice at arbitrary byte offsets
  and can panic on inherited UTF-8 values.
- A one-character single or double quote passes the outer-pair test, then
  produces a reversed slice range and panics.

Both have concise P2 backlog owners:
[UTF-8 processing](../tasks/bug-expand-utf8-value-processing.md) and
[lone quotes](../tasks/bug-expand-lone-quote.md).
The manual warns about the defects without declaring an ASCII-only
language or silently selecting a new quote grammar. No helper repair is
included in this documentation/retirement task.

The existing
[stdout-error owner](../tasks/bug-stdout-write-failure-panics.md)
and [converter annotation owner](../tasks/bug-convert-annotate-midline.md)
remain separate. DD133's accepted stdout policy is not presented as
implemented, nor is annotated converter output promised to compile
unchanged.

## Verification

All four command pages render with `groff -Tutf8 -t -man -ww` without
warnings, and their rendered text was reviewed. Real CLI controls compare
getopt output as bytes, not a scalar command substitution. They cover the
examples, clusters, empty/attached optional arguments, long-option
canonicalization, ordering, compatibility environment, quiet/unknown
behavior and statuses. The direct C helper build also produces the
documented byte sequence.

The expand example and its operator/quote/order distinctions were checked
through the actual command. Converter file/stdin/output-file forms agree;
the ordinary generated file compiles with normal taint checking and prints
`3`. The annotation example produces its requested comments, without a
claim that every annotated program is correct. Generated probe executables
and objects were removed.

There are 33 successful final interface/build/run controls, in addition to
the earlier expand example/defect/help probes. Two private check assumptions
were corrected during verification: `--unknown` intentionally suppresses
parser diagnostics, and rendered man-page headers are uppercase. Neither
was a product failure.

The mk prerequisite change warranted the ordinary implementation gate:

```text
NEW=0 FLIPS=0 RETIRED=0 TOTAL=11345 timeout 900 mk -p 1 gate
```

It passed: 238/27/20 Rust tests, 152 C-engine checks, five getopt checks;
11345 spec rows (11332 passing, 13 existing skips), native 5784 and C 5561.
The sweep occupies rowids **6890787-6902131**. There are no flips, new IDs,
registry retirements, ghosts or plan mismatches; liveness is 251.
The retired 26-check harness was never part of `spec.db`, so it does not
alter the predicted spec total or retirement count.

The gate regenerated status's managed block from this actual sweep.
Compiler/runtime/helper binaries and both local lockfiles still match the
saved baseline identities. No compiler, runtime, converter, expansion,
getopt, spec or dependency source changed, and no coverage run was started.
Normal commit/push hooks remain in force.

Evidence is retained in the session's
`files/companion-manpages-20261111/`: initial and final CLI results,
source fixtures, the retired harness, rendered pages, the mk dry run,
gate output and baseline identities. The directory's date reflects the
start of the investigation; completion crossed into November 12.
