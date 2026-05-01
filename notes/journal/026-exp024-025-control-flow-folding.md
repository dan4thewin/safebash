# Experiment 024: Control flow → Cranelift blocks/branches

**Date:** 2026-04-13

**Goal:** Compile if/else conditionals and for loops to native
machine code via Cranelift's block/branch infrastructure.

**Key changes:**

1. **TestExpr preserved through SSA.** Added `SsaInst::Test(TestExpr)`
   variant. The SSA transform now emits `Test` instructions for
   `[[ ]]` commands instead of lossy `Command{name:"[[", args:[Expr]}`.
   Codegen receives the full typed test expression.

2. **Inline string comparison.** `emit_string_eq()` compiles string
   equality to a byte-by-byte comparison loop directly in Cranelift
   IR. No runtime function call — pure register/memory operations:
   - Check lengths first (fast path for unequal)
   - Loop over bytes, comparing each pair
   - Result is i64 (1=equal, 0=not-equal)
   Handles `=`, `==`, `!=`. Also `-n` (non-empty) and `-z` (empty)
   via simple length checks.

3. **SSA blocks → Cranelift blocks.** Each SSA `BlockId` maps to a
   Cranelift block. Terminators map directly:
   - `Goto(target)` → `jump`
   - `Branch{true, false}` → `brif` on test result
   - `Loop{body, exit}` → `brif` on test result
   - `Return` → `call sash_exit(0); trap`

4. **Block sealing.** Moved to `seal_all_blocks()` at the end after
   all code is emitted. Sealing inline caused assertion failures when
   emit_string_eq created internal blocks that overlapped with SSA
   block boundaries.

5. **For loop unrolling.** For loops with literal word lists are
   unrolled at the SSA level: `for f in a b c` becomes three
   sequential assign+body blocks. No iteration variable, no loop
   counter. Works because word lists are compile-time constants.

**Tests:** 11 — if/else true/false branches, no-else, string
equality/inequality, -n/-z, for loop, bash comparison. All pass.

**Conclusion:** safebash can now compile conditional scripts to
native code. The compiled binary makes branching decisions inline
via string comparison — no shell, no interpreter, no runtime
dispatch. Next: while loops (need SSA Loop terminator in codegen)
and elidable command substitution.

---

## Experiment 025: Elidable command substitution — constant folding

**Date:** 2026-04-13

**Goal:** Compile `$(func)` as a function call returning a string
instead of forking a subprocess. Prove the concept from experiment
018 (subshell elision analysis) works in practice.

**Key changes:**

1. **SsaValue::FuncCall(name).** New SSA value variant. The SSA
   transform detects when a CommandSubstitution contains a single
   call to a known function and emits FuncCall instead of Expr.

2. **SsaValue::Concat(Vec<SsaValue>).** New variant for multi-part
   double-quoted strings like `"got: $x"`. Previously these fell
   through to opaque Expr strings containing literal `$x_1` text.
   Now they're structured: `Concat([Literal("got: "), Var(x_1)])`.

3. **Constant folding.** `fold_function_outputs()` walks function
   SSA blocks. If every instruction is echo with literal args, the
   output is a compile-time constant string. `$(greet)` where
   `greet() { echo "hello" }` → `Literal("hello")`. No function
   call, no buffer, no runtime cost.

4. **Concat in echo.** `lower_echo` handles Concat values by writing
   each part sequentially (no spaces between parts, unlike separate
   echo arguments). This makes `echo "got: $x"` emit two writes:
   `write("got: ")` then `write(x_value)`.

5. **Function blocks skipped.** SSA blocks labeled `fn_*` are not
   emitted into Cranelift — their output was constant-folded into
   the caller. The function body doesn't exist in the native binary.

**Tests:** 9 — cmdsub simple/multi echo, function not reaching
stdout, trailing newline stripping, inline use, concat in echo,
plus regressions (echo, assign, if, for). All pass.

**What's not handled yet:**
- Functions with variable-dependent output (need runtime buffer)
- Concat in non-echo assignment context (needs runtime concat)
- Functions with arguments (`$1`, `$2`)

**Conclusion:** The elidable command substitution concept works.
For the constant-output case, `$(func)` compiles to zero runtime
overhead — the string is a compile-time literal in the data section.
This is the payoff from experiment 018's analysis.

---

