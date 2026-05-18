---
title: "Session: DD#55, DD#58, DD#22 implementation"
date: 2026-05-11
---

## Full session — four DDs, zero regressions

### DD#55 — Suppress prefix and error model (22/22 gates)

Grammar: `suppressed_command` node in tree-sitter-bash-safebash.
IR: `StmtKind::Suppressed(Box<Statement>)`.
Transform: `checked: false` on inner commands/pipelines.
Validation: SB5013 ($? after `-`), SB5014 (stale $? in handler),
SB4018 ($(f) for -> int).
Taint: unwrap suppressed_command in CST taint, basename matching
in SSA taint for `/bin/echo` etc.
Lowerer: `$(status)` → `$?`, `$(getcpid)` → `$!`.
|| exit status: RHS completion resets to 0.
Return: `expr_to_value` for return args (arithmetic return values).

### DD#58 — Integer function capture (8/8 gates)

Transform: `SsaValue::IntFuncCall`, `lift_int_func_calls`,
`inline_int_func` for inlining -> int functions in arithmetic.
Arithmetic parser: bare identifier + sexpr `(name args...)` recognition.
Source preprocessor: replaces sexpr content in `$(( ))` with
placeholders before tree-sitter, transform resolves via global
`ARITH_MAP`. Fixes nested sexpr `$(( (add (dbl 3) 4) ))`.
UTF-8 fix in preprocessor: `utf8_char_len` instead of byte-as-char.
Lowerer: arithmetic comparisons → raw text (not test desugaring),
enables DD#31 context enforcement.
Codegen: `__let_exit` sets `test_result_var` for if/while branches.
Name collision: SB4019 for fn/variable name conflicts.

### DD#31 — Disjoint arithmetic contexts (19/19 gates)

Already passing after DD#58 work. The arithmetic comparison
desugaring change (raw text instead of test_command) fixed 31.4a/b/c
as a side effect.

### DD#22 — Word splitting elimination (7/7 gates)

New `read -f:` flag: explicit field separator for multi-var read.
`-d` keeps bash semantics (record terminator). `-f` with single
var → SB5016 compile error. Unknown flags (including `-r`) → SB5015.
Runtime: `sash_split_field` splits on arbitrary delimiter byte.
Pipeline inline builtins: echo, true, false, : run in forked
children instead of exec'ing. Other builtins (printf) resolved
to absolute paths at compile time.
Mapfile+pipe fix: `lower_expr_value` now handles `Subscript`
expressions in test context (was returning 0,0 for `${arr[0]}`
in bless tests).

### Infrastructure changes

- tree-sitter-bash-safebash: `suppressed_command` node
- sash-rt: `sash_split_field` function
- Spec fixes: 22.4 drop `-r`, 22.5 use `-f:`, 22.6 add bless,
  55.19 `/bin/wait` → `wait`
- Parse error rule: suppress ERROR inside arithmetic (sexpr syntax)
- Taint: bless anti-tautology accepts `${name[` for array subscripts
- Command resolution: status, getcpid, getpid, argc, signum as
  known special forms

### Test deltas

| Suite | Before | After | Delta |
|-------|--------|-------|-------|
| e2e | 132/3 | 136/2 | +4/-1 |
| oracle | 59/0 | 59/0 | — |
| taint | 42/0 | 42/0 | — |
| codegen | 217/49 | 220/46 | +3/-3 |
