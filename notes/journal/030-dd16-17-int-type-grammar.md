# Design Decision #16: Integer type system

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

