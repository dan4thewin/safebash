title: Unattended converter boundaries and guidance
date: 2026-11-28

# Unattended converter boundaries and guidance

DG selected the unattended twenty-owner session and its separate branch.
This cohort implements four already-determined owners; it changes the
converter, not native language policy. Work remains on
`unattended-backlog-20261128`, with main at `4e5090c9`.

## Changes

- Actual Bash `$-`/`${-}` expansions receive MANUAL guidance and remain
  unchanged, including inside functions. The early parsed-node guard also
  prevents `${-}` from becoming the invalid `${:-}`. Literal text is not
  an expansion.
- Positional and array splat guidance names all four empty cases: zero
  arguments, one empty argument, one fallback argument, and abort. Existing
  pass-through, joins, loops and attest review stay; no scaffold is selected.
- Conditional test operands receive the DD57 CHECK hint when a capture has
  an uncaught operation. Parsed roles distinguish `[ ]`, `[[ ]]`, `test`,
  arithmetic conditions, branches, loops, logical lists and inner handlers.
  Handling an outer command does not handle its nested operands or body.
- Unbraced top-level positionals consume one digit and preserve the suffix:
  `$10` becomes `${arg1}0`, `$01` becomes `${arg0}1`, `$1x` becomes
  `${arg1}x`. Full braced indices and function-local pass-through stay.

The AI used existing CST traversal, prefix annotations and positional
hoisting. Prefix placement moves outside enclosing multiline strings or
heredocs. Status classification now recognizes the actual final condition,
not every child of an if/loop, pipeline or capture. Only a direct sole
producer can pass its status through an assignment capture.

Two coupled implementation corrections came from the actual spec xtraces.
Tree-sitter assigns the `condition` field to anonymous separators too;
selecting its last child must first exclude those separators and comments.
Also, colon disposal had used status consumption as a proxy for structural
placement. It now separately refuses to expand operands across logical-list,
pipeline, capture or process-substitution boundaries. Existing
`convdiscard.6` caught the regression, including annotated code swallowed
by an inline comment. No semantic exception was introduced.

## Evidence

`convert.sh` grows from 235 to 248 checks. `conv.x7` is strengthened rather
than duplicated. Thirteen new IDs cover flags, splats, capture hints and
positionals; converter IDs are native-only even where their bodies run
converted programs through both backends.

The finalized before run has exactly eleven failures: existing `conv.x7`,
`convflags.1`, `convsplat.1`, `convcapture.1/.2/.3/.5/.6`, and
`convpos.1-.3`. Three new preservation controls pass before. The initial
before run also exposed a test assertion that expanded `${a[@]}` itself;
the final literal assertion was corrected and rerun, not counted as a
product defect. Final positive: 248/248.

Eight independent final-tree reversions have exactly these red sets:

| Reverted mechanism | Failing IDs |
|---|---|
| Option-flags recognition | `convflags.1` |
| Both splat guidance strings | `conv.x7`, `convsplat.1` |
| One-digit positional conversion | `convpos.1-.3` |
| Conditional capture hint | `convcapture.1/.2/.3/.5/.6` |
| Capture status boundary | `convcapture.1/.2/.3/.5/.6` |
| Safe multiline placement | `convflags.1`, `convcapture.5` |
| Explicit inner handling | `convcapture.4/.6` |
| Colon structural boundary | `convdiscard.6` |

All stubs are restored. Source, specs, rebuilt converter, compiler, runtime
and lockfile identities match the final positive manifest. The compiler and
runtime themselves are unchanged by this cohort.

The serialized ordinary gate passes with
`NEW=13 FLIPS=0 RETIRED=0 TOTAL=13507`: rows 7638012-7651518,
native 6880, C 6627, 13494 passing and 13 existing skips. No failures,
unexpected flips, retirements, ghosts or plan mismatches; liveness 246.
Formatting, converter Clippy and static checks pass; the gate includes the
ordinary unit/build checks. No coverage run or dependency change.

Private evidence is in `files/batch20-20261128/`: `conv-before-final2-*`,
`conv-after-final2-*`, all eight `conv-revert-*` families,
`conv-positive.sha256`, `conv-gate.log`, and archived exact xtrace blocks.
Eleven selected owners are now complete; the remaining nine continue
without a new sequencing decision.
