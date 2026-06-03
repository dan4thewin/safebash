# Journal 090 — retire --ir / --ssa dump modes

## Goal

DG observation: `--ir` and `--ssa` pretty-printers are inferior to
`--janet`. `--janet` produces executable output backed by 401 lines
of property tests; `--ir` and `--ssa` produce inert text with zero
tests, zero docs, zero consumers besides authoring debug.

Decision: retire both pretty-printers now (option B); file a future
task to retarget `emit_janet` onto SSA (option D, the proper fix
that subsumes `--ssa`'s niche).

## What was retired

| file | LOC | exposed via |
| --- | --: | --- |
| `sash/src/printer.rs` | 548 | `--ir` |
| `sash/src/ssa_printer.rs` | 234 | `--ssa` |
| **total** | **782** | |

Plus:
- `transform::ir_to_ssa` (the 2-arg convenience wrapper, only
  consumer was the `--ssa` branch). Codegen uses
  `ir_to_ssa_typed` directly. `mod.rs` re-export removed.
- Two `if parsed.flags.contains(&"--ir")` / `--ssa` branches in
  `cli_main`.
- Two `mod printer` / `mod ssa_printer` declarations.
- Two usage-line references (the `--help`-equivalent eprintln!s
  at lib.rs:72 and lib.rs:212).
- Two README lines under "Compiler Modes".

Net delta after removal: roughly **−820 LOC**.

## What `--janet` does that `--ir` and `--ssa` don't

| flag | output | executable | tests |
| --- | --- | --- | --- |
| `--janet` | Janet Lisp source | yes (runs in `janet`) | 401 LOC, 38 tests |
| `--ir` | bash-like indented text | no | 0 |
| `--ssa` | basic blocks + phi | no | 0 |

`--janet` does triple duty:
1. Inspection (same semantic domain as `--ir`).
2. Executable artifact — the Janet runtime is a legitimate
   alternate backend.
3. **Property test oracle** — `janet_tests.rs` runs the same script
   through bash and through compiled Janet, diffs the outputs.

For the IR-stage inspection use case, `--janet` strictly dominates
`--ir`: anything `--ir` can show, `--janet` shows in executable
form with property tests covering the semantics.

## What is lost (briefly)

**SSA-stage inspection** is gone. The IR → SSA transform pass —
phi insertion, basic block formation, var versioning, branch
terminators — no longer has a textual dump. Workflow for
transform-stage bugs becomes:

- Build a small repro script.
- Build sash at a known-good commit; run; observe behavior.
- Build sash at the suspect commit; run; observe; bisect.

Coarser than `--ssa | diff`, but the suspect commits are
identifiable from git history anyway, and the spec test suite
covers most of the transform's failure modes.

## What replaces it (future)

**Task: `notes/tasks/emit-janet-from-ssa.md`** files the proper
fix. Retarget `emit_janet::emit_janet` to take `&SsaProgram`
instead of `&Program`. This unifies the test oracle with
SSA-stage inspection — post-transform output regains a tested +
executable inspection surface, and the property oracle covers a
stage it currently skips entirely.

Big lift: structurization pass to rebuild structured control flow
from flat basic blocks (Janet has `cond` / `while` / `for` natively,
not `goto`). ~1331 LOC of emit_janet + 401 LOC of tests to revalidate.

Triggering condition: after Phase 10 (C-verdict refactors)
completes and transform-stage churn slows.

## Top-5 after this commit

```
49 check_stmt
43 validate_stmt_ctx
29 scan_value_for_vars
28 emit_command_kind
28 taint_stmt
```

`print_ssa` (CC 29) dropped out — it doesn't exist anymore.

## Files touched

- **Deleted:** `sash/src/printer.rs`, `sash/src/ssa_printer.rs`.
- `sash/src/lib.rs` — remove 2 `pub(crate) mod` declarations, 2 if
  branches, 2 usage lines.
- `sash/src/transform/entry.rs` — remove `ir_to_ssa` convenience fn.
- `sash/src/transform/mod.rs` — remove `pub use` of `ir_to_ssa`.
- `README.md` — remove `--ir` / `--ssa` lines from Compiler Modes.
- `notes/tasks/emit-janet-from-ssa.md` — new (option D for the future).

## Gates

- `cargo build --release` clean
- `cargo clippy -D warnings` clean
- `cargo test --lib` 12/12
- `spec/run.sh` 291/291
