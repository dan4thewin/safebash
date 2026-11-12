title: Retarget emit_janet to consume SSA instead of IR
status: done
priority: P3
kind: refactor
depends: []
superseded-by: add-c-oracle
resolved: 2026-06-07
resolution: "Superseded by add-c-oracle (now also done). DG decided 2026-06-06 that the SSA-stage oracle should be emit_c (which maps 1:1 to SSA via goto+labels without a structurization pass) rather than retargeting emit_janet. The Janet emitter stays on IR as the IR-stage oracle and joined the complexity-gate cohort. emit_c now provides the SSA-stage inspection + differential-oracle role this task was meant to give Janet. See notes/tasks/add-c-oracle.md. REAFFIRMED 2026-08-16: DG revisited \"make janet like c (thin + shared runtime)\" and again kept janet on IR — but adopted the primitive-library refinement (a `sash-rt.janet` runtime + thin leaf lowering, control flow stays IR-based). See notes/tasks/janet-rt-primitive-library.md."
gates:
  - all 401 lines of janet_tests.rs pass after the rewrite
  - 291/291 spec gates remain green
  - no regression in cross-output property tests

# Retarget emit_janet to consume SSA instead of IR

## Current state (2026-06-03)

`emit_janet::emit_janet(&program)` takes the post-lower `Program`
(IR), not the post-transform `SsaProgram`. This is the original
"Janet Oracle" milestone shape from the README — IR → Janet, runs
in `janet`, diff-tested against bash via `janet_tests.rs` (401 LOC,
38 tests).

Because Janet input is IR, **the Janet output cannot diagnose
transform-stage (IR→SSA) bugs**. The transform pass — phi insertion,
basic block formation, var versioning, branch terminators — is
invisible to the Janet oracle.

## Why this matters

The transform pass is the **highest-churn stage in the compiler**.
Wave 2 of r47-abstraction-audit landed ~22 refactors there
(`transform/cmd_handlers.rs`, `transform/expressions.rs`,
`transform/control_flow.rs`, etc.). Yet the Janet oracle never sees
transform output.

The `--ssa` dump (the only post-transform inspection tool) was
retired in commit `<this commit>` because it had no tests, no docs,
and no executable form. With this proposed change, post-transform
output regains a tested + executable inspection surface, **and** the
property-based oracle gains coverage of a stage it currently skips.

## Proposed change

Rewrite `emit_janet::emit_janet` to take `&SsaProgram` instead of
`&Program`. New shape:

```rust
pub fn emit_janet(ssa: &SsaProgram) -> String { ... }
```

Caller:

```rust
let (ssa, _diags) = transform::ir_to_ssa(&program);
print!("{}", emit_janet::emit_janet(&ssa));
```

## Scope

`sash/src/emit_janet.rs` is **1331 LOC**. Most of the file is
expression / arithmetic / parameter-expansion lowering that maps
cleanly from IR shapes to SSA values (or vice versa). The bigger
surgery is in control flow:

- IR: nested `If`, `While`, `For`, `Case`, `Pipeline`, `Group`,
  `Subshell`, `Redirected` wrapper nodes.
- SSA: linearized basic blocks with `phi` nodes and branch
  terminators.

Janet has `cond`/`while`/`for` natively, so emitting structured
Janet from SSA basic blocks requires **structurization** — rebuilding
the nested control flow from the flat block graph. That's a
non-trivial pass in its own right (compare: `regalloc2` consumers,
or `wasmtime`'s SSA→structured-control-flow lowering for WASM).

## Alternatives

1. **Don't structurize — emit Janet labels + `goto`-style.** Janet
   doesn't have `goto`. Would need a CPS-style transform with
   continuations, or a top-level dispatch loop over block IDs. Ugly
   Janet output, hard to read for debugging — defeats most of the
   "inspection" value.

2. **Emit Janet from a midpoint IR.** Add a "structured SSA" stage
   between IR→SSA and SSA→codegen that preserves block hierarchy
   without going fully to phi-nodes-and-branches. Doable but adds a
   new compiler stage to maintain.

3. **Keep emit_janet on IR; add a separate SSA-to-Janet emitter.**
   Doubles maintenance surface; abandons the unification goal.

## Risk

Janet oracle is the most rigorously-tested piece of the compiler
(38 tests covering arithmetic, control flow, pipelines, param
expansion, redirects). Breaking it during the rewrite would be
expensive to debug. Recommended sequencing:

1. Land structurization pass in isolation, with its own unit tests
   over hand-crafted SsaProgram fixtures.
2. Build a parallel `emit_janet_from_ssa` next to the existing one.
3. Once parity is demonstrated on all 38 tests, swap.
4. Delete the IR-consuming emit_janet.

## Triggering condition

File this for "when transform/SSA churn slows down". As long as
Wave 3+ continues to land helpers in `transform/`, the value of
SSA-stage testing is high — but the rewrite itself competes for
attention with the refactors. Reasonable trigger: after Phase 10
(C-verdict wave) completes and a stretch of transform-stage stability
makes the investment a clear win.

## References

- Predecessors deleted in this commit: `sash/src/printer.rs`
  (`--ir`), `sash/src/ssa_printer.rs` (`--ssa`)
- Journal 090 documents the retirement
- Janet oracle tests: `sash/tests/janet_tests.rs`
- Janet emitter: `sash/src/emit_janet.rs` (1331 LOC)
- SSA representation: `sash/src/ssa.rs`
- Transform pass: `sash/src/transform/`
