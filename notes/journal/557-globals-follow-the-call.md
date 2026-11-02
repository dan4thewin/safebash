# 557 - Globals follow the call, not the final state

2026-11-02. W1b-1, the GLOBALS half of the split DG requested after the
combined unit ran too long. DG chose DD127's single-use rule and approved
the split; I implemented the globals portion on the existing statement
transfers. Collection slots remain W1b-2, which waits for another go.

## What changed

The old two-phase global check used the final top-level taint set inside
every function. A source after a call could falsely taint that earlier call;
a cleanup after a call could hide a genuinely unsafe one. Global summaries
now apply at the call itself (`gflow.1`/`.2`, B100).

A function with one definition and one syntactic call site can pass its own
global taint outward, for the caller to attest immediately after the call.
An anonymous capture is single-use too. A shared function still has to
attest internally: SB5067 reports the last source per escaping global and
says why the function is shared, including the call-site positions.
Re-tainting an already-tainted global is a source, not an exemption.

Incoming global reads and attests are conditional on the caller's state.
Their original diagnostic is relayed at the call, naming the location in
the body; a transitive call keeps the chain. Inherited attests are SB5088.
Ordinary calls also propagate clean overwrites, so a callee can clear
inherited global taint without attesting it.

## Mechanism and implementation choices

The branch's context-sensitive walker is not ported. The existing
per-statement transfers remain, including W1a's capture-before-effect and
per-initializer order.

- `Census` uses `Program::function_defs` and the shared scoped walk.
  Command and arithmetic call sites count, including bare int-function
  names, captures and END. Self calls do not: the only legal self calls are
  DD27's tail loops. The new arithmetic-only name walkers in `resolve.rs`
  reuse the F5 traversal without counting capture calls twice.
- `FnSummary` contains global generators, per-input global flows and
  per-input conditional diagnostics. For a definition, an empty-entry walk
  finds generators; a singleton-entry walk per global finds its effects.
  Diagnostic subtraction is an order-preserving multiset difference, not
  code-based deduplication.
- A flow excludes independently generated names except its own input name.
  Retaining that input matters when the function re-taints it: inherited
  taint must not disappear merely because the same name is also a generator.
  Completed summaries have a flow entry for every global; calls index that
  invariant directly rather than supplying an undocumented fallback.
- `SummaryBuilder` builds callees first. Multiple definitions are
  conservatively shared and their summaries unioned by name. There is no
  call-context cache or inter-function fixpoint. Each definition's own
  errors are reported independently of its callers.
- `BodyCx` collects returns and self-tail back edges separately. A summary
  unions explicit return states with the final state only if the body
  falls through. Self-tail entry grows monotonically to a fixpoint.
- `SiteLog` records source marks during the actual walk. Nested marks win
  over the earlier enclosing mark, preserving the inner source. It also
  records re-introduction of taint, not just a new set insertion.
  The adjacency probe `stmt_taints` gets a scratch return frame, no log and
  probing mode, so it cannot deposit live exits or relay diagnostics twice.
- The old global-leak collector is deleted. The global-name collector and
  `bulk_fill_targets` remain useful. `SlotInfo`, `analyze_slots`,
  `tainted_slots_on_entry` and `taint_out_params` deliberately remain.
  `check_slot_entry` preserves their interim diagnostics, including the
  older slot-to-global SB5067.

These are reversible implementation choices, not a new taint policy.
DD127's mechanism section is rewritten to describe them instead of the
unimplemented one-walk I/C/T plan. Its normative text now says a definition
is independently summarised, not literally walked once. GUIDE and DD100
distinguish implemented globals from pending slots and stdout.

### Expression effects

A capture gets a fresh return frame and merges its resulting taint into
the enclosing state. Its own global taint may escape; its incoming reads
and attests are still checked in the body.

Both captures and arithmetic/value calls union effects rather than clearing
incoming names. An arithmetic call in a later operand must not hide an
earlier read:

```
str x=clean
fn clean -> int { x=safe; return 7 }
x=$(/bin/echo dirty)
echo "$x" $(( clean ))
```

Applying the call's clean overwrite before checking the words lost the
located error and reached only the SSA backstop. `taint_value_call` now
applies the summary to a clone and unions it; `.21` pins this ordering,
`.22` a skipped call. This deliberately also loses guaranteed eager kills.
That precision limit was reported to DG and is recorded, with two measured
pre-existing false positives, in `taint-global-summary-followups`.

### Process effects

Precise statement-call overwrites make B18's validator barrier necessary
now, so its half of W4 lands here. `taint_discarded` checks a cloned child
state with its own return frame and no source log. Child reads still count;
writes, returns and source attribution cannot change the parent.

This covers background calls and both final and non-final forked stages.
The final parent reader retains its effects. The lowering has two reader
paths, not one: `dispatch_multistage_pipeline` first handles a final
`while` or array `read`, then recognises scalar `readline` and emits it with
`handle_lastpipe_read` in the parent. The validator preserves both.
A redirected final `while` is SB4021, not an overlooked accepted spelling.

This is not a claim that W4 is finished. In:

```
str x=clean
fn fill { x=$(/bin/echo dirty) }
- echo hi | fill
echo "$x"
```

the new validator accepts the parent read, but the SSA backstop still
rejects `x_2`. The old compiler rejected it with SB5067; the new native
`--no-taint` binary prints `clean`. The runtime isolation is intact and the
backstop false positive remains in B18/W4, with its probe.

## Specs and proof

`taint.sh` grows from 446 to 468 checks per mode, with `gflow.1`-`.22`.

| Cases | Claim |
|---|---|
| .1-.3, .6 | Call-time state, inherited read/attest diagnostics and transitive relays |
| .4-.8 | Single-use generators, shared re-taint attribution, captured and anonymous effects |
| .9-.12 | Child writes discarded; child reads still checked |
| .13-.17 | Return exits, self-tail fixpoint and command/arithmetic call census |
| .18-.19 | A real clean overwrite clears; a conditional overwrite does not |
| .20-.22 | Lazy capture and earlier/skipped arithmetic reads cannot be laundered |

Existing DD127 consequences were rewritten in place:

- `bfb.18`/`.20`/`.21` require SB5020 at the later read, without SB5067.
- `bfb.23`/`.24`/`.28` are accepted.
- `cov.266` requires SB5020. `.264`/`.265`/`.267`-`.269`/`.341` each get a
  second call site so their SB5067 assertions remain meaningful.

The final negative control used the saved W1a compiler (the same compiler
as d8e6491e). Exactly **20 failures per mode**: the six rewritten `bfb`
cases, plus `gflow.1`-`.8`, `.12`-`.16` and `.18`. That is 14 new failing
before-controls; the other eight new cases preserve old behavior and need
reversion witnesses.

Eight independent stubs ran the actual `taint.sh` in native mode:

| Stub | Red cases |
|---|---|
| Mutate the parent instead of a child clone | .9, .10, .11 |
| Stop self-tail analysis after one pass | .14 |
| Count a self call as another call site | .17 |
| Omit incoming flow outputs at a call | .19 |
| Replace, rather than union, capture effects | .20 |
| Replace, rather than union, arithmetic-call effects | .21, .22 |
| Ignore ordinary return-state deposits | .13, .16 |
| Skip call-summary application | bfb.18/.20/.21 and .2/.3/.6/.12/.13/.18 |

Every pre-change-passing new case has a red stub witness. These are
front-end changes; the positive gate runs both backends.

The first capture-union witness was masked: its read followed an `if`
without `else`, whose false edge retained the original taint. Moving the
read into the taken arm of the lazy condition made `.20` fail under the
stub. An earlier literal-vs-literal fixture hit SB5086, and an attempted
capture in a parameter default hit SB4070; neither is evidence for the
merge. The final negative-control counts use only the repaired fixtures.
The new block was also moved before the harness's final `set -e`.

All stubs were restored. The rebuilt compiler is byte-identical to the
saved positive compiler.

## Cost and gate

A compile-only source with 40 functions, each reading all 40 globals, took
0.029 s before and 0.042 s after, using `--emit-c` on this host. This is a
single cost probe, not a general performance claim.

`NEW=44 FLIPS=0 RETIRED=0 TOTAL=10610 mk -p 1 gate` passed on its first
run, under `timeout`:

- `mk test`: 237 compiler, 27 converter and 20 expansion tests.
- Static IDs, variable IDs, plans and both directions of the SB census pass.
- Sweep: 239 seconds, rows **6638313-6648922**.
- **10610 = 10597 passing + 13 skipped**; native 5416, C 5194.
- Exactly 44 first-ever IDs; zero flips, retirements, ghosts or plan mismatches.
- Liveness: 260 IDs differ from their first recorded status.

The staged hook passes formatting, clippy with tests, totality and the live
complexity gate, without a bypass. No coverage run, instrumentation or
dependency change was needed. Standalone negative/stub runs used explicit
binaries and separate TSVs, not the spec history.

## Ledger and next boundary

The ledger has 247 ported, 23 reimplemented, 40 superseded, 7 pending and
13 dropped rows (330 total). Completed globals/plumbing and B18 validator
rows are marked; mixed rows retain their slot, stdout, ordinary-loop or END
remainders. H246's unused Evaluation import and H283's neutral rename are
dropped. H317's branch-only tests are not imported: this walker deliberately
skips definitions, and its source log uses a definition-span fallback rather
than that branch's missing-site ICE.

B100 is closed; B95 records the return/self-tail portion; B18 records
validator isolation and retains the per-process SSA work. B115 is still open.
`taint-global-summary-followups` distinguishes measured eager-overwrite
false positives from the unverified precision questions.

Evidence is archived as the session's `files/port-w1b1-probes.tar.gz`.
W1b-2 is next, only on DG's go: replace the slot machinery, implement B115
and the inherited-slot attest rule, then make the corresponding spec changes.
