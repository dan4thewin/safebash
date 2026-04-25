# safebash — Experiment Journal

## Prologue

Shell scripts run in critical paths — deployment, provisioning,
system updates — but they cannot participate in a chain of trust.
They cannot be signed, verified, or refused by the kernel when
tampered with. safebash changes that.

This journal documents the incremental journey of building a compiler
for a safe subset of Bash. Rather than attempt a monolithic build of
the full pipeline (parser → validator → IR → SSA → Janet oracle →
Cranelift → signing), we proceed through a series of small,
self-contained experiments. Each experiment produces a working
artifact that demonstrates one concept needed for the compiler.

### How this work is being done

safebash is a collaboration between DG (the human directing the
project) and an AI — specifically, Claude (Anthropic) running as
GitHub Copilot in the terminal. The AI writes the code, the Rust,
and these documents. DG directs the effort: setting goals, reviewing
output, correcting mistakes, and making design decisions.

The workflow is conversational. DG describes what should happen next.
The AI proposes an approach, writes the code, tests it, and produces
a working experiment. DG reviews the result, points out what's wrong
or what was misunderstood, and the AI corrects course.

The prose in this journal and the companion guide is AI-generated.
Direct quotes from DG are attributed. The goal is transparency: a
future reader should know that an AI wrote these words, that a human
directed the work, and that neither could have done it alone in quite
this way.

### Design decisions (established in planning)

- **Rust** implementation — compiler, front-end, Cranelift backend
- **tree-sitter-bash** for parsing
- **SSA IR** between parsing and code generation
- **Janet** as external correctness oracle (S-expression syntax,
  built-in POSIX primitives, lexically scoped, independent
  verification of compiler semantics)
- **Cranelift** for JIT (REPL) and AOT (deployment) compilation
- **Signing + IPE/fs-verity** integration for tamper-evident execution
- **`sash`** as the CLI binary name

---

## Experiment 001: cargo init — proving the Rust toolchain works

**Goal:** Create a Rust binary project named `sash`, compile and test it.

**Actions:**
- `cargo init --name sash` in `exp/001-cargo-init/`
- Added a trivial test (`assert_eq!(2+2, 4)`)
- `cargo build` succeeded, `cargo test` passed, binary prints
  `sash: safebash compiler`

**Results:**
- Rust 1.93.0 (cargo 1.93.0) on ROVE
- Edition 2024 selected automatically
- Build time: ~2s (clean)

**Conclusion:** Toolchain works. Binary name is `sash`.

---

## Experiment 002: tree-sitter dependency — adding the parser

**Goal:** Add `tree-sitter` and `tree-sitter-bash` crate dependencies.
Compile successfully. Verify the Bash grammar loads.

**Actions:**
- Added to Cargo.toml: `tree-sitter = "0.25"`, `tree-sitter-bash = "0.23"`
- Changed main to create a Parser, load the Bash language, print confirmation
- Test: create parser + load language without panic

**Results:**
- 25 crates downloaded, including `cc` (C compiler driver for the
  tree-sitter C code)
- Build time: ~10s (clean, compiling tree-sitter C sources)
- Grammar loads successfully
- tree-sitter 0.25.10, tree-sitter-bash 0.23.3

**Conclusion:** tree-sitter-bash works via FFI. The C dependency is
invisible at the API level — Rust types all the way. The `cc` crate
handles finding the system C compiler automatically.

---

## Experiment 003: parse hello — first successful parse

**Goal:** Parse `echo "hello world"` and inspect the resulting tree.

**Actions:**
- Called `parser.parse(source, None)` on a one-line script
- Printed root node kind and children

**Results:**
```
root: program [(0, 0)-(0, 18)]
  command: "echo \"hello world\""
```
- Root is always `program`
- `echo "hello world"` parses as a single `command` node
- The CST is concrete — it preserves quotes, whitespace positions

**Conclusion:** The parse API is straightforward. `parse()` returns a
`Tree` with a `root_node()`. Nodes have `.kind()` (string), `.child()`
accessors, and `.utf8_text()` to recover source text. The tree is
read-only and navigated via cursors or child iterators.

---

## Experiment 004: walk CST — understanding tree structure

**Goal:** Write a recursive tree walker. Parse a script with assignment,
echo, and if/then. Print every node with indentation to reveal the
full tree structure.

**Actions:**
- Wrote `print_tree()` — recursive, prints kind + text for leaf nodes
- Parsed a script with: shebang, assignment, echo with expansion,
  if/test/-f/then/echo/fi

**Results (abridged):**
```
program
  comment "#!/bin/bash"
  variable_assignment
    variable_name "name"
    = "="
    string
      string_content "world"
  command
    command_name
      word "echo"
    string
      string_content "hello "
      simple_expansion
        variable_name "name"
  if_statement
    test_command
      unary_expression
        test_operator "-f"
        string
          simple_expansion
            variable_name "name"
    command
      command_name
        word "echo"
      string
        string_content "found"
```

**Key discoveries:**
- Variable expansions (`$name`) appear as `simple_expansion` nodes
  inside `string` nodes. This is where SC2086 (unquoted expansion)
  detection will happen — checking whether `simple_expansion` appears
  inside a `string` (quoted) or bare (unquoted).
- `command_name` wraps the command word. Arguments are sibling nodes.
- `test_command` wraps `[ ... ]`. The test operator (`-f`) and its
  argument are children of `unary_expression`.
- Punctuation tokens (`=`, `"`, `[`, `]`, `;`, `if`, `then`, `fi`)
  are all explicit CST nodes.

**Conclusion:** The CST is detailed enough for validation. Every
construct we need to check is visible as a named node type.

---

## Experiment 005: parse script — real-world construct coverage

**Goal:** Parse a script resembling a real deployment tool: shebang,
`set -euo pipefail`, `readonly`, functions, `trap`, `for` loop with
glob, pipeline. Verify tree-sitter handles all of these without errors.

**Actions:**
- Wrote a 25-line script with: set, readonly, function definition,
  trap, mkdir, mount, for/in/do/done, cp, command substitution
- Tests verify: `function_definition`, `for_statement`, `pipeline`,
  `variable_assignment`, `if_statement` all appear in the CST
- Test: `complex_script_has_no_errors()` — tree-sitter reports no
  parse errors for a shellcheck-clean script

**Results:**
- All 6 tests pass
- tree-sitter-bash handles: shebang, set, readonly, functions,
  command substitution `$(...)`, trap, for/glob, pipelines, if/then/fi
- `has_error()` correctly returns false for clean scripts

**Conclusion:** tree-sitter-bash parses everything we need. Phase 1
is complete. The parser handles the full range of constructs that
appear in production deployment scripts. Ready to move to safe subset
validation in phase 2.

---

## Design note: adversarial review (2026-03-28)

*Not an experiment — a future practice decision.*

DG tested adversarial LLM code review and test auditing on a more
mature codebase. The results were valuable. Two reasons to adopt
this practice in safebash once there is enough code to review:

1. **Credibility.** An AI-coded security tool will be dismissed
   out-of-hand unless it demonstrates rigorous independent review.
   Janet as the correctness oracle validates semantics, but the
   implementation itself (Rust code quality, unsafe patterns, logic
   errors in the validator) needs separate scrutiny.

2. **Blind spots.** The LLM that writes the code and the LLM that
   reviews it share training data and reasoning patterns, but a
   review pass with adversarial framing ("find bugs, find ways to
   bypass the safe subset") surfaces issues that a constructive pass
   misses. DG observed this empirically.

**When to introduce it:** after the validator framework exists
(experiment 011) and there are real accept/reject decisions to
audit. Before that, there's nothing meaningful to attack. The
right trigger is phase 3 (IR generation) — by then the compiler
has enough moving parts that an adversarial pass can find real
issues rather than style nits.

**What it is not:** a replacement for Janet. Janet validates that the
compiler's output is semantically correct. Adversarial review
validates that the compiler's implementation is sound. Different
failure modes, both needed.

---

## Experiment 006: ShellCheck survey — defining the safe subset

**Goal:** Catalog ShellCheck's ~520 SC codes. Classify each by
relevance to the safebash safe subset. Produce `exp/SUBSET.md` as
the specification document for phase 2 validation.

**Actions:**
- Cloned the ShellCheck wiki (520 SC code pages) and extracted
  every code with its description
- Ran ShellCheck (0.10.0, with `--enable=all`) against synthetic
  scripts covering eval, quoting, backticks, test constructs,
  subshell scope, arrays, arithmetic, and safety patterns
- Classified codes into 9 categories, with each rule marked as
  reject (error), coerce (auto-fix), or inform (warning)

**Results:**
- 520 SC codes total: 131 SC1xxx (parse), 324 SC2xxx (code),
  65 SC3xxx (portability)
- SC1xxx and SC3xxx are out of scope (tree-sitter handles parse
  errors; portability is irrelevant for Bash 4.0+ target)
- The safe subset draws from ~50 SC2xxx codes across 9 categories:
  quoting, eval/dynamic execution, deprecated syntax, test
  constructs, subshell scope, safety/robustness, arrays, command
  safety, and arithmetic

**Key design decisions:**
- `eval` is rejected unconditionally — no coerce mode. If eval is
  allowed, the entire safe subset collapses.
- Indirect variable access (`${!var}`, `declare -n`) is also
  rejected — same reasoning as eval.
- Variable-as-command-name (`$cmd arg`) is rejected — command names
  must be literal words for static analysis.
- `[ ]` / `test` are coerced to `[[ ]]` — this eliminates an entire
  class of quoting bugs inside conditionals.
- `cd` without `|| exit` is coerced — the canonical "delete
  everything" bug.

**Conclusion:** `exp/SUBSET.md` is the specification. Phase 2
experiments (007–011) implement these rules against the CST.

---

## Experiment 007: Reject eval — first validator rule

**Goal:** Implement a CST validator that rejects constructs enabling
dynamic code execution: `eval`, `source`/`.`, `${!var}`, `declare -n`,
and variable-as-command-name (`$cmd args`).

**Actions:**
- Explored tree-sitter-bash CST output for each target construct to
  determine node kinds and structure (using 005's parser as a probe)
- Implemented `validate()` — a recursive CST walker that emits
  `Diagnostic` structs with line/col, error code, and message
- Defined five safebash diagnostic codes:
  - SB001: variable as command name (command_name contains expansion)
  - SB002: eval
  - SB003: source / dot-source
  - SB004: declare -n / local -n (nameref)
  - SB005: ${!var} indirect expansion
- Wrote 20 tests covering rejection of each construct (including
  nested contexts: inside functions, inside if), acceptance of safe
  constructs (normal commands, `$(cmd)`, arithmetic, `declare -i`,
  `declare -r`), multi-violation reporting, and line/column accuracy

**Key observations:**
- tree-sitter-bash parses `eval` as a regular `command` node with
  command_name "eval" — no special node kind. Detection is by
  matching the word text.
- `${!var}` indirect expansion appears as an `expansion` node with
  a `!` child node. This also catches `${!prefix*}` enumeration.
- `declare -n` uses `declaration_command` node kind, distinct from
  `command`. The `-n` flag appears as a `word` child.
- Variable-as-command: when `$cmd` is the command name, the
  `command_name` node's first child is `simple_expansion` instead
  of `word`.

**Conclusion:** First validator rule works. The `Diagnostic` struct
and `validate()` walker establish the pattern for all subsequent
rules (008–011).

---

## Design review: source, indirect expansion, namerefs — 2026-04-11

DG reviewed the design decisions AI had made unilaterally in
experiments 006–007. Three decisions were resolved through
discussion. Recorded here as durable design rationale.

### Decision 1: source / . — compiled away, not rejected

**Problem:** Scripts need to acquire key-value configuration from
the local host at runtime (e.g., compile on build box, deploy to
node, node has a config file the script sources). The sourced file
is not available at compile time.

**Resolution:** The script author writes idiomatic `. ./vars`. The
compiler replaces it with a safe runtime mechanism:

1. Fork a Rust helper (`sash-expand`) on the file path.
2. The helper validates every line is `KEY=VALUE`, comment, or
   blank. Dies on the first non-conforming line, printing the
   offending line and line number.
3. Values may use the safe subset of variable operations —
   the same operations safebash allows in compiled code:
   - `$VAR` / `${VAR}` — simple expansion
   - `${VAR:-default}` — default value
   - `${VAR:+alternate}` — alternate value
   - `${VAR%%pat}` / `${VAR%pat}` — suffix stripping
   - `${VAR##pat}` / `${VAR#pat}` — prefix stripping
   The helper evaluates these against its inherited environment
   and any variables set by preceding lines in the same file.
   Dies on any disallowed form (`$(cmd)`, backticks,
   `${!indirect}`, arithmetic, etc.).
4. The helper outputs a trivial format: `KEY=value-to-newline`.
   No quoting, no escaping. Values cannot contain newlines.
5. The compiled binary reads the helper's stdout, sets the
   variables. If the helper exits nonzero, the binary dies too.

**Security argument:** The helper is a small, auditable Rust binary.
The shell is not involved in parsing the config file. If someone
puts code (functions, loops, commands) in the sourced file, the
helper dies loudly at runtime. The trivial output format means even
a buggy helper can't inject commands.

**Future:** Static linking of safebash modules for sourcing code
(functions, libraries). That's a separate feature — the kvp loader
handles the config use case.

### Decision 2: ${!var} indirect expansion — rejected

**Problem:** `${!var}` accesses a variable whose name is determined
by the runtime value of another variable. The compiler cannot know
which variable is being accessed without tracing every assignment
to the name variable.

**Resolution:** Reject. The indirection is opaque at compile time.
The variable name is a string that could come from user input, file
contents, or string concatenation. SSA analysis (phase 4) cannot
resolve the referent in the general case.

May revisit after SSA if a narrow safe case emerges (e.g., the name
variable is provably constant), but the default is reject.

### Decision 3: declare -n / local -n (namerefs) — allowed

**Problem:** Namerefs were initially rejected alongside `${!var}`
as another form of indirection. DG uses `local -n` extensively in
shell functions and considers it safe.

**Resolution:** Allow. Namerefs are fundamentally different from
`${!var}`:

- **`${!var}`**: The variable name is a *string value* at runtime.
  Any string could be in there. The binding is invisible.
- **`local -n ref=target`**: The referent is declared at the point
  of creation. The compiler sees exactly which variable `ref`
  points to. Even `local -n ref=$1` is analyzable at the call
  site — the compiler knows what argument was passed.

Namerefs provide the same indirection capability as `${!var}` but
in a structured form the compiler can reason about. The binding is
explicit, visible in source, and traceable through function call
graphs. It's equivalent to passing a pointer in C — the type system
can track it.

Rejecting namerefs while allowing no alternative would push users
toward eval, which is worse.

**Impact on experiment 007:** Remove `declare -n` / `local -n` from
the SB004 rejection rule. SB004 is retired or reassigned.

### Decision 4: $cmd variable-as-command — two-pass resolution

**Problem:** Rejecting all variable-as-command blocks legitimate
patterns like `readonly EDITOR=vi; "$EDITOR" file` and conditional
tool selection (`AWK=gawk` / `AWK=awk`). But allowing all of them
opens the door to runtime-constructed command names from arbitrary
data.

**Resolution:** Two-pass approach.

**Phase 2 (CST walking):** When `$cmd args` is encountered, look
backward in the same scope for an assignment to `cmd`. If the
assignment is `readonly cmd=literal` or `declare -r cmd=literal`
with no intervening reassignment visible in the CST, allow it.
Otherwise, emit a *deferred* marker — not an error yet, but a
signal that SSA must resolve this.

**Phase 4 (SSA):** Deferred dispatch variables get full data-flow
analysis. If SSA can prove the value set is bounded to literal
strings (e.g., both branches of an if/else assign literal values),
allow it. If SSA cannot prove it safe, hard error.

**Design consequence:** The validator framework (experiment 011)
needs the concept of *deferred judgments* — diagnostics that are
not errors at the CST level but become errors if SSA can't resolve
them. This is a three-state system: allow, reject, defer.

**Impact on SB001:** Changes from "reject all variable-as-command"
to "allow if provably literal at CST level, defer to SSA otherwise,
hard error if SSA can't resolve."

### Decision 5: Diagnostic code scheme — reuse ShellCheck, SB4xxx for new

**Problem:** The AI invented SB001–SB005 without consulting DG.
Need a principled code scheme.

**Resolution:** Reuse exact ShellCheck SC codes where safebash
enforces the same rule. Introduce SB codes only for safebash-
specific restrictions that ShellCheck doesn't cover.

**ShellCheck reuse:** When safebash rejects unquoted expansions,
it emits SC2086 — the same code ShellCheck uses. Users already
know these codes. No reason to invent new ones.

**SB4xxx for safebash-specific rules:** ShellCheck's scheme uses
1xxx (parse), 2xxx (code), 3xxx (portability). SB4xxx is a new
category: safe subset violations. It's clearly distinct from SC
codes and will never collide with future ShellCheck additions.

Current SB4xxx assignments:
- SB4001: eval forbidden
- SB4002: source/. compiled away (marker, not error)
- SB4003: ${!var} indirect expansion rejected
- SB4004: $cmd variable-as-command (deferred to SSA if not
  provably readonly+literal at CST level)

**Stability:** All codes (SC and SB) are permanent once assigned.
Users may reference them in ignore directives.

### Decision 6: source and . treated identically

In bash, `source` and `.` are synonyms. The only difference is
portability (`.` is POSIX, `source` is a bashism). Since safebash
targets bash syntax, the distinction is irrelevant. Both are
accepted and both compile to the same safe kvp loader mechanism.

### Decision 7: No minimum bash version — accept whatever tree-sitter-bash parses

The compiler doesn't invoke bash on the target — it compiles to a
native binary. The relevant question is not "what bash version runs
on the target" but "what syntax can the compiler accept." The
answer: whatever tree-sitter-bash can parse. No artificial version
floor.

### Decision 8: AOT strict by default, JIT coerce by default

**AOT (compile to signed binary):** Strict by default. Every
coercible issue is an error. Error messages include the flag to
enable coerce mode: `sash: SC2086: unquoted expansion at line 12
(use --coerce to auto-fix)`. The user sees every issue, fixes
deliberately, ships clean code.

**JIT (run interactively):** Coerce by default. The script runs.
Coercions are applied silently but reported as warnings so the
user knows what was fixed. Does not block execution.

**Workflow:** Develop with JIT (coerce), compile with AOT (strict)
for release. AOT errors show exactly what the coercer was silently
fixing during development.

Reject-only rules (eval, ${!var}, unresolvable $cmd) are always
hard errors in both modes. Coerce only applies to rules that have
a mechanical, semantics-preserving transformation.

### Decision 9: cd without error handling — coerce rule

**Problem:** `cd dir; rm -rf *` is the canonical "delete everything"
bug if `cd` fails. But inserting `|| exit 1` changes the program's
control flow — the author might have intentionally continued after
a failed cd.

**Resolution:** Coerce rule, not reject.

- **JIT (coerce by default):** Add `|| exit 1` and warn. The
  script doesn't silently continue in the wrong directory.
- **AOT strict (default):** Error: "cd without error handling at
  line N." The author chooses the fix: `|| exit`, `|| return`,
  or `|| { handle_error; }`. The compiler identifies the problem
  but doesn't choose the recovery strategy.

This reuses SC2164, not a new SB code.

---

## Experiment 008: Unquoted expansion detection (SC2086, SC2046)

**Goal:** Detect unquoted parameter expansions and command
substitutions — the single most common source of Bash bugs.

**Actions:**
- Used a CST dumper to study tree-sitter-bash's representation of
  quoted vs unquoted expansions across all relevant contexts
- Key insight: an expansion node (`simple_expansion`, `expansion`,
  `command_substitution`) is "unquoted" when it is NOT inside a
  `string` node (double quotes) and NOT in a safe context
- Implemented `is_quoted_or_safe()` which walks the ancestor chain
  to classify context

**Safe contexts identified (no word splitting occurs):**
- Inside `string` node (double quotes)
- RHS of `variable_assignment` (x=$var — no splitting on RHS of =)
- Inside `test_command` with `[[` opener (but NOT `[`)
- Inside `arithmetic_expansion` $(( ))
- Inside `heredoc_body`
- `case_statement` discriminant (the [value] field)

**Unsafe despite being inside assignment:**
- Inside `array` node: `arr=($var)` — splitting occurs inside ( )

**Diagnostic codes:**
- SC2086 for unquoted `$var`, `${var}`, `$1`, `$@`, etc.
- SC2046 for unquoted `$(cmd)` command substitution

**Tests:** 29 tests covering:
- 12 unquoted patterns that should flag (simple, braced, positional,
  special, command sub, default, args, for-value, concat, array, [ ])
- 8 quoted patterns that should pass (same set, double-quoted)
- 7 safe-context patterns (assignment, local, export, [[ ]], $(()),
  heredoc, case)
- Multi-violation and position accuracy tests
- Complex safe script integration test

**Conclusion:** The ancestor-chain walk approach is clean and
handles all contexts correctly. The same pattern will work for
other context-dependent rules (e.g., [ ] vs [[ ]] detection in 010).

---

## Experiment 009: Reject backtick substitution (SC2006)

**Goal:** Detect legacy backtick command substitution and flag it
for conversion to the modern $(...) form.

**Actions:**
- Probed tree-sitter-bash CST to confirm both backtick and $()
  parse as `command_substitution` nodes, distinguished by first
  child: `` ` `` for backticks, `$(` for modern form
- Implemented validator: walk the CST, find `command_substitution`
  nodes whose first child is `` ` ``, emit SC2006

**Simplest experiment so far:** The detection is a single condition
on the first child of `command_substitution`. No context analysis
needed (unlike SC2086 which requires ancestor-chain walking).
Backticks are always wrong regardless of context.

**Tests:** 14 tests covering:
- 7 backtick patterns (assignment, arg, double-quoted, if condition,
  variable, multiple, nested inside modern $())
- 5 modern $() patterns that should pass
- Position accuracy test
- Complex safe script integration test

**Conclusion:** SC2006 detection works. In coerce mode (future),
the transformation is mechanical: replace `` ` `` delimiters with
`$(` and `)`, remove backslash escaping of inner backticks.

---

## Experiment 010: Reject single-bracket test / test builtin (SC2292)

**Goal:** Detect `[ ]` and the `test` builtin, flag for conversion
to `[[ ]]`.

**Actions:**
- Probed tree-sitter-bash CST: `[ ]` and `[[ ]]` both parse as
  `test_command` nodes, distinguished by first child (`[` vs `[[`).
  The `test` builtin parses as a regular `command` node with
  command_name "test".
- Implemented two checks: `check_test_command` (first child is `[`)
  and `check_test_builtin` (command name is "test")
- Both emit SC2292 with distinct messages

**Tests:** 21 tests covering:
- 6 single-bracket patterns (unary, binary, -a compound, in if,
  while, elif)
- 4 test-builtin patterns (unary, binary, in if, message check)
- 5 double-bracket patterns that should pass (unary, binary, &&
  compound, in if, regex)
- 2 non-test commands that should pass
- Multi-violation, position accuracy, complex safe script

**Conclusion:** Same pattern as 009 — detection by node kind and
first-child inspection. Simple and reliable.

---

## Experiment 011: Validator framework

**Goal:** Unify the individual rule implementations from experiments
007–010 into a composable validation framework. Single CST walk,
multiple rules, collected diagnostics.

**Architecture:**

```
src/
  lib.rs            — parse(), module declarations
  diagnostic.rs     — Diagnostic struct, Severity enum, constructors
  rule.rs           — Rule trait (name, node_kinds, check)
  validator.rs      — validate() — single walk, dispatch to rules
  rules/
    mod.rs          — all_rules() registry
    dynamic_execution.rs   — SB4001 eval, SB4002 source, SB4004 $cmd
    indirect_expansion.rs  — SB4003 ${!var}
    unquoted_expansion.rs  — SC2086/SC2046
    backtick_substitution.rs — SC2006
    single_bracket_test.rs — SC2292
  main.rs           — CLI entry point
tests/
  integration.rs    — 37 cross-rule integration tests
```

**Key design decisions:**

- **Rule trait:** `name()`, `node_kinds()`, `check()`. Rules are
  stateless. The framework calls `check()` only for nodes matching
  the rule's declared `node_kinds()`. Empty `node_kinds()` means
  receive all nodes.

- **Single walk:** The framework walks the CST once and dispatches
  each node to all rules whose `node_kinds()` match. This is O(n)
  in CST size regardless of rule count.

- **Severity enum expanded:** Error, Coerce, Deferred, Marker.
  The Coerce level distinguishes rules that are errors in AOT/strict
  but auto-fixable in JIT/coerce.

- **Diagnostic constructors:** `Diagnostic::error()`, `::coerce()`,
  `::deferred()`, `::marker()` — convenience methods that extract
  line/col from the node.

- **Rule interaction bug found and fixed:** `$cmd arg` was flagged
  by both SB4004 (variable-as-command, correct) and SC2086
  (unquoted expansion, incorrect — command-name position doesn't
  undergo word splitting). Fixed by adding `command_name` to the
  safe-context list in `is_quoted_or_safe()`.

- **Output sorted by position:** Diagnostics are sorted by
  (line, col) regardless of rule firing order.

**Tests:** 37 integration tests covering all 5 rules individually,
cross-rule interaction (all rules fire together), diagnostic sorting,
rule registration, selective rule loading, safe script integration,
and position accuracy.

**Conclusion:** Phase 2 is complete. The framework makes adding a
new rule a matter of: write a struct implementing `Rule`, add it to
`all_rules()`, write tests. No changes to the walker or framework.

---

## Experiment 012: IR type definitions

**Goal:** Define Rust types for the safebash intermediate
representation — the layer between tree-sitter CST and SSA.

**Actions:**
- Probed tree-sitter-bash CST for all construct types: commands,
  pipelines, assignments, declarations, if/elif/else, while/until,
  for, case, functions, redirections, lists (&&/||), subshells,
  groups, and all expression types
- Designed a type hierarchy in `src/ir.rs` covering:

**Statement types (StmtKind):**
  Assignment, Declaration, Command, Pipeline, If, While, For,
  Case, Function, Redirected, List, Subshell, Group, Comment

**Expression types (ExprKind):**
  Literal, DoubleQuoted, SingleQuoted, SimpleExpansion,
  ParameterExpansion, CommandSubstitution, ArithmeticExpansion,
  Concat, Array, Test

**Supporting types:**
  Span (source location), DeclKeyword (declare/local/export/readonly),
  RedirectOp (8 variants), ListOp (And/Or), TestExpr (unary/binary/
  not/and/or/paren), ParameterExpansion (name + operator + operand),
  ElifClause, CaseItem (with fallthrough flag)

**Design principles applied:**
- Every CST construct that survives the safe-subset validator has
  a corresponding IR node
- IR is a tree (SSA adds graph edges in phase 4)
- String data is owned — IR is independent of source text
- Spans preserved for error reporting
- Comments preserved for round-trip fidelity

**Tests:** 32 tests verifying every IR node type can be constructed,
all enum variants are distinct, Span::from_node works with real CST
nodes, and a complete Program models a multi-statement script.

**Conclusion:** The IR type system is complete enough to represent
any bash script that passes the safe-subset validator. Ready for
CST→IR lowering in experiments 013–018.

---

## Experiment 013: CST→IR lowering — assignments, declarations, commands

**Goal:** Build the `lower` module that converts tree-sitter CST
nodes into the IR types defined in experiment 012.

**Scope:** This experiment covers three statement types plus the
full expression lowering pipeline:

**Statements lowered:**
- `variable_assignment` → `StmtKind::Assignment`
- `declaration_command` → `StmtKind::Declaration`
  (declare/local/export/readonly with flags and assignments)
- `command` → `StmtKind::Command` (name + arguments)
- `comment` → `StmtKind::Comment`

**Expressions lowered (all RHS/argument positions):**
- `word` → `ExprKind::Literal`
- `string` (double-quoted) → `ExprKind::DoubleQuoted` with parts
- `raw_string` (single-quoted) → `ExprKind::SingleQuoted`
- `simple_expansion` ($var) → `ExprKind::SimpleExpansion`
- `expansion` (${var:-...}) → `ExprKind::ParameterExpansion`
- `command_substitution` → `ExprKind::CommandSubstitution`
- `arithmetic_expansion` → `ExprKind::ArithmeticExpansion`
- `concatenation` → `ExprKind::Concat`
- `array` → `ExprKind::Array`
- `command_name` → unwrap to inner expression
- Subscript expansion (${arr[@]}) handled

**Architecture:**
- `lower_program()` — entry point, walks top-level children
- `lower_statement()` — dispatches by CST node kind
- `lower_expr()` — recursive expression lowering
- `lower_string()` — double-quoted string decomposition
- `lower_parameter_expansion()` — ${...} with operators

**Tests:** 27 integration tests covering every assignment form
(literal, empty, quoted, expansion, param-expansion, cmd-sub,
arithmetic, array, concatenation, interpolated string),
declarations (readonly, local, export, declare with flags, local -n),
commands (simple, multi-arg, expansion arg, no args), comments,
multi-statement programs, span accuracy, and a realistic script.

**Conclusion:** The expression lowering is complete enough to
handle all value positions in assignments and command arguments.
This foundation is reused by experiments 014–018 for pipeline,
if, while, for, and function lowering.

---

## Experiment 014: CST→IR complete lowering

**Goal:** Bundle experiments 014–018 into a single experiment that
lowers all remaining CST constructs to IR: pipeline, list (&&/||),
if/elif/else, while/until, for, case, function, redirected, negated,
subshell, group, and test expressions.

**Actions:**
- Probed tree-sitter CST for every remaining construct type
- Extended `lower.rs` from 013's foundation (assignments, declarations,
  commands, expressions) to handle all 14 statement types
- Extracted `lower_body()` as shared helper for compound bodies
  (program, function, do_group, compound_statement, else_clause)
- Added test_command lowering → `TestExpr` (unary, binary)

**Bug found and fixed:** Negated pipelines. Tree-sitter puts `!`
inside the pipeline as a `negated_command` wrapping the first element,
not outside the pipeline. Initial code treated `!` as always wrapping
the entire pipeline — this missed cases like `! echo a | grep b`
where `negated_command` is a child of `pipeline`. Fixed by detecting
`negated_command` inside `lower_pipeline()` and setting the negated
flag there.

**Tests:** 33 integration tests covering:
- Pipeline (2, 3 commands, negated command, negated pipeline)
- List (&& and ||)
- If (simple, else, elif+else, condition-is-test)
- While and Until loops
- For (single and multiple words)
- Case (multi-item, wildcard pattern)
- Function (simple, with local declaration)
- Redirected (write, append, read, fd, multiple)
- Subshell and Group
- Test expressions (unary and binary in conditions)
- Carry-forward (assignment, expansion, declaration, command)
- Full script integration and statement count verification

**Conclusion:** CST→IR lowering is complete. Every bash construct
that passes the safe-subset validator can now be represented in
the IR. Experiments 015–018 from the original roadmap are absorbed
into this single experiment. Phase 3 is effectively one experiment
away from completion (019: IR printer).

---

## Experiment 015: IR printer

**Goal:** Pretty-print the safebash IR as indented, readable text
for visual verification of CST→IR lowering.

**Design:**
- `print_program()` → `String`: deterministic, readable, diffable
- 2-space indentation for nesting (if/while/for/case/function bodies)
- Expressions printed inline: `"hello $var"`, `${var:-default}`,
  `$((n + 1))`, `$(date)`, `('a' 'b')`, `[[ -f $x ]]`
- Redirections appended inline: `echo hello > out.txt`
- Pipelines: commands separated by `|` on separate lines
- Lists: `&&` / `||` operators on separate lines between operands
- Command substitution bodies printed inline with `;` separators
- Comments preserved verbatim (including shebang)

**Tests:** 41 tests covering:
- Every assignment form (literal, empty, quoted, single-quoted,
  expansion, param-expansion, cmd-sub, arithmetic, array, concat)
- Declarations (readonly, local -n, export)
- Commands (simple, quoted arg, no args)
- Pipeline and negated pipeline
- List (&& and ||)
- If/elif/else
- While and Until
- For
- Case with multiple items
- Function definition
- Redirected (write, append, fd)
- Subshell and Group
- Comment and shebang
- Expressions in various positions (param expansion, array subscript,
  command substitution in string)
- Full script integration roundtrip
- Determinism (same input → same output)
- Nested indentation depth verification

**Conclusion:** Phase 3 (IR construction) is complete. The pipeline
is: source → tree-sitter CST → IR → printed text. Every construct
that passes the safe-subset validator can be lowered and printed.
Ready for Phase 4 (SSA).

---

## Experiment 016: SSA transformation

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

## Experiment 017: Janet oracle

**Goal:** Bundle experiments 025–030 into one: emit Janet code from
the IR, run it as a subprocess, compare output against Bash.

**Architecture:**
```
src/
  emit_janet.rs  — IR → Janet source code emitter
  oracle.rs      — Janet/Bash subprocess runner
```

**Mapping (Bash → Janet):**
- `x=value` → `(var x "value")` / `(set x "value")`
- `readonly X=v` → `(def X "v")`
- `echo $x` → `(print x)`
- `echo "a $b"` → `(print (string "a " b))`
- `cmd arg` → `(os/execute ["cmd" "arg"] :p)`
- `cmd1 | cmd2` → `os/spawn` pipe chain with `:out :pipe`
- `if [[ cond ]]; then` → `(if cond (do ...) (do ...))`
- `for x in a b; do` → `(each x ["a" "b"] ...)`
- `func() { body }` → `(defn func [] body)` + call via `(func)`
- `[[ -f x ]]` → `(= (os/stat x :mode) :file)`
- `[[ a = b ]]` → `(= a b)`
- `[[ n -gt 0 ]]` → `(> (scan-number n) (scan-number 0))`
- `$(cmd)` → `os/spawn` with stdout capture
- `$((n+1))` → `(string (+ (scan-number n) 1))`

**Bugs found and fixed:**

1. **Double-quoted string gap detection.** `"$x $y"` — tree-sitter
   includes the inter-variable space inside the second expansion
   node's `$` token (shown as `" $"` not `"$"`). The original
   lower_string only collected named children, missing this implicit
   whitespace. Fixed by detecting leading whitespace in expansion
   tokens within strings and emitting it as a literal part.

2. **Janet variable scoping in if/else.** Janet's `var` creates a
   local binding. Variables first assigned inside an if/else branch
   aren't visible after the `if`. Fixed by pre-scanning branches for
   assigned variables and emitting `(var x nil)` before the `if`.

3. **Pipeline subprocess ordering.** Reading from the last pipe
   before upstream processes finish → `EAGAIN`. Fixed by waiting
   for upstream processes before reading the final output.

4. **Function calls vs external commands.** `greet` after
   `greet() { ... }` was emitted as `os/execute ["greet"]`.
   Fixed by tracking function definitions in EmitCtx and emitting
   Janet function calls for known functions.

**Tests:** 28 oracle tests covering:
- Janet subprocess (runs, exit code)
- Assignment (literal, reassignment, readonly → def)
- Commands (echo literal, multiple args, quoted, variable,
  interpolation, general command)
- If/else (true path, false path, string equality, branch assignment)
- Pipeline (echo|cat, echo|grep)
- For loop
- Function definition and call
- Integration (assign+echo, conditional output)
- Janet code validity (5 scripts)
- Edge cases (empty echo, multiple echo, comment)

**Conclusion:** Phase 5 (Janet oracle) complete. The full pipeline
is now: Bash source → CST → IR → Janet → execute → compare output.
The oracle confirms semantic correctness of the IR transformation
for the tested constructs.

---

## Experiment 018: Subshell elision analysis

**Date:** 2026-04-11

**Goal:** Prove that subshells and command substitutions don't always
need a real process.  Build a static analysis pass that classifies
each `$(...)` and `(...)` as **elidable** (can be compiled as inline
function calls / string buffers) or **required** (must spawn a real
subprocess).

**Insight (DG):** Bash conflates three things in `$(...)`:
1. Capture — getting output as a string.  Needs a pipe, not a process.
2. Isolation — preventing side effects from leaking.  SSA versioning
   already provides this at compile time.
3. Parallelism — concurrent execution.  This genuinely needs processes
   (and is probably outside the safe subset).

For safebash, only (3) truly requires a process.  For (1) and (2),
the compiler can lower `$(my_func)` to a function call returning a
string.  No fork, no pipe, no fd manipulation.

**Architecture:**
```
src/elision.rs  — classify subshells and command substitutions
```

Key types:
- `SubshellClass::Elidable` — no process needed
- `SubshellClass::Required(Vec<ProcessReason>)` — must fork
- `ProcessReason` — ExternalCommand, Pipeline, BackgroundJob, UnknownCommand
- `FunctionMap` — tracks function definitions and their classifications
- `ElisionReport` — per-program analysis results

**Classification rules:**
- Builtin commands (echo, printf, cd, local, etc.) → elidable
- Known function whose body is all-elidable → elidable
- Known function whose body has externals → required
- External/unknown command → required
- Multi-stage pipeline → required
- Nested command substitutions → recursive analysis
- Expressions inside assignments/declarations → checked for cmd subs

**Sample output** (realistic deploy script):
```
Functions:
  get_version  → ELIDABLE (no process needed)
  format_msg   → ELIDABLE (no process needed)
  log          → REQUIRED (external: date)
  get_timestamp → REQUIRED (external: date)

Elidable (3): $(get_version), $(format_msg), (cd /tmp && echo ...)
Required (4): $(date), $(get_timestamp), $(tar ...), $(pipeline)
```

**Tests:** 30 tests covering:
- Command substitutions: echo/printf/true/cd/known-function elidable
- Command substitutions: external/pipeline/unknown/function-calling-external required
- Subshells: builtins elidable, external required, isolation-only elidable
- Function analysis: pure-builtins/external/mixed classification
- Mixed scripts, nested cmd subs, realistic deploy script
- Edge cases: empty cmd sub, cmd sub in array, context strings
- Entry kinds (CommandSubstitution vs Subshell)

**Implications for Cranelift:**
When lowering to native code, elidable `$(func)` becomes a normal
function call returning a string.  No fork/exec/pipe.  This is a
significant optimization and simplification for AOT compilation.

**Conclusion:** Subshell elision analysis works.  The compiler can
now distinguish "needs a process" from "needs a string value" and
choose the appropriate compilation strategy.

---

## Design Decision 10: `bash -c` and Bourne-lineage `-c` handling

**Date:** 2026-04-12

**Context:** The adversarial audit (B2) flagged `bash -c "$payload"` as
a CRITICAL bypass equivalent to eval. Initial remediation plan proposed
rejecting all `bash -c` invocations (SB4005). DG challenged this:
nothing stops evasion via a script with a shebang, and safebash doesn't
police external commands like `rm` or `mount`.

**Decision:** Don't reject. Don't ignore. **Compile it.**

- `bash -c 'literal string'` → parse the string at compile time,
  validate against the safe subset, compile as an inline subshell.
  **Allow.**
- `bash -c "$var"` → string not known at compile time. **Reject**
  (same rationale as eval/SB4001: the compiler can't compile code
  it can't see).
- `bash -c "literal with $expansion"` → reject unless all expansions
  resolve to compile-time constants (SSA phase could prove this).

Applies to Bourne-lineage shells: `sh`, `bash`, `dash`, `ksh`, `zsh`
with `-c`. Non-Bourne interpreters (`python3 -c`, `perl -e`) remain
ordinary external commands — not the compiler's jurisdiction.

**Rationale:** This follows the same principle as `source`/`.`
(decision #1). The compiler doesn't reject the construct — it compiles
it away into something analyzable. The dangerous case (runtime string)
is rejected because it's literally uncompilable, not as a policy choice.

The process isolation that `bash -c` implies (clean environment, no
parent locals/functions) maps to the same subshell elision analysis
from experiment 018 — SSA versioning provides the isolation.

**Implication for B2:** SB4005 changes from "reject bash -c" to
"reject bash -c with non-literal argument; compile literal argument
as inline subshell." This is a refinement, not a reversal.

**Revisit if:** compile-time parsing of the `-c` string proves too
costly or introduces unacceptable complexity in the lowering pipeline.

---

## Design Decision 11: Alias handling

**Date:** 2026-04-12

**Context:** Adversarial audit (B5) found that `alias e=eval; e` could
bypass the validator. Discussion explored whether aliases are "just
syntactic sugar for functions."

**Analysis:** Most aliases (`alias ll='ls -la'`) are trivially
convertible to functions (`ll() { ls -la "$@"; }`). The dangerous
aliases are syntax-fragment macros (`alias begin='{'`) that can't be
functions because they inject partial syntax — and those are exactly
the ones we can't analyze.

**Decision:**
- **AOT strict:** Reject `alias` and `unalias` entirely. Error message
  includes the converted function form so the author knows exactly what
  to write instead.
- **JIT coerce:** Convert simple aliases to functions automatically.
  Parse the alias expansion text as a command; if it's a valid complete
  command, emit `name() { expansion "$@"; }`. If it's a syntax fragment,
  reject even in JIT — there's no safe conversion. Emit warning either
  way.

**Rationale:** Every practical alias has a function equivalent. Functions
are parseable, analyzable, and have proper scope. Aliases are text
macros that operate before parsing — the compiler can't reason about
them. Rejecting aliases in AOT loses nothing; converting in JIT
preserves daily-driver usability.

**Codes:** SB4008 (alias rejected / converted).

---

## Experiment 019: Critical bypass fixes (B1–B4)

**Date:** 2026-04-12

**Goal:** Close the four CRITICAL vulnerabilities identified by the
adversarial audit (run-1).

**Actions:**

Rewrote `dynamic_execution.rs` from a simple command-name matcher to a
multi-layer command analyzer:

1. **B1: Wrapper unwrapping** — `builtin eval`, `command eval` now
   detected. The rule extracts the effective command by looking past
   `builtin`/`command` wrappers. Same treatment for `source`/`.`
   through wrappers.

2. **B2: Bourne-lineage `-c`** — `bash -c`, `sh -c`, `dash -c`,
   `ksh -c`, `zsh -c` detected. Literal string argument → Marker
   (will be compiled as inline subshell per decision #10). Variable
   argument → Error (compiler can't see the code). Works through
   `command` and `env` wrappers.

3. **B3: `trap` code strings** — `trap 'literal code' SIG` → Marker
   (will be compiled inline, same principle as B2). `trap "$var" SIG`
   → Error. `trap funcname SIG` → allowed (function reference).
   `trap -` and `trap -p` → allowed.

4. **B4: ANSI-C obfuscation** — `$'\x65\x76\x61\x6c'` (decodes to
   `eval`) now caught. Added `decode_ansi_c_string()` handling hex,
   octal, unicode, and named escapes. Applied to all command-name
   extraction paths.

5. **B8: `env` wrapper** — `env [-i] [-u X] [K=V]... cmd` now
   resolved. Skips env flags and assignments, finds the effective
   command, applies all checks. `env FOO=1 bash -c "$cmd"` caught.

**Architecture:**

- `extract_command_word()` — resolves word or ANSI-C string to literal,
  returns None for expansions
- `collect_args()` — gathers argument nodes after command name
- `check_command()` — dispatches to eval/source/shell-c/trap checks
- `check_wrapper()` — unwraps builtin/command, recurses to check_command
- `check_env()` — unwraps env flags/assignments, recurses to check_command
- `check_shell_c()` — detects -c flag, classifies literal vs variable
- `check_trap()` — classifies function ref / literal / variable
- `is_literal_string()` — checks raw_string, ansi_c_string, or
  expansion-free double-quoted string
- `decode_ansi_c_string()` — full ANSI-C escape decoder

**New diagnostic codes:**
- SB4005: Bourne-lineage shell with -c (error if variable, marker if literal)
- SB4006: trap with code string (error if variable, marker if literal)

**Refinement of SB4001:** `eval` with all-literal arguments now emits
Marker instead of Error (will be compiled inline, recursive parse
pending). `eval` with variable arguments remains Error.

**Tests:** 37 integration tests covering all four bypasses, wrapper
combinations, env resolution, regression on original rules, complex
script with all bypasses, and safe script with no false positives.

**Bug found:** `check_trap` was calling `utf8_text(&[])` (empty source
slice) instead of passing the actual source bytes, causing a panic on
any trap with a bare word argument. Fixed by threading `source`
parameter through.

---

## Experiment 020: HIGH gap fixes (B5, B8, coproc)

**Date:** 2026-04-12

**Goal:** Close remaining HIGH findings from adversarial audit run-1.

**Actions:**

1. **B5: Alias restriction (Decision #11)** — New rule `alias_restriction.rs`
   with `AliasRestriction` struct. Handles three commands:
   - `alias name='expansion'` — tree-sitter parses as a `concatenation`
     argument (word "name=" + raw_string "'expansion'"). Dangerous
     expansions (eval, source, bash, sh, etc.) → Error. Safe expansions
     → Coerce with function conversion in the message.
   - `unalias` → Coerce (implies aliases exist).
   - `shopt -s expand_aliases` → Error SB4009 (enables text-macro
     expansion that defeats static analysis).

2. **Coproc rejection** — `UnsupportedConstructs` rule rejects `coproc`
   with SB4013 (concurrency is out of safe subset permanently).

3. **Dropped B6 (exec)** — `exec` with a command is just running a
   program; all existing rules already apply to the command being exec'd.
   Nothing to check that isn't already covered.

4. **Dismissed B7 (xargs)** — Out of scope. safebash doesn't analyze
   external program arguments. To be documented in SUBSET.md.

**Tree-sitter discovery:** `alias ll='ls -la'` parses as:
```
command
  command_name [name]: word "alias"
  concatenation [argument]: word "ll=" + raw_string "'ls -la'"
```
Not inside `command_name` as initially assumed. The assignment is a
regular command argument, which required rewriting the check logic.

**New diagnostic codes:**
- SB4008: alias rejected (Error for dangerous, Coerce for safe)
- SB4009: shopt -s expand_aliases rejected (Error)
- SB4012: select rejected (not yet implemented — needs node kind check)
- SB4013: coproc rejected (Error)

**Tests:** 21 integration tests covering alias classification,
dangerous vs safe alias detection, unalias, shopt, coproc, regression
on all 019 rules, complex script integration, and false positive check.

---

## Experiment 021: Cranelift hello world

**Date:** 2026-04-12

**Goal:** Prove we can generate a native binary from Rust using
Cranelift's code generation API.

**Actions:**

Built a program that:
1. Creates a Cranelift IR function (`main`) that calls libc `write(1,
   buf, 12)` to print "hello world\n" then calls `_exit(0)`.
2. Emits a `.o` object file via `cranelift-object`.
3. Links it with `cc -no-pie` to produce a native ELF binary.
4. Runs the binary, asserts stdout = "hello world\n" and exit 0.

**Issues encountered:**

- `TrapCode::user(0)` returns `None` — user trap codes must be
  non-zero. Fixed to `user(1)`.
- `InstBuilder` trait not in scope — Cranelift 0.130 requires
  explicit `use cranelift_codegen::ir::InstBuilder`.
- Initial attempt used `-nostartfiles -static` linking, which fails
  on Ubuntu 24.04 due to missing `_init`/`_fini`/`_DYNAMIC` symbols.
  Switched to `cc -no-pie` using standard libc startup (defines
  `main` instead of `_start`).

**Result:** Native ELF x86-64 binary generated and executed
successfully. Two tests pass: IR verification and end-to-end binary
execution.

**Cranelift version:** 0.130.1

**Key dependencies:**
- `cranelift-codegen` — IR types, code generation
- `cranelift-frontend` — `FunctionBuilder` API for constructing IR
- `cranelift-module` — module-level linking (data + functions)
- `cranelift-native` — host ISA detection
- `cranelift-object` — emit to object files

**Conclusion:** Cranelift works. Next experiment: lower SSA
assignments to Cranelift IR (string variables as heap values,
assignment as store, expansion as load).

---

## Experiment 022: Cranelift string variables and echo

**Date:** 2026-04-12

**Goal:** Compile bash variable assignments and echo to native code
via Cranelift. Full pipeline: bash → parse → IR → SSA → Cranelift
→ object → link → run.

**Actions:**

1. Built `codegen.rs` — lowers SSA to Cranelift IR:
   - String variables are (ptr, len) pairs. Literals live in the
     data section. Assignment copies the pair. Variable reads use
     `use_var` to get the current (ptr, len).
   - Echo calls `write(1, ptr, len)` for each argument with spaces
     between and a trailing newline.
   - Program exits via `_exit(0)`.

2. `compile_to_object()` — convenience function that runs the full
   pipeline: parse → lower → SSA → Cranelift → object bytes.

3. Fixed SSA transform: `DoubleQuoted([Literal("hello")])` was
   being flattened to `Expr("\"hello\"")` instead of unwrapping to
   `Literal("hello")`. Same for `DoubleQuoted([SimpleExpansion("x")])`
   → should be `Var(x_1)`. Single-element DoubleQuoted now unwraps;
   all-literal multi-part concatenates.

4. Fixed test race condition: all tests were writing to the same
   `/tmp/sash_test` path. Now each test gets a unique path via
   atomic counter.

**Cranelift API discovery:**
- Cranelift 0.130: `declare_var(Type) -> Variable` (no more
  `Variable::new(index)`). The builder allocates variable indices.
- `InstBuilder` trait must be explicitly imported.
- `TrapCode::user(0)` returns `None` — user codes must be non-zero.

**Tests:** 10 end-to-end tests:
- echo_literal, echo_quoted_literal, echo_multiple_literals, echo_no_args
- assign_and_echo, assign_two_vars_echo, reassign_variable
- assign_empty_string, multiple_echo_statements
- compile_produces_nonempty_object

**Conclusion:** The pipeline works end-to-end: bash source →
native binary → correct output. Variables are heap strings, echo
calls write(). Next: fork/exec for external commands (exp 023).

---

## Experiment 023: External command execution via fork/exec

**Date:** 2026-04-12

**Goal:** Compile non-builtin commands (`date`, `uname`, `ls`, etc.)
to native code that forks and execs the external program.

**Actions:**

1. Wrote `sash_rt.c` — a small C runtime that provides `sash_exec()`:
   - Takes parallel arrays of (ptrs, lens, argc)
   - Copies each (ptr, len) pair to a null-terminated string
   - Builds argv, forks, calls execvp in the child
   - Parent waitpids and returns the exit status

2. Extended `codegen.rs`:
   - Declares `sash_exec` as an imported function
   - `lower_exec()`: builds two stack-allocated arrays (pointers
     and lengths), stores command name + args, calls sash_exec
   - `lower_inst()`: dispatches echo to write(), true/false to
     no-ops, all other commands to sash_exec

3. Refactored codegen into free functions (`lower_inst`, `lower_echo`,
   `lower_exec`, `lower_value`) instead of methods on a struct — the
   closure-based approach from 022 didn't scale with multiple function
   references.

**Design decision:** Use a C runtime helper rather than emit
fork/exec syscalls directly in Cranelift IR. Keeps the IR clean
and the runtime auditable. The runtime is linked into the binary
as `sash_rt.o`.

**Tests:** 12 end-to-end tests:
- echo regression (literal, assign+echo)
- External commands: date, uname, /bin/echo, printf, cat, wc
- Multiple commands in sequence
- External command with variable argument
- touch + ls
- Object file size check

Each test compiles bash → Cranelift → object, links with sash_rt.o,
runs the binary, and asserts correct stdout.

**Conclusion:** External commands work via fork/exec. The compiled
binary can run arbitrary programs. Next: control flow (exp 024).

---

## Design Decision 12: Null-terminated counted strings

**Date:** 2026-04-12

**Context:** Experiment 023 uses a C runtime (`sash_rt.c`) to convert
(ptr, len) strings to null-terminated strings for `execve`. This is
wasteful and introduces a copy at every syscall boundary.

**Decision:** All strings in the compiled binary are stored as
`(ptr, len)` where `ptr` points to `len+1` bytes with `ptr[len] == 0`.
The `len` field is authoritative for content boundaries. The trailing
null is always present for C/syscall interop and defense in depth.

This is the same representation used in DG's Forth project: counted
strings with a hidden null terminator.

**Consequences:**
- String literals in the data section include the null byte.
- `write(fd, ptr, len)` uses `len` — ignores the null. Correct.
- `execve(path, argv, envp)` passes `ptr` directly — no copy needed.
- Concatenation: `malloc(len_a + len_b + 1)`, copy A (len_a bytes),
  copy B (len_b + 1 bytes including its null).
- The C runtime (`sash_rt.c`) is eliminated — no need for the copy
  step that was its reason for existing.
- Combined with inline syscalls, the compiled binary has zero
  external dependencies (no libc, no dynamic linker).

**Relation to other decisions:** This enables the move from libc
calls to inline Linux syscalls for `write`, `fork`, `execve`,
`wait4`, and `_exit`. The binary becomes a static, self-contained
ELF — strengthening the chain-of-trust story (fewer dependencies
to verify, complete syscall set visible in the IR).

**Implementation:** Experiments 022a and 023a will implement this
approach, replacing the libc-based 022/023. Experiments 022 and 023
are preserved as-is for historical reference.

---

## Experiment 022a: Null-terminated counted strings (v2)

**Date:** 2026-04-12

**Goal:** Implement Decision #12 — all strings stored as (ptr, len)
where ptr points to len+1 bytes with ptr[len] == 0.

**Changes from 022:**
- `intern()` appends a null byte to every literal in the data section.
- `lower_value()` reports `len` (content length, not including null).
- No change to echo/write — `write(fd, ptr, len)` uses `len`.
- New test: `string_data_is_null_terminated` verifies the object file
  contains `"hello\0"`.

**Tests:** 11 (10 from 022 + 1 null-termination check). All pass.

---

## Experiment 023a: Fork/exec via raw syscalls (v2)

**Date:** 2026-04-12

**Goal:** Replace the C runtime (sash_rt.c) with raw Linux syscalls.
No libc dependency. Truly standalone ELF binaries.

**Architecture:**
- `sash_syscall.S` — 5 syscall stubs (write, fork, execve, wait4,
  exit) plus `_start` entry point that saves `envp` to a global.
- `sash_path.c` — PATH lookup using only the syscall stubs. No libc.
  `sash_run(argv, envp)` handles fork, PATH search, execve, wait4.
  Compiled with `-ffreestanding -nostdlib`.
- `codegen.rs` — builds null-terminated argv directly from data-section
  pointers (no string copying thanks to Decision #12). Calls
  `sash_run(argv, envp)` where envp comes from the global set by
  `_start`.
- Linked with `ld` (not `cc`). No `-lc`. No dynamic linker.

**Key result:** `binary_is_statically_linked` test confirms `ldd`
reports "not a dynamic executable."

**Tests:** 13 end-to-end tests. Echo regression (4), external
commands with PATH lookup (date, uname, /bin/echo, printf), mixed
sequences, variable arguments, touch+ls, static linking check,
null-termination check. All pass.

**Note:** sash_path.c is technically C compiled without libc. It
uses only sash_syscall.S stubs and inline asm for access(). This
could be rewritten in Rust with `#[no_std]` or as more asm stubs.
The important thing is: zero libc in the final binary.

**Conclusion:** Compiled safebash binaries are now self-contained
static ELF executables with no external dependencies. The complete
set of syscalls is: write, fork, execve, wait4, exit, access.

---

## Experiment 023b: Pure Rust runtime via rustix

**Date:** 2026-04-12

**Goal:** Replace the C runtime (sash_rt.c) and assembly stubs
(sash_syscall.S) with pure Rust. No C, no external .S files.

**Architecture:**
- `sash-rt/` — a standalone Rust crate (`#![no_std]`, staticlib)
  that provides the runtime for compiled safebash binaries:
  - `sash_write(fd, buf, len)` — via `rustix::io::write`
  - `sash_exit(status)` — via `rustix::runtime::exit_group`
  - `sash_run(argv, envp)` — fork/execve/wait4 with PATH lookup,
    all via rustix syscalls
  - `_start` — 8 lines of inline asm per architecture (the ONLY
    assembly in the project). Extracts envp from the stack, calls
    main(), exits.
  - libc stubs (strlen, memcpy, memset, memcmp, bcmp) — ~5 trivial
    functions required because `no_std` doesn't provide them.
  - Panic handler writes "sash: panic" to stderr and exits 99.

- `exp/023b-rustix-runtime/` — test harness. Compiles bash → 
  Cranelift → .o, links with `ld` against `libsash_rt.a`, runs
  the resulting static ELF binary.

**Key metrics:**
- Binary size: 11,320 bytes for "echo hello"
- Static: `ldd` reports "not a dynamic executable"
- No libc, no C compiler, no assembler in the build
- Only dependency: rustix (by the Wasmtime/Cranelift team)

**Design decisions recorded:**
- Decision #12: null-terminated counted strings
- Decision #13: keep `ld` as the linker (battle-tested, audited,
  provides a verification boundary between .o files)
- `_start` is the only assembly — one `#[cfg(target_arch)]` block
  per architecture (~8 instructions each). Everything else is Rust.
- rustix with `default-features = false` for true no_std.

**Architecture portability:**
- x86_64: implemented and tested ✅
- aarch64: `_start` implemented, untested (needs aarch64 host)
- arm, riscv64: future — add one cfg block each (~10 lines)
- macOS: rustix falls back to libSystem backend; `_start` not
  needed (use `main` entry point via cc linkage)

**Tests:** 13 end-to-end: echo (5), external commands (5),
binary properties (3: static linking, size, null termination).

**Conclusion:** The safebash toolchain is now 100% Rust (plus ld).
No C, no external assembly files. Compiled binaries are self-
contained static ELFs using ~6 Linux syscalls: write, fork,
execve, wait4, exit_group, access.

---

## Experiment 024: Control flow → Cranelift blocks/branches

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

## Experiment 026: End-to-end AOT compilation

**Date:** 2026-04-13

**Goal:** Unify the complete pipeline: bash source → parse → validate
→ IR → SSA → Cranelift codegen → link → static native binary. Prove
that all phases work together on a real script.

**Architecture:**

```
demo.sh → tree-sitter → CST
       → validator (7 rules) → diagnostics
       → lower → IR (12 statements)
       → transform → SSA (5 blocks)
       → codegen → object file (4312 bytes)
       → ld + libsash_rt.a → static ELF (16280 bytes)
```

**The unified `sash` binary:**
- `sash script.sh` — validates, compiles, links, produces a native binary
- `sash script.sh --emit-obj` — stops after object file
- `sash script.sh --skip-validate` — compiles without validation

**Demo script** (`demo.sh`) exercises all implemented features:
- Variable assignment and interpolation (`"$name v$version"`)
- Function definition + elidable command substitution (`$(get_status)`)
- For loop (compile-time unrolling over literal word list)
- If/else with `[[ ]]` string comparison
- External commands (`uname -s`, `date +%Y-%m-%d`)
- Multi-part double-quoted strings (`"$x $y"`)

**Output:** `diff <(./demo) <(bash demo.sh)` produces no differences.

**Binary properties:**
- 16,280 bytes, statically linked, no libc
- Pure Rust toolchain: tree-sitter + Cranelift + rustix
- Only external tool: `ld` (linker)

**Fixes applied:**
- `lower_string` gap fix: captures literal text between variable
  expansions in double-quoted strings (e.g., space in `"$x $y"`)

**Tests:** 12 end-to-end tests covering:
- Compilation + bash comparison (echo, assign, if/else, for, function cmdsub, external cmd, mixed script)
- Validator integration (rejects eval, backticks, indirect expansion, single bracket)
- Binary properties (statically linked, under 1MB)

**Conclusion:** The safebash pipeline is complete from source to
native binary. A bash script goes in, a verified static ELF comes
out. The binary produces identical output to bash. All implemented
language features work end-to-end through all six compilation phases.

---

## Experiment 027: Positional parameters, env vars, exit builtin

**Date:** 2026-04-13

**Goal:** Compile scripts that access `$1`, `$2`, `$HOME`, `$USER`,
and use `exit N`.

**Changes to sash-rt:**
- Added `sash_argc` and `sash_argv` globals, populated by `_start`
- Added `sash_getenv(name, name_len, &out_len) → ptr` for env vars
- Added `sash_capture(argv, envp, &out_len) → ptr` for runtime
  command substitution (fork/exec with stdout capture)
- Added bump allocator for capture buffers (128KB, no-op free)
- Updated `_start` to store argc and argv for both x86_64 and aarch64
- Added `pipe` feature to rustix dependency

**Changes to codegen:**
- Pre-scans SSA for variable references that are positional params
  (numeric names), special vars (`#`, `@`), or env vars (ALL_CAPS)
- Initializes positional params from `sash_argv` at block entry
- Initializes env vars via `sash_getenv` call
- Handles `exit N` as builtin: calls `sash_exit`, traps, and
  signals block termination to skip remaining instructions
- Handles `shift` as no-op (TODO: implement properly)
- Imports `strlen` from sash-rt for null-terminated string length
- Added `sash_capture`, `sash_getenv`, `strlen`, `sash_argc`,
  `sash_argv` to extern declarations

**Tests:** 10 end-to-end tests: positional params, env vars, exit
builtin (code 0 and 42), mixed positional+literal, regressions.

**Binary size:** 11,400–11,504 bytes (still under 16KB).

---

## Experiment 028: Parameter expansion operators

**Date:** 2026-04-13

**Goal:** Compile scripts using `${var%%pat}`, `${var%pat}`,
`${var##pat}`, `${var#pat}`, and `${var:-default}` — the parameter
expansion operators needed by `~/bin/kusto`.

**Changes to sash-rt (lib.rs):**
- `sash_strip_suffix(s, s_len, pat, pat_len, longest) → new_len`:
  Implements `%%` (longest=1) and `%` (longest=0). Handles glob
  patterns with `*` at start or end, plus literal suffix matching.
  Pointer unchanged; returns truncated length.
- `sash_strip_prefix(s, s_len, pat, pat_len, longest) → offset`:
  Implements `##` (longest=1) and `#` (longest=0). Same glob
  pattern handling. Returns byte offset to skip from start.
- `sash_default_value(s_len) → 0|1`: Returns 0 if empty (use
  default), 1 if set. Used by `:-` operator.

**Changes to SSA (ssa.rs):**
- Added `SsaValue::ParamExpansion { var, operator, operand }` variant
  to represent expansions with operators.

**Changes to transform (transform.rs):**
- `expr_to_ssa_value` now checks `ParameterExpansion` for an operator
  field. Plain `${var}` remains `SsaValue::Var`; expansions with
  operators become `SsaValue::ParamExpansion`.
- Added `expr_to_string` helper for operand extraction.

**Changes to codegen (codegen.rs):**
- `emit_ssa_value` handles `SsaValue::ParamExpansion`:
  - `%%`/`%`: calls `sash_strip_suffix`, returns (same ptr, new_len)
  - `##`/`#`: calls `sash_strip_prefix`, returns (ptr+offset, len-offset)
  - `:-`/`-`: calls `sash_default_value`, uses `select` to pick
    variable value or default operand
- Unset variables with `:-` operator treated as empty (len=0) instead
  of panicking — enables `${x:-fallback}` when x was never assigned.
- Interns operand strings alongside other literals.
- Added `strip_suffix`, `strip_prefix`, `default_value` to ExternFuncs.
- `scan_value_for_vars` handles `ParamExpansion` to pre-initialize
  positional/env vars used in expansions.

**Bugs found and fixed:**
- Glob direction was backwards in both strip functions — `starts_with(b"*")`
  and `ends_with(b"*")` were swapped. For suffix stripping, `/*` means
  "/ then anything" which is `*` at end of pattern. Same logic error
  in prefix. Both fixed.
- Original API used `out_len: *mut i64` pointer parameter for return
  values. Cranelift stack slot pointer passing didn't work (function
  was called but result ignored). Refactored to simpler return-value
  API: suffix returns new_len, prefix returns offset.

**Tests:** 10 end-to-end tests:
- `strip_suffix_longest` — `${x%%/*}` on `foo/bar/baz` → `foo`
- `strip_suffix_shortest` — `${x%/*}` on `foo/bar/baz` → `foo/bar`
- `strip_prefix_shortest` — `${x#*/}` on `foo/bar/baz` → `bar/baz`
- `strip_prefix_longest` — `${x##*/}` on `foo/bar/baz` → `baz`
- `default_value_empty` — `${x:-fallback}` when unset → `fallback`
- `default_value_set` — `${x:-fallback}` when set → value
- `strip_suffix_no_match` — no match returns original
- `kusto_style_spec_split` — `CLUSTER=${SPEC%%/*}` + `DB=${SPEC#*/}`
  on `azcore/mydb` → `azcore` + `mydb` (kusto's actual pattern)
- `regression_echo` — basic echo still works
- `regression_positional` — `$1` still works

All tests compare native binary output against bash.

---

## Experiment 029: Runtime command substitution

**Date:** 2026-04-13

**Goal:** Compile `x=$(command args...)` — runtime command substitution
where the command is an external program, not an elidable function.
This is the pattern kusto uses for `TOKEN=$(az account get-access-token ...)`.

**Design Decision #14: Composable process primitives.**

Instead of monolithic `sash_run` (fork+exec+wait) and `sash_capture`
(fork+pipe+exec+read+wait) wrappers, the runtime now exposes:
- `sash_fork() → pid` (0 in child, >0 in parent, -1 error)
- `sash_exec(argv, envp) → !` (execve with PATH lookup, no return)
- `sash_wait(pid) → exit_status`
- `sash_pipe(out_ptr) → 0|-1` (writes [read_fd, write_fd])
- `sash_close(fd) → 0|-1`
- `sash_dup2(src_fd, dst_fd) → 0|-1`
- `sash_redirect(fd, path, flags) → 0|-1` (open+dup2, for `2>/dev/null`)
- `sash_read(fd, buf, len) → bytes_read`
- `sash_alloc(n) → ptr` (bump allocator, now exported)

Redirects happen in the child's address space between fork and exec.
Parent's file descriptors are never touched. This makes `2>/dev/null`,
`>file`, `2>&1` all composable without special-casing each one.

Legacy `sash_run` and `sash_capture` are rebuilt as thin wrappers
on top of the primitives, preserving backward compatibility with
experiments 021–028.

**Changes to sash-rt (lib.rs):**
- Replaced monolithic `run_command()` with composable primitives
- `sash_alloc` promoted from internal to `#[no_mangle] pub extern "C"`
- `sash_capture` rewritten using `sash_fork`/`sash_pipe`/`sash_exec`/
  `sash_read`/`sash_close`/`sash_wait` (was ~100 lines of inline
  fork/pipe/exec, now ~30 lines calling primitives)
- `sash_run` reduced to 5 lines: fork → child exec / parent wait

**Changes to SSA (ssa.rs):**
- Added `SsaValue::CmdSubst(Vec<SsaValue>)` — carries the command's
  argv as SSA values (allows variable interpolation in arguments).

**Changes to transform (transform.rs):**
- `CommandSubstitution` that isn't an elidable function call now
  emits `SsaValue::CmdSubst` with argv extracted from the command.

**Changes to codegen (codegen.rs):**
- `lower_value` handles `SsaValue::CmdSubst`: emits the full
  fork/pipe/exec/read sequence using composable primitives:
  1. Build argv array on stack
  2. `sash_pipe` → get read/write fds
  3. `sash_fork` → branch on pid==0
  4. Child block: close read fd, dup2 write→stdout, close write, exec, trap
  5. Parent block: close write fd, read loop into heap buffer,
     close read fd, wait, strip trailing newline
- All 9 composable primitives declared as extern functions
- `intern_value` and `scan_value_for_vars` handle `CmdSubst`

**Tests:** 9 end-to-end tests:
- `cmdsub_echo_simple` — `x=$(echo hello)` → `hello`
- `cmdsub_uname` — `x=$(uname -s)` → matches bash
- `cmdsub_with_args` — `x=$(date +%Y)` → matches bash
- `cmdsub_in_echo` — `echo "$(echo world)"` → `world`
- `cmdsub_strips_trailing_newline` — shell convention verified
- `cmdsub_with_variable_arg` — `$(echo "$name")` with interpolation
- `regression_echo`, `regression_positional`, `regression_param_expansion`

---

## Experiment 030: Janet oracle parity

**Date:** 2026-04-14

**Goal:** Bring the Janet oracle current with experiments 028–029 and
reframe its role in the project.

**Design Decision #15: The Janet oracle defines safebash semantics.**

Safebash is not a bug-for-bug reimplementation of bash. It compiles a
safe subset — and may refuse functionality or replace it with a safe
alternative, so long as the alternative does what the programmer
*intended*, safely. Bash itself is not the specification. The *model*
is the specification.

Testing against bash checks behavior for a given set of inputs. The
Janet oracle externalizes the *reasoning* — the emitted Janet is a
legible, inspectable statement of what each bash construct *means*.
`(string/slice x 0 (string/find "/" x))` is a readable claim about
what `${x%%/*}` does. If that claim is wrong, you can see it by
reading, even without running.

When the oracle and bash disagree, the question is not "which is
right?" but "which behavior does safebash want, and why?" Every such
disagreement is a design decision worth documenting.

Two independent implementations agreeing is epistemically stronger than
one implementation matching a reference. If codegen has a bug that
happens to produce correct output for test inputs, the oracle catches
it — because it would need the *same* bug independently. The oracle
can't share Cranelift bugs with the codegen because it doesn't use
Cranelift at all.

**Changes to emit_janet.rs:**
- `%%` / `%` (suffix stripping): glob patterns with `*` at either end
  → `string/find` or `string/find-all` + `string/slice`
- `##` / `#` (prefix stripping): same glob handling → `string/find`
  for shortest, `string/find-all` for longest
- `:-` (default value): improved to use `(if (= (length var) 0) ...)`
  for declared variables, rather than `(or ...)` which treats empty
  string as truthy in Janet
- Literal suffix/prefix patterns → `string/has-suffix?` /
  `string/has-prefix?` + `string/slice`

**Changes to ssa_printer.rs:**
- Updated to handle all new SSA value types: `FuncCall`, `CmdSubst`,
  `Concat`, `ParamExpansion`, `Test`

**Changes to ir.rs, lower.rs, ssa.rs, transform.rs:**
- Brought forward from experiment 029 (current IR/SSA/transform)

**Changes to GUIDE.md:**
- Rewrote Janet Oracle section to reflect DD#15 — oracle as normative
  semantics, not just a test tool

**Tests:** 38 oracle tests (28 original + 10 new):
- `oracle_default_value_empty` — `${x:-fallback}` when unset
- `oracle_default_value_set` — `${x:-fallback}` when set
- `oracle_strip_suffix_longest` — `${x%%/*}` → `foo`
- `oracle_strip_suffix_shortest` — `${x%/*}` → `foo/bar`
- `oracle_strip_prefix_shortest` — `${x#*/}` → `bar/baz`
- `oracle_strip_prefix_longest` — `${x##*/}` → `baz`
- `oracle_cmdsub_echo` — `x=$(echo hello)` → `hello`
- `oracle_cmdsub_with_args` — `x=$(uname -s)` → matches bash
- `oracle_cmdsub_inline` — `echo "$(echo world)"` → `world`
- `oracle_cmdsub_with_variable` — `$(echo "$name")` with interpolation

## Design Decision #16: Integer type system

Safebash distinguishes two types: **string** (default) and **int**.

### Type annotations
- `declare int x=5` / `local int x=0` — explicit int variable
- `declare -i x=5` — same, backward-compatible bash syntax
- `fn name() -> int { return 8080; }` — integer-returning function
- `function name -> int { ... }` — same with `function` keyword
- `name() -> int { ... }` — same with `name()` syntax

### Intrinsically-typed values
- `$#`, `$?` — always int (argc, exit status)
- `$((...))` — arithmetic expansion result is int

### Type conversion boundaries
- **String → int**: `$(($var))` — arithmetic context
- **Int → string**: `"$var"` — interpolation context
- Cross-type assignment without conversion is a compile-time error

### Function return types
- **Untyped functions**: `return` is 0–255 (exit code), boolean-testable with `if func`
- **Int functions** (`-> int`): `return` carries full i64, NOT boolean-testable
- `if func` where `func` is `-> int` is a compile-time error
- `declare int x=func()` — captures int return value (see DD#17 for syntax)

### Grammar changes
- `fn` keyword alias for `function` (3 chars vs 8)
- `-> int` return type annotation (Rust/Zig-inspired, between signature and body)
- `type_annotation` node in tree-sitter-bash fork (4-line patch)
- `int` in declaration context handled in lowerer (external scanner eats it as variable_name)
- `-i` in declaration context handled in lowerer (same as before)

### Rationale
Bash's string-only model means `-lt` vs `<` is a footgun — same variables,
different interpretation, chosen by one character. DG has personally hit this
bug. A type system makes the model explicit: comparisons on int must be numeric,
comparisons on strings must be lexicographic. The compiler catches the mismatch
at compile time instead of producing silently wrong results at runtime.

### Safety gain
- `[[ $name -lt 1 ]]` where `$name` is a string → compile-time error
- `[[ $# < 5 ]]` where `$#` is int → compile-time error (use `-lt`)
- `exit $code` where `$code` is unbounded int → compile-time error

## Design Decision #17: Integer function call syntax

**Question**: How to capture an int function's return value?

### Options considered
- **A: `declare int x=foo()`** — direct call syntax, mirrors definition.
  Problem: with arguments, `foo(1 "$name" $#)` invents a non-bash calling
  convention (bash uses spaces, not parens for args). Collides with subshell.
- **B: `declare int x=$((foo()))`** — route through arithmetic context.
  Problem: `"$name"` inside `$(())` makes no sense — arithmetic doesn't have
  strings. Overloads `$(())` to mean function invocation.
- **C: `declare int x=$(foo 1 "$name" $#)`** — command substitution syntax.

### Decision: Option C — `$(func args...)` is universal

`$(...)` is the function call + capture mechanism for both string and int
functions. The compiler checks the function's declared type:
- **String function**: `$(...)` → fork/pipe/exec, capture stdout (existing)
- **Int function** (`-> int`): `$(...)` → call, capture return register

No new syntax needed. The return type determines the representation (i64
register vs string buffer), not the call syntax. The caller doesn't need
different syntax for int vs string functions.

Works in all contexts:
- `declare int x=$(foo ...)` — assignment
- `echo "$(foo ...)"` — int→string at interpolation boundary
- `[[ $(foo ...) -lt 10 ]]` — comparison operand
- `$(( $(foo ...) + 1 ))` — arithmetic operand

# Experiment 031: Type system + grammar fork

**Goal**: Introduce a two-type system (string/int) with explicit annotations,
forked tree-sitter-bash grammar with `fn` keyword and `-> int` return types,
and a type checker that catches cross-type assignment errors at compile time.

## Grammar fork

4-line patch to tree-sitter-bash (forked at `/home/dangood/src/tree-sitter-bash-safebash`,
branch `safebash`):

1. `choice('function', 'fn')` — `fn` keyword alias
2. `optional(seq('->', field('type', $.type_annotation)))` — return type on functions
3. `type_annotation: $ => token(prec(2, 'int'))` — new node type
4. `field('type', $.type_annotation)` in `declaration_command` repeat — type in declarations

`-i` in declarations is NOT handled by the grammar (external scanner eats it as `word`).
The lowerer handles `-i` and `variable_name "int"` as type annotations.

## IR changes

- `VarType` enum: `String` (default), `Int`
- `Declaration.var_type: VarType`
- `Function.return_type: VarType`

## Lowerer changes

- `declaration_command`: checks for `type_annotation` node, `word "-i"`, or
  `variable_name "int"` → sets `var_type = VarType::Int`
- `function_definition`: checks for `type` field → sets `return_type = VarType::Int`
- `fn` keyword: handled transparently by grammar

## Type checker (`type_check.rs`)

New module operating on the IR after lowering. Tracks variable and function types
in a `TypeEnv`. Reports errors for:

- **Cross-type assignment**: `declare int x=hello` (string to int), or
  `declare x=hello; x=$#` (int to string var)
- **Exit with unbounded int**: `exit $code` where `code` is `declare int`
- **Int function in boolean context**: `if int_func; then` — int functions
  return values, not success/failure
- **Intrinsic int types**: `$#`, `$?` are always int
- **Arithmetic expansion**: `$((...))` is always int
- Includes conversion hints in error messages:
  `"use \"$var\" to convert int to string"`,
  `"use $(($var)) to convert string to int"`

## Janet oracle changes

- `EmitCtx` tracks `int_vars` and `int_funcs`
- `declare int x=5` emits `(var x 5)` (bare number, no quotes)
- Numeric comparisons (`-lt`, `-eq`, etc.) skip `scan-number` for known-int operands
- New helpers: `emit_expr_as_int()`, `is_int_expr()`

## Parse tree examples

```
fn get_port() -> int { return 8080; }
→ (function_definition name:(word) type:(type_annotation) body:(compound_statement ...))

declare int x=5
→ (declaration_command (variable_name "int") (variable_assignment name:(variable_name) value:(number)))
```

Note: `int` in `declare int x=5` parses as `variable_name` (external scanner wins),
not `type_annotation`. Lowerer interprets it. `-> int` on functions parses correctly
as `type_annotation` (no external scanner competition in that position).

## Tests (59 total)

### Phase 10: Type system IR (8 tests)
- `type_declare_int` — `declare int x=5` → `VarType::Int`
- `type_declare_dash_i` — `declare -i x=5` → `VarType::Int`
- `type_local_int` — `local int x=42` → `VarType::Int`
- `type_fn_keyword` — `fn hello() { echo hi; }` → runs via oracle
- `type_fn_return_int` — `get_port() -> int {}` → `return_type: VarType::Int`
- `type_fn_keyword_return_int` — `fn get_port() -> int {}` → `VarType::Int`
- `type_fn_default_string` — `hello() {}` → `return_type: VarType::String`
- `type_declaration_vartype` — int vs string declarations

### Phase 10b: Type-aware Janet emission (4 tests)
- `type_int_no_scan_number` — int var in `-lt` → `(< x 10)`, no scan-number
- `type_string_needs_scan_number` — string var in `-lt` → `(< (scan-number x) ...)`
- `type_int_vs_literal` — int var vs literal in `-eq` → `(= count 3)`
- `type_fn_int_oracle` — `fn greet() { echo hello; }` → Janet emission works

### Phase 10c: Type checker (9 tests)
- `type_err_int_to_string_var` — `$#` assigned to string var → error
- `type_ok_int_to_int_var` — `declare int x=$#` → OK
- `type_err_string_to_int_var` — `declare int x=hello` → error
- `type_ok_literal_number_to_int` — `declare int x=42` → OK
- `type_err_exit_int_var` — `exit $code` where code is int → error
- `type_ok_exit_literal` — `exit 1` → OK
- `type_ok_bare_assignment_infer` — `x=$#` infers int → OK
- `type_err_int_func_in_if` — `if get_port; then` → error
- `type_ok_string_func_in_if` — `if check; then` → OK

### Plus 38 existing oracle tests (all passing)

## Design Decision #18 (candidate): `local fn` — lexically scoped function shadowing

**Date:** 2025-04-15

**Problem:** Test harnesses need function mocking. Bash achieves this by
redefining functions at runtime, saving the original via `declare -f`,
and restoring via `eval`. Both `eval` and `declare -f` are incompatible
with ahead-of-time compilation.

**Resolution:** Introduce `local fn name() { ... }` — a function definition
scoped to the enclosing block. The shadow goes out of scope at `}`, and the
original definition is automatically restored. Same scoping semantics as
`local var`.

```bash
test_foo() {
    local fn real_func() { mock behavior; }
    # calls here see the mock
    call_something_that_uses real_func
}  # mock expires, original restored — no eval needed
```

The compiler knows at compile time which definition is active at each call
site — it's lexical, not dynamic.

**Eliminates:** `eval "$reset"`, `declare -f` for capture, `unset -f`.

**Grammar:** Needs tree-sitter-bash-safebash patch — `local fn` as a
declaration form in compound statements.

## Design Decision #19 (candidate): Runtime function selection via SSA phi

**Date:** 2025-04-15

**Problem:** Production scripts like `soc-os-update-flash.sh` redefine
functions at runtime based on version detection:

```bash
if [[ $ver -lt 201904 ]]; then
    dumpimage() { command dumpimage ... "$3" "unused"; }
else
    dumpimage() { command dumpimage ... "$3"; }
fi
```

Both definitions are visible in the source. The choice depends on a runtime value.

**Resolution:** Treat this as SSA — the compiler sees both definitions, emits
both as concrete functions (`dumpimage_v1`, `dumpimage_v2`), and uses a phi
node to select which one is active based on the runtime branch. Every call
site goes through the selector (indirect call or branch-selected dispatch).

Requirements:
- All branches must define the function (no "maybe defined" paths)
- All definitions must have the same arity
- The compiler validates these at compile time

This also handles the `check_soft_deps` eval pattern — `eval 'cpio() { ... }'`
inside an `if` becomes a static function definition in one branch, with the
external command path as the other branch. No `eval` needed; the source needs
minor rewriting to make branches explicit.

## Assessment: nothing fundamentally hard remains

**Date:** 2025-04-15

After analyzing three compilation targets (kusto, unittest,
soc-os-update-flash.sh), all remaining work falls into:

1. **Incremental compiler features** — same shape as 027–031:
   concat, arithmetic, while, case, arrays, heredocs, herestrings,
   `read -r`, `getopts`, `printf -v`, `command -v`, substring
   expansion, pattern substitution, array length, file tests,
   glob matching in `[[ ]]`, namerefs (DD#3)

2. **Composable from DD#14 primitives** — fork/exec/wait/pipe already exist:
   subshells, backgrounding (`&`), `kill` (one syscall), fd redirection
   codegen, pipelines

3. **Resolved by design decisions**:
   - `eval` for mocking → `local fn` (DD#18)
   - Runtime function redef → SSA phi (DD#19)
   - `. /etc/os-release` → `sash-expand` KVP loader (DD#1)
   - Type safety → `int` type system (DD#16, DD#17)

4. **Compile-time decisions** — `set -eu`, `shopt -s extglob/lastpipe/nullglob`

Only genuinely interactive constructs (`select`, coproc) remain out of scope.
None of the three targets use them.

## Design Decision #20: Shell options are language definition, not runtime toggles

**Date:** 2025-04-15

**Principle:** `shopt` and `set` commands don't exist in safebash. Shell options
aren't configurable — they're part of the language definition. There's nothing
to toggle.

### shopt options (all resolved at compile time)

**Always on:**
- `extglob` — strictly more expressive pattern matching, no downside
- `lastpipe` — last pipeline command in current shell, the alternative is a footgun
- `globstar` — `**` recursive matching, useful, no downside
- `inherit_errexit` — command substitutions inherit error checking
- `shift_verbose` — `shift` past end of args is an error, not silent
- `assoc_expand_once` — double expansion is a bug source

**Context-sensitive glob behavior:**
- **Iteration contexts** (`for f in *.txt`): nullglob — no matches → skip body
- **Command argument contexts** (`rm *.bak`): failglob — no matches → error
- The compiler knows the context and applies the right semantics

**Off:**
- `dotglob` — `*` skipping hidden files is the safe default
- `nocaseglob` / `nocasematch` — Linux is case-sensitive, case-insensitive matching surprises
- `localvar_inherit` — implicit inheritance is confusing
- `localvar_unset` — confusing semantics
- `xpg_echo` — implicit backslash interpretation surprises; `echo -e` is explicit

**Irrelevant:** All interactive/history options — safebash compiles scripts, not sessions.

### set options

**`set -e` (errexit): doesn't exist.**
The compiler enforces exit checking statically. Every command's exit code must
be explicitly handled — checked (`if cmd`, `cmd || handle`), ignored (`cmd || :`),
or in a propagating context (pipeline). Unchecked commands are compile-time errors.
This is strictly safer than `set -e`, which has known blind spots (conditions,
`&&`/`||` chains, function bodies called from `if`).

**`set -u` (nounset): doesn't exist.**
The type system and variable tracking enforce declarations at compile time.
Referencing an undeclared variable is a compile-time error.

**`set -o pipefail`: always on.**
Ignoring upstream pipeline failures is wrong. The practical conflict with `grep`
(exit 1 = no matches, not an error) is handled by the coerce tool (DD#8):
the coercer inserts `|| :` on non-terminal pipeline segments where nonzero exit
is informational, not fatal. The programmer reviews the diff.

**`set -x` (xtrace): compile-time `--trace` flag.**
Not a runtime toggle. `safebash --trace script.sh` emits trace instrumentation
in the compiled binary. Production builds omit it.

**`set -C` (noclobber): off.**
Too surprising as default. Handle case-by-case via safe-write patterns.

**`set -f` (noglob): off.**
Globs are safe via context-sensitive nullglob/failglob.

### Coerce tool (DD#8) for migration

The coerce tool (`--coerce`) takes a bash script and outputs the safebash-ready
version, enabling easy diffs and quick migration:

1. Parse the bash script
2. Identify unchecked commands and pipeline segments
3. Insert `|| :` where exit codes are informational, not fatal
4. Remove `set -e`, `set -u`, `shopt` calls (now implicit)
5. Output the coerced version
6. Programmer diffs, adjusts, commits

The coercer is conservative — it marks everything it's unsure about and the
programmer decides.

## Design Decision #21: `-` prefix for error suppression

**Date:** 2025-04-15

**Problem:** With pipefail always on (DD#20) and implicit fatal default for
commands, the programmer needs a way to suppress failure for specific commands.
The `|| :` idiom is syntactically broken inside pipelines (`|| :` binds tighter
than `|`), and modifying the command name (`cmd?`, `cmd!`) feels wrong — the
command is a program, the modifier is about *our response* to its result.

**Resolution:** Borrow Make's `-` prefix. A `-` before a command means
"ignore this command's nonzero exit."

```bash
mktemp                          # fatal on failure (default)
- grep pattern file             # no matches is ok
- diff a b                      # difference found is ok
- grep pattern file | wc -l    # grep tolerates failure, wc is fatal
sort file | - head -1           # head gets SIGPIPE, that's ok
```

**Rules:**
- `-` always applies to the single command immediately following it
- `-` never applies to a whole pipeline — annotate each segment individually
- No ambiguity: `- cmd1 | cmd2` means cmd1 tolerates failure, cmd2 is fatal
- `-` at statement position is not valid bash syntax, so no collision

**Rationale:**
- Same semantics as Make's `-` prefix, decades of precedent
- The modifier is on the *statement*, not the command — the command is just
  a program, we're modifying how we respond to its result
- Reads naturally: "don't care about this one"
- The coerce tool (DD#8) inserts `-` where needed during migration

**Considered and rejected:**
- `cmd?` — reads as a predicate/question, opposite of "ignore"; also modifies
  the command name rather than the statement
- `|| :` inside pipelines — syntactically broken, needs ugly `{ cmd || true; }`
- `try cmd` — implies `catch`, feels incomplete without it
- `|:` as pipe variant — only works in pipe context, not standalone

## Design Decision #22 (candidate): No word splitting, no IFS, arrays and read

**Date:** 2025-04-15

**Principle:** Word splitting doesn't exist in safebash. `$var` and `"$var"` are
identical — quotes are purely cosmetic. IFS doesn't exist as a concept. This
eliminates an entire class of bugs.

**Consequences:**

1. **Unquoted `$var` is never split.** No implicit splitting on spaces, tabs,
   newlines, or any other character. An unquoted expansion always produces
   exactly one value.

2. **`for x in $choices` is a compile error.** Space-separated strings as
   pseudo-lists are replaced by proper arrays: `for x in "${choices[@]}"` (or
   a simpler syntax TBD).

3. **`IFS` doesn't exist.** No global variable controlling field splitting.
   The only place splitting occurs is `read`, which specifies its separator
   explicitly.

4. **`read` / `mapfile` taxonomy (finalized 2026-04-17):**

   Four distinct operations, all IFS-free:

   | Form | Semantics | Delimiter |
   |---|---|---|
   | `read -r line` | One line → one variable | newline (implicit) |
   | `read -d: -r a b c` | One line → named fields | explicit via `-d` |
   | `read -d: -a arr` | One line → array of fields | explicit via `-d` |
   | `mapfile -t arr` | Many lines → array of lines | newline (default) |

   - `-r` is always implied (SC2162 coerces bare `read` → `read -r`).
   - `-d` replaces all `IFS=x read` patterns. No IFS variable exists.
   - `<<<` (here strings) supported for feeding variables to `read`:
     `read -d: -a path <<< "$PATH"` — compiler lowers to memory buffer,
     not a temp file.
   - `read -a` is NOT redundant with `mapfile`: `read -a` splits one
     line into fields; `mapfile` loads many lines as whole elements.

5. **Array syntax (TBD):** The `"${array[@]}"` syntax is widely considered
   awkward. A simpler iteration syntax is desirable but deferred to a dedicated
   design session.

6. **Multi-dimensional arrays (TBD):** Currently faked with composite string keys
   (`payload[foo:version]`). Whether safebash needs proper multi-dimensional
   arrays or whether composite keys are sufficient is deferred.

**Migration:** The coerce tool (DD#8) converts `IFS=x read` to `read -dx`,
converts `for x in $var` to array iteration, and makes quotes optional
(since they're no longer semantically meaningful for variable expansion).

## Design Decision #23: PATH is compile-time only

**Date:** 2025-04-16

**Principle:** PATH is not a runtime-mutable variable. Scripts cannot
`export PATH=...` or `PATH=$PATH:/extra`. The compiler controls how
external commands are found.

### Two modes

**Mode 1: Compile-time PATH, runtime search** (default)

PATH is baked into the binary as a constant. At runtime, `sash_exec`
walks the compiled-in PATH to find binaries. Good for general-purpose
scripts (kusto, dev tooling) where the target machine is similar to
the build machine.

```
safebash compile script.sh                    # uses build machine's PATH
safebash compile --path=/usr/bin:/bin script.sh  # explicit PATH
```

**Mode 2: Compile-time resolution, absolute paths** (`--resolve`)

Every external command is resolved to an absolute path at compile time.
No PATH search at runtime — the binary calls `execve("/usr/bin/mktemp", ...)`
directly. Any unresolved command is a fatal compile error.

```
safebash compile --resolve script.sh
safebash compile --resolve --sysroot=/mnt/target script.sh
safebash compile --resolve --sysroot=image.squashfs script.sh
```

The `--sysroot` flag (same concept as GCC) points the compiler at a target
root filesystem. Commands are searched relative to sysroot but paths in the
binary are absolute relative to `/` on the target.

**Manifest:** In `--resolve` mode, the compiler emits a dependency manifest
listing every external binary the compiled script requires. Useful for
portable services — you know exactly what host binaries the service needs.

### Escape hatch

`env PATH=/special/dir command` is allowed in both modes. It runs a single
child process with a different PATH without affecting the compiled binary's
own resolution. This is the mechanism for commands that genuinely need a
different search path at runtime.

### Security argument

- Mode 1: No PATH injection at runtime — attacker can't prepend a directory
- Mode 2: No PATH at all — binary has absolute paths, zero search surface
- Both: `PATH=... command` in the script is a compile error
- `env` escape hatch is explicit, visible, auditable

### Use cases

- **kusto:** Mode 1 — general dev tooling, machines are similar
- **soc-os-update-flash.sh:** Mode 2 with `--sysroot` — systemd portable
  service atop a known base image, exact binary layout known
- **unittest:** Mode 1 — runs on the build machine

## Design Decision #24 (candidate): Taint quarantine and bless

**Date:** 2025-04-16

**Principle:** Taint doesn't exist inside the program. External data is
quarantined until blessed. The quarantine is enforced by the compiler refusing
any use of unblessed external data.

### What is tainted

Only truly external data:
- **Program entry:** `$1`–`$N`, `$@` from the OS
- **Runtime input:** `read`, `$(external command)`, `sash_getenv`

What is NOT tainted:
- `$0` — known at compile time
- `$#`, `$?` — integers, constrained by type system (DD#16)
- Literals, arithmetic results from untainted operands
- Function parameters whose arguments were untainted at the call site

### No propagation

Tainted values cannot be used for any purpose except blessing.
No assignment, no echo, no passing to functions, no command arguments.
The compiler rejects all uses of unblessed external data at compile time.

```bash
x=$1              # compile error
echo $1           # compile error
cmd $1            # compile error
process $1        # compile error
bless [[ $1 =~ ^[0-9]+$ ]]  # ok — $1 is now untainted
echo $1           # ok — blessed
```

This is simpler than Perl's taint mode. Perl tracked taint through every
operation (concat, substring, etc.). Safebash has no taint inside the
program — external data is quarantined at the boundary.

### Bless syntax

`bless` combines validation and untainting in one construct. The `[[ ]]`
or `(( ))` test expression validates; the LHS variable is untainted on
success. Only one variable per `bless`, `&&`/`||` ok within the test so
long as the same variable is tested throughout.

**Three forms, distinguished by context:**

| Form | Failure behavior |
|------|-----------------|
| `bless [[ $1 =~ ... ]]` bare | fatal exit |
| `if bless [[ $1 =~ ... ]]` | falsy, take else branch |
| `bless [[ ${1:~default} =~ ... ]]` | substitute default |

```bash
# Fatal on bad input
bless [[ $1 =~ ^[a-z]+$ ]]

# Handle bad input gracefully
if bless [[ $1 =~ ^[0-9]+$ ]]; then
    declare int port=$(($1))
else
    echo "bad port"
    exit 1
fi

# Fall back to default on bad input
bless [[ ${port:~8080} =~ ^[0-9]+$ ]]

# Arithmetic validation
bless (( ${timeout:~30} > 0 ))
```

### The `:~` operator

New parameter expansion operator, valid only inside `bless`. Means
"if unset *or if validation fails*, use this default."

- `${1:-8080}` — if unset, use 8080 (existing bash, unchanged)
- `${1:~8080}` — if unset or invalid, use 8080 (bless-only)

Outside `bless`, `:~` is a compile error. One context, one meaning.

### Objections acknowledged

1. `bless` does three things depending on context (fatal, truthy, substitute).
   Same overloading concern as `set -e` behavior varying by context.
   Counter: the three forms are syntactically distinct — bare statement,
   `if` condition, `:~` present. The compiler and the reader can distinguish
   them trivially.

2. `:~` mutates the variable to a value the caller never provided.
   Original input is destroyed. Functions that need both the original
   and the validated value can't get both.
   Counter: the author explicitly chose substitution by writing `:~`.
   If they need the original, they use the `if` form instead.

3. Context-only syntax (`:~` only in `bless`) is a learning trap.
   Counter: safebash's value proposition is "mechanical rewrite passes
   your security audit." The new syntax is the audit trail — `bless`,
   `:~`, `-` are security-relevant markers that bash lacks. Each makes
   a security property visible in source.

### Value proposition (clarified)

safebash's value is NOT "you already know the syntax." It's:
- You have a bash script in production
- It needs to pass a security audit
- Rewriting in Go/Rust is months of work
- safebash: mechanical changes, same structure, compiles to static binary, passes audit
- Every piece of new syntax (`bless`, `:~`, `-`, `-> int`) is an auditable
  security marker that makes the script's safety properties visible

## Design Decision #25: Arithmetic overflow traps and call depth limit

**Date:** 2025-04-16

### Arithmetic overflow

Bash integers are signed 64-bit with silent wraparound on overflow.
Safebash traps on signed overflow — it's almost certainly a bug, never
intentional in shell scripts.

**Implementation:** Cranelift provides `sadd_overflow`, `ssub_overflow`,
`smul_overflow` — instructions that return (result, overflow_flag).
The codegen emits a conditional exit on overflow. Three extra instructions
per arithmetic operation.

```
result, overflow = sadd_overflow(a, b)
if overflow → sash_exit(error)
```

No unsigned type. Bash doesn't have one. Bitmask operations on small values
(`logind_flags=$((flags | 2))`) never overflow in practice.

### Call depth limit

Tail self-recursion compiles to a loop (zero stack growth). Non-tail
recursion is a compile error. But mutual recursion and deep non-recursive
call chains could still overflow the stack — and in safebash's `#![no_std]`
runtime, stack overflow is undefined behavior (segfault or silent corruption).

**Solution:** Runtime depth counter. A global `call_depth` variable,
incremented on function entry, decremented on exit. If it exceeds a
compile-time limit (default 256 or 1024), the program exits cleanly.

**Considered and rejected: guard page.** Hardware-based stack overflow
detection via `mprotect` + signal handler on alternate stack. Zero per-call
overhead but: requires signal handler complexity in `#![no_std]` binary,
fires at the edge with no headroom for cleanup/diagnostics, platform-specific
guard page sizes, doesn't trigger on large stack frame jumps.

**Why depth counter wins for safebash:**
- Simple: five instructions per function call
- Deterministic: documented limit, consistent behavior
- Early detection: fires with plenty of stack left for error message
- Tunable: `safebash compile --max-depth=512`
- No stack-allocated buffers in safebash — strings are (ptr, len) on heap

**Guarantee:** If a safebash program compiles, it cannot:
- Overflow signed arithmetic silently
- Stack overflow (tail recursion → loop, everything else → depth counter)
- Non-tail recursion is a compile error

## Design Decision #26: FreeRTOS heap_4 allocator, compiled from C

**Date:** 2025-04-16

**Problem:** The current bump allocator (128KB, never frees) works for
short-lived scripts but fails for:
- Loops with command substitution (each iteration leaks the old value)
- Long-running services (memory grows without bound)
- Any script that reassigns variables in a loop

**Resolution:** Adopt FreeRTOS heap_4 as `sash_heap.c`. Address-ordered
free list with first-fit allocation and coalescing on free. The compiler
emits `sash_free(old_ptr)` at every variable reassignment and scope exit.

### Why heap_4

- Battle-tested: deployed on billions of FreeRTOS devices for 10+ years
- Audited, fuzzed, formally analyzed — the algorithm is proven correct
- Coalescing prevents fragmentation for long-running alloc/free patterns
- Simple: ~300 lines of C, single-threaded (no locking needed)
- Static heap array — no mmap, no sbrk, no OS allocator dependency

### Why C, not a Rust rewrite

Rewriting introduces risk. Every rewritten line diverges from the proven
original. A pointer cast error, alignment bug, or coalescing edge case in
the rewrite would introduce exactly the kind of memory corruption safebash
is designed to prevent. The algorithm stays untouched — only the FreeRTOS
wrappers are stripped.

Build impact: requires `cc` in the toolchain. Trade-off accepted —
`cc` is universally available, and algorithmic correctness outweighs
toolchain purity.

### Why not arena-per-scope

Arenas have zero per-allocation overhead and bulk-free simplicity, but:
- Loops inside a scope accumulate without freeing — grows forever
- Values that escape a scope need promotion (escape analysis or copy)
- Arena-per-iteration requires the compiler to identify scope boundaries
  for freeing, which is equally bug-prone as emitting free calls
- "Arenas can't have use-after-free" is only true if scope boundaries are
  correct — the compiler is the same codebase either way

### Codegen changes

The SSA already knows the lifetime of every value. Codegen is mechanical:
- Variable reassignment: `sash_free(old_ptr)` then assign new value
- Scope exit (function return, block end): `sash_free` all locals
- Function return value: caller owns it, frees on reassignment
- No escape analysis needed — one variable owns one allocation, no aliasing

### Implementation plan

1. Strip FreeRTOS-specific parts from heap_4.c into `sash_heap.c`
   (remove `vTaskSuspendAll`, `configASSERT` macros, hooks)
2. Configure: heap size (default 1MB?), alignment (8 bytes)
3. Export `sash_alloc(size) -> *mut u8` and `sash_free(ptr)`
4. Compile with `cc -c -O2 -ffreestanding`
5. Link alongside `libsash_rt.a` and Cranelift output
6. `sash_alloc`/`sash_free` in sash-rt become thin extern wrappers
7. Update codegen to emit `sash_free` at reassignment/scope-exit points
8. Tests: loop allocation, long-running simulation, fragmentation

### Aspiration

If safebash handles memory correctly, it becomes a viable alternative to
Go or Rust for long-running processes — daemons, systemd services,
monitoring loops. The allocator is one of the foundations that makes
this possible.

# Experiment 032: Concat, while loops, numeric tests in codegen

**Goal**: Three codegen features needed for real scripts — string concatenation,
while loops with SSA phi nodes, and numeric test comparisons (`-lt`, `-eq`, etc.).

## String concatenation

Added `sash_concat(a_ptr, a_len, b_ptr, b_len) -> (ptr, len)` to sash-rt.
Allocates new buffer, copies both halves. Codegen emits concat for multi-part
string expressions (variable interpolation, mixed literal+variable).

Also added `sash_atoi(ptr, len) -> i64` and `sash_itoa(val) -> (ptr, len)` for
string↔integer conversion at the boundary (DD#16).

## While loops

The big challenge: SSA phi nodes in Cranelift.

### Key insight (after failed attempts)

All SSA versions of the same bash variable must share the **same** Cranelift
Variable pair (ptr, len). Cranelift's SSA builder handles block parameter
merging automatically — we don't need to emit explicit phi nodes. We just:
1. Define the variable in the entry block
2. Use `def_var`/`use_var` consistently across all blocks
3. Cranelift inserts block parameters at merge points

### Two-pass while lowering

Single-pass lowering failed: the loop body referenced pre-loop SSA versions
instead of phi-merged versions. Fix: two-pass approach in `transform.rs`:
1. **First pass**: scan body to discover which variables are modified, insert
   phi assignments at loop header
2. **Second pass**: re-lower body statements using phi-merged SSA versions

### $# initialization fix

`$#` was stored as `(null_ptr, count_as_len)` which broke numeric comparisons
(atoi of a null pointer). Fixed by adding `sash_itoa` and converting `$#` to
its string representation at program initialization.

## Numeric test comparisons

`[[ $x -lt 5 ]]`, `-eq`, `-gt`, `-ne`, `-ge`, `-le` — codegen calls `sash_atoi`
on both operands, emits Cranelift `icmp` with appropriate `IntCC` condition.

### Test expression interning fix

Literals inside `[[ ]]` weren't being interned, causing `lower_expr_value` to
return `(0, 0)`. Added `intern_test_expr` to the interning pass and
`scan_test_for_vars` to find variable references in test expressions.

## Tests

- 13 codegen tests (concat, while, numeric comparisons, argc numeric)
- 59 oracle tests (inherited + new parity tests)
- All passing

## Files changed

- `sash-rt/src/lib.rs`: `sash_concat`, `sash_atoi`, `sash_itoa`, `sash_arith_eval`
- `032/src/codegen.rs`: concat emission, while loop blocks, numeric icmp
- `032/src/transform.rs`: two-pass while lowering with phi insertion
- `032/src/lower.rs`: `intern_test_expr`, `scan_test_for_vars`
- `032/tests/codegen_tests.rs`: 13 integration tests

# Experiment 033: shift, export, $@ in codegen

**Goal**: Three small features needed for `~/bin/kusto` compilation target.

## shift

Codegen adjusts the global `sash_argc` and `sash_argv` at runtime:
decrement argc by 1, advance argv pointer by 8 bytes (one pointer).
After shift, all positional params (`$1`, `$2`, `$#`, `$@`) are
reloaded from the updated globals by iterating the vars map and
re-emitting loads for any variable whose base name is numeric, `#`,
or `@`.

## export

New SSA instruction `SsaInst::Export { target, value, name }` — emits
an assignment (same as `Assign`) followed by a call to `sash_setenv`
which updates the process envp array. `sash_setenv` either replaces an
existing entry (pointer swap) or allocates a new envp array with the
entry appended.

## $@

New runtime function `sash_join_args(argv, argc, sep_ptr, sep_len,
out_len)` joins `argv[1..argc]` with a separator. In echo context,
`$@` is initialized at startup by calling `sash_join_args` with a
space separator. After `shift`, it's re-joined from the updated argv.

Note: `"$@"` in command argument position (where each arg should be a
separate argv entry) is not yet handled — would need dynamic argv
construction at the call site. The current implementation handles the
echo/interpolation case only.

## sash-rt additions

- `sash_setenv(name_ptr, name_len, val_ptr, val_len)` — set env var
- `sash_join_args(argv, argc, sep_ptr, sep_len, out_len) -> ptr` — join args

## Tests

- 20 codegen tests (13 inherited + 7 new: shift_basic, shift_with_argc,
  export_basic, export_visible_to_child, dollar_at_echo, dollar_at_empty,
  shift_then_dollar_at)
- 59 oracle tests (inherited, all passing)

# Experiment 034: Redirections in codegen

**Goal**: Support `2>/dev/null`, `>&2`, and `> file` redirections.

## Approach

Added `redirects: Vec<Redirect>` field to `SsaInst::Command`. The
SSA transform now threads redirect info from `StmtKind::Redirected`
through to the last emitted Command instruction.

Codegen emits a save/redirect/restore pattern around each command:

1. **Save**: `dup(fd)` to preserve the original fd
2. **Redirect**: `sash_redirect(fd, path, flags)` for file targets,
   `sash_dup2(target_fd, fd)` for `>&N` fd duplication
3. **Execute**: the command runs with redirected fds
4. **Restore**: `dup2(saved, fd)` then `close(saved)`

Interning pass updated to intern redirect target paths (e.g.
`/dev/null`).

## sash-rt addition

- `sash_dup(fd) -> new_fd` — raw dup syscall

## Tests

- 23 codegen tests (20 inherited + 3 new: redirect_stderr_to_devnull,
  redirect_stdout_to_file, redirect_stderr_devnull_on_command)
- 59 oracle tests (inherited)

# Experiment 035: Command substitution codegen

**Goal**: Verify and test `$(...)` command substitution in assignment context.

## Findings

The full fork/pipe/exec/read pipeline for `SsaValue::CmdSubst` was already
implemented in codegen (from exp 028). The SSA transform correctly lowers
`X=$(cmd args...)` into `SsaInst::Assign { target: x_1, value: CmdSubst([cmd, args...]) }`.

Codegen builds a null-terminated argv on the stack, forks, redirects the
child's stdout through a pipe, reads into a 64KB heap buffer, strips the
trailing newline (shell convention), and returns (ptr, len).

## Tests added

7 new codegen tests:
- cmdsub_echo — basic `X=$(echo hello)`
- cmdsub_in_string — interpolation `"hello $NAME"` where NAME comes from cmdsub
- cmdsub_with_args — multi-arg command `$(echo -n foo)`
- cmdsub_multiline_strips_trailing_newline — confirms newline stripping
- cmdsub_empty_output — `$(true)` produces empty string
- cmdsub_in_conditional — cmdsub result used in `[[ -z ]]`
- cmdsub_with_redirect — confirms cmdsub works (redirect on outer cmd TBD)

**Totals**: 30 codegen + 59 oracle = 89 tests

# Experiment 036: Associative arrays (declare -A)

**Goal**: Support `declare -A MAP=([key]=val ...)` and `${MAP[$KEY]}` subscript access.

## Design

Associative arrays are stored as flat tables of `(key_ptr, key_len, val_ptr, val_len)`
tuples on the stack. The variable's StringVar (ptr, len) slots store (table_address,
entry_count) instead of the usual (string_ptr, string_len).

Lookup uses `sash_assoc_lookup` in sash-rt: linear scan comparing keys byte-by-byte.
For kusto's ~10-entry cluster map, linear scan is optimal.

## Implementation

**IR**: Added `ExprKind::AssocArray(Vec<(String, Expr)>)` and
`ExprKind::Subscript { name, index: Box<Expr> }`.

**Lower**: `lower_assoc_assignment` parses `[key]=value` concatenation nodes from
tree-sitter's array children. Expansion handler produces `Subscript` when a
`subscript` node is found (instead of flattening into `name[idx]` string).

**SSA**: `SsaValue::AssocArray(Vec<(String, SsaValue)>)` and
`SsaValue::Subscript { array, index: Box<SsaValue> }`.

**Codegen**: AssocArray builds the flat table on a stack slot, stores each entry's
key/value as interned data pointers. Subscript calls `sash_assoc_lookup`.

**sash-rt**: Added `sash_streq` (byte comparison) and `sash_assoc_lookup` (linear scan).

## Bug found and fixed

`scan_value_for_vars` was adding the array base name (e.g. "MAP") to
`positional_refs`, causing the init pass to create MAP_0 with fresh CL Variables
and initialize it via `getenv("MAP")`. This shadowed the AssocArray assignment
(MAP_1) which used different CL Variables from `base_vars`. Fix: don't add
Subscript array names to positional_refs — they're locally assigned variables.

## Tests

- 34 codegen tests (30 inherited + 4 new: assoc_array_basic, assoc_array_second_key,
  assoc_array_missing_key, assoc_array_variable_key)
- 59 oracle tests (inherited)

### 036 continued: Positional bounds checks, kusto integration, sash CLI

**Positional init bounds check**: `$1` init loaded `argv[1]` without checking
`argc`, segfaulting when called with no args. Fix: branch on `idx < argc` —
in-bounds loads the arg pointer + strlen, out-of-bounds sets (null, 0).

**Shift bounds check**: Same issue — shift reloaded positional params
unconditionally. Fix: same branch-on-bounds pattern.

**Quote stripping**: `declare -A MAP=([k]="value")` included literal quotes
in the stored value. Fix: strip matching quotes in `lower_assoc_assignment`.

**Redirected CmdSubst**: `TOKEN=$(az ... 2>/dev/null)` — the `command_substitution`
lowerer wraps the command in `StmtKind::Redirected`. The transform's CmdSubst
handler only matched `StmtKind::Command`, so it fell through to `SsaValue::Expr("$(..)")`.
Fix: unwrap `Redirected` to get the inner `Command` before matching.

**sash CLI**: Replaced the tree-sitter demo `main.rs` with a proper CLI:
`sash <script.sh> [-o <output>]`. Reads source, calls `compile_to_object`,
writes `.o`, links with `ld -static` against `sash-rt`, cleans up.

## Milestone: ~/bin/kusto compiled and running

The full `~/bin/kusto` script (42 lines) compiles to a **22KB static ELF** and
runs end-to-end:

```
$ sash ~/bin/kusto -o /tmp/kusto
$ /tmp/kusto azcore/OvlProd
>                                    ← Kusto CLI interactive prompt
```

The binary: resolves cluster alias via associative array, runs `az account
get-access-token` via command substitution, captures the JWT, validates it
with `-z` check, exports PATH, exec's `dotnet Kusto.Cli.dll` with the full
connection string. No libc, no interpreter, 22KB.

Features exercised: `declare -A`, `${MAP[$KEY]}`, `$1`, `shift`, `${SPEC%%/*}`,
`${SPEC#*/}`, `$(cmd)`, `if [[ -z ]]`, `if [[ $# -lt N ]]`, `echo ... >&2`,
`export`, `$@`, `exit`, string concatenation, env var lookup.

## Tests

- 44 codegen tests (10 new: kusto_cluster_resolve, kusto_usage_error,
  kusto_unknown_cluster, kusto_param_expansion, kusto_full_pipeline,
  kusto_full_unknown, kusto_full_usage_error, shift_then_dollar_at,
  redirect_stderr, export_visible_to_child)
- 59 oracle tests (inherited)

## Experiment 037: for, case, functions in codegen (2026-04-16)

Added native code generation for `for` loops (literal word lists,
single word, variable expansion), `case` statements (pattern matching
with wildcards, variable-derived discriminants), and user-defined
functions (basic call/return, function-scoped variables).

**Tests added:** 10 codegen tests (for_literal_words, for_single_word,
for_with_variable, case_basic, case_second_branch, case_wildcard,
case_from_arg, case_from_arg_wildcard, case_sets_variable,
function_basic). Total: 58 codegen + 59 oracle = 117.

## Experiment 038: dynamic for loops (2026-04-16)

Extended `for` loops to iterate over `$@` (positional parameters).
Handles empty argument lists and single-argument edge cases.

**Tests added:** 3 (for_dollar_at, for_dollar_at_empty,
for_dollar_at_single). Total: 61 codegen + 59 oracle = 120.

## Experiment 039: until loops, while read (2026-04-17)

Added `until` loop codegen and `while read line` for line-by-line
stdin processing. This enabled the first scripts that consume piped
input.

**Tests added:** 5 (until_basic, until_immediate_exit,
while_read_stdin, until_countdown, while_read_count). Total: 66
codegen + 59 oracle = 125.

## Experiment 040: arithmetic statements (2026-04-17)

Codegen for `(( ))` as a command (not just `$(( ))` expansion):
postfix increment/decrement, compound assignment (`+=`, `-=`),
bare assignment, `(( ))` as an `if` condition, and C-style
`for (( ; ; ))` loops.

**Tests added:** 6 (arith_cmd_postfix_increment,
arith_cmd_compound_assign, arith_cmd_assign, arith_cmd_if_condition,
arith_cmd_decrement, c_style_for_loop). Total: 72 codegen + 59
oracle = 131.

## Experiment 041: integer variables (2026-04-17)

Implemented `declare -i` / `int` variable support in codegen.
Integer variables store machine-width integers directly instead of
string pointers. Arithmetic on int vars avoids atoi/itoa round-trips.
Type checker tracks int vs string through the SSA graph.

**Tests added:** 7 (int_var_arithmetic, int_var_arith_cmd,
int_var_if_arith, int_var_if_arith_false, int_var_while_countdown,
int_var_c_style_for, int_var_dollar_arith_expansion). Total: 79
codegen + 59 oracle = 138.

## Experiment 042: exec, lists, source emission (2026-04-18)

Added `exec` (replaces process), `&&`/`||` lists with short-circuit
evaluation, `!` pipeline negation in codegen. Also the `--emit-source`
flag for source-to-source output (precursor to the full source emitter
built later in Phase 2).

**Tests added:** 23 (exec_replaces_process, list_and_success/failure,
list_or_success/failure, list_cmd_and, negation variants, plus
source emission tests). Total: 102 codegen + 59 oracle = 161.

## Experiment 043: printf, arrays (2026-04-18)

Codegen for `printf` (basic format strings, `%d`, multiple args)
and indexed arrays (declaration, variable indexing, loop iteration).
Associative arrays wired through the unified `SashTable` (DD#33).
Sash-native `array` and `map` keywords first supported here.

**Tests added:** 11 (printf_basic, printf_multiple_args,
printf_no_format, printf_percent_d, indexed_array_basic,
indexed_array_variable_index, indexed_array_loop,
assoc_array_with_table, indexed_array_from_args, keyword_map,
keyword_array). Total: 113 codegen + 59 oracle = 172.

## Experiment 044: pipe-while-read, final feature round (2026-04-19)

The last and largest experiment. Multi-stage pipelines with
`while read` (`echo | while read line`), the noop colon command
(`:`), `echo -n`, arithmetic comma operator, and many edge cases.
This experiment reached 247 codegen tests — the full set that
carried forward into the consolidated `sash/` crate.

**Tests added:** 134 (pipe_while_read_basic, pipe_while_read_count,
pipe_while_read_multi_stage, noop_colon, echo_n_no_newline,
arith_comma_operator, plus ~128 others covering all remaining
codegen edge cases). Total: 247 codegen + 59 oracle = 306.

## Experiment 045: validator with taint tracking and bless (2026-04-19)

Standalone validator experiment implementing flow-sensitive taint
analysis. All external input (command substitution, environment
variables, positional parameters, `read`) is tainted. The `bless`
command clears taint after validation. This experiment proved the
taint model before integrating it into the consolidated compiler.

**Tests added:** 68 integration tests covering eval/source rejection,
indirect expansion, taint propagation, bless semantics, and clean
scripts. This was the last numbered experiment.

---

# Phase 2: Consolidated compiler (2026-04-20 onward)

After experiment 045, all code was consolidated into the `sash/`
crate. The numbered experiment model was retired — further work
happens directly in `sash/`. The experiments were later split into
the `safebash-exp` repo (git submodule at `exp/`).

Key developments in Phase 2 (each supported by design decisions
documented below):

**Taint integration (DD#43, DD#44).** The taint tracker from exp/045
was integrated into the compiler pipeline. Taint analysis runs after
validation, before lowering. `bless var [[ condition ]]` is the
only way to clear taint. All use sites of tainted variables produce
SB4020 diagnostics. Arithmetic context `(( ))` is a read context
for taint purposes (DD#43 amendment).

**Source emitter (DD#45).** `--emit-source` outputs sash-native
source by recording coercions during the CST walk and applying them
to the original source bytes. Comments and formatting are preserved.
`--annotate` adds `# was: ...` comments showing original syntax.
The same coerced source is embedded in a `.sash_source` ELF section
in every compiled binary, recoverable via `sash --extract <binary>`.

**Declaration axes (DD#45 amendment).** Four orthogonal axes for
variable declarations: mutability (`const`), lifetime (`local`),
type (`int`), structure (`array`, `map`). Stacked freely:
`local const int x=5`. Coercions from bash syntax: `declare -i` →
`int`, `declare -a` → `array`, `declare -A` → `map`, `readonly` →
`const`, `let "expr"` → `(( expr ))`. Combined flags split:
`declare -ri` → `const int`.

**Export rejection (SB5001).** `export` and `declare -x` are compile
errors. The sash model uses `FOO=bar cmd` for per-command env vars.
The validator rule catches both the keyword and the flag.

**Heredoc command substitution rejection (SB5002).** `$(cmd)` inside
heredoc bodies feeding external commands is rejected — the tainted
data has no named variable to bless. The safe pattern: extract to
a variable, bless it, reference the blessed variable in the heredoc.

**Regex expression syntax (DD#39 amendments).** `${var:s/re/str/flags}`
is the sash-native expression form of regex replace — returns a value
without mutating. `var =~ s/re/str/flags` is the statement form that
mutates in place. Tree-sitter grammar extended with a new external
scanner token (`:s/`). The `${var/pat/rep}` family is accepted via
glob→regex coercion.

**Known externals auto-bless.** `known_externals.rs` defines
conservative regex patterns for 26 environment variables and 25
commands. When the taint tracker recognizes a known-safe source, it
auto-blesses the target variable. `--emit-source` inserts full
`bless` lines for transparency.

**Janet backend parity.** `emit_janet.rs` brought to functional
parity with the native backend: break/continue, function arguments
in arithmetic, string operations (length, case, substring, prefix/
suffix strip, replace), heredocs, `$?` exit status, `&&`/`||` lists,
modulo. 37 round-trip tests verify identical output between bash and
Janet execution.

**Function argument passing.** Fixed `$1`/`$2` in arithmetic
expressions inside functions. The lowerer now preserves `$` prefix
on positional parameters so the transform doesn't misidentify them
as numeric literals.

**Repo split.** `safebash-exp` created as a separate repo. History
rewritten with `git-filter-repo` to remove experiment directories
and test artifacts from all commits. `.git` shrunk from 16MB to
648KB. Experiments remain accessible as a git submodule at `exp/`.

**Test suite at end of Phase 2:** 457 tests (247 codegen, 114 e2e,
37 Janet, 59 oracle), zero warnings.

## Design Decision #27: Trampoline model for functions

**Decision:** Bash functions are implemented as blocks within the single
Cranelift `_start` function, not as separate Cranelift functions.

**Rationale:** Analyzed all six potential drawbacks of the trampoline model:

1. **No recursion** — eliminated. Safe subset only allows tail recursion,
   which is just a `Goto` back to the function entry block. Guaranteed
   flat loop, better than relying on Cranelift tail-call optimization.

2. **Register pressure** — not real. Cranelift's liveness analysis sees
   that variables in disjoint function blocks are never simultaneously
   live. Registers are reused, not competed for.

3. **No `local` isolation** — solved by the transform. `local x` inside
   `fn foo` is renamed to `foo__x`, a different base variable with its
   own Cranelift Variable pair. Non-local variables share the caller's
   CL Variables, matching bash semantics (functions modify globals).

4. **`$(func)` capture** — turned into a compiler advantage. When the
   call site is `X=$(func)`, the compiler emits duplicate function
   blocks where `echo` appends to a buffer instead of writing to fd 1.
   No fork, no pipe. This is something bash cannot do.

5. **Compile time** — irrelevant. Longest target script is 898 lines.
   Cranelift compilation is near-instant at this scale.

6. **No separate compilation** — not a use case. Safebash compiles
   self-contained scripts, not reusable function libraries.

**Avoided alternative:** Real Cranelift functions (option 2) would require
moving all bash variables from Cranelift Variables to a global memory
struct with explicit load/store — a significant rewrite of the codegen
for no practical benefit at this scale.

**Implementation plan:**
- Transform: `fn_*` blocks already emitted, stop skipping them in codegen
- Call site: save `argc`/`argv`, set new positional params, `Goto(fn_entry)`
- Return: `Goto(return_site)`, restore `argc`/`argv`
- `local x`: transform renames to `funcname__x`
- `$(func)`: emit `fn_foo__capture` variant with buffer writes (option A)

## Design Decision #28: No word-split loops, no glob loops

**Context:** `for x in $(cmd)` and `for x in *` are two of bash's
most dangerous patterns. The first word-splits command output on IFS
(spaces in filenames break it, buffering scales poorly). The second
expands globs at runtime with behavior that varies by `nullglob`,
`failglob`, `dotglob` settings.

**Decision:** Both forms are coerce-mode rewrites (SB4020, SB4021):

- `for x in $(cmd)` → `cmd | while read -r x` — streams one line
  per iteration, no word splitting, handles arbitrary filenames.
- `for x in *` → `find ... | while read -r x` or internal
  `sash_readdir` — same streaming model, no glob ambiguity.

**Rationale:** The pipe-while-read form is strictly superior:
1. Streams (O(1) memory vs O(n) for command substitution buffering)
2. One entry per iteration (no word-split bugs)
3. Handles filenames with spaces, newlines, etc.
4. Makes the splitting rule explicit (newline-delimited)

The safe subset eliminates word splitting entirely (DD#22). These
coerce rules are the for-loop consequence of that decision.

**New runtime needed:** `sash_read_line(fd) → (ptr, len, eof)` for
the `while read` loop body.

### Design Decision #29: Coerce-on-failure, not separate tool

**Context:** Previously planned a standalone `sash-coerce` tool as a
separate pipeline step. Reconsidered.

**Decision:** `sash` is one tool. On compilation failure due to unsafe
bash, it emits `script.sash` — a valid bash script with:

1. **Safe coercions applied** as code changes, each annotated with a
   comment citing the rule (e.g., `# [SC2086] unquoted → quoted`).
2. **Rejected lines left as-is**, with an annotation comment above
   (e.g., `# [SB0001] REJECT: eval with dynamic argument`).

The `.sash` file is always valid bash that runs identically to the
original. The only code changes are safe mechanical rewrites. Rejected
lines are untouched — the user must delete or rewrite them manually.

**Workflow:**
1. `sash script.sh -o binary` → fails, emits `script.sash`
2. `diff script.sh script.sash` → review coercions, resolve rejects
3. `sash script.sash -o binary` → succeeds or shows remaining rejects
4. Repeat until clean.

**Exit codes:** 0 = compiled. 1 = coerced file emitted (all auto-fixable).
2 = coerced file with rejects (needs human). 3 = unparseable.

**Rationale:** One tool, not two. The diff is the audit trail. Rejected
code stays uncommented so recompilation fails on it again — convergent
workflow. The `.sash` file is the artifact the security auditor reviews.

**Design Decision #30: Strict type separation — no implicit conversion**

Problem: Bash variables have dual personality — `x=3` is simultaneously a
string "3" and the integer 3. Perl's SVs had the same problem. Convenient
but harmful to correctness — you can never be sure what type you're
operating on.

Decision:
1. A variable is int or string, decided at declaration, never changes.
2. `int x=$1` is the explicit conversion boundary (compiler inserts atoi).
   No quotes: `int x="$1"` is wrong (quotes signal string context).
3. `echo "$x"` on an int var is the output boundary (compiler inserts itoa).
4. `(( ))` and `$(( ))` require all operands to be int-typed. String vars
   in arithmetic context are a **compile error**, not an implicit conversion.
5. Variables become int via: `int x=...` or `declare -i x` only.
   `x=$((expr))` is coerced to `int x=$((expr))` — the `int` keyword
   must be explicit. Intrinsics (`$#`, `$?`) are always int.
6. `-eq`/`-ne`/`-lt`/`-le`/`-gt`/`-ge` are **rejected** inside `[[ ]]`.
   Coerced to equivalent `(( ))` expressions.
7. `[[ ]]` is string/file/boolean only. `(( ))` is arithmetic only.
   No operator set overlap.
8. Mixed conditions use `&&`/`||` interposition:
   `if (( count > 0 )) && [[ -n "$name" ]]; then`
9. The coercer rewrites `x=0` → `int x=0` when the variable is used in
   arithmetic context, and `[[ $x -gt 3 ]]` → `(( x > 3 ))`.

Type behavior matrix:

| Context             | String var        | Int var           |
|---------------------|-------------------|-------------------|
| `echo "$x"`         | direct write      | itoa at use site  |
| `(( x + 1 ))`      | **compile error** | direct i64 add    |
| `$((x + 1))`       | **compile error** | direct i64 add    |
| `[[ $x -gt 3 ]]`   | **compile error** — coerce to `(( ))` | N/A |
| `[[ $x = "foo" ]]` | direct compare    | **compile error** |
| `int y=$x`          | atoi at boundary  | direct copy       |
| `str s=$x`          | direct copy       | itoa at boundary  |

Boundary conversion pattern:
```bash
int count=$1          # atoi at the boundary (argv → int)
int offset=$2         # atoi at the boundary
(( count += offset )) # pure int, no conversion
echo "$count"         # itoa at the boundary (int → stdout)
```

`[[ ]]` retains: `=`, `==`, `!=` (string equality), `<`, `>` (lexicographic),
`-z`, `-n` (empty/nonempty), `-f`, `-d`, `-e`, `-r`, `-w`, `-x` (file tests),
`&&`, `||`, `!` (boolean), `=~` (regex, future).

`(( ))` owns all of: `==`, `!=`, `<`, `<=`, `>`, `>=`, `+`, `-`, `*`, `/`,
`%`, `++`, `--`, `+=`, `-=`, `*=`, `/=`, `&&`, `||`.

Coercion examples:
```bash
# Before:                        # After:
x=0                              int x=0
while [[ $x -lt 10 ]]; do       while (( x < 10 )); do
  (( x++ ))                        (( x++ ))
done                             done

[[ $count -gt 0 ]] && ...       (( count > 0 )) && ...
```

Rationale: Every atoi/itoa is visible at a declaration or output boundary.
No dual personality. The compiler enforces the type at every use site.
Perl taught us that implicit conversion is convenient until it isn't —
safebash chooses correctness over convenience. The Perl `SV` was
simultaneously PV (string), IV (integer), NV (float) — silent conversion
meant you could never be sure what type a variable held.

**Amendment (2026-04-19): Bare numeric assignment is a hard error**

`x=5` without a type keyword is ambiguous — is `5` a string or an integer?
The compiler refuses to guess. This is NOT a coerce rule — the coerce-then-
recompile workflow could silently pick the wrong type. Hard errors force
explicit intent:

    int x=5     ← integer 5
    x="5"       ← string "5"
    x=5         ← COMPILE ERROR: ambiguous, use 'int x=5' or x="5"

The principle: coerce when intent is clear (e.g., quoting an unquoted $var),
error when intent is ambiguous (two valid interpretations with different
runtime behavior). Bare numeric literals without type annotation fall in
the ambiguous category.

Non-numeric bare words are unambiguous strings: `x=hello`, `path=/usr/bin`.
Only unquoted literals matching `^[0-9]+$` trigger the error.



### Design Decision #31: `(( ))` mutations and comparisons are syntactically disjoint

Problem: In bash, `(( x = 0 ))` returns exit status 1 (falsy). Under
`set -e`, this kills the script. The idiom `((x++,1))` works around it
but is non-intuitive. `let` has the same problem. The root cause: `(( ))`
conflates mutation (side effect) and test (exit status) in a single
construct.

Decision: Safebash separates the two roles syntactically:
- **Statement position** (bare `(( ))`) — only mutations allowed: `=`,
  `+=`, `-=`, `*=`, `/=`, `++`, `--`. Exit status is always 0.
- **Condition position** (`if (( ))`, `while (( ))`) — only comparisons
  allowed: `==`, `!=`, `<`, `<=`, `>`, `>=`, `&&`, `||`. Produces boolean.
- `(( x > 0 ))` as a bare statement — **compile error** (computed boolean
  discarded).
- `if (( x++ ))` — **compile error** (mutation in condition position).

The errexit footgun is eliminated entirely. There is no ambiguous case
where `(( ))` could silently produce a nonzero exit status from a mutation.
`let` is rejected and coerced to `(( ))` or `int x=`.

### Design Decision #32: Honest trace + embedded source

**Problem:** Bash's `set -x` is the primary debugging tool for shell
scripts. Compiled binaries lose this. How do we give equivalent or
better debugging for compiled scripts?

**Resolution:** Two features, both optional:

1. **Embedded source.** The original bash source is stored in a
   `.sash_source` ELF section. Extractable via `sash --extract`
   or `objcopy`. Default on, `--strip` to remove.

2. **Honest trace.** At each decision point (branch, test, command),
   the binary can emit a trace line to stderr showing:
   - Source file and line number (from Span)
   - The evaluated condition with **actual runtime values**
   - Which branch was taken

   Example:
   ```
   + git-ssh:5: [[ $? != 2 ]] → ?=0, true
   + git-ssh:7: agent_ok || . → succeeded, skipping right
   + git-ssh:9: ! agent_ok → ?=0, true → negated → false
   ```

   Activated via `SASH_TRACE=1` at runtime. Compiled in by default,
   stripped with `--no-trace`. Branch predictor eliminates cost when
   inactive.

**Why not just `set -x`:** Bash's trace shows commands as expanded
text — what the source *says*. The honest trace shows evaluated
values and branch outcomes — what the binary *decided*. If there's
a codegen bug, bash-style trace is misleadingly reassuring (shows
"correct" source text while executing wrong code). Honest trace
shows the actual runtime values that drove the decision, making
codegen bugs visible.

**Why not just DWARF/gdb:** DWARF is powerful but requires gdb
expertise. Honest trace + strace gives full visibility to anyone
who can read shell scripts:
- Honest trace → decisions and values
- strace → syscalls and I/O

The combination is strictly more powerful than `set -x` alone.

**Rejected alternative:** Compile-time-only trace (baked strings
showing source text, no runtime values). This was the "misleadingly
reassuring" approach — the trace looks correct even when execution
is wrong.

### Design Decision #33: Unified hash table for arrays

**Context:** Bash has two array types — indexed (`declare -a`) and
associative (`declare -A`). The current safebash implementation uses
stack-allocated immutable lookup tables for associative arrays only.
Both target scripts (unittest, flash) need mutable arrays.

**Decision:** One runtime data structure (`sash_table`) for both.

**Design:**
- **String keys always.** `arr[5]` → key `"5"`. `map["foo"]` → key
  `"foo"`. No type distinction at the runtime level.
- **Values are `(ptr, len)` pairs.** For string values, `ptr` points
  to string data and `len` is byte count. For int values, `ptr` is
  null and `len` IS the integer. Zero conversion overhead.
- **`declare -a` vs `-A` is a compile-time type check**, not a runtime
  distinction. The validator catches "declared indexed, used string key."
- **No stored iteration order.** Integer iteration uses C-style for
  loop incrementing a lookup key. Associative iteration returns keys
  in hash-table order (unspecified, matches bash `-A` semantics).
- **Open-addressing hash table** with linear probing, FNV-1a hash,
  power-of-2 sizing, grow at 75%.

**Rationale:**
- One implementation, one set of bugs, one set of tests.
- Sparse integer keys are free (port 8080 doesn't allocate 8081 slots).
- The `len`-as-int-value trick avoids atoi/itoa at storage boundaries,
  consistent with DD#30 (conversion at output, not at storage).
- Lua demonstrated that unified tables work; our addition is compile-time
  type checking to catch accidental key type mixing.

**Operations:** `sash_table_new`, `sash_table_set`, `sash_table_get`,
`sash_table_len`, `sash_table_keys`. ~100 lines of `#![no_std]` Rust.

**Amendment:** Integer keys are hashed as raw 8-byte `i64` values,
not converted to strings via itoa. The hash function operates on
bytes — it doesn't care whether they represent a number or a
character sequence. Integer key lookup is: hash 8 bytes, probe,
memcmp 8 bytes. No conversion overhead.

**Amendment 2:** Adopt Python compact dict layout. Two arrays:
- `indices`: sparse, power-of-2, stores entry offsets (i32, -1 = empty)
- `entries`: dense, append-only, stores (key_ptr, key_len, val_ptr, val_len)

Benefits at ~20 lines extra cost: cache-friendly iteration (dense scan),
cheap resize (rehash indices only, entries stay put), memory efficient
(empty slots cost 4 bytes not 32), insertion-order iteration for free.
Lookup adds one indirection (index → entry offset → entry).

### Design Decision #34: Minimize unsafe in sash-rt

**Context:** The sash-rt runtime is ~1000 lines, all currently `unsafe`.
Most of it is pure data manipulation (hashing, string ops, parsing)
that happens to use raw pointers because `extern "C"` entry points
provide them and `#![no_std]` lacks `Vec`/`String`.

**Decision:** Thin unsafe boundary at each `extern "C"` entry point.
Convert raw pointers to slices immediately, then call safe Rust for
the core logic.

**What must remain unsafe:**
- Syscalls (kernel boundary)
- `extern "C"` function signatures (raw pointers from codegen)
- `sash_alloc` (bump allocator, raw pointer arithmetic)
- `_start` (entry point, stack setup)

**What becomes safe:** ~80% of the runtime — table operations,
string manipulation, number conversion, parameter expansion,
file parsing, string comparison.

**Cost:** Negligible. Release builds: LLVM eliminates most bounds
checks. Remaining checks are one predicted-not-taken branch.
Development: refactor `ptr.add(i)` to `slice[i]`, ~1 hour.

**Benefit:** Audit surface shrinks from ~1000 lines to ~200 lines.
Safe Rust code is guaranteed free of memory corruption — reviewers
can skip it. For a project whose value proposition is "mechanical
rewrite passes your security audit," the runtime should hold itself
to the same standard.

**Design Decision #35: `int`, `map`, `array` as first-class type keywords**

The bash pattern of `declare -i/-a/-A` is replaced by purpose-built keywords:

- `int x=5` — integer variable (DD#30)
- `map config=([host]=foo [port]=8080)` — associative array (string keys)
- `array files=(alpha beta gamma)` — indexed array (integer keys)

`declare` becomes optional decoration. The coercer rewrites:
- `declare -i x` → `int x`
- `declare -a arr` → `array arr`
- `declare -A map` → `map map`
- `declare x=5` → `x=5`

`local` composes with type keywords:
- `local int count=0` — scoped integer
- `local map cache=(...)` — scoped associative array
- `local array parts=(...)` — scoped indexed array

`local` controls scope (DD#27 function-scoped renaming).
Type keyword controls representation. They are orthogonal.

`local` without a type keyword defaults to string (`local x="hello"`).
`local` outside a function is a validator error.

**Design Decision #36: Fixed-point arithmetic via `dtoi`/`itod`**

Problem: Bash has no floats. Scripts needing fractional values (`sleep 0.5`,
formatted percentages, version numbers) shell out to `bc`/`awk`/`python` —
a fork+exec for simple decimal string arithmetic.

Solution: Two boundary conversion functions with explicit scale:

    dtoi(digits, string) → int    # "19.99" with digits=2 → 1999
    itod(digits, int) → string    # 1999 with digits=2 → "19.99"

Rules:
- `digits` specifies decimal places (compile-time constant in practice)
- Error if fractional part exceeds `digits` precision (no silent truncation)
- Error if input isn't a valid decimal number
- Overflow traps per DD#26
- `itod` always produces exactly `digits` decimal places: itod(2, 5) → "0.05"

Arithmetic at same scale is direct:
    int a=$(dtoi 2 "19.99")    # 1999
    int b=$(dtoi 2 "5.50")     #  550
    (( total = a + b ))         # 2549
    echo "$(itod 2 $total)"    # "25.49"

Multiplication/division require manual scale correction — the programmer
manages precision explicitly. No hidden rounding, no IEEE 754 surprises.

This is NOT a float type. No `declare float`. No new IR nodes or codegen.
`dtoi`/`itod` are runtime builtins alongside `atoi`/`itoa` — ~30 lines each.

Rationale: Reducing external dependencies for common cases is a win.
Convenience at near-zero cost is a win. Every fork+exec eliminated for
`sleep $computed_delay` or `printf "%.2f"` makes compiled scripts faster
and more self-contained.

### Design Decision #37 — Reject process substitution <(cmd)

Process substitution `<(cmd)` is rejected. Two coerce paths:

1. **Streaming input:** `while read < <(cmd)` → `cmd | while read -r var`
   (pipe-while-read, DD#28). Safebash's pipe version runs the loop in the
   parent process, so the bash motivation for `< <(cmd)` (avoiding subshell
   variable loss) doesn't apply.

2. **Multi-fd substitution:** `diff <(cmd1) <(cmd2)` → explicit `mkfifo`
   with named pipes. More verbose but auditable — no magic `/dev/fd` paths.

Rule: SB4025: process substitution rejected, coerce to pipe-while-read
or mkfifo.

### Design Decision #24 amendment: Bless syntax finalized

**Date:** 2026-04-19

**Amendment to DD#24 (candidate → adopted)**

Bless is a **declaration + optional validation** at the trust boundary.
The variable name is always stated explicitly (like `export`, `readonly`).
The test is optional.

**Syntax:**

```bash
bless VAR                              # bare — declare, no validation
bless VAR [[ -v VAR ]]                 # existence check (was key provided?)
bless VAR [[ $VAR =~ ^/ ]]            # content validation
bless VAR (( $VAR > 0 ))              # arithmetic validation
if bless VAR [[ -v VAR ]]; then       # conditional — take else if absent
    ...
else
    VAR="/tmp/default"
fi
bless VAR [[ -v VAR && $VAR =~ ^/ ]]  # combined existence + content
```

**Naming rule:** `bless` always takes the variable *name* (no `$`). Inside
the `[[ ]]` or `(( ))` test, normal shell conventions apply — `$` for
value expansion, bare name for `-v` existence check.

**Multi-variable clarity:** The explicit name removes ambiguity:

    bless X [[ $X =~ $Y ]]    # X is blessed. Y must already be blessed.

**`-v` at bless boundary only:** The `-v` test checks whether a key was
provided by the environment or sourced file. It is only meaningful at the
bless boundary — beyond it, all variables are always set (possibly empty).
`[[ -v VAR ]]` outside a bless context is a compile error: "all variables
are always set after declaration."

**No "unset" beyond bless:** After the bless point, the variable is in the
program's typed world. It has a value (possibly empty string). There is no
"undefined" state to check. The Perl lesson: `defined` matters at the
import boundary, not throughout the program.

**Colon/no-colon distinction eliminated:** Since safebash has no runtime
"unset" concept (only empty-or-not), the without-colon forms (`${x-d}`,
`${x+a}`, `${x=d}`, `${x?e}`) are equivalent to the with-colon forms
(`${x:-d}`, `${x:+a}`, `${x:=d}`, `${x:?e}`). Safebash accepts both
spellings and treats them identically. The coercer normalizes to the
colon form.

**Security model:** `bless` is a documentation/audit construct, not a
security enforcement mechanism. The compiler's job:
1. Refuse to use unblessed external data (catches accidental use)
2. Record the bless point (auditors grep for `bless`)
3. Accept whatever validation the programmer provides (including none)

The security value is making every external data entry point visible
and intentional. A compiled binary has no eval, no shell expansion —
a string like `"; rm -rf /"` is just bytes, dangerous only if passed
to `exec`. The compiler can track that statically.

**`unset` rejected:** Variables don't go back to undefined once declared.

### Design Decision #38: Reject subshells, `&` is the fork mechanism

**Date:** 2026-04-19

**Decision:** Subshell syntax `( cmd )` is rejected. The `&` operator is
safebash's only fork mechanism, extended to work on both external commands
and shell functions.

**Rationale:** Subshells in bash exist for isolation — temporary `cd`,
temporary env, temporary variable changes. The mechanism is `fork`, which
is expensive. The three subshell use cases have better alternatives:

- `(cd dir && cmd)` → use absolute paths, `-C` flags, or `build_fn &`
  (fork gives natural process isolation, cd in child doesn't affect parent)
- `(export VAR=x; cmd)` → `VAR=x cmd` (per-command env, already works)
- `(exec cmd)` → `cmd &` (background, explicit)

**`&` on shell functions:** `my_func &` forks the process. The child
jumps to the function's trampoline blocks. The parent gets `$!` (child
pid) and continues. `wait` or `wait $pid` collects the child.

**mkfifo pattern (DD#37):**

```bash
produce_left()  { cmd1 > /tmp/fifo1; }
produce_right() { cmd2 > /tmp/fifo2; }

mkfifo /tmp/fifo1 /tmp/fifo2
produce_left &
produce_right &
diff /tmp/fifo1 /tmp/fifo2
wait
rm /tmp/fifo1 /tmp/fifo2
```

**No `fork` builtin needed.** `&` already means "run in background" —
extending it to functions is natural. No new syntax, no `$?` overloading,
no subshell scoping rules.

**Lifecycle:** `$!` stores last background pid. `wait` collects all
children. `wait $pid` collects a specific child. Same as bash.

### DD#39: Pattern strategy — star-only glob + regex mutation (2026-04-19)

**Problem:** Bash glob syntax (`*`, `?`, `[abc]`, extglob) is a mediocre
pattern language trapped between simple and powerful. Parameter expansion
patterns (`${var#pat}`, `${var##pat}`, `${var%pat}`, `${var%%pat}`,
`${var/pat/repl}`, `${var//pat/repl}`) use the same glob syntax for
string manipulation — six operators with subtle semantics. And most
bash users never learn them, reaching for `$(basename "$var")` instead
(which forks).

**Resolution:** Split into simple and powerful:

1. **Star-only glob for files.** `*` is the only glob character.
   No `?`, `[abc]`, or extglob. Works inline (`ls *.sh`), in array
   assignment (`arr=(*.sh)`), and with regex filter (`arr=(* =~ '\.sh$')`).
   All three are readdir underneath. Star matching is ~10 lines of Rust,
   not a pattern engine.

2. **Perl-style regex for string mutation.** `var =~ s/re/str/flags`
   replaces the entire `${var#pat}` family. In-place mutation. Sed/Perl
   muscle memory — more people know this than `${var##*/}`.

3. **Boolean regex test** (existing): `[[ $var =~ re ]]`.

4. **One pattern engine:** regex. Already in the binary for `=~`.

**What's dropped:**
- `?` and `[abc]` glob characters — use `arr=(* =~ re)` instead
- `${var#pat}`, `${var##pat}`, `${var%pat}`, `${var%%pat}` — coerce to `var =~ s/re/str/`
- `${var/pat/repl}`, `${var//pat/repl}` — coerce to `var =~ s/re/str/` and `var =~ s/re/str/g`
- extglob (`@()`, `+()`, `!()`, etc.)

**Migration examples:**
- `${path##*/}` (basename) → `path =~ s/.*\///`
- `${file%.*}` (strip ext) → `file =~ s/\.[^.]*$//`
- `${url#*://}` (strip proto) → `url =~ s/^[^:]*:\/\///`
- `${var/foo/bar}` → `var =~ s/foo/bar/`
- `${var//foo/bar}` → `var =~ s/foo/bar/g`

**File matching layered progression:**
```
ls *.sh                    # inline star glob — 90% case
arr=(*.sh)                 # star glob to array
arr=(* =~ '\.sh$')         # regex filter — full power
```

**Key insight:** The population that knows sed/Perl regex vastly
exceeds the population that knows `${var##*/}`. Choosing regex as the
one pattern syntax picks the more popular option, not the less.

### DD#20 amendment: glob options superseded by DD#39 (2026-04-19)

DD#39 supersedes DD#20's glob-related options:

- **extglob** — was "always on." Now rejected. `@()`, `+()`, `!()` 
  not in safe subset. Use `arr=(* =~ 're')` regex form instead.
- **globstar** (`**` recursive matching) — was "always on." Now 
  rejected. Use `find` for recursive directory traversal (DD#28 spirit).
- **nullglob** — was "on in iteration contexts." Now moot — iteration
  goes through arrays (`arr=(*)`), programmer checks `${#arr[@]}`.
- **failglob** — was "on in argument contexts." Still applies —
  `ls *.sh` with no matches is a compile-time or runtime error,
  not a silent pass-through of the literal glob string.
- **Context-sensitive glob behavior** — simplified. Only inline star
  globs exist in argument position, and they always failglob.

All other DD#20 options (dotglob off, nocaseglob off, etc.) remain
unchanged.

### DD#40: Shellcheck as lint gate, validator scope reduction (2026-04-20)

**Context:** The safebash validator (exp 007-011) implements 7 CST-level
rules. 60+ additional rules are specified in SUBSET.md but unimplemented.
Meanwhile, the compiler's value model (typed variables, no IFS splitting,
no glob expansion) eliminates the root cause of most bash quoting bugs.
The validator is doing work the compiler already makes unnecessary.

**Decision:** Reduce the validator to **subset enforcement only** — rules
that reject constructs sash literally cannot compile. Delegate bash-level
linting to shellcheck, run by default.

**Validator scope (4 rules):**

| Rule | Code | Action |
|------|------|--------|
| `eval` (variable args) | SB4001 | reject |
| `source` / `.` | SB4002 | marker (compiled to kvp loader) |
| `${!var}` indirect expansion | SB4003 | reject |
| `$cmd` as command name | SB4004 | defer to SSA; reject if unresolvable |

**Removed from validator (now handled by compiler or shellcheck):**

| Rule | Former action | Why removed |
|------|--------------|-------------|
| Backticks → `$()` | coerce | Parser handles both; compiler compiles both |
| `[ ]` → `[[ ]]` | coerce | Sash has no word splitting; `[ ]` compiles identically |
| `$[expr]` → `$((expr))` | coerce | Same arithmetic either way |

**Quoting rules (SC2086 et al.) — not implemented, never will be:**
Sash's value model means `$var` and `"$var"` are semantically identical.
No word splitting, no glob expansion, empty variables always occupy one
argument slot (matching `"$var"` bash semantics). Quotes are accepted
but never required.

**Coercions become silent transforms:** Constructs like `for x in $(cmd)`
are silently rewritten to pipe form. The intent is unambiguous ("iterate
over cmd output"), and the transformation is safe. No diagnostic emitted.

**Shellcheck integration:**

| Invocation | Behavior |
|------------|----------|
| `sash script.sh` | Run shellcheck, then compile. If shellcheck not installed: warning, compile anyway |
| `sash --nolint script.sh` | Compile directly, skip shellcheck |
| `sashn script.sh` | Alias for `sash --nolint` — sash-native code |

Shellcheck catches genuine intent-mismatch bugs (did the programmer
want word splitting, or forget quotes?) that sash's compiler silently
handles correctly. It's a pre-flight safety net for bash scripts being
ported to sash. Native sash code skips it.

**Shellcheck configuration:** Use `enable=all` with a curated disable
list. Start minimal (e.g., `disable=SC2248,SC2250`), suppress additional
rules as false positives for sash are discovered.

**Rationale:** A sash-compiled binary is **strictly safer** than the same
script run by bash, regardless of quoting discipline. Sash eliminates
word splitting, glob expansion, and disappearing empty strings by
construction. Shellcheck adds defense-in-depth for ported scripts but
is not required for correctness.

DG directed the design. AI implemented the analysis and writeup.

---

### DD#41: Sash-native array syntax (2026-04-20)

**Context:** Bash array syntax is notoriously noisy: `"${arr[@]}"`,
`${#arr[@]}`, `${!arr[@]}`, `arr+=("val")`. The sigils, braces,
quotes, and `@` are all necessary in bash to prevent word splitting
and glob expansion. In sash, none of these hazards exist. The ceremony
is pure overhead.

**Decision:** Introduce clean array syntax for sash-native code. Old
bash syntax continues to work — scripts compile unchanged.

**New syntax:**

| Sash syntax | Meaning | Bash equivalent |
|-------------|---------|----------------|
| `@arr` | all elements (list context) | `"${arr[@]}"` |
| `$arr[i]` | element by index | `"${arr[i]}"` |
| `$arr[key]` | element by key (maps) | `"${arr[key]}"` |
| `arr[i]=val` | assign element | `arr[i]=val` (same) |
| `arr+=(val)` | append element | `arr+=("val")` (same, keep parens) |
| `len arr` | array length | `${#arr[@]}` |
| `len $var` | string length | `${#var}` |
| `keys arr` | all keys (iterable) | `"${!arr[@]}"` |
| `join @arr` | join with spaces | (no bash equivalent) |
| `join sep @arr` | join with separator | (no bash equivalent) |

**Sigil rules:**

- `$` = expand a scalar value (variable or array element). Required
  everywhere — in command args and inside double-quoted strings.
- `@` = expand all array elements as a list. Valid only in **list
  contexts**: `for` loop words, command arguments (each element becomes
  a separate argument), function call arguments. **Not valid inside
  double-quoted strings** — use `$(join @arr)` instead.
- Bare name = for builtins that operate on the **container itself**:
  `len arr`, `keys arr`, `array arr`, `map m`.

**Element access (`$arr[i]`) — lowerer recognition, not grammar change:**

Bash requires braces (`${arr[i]}`) because `$arr[i]` would be parsed
as `$arr` (simple expansion) + `[i]` (glob pattern / word). Tree-sitter
parses `$arr[0]` as a `concatenation` of `simple_expansion($arr)` +
`word([0])`. Rather than modifying the grammar and fighting the external
scanner's CONCAT behavior, the lowerer pattern-matches this CST shape:
when it sees a concatenation of `$name` + `[expr]` and `name` is a known
array variable, it interprets it as subscript access. This is consistent
with how the existing grammar changes were evaluated — grammar changes
are justified when they affect node kind selection (e.g., `fn` keyword,
`int`/`map`/`array` declarations). CST reinterpretation in the lowerer
is appropriate when the node kinds are correct but the semantic meaning
needs compiler type knowledge to resolve.

Similarly, `@arr` parses as a plain `word` token. The lowerer recognizes
words starting with `@` and checks if the remainder is a known array.

**String interpolation:**

`@arr` is not valid inside double quotes. This prevents implicit
joining with an unspecified delimiter (space? newline? comma?).
Array-to-string conversion is always explicit:

```bash
echo "colors: $(join ', ' @colors)"   # explicit delimiter
echo "all: $(join @colors)"           # default: space
```

`$arr[i]` works inside double quotes: `"color is $colors[0]"`.

**Builtins (`len`, `keys`, `join`):**

These are sash builtins compiled to inline code, not external
commands. They take array names (not expansions) as arguments,
similar to `declare` and `unset`.

- `len arr` — compiles to `sash_table_len` call
- `len $var` — compiles to string length
- `keys arr` — produces iterable list of keys (for-loop compatible)
- `join @arr` — concatenates elements with space delimiter
- `join sep @arr` — concatenates with custom separator

**Backward compatibility:** All existing bash array syntax continues
to work. `"${arr[@]}"`, `${arr[i]}`, `${#arr[@]}`, `${!arr[@]}`,
`arr+=("val")` compile identically. The new syntax is additive.

**Example — bash vs sash-native:**

```bash
# Bash style (still works)
declare -a colors=("red" "green" "blue")
for c in "${colors[@]}"; do
    echo "$c"
done
echo "${#colors[@]} colors"

# Sash-native style
array colors=("red" "green" "blue")
for c in @colors; do
    echo $c
done
echo "$(len colors) colors"
```

DG directed the design. AI implemented the analysis and writeup.

### DD#42: Environment variables and positional params as builtins (2026-04-21)

**Problem:** Bash's model of "variables just appearing" (`$HOME`, `$1`) is
incompatible with sash's explicit declaration model. Every variable should
have a visible origin. And the taint system (DD#24) needs to track external
data boundaries — undeclared variables referenced in the script could be
env vars or positional params, both externally-controlled.

**Decision:** Replace implicit magic variables with explicit builtins:

| Sash mode | Bash equivalent | Status |
|-----------|-----------------|--------|
| `$(getenv HOME)` | `$HOME` | Tainted result |
| `$(argv 1)` | `$1` | Tainted result |
| `$1` in function | `$1` in function | Clean — compiler sees call site |

In `sash` mode (bash compat), the compiler silently coerces `$HOME` →
`$(getenv HOME)` and `$1` → `$(argv 1)`. In `sashn` mode (native), the
bare forms are compile errors.

Return status: `$(getenv HOME) || { echo "not set"; exit 1; }` — the
`|| exit` pattern handles missing env vars. No null/unset concept.

This eliminates `-v` from the design — variable existence is handled by
return status, not a test operator.

DG directed the design. AI implemented the analysis.

### DD#43: Revised bless semantics — assign then validate (2026-04-21)

**Problem:** DD#24 originally had two formulations for `bless`:
(A) assign tainted value first, then bless validates it, and
(B) bless captures value from expression and binds it.

Formulation B requires the bless expression to be both a test and a
value source, creating ambiguity (which subexpression provides the value?).

**Decision:** Formulation A — assign then bless.

```bash
port=$(argv 1) || { echo "usage: $0 port"; exit 1; }  # assign + existence check
bless port [[ $port =~ ^[0-9]+$ ]]                      # validate shape
echo $port                                               # clean
```

Assignment is where you distinguish existence (return status).
Bless is where you validate shape (boolean test).

The rule: **you can write tainted data into a variable (assignment is a
write context). You cannot read it except inside `bless`.** Bless evaluates
one boolean expression and flips the taint bit on success.

`bless VAR EXPR` — VAR is explicitly named (not inferred from expression).
Compiler verifies VAR appears in EXPR.

Three forms by context:
- `bless VAR [[ ... ]]` bare → fatal exit on failure
- `if bless VAR [[ ... ]]` → falsy, take else branch
- `:~` operator → deferred for later (sugar)

`[[ -v VAR ]]` removed from design. Existence is handled by getenv/argv
return status at the assignment boundary, not by a test operator.

DG directed the design. AI implemented the tracker.

### DD#44: Taint quarantine — full enforcement (2026-04-21)

**Problem:** Should sash gate tainted data at "dangerous" use sites only
(command names, paths) or at all use sites?

**Decision:** All use sites. The rationale: `echo $var` piped to
`xargs rm -rf` — the echo is the dangerous part. You can't know what's
downstream. The only safe position is: tainted data cannot be used at all
until blessed. Full stop.

Taint categories:
- **Tainted:** positional params, read targets, $(external cmd), env vars
- **Clean:** literals, arithmetic, $0/$#/$?/$$, assigned from clean exprs
- **Blessed:** was tainted, passed bless validation

Flow-sensitive tracking: taint status changes per-variable as the program
executes. `read -r x` taints x. `bless x [[ ... ]]` blesses x. Second
`read -r x` re-taints x.

Assignment propagation: `x=$(tainted_cmd)` → x is tainted. The assignment
itself is not an error (write context). `echo $x` after is the error (read
context).

Implemented in exp 045 with 68 tests covering: basic taint/clean, bless
untainting, getenv/argv builtins, assignment propagation, re-tainting,
case/if/while contexts, nested cmd subst, string interpolation, multiple
vars, if-bless branching.

DG directed the design (especially the "echo is dangerous" insight and
the assignment-as-write-context rule). AI implemented the tracker.

### DD#45: Source emitter — coerce in lowerer, emit from CST (2026-04-22)

**Problem:** Two features need source output: `--emit-source` (source-to-source
translation tool) and `.sash_source` ELF section (embedded in compiled binary for
`sash --extract`). Both should produce identical output. How?

**Resolution:** The lowerer is the single coercion site. The source emitter
operates during the CST walk, not by reconstructing from IR.

**Mechanism:** A `SourceEmitter` tracks position in the original source bytes.
As the lowerer walks CST nodes:

1. **No coercion needed:** copy `source[start..end]` verbatim — preserving
   comments, whitespace, formatting, blank lines.
2. **Coercion needed:** emit the sash-native form, and append
   `# was: <original text>` as annotation.
3. **Gaps between nodes** (comments, blank lines): copy verbatim from the
   byte range between the last emitted node and the current one.

The emitter is `Option<&mut SourceEmitter>` in the lowerer — when `None`,
zero overhead.

**Coercions owned by the lowerer (source-level syntax changes):**

| Bash syntax | Sash native | Notes |
|---|---|---|
| `let "x=1"` | `(( x=1 ))` | Same semantics, cleaner |
| `declare -i x` | `int x` | DD#30 |
| `declare -a arr` | `array arr` | DD#41 |
| `declare -A m` | `map m` | DD#41 |
| `${var/pat/rep}` | `${$var =~ s/re/str/}` | DD#39 amendment, expression form |
| `${var//pat/rep}` | `${$var =~ s/re/str/g}` | DD#39 amendment |
| `${var/#pat/rep}` | `${$var =~ s/^re/str/}` | DD#39 amendment |
| `${var/%pat/rep}` | `${$var =~ s/re$/str/}` | DD#39 amendment |
| `${var#pat}` | `${$var =~ s/^re//}` | DD#39 amendment |
| `${var##pat}` | `${$var =~ s/^re.*//}` | DD#39 amendment, greedy |
| `${var%pat}` | `${$var =~ s/re$//}` | DD#39 amendment |
| `${var%%pat}` | `${$var =~ s/.*re$//}` | DD#39 amendment, greedy |
| `` `cmd` `` | `$(cmd)` | Backtick elimination |
| `declare -x var` | **error** | Cannot coerce export (DD#45 amendment) |

**Not coercions (stay in transform, invisible to source):**
- SSA construction (variable versioning, phi nodes, block wiring)
- Arithmetic constant folding (`$((2+3))` → 5 at compile time)
- `unset var` → assign empty (compilation strategy, not syntax change)
- `break`/`continue`/`return` → terminators (control flow lowering)
- `read` flag parsing → runtime call setup
- `trap` → signal handler wiring

**Key principle:** If the construct has a different *spelling* in sash-native
vs bash, it's a lowerer coercion. If it has the *same spelling* but different
*compilation*, it stays in transform.

**Outputs are identical:** `--emit-source` writes the emitter buffer to stdout.
Compilation writes the same buffer to a `.sash_source` ELF section. `sash --extract`
dumps the section. Round-trip invariant: compiling the emitted source produces
identical binary output.

**Replaces DD#32 embedded source** design (which was described but not
implemented). Honest trace (DD#32's other half) is orthogonal and unchanged.

### DD#45 amendment: Declaration axes and const (2026-04-22)

**Four orthogonal declaration axes:**

| Axis | Options | Default |
|---|---|---|
| Mutability | `const` | mutable |
| Lifetime | `local` | global/inherited |
| Type | `int` | string |
| Structure | `array`, `map` | scalar |

Axes combine freely for scalars: `local const int x=5`. For collections,
`const` does not apply — arrays and maps are inherently mutable
(append, delete, update elements). A collection you never modify is a
verbose way to spell a series of constants; the compiler can detect
"never written" from the SSA graph without syntax.

**Examples:**
```
x=5                          # mutable string scalar
int x=5                      # mutable int scalar
const x="hello"              # immutable string scalar
const int x=5                # immutable integer
local x=5                    # function-scoped mutable string
local const int x=5          # function-scoped immutable integer
array arr=(1 2 3)            # mutable string array
map m=([a]=1 [b]=2)          # mutable string map
int array arr=(1 2 3)        # mutable int array
int map m=([a]=1 [b]=2)      # mutable int map
local array arr              # function-scoped mutable array
```

**`const` vs `readonly`:**
- `readonly` is a bash runtime trap — assignment fails with a cryptic error
- `const` is a compile-time check — assignment is rejected with a clear diagnostic
- The compiler can also inline `const` values as literals (optimization)
- Coercion: `readonly x=5` → `const x=5`; `declare -r x=5` → `const x=5`
- `readonly` on arrays/maps → error (no sash-native equivalent)

**`export` is an error:**
`export FOO=bar` and `declare -x FOO=bar` cannot be mechanically coerced
because the compiler doesn't know which subsequent command consumes the
variable. The sash pattern is `FOO=bar cmd` (per-command environment).
Diagnostic:
```
script.sh:5:1: error [SB5001] 'export' not supported — use FOO=bar cmd
```

**Coercion table (updated from DD#45):**

| Bash syntax | Sash native | Notes |
|---|---|---|
| `declare x=5` | `x=5` | Bare declare → plain assignment |
| `declare -i x=5` | `int x=5` | |
| `declare -a arr` | `array arr` | |
| `declare -A m` | `map m` | |
| `declare -r x=5` | `const x=5` | Scalars only |
| `declare -ri x=5` | `const int x=5` | |
| `declare -x x=5` | **error** | Cannot coerce export |
| `typeset x=5` | `x=5` | Synonym for declare |
| `local -i x=5` | `local int x=5` | |
| `local -a arr` | `local array arr` | |
| `readonly x=5` | `const x=5` | |
| `let "x=1"` | `(( x=1 ))` | |

### DD#39 amendment: Expression form of regex replace (2026-04-22)

**Problem:** DD#39 defined `var =~ s/re/str/flags` as a *statement* that
mutates in place. But `${var/pat/rep}` in bash is an *expression* — it
returns a value without modifying the original variable. When used inline
(`echo ${var/foo/bar}`), the coercion to a mutation statement requires a
temp variable. That's verbose and unnatural.

**Resolution:** Two forms of regex replace, distinguished by `$`:

```bash
var =~ s/foo/bar/           # no $ → lvalue → mutates var in place (statement)
${$var =~ s/foo/bar/}       # $var → rvalue → returns transformed value (expression)
```

This extends bash's existing convention: `var=5` assigns (lvalue),
`$var` reads (rvalue). The `=~` operator respects the same marker.

**Expression form syntax: `${$var =~ s/re/str/flags}`**

Appears anywhere an expansion can appear: command arguments, string
interpolation, assignment RHS. The original variable is unchanged.

**Full coercion table:**

| Bash expression | Sash expression |
|---|---|
| `${var/foo/bar}` | `${$var =~ s/foo/bar/}` |
| `${var//foo/bar}` | `${$var =~ s/foo/bar/g}` |
| `${var/#foo/bar}` | `${$var =~ s/^foo/bar/}` |
| `${var/%foo/bar}` | `${$var =~ s/foo$/bar/}` |
| `${var#pat}` | `${$var =~ s/^pat//}` |
| `${var##pat}` | `${$var =~ s/^pat.*//}` |
| `${var%pat}` | `${$var =~ s/pat$//}` |
| `${var%%pat}` | `${$var =~ s/.*pat$//}` |

Note on greedy: `##` and `%%` map to regex `.*` which is greedy
by default — no extra syntax needed. `#` and `%` use non-greedy
patterns (shortest match).

**Statement form coercion (unchanged from DD#39):**

When `${var/pat/rep}` appears in assignment context (`x=${var/foo/bar}`),
the emitter may coerce to:
```bash
x="$var"
x =~ s/foo/bar/
```

Or keep the expression form — compiler's choice, both compile identically.

**Coexistence:** Both forms live in sash-native syntax. The compiler
implements the same regex engine path for both. The expression form
creates a temporary copy internally; the statement form mutates SSA
variable in place.

**Grammar:** Requires tree-sitter-bash-safebash extension to parse
`${$var =~ s/re/str/flags}`. This joins `int`, `array`, `map`, `bless`
as sash-specific grammar additions.

### DD#43 amendment: arithmetic context is a read context (2026-04-22)

**Clarification:** `(( ))` and `$(( ))` are read contexts for taint
purposes. Evaluating a tainted variable in arithmetic is a compile error,
same as using it in `echo` or a command argument. DD#44 already mandates
this — "tainted data cannot be used at all until blessed" — but arithmetic
deserves explicit mention because the implicit string→int conversion
might suggest a "type boundary" that clears taint. It doesn't.

**Rationale:** `$(stat -c %s "$file")` should return `"4096"` but could
return anything — error messages, empty string, garbage. If `(( ))`
silently converts and clears taint, the failure is hidden. Non-numeric
input becomes 0 with no diagnostic.

**Correct pattern:**
```bash
fn file_size -> int {
    local tmp
    tmp=$(stat -c %s "$1")
    bless tmp [[ $tmp =~ ^[0-9]+$ ]]
    return $(( tmp ))
}
```

The flow: capture (tainted) → validate via bless (blessed) → convert
via arithmetic (safe). `bless` is the only taint-clearing construct.
`(( ))` is just arithmetic — a read context like any other.

This falls out from DD#44's "all use sites" rule. No new mechanism needed.

### DD#39 amendment 2: expression form simplified to ${var:s/re/str/} (2026-04-22)

**Problem:** The expression form `${$var =~ s/re/str/}` proposed in
the first DD#39 amendment is syntactically awkward — three pieces of
ceremony (`$` rvalue marker, `=~` operator, `s/` prefix) when any one
already disambiguates from glob. It also requires difficult tree-sitter
grammar changes: `$` inside `${}`, spaces, and `=~` tokenization in
expansion context.

**Decision:** Simplify to `${var:s/re/str/flags}`.

The colon is already the expansion operator namespace in bash.
Currently `:` routes to substring (`:offset:length`), `:-` to default,
`:+` to alternate, etc. `:s/` is unambiguous — no existing operator
starts with `s` after colon.

The `s/` prefix is the crucial indicator that regex (not glob) is in
play. It's familiar from sed, Perl, and vim. The statement form keeps
`var =~ s/re/str/flags` (mutation in place).

**Revised table:**

| Bash expression | Sash expression |
|---|---|
| `${var/foo/bar}` | `${var:s/foo/bar/}` |
| `${var//foo/bar}` | `${var:s/foo/bar/g}` |
| `${var/#foo/bar}` | `${var:s/^foo/bar/}` |
| `${var/%foo/bar}` | `${var:s/foo$/bar/}` |
| `${var#pat}` | `${var:s/^pat//}` |
| `${var##pat}` | `${var:s/^pat//}` (greedy by default) |
| `${var%pat}` | `${var:s/pat$//}` |
| `${var%%pat}` | `${var:s/pat$//}` (greedy by default) |

**Grammar impact:** small. Add a new `_expansion_body` alternative
triggered by `:s/` after the variable name. The external scanner
already handles `/` tokenization in expansion context. DG directed.

### DD#39 amendment 3: non-colon parameter expansions coerced to colon forms (2026-04-22)

**Problem:** Bash has two families of parameter expansion defaults:

| With colon (unset or null) | Without colon (unset only) |
|---|---|
| `${var:-default}` | `${var-default}` |
| `${var:=default}` | `${var=default}` |
| `${var:+alternate}` | `${var+alternate}` |
| `${var:?error}` | `${var?error}` |

The distinction only matters when a variable is set to the empty string
and you want to preserve that emptiness.

**Observation:** In sash, variables are never unset:
- Assignment always produces a value (possibly empty)
- `getenv VAR || exit` handles missing env vars via exit status
- `argv N || exit` handles missing args via exit status
- `unset var` → assigns empty string
- `read` always sets the target
- SSA catches use of undeclared variables at compile time

Since nothing is ever unset, `${var:-default}` and `${var-default}`
have identical semantics — both check "if empty."

**Decision:** Keep the colon forms (muscle memory). Silently coerce
non-colon forms to colon forms. The distinction is formally dead in
the safe subset.

DG directed: "I've been using bash since it was new and never needed
${var-default}."

### DD#21 amendment: pipeline coercion semantics (2026-04-23)

**Context:** DD#21 defines `-` prefix for error suppression. DD#20
defines pipefail as always on. This amendment clarifies how bash
pipelines are coerced to sash when the original script did or did
not use `set -o pipefail`.

**Sash-native rule:** Every pipeline segment's exit status must be
handled — checked (`if`, `&&`, `||`), explicitly ignored (`-`), or
propagated. `- cmd1 | - cmd2` is the explicit per-segment form.
No whole-pipeline suppression; think about each segment.

**Coercion from bash:**

*Script has `set -o pipefail`:* The author explicitly cared about
pipeline failures. Coerce as-is — no `-` inserted. The compiler
flags unchecked segments. Author decides.

*Script does NOT have `set -o pipefail`:* The author was operating
under bash's default — only the last segment's exit matters. The
coercer inserts `-` on every non-terminal pipeline segment:

```bash
# Original bash (no pipefail):
cat file | grep pattern | sort | head -5

# Coerced to sash:
- cat file | - grep pattern | - sort | head -5
```

This preserves the original semantics mechanically. The last segment
is checked (as it was in bash), earlier segments are suppressed (as
they were in bash). `--annotate` shows the original.

**Rationale:**
- Conservative about semantics — preserve what the bash script did
- Visible about assumptions — `-` markers make the implicit explicit
- The programmer can review and tighten, e.g., decide that `grep`
  failing should be an error even though the original never checked

**Terminal grep edge case:** `cat file | grep pattern` without
pipefail — bash continues regardless of grep's exit. The coercer
leaves the terminal segment unchecked; the compiler flags it. The
programmer wraps in `if` or adds `-`.

DG directed: "- cmd1 | - cmd2 makes me think about each segment."

## Bug fix: taint analyzer skipped function bodies (2026-04-24)

**Discovered during a TODO.md gap survey.** While steelmanning YAGNI on
"taint inheritance through function calls," DG asked the obvious
verification question: *if `getenv` is called inside a function body,
is the result tainted?*

Empirical answer: **no, it was not.** The analyzer compiled this without
diagnostic:

```bash
foo() {
    local x
    x=$(getenv USER_INPUT)
    echo "$x"
    cmd "$x"
}
foo
```

This is a validator bypass — exactly the class of finding the overdue
adversarial-audit run-2 is meant to catch. We found it without running
an audit because DG asked the right question.

### Root cause

`walk_statement` matched `function_definition` and called
`handle_function_def`, which only recorded the function name and never
descended into the body. The four DD#24 taint sources
(`getenv`/`argv`/`read`/`$(external)`) were therefore unanalyzed
anywhere inside any function body.

A misleading comment I had added the same day asserted that "parameters
are clean by construction" — true for the call-site-rejection of
tainted *arguments*, but it concealed the real gap, which was that the
body wasn't walked at all. New taint *created inside* the body was
invisible.

### Fix

`handle_function_def` now walks the body with `walk_statement`, after
seeding clean status for the function-local positionals (`$1..$9`,
`$@`, `$*`). Variable state is snapshotted before the walk and restored
after, so taint from inside the body does not leak to siblings.

This is intra-procedural analysis only. No signatures. No propagation
across call boundaries. DD#24's quarantine model is preserved: external
data is rejected at the boundary it crosses, and a sash function call
site is itself a boundary (handled by the existing argument check in
`handle_command`).

### Why function-local positionals are clean

Inside `foo()`, `$1` refers to `foo`'s first call-site argument, not
the program's argv. The call site is a boundary: `handle_command`
rejects any tainted argument expression at the call. Therefore inside
the body, `$1..$N`/`$@`/`$*` are clean by construction. (This is the
correct version of the claim the prior comment got wrong.)

### Tests

Added `neg_tainted_env_inside_function_body` to `e2e_tests.rs`. It
fails before the fix and passes after. Existing function-argument
tests (`pos_func_args_arithmetic`, etc.) continue to pass because the
positional-clean seeding matches the call-site-quarantine semantics.

Full suite: 458 pass, 1 ignored (was 457 + 1).

### Lessons

1. **DG's verification question caught a bug my steelman assumed away.**
   I argued "the call-site check is airtight" without testing whether
   the call-site check was the only check that mattered. It wasn't.
   The body-walk was missing entirely.
2. **A confidently wrong comment is worse than no comment.** I had just
   written "parameters are clean by construction" as DD#24
   reinforcement. It locked in the wrong story. Reverted.
3. **Bug policy worked.** Stopped, fixed, didn't route around. The
   feature TODO entry now reads "incidentally enforced by the
   call-site check" — true *and* sufficient, because the body is now
   walked.

## Design Decision #24, amendment 2 (2026-04-24): no propagation, only classification

The original DD#24 framed taint as flow-sensitive: "reassignment
re-taints," tainted values flowing through assignments until blessed.
Working through the SSA-replatform spec exposed that this framing was
wrong — and the wrongness explains every bug we've found in `taint.rs`.

### The corrected rule

A value is tainted iff its **defining instruction** is one of the
boundary sources. There is no propagation. Tainted values cannot be
copied (`b=$a` is a read of `$a` and is rejected), concatenated,
indexed with, or used in arithmetic. The only legal operation on a
tainted value is to validate it via `bless`.

### The complete source list

- `$(getenv VAR)`
- `$(argv N)` / `$1..$9` / `$@` / `$*`
- `read` / `mapfile` from any inherited or opened fd, or any pipe
  (heredoc / here-string-of-clean produce clean values)
- `$(cmd)` for external `cmd`

User-defined sash functions cannot introduce taint. A function whose
body touches a tainted source without blessing fails to compile, so
`$(user_func ...)` capture is always clean (or compilation has
already failed). No signatures, no inference.

### What "read" means

Every non-bless use is a read: command args, RHS of assignment,
arithmetic, subscript indices, `join` over an array containing any
tainted element, function-argument passing. All rejected.

### Containers

Arrays/maps are containers of independent variables. Each element
carries its own taint. `join @arr` is rejected if any element is
tainted, exactly as if the elements were separate scalars.

### Why this is simpler

- No flow analysis. Per-definition classification + per-use rejection.
- No "taint propagation through assignment" — copying is reading.
- No inter-procedural reasoning — functions can't introduce taint, so
  call boundaries are not interesting.
- No scope-shadowing complications — at the SSA layer the question
  "is `x` tainted?" is "is the SSA defining instruction of this
  version of `x` a source instruction?" Yes/no, no map state.

### Why CST-layer enforcement keeps breaking

The CST analyzer has been trying to model "flow-sensitive taint with
reassignment re-taint" — the original (wrong) DD#24 framing. That
required tracking per-name state across statements, modeling scope,
walking function bodies, handling locals. Every layer of that was a
bug surface. The corrected rule needs none of it: just look at each
definition and classify, then reject reads.

### Implementation plan

Move taint enforcement to a pass over SSA. Per `SsaInst::Assign`,
classify the target's taint by the defining `SsaValue` shape (source
patterns → tainted, everything else → clean). Per use site
(`Command`, `Pipeline`, RHS of any other assign), if any operand is a
tainted `VarRef`, emit SB4020 unless the use is the test expression
of a `bless`. `bless` produces a new clean SSA version of its target.

The CST analyzer keeps only purely-syntactic checks: bless argument
shape (must be a name + test), heredoc-with-cmdsub rejection (SB5002),
eval rejection (SB4001), indirect expansion rejection.

### Lesson

DG's instinct to ask "is `getenv` inside a function tainted?" was the
right question. The answer exposed not one bug but a wrong mental
model that had been embedded in `taint.rs` from the start.
"Flow-sensitive with reassignment re-taint" sounded conservative and
felt right. It wasn't conservative — it was an attempt to mimic
Perl's taint mode in a CST analyzer, which is exactly what DD#24's
prose said we wouldn't do. The amendment makes the prose match the
intended semantics.

## Design Decision #46: Positional parameters are function-local only

### Bug that surfaced this

Empirical test (2026-04-24) of:
```bash
foo() { echo "got: $2"; }
foo onearg
```
compiled with sash and run with program args `first second third`:
- bash output: `got: ` (empty)
- sash compiled output: `got: second`

Program argv leaked through unbound positional slots inside the
function. Root cause: `transform.rs` inliner only redefines `$1..$N`
for the args supplied at the call site; `$N+1..$9` retain whatever
SSA version was reaching from the caller, which at top level is
version 0 loaded from `sash_argv`. Both a bash-divergence bug and a
structural enabler of taint bypasses under DD#24 amendment 2.

### The decision

**Sash-native mode**: `$1..$N`, `$@`, `$*`, `$#` are *only* valid
inside a function body. Their meaning is "argument bound by the
nearest enclosing call site." At top level, references to these
names are a compile error with guidance to use `argv N` /
`argv` (the explicit boundary builtin per DD#42).

**Bash mode** (compiling unconverted bash scripts): top-level
`$1..$N` are coerced to `$(argv N)`, top-level `$@` / `$*` to
`$(argv)`, top-level `$#` to argc-1 via the appropriate builtin.
The coercion is visible via `--emit-source`, like other bash→sash
transforms.

**Both modes**: inside a function body, reading `$K` where `K`
exceeds the call's actual arg count (`$#`) is a compile error,
not silent empty. Bash returns empty for unbound positionals; sash
treats it as a programmer mistake and rejects at compile time.
This requires the inliner to know the per-call argc statically,
which it already does (the call-site `c.args.len()`).

### Why

DD#24 amendment 2 enumerates four taint sources, one of which is
"program positional params." Having `$1..$N` *also* mean "function
arg" inside a body creates two meanings for one syntax, with the
disambiguation depending on lexical context. Removing the top-level
meaning makes:

- Taint classification syntactic: `argv` and `getenv` are the only
  ways to introduce program-level external data; positional-param
  syntax in any position means "function arg," whose taint is the
  taint of the call expression (typically clean).
- The leak structurally impossible: at top level there is nothing to
  leak from; inside a function the inliner's bindings fully define
  the slots and out-of-range reads are rejected.
- `$(argv N)` becomes the canonical, documented way to read program
  argv, matching the documented DD#42 idiom.
- Bash compatibility preserved via coercion, not via shared meaning.

### Implementation plan

1. Add a validator rule (CST layer) for sash-native mode: top-level
   reference to `$1..$9`, `$@`, `$*`, `$#` → diagnostic with
   suggested `argv N` / `argv` / `len argv` replacement.
2. Add a coercion (CST→IR or lower.rs) for bash mode: rewrite
   top-level positional refs to the corresponding `argv` calls.
3. In the inliner (`transform.rs:579-617` and the capture path
   `:147-220`): after defining `$1..$N` from call args, emit a
   diagnostic-bearing marker for slots `$N+1..$9` so any read of
   them in the body emits an "out-of-range positional parameter"
   error. (Or: track per-function the call-site argc, and have the
   use-site SSA reference check against it.)
4. Tests: add the leak test from above as a regression; add tests
   for top-level `$1` rejection in sash-native mode and coercion in
   bash mode; add tests for out-of-range reads in both modes.

### Lesson

The earlier fix to walk function bodies in the CST taint analyzer
was correct as far as it went, but the right architecture was
upstream: don't have positional-param syntax mean two things.
DG's question — "if a function takes one arg but reads $2, does
it get argv 2?" — exposed both a concrete bug and a design
clarification that reduces taint-pass surface area.

---

## Session 2026-04-24 — Bless Runtime Enforcement, CST Taint Fixes, DD#46

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

## Session 2026-04-24 (cont.) — Adversarial Audit Run-2 Remediation

**Goal:** Fix all 12 findings from the adversarial audit of taint/bless/DD#46.

**Approach:** TDD — wrote 16 failing tests for audit findings, then
implemented fixes until all 43 taint spec tests pass, 502 total.

### Audit Pipeline

5 models (gpt-4.1, claude-sonnet-4, claude-sonnet-4.5, gpt-5.2,
gpt-5.3-codex). ~60 raw findings → 12 deduplicated. Two remediation
plans (gpt-5.3-codex, claude-sonnet-4.5), reconciled by gpt-5.2.
All artifacts in `audit/run-2/`.

### Fixes

**T1 — Herestring/heredoc false clean** (CRITICAL, 5/5 models)
`handle_read` now calls `has_clean_heredoc_or_herestring()` which
inspects redirect content for tainted expansions via
`subtree_has_tainted_expansion()`. Only marks target clean if
content is actually clean.

**T2 — Tautological bless** (CRITICAL, 5/5 models)
`handle_bless` now requires the test expression to contain `$VAR`
where VAR is the variable being blessed. New diagnostic: SB4012
"test expression does not reference $VAR — tautological bless".

**T3 — Bless without valid test** (HIGH, 3/5 models)
`handle_bless` now requires args to start with `[[`. New diagnostic:
SB4013 "bless expected [[ test expression".

**T4 — Arithmetic assignment taint laundering** (HIGH, 3/5 models)
`expr_is_tainted` now handles `arithmetic_expansion` nodes via
`arith_subtree_has_tainted_var()`. `y=$((x+1))` where x is tainted
now correctly marks y as tainted.

**T5 — Parameter expansion operand taint** (HIGH, 2/5 models)
Both `expr_is_tainted` and `check_expr_reads` now recurse into all
children of `expansion` nodes, catching `${safe:-$tainted}` patterns.

**T6 — Function global re-taint** (HIGH, 1/5 models)
`handle_function_def` now collects free variable reads via
`collect_free_reads()`. `handle_command` checks free-read globals
at call sites — if any captured global is tainted, emits SB4020.
New fields: `function_free_reads: HashMap<String, Vec<String>>`.

**T9 — Shift + positional** (MEDIUM, 3/5 models)
`collect_max_positional` now tracks cumulative `shift` count and
adds it to max positional reference. `f(){ shift; echo "$1"; }; f one`
now correctly computes required argc as 2 (1 + 1 shift).

**T10 — Direct $(getenv) in expression** (MEDIUM, 1/5 models)
`check_expr_reads` now handles `command_substitution` nodes —
if `expr_is_tainted` returns true, emits SB4020.

**T11 — Case pattern taint** (MEDIUM, 3/5 models)
`walk_statement` case handler now descends into `case_item` children
and calls `check_expr_reads` on pattern expressions.

**T8 — Nameref rejected** (regression lock)
New rule: `rules/nameref_rejected.rs` — SB5003 rejects `declare -n`,
`local -n`, `typeset -n` including combined flags.

**T12 — Process substitution rejected** (regression lock)
New rule: `rules/process_substitution.rs` — SB5004 rejects `<(...)` and
`>(...)` process substitution nodes.

### Design Decision #49: Tautological Bless Rejection

**Context:** `bless x [[ 1 == 1 ]]` always succeeds at runtime
without constraining `x`. All 5 audit models flagged this.

**Decision:** The test expression in `bless VAR [[ expr ]]` must
contain `$VAR`. If it doesn't, the compiler rejects with SB4012.
This prevents tautological tests and tests on wrong variables.

**Tradeoff:** This is a syntactic check, not semantic. A test like
`[[ $x == $x ]]` would pass the check despite being tautological.
A semantic check would require expression analysis. The syntactic
check catches the common case (wrong variable, literal tests) with
zero false positives on real code.

### Design Decision #50: Function Free-Read Tracking

**Context:** Functions analyzed at definition time could read globals
that become tainted before call time.

**Decision:** Track which globals each function body reads (free
variable reads). At call sites, check whether any of those globals
are currently tainted. This is lightweight inter-procedural analysis
— no function signatures, no call graph, just a set of variable
names per function.

DG directed architecture and design. AI implemented.

### Design Decision #51: SSA Taint Verification Pass

**Context:** The CST taint analyzer is the primary enforcement gate.
After the audit run-2 remediation, it catches all known bypass
patterns. But it operates on syntax, not on the representation that
gets compiled. Any future bug in the CST pass could let tainted data
reach codegen undetected.

**Decision:** Add `ssa_taint.rs` — a second taint pass that runs on
the SSA representation after transformation, before codegen. It
classifies each VarRef version as tainted or clean based on how it
was defined:
- `getenv`, `argv`, `read` (non-herestring), external cmdsub → tainted
- `getenv` of known env var (auto-bless) → clean
- Known-safe external commands → clean
- User-defined function capture → clean
- Literal, bless target → clean
- Phi → clean unless a classified incoming is tainted
- Arithmetic/concat/param-expansion → tainted if any operand is

If any tainted VarRef reaches a use site (command arg, pipeline arg,
subscript, export), the pass emits an ICE ("CST taint pass missed
this") and compilation fails. This is the airbag — it should never
fire. If it does, that's a bug in the CST pass.

**Implementation:**
- `Bless` changed from `{var, test}` to `{target, source, test}` —
  produces a new SSA version. `source` is tainted, `target` is clean.
  Codegen copies the value on the pass path.
- `verify_ssa_taint()` called in `main.rs` before `compile_to_object`.
- 264 lines, no design decisions — purely mechanical verification.
- Special variables (`?`, `!`, `$`, `#`, etc.) and positionals at
  version 0 pre-seeded as clean.

DG directed architecture and design. AI implemented.

---

## Session 2026-04-25 (overnight) — Codegen Panic Cleanup + Brush Triage

**Goal:** Convert user-triggerable panics to proper implementations or
diagnostics. Re-run Brush test suite and triage.

### Codegen Improvements

**New test operators implemented:**
- `[[ f1 -ot f2 ]]` — file older-than, via reversed args to `sash_file_newer`
- `[[ f1 -ef f2 ]]` — same device+inode, new `sash_file_same_dev` in sash-rt
- `[[ -v var ]]` — variable set test (treated as -n; will need refinement
  for "set to empty" vs "unset" distinction)

**Panic cleanup:**
- All 32 `panic!()` calls in codegen.rs prefixed with `ICE:` for
  recognizability as internal compiler errors
- `eprintln!` for unknown unary test operator → ICE panic (was silently
  producing wrong code)
- `eprintln!` for unsupported param expansion → ICE panic
- Undefined variable in param expansion → returns empty string (bash behavior)
  instead of panicking
- Known env vars (from `known_externals.rs`) pre-seeded as clean in SSA
  taint verifier, fixing false ICEs on `$HOME`, `$USER`, etc.

3 new codegen tests: `test_file_older_than`, `test_file_same_device`,
`test_file_ef_different`. 505 tests total, all green.

### Brush Suite Triage

Re-ran 622 cases. Results vs baseline:

| | 2026-04-22 | 2026-04-25 |
|---|---|---|
| PASS | 173 | 160 |
| FAIL_OUTPUT | 78 | 59 |
| FAIL_COMPILE | 43 | 75 |
| NA | ~300 | 300 |

PASS went down because 54 scripts now correctly hit the taint gate
(SB4020). FAIL_OUTPUT improved from 78 → 59. Compile failures went
up for the right reason — the compiler is stricter.

Top output failure categories:
- Parameter transformations (`${var^^}`, `${var@Q}`, etc.) — 7 cases
- Compound command redirects — 3 cases (already on TODO)
- Arithmetic side effects (`(( x = 1 ))`) — 3 cases
- Array substring replacement — 5 cases
- Continue/break N — 2 cases

Full triage in `audit/run-2/BRUSH_TRIAGE.md`.

DG directed architecture and design. AI implemented.
