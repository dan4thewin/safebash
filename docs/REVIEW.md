# Reviewing sash

Review against the safebash contract, not Bash byte parity. A deliberate
divergence is not a bug; compiler acceptance alone is not proof of intended
semantics. The [design decisions](../notes/decisions/), their implementation
records and [current status](../notes/status.md) establish the distinction.

Use [GUIDE.md](GUIDE.md) for the architecture and source map, and
[AGENTS.md](../AGENTS.md) for the development and evidence procedures.
This checklist is a review aid, not a claim that the implementation is
free of defects.

## Establish the scope

- [ ] Identify the governing decision and whether the relevant behavior
  is delivered, accepted but pending, or provisional. State the intended
  change and preservation cases before judging its implementation.
- [ ] Trace the affected path from `cli_main` in
  [`lib.rs`](../sash/src/lib.rs) through the relevant gates, lowering,
  backends and runtime. Include converter behavior when the change affects
  migration. Do not review only the file containing the final symptom.
- [ ] Find the shared walkers, builders and classifiers before adding
  another implementation. Declared-total enum walkers must retain explicit
  variant coverage; a wildcard can hide the next language extension.

## Frontend and representation

- [ ] Exercise the real CLI for source-level claims. Structural checks,
  resolution, bindings, semantic checks and typing must reach the exact
  SSA used by emission. `compile_to_object` is not a validation API.
  Inspection exits (`--emit-ir`, `--manifest`) and executable backends
  have different final gates; use the route the claim concerns.
- [ ] Follow nested operands and bodies: defaults, subscripts, arithmetic
  calls, captures, wrappers, redirects, functions and EXIT/ERR. An earlier
  refusal of the outer form must not masquerade as proof of the inner rule.
- [ ] Preserve lexical binding identities and source-name restoration.
  Lowering must consume `CheckedTypes`, including implicit reader targets
  and collection element/slot types, rather than infer a second answer.
- [ ] Check collection count/kind/element signatures through forwarding.
  Distinct slot identities and non-overlap with transitively named globals
  must hold independently of taint switches.
- [ ] Allocate every synthetic name through `reserve_temp_name` with its
  real type. Preserve meaningful-zero initialization of newly owned locals,
  including self-tail entry, without resetting live outer state. Do not
  confuse an SSA version with a distinct physical storage cell.
- [ ] Preserve source origins and diagnostic identity through nested
  parsing and desugaring. User errors must not become panics or successful
  empty defaults. Internal invariant guards need construction evidence,
  not merely a green source sweep.

## Taint and executable roots

- [ ] Classify the acquisition, not the writer's reputation or pathname.
  Kernel reads introduce taint; compiler-owned copies introduce none and
  erase none. Buffer eligibility and lowering must agree, and the SSA
  backstop must inspect the transport actually emitted.
- [ ] Check legal adjacency, subject identity and ownership at resolution.
  Ordinary overwrite, unset or trusted self-copy must not clear unresolved
  taint. Received taint cannot be attested in a callee; single-use outward
  resolution and shared-function ownership have different obligations.
- [ ] Check summary application and reaching definitions across calls,
  lazy branches, loop back-edges, counted transfers and returns. Preserve
  formal-to-actual slot mapping without an unused global identity flow
  undoing slot cleanup.
- [ ] Keep main, handlers and forked children separate. Non-executed
  function templates are not parent definitions; child writes cannot
  update parent analysis state. Check both source analysis and the SSA
  backstop, not just one layer's refusal.
- [ ] Be precise about analysis limits.
  [DD127](../notes/decisions/DD127-taint-across-function-boundaries.md#implementation-status)
  records the completed-parent view inherited by children and remaining
  redirect lookup limits. Do not describe those as exact fork-time state.
  `:!` and taint-disable switches establish no data-validation proof and
  must not disable independent type, placement or aliasing checks.

## Execution, cleanup and failures

- [ ] Check word/component evaluation, lazy operands and redirect timing
  against the governing contract and the change's stated scope. Preserve
  already-evaluated values across later effects. Handling an operation's
  status must not suppress checks in its operands or called bodies.
- [ ] Keep void, status, integer and stdout channels distinct through
  captures, redirects, forwarding and pipelines. Explicit discard does
  not repair a type mismatch; constant function results are not errors.
- [ ] Account for every descriptor, child and capture frame on normal,
  failed and control-transfer paths. Parent readers must consume before
  waiting, close before reaping, finish in nesting order and preserve the
  required outcome. A fork child must not inherit parent cleanup duties or
  branch into parent continuation code.
- [ ] Check failure cause separately from reported status and from the
  syntactic location of setup. Consult the current DD/task delivery state
  rather than silently presenting a pending failure-policy rollout as
  complete. Failed setup must not run the operation against unintended I/O
  or be replaced by success-shaped output.
- [ ] Check EXIT/ERR against actual completed global writes and their
  separate incoming-state rules. `exit` selects EXIT regardless of status;
  `fail` selects ERR, including zero. Preserve once-only, main-process-only
  dispatch, EXIT-to-ERR progression and allocation-free fatal dispatch.
  Signal deferral must protect legal resolution without postponing ordinary
  failures or explicit termination.

## Backends and runtime

- [ ] Follow the shared SSA into both native and C emission. Equivalent
  output is useful evidence, but agreement cannot rule out a shared
  frontend, lowering or runtime bug. Direct backend tests must not be
  presented as end-to-end compiler tests.
- [ ] Verify runtime ABI types, lengths, terminators, ownership and
  lifetimes. Check size arithmetic before allocation/copying and integer
  fault behavior in both backends. Pointer/length strings and Rust source
  do not by themselves prove the unsafe, C or assembly boundaries sound.
- [ ] Verify syscall, signal and assembly contracts for the selected
  target. Build shared C changes into both consumers where applicable and
  rebuild the separate runtime before linking. A shim that compiles for an
  architecture is not evidence that the full compiler/runtime works there.

## Evidence and handoff

- [ ] Add source behavior coverage to the existing spec suite using the
  actual built CLI and helpers. Reserve direct unit tests for internal
  contracts source programs cannot exercise; avoid rival assertions already
  pinned by specs.
- [ ] Show the intended assertion fails before the change or under a
  targeted reversion, then passes afterward. A diagnostic assertion must
  identify the intended rule/site, not just nonempty stderr or a co-fired
  code. Read the harness's retained xtrace before explaining a failure.
- [ ] Follow the [spec gate procedure](../AGENTS.md#spec-tests): historical
  ID high-water marks, count predictions, native/C scope and result-history
  accounting matter. A passing build or `spec/run.sh` exit status is not a
  substitute for the applicable `mk gate` result.
- [ ] Treat coverage as reachability, not assertion strength. Check
  measurement freshness and use positive controls before concluding an
  arm is unreachable. Enumerate producers before narrowing a type or
  deleting a path; the new program compiling does not prove equivalence.
- [ ] Record what changed, what was actually proved and what remains open.
  Keep the journal/status and affected architecture documentation coherent,
  while leaving detailed procedures and language policy in their canonical
  records. Do not turn old measurements into claims about the new tree.
