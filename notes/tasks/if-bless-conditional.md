title: Implement if-bless conditional form
status: done
severity: correctness
gates: [24.9, 24.12, 24.13]

DD#24 specifies two bless forms:
- `bless VAR [[ test ]]` → fatal exit on failure (implemented)
- `if bless VAR [[ test ]]` → false branch taken, VAR remains tainted (not implemented)

Current state: transform.rs emits SsaInst::Bless unconditionally.
Codegen always emits the fatal path (write stderr, exit 1).

Implementation plan:

1. **transform.rs**: when bless appears as the condition of an if-statement,
   emit a different SSA pattern. Instead of SsaInst::Bless followed by
   continuation, emit SsaInst::BlessCond (or flag on Bless) that branches
   to true_block (VAR clean, new SSA version) or false_block (VAR stays
   at original tainted version).

2. **ssa.rs**: add BlessCond variant or add a `conditional: bool` flag
   to existing Bless.

3. **codegen.rs**: for conditional bless, emit the test evaluation and
   branch to pass/fail blocks, but fail block continues execution
   instead of calling exit. Target var only defined in pass block;
   fall-through uses original tainted SSA version.

4. **ssa_taint.rs**: BlessCond target is clean only in the true branch.
   The false branch must reject reads of the var (still tainted).

5. **taint.rs**: recognize `if bless` at the CST level — bless as the
   condition of an if-command.

6. **Tests**: add e2e and taint_spec tests for if-bless pattern:
   - true branch uses blessed var → compiles and runs
   - false branch uses tainted var → compile error
   - runtime: matching input → true branch, non-matching → false branch
