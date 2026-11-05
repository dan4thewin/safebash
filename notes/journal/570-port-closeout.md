# 570 - Reference port closeout

2026-11-05. DG authorized closeout and deletion of `taint-walker-redesign`
after the checked EXIT/ERR implementation shipped as `09a5bd49`.
I reconciled the remaining findings, completed the tracking record and
deleted the local branch. This is bookkeeping and backlog reconciliation,
not another compiler implementation or a resumption of coverage.

## What completion means

All **330 reference hunks** have final dispositions:

| Disposition | Rows |
|---|---:|
| Ported | 246 |
| Reimplemented | 62 |
| Superseded | 7 |
| Dropped | 15 |

No row remains pending or drop-proposed. The ledger itself is byte-identical
to `09a5bd49`. The superseded rows have explicit replacement/decision notes,
not unfinished work.

My initial expectation that all first fourteen fields still matched Step 0
was too strong; that assertion failed. Git history identifies exactly six
earlier catalogue-label changes on four rows:

| Rows | Earlier change | Commit |
|---|---|---|
| H063 | `family` and `families`: F1 -> F5 | `d60e74fb` |
| H213 | `family`: split -> F5; `families`: F1,F5 -> F5 | `d60e74fb` |
| H251, H252 | `bids`: B69 -> B52,B69 | `bf48becf` |

These record F5 ownership and B52's association with the static-regex fixes.
The source-hunk facts (fields 1-11: identity, file, coordinates, counts,
functions and spec IDs) still match Step 0 exactly. The corrected comparison
allows exactly the six historical label changes above, not an open-ended
exception. Closeout preserves all sixteen fields; it does not undo earlier
classification work.

The original tracking gate demanded every discovered finding be fixed or
closed. DG later narrowed the port: findings not already fixed on the
reference and not blocking the remaining work go to independent backlog.
The tracking task now states that actual rule. It has 109 checked entries
and 26 unchecked entries, all with concrete backlog destinations. Checked
does not mean 109 fixes: it also includes decisions, duplicates and controls.
Unchecked bugs and questions are not falsely marked fixed to close the port.

## Last DD58/DD74 reconciliation

I ran seventeen current-tree CLI cases per mode, serially through the
existing spec harness, with repository binaries explicit. These were
observations, not new registered specs. The frozen base/reference compilers
were neither rebuilt nor rerun.

| Finding | Current native/C result | Disposition |
|---|---|---|
| B77 | A function-only `$f` is empty; bare arithmetic `f` is zero without a call. A real int-function call still returns 7. | Existing `bug-function-name-read-as-variable`. |
| B78 | `int f=3` and `str f=3` compile; plain `f=3` gets SB4019. | Same existing task. |
| B79 | An int stage returning 256 succeeds; returning 3 aborts with stage status 3. | New `q-int-function-pipeline-status`; no policy chosen here. |
| B80 | Arithmetic `(len f)` still ICEs native and produces garbage in C; ordinary `$(len f)` gets SB5039. | Added to the existing function-name task, now P1 for the ICE/miscompile. |
| B121 | Redirected void calls in `if`, `\|\|`, negation and a condition list all compile; plain `if f` gets SB4037. | Existing `bug-peel-family-classifiers`; still open. |

B79's reader use has `:!` solely to keep the newer taint rule from masking
the process-status question. Its integer-return syntax needed no repair.
I did not treat a different rejection as evidence that an old mechanism
was fixed.

B80's native compiler exits 1 with `ICE: undefined arrlen: f`. The
harness xtrace showed that its stderr filter dropped this nonstandard
diagnostic, so I retained a separate raw compiler invocation and its exit
status. C printed `4294940122` in this run; that is an observation of
garbage, not an expected stable value.

B121's missing `Redirected` arm remains visible in
`validate::mark_predicate_operand`. F6's SB4076 rejection of suppressed
void calls did not fix it. The separate already-completed cases still
reject: a captured int-function predicate gets SB4030 (B7, journal 565),
and a bare status-function call gets SB4016 (B15, journal 547).
There is no unspecified DD74 port family left.

## Reference preservation and deletion

The deleted local branch pointed to exactly
`e98fca888c3e0b2902c3d9a88f9197c5dad7711b`, the one WIP commit above
`13093856525bbe8b83e21e2a3bf28c882981372b`. It had no remote branch.
Both reference worktrees were already detached; neither worktree nor its
frozen files was removed.

Before deletion I created and verified the session bundle
`files/port-reference-taint-walker-redesign.bundle`. It contains the exact
reference head and requires the base commit above, which is an ancestor
of the current branch. Its SHA256 is:

`7fdb8d31f2c1cdbb287651a1160a498de4874fbead7a980a653262f2ae2820af`

After deletion, `git bundle verify` still passes and the local branch ref
is absent. No history rewrite, force-push, garbage collection, tag creation
or reference-worktree removal was needed.

## Evidence and unchanged acceptance boundary

`files/port-closeout-probes.tar.gz` contains eleven regular files:
the mode observations, both complete xtraces, raw B80 compiler evidence,
bundle verification, worktree state, binary hashes and structured metadata.
The seventeen case IDs in each xtrace match its observation log in order.
Every archived member was compared byte-for-byte with its source.
Uncompressed size is 54,762 bytes; archive size is 5,465 bytes. SHA256:

`6dd8e5f5f5aedfdeed54f78222a0e017e3dc8eb0814eec65e5b8f76c10c69c76`

No compiler, runtime or spec source changes. The accepted source/binary
identities from journal 569 are unchanged. Its full gate remains the
acceptance boundary: 11,157 checks, 11,144 passing and 13 existing skips,
rows 6,811,808-6,822,964. Closeout writes no spec history and does not rerun
that full source gate for documentation changes. Static accounting and
normal commit hooks remain required, without bypass.

Coverage is still paused. The old 46-line ranking predates the port and
must not be reused; any separately authorized resumption starts with a
fresh scope-correct union and provenance check. B55, sed extent and the
other independent backlog tasks remain unresolved on their own merits.
