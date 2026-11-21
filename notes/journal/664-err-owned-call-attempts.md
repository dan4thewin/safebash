title: ERR-owned call attempts
date: 2026-11-21

# ERR-owned call attempts

DG requested a separate branch with a provisional decision, implementation,
tour, DD alignment and specification-outline changes. He rejected recovery
at the nearest helper: an abort must abandon the intervening call chain and
resume at its ERR-owned boundary. He limited that boundary to one user call
and selected the narrow recovery inventory. The AI implemented that model.

The branch is `err-attempt-trial`, starting at main `02217248`.
[DD136](../decisions/DD136-err-call-attempts.md) remains **provisional**.
This delivery does not authorize adoption, a main landing or a PR.
The [integrated owner](../tasks/err-call-attempt-trial.md) stays open for
DG's evaluation.

## Three distinct operations

```sash
:                       # successful noop
_=$value                # checked value disposal
: cleanup               # ERR-owned void-function attempt
```

A caught status-function call in ERR also establishes an attempt, including
an otherwise-legal named predicate capture. Normal returns retain their
ordinary full status. Abnormal completion is false even for `fail 0`;
numeric status and verdict are separate until authored negation performs
its normal 0/1 update. The caller's capture retains already-emitted output,
not an invented return value or an unfinished inner capture.

The boundary includes argument evaluation and attached redirects. It is
inactive before either continuation starts. A helper's own conditions,
a bare void call and source `-` do not establish another attempt. Integer
calls may execute inside an attempt but cannot establish one. A resultless
colon attempt gives no new status-read permission. ERR fall-through retains
its original entry status, rather than the last recovered status.

Only explicit `fail` and existing unchecked operational results recover.
Locally handled results do not raise that abort. `:?`, argv-NUL and invalid
index guards, currently unconditional slurp/read failures, fatal faults,
explicit `exit` and failed restoration remain terminal. This contextual
boundary does not reclassify ordinary file errors as globally fatal or
complete the separate file-failure work.

Completed effects remain: no rollback of files, output, consumed input or
completed global writes. Automatic abort diagnostics are suppressed on
recovery; authored output and external stderr are not.

## Implementation

The parser records `Command.best_effort`; lexical `AttemptSites` selects
ERR-owned calls with the appropriate result kind. SB5108 diagnoses invalid
targets, kinds and placements rather than interpreting arbitrary words as
ignored noop operands. IR schema 4 carries `:best-effort` on commands in
both emitted and embedded data. Older consumers must refuse the new schema,
not erase recovery semantics.

User functions already inline. The transform therefore uses visible
`AttemptStart`/`AttemptEnd`, checked exceptional edges and `RecoverGlobal`,
not a nonlocal machine-stack jump through Rust. Failure edges take the
pre-operation state; normal edges take the completed result. Splitting uses
the executable instruction view and preserves origin, signal and phi
metadata. Loop jumps retain their actual predecessor version snapshots.

Source summaries distinguish normal and abandoned effects, with ownership
and freshness origins retained. No abandoned outcome is different from an
empty abandoned state. The SSA backstop and regex proof analysis follow the
exceptional edges separately. An unreached or failed attest cannot clean
the recovered state; a shared body cannot export its own unresolved taint.
Readonly ERR imports remain enforced.

The runtime attempt state is a cleanup ledger, not a general abort router.
It bookmarks enclosing captures/readers and records outstanding descriptor
backups. Recovery first deactivates ownership, restores scoped descriptors,
discards unfinished inner captures, closes every abandoned reader, and only
then reaps their producers. Enclosing ERR resources survive. Committed
descriptor changes are unregistered; children clear inherited attempt
ownership. Failed restoration terminates rather than resuming uncertain
execution.

Every write-only `_` site gets an independent lexical identity. Separate
scalar and list sinks can have different types; RHS effects and checks
remain, and `_` never becomes readable. Compound updates that would read
the discarded previous value still reject. The converter migrates Bash
colon operands to disposal rather than accidentally creating function
attempts; bare and redirect-only colon remain noops.

SB5101's private possible-abort inventory and mandatory static ERR NUL
prover are removed. Authored `0?`/filename checks and runtime byte guards
remain. This retires a compile-time restriction, not the runtime guard or
the independent taint/read-only rules.

## Coupled repairs

### Ordinary named predicate captures and branch adjacency

DD88's accepted named-predicate extension now uses the actual status-return
classification and the shared capture path. The ordinary consumer cohort
covers `if`/`elif`, `while`/`until`, negation, `&&` and `||`; full status 256;
empty and nonempty output; early/forwarded returns; one invocation with
persisted scalar/collection effects; redirect restoration; and ordinary
body/operand failure isolation.

The new `predcap.7` exposed a pre-existing DD121 violation: a condition's
acquisition metadata reached its guarded body but not its final `else`.
An already-supported external capture reproduced the same refusal on both
the original and trial compilers. The repair passes the last condition's
metadata to `else`, filtering it against the actual failed state.
`errtaint.19` permits an adjacent attest after acquisition-before-abort;
`.20` refuses one when the abort happened before acquisition.

Two draft fixtures were wrong. The first `predcap.3` used a literal
comparison correctly refused by SB5086. The first `errtaint.19` placed
`return 0` after an unconditional `fail`, leaving no reachable status
return: it was a void function, not a status attempt. Instrumentation
confirmed that classification and fresh failed-state origins. A conditional
failure repairs the fixture. No normal-outcome redesign, marker redesign
or removal of the ordinary condition-entry union followed that disproved
hypothesis. The invalid runs are retained but are not acceptance evidence.

### Fatal fork cleanup

The first trial made `sash_fork` abort immediately. That prevented its
callers from closing construction-owned pipes and reaping started stages.
The repaired low-level API returns `-1` with errno; each caller performs
its cleanup before unconditional fatal dispatch. Background failure guards
precede PID formatting, allocation and publication. Ordinary command
failure now names the fork and exits 1 instead of returning an exec-like 127.
The AI reused the existing fork-fault status convention; DG selected
fatality, not a new numeric-status policy.

`errtry.46/.47` fail on the unrepaired trial and pass after repair in both
backends. A `/dev/zero` producer cannot finish while the parent retains its
pipe read end and the second child does not exist, making second-clone
EAGAIN injection stable against the known restarted-clone counting hazard.
The traces require both pipe ends closed before waiting, successful reaping
before the diagnostic/handler output, and no ERR-attempt continuation.
Independent native and C routing mutations each turn only that backend's
`errtry.47` red. There is no process-tree broadcast, forced asynchronous
wait or producer killing.

## Evidence

The matched original compiler, runtime, helpers and local locks are frozen.
Session-only linker wrappers and actual archive-open traces prove that the
before controls use that runtime, not the evolving working archive.
All spec runs are serial, through the real CLI and explicit release PATH.

The final source inventory adds **315 mode IDs and retires 149** relative
to the original lookup baseline, with **12853 total checks**: native 6541,
C 6312. Contracts that changed use fresh IDs. Retirements include the old colon
operand checks and SB5101 expectations; `errnul.40`, not `.31`, is in that
set. The separately owned argument-order bug was not ratified by weakening
`errnul.28`.

Deduplicating the original-control records against those 315 new IDs gives
269 failing, 44 passing and two skipped. No retained-current ID fails.
The nonred group is not 46 failed-before fixes: it includes preservation
cases and tests needing independent mechanism evidence. In particular,
the original compiler can satisfy `errtry.43` without invoking a colon
attempt, `predcap.6` already rejects tainted captured output, and
`errtaint.16` is not abandoned-regex-prefix proof.

The corrected final `capture_cond` 117 and `err_attempt` 67 cases pass in
both modes: 368 checks. Their matched original runs have exactly 10 and
64 reds per mode. Those 148 observations overlap earlier controls and must
not be added to the unique new-ID count.

There are **21 independent mutation campaigns**, 46 file/mode runs and
3064 checks. Twenty campaigns produce 104 red mode-ID observations, not
104 distinct specs. They pin executable-instruction splitting, separate
native/C abnormal verdicts, source abandoned summaries, SSA recovered
taint, regex failed edges, freshness and shared ownership, phi retargeting,
continue/header inputs and jump snapshots, discard identities, descriptor
restoration, reader/capture bookmarks, close-before-wait, both else
adjacency/freshness guards, and separate native/C fatal-fork routing.

The SSA pre/post-snapshot mutation produced no reds and is **not proof**.
Initial native-verdict, summary, phi, continue, jump-snapshot and descriptor
forecasts missed or overpredicted assertions; the actual sets remain in the
records. The final else mutations match exactly: removing else metadata
breaks `predcap.7` and `errtaint.19` in both modes; omitting only failed-state
freshness filtering breaks `errtaint.20` in both. All mutations are restored,
including exact positive source/compiler/runtime/lock identities.

Both tour editions contain 73 programs, all checked after final restoration.
Ten earlier exact native/C executions additionally cover the new examples
and their normal/abnormal variants. GUIDE, the 21-topic outline and sixteen
companion DDs are aligned without accepting DD136.

Root/runtime formatting, root strict Clippy, totality, static accounting and
all 258 touched/new compiler functions pass their existing checks. Runtime
strict release Clippy is **not green**: the matched baseline has 115
diagnostics and the final trial 114, with no additions. The removed
raw-pointer diagnostic belonged to the deleted command-name dereference on
`sash_run`'s failed-fork path. An initial equality check expected 115 and
stopped on that explained removal; no unrelated lint cleanup was done.

## Ordinary gate and fixture migration

The first ordinary gate passed `mk test` but failed the spec sweep.
Its predicted 301 first-ever IDs, 135 retirements and 12853 rows all
matched, at rowids **7321769-7334621**. Fourteen retained checks failed:
seven fixtures in `pipe_functions.sh`, in both backends. Their actual
xtraces showed five old recoverable-fork expectations and two retired
colon-operand forms, not a reason to reverse the implemented contracts.

The seven one-for-one migrations keep that file's plan at 179:

| Retired ID | Replacement | Preserved claim under the current contract |
|---|---|---|
| `p518.18` | `errmig.52` | Started child completes before fatal fork dispatch |
| `p518.20` | `errmig.53` | Interrupted cleanup wait retries and reaps before fatal dispatch |
| `p518.22` | `errmig.54` | Failed construction leaves the old array visible in ERR, without resuming |
| `p518.24` | `errmig.55` | Partial reader construction reaps the started child, then terminates |
| `pipeown.28` | `errmig.56` | Nested partial construction fails fatally on the intended third clone |
| `stgarg.28` | `errmig.57` | A function stage's `_=$(g)` discards text but retains stderr |
| `stgarg.29` | `errmig.58` | Failed disposal reports the inner command and the named function stage |

The new suffixes start above the database's historical high-water mark 51.
Both final 179-case mode files pass. The matched original compiler/runtime
fails exactly `errmig.52` through `.56` per mode; `.57/.58` are preservation
controls. The final original accounting above includes these observations.
No production source changed to repair these seven expectations.

The nested case also repairs its previously recorded strace-count hazard.
An explicit `- wait` reaps the small outer producer before constructing
the inner pipeline. Its assertion requires that actual reap before clone 2,
exactly three clone entries, EAGAIN injected at entry 3 and no restarted
clone. The interrupted-wait case requires both the injected EINTR and a
successful later reap. Actual native/C syscall logs are retained for both.

A separate natural-restart control delays the outer producer's exit by
0.1 seconds and clone entry 2 by 0.5 seconds. Without the authored wait,
entry 2 restarts in both backends; with it, the producer is already reaped
and neither entry restarts. Normal output is unchanged in all four runs.
The initial fast-printf control did not force an overlapping exit and is
retained as an unsuccessful trigger, not negative proof. The generic
detector and whole-suite census remain in the
[strace owner](../tasks/bug-strace-count-injection-drifts-on-restart.md).

The corrected ordinary gate passes:

```sh
timeout 1800 env NEW=14 RETIRED=14 FLIPS=0 TOTAL=12853 mk -p 1 gate
```

The sweep occupies rowids **7334622-7347474**, taking 317 seconds. Its
12853 distinct checks comprise 12840 passing and 13 existing C-mode skips:
native 6541, C 6312. All 14 first-ever IDs and 14 retirements match the
prediction, with zero flips, failures, plan mismatches or registry ghosts.
The failed old IDs retire rather than becoming flips. Liveness reports
246 IDs differing from their first-ever status. `mk test` passes 241
compiler, 27 converter and 20 expander unit tests; the managed status block
is regenerated from this successful sweep. Positive source, compiler,
runtime and lock identities remain unchanged across it.

Delivery is confined to `err-attempt-trial`. The named-predicate capture
and fork-fatality owners close for their verified scopes on this branch.
The integrated trial owner stays open for DG's evaluation; DD136 remains
provisional and main stays at `02217248`. General file-failure handling,
direct-background-exec, the strace detector/census and unrelated backlog
work are not included.

Evidence is retained under session
`files/err-attempt-20261121/`: `final-inventory.json`,
`final-original-accounting.json`, `final-reversion-summary.json`,
`runtime-lint-final-verified.json`, `final-complexity.json`, final tour logs,
original/focused TSVs and captured xtraces. `retry-inventory.json`,
`retry-final-original-accounting.json` and `retry-positive-identities.json`
include the last seven fixture migrations. `final-fault-trace-results.json`
and `natural-restart-control-results.json` preserve their syscall controls.
`retry-gate-start.json` and `retry-gate.log` retain the corrected ordinary
gate's predictions and result.
The first `final-*` inventory/identity records precede those migrations;
the older unqualified inventory/positive artifacts predate the fork repair.
Initial predicate/else records contain the invalid fixtures described above.
