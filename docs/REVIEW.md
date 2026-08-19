# Security Review Guide

*Reading order and verification checklist for human reviewers.*

## What sash guarantees

A compiled sash binary:
1. Executes only commands visible in the source (no eval, no dynamic dispatch)
2. Cannot use external data without explicit validation (`bless`)
3. Contains no implicit environment fallbacks ($HOME "just working")
4. Runs external commands via direct execve (no shell interpretation)
5. Has its source embedded in `.sash_source` for reproducibility

## Architecture overview

```
Source → Parse (tree-sitter) → Validate (rules/) → Taint check
       → Lower (CST→IR) → Type check → Transform (IR→SSA)
       → Constant fold → Codegen (Cranelift) → Link → Static ELF
```

Every stage rejects — none silently passes through unknown constructs.
If a construct isn't explicitly handled, compilation fails.

## Recommended reading order

### Phase 1: The specification (what's allowed)

| File | What to verify |
|------|---------------|
| `docs/SUBSET.md` | The safe subset definition |
| `src/rules/*.rs` (16 files) | Each rejected construct has a validator rule |
| `src/validator.rs` | All rules are registered and run |

**Key question:** Is there any Bash construct that isn't in SUBSET.md
and also isn't rejected by a rule?

### Phase 2: Data provenance (taint model)

| File | What to verify |
|------|---------------|
| `src/taint.rs` | CST-level taint tracking |
| `src/ssa_taint.rs` | SSA-level taint verification |

**Key question:** Can tainted data reach a sensitive position (command
name, file path) without passing through `bless`?

Sources of taint (must ALL be tracked):
- `$(getenv NAME)` — environment variables
- `$(argv N)` — positional parameters
- `read VAR` — stdin/fd input
- `$()` command substitution of external commands

Clean values (must NOT be taintable from outside):
- String literals in source
- Arithmetic on clean operands
- Compiler-managed: `$?`, `$$`, `$#`, `$!`

### Phase 3: Lowering (faithful translation)

| File | What to verify |
|------|---------------|
| `src/lower.rs` | CST → IR is faithful (no invented semantics) |
| `src/ir.rs` | IR can only represent safe-subset constructs |
| `src/transform.rs` | IR → SSA preserves semantics |
| `src/ssa.rs` | SSA representation (instructions, values, blocks) |

**Key question:** Does the lowering introduce any behavior not present
in the source? (e.g., implicit variable initialization, hidden fallbacks)

### Phase 4: Code generation (the trust boundary)

| File | What to verify |
|------|---------------|
| `src/codegen.rs` | SSA → machine code |
| `sash-rt/src/lib.rs` | Runtime: what syscalls can the binary make? |

**Key questions:**
- Does the codegen emit any instructions not derivable from the SSA?
- Does the runtime provide any capabilities not needed by the safe subset?
- Are string operations bounds-checked (ptr+len, null terminator)?
- Is fork/exec the only way to run external commands? (no `system()`)

### Phase 5: Command resolution (supply chain)

| File | What to verify |
|------|---------------|
| `src/rules/command_resolution.rs` | Bare commands resolved at compile time |
| `src/transform.rs` (`resolve_command_path`) | PATH resolution logic |

**Key question:** Can a compiled binary execute a command not visible
at compile time? (Answer should be NO — all paths are absolute after
resolution, embedded in the binary.)

## Trust annotations in source

The codebase uses inline trust annotations:

```rust
// TRUST: <assertion about why this is safe>
// REJECT: <this arm should be unreachable — validator gates it>
// SAFETY: <unsafe operation — why it's correct>
```

`grep -r "TRUST:\|REJECT:\|SAFETY:" sash/src/` to find all trust claims.

## Specific attack surfaces to verify

### 1. Validator bypass
Can any input reach codegen without passing through validation?
- Check: `compile_to_object` in `codegen.rs` — does it always validate first?
- Check: test helpers that call `compile_ssa` directly bypass validation
  (this is intentional for testing but reviewers should note it)

### 2. Taint escape
Can tainted data reach command execution?
- Check: `lower_exec` in `codegen.rs` — where does the command name come from?
- Check: pipeline stages — are names always from SSA (which was validated)?
- Check: `bless` — does it actually gate usage or just mark?

### 3. Environment injection
Can environment variables influence compiled binary behavior?
- Check: `sash_getenv` in runtime — only called for explicit `$(getenv NAME)`
- Check: no implicit env-var mirroring in codegen (removed by DD#53)
- Check: PATH is only used at *compile* time, not runtime

### 4. Integer overflow
Can arithmetic overflow cause unexpected behavior?
- Check: `emit_arith_op` in `codegen.rs` — overflow model
- Check: DD#25 overflow decision

### 5. String buffer overflow
Can string operations write past allocated memory?
- Check: all string ops in runtime use explicit length
- Check: `sash_concat` — does it allocate len1+len2+1?
- Check: null terminator always written

## Files by size (complexity budget)

```
codegen.rs      6593 lines  — instruction emission (largest, most critical)
transform.rs    4012 lines  — IR→SSA (second largest)
lower.rs        3043 lines  — CST→IR
emit_janet.rs   1280 lines  — Janet backend (not security-critical)
taint.rs         975 lines  — taint tracking
lib.rs           607 lines  — CLI and preprocessing
type_check.rs    555 lines  — type checker
ssa.rs           516 lines  — SSA data structures
ssa_taint.rs     330 lines  — SSA taint verification
ssa_printer.rs   287 lines  — SSA pretty-printing (debug only)
```

## Design decisions relevant to security

| DD# | Topic | Security relevance |
|-----|-------|-------------------|
| DD#12 | String representation | Ptr+len prevents buffer overflows |
| DD#14 | Pipeline primitives | Fork/exec only, no shell interpretation |
| DD#23 | Command resolution | Compile-time PATH, no runtime lookup |
| DD#24 | Taint/bless model | All external data tracked |
| DD#42-44 | getenv/argv/argc | Explicit data entry points |
| DD#53 | Env/var separation | No implicit env-var mirroring |
| DD#55 | Exit checks | Nonzero exit aborts (fail-closed) |
| DD#56 | Signal model | Controlled signal handling |

Full decisions in `notes/decisions/` (one file per DD#).
