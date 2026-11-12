title: Replace tree-sitter with hand-written parser
status: done
depends: []
gates:
  - all 281 spec tests pass
  - all Rust tests pass (e2e, oracle, taint, codegen, janet)
  - cargo clippy clean
  - no new dependencies except logos
resolved: 2026-06-07
resolution: Shipped via the tree-sitter-removal arc and the logos migration. logos integrated as the structural lexer (DD#59), hand-written recursive descent parser in sash/src/parse/ produces ir::Program directly, tree-sitter dependency removed from sash/Cargo.toml and source tree. 291/291 spec gates green on hand-parser branch. The "no new dependencies except logos" gate was honored — logos is the only addition.
landed: DD#59-logos-structural-lexer + status.md 2026-05-24 "tree-sitter removal complete"

# Replace tree-sitter + lower/ with a hand-written recursive descent parser

## Goal

Eliminate tree-sitter (C dependency, grammar fork, CST→IR lowering pass)
and replace with a `logos` lexer + hand-written recursive descent parser
that produces `ir::Program` directly. The parser also absorbs the
validator rules that are parse-time rejections (category 1 + 3).

## Design decisions

- **Lexer**: `logos` crate (derive macro, SIMD-optimized DFA)
- **Parser**: hand-written recursive descent, Pratt parsing for arithmetic
- **AST**: reuse existing `ir.rs` types unchanged — `Program`, `Statement`,
  `Expr`, `TestExpr`, `Command`, etc.
- **Heredocs**: DROPPED from the language. Use herestrings with quoted
  multi-line content instead. (`<<EOF` → `<<< "content"`)
- **Arithmetic sexprs**: parsed inline by the parser (no DD#58 preprocessor)
- **Error codes**: all SB4001–SB5023 codes preserved with same messages.
  Category 1+3 become parse errors; category 2 stays as post-parse passes.
- **Taint**: remains a separate pass over the IR (most robust, most auditable)
- **sash-convert**: untouched (keeps tree-sitter for Bash parsing)
- **Migration**: big bang — new parser replaces old in one pass

## What gets deleted

- `sash/src/lower/` (7 files, ~3100 lines) — CST→IR lowering
- tree-sitter dependency in `sash/Cargo.toml`
- `tree-sitter-bash-safebash` grammar fork reference
- `DD#58` arithmetic preprocessor in `lib.rs`
- `preprocess_end_block()` in `lib.rs` (parser handles `END {}` natively)
- `preprocess_local_fn()` in `lib.rs` (parser handles `local fn` natively)
- Validator rules that become parse errors (12 rules from `rules/`)
- `sash/build.rs` (tree-sitter compilation)
- `sash/csrc/regex_stubs.c`

## What gets created

- `sash/src/parse/mod.rs` — public entry point: `parse(source) -> Result<Program, Vec<Diagnostic>>`
- `sash/src/parse/lexer.rs` — `logos` token definitions + heredoc-free tokenization
- `sash/src/parse/parser.rs` — recursive descent: statements, commands, control flow
- `sash/src/parse/expr.rs` — expression parsing: strings, expansions, parameter expansions
- `sash/src/parse/arith.rs` — arithmetic: Pratt parser for `$(( ))` including sexpr calls
- `sash/src/parse/tests.rs` — `[[ ]]` test expression parsing
- `sash/src/parse/error.rs` — diagnostics with SB* codes and span tracking

## END and local fn as first-class syntax

**END block**: first-class keyword. Parser sees `END` token, calls
`parse_end_block()`, produces IR with `Program.end_block: Option<Vec<Statement>>`.
No preprocessor rewrite. Transform reads `program.end_block` directly
(currently it looks for a function named `__sash_end__` — change to
check the dedicated field).

**local fn**: first-class keyword pair. Parser sees `local` + `fn`,
calls `parse_function(is_local=true)`, produces `Function { is_local: true, ... }`.
Transform checks `is_local` flag for scoping (currently checks
`__local__` prefix — change to check the flag).

IR changes needed:
- `Program`: add `pub end_block: Option<Vec<Statement>>`
- `Function`: add `pub is_local: bool`

Transform changes:
- `preprocess_end_block()` deleted
- `preprocess_local_fn()` deleted
- Transform looks at `program.end_block` instead of scanning for `__sash_end__`
- Transform looks at `function.is_local` instead of `__local__` prefix

## Migration of old parse()

The existing `pub fn parse(source: &str) -> tree_sitter::Tree` in `lib.rs`
moves to `sash-convert` (which still needs tree-sitter for Bash input).
Rename to `pub fn parse_bash(source: &str) -> tree_sitter::Tree` in
sash-convert to avoid collision.

The new parser entry point: `sash::parse::parse_sash(source: &str) -> Result<Program, Vec<Diagnostic>>`

## No parallel-run

The spec suite (281 tests) is the gate. No parallel execution of both
parsers. Delete old code once specs pass.

## Token set (logos)

```rust
#[derive(Logos)]
enum Token {
    // Keywords
    If, Then, Else, Elif, Fi,
    While, Until, Do, Done,
    For, In, Case, Esac,
    Function, Fn, Local,
    Int, Const, Array, Map,
    Bless, Signal, End,
    Return, Break, Continue,

    // Operators
    Pipe, And, Or, Background,  // | && || &
    Semi, Newline,
    LParen, RParen, LBrace, RBrace,
    DollarLParen, DollarDLParen, // $( $((
    DoubleBracketOpen, DoubleBracketClose, // [[ ]]

    // Redirects
    Lt, Gt, DGt, Amp,  // < > >> &
    HereString,        // <<<

    // Literals and identifiers
    Word(String),
    SingleQuoted(String),
    DoubleQuoted(Vec<StringPart>),  // with interpolation
    Number(i64),
    Variable(String),    // $name or ${name}
    Assign,             // =
    PlusAssign,         // +=

    // Special
    Comment,
    Eof,
}
```

## `(` disambiguation

`$(` is always command substitution. `$((` is always arithmetic (greedy
match). There are no subshells in sash — `(` outside arithmetic/arrays
is a parse error. No ambiguity.

Uses of `(` in sash-native:
- `$((expr))` — arithmetic
- `$(cmd)` — command substitution
- `(sexpr args...)` — inside `$(( ))` only
- `arr=(a b c)` — array literal after `=`
- `fn name()` — optional in function definition

## Parser structure

```rust
pub fn parse(source: &str) -> Result<Program, Vec<Diagnostic>> {
    let lexer = Lexer::new(source);
    let mut parser = Parser::new(lexer);
    let program = parser.parse_program()?;
    Ok(program)
}

impl Parser {
    fn parse_program(&mut self) -> Result<Program, Vec<Diagnostic>>;
    fn parse_statement(&mut self) -> Result<Statement, Diagnostic>;
    fn parse_command(&mut self) -> Result<Command, Diagnostic>;
    fn parse_if(&mut self) -> Result<If, Diagnostic>;
    fn parse_while(&mut self) -> Result<While, Diagnostic>;
    fn parse_for(&mut self) -> Result<For, Diagnostic>;
    fn parse_case(&mut self) -> Result<Case, Diagnostic>;
    fn parse_function(&mut self) -> Result<Function, Diagnostic>;
    fn parse_end_block(&mut self) -> Result<Statement, Diagnostic>;
    fn parse_declaration(&mut self) -> Result<Declaration, Diagnostic>;
    fn parse_test_expr(&mut self) -> Result<TestExpr, Diagnostic>;
    fn parse_expr(&mut self) -> Result<Expr, Diagnostic>;
    fn parse_word(&mut self) -> Result<Expr, Diagnostic>;
    fn parse_string(&mut self) -> Result<Expr, Diagnostic>;
    fn parse_arithmetic(&mut self) -> Result<Expr, Diagnostic>;  // Pratt
    fn parse_param_expansion(&mut self) -> Result<Expr, Diagnostic>;
}
```

## Validator rules absorbed into parser

These become parse errors (parser refuses to produce AST for them):

| Code | Construct | Parse action |
|------|-----------|--------------|
| SB4001 | `eval` | reject keyword |
| SB4002 | `source` / `.` | reject keyword |
| SB4003 | `${!var}` indirect expansion | reject in param_expansion |
| SB5001 | `export`, `declare`, `typeset` | reject keyword |
| SB5002 | `$(cmd)` in heredoc | N/A (no heredocs) |
| SB5006 | unquoted glob in word position | reject in parse_word |
| SB5010 | `${arr[@]}` in for-in | reject in parse_for |
| SB5018 | bare `$HOME` etc. | reject in parse_expr |
| SB5019 | `-gt` etc. in `[[ ]]` | reject in parse_test_expr |
| SB5020 | backtick substitution | reject in lexer |
| SB5021 | `[ ]` single bracket | reject in lexer |
| SB5023 | invalid regex | reject in parse_expr |
| SC2006 | backtick | same as SB5020 |
| SC2292 | single bracket | same as SB5021 |
| — | nameref (`declare -n`) | reject keyword combo |
| — | process substitution `<()` | reject in lexer |
| — | extglob `@()` etc. | reject in lexer |

## Validator rules that remain as post-parse passes

| Code | Concern | Reason |
|------|---------|--------|
| SB4020 | taint violations | requires data flow analysis |
| SB5011 | unresolved command | requires PATH lookup |
| SB5012–5014 | `$?` scoping | requires control flow |
| SB5022 | arithmetic mutation | requires expression analysis |

## Taint rewrite

`taint.rs` currently walks tree-sitter `Node` types. Rewrite to walk
`ir::Statement` / `ir::Expr` types instead. Same logic, different
tree type. The `TaintTracker` struct stays; its `analyze()` method
changes from `analyze(node: Node, source: &[u8])` to
`analyze(program: &Program)`.

## Integration point

`lib.rs` `cli_main` currently does:
```rust
let tree = parse(&source);           // tree-sitter
let diags = validate(tree, source);  // validator
let program = lower(tree, source);   // CST→IR
```

Becomes:
```rust
let program = parse::parse(&source)?;  // new parser (includes rejections)
let diags = taint::analyze(&program);  // taint on IR
```

## Implementation order

1. **Lexer** (`parse/lexer.rs`): define tokens, handle string interpolation,
   `$((`, `${`, quoted strings with `$var` expansion. Test with unit tests
   on representative sash source.

2. **Parser skeleton** (`parse/parser.rs`): parse_program, parse_statement
   dispatching on first token. Initially handle only: simple commands,
   assignments, if/fi, while/done, for/done. Produce `ir::Program`.
   Test: compile a simple script end-to-end through the new parser.

3. **Expressions** (`parse/expr.rs`): word parsing, string interpolation,
   parameter expansion (`${var:-default}`, `${var:0:3}`, `${var/pat/rep}`),
   command substitution `$(...)`, array literals.

4. **Arithmetic** (`parse/arith.rs`): Pratt parser for `$(( ))` contents.
   Binary ops with precedence, unary, ternary, sexpr function calls
   `(name args...)`. Replaces DD#58 preprocessor entirely.

5. **Test expressions** (`parse/tests.rs`): `[[ ]]` with unary (`-f`, `-n`),
   binary (`==`, `!=`, `-eq`, `=~`), logical (`&&`, `||`, `!`).

6. **Remaining constructs**: case/esac, function definitions, `local fn`,
   `END {}`, bless, signal, declarations (int/const/array/map),
   pipelines, lists (`&&`, `||`, `&`), redirects, herestrings.

7. **Error codes**: add SB* rejection diagnostics for all category 1+3
   constructs. Verify each produces the same code and similar message.

8. **Taint rewrite**: change `taint.rs` to walk IR types.

9. **Delete old code**: remove lower/, tree-sitter dep, preprocessors,
   absorbed validator rules, build.rs.

10. **Verify**: all 281 spec tests pass, all Rust tests pass, clippy clean.

## Testing strategy

- Each parser function gets unit tests in `parse/tests/` using the same
  patterns as existing `codegen_tests.rs` (source string → expected IR).
- Integration: the spec suite (`spec/*.sh`) is the gate. All 281 must pass.
- Oracle tests compare against Bash behavior — unchanged.
- Taint tests verify taint tracking — adapted to use new parser entry point.

## Size estimate

- Lexer: ~300 lines (logos derive + string interpolation handling)
- Parser: ~1200 lines (one method per construct, each 10-50 lines)
- Arithmetic: ~200 lines (Pratt parser with ~15 operators)
- Test expressions: ~100 lines
- Error handling: ~150 lines
- Taint rewrite: ~200 lines (walk IR instead of CST, same logic)
- Total new: ~2200 lines
- Total deleted: ~3500 lines (lower/ + validator rules + preprocessors + tree-sitter build)
- Net: -1300 lines

## Task for sash-convert

Create separate task: "Add bash-aware messaging to sash-convert for
constructs that the compiler now rejects at parse time." This ensures
users migrating from Bash get helpful guidance from the converter even
though the compiler's error is now a terse parse error.

Also:
- Move `parse()` from sash to sash-convert as `parse_bash()`
- Add spec test verifying heredocs are rejected with a clear error
  message: "heredocs are not supported in sash — use herestring `<<<`
  with quoted content instead"
- Add tree-sitter dependency to sash-convert's Cargo.toml (if not
  already there)
