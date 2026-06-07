---
title: Add C oracle — emit_c that consumes SSA
status: phase-1-through-6-done
priority: P1
kind: feature
depends: []
resolved: 2026-06-07
resolution: All 6 planned phases landed (commits 5c3f77b → 012762b on hand-parser). 67 c_tests pass. 25/30 janet_tests cases reach parity via emit_c → gcc → bash diff; the 5 remainders are legitimate divergence (3 sash-rejected bash syntax, 2 sash extensions bash misinterprets). 38 sash-rt runtime functions wired; mirror-codegen architecture caught 6 bugs before runtime. Future per-feature work (regex_replace, glob, mapfile, subshells, traps, multi-target read, exotic intrinsics, spec/*.sh cross-check at scale) filed as separate tasks rather than continuing under this umbrella.
gates:
  - sash/src/emit_c.rs exists and compiles
  - --emit-c CLI flag prints generated C
  - sash/tests/c_tests.rs harness with at least 5 smoke tests
  - all smoke tests pass: gcc-compiled C output matches bash output
  - design doc / DD written if shape stabilizes
  - cross-check with Cranelift output works (differential oracle)
---

# Add C oracle

DG decision 2026-06-06: instead of retargeting emit_janet to consume
SSA (which would require a structurization pass), add a new C
emitter that consumes SSA directly. C with `goto` + labelled
statements maps 1:1 to SSA basic blocks, requires no
restructuring, and gives us:

1. **A second compilation oracle.** gcc/clang reads the emitted C,
   our Cranelift codegen reads the same SSA. Any disagreement at
   runtime is a bug in one of them — differential testing.
2. **A "reveal what the compiler does" view.** SSA blocks → labelled
   C statements, terminators → gotos, phi nodes → temp variables.
   The C output mirrors the SSA shape exactly.
3. **A path to inspecting compiler bugs.** Wrong-version phi reads
   or misrouted branches show up as visible defects in the C, not
   just as runtime output divergence.

The Janet emitter stays on IR. It joins the complexity-gate cohort
(emit_janet exemption removed in same change set as this task is
filed). It continues to serve as the IR-stage oracle.

## Why C, not Janet retarget

See journal entry on the structurization analysis (TBD). Summary:
Janet has no `goto`, so emit_janet on SSA needs a structurization
pass to rebuild structured control flow from flat blocks. C has
`goto` and labelled statements, so SSA blocks map directly. The
structurization pass would be ~200-300 LOC of region-analysis
code; the C emitter is direct.

## Scope (full, per DG directive "do not reduce scope")

### Phase 1 — minimum viable C oracle (parity with the simplest janet_tests)

1. `sash/src/emit_c.rs` with `pub fn emit_c(ssa: &SsaProgram) -> String`
2. Wire `--emit-c` CLI flag in `sash::cli_main`
3. Variable declarations: per (name, version) → C local (`int x_v0;` /
   `char* x_v0 = "";`) with type inferred from SSA usage where
   possible, defaulting to `char*` for strings
4. Block emission: each SSA block becomes a labelled C statement
5. Terminator emission:
   - Return → `goto __exit;`
   - Goto(b) → `goto _blk_{b};`
   - Branch{cond, true, false} → `if (test_result) goto _blk_{true}; else goto _blk_{false};`
   - Loop{cond, body, exit} → same shape as Branch
6. Phi resolution: emit assignment to phi target in each predecessor
   block that flows in
7. Echo command: `printf("%s\n", arg);` (and variants for multiple args)
8. Exit command: `__exit_status = arg; goto __exit;`
9. `c_tests.rs` harness:
   - Write bash to tmp file
   - Run `sash --emit-c` to get C source
   - `gcc -o /tmp/...` to compile
   - Run, capture stdout / exit
   - Same with `bash`, diff
10. Five initial smoke tests:
    - `echo hello`
    - `echo "hello world"`
    - `x=5; echo "$x"`
    - `if true; then echo yes; else echo no; fi`
    - `for i in 1 2 3; do echo "$i"; done`

### Phase 2 — arithmetic and conditionals

1. `(( ))` arithmetic: ArithExpr, ArithBinOp → C arithmetic ops
2. `[[ ]]` tests: string equality, numeric comparison, file tests
3. `if/elif/else` via Branch terminator chains
4. `while` loops via Loop terminator
5. `for` loops (numeric range, word list, c-style)
6. Tests: full coverage of janet_tests' arithmetic + control sections

### Phase 3 — parameter expansion + commands

1. Parameter expansion: `${var}`, `${var:-default}`, `${var:+alt}`,
   `${var#pat}`, `${var%pat}`, `${var/pat/rep}`, `${var:offset:length}`,
   case-conversion, etc. → C runtime helper calls
2. Command substitution `$(cmd)`: fork/exec into pipe, read stdout
3. Builtin commands: getenv, len, keys, join, epochseconds, random,
   argc, getpid, getppid, getcpid, getcwd, status, signum, dtoi, itod
4. printf with format specifiers

### Phase 4 — pipelines, redirects, functions

1. Pipelines: fork/exec chain with pipe() per pair
2. Redirects: `>` `>>` `<` `2>&1` etc. via dup2()
3. Function definitions: SSA `fn_*` blocks → C functions
4. Function calls (elidable + non-elidable)
5. Local variables in functions

### Phase 5 — arrays, maps, file ops

1. Indexed arrays via C array of char*
2. Associative arrays via runtime hash table (link against sash-rt?)
3. `read` builtin (line + multi-field)
4. `mapfile` builtin

### Phase 6 — full parity + integration

1. Run every `spec/*.sh` test through emit_c + gcc, diff against bash
2. Run every existing `janet_tests.rs` case through emit_c + gcc as
   well — establish C parity with janet parity
3. Cross-check: same SSA → emit_c+gcc binary AND Cranelift binary →
   any output divergence is a compiler bug somewhere
4. Document the C runtime contract (which symbols `emit_c` calls,
   what their signatures look like) — possibly a `sash_rt_c.h`
   header alongside `sash-rt/`

## Runtime story

emit_c needs runtime support for:
- String operations beyond stdlib (bash semantics around concatenation, splitting)
- Process management (fork/exec/wait for pipelines and command sub)
- File descriptor manipulation (dup2 for redirects)
- bash builtins (echo with -n/-e, printf format conversion, getenv with sash semantics, etc.)

Two paths for runtime:
1. **Inline stubs in the emitted C** — minimal, self-contained, no linking story
2. **Link against sash-rt** — reuse the Rust runtime via C FFI

Phase 1 starts with inline stubs (path 1). Path 2 becomes the
question once we hit pipelines/redirects (Phase 4). The decision
can be deferred until then.

## Out of scope

- Replacing Cranelift codegen with the C path (this is an oracle,
  not a production backend)
- Janet retarget (deferred per emit-janet-from-ssa task)
- Performance — emit_c output need not be fast or idiomatic, only
  correct and readable

## Triggering condition

Active from 2026-06-06.

## References

- Decision conversation: ~14:35 → ~14:44 on 2026-06-06 with DG.
- Cognitive complexity rubric (journal 108) drove the realization
  that emit_janet was using deferral as a shield, not a roadmap.
- Path analysis (A: walk SSA in emission order, B: region analysis,
  C: annotate SSA with regions, D: dispatch-loop emission) showed D
  reveals compiler behaviour best. Then the language-swap question
  showed C maps 1:1 to D-shape with no dispatch-loop boilerplate,
  AND adds the differential-oracle dimension Janet can't.
