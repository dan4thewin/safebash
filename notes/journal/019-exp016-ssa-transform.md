# Experiment 016: SSA transformation

**Goal:** Bundle experiments 020–024 into one: transform safebash IR
into SSA (Static Single Assignment) form with versioned variables,
basic blocks, phi nodes, and a printer.

**Architecture:**

```
src/
  ssa.rs         — SSA types: VarRef, PhiNode, SsaInst, BasicBlock,
                   Terminator, SsaProgram, VersionState
  transform.rs   — IR → SSA conversion
  ssa_printer.rs  — Pretty-print SSA as readable text
```

**Key types:**
- `VarRef(name, version)` — versioned variable reference
- `SsaInst` — Assign (versioned), Command, Phi
- `SsaValue` — Literal, Var(VarRef), Expr(string)
- `BasicBlock` — label, phi nodes, instructions, terminator
- `Terminator` — Goto, Branch, Loop, Return
- `VersionState` — tracks current version + high-water mark per
  variable (counter never rolls back across branches)

**Bug found and fixed:** Version counter rollback. The initial
`VersionState` used a single `current` map for both tracking and
assigning. When `restore()` rolled back to a pre-branch snapshot,
it also rolled back the counter — so both branches of an if/else
could assign the same version number (e.g., x_2 in then and x_2
in else). No phi node was generated because the versions appeared
identical. Fixed by splitting into `current` (tracks visible
version, reset on restore) and `counter` (high-water mark, never
rolled back). Now then gets x_2, else gets x_3, phi merges them
into x_4.

**Control flow lowering:**
- **If/else:** condition → Branch → then block + else block →
  Goto merge. Phi nodes at merge for variables modified in
  either branch.
- **If (no else):** Branch → then + fallthrough to merge.
  Phi merges then-version with pre-branch version.
- **While/Until:** entry → Goto header → Loop → body → Goto header.
  Phi nodes at header for loop-carried variables.
- **For:** same as while, with loop variable assigned at header.
- **Case:** each arm in its own block → Goto merge. Multi-source
  phi nodes at merge.
- **Function:** separate block (fn_name), Return terminator.
- **Pipeline/List:** sequential in same block (parallelism and
  short-circuit are runtime concerns, not SSA concerns).

**Sample output (deployment script):**
```
bb0 [entry]:
  DEVICE_1 = "/dev/sda1"
  MOUNT_POINT_1 = "/mnt/update"
  br condition → bb2, bb4
bb2 [if.then]:
  STATUS_1 = "exists"
  → bb3
bb4 [if.else]:
  STATUS_2 = "missing"
  → bb3
bb3 [if.merge]:
  STATUS_3 = phi(bb2:STATUS_1, bb4:STATUS_2)
  → bb5
bb5 [for.header]:
  img_1 = <iter>
  loop for img in ... → bb6, exit bb7
bb7 [for.exit]:
  echo "$STATUS_3"
  ret
```

**Tests:** 34 tests covering SSA types (VarRef, VersionState,
SsaProgram), sequential versioning, version references, unset
variables, declarations, commands, if/else phi insertion,
if-no-else phi, no-modification-no-phi, phi version correctness,
while loop structure + loop-carried phi, for loop structure +
variable versioning, case merge + phi, function blocks, pipeline
+ list, printer output (versioned vars, phi nodes, blocks,
terminators, goto arrows), realistic script integration, return
terminator, empty program.

**Conclusion:** Phase 4 (SSA) complete. The pipeline is now:
source → CST → IR → SSA. Versioned variables and phi nodes
enable the data-flow analysis needed to resolve SB4004
(variable-as-command deferred diagnostics) in a future pass.

---

