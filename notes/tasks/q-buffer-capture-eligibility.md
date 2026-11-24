title: Consider wider compiler-owned capture buffering
status: backlog
priority: P3
depends: []

DG chose existing buffer paths first for DD127's read boundary (2026-11-02,
journal 560). Kernel reads introduce taint; a compiler-owned buffer copy
does not. Wider buffering is an optimization question, not permission to
trust a pipe or pathname.

Today any pipeline or redirect, a direct external, or a recursive purity
visit disqualifies the enclosing capture. Nested external capture followed
by attest and echo already permits a clean outer buffer (`outflow.11`).

## Negated tests also disqualify the buffer

DG reported an unexpected SB5020 from capturing `resolve_reboot_type` in
`_tmp/t1.sb`: the function contains no external commands. In the reported
version, line 323 was `if ! [[ $entry =~ : ]]`; the capture at line 476
was then rejected when read at line 480.

At `54b49dd063cbcbd15d651b93223a08292394f7b6`, this independent minimal
program reproduces the outer SB5020 with the real native CLI:

```sh
fn f { if ! [[ $1 =~ : ]] { echo $1 } } s=$(f warm); echo $s
```

Changing only the negation to `if [[ ! $1 =~ : ]]` compiles. This pair has
no string split or nested substitution, so it isolates the eligibility
limitation from the separate read-source bug below.

`parse_negated` (`sash/src/parse/parser.rs:1590-1595`) represents the outer
`!` as a negated one-command `Pipeline`. `Purity::shape_is_pure`
(`sash/src/capture_purity.rs:68-83`) rejects every pipeline, explicitly
including that shape. The named capture therefore selects the memfd path,
whose later kernel read introduces taint under
[DD127](../decisions/DD127-taint-across-function-boundaries.md).
The immediate cause is the conservative shape classification, not an
external command hidden in the resolver.

Evaluate this narrow case separately from real pipelines and redirects.
Any eligibility change must make the actual buffer transport and the
shared classifier agree, while preserving test/status and operand effects.
Do not merely classify kernel-captured bytes as clean. This filing does
not adopt a wider buffering policy.

The resolver's `read` source also exposes an independent
[unlowered-substitution bug](bug-read-source-capture-unlowered.md).
Moving the negation alone does not make that complete function compile.
DG requested both findings be backlogged on 2026-11-24;
[journal 669](../journal/669-capture-buffer-and-read-source-backlog.md)
records the investigation.

## Broader buffering questions

Consider whether streaming wrappers and stdout-disconnected operations can
safely retain an outer buffer:

```
fn list {
  /bin/echo item | while readline x {
    attest x [[ -n $x ]]
    echo "$x"
  }
}
str result=$(list)
echo "${result:!}"
```

That result is currently kernel-backed. Any extension must preserve output,
status, nested capture frames and redirect scope, and make the actual runtime
buffer path agree with the shared eligibility predicate. Attesting before
a subsequent kernel read never attests that read. An external redirected
to `/dev/null` can reopen a saved parent memfd; pathname routing alone is
not an isolation argument.

DG also proposed compiler-owned buffer transfers from completely known
sash-generated pipe writers with no external calls (2026-11-02). This
would avoid the read rather than exempt it from taint. Preserve DD#122's
producer isolation, concurrent consumption, early close and status handling;
eagerly collecting an arbitrary producer's whole output is not equivalent.

Measure the benefit and discuss the eligibility change with DG before
implementation. Do not add dynamic taint tags, descriptor alias guards or
process isolation as an incidental part of this optimization.
