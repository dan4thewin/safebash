# Session 2026-04-24 — Bless Runtime Enforcement, CST Taint Fixes, DD#46

**Goal:** Close the gaps identified in GAPS-2026-04-24.md around bless
runtime enforcement, CST taint over-tainting, arithmetic/subscript
taint checking, and DD#46 out-of-range positional detection.

**Approach:** TDD — wrote 27 taint spec tests first (13 passed
immediately, 14 failed), then implemented until all pass.

### Actions

1. **Bless runtime enforcement (codegen)**
   - Added `SsaInst::Bless { var, test }` to SSA instruction set.
   - `lower.rs`: re-parses `bless x [[ cond ]]` via tree-sitter to
     extract a proper `TestExpr` (tree-sitter sees bless as a plain
     command with word arguments — the `[[ ]]` isn't a test_command).
   - `transform.rs`: recognises bless command, emits `SsaInst::Bless`.
   - `codegen.rs`: emits `lower_test_expr` → branch → stderr message
     "bless: validation failed\n" → `sash_exit(1)` → trap on fail path.
   - Pre-interns the failure message string alongside test expr literals.

2. **CST taint over-tainting fixes (taint.rs)**
   - `read` from herestring (`<<<`) or heredoc (`<<`) now produces
     clean variables instead of unconditionally tainting.
   - `$(user_func)` command substitution now recognised as clean
     (user-defined functions can't introduce external taint by DD#24).
     Added `functions: HashSet<String>` to TaintTracker.
   - `arithmetic_command` (`(( expr ))`) — new walk_statement case
     for the safebash grammar's `arithmetic_command` node type.
     Checks all `variable_name` nodes inside for taint.
   - `arithmetic_expansion` (`$(( expr ))`) — check_expr_reads now
     recurses into arithmetic expansion nodes via `check_arith_reads`.
   - Array subscript `${arr[$x]}` — check_expr_reads now recurses
     into subscript index expressions inside `expansion` nodes.

3. **DD#46 out-of-range positional detection (transform.rs)**
   - `collect_max_positional()` scans function body IR for the
     highest positional parameter reference ($1..$9).
   - `scan_expr_positionals_stmt/expr` — recursive IR walkers.
   - At each function call site in `lower_one`, compares
     `max_positional` against `c.args.len()`. If body reads $K
     where K > argc, emits a diagnostic.
   - `VersionState.diagnostics: Vec<String>` — new field to
     collect transform-phase diagnostics.
   - `ir_to_ssa`/`ir_to_ssa_typed` now return `(SsaProgram, Vec<String>)`.
   - `compile_to_object` returns `Result<Vec<u8>, Vec<String>>`.
   - `main.rs` handles the Result, printing diagnostics to stderr.

4. **Test updates**
   - `taint_spec_tests.rs`: 27 new tests covering all taint/bless
     scenarios from GAPS doc + bless expression mismatch aborts.
   - `e2e_tests.rs`: removed unnecessary `bless` from herestring
     tests (now clean by construction); updated
     `pos_func_unbound_positional_is_empty` to expect rejection.
   - `codegen_tests.rs`: updated for `Result` API.

### Design Decision #47: Bless Runtime Enforcement

**Context:** The `bless` keyword was parsed and validated at the CST
level (taint.rs) but had no runtime effect — the compiled binary
silently accepted values that failed the test expression.

**Decision:** `bless` is now a first-class SSA instruction with
codegen support. The test expression is evaluated at runtime; on
failure, the binary writes "bless: validation failed\n" to stderr
and exits with code 1. This matches the SUBSET.md specification
that bless "narrows the value at the point of the test."

**Alternatives considered:**
- Trap signal instead of exit: too harsh, prevents cleanup.
- Return nonzero $?: too subtle, caller might ignore.
- stderr + exit(1) is the right balance of visibility and safety.

### Design Decision #48: Out-of-Range Positional Detection

**Context:** DD#46 cleared unbound positional slots to empty strings
but the comment noted "(Future: emit a diagnostic instead)".

**Decision:** At function call sites, scan the function body IR for
the highest $N reference. If the call provides fewer arguments than
the function reads, emit a compile-time error. This catches bugs
like `foo() { echo "$2"; }; foo "one"` at compile time.

**Limitation:** This is a conservative static analysis — it won't
catch dynamic patterns like `shift` followed by `$1`, or positional
references inside eval'd strings. Those patterns are already
rejected by the safe subset.

DG directed the architecture. AI implemented.

---

