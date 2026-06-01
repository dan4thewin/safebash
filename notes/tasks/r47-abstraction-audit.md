title: Abstraction audit — qualitative review of CC>14 functions
status: ready
priority: P1
kind: review
source: opus-47-review
depends: []
gates:
  - DG reviews verdicts before scheduling refactors
  - each follow-up refactor filed as its own task

## Context (DG, 2026-05-26)

DG rubric: flag any function more than 2× the codebase average.
Mean CC = 7.0 across 637 functions; threshold = CC > 14. **90
functions** qualify.

But thresholds are not the point. The question is qualitative:

> Take each of the 90 and judge whether they would benefit (as
> relates to a human reviewer's comprehension) by the addition of
> abstractions and helpers (we want to avoid things like excessive
> method chaining). Any functions that aren't truly complex (i.e.
> over-counted by the cyclomatic complexity algorithm) can be
> whitelisted.

This file records that judgment for every one of the 90.

## Verdict legend

- **A** — Needs new abstractions. Either (a) distinct concerns
  inlined that a helper would isolate, (b) a repeated pattern that
  cries out for an extraction, or (c) genuinely tangled control
  flow where helpers improve scanning.
- **B** — Whitelist. CC algorithm counts every match-arm, but the
  arms are a 1:1 declarative table where each entry is trivial. No
  reviewer would benefit from extraction.
- **C** — Marginal. Modest extraction possible (typically of one
  inline sub-block), but the function reads cleanly as-is. Lower
  priority than A; higher than B.

## Cross-cutting patterns

These appear in many A-verdicts. Landing the helpers first makes
the per-function extractions cheap.

1. **`emit_int_to_str(builder, module, externs, int_val) → (ptr, len)`**
   — itoa-from-i64 + ptr-len construction is duplicated across
   `lower_expr_value`, `lower_value` (5 arms), `handle_cmdsub`, and
   `handle_param_expansion`. One helper would shrink all four.

2. **`find_highest_version_var(builder, base_name) → Variable`** —
   "find $X$N where N is largest among declared variables" appears
   inline in `lower_test_unary` (-v arm), `lower_value` ($name with
   versioned suffix), and `lower_expr_value`. Belongs in
   `codegen/types.rs` next to `VarSlot`.

3. **`BuiltinCmd` enum + dispatch** — string-keyed builtin dispatch
   appears as the bulk of `emit` (CC 63), `emit_command` (39),
   `handle_command` (48), and `handle_command_substitution` (37).
   A `BuiltinCmd::from_name(&str)` enum and per-builtin handler fns
   would let each call site write `match cmd { ... }` against a
   typed enum instead of nested `if name == "..."` chains.

4. **Per-operator parameter-expansion helpers** —
   `handle_param_expansion` (CC 58) and `emit_param_expansion_inline`
   (41) both dispatch on operator strings (`#`, `##`, `%`, `%%`, `/`,
   `//`, `^`, `,`, etc.). Extract `emit_prefix_strip`,
   `emit_suffix_strip`, `emit_case_change`, `emit_sed_replace`, …
   shared between the two emitters.

5. **`ir::child_bodies()` exists at `ir.rs:81` but is underused**
   — `scan_expr_positionals_stmt` (CC 37), `collect_returns_inner`
   (21), `validate_stmt_ctx` (43), `taint_stmt` (28), `check_stmt`
   (47), `check_dq_stmt` (21) all manually recurse over each
   StmtKind. Use the helper aggressively. (Likewise
   `direct_exprs()` at `ir.rs:120` for expression-walking arms.)

   **Per-walker addendum (after audit during rollout attempt):**
   blanket recommendation was wrong. Of six candidates:
   - `taint_stmt`: already done.
   - `check_dq_stmt`: not applicable — `_ =>` arm tracks `$?` state,
     not a recursion site.
   - `scan_expr_positionals_stmt`: clean rollout candidate.
   - `collect_returns_inner`: careful — must not descend into
     Subshell/Function; needs explicit early-returns.
   - `validate_stmt_ctx`: needs a state-aware `child_bodies_with_ctx`
     helper to thread `in_condition` flag.
   - `check_stmt`: was a **bug** — `_ => {}` silently dropped
     Pipeline/Group/Subshell/List/Redirected. Fixed in journal 056;
     CC went 47 → 49 because the existing explicit arms each carry
     custom pre-work and were not collapsed. Collapse is a separate
     refactor.

   Each remaining walker should be its own task, not a single
   "rollout".

6. **`if let SsaInst::X = inst` → `match inst`** — `allocate_variables`
   (CC 36) and `print_ssa` (29) use sequential `if let` chains where
   a single `match` would be both flatter and cheaper-on-CC. Pure
   form change.

7. **Hex / octal / numeric-literal scanners** — `scan_arith_number`
   (20) and `scan_ansi_c_quoted` (30) both have inline hex and octal
   accumulation loops. Extract `scan_hex_byte`, `scan_octal_byte`.

## Per-function verdicts

Grouped by file for review. CC numbers from `bash contrib/cyclo.sh
sash/src --top 90` as of 2026-05-26.

### sash/src/codegen/dispatch.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 63 | `emit` | **A** | Top hotspot. String-keyed builtin dispatcher. Extract `BuiltinCmd` enum + handler table (pattern #3). |

### sash/src/codegen/value_handlers.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 58 | `handle_param_expansion` | **A** | Per-operator if-chain with repeated "intern, build ptr/len, call extern" boilerplate. Per-operator helpers + dispatch (patterns #1, #4). |
| 19 | `handle_cmdsub` | **A** | Same itoa+stack-slot pattern as `handle_param_expansion` (pattern #1). |

### sash/src/codegen/values.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 48 | `lower_value` | **A** | Itoa-from-i64 duplicated across $?, $$, int var, ArithExpr, ArithBinOp arms (pattern #1, #2). |
| 32 | `scan_value_for_vars` | **B/C** | Pure variant traversal; multiple arms repeat positional-name check. Extract `maybe_insert_positional`. |
| 26 | `lower_expr_value` | **A** | Itoa pattern duplicated (pattern #1). |

### sash/src/codegen/tests.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 45 | `lower_test_unary` | **A** | -e/-f/etc. op-code table inline + 60-line $-v find-highest-version block. Extract `file_test_op_code()` and `find_highest_version_var()` (pattern #2). |
| 22 | `lower_test_binary` | **C** | Op-keyed match with `lower_expr_value` repeated. Extract `lower_pair(...)`. |

### sash/src/codegen/inst_handlers.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 34 | `handle_pipeline` | **A** | Massive procedural fork/pipe/dup/wait codegen. Extract `create_pipes`, `wire_child_io`, `lower_pipeline_builtin`, `lower_compound_stage`, `lower_exec_stage`, `wait_for_children`. |

### sash/src/codegen/setup.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 36 | `allocate_variables` | **A** | Two-pass loop with `if let SsaInst::X` instead of `match inst` (pattern #6). Convert to match + extract `declare_base_var` and `map_versioned`. |
| 22 | `init_positional_params` | **C** | Top-level setup driver. Mostly linear; modest. |
| 17 | `intern_inst` | **B** | SsaInst variant traversal. WHITELIST. |
| 15 | `intern_value` | **B** | SsaValue variant traversal. WHITELIST. |

### sash/src/codegen/arithmetic.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 22 | `emit_arith_op` | **A** | Mostly 1-line per ArithOp arm except a 50-line Div/Mod zero-check block. Extract `emit_div_or_mod_with_zero_check(op, ...)`. |

### sash/src/codegen/redirects.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 19 | `emit_redirects` | **A** | "Literal-path-fast vs dynamic-path-slow" duplicated across Write/Append/Read/HereString arms. Extract `emit_open_path(builder, target, flags) -> ()`. |
| 16 | `lower_ir_redirect_expr` | **C** | ExprKind→(ptr, len) lowering. Modest. |

### sash/src/codegen/entry.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 20 | `compile_ssa` | **C** | Top-level codegen driver. Setup + one block loop. Fine. |

### sash/src/transform/statements.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 57 | `lower_one` | **C→A** | StmtKind dispatcher with inline arms for Until-desugaring, Function, Redirected, Suppressed. Extract those four. |

### sash/src/transform/stmt_handlers.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 54 | `handle_pipeline` | **A** | 200 lines, deeply nested. Extract `build_producer_stages`, `parse_read_flags`, sub-handlers per pipeline shape. |
| 48 | `handle_command` | **A** | Multiple distinct concerns: bless, [[, regex sub, shift. Per-shape `try_X` helpers (pattern #3). |
| 24 | `handle_assignment` | **A** | Inline FuncCall capture block (~110 lines). Extract `emit_capture_inline`. |

### sash/src/transform/control_flow.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 44 | `lower_for` | **A** | Shape dispatcher: dynamic vs static, $@, array iter, $(cmd)→pipe-while-read. Extract `try_lower_dollar_at`, `try_lower_array_iter`, `try_lower_cmdsubst_pipe`. |
| 44 | `lower_case` | **A** | Multi-phase: build bodies, wire fall-through, build dispatch, build no-match. Split phases. |

### sash/src/transform/expr_handlers.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 37 | `handle_command_substitution` | **A** | Shape recognition: known fn, len, keys, join, echo, generic. Per-builtin `try_X` helpers (pattern #3). |
| 30 | `arith_to_ssa` | **A** | ArithExpr dispatcher with constant-folding inline in 4 arms. Extract `try_fold_binop`, `try_fold_unop`. |
| 25 | `fold_arith_op` | **B** | Small ArithOp dispatcher; 1-3 lines per arm. WHITELIST. |
| 20 | `ir_op_to_ssa` | **B** | 1:1 enum→enum. WHITELIST. |

### sash/src/transform/expressions.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 34 | `expr_to_value` | **A** | ExprKind dispatcher with non-trivial Array (regex/glob detection) and DoubleQuoted (all-literal collapse). Extract `lower_array_literal`. |
| 16 | `expr_to_string` | **B** | Pure ExprKind→String formatter. WHITELIST. |

### sash/src/transform/helpers.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 37 | `scan_expr_positionals_stmt` | **C** | StmtKind dispatcher with same recursion pattern. Use `child_bodies()` aggressively (pattern #5). |
| 21 | `scan_expr_positionals_expr` | **C** | ExprKind dispatcher. Use `direct_exprs()` (pattern #5). |

### sash/src/transform/cmd_handlers.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 24 | `handle_trap` | **C** | Multi-shape with signal-name→number table inline. Extract `signal_name_to_num()`. |
| 21 | `handle_read` | **A** | Hand-rolled flag-parser with awkward `skip_next` state. Extract `parse_read_flags` returning (delim, field_sep, array, vars). |
| 17 | `handle_function_call_or_command` | **C** | Function inlining; mostly linear data plumbing. |

### sash/src/transform/functions.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 21 | `collect_returns_inner` | **C** | StmtKind recursion duplicating shape. Use `child_bodies()` (pattern #5). |

### sash/src/transform/entry.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 23 | `ir_to_ssa_typed` | **C** | Top-level driver with 3 distinct phases (collect fns, lower body, splice END, mark unchecked). Extract `mark_unchecked_in_conditions`, `splice_end_block`. |

### sash/src/parse/lexer.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 47 | `scan_word` | **A** | Byte-match dispatcher + 30-arm keyword table. Extract `keyword_token(&str) -> Option<Tok>`. |
| 30 | `scan_ansi_c_quoted` | **A** | 14-arm escape table (`\n`/`\t`/`\r`/…) + inline octal and hex scanners. Extract `simple_escape(byte) -> Option<char>` + share hex/octal scanners (pattern #7). |
| 18 | `scan_brace_expansion` | **C** | Bracket-counting lexer with quote-state. Modest. |
| 16 | `scan_dollar` | **B** | Pure byte-dispatcher. WHITELIST. |
| 15 | `next_token` | **B** | Already cleared in r47 phase 6. WHITELIST. |

### sash/src/parse/logos_tokens.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 48 | `to_tok` | **B** | Pure 1:1 logos-token→Tok mapping. WHITELIST. |

### sash/src/parse/expr.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 44 | `parse_parameter_expansion` | **A** | Multi-stage parser with distinct shape branches. Extract `try_length`, `try_keys`, `try_subscript`, `try_sed`, `try_substring`, `try_transform`, `parse_standard_op`. |
| 32 | `parse_expr` | **A** | Tok dispatcher with rich per-arm logic. Extract the Tok::Word arm (tilde/@arr) and Tok::Dollar arm ($arr[i] / LINENO). |
| 24 | `keyword_to_str` | **B** | Pure 1:1 Tok→str. WHITELIST. |
| 22 | `parse_command_substitution` | **A** | Special-word handling (status, getcpid) inline. Extract `try_special_word(scanner, span) -> Option<Expr>`. |
| 20 | `parse_array_literal` | **C** | [key]=val parsing inline. Extract `try_parse_assoc_pair`. |
| 17 | `string_parts_to_exprs` | **C** | Per-StringPart dispatch; modest. |

### sash/src/parse/arith.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 28 | `parse_prefix` | **A** | 12-arm token dispatcher; ++/-- arms duplicate, unary Minus/Plus/Bang/Tilde all duplicate. Extract `parse_pre_incdec(inc)` and `parse_unary(op)`. |
| 26 | `tok_to_text` | **B** | Pure 1:1 Tok→String. WHITELIST. |
| 21 | `match_arith_op2` | **B** | 1:1 byte→token. WHITELIST. |
| 21 | `binary_op_for` | **B** | Declarative token→(op, bp) table. WHITELIST. |
| 20 | `scan_arith_number` | **C** | Three distinct number-form branches. Extract `scan_hex`, `scan_octal`, `scan_base_n`, `scan_decimal` (pattern #7). |
| 20 | `match_arith_op1` | **B** | 1:1 byte→token. WHITELIST. |

### sash/src/parse/parser.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 31 | `parse_statement` | **A** | 14-arm Tok dispatcher + 5-pattern postfix-rejection table. Extract `reject_postfix_on_compound`. |
| 26 | `parse_test_primary` | **C/A** | Token dispatcher with multiple binary-op arms. Could fold via a small op-table. |
| 23 | `maybe_wrap_redirects` | **A** | 3-phase tail (redirects → pipeline → list-op). Extract `parse_trailing_redirects`, `parse_trailing_pipeline`, `parse_trailing_list_op`. |
| 17 | `parse_simple_or_assignment` | **A** | 6-arm leading-rejected-keyword table inline. Extract `try_reject_unsafe_keyword(scanner, span) -> Option<Diagnostic>`. |
| 17 | `parse_regex_rhs` | **B** | Tok→char mapping. WHITELIST. |
| 17 | `parse_redirect` | **B** | Tok→(fd, op) dispatch. WHITELIST. |
| 17 | `parse_local` | **C** | `local x=v` parser; mostly linear. |
| 15 | `parse_command` | **C** | Simple command parser; modest. |

### sash/src/type_check.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 47 | `check_stmt` | **C** | StmtKind dispatcher mostly recursing over children. Extract `check_command_stmt`, `check_assignment_stmt` and use `child_bodies()` (pattern #5). |
| 19 | `infer_stmt` | **C** | StmtKind dispatcher. Modest. |
| 15 | `expr_type` | **B** | Small ExprKind→VarType dispatcher. WHITELIST. |

### sash/src/validate.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 43 | `validate_stmt_ctx` | **C** | StmtKind dispatcher; mostly recurse-with-condition-bool (pattern #5). |
| 28 | `taint_stmt` | **C** | StmtKind dispatcher, Pipeline/Redirected have distinct logic. Extract `taint_pipeline`, `taint_redirected_read`. |
| 21 | `check_dq_stmt` | **C** | Status state-machine across StmtKind. Already well-structured. |
| 19 | `validate_command` | **C** | Per-builtin SBxxxx error rules. Extract `check_heredoc`, `check_dynexec`. |

### sash/src/ssa_taint.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 40 | `value_is_tainted` | **B** | SsaValue dispatcher + 15-arm cmd-basename table. WHITELIST. |
| 36 | `verify_ssa_taint` | **A** | Two-phase analysis with inline inst dispatchers. Extract `classify_inst_def` and `check_inst_uses`. |
| 16 | `check_value_use` | **B** | SsaValue traversal. WHITELIST. |

### sash/src/ssa.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 21 | `from_str` (ArithOp) | **B** | 1:1 str→enum. WHITELIST. |
| 21 | `display` (ArithOp) | **B** | 1:1 enum→str. WHITELIST. |
| 21 | `as_u8` (ArithOp) | **B** | 1:1 enum→byte. WHITELIST. |

### sash/src/ssa_printer.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 29 | `print_ssa` | **C** | SsaInst dispatcher; most arms 2-5 lines of formatting. Extract `print_inst` (pattern #6). |
| 19 | `print_value` | **B** | SsaValue dispatcher with trivial formatting. WHITELIST. |

### sash/src/printer.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 48 | `print_arith_expr` | **B** | Mostly inline op-table strings + simple recursion. WHITELIST. |
| 45 | `print_stmt` | **C** | StmtKind dispatcher; most arms 5-10 lines. Modest. |
| 24 | `print_expr` | **C** | ExprKind dispatcher with DoubleQuoted nested. Modest. |

### sash/src/emit_janet.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 44 | `arith_expr_to_janet` | **B** | ArithExpr variant dispatch with three op-string tables (19+4+5 variants). WHITELIST. |
| 41 | `emit_param_expansion_inline` | **A** | By-operator string-keyed dispatcher, 4-6 way sub-matches. Per-operator helpers shared with `handle_param_expansion` (pattern #4). |
| 39 | `emit_command` | **A** | String-keyed builtin dispatcher — same pattern as `emit` (pattern #3). |
| 39 | `emit_stmt` | **C** | StmtKind dispatcher with For/Case/Function/List arms inline. Modest. |
| 31 | `emit_expr_inline` | **B/C** | ExprKind dispatcher, mostly trivial mapping. CC overcounted by SimpleExpansion special-name arms. |
| 26 | `emit_test_expr` | **C** | TestExpr dispatcher with op-string tables. Extract `binary_op_to_janet`, `unary_op_to_janet`. |
| 20 | `emit_redirected` | **C** | Per-redirect-op codegen; modest. |
| 20 | `_stmt_uses_args` | **B** | Pure StmtKind→bool traversal. WHITELIST. |

### sash/src/ir.rs

| CC | Function | V | Note |
|---:|---|:-:|---|
| 16 | `child_bodies` | **B** | IR helper returning children of StmtKind. WHITELIST (and *use it more aggressively* — see pattern #5). |
| 15 | `direct_exprs` | **B** | Same as above for exprs. WHITELIST. |

## Verdict tallies

- **A** (needs abstractions): **32**
- **B** (whitelist — CC algorithm artifact): **31**
- **C** (marginal): **27**

So roughly a third of the high-CC list is genuinely worth refactoring,
a third is noise the algorithm flagged that a human reviewer reads
fine, and a third is in-between.

## Proposed refactor queue (derived from A-verdicts)

Order by **shared-helper-first**, then by **highest-CC remaining**.

### Wave 1 — land the cross-cutting helpers (4 commits)

These touch many files but each landing is small and unblocks
multiple per-function refactors.

1. **`emit_int_to_str` helper** (pattern #1) — net reduction across
   `lower_value`, `lower_expr_value`, `handle_cmdsub`,
   `handle_param_expansion`.
2. **`find_highest_version_var` helper** (pattern #2) — fold the
   duplicated -v lookup into `codegen/types.rs`.
3. **`BuiltinCmd` enum + dispatch** (pattern #3) — used by `emit`,
   `emit_command`, `handle_command`, `handle_command_substitution`.
4. **`child_bodies` / `direct_exprs` rollout** (pattern #5) — touch
   `scan_expr_positionals_*`, `collect_returns_inner`,
   `validate_stmt_ctx`, `taint_stmt`, `check_stmt`, `check_dq_stmt`.

### Wave 2 — top-CC A-verdicts (after helpers land)

In order:

1. `emit` (63 → expected ~25 after BuiltinCmd)
2. `handle_param_expansion` (58 → expected ~20)
3. `handle_pipeline` transform (54)
4. `handle_command` transform (48)
5. `lower_value` (48 → expected ~15 after itoa helper)
6. `scan_word` lexer (47 → keyword_token extraction)
7. `lower_test_unary` (45)
8. `lower_for`, `lower_case`, `parse_parameter_expansion` (44 each)
9. `emit_param_expansion_inline`, `emit_command` (41, 39)
10. `verify_ssa_taint`, `allocate_variables` (36 each)
11. `handle_pipeline` codegen (34)
12. `expr_to_value` (34)
13. Remaining A-verdicts ≤ 32.

### Wave 3 — C-verdicts (file separately, lower priority)

Don't touch until A-list is cleared and DG signs off on each. The
risk/reward on C-verdicts is much worse: they're already readable.

### Whitelist (B-verdicts)

Recorded above. Do not refactor — and consider whether they should
be flagged in `contrib/cyclo.sh` so they stop appearing in the
top-N. A simple `// cyclo-allow` magic comment is the cheapest
mechanism; another option is to add a list of (file, function) to
the `cyclo.sh` driver. **DG decision needed: whitelist mechanism?**

## DG decisions requested

1. **Approve verdicts?** Read the table and disagree with any A/B/C
   call. Particularly worth checking: are the B-verdicts truly noise
   in DG's eyes, or do any deserve attention?
2. **Whitelist mechanism?** Magic comment vs cyclo.sh-driver list
   vs leave the noise.
3. **Wave 1 first, or pick one big A-verdict?** The patterns
   argue for landing helpers first; an alternative is to take the
   single largest hotspot (`emit`) and refactor it standalone.
4. **Threshold enforcement?** Current rule "twice the average" is a
   moving target. As we land Wave 2, the average will drop, threshold
   tightens. Hard ceiling (e.g. CC ≤ 40) might be more stable.
