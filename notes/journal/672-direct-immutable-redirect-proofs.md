title: Direct immutable redirect proofs
date: 2026-11-25

# Direct immutable redirect proofs

DG requested a small ergonomic extension to DD119, not general nonempty
value analysis. He explicitly rejected transitive properties to reduce
review burden, restricted optional literal concatenation to the redirect
site, and selected obvious unary tests in simple usage. The AI implemented
that contract on `err-attempt-trial`; DD136 remains provisional and main
does not move.

## The visible rule

```sash
_dir=$(- arg 1)
attest _dir cpath?
echo message > $_dir/log
```

Exactly one immutable scalar read may omit its emptiness disposition after
a successful direct test in the same body. The whitelist is:

```text
-n cpath? p? g? -e -f -d -r -w -x -s -L -h
```

Simple ordinary tests, scalar attest shorthand and equivalent full unary
attests qualify. Success must hold on every reaching path for that binding
instance. Ordinary consts qualify as well as scratch; a known nonempty
initializer alone does not.

Quotes, braces and empty literal wrappers preserve the tested read.
Nonempty literal neighbors are allowed only at the redirect site.
Copies, precomputed concatenation, function arguments/results, helper
boundaries, collection attests and cell reads do not transfer the proof.
Even two reads of the same proved binding do not qualify together.
A derived binding can acquire its own proof through its own direct test.
Compound predicates, comparisons, false-`-z` deductions and unlisted unary
tests remain outside this allowance.

The certificate says only **nonempty**. It promises neither continued file
existence nor authorization, and appending literal text need not preserve
`cpath?` spelling. Taint and caught-scratch ordinary-read permissions stay
independent: an ordinary test does not attest input, and `:!` supplies no
nonempty proof by itself. The taint switches cannot manufacture one.
Existing literal, disposition and quoted-integer routes remain separate;
this feature does not add collection-cell inference or integer-cell parity.

DD119 is reconciled in place, including its opening rule, concatenation
strictness and diagnostics. GUIDE describes the shared implementation;
the specification outline includes the same bounded exception.

## Reusing flow without inheriting regex propagation

The existing pre-fold regex/caught-read worklist now carries a separate
nonempty fact set keyed by source body and logical binding identity.
Meets intersect it; real writes and lifetime starts invalidate it.
Assignment, equality and collection propagation never transfer these
facts. This reuses convergence and control-flow handling without importing
regex's transitive certificate semantics.

The small `redirect_proof` module recognizes original target and predicate
shapes. Source origins retain whole-predicate eligibility before Boolean
lowering, so a compound predicate cannot accidentally become several
qualifying simple tests. Inert body origins distinguish named and anonymous
body entries. Targets retain their source binding and span before spilling.
The type checker defers only eligible single-string-binding shapes;
the shared proof pass reports missing certificates as SB4030.

Function-template entries are additional roots for redirect obligations.
Thus an unused function with its own proof is valid, and an unused
unproved function still receives a located diagnostic. Existing regex and
caught-read diagnostics remain restricted to executable blocks.

Source review also checked handler entry: `handler_input` starts from
empty facts and carries only blocked global caught-read permissions.
It does not import the new certificates into EXIT or ERR. This is source
inspection, not a newly executed handler experiment or another repair.

## Two coupled defects in the draft

Caught-attest success cases initially reached the SSA taint backstop with
raw target names. Its existing name lookup selected a later tainted merge
version rather than the clean version reaching that redirect. Newly admitted
targets now use the existing scalar spill path: evaluate the reaching SSA
value, reserve a correctly typed temporary, assign it, then redirect through
that unique name. This preserves the independent backstop rather than
bypassing it. `ssa_taint.rs` is unchanged; this is not a general repair of
every highest-version name lookup.

The first draft also lost deferred obligations after terminal commands.
Lowering retained an unproved target after `exit`, but the flow walk stopped
there and never checked it. Every recorded target now starts pending;
visited targets are checked and discharged, and remaining obligations
receive SB4030. `redirproof.35` covers both `exit` and `fail`.

Other early failures were fixture mistakes: testing `-n` on a literal-suffix
concatenation hit the existing always-true check, double negation was invalid
grammar, and one proposed taint switch does not exist. The final cases use
legal source and supported switches. Their original traces remain distinct
from the accepted evidence.

## Focused controls and independent reversions

`filetest.sh` grows from 74 to 109 checks per mode, with fresh historical
IDs `redirproof.1` through `.35`. The complete file passes in native and C;
the adjacent errors and regex files also pass all 723 and 321 checks per
mode respectively.

Against the final 109-check inventory, the frozen original compiler fails
exactly nineteen new assertions in each backend:
`redirproof.1`-`.16`, `.29`, `.30` and `.32`. Every other assertion passes.
These include admission and diagnostic-boundary assertions; the original
run is not evidence that all new refusal cases distinguish the change.

Eight independent source reversions ran the complete file in both modes.
Every predicted failure set matched exactly, with no other failures:

| Reversion | Failing `redirproof` suffixes, each mode |
|---|---|
| Remove immutable membership guard | 17 |
| Mark every original predicate eligible | 21, 22, 23 |
| Collapse source body identities | 26 |
| Omit function-template roots | 11, 12 |
| Omit unvisited-obligation checking | 35 |
| Permit multiple dynamic reads | 20 |
| Transfer certificates on scalar copies | 19 |
| Restore the old spill-only condition | 6, 8 |

A ninth reversion removes only nonempty invalidation in `Facts::kill`.
The direct-API lifetime unit fails: it receives no diagnostics instead of
the expected located SB4030 after `LifetimeStart`. This unit is necessary
because source-loop merges independently lose facts; the source lifetime
spec alone cannot isolate that invalidation line.

All nine mutations are restored. The positive source, spec, release
compiler, runtime archive and both local lockfiles matched their saved
hashes after restoration and after the first full gate.

## Full gates and final complexity cleanup

```sh
NEW=70 FLIPS=0 RETIRED=0 TOTAL=13190 timeout 900 mk -p 1 gate
```

The first full gate passes at rows 7451499-7464688: 6711 native and 6479 C
checks, 13177 passing and 13 existing skips. The sweep took 337 seconds.
There are no failures, flips, retirements, ghosts or plan mismatches;
liveness is 246. An independent comparison with the accepted cpath block,
7438379-7451498, retains all 13120 statuses unchanged and adds exactly the
70 intended `native:redirproof.*` / `c:redirproof.*` identities.

The complete touched-function measurement then found one inherited nested
helper over the live complexity limit: `plain_read_proof`, cc 4 / cog 5.
Flattening its index/type/quote refusals into ordered early returns reduces
it to cc 4 / cog 3 without changing accepted forms or diagnostic text.
The positive mutation-control snapshot remains separate from this final
cleanup. The final gate uses zero new IDs because the first sweep already
recorded all seventy.

```sh
NEW=0 FLIPS=0 RETIRED=0 TOTAL=13190 timeout 900 mk -p 1 gate
```

That final gate passes at rows 7464689-7477878, with the same per-mode and
status totals, no flips or retirements, and liveness 246. The sweep took
336 seconds. All 13190 first-gate statuses are unchanged; the independent
comparison still retains all 13120 original statuses plus exactly seventy
new IDs. The ordinary unit runs pass 241 compiler, 27 converter and
20 expansion tests; formatting and compiler Clippy also pass.

`final.sha256` and `sash-final` preserve the gated implementation after the
complexity cleanup. Only `type_check.rs` and the release compiler differ
from the restored mutation-control snapshot. Runtime, helper binaries,
both local locks, `notes/dat` and `_tmp/t1.sb` retain their original bytes.
The detached reference worktrees and local/remote main remain unchanged.

Evidence is retained in session `files/redirect-nonempty-20261125/`:
frozen before/positive compilers and runtime, focused and original-control
logs, the nine independent reversion logs, restoration manifests and gate
history comparisons. `control-xtraces.tar.gz` contains six byte-verified
Wednesday trace snapshots covering the controls and reversions; its
manifest does not claim later final-gate additions. No runtime
implementation, dependency refresh,
coverage measurement or unrelated backlog implementation was performed.
