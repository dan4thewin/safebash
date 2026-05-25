//! safebash intermediate representation.
//!
//! This IR sits between the tree-sitter CST and SSA.  It strips
//! syntactic noise (semicolons, keywords, braces) and models the
//! semantic structure of a Bash script.
//!
//! Design principles:
//!   - Every CST construct that survives the safe-subset validator
//!     has a corresponding IR node.
//!   - The IR is a tree, not a graph (SSA adds edges later).
//!   - String data is owned — the IR is independent of the source text.
//!   - Spans are preserved for error reporting.

/// Source location for error reporting.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Span {
    pub line: usize, // 1-based
    pub col: usize,  // 1-based
}

impl Span {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }

}

/// A complete script — a sequence of statements.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub statements: Vec<Statement>,
    /// DD#56: dedicated END block (parsed as first-class syntax).
    pub end_block: Option<Vec<Statement>>,
}

/// A statement in the script.
#[derive(Debug, Clone, PartialEq)]
pub struct Statement {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    /// x=value or x=(a b c)
    Assignment(Assignment),

    /// declare/local/export/readonly with optional assignment
    Declaration(Declaration),

    /// A simple command: name + arguments
    Command(Command),

    /// cmd1 | cmd2 | cmd3
    Pipeline(Pipeline),

    /// if/elif/else/fi
    If(If),

    /// while condition; do body; done
    While(While),

    /// for var in words; do body; done
    For(For),

    /// case value in pattern) body;; esac
    Case(Case),

    /// fname() { body; }
    Function(Function),

    /// Any statement with redirections attached
    Redirected(Redirected),

    /// cmd1 && cmd2, cmd1 || cmd2
    List(List),

    /// ( statements )
    Subshell(Subshell),

    /// { statements; }
    Group(Group),

    /// Comment line (preserved for round-trip fidelity)
    Comment(String),

    /// DD#55: `- cmd` — suppressed command (no implicit exit check)
    Suppressed(Box<Statement>),

    /// (( expr )) used as a condition — arithmetic test.
    /// Distinct from [[ ]] which is string/file tests only.
    ArithTest(Box<ArithExpr>),
}

// ── Individual statement types ────────────────────────────────

/// Variable type in safebash's type system (DD#16).
/// String is the default (untyped bash). Int requires explicit annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VarType {
    String,
    Int,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    pub name: String,
    pub index: Option<Expr>, // arr[i]=val or map[key]=val
    pub op: AssignOp,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssignOp {
    Set,    // =
    Append, // +=
}

#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    pub keyword: DeclKeyword,
    pub var_type: VarType,
    pub flags: Vec<String>,
    pub assignments: Vec<Assignment>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeclKeyword {
    Declare,
    Local,
    Export,
    Readonly,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    pub name: Expr,
    pub args: Vec<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pipeline {
    pub commands: Vec<Statement>,
    pub negated: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct If {
    pub condition: Vec<Statement>,
    pub then_body: Vec<Statement>,
    pub elif_clauses: Vec<ElifClause>,
    pub else_body: Option<Vec<Statement>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ElifClause {
    pub condition: Vec<Statement>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct While {
    pub condition: Vec<Statement>,
    pub body: Vec<Statement>,
    pub until: bool, // true for `until` loops
}

#[derive(Debug, Clone, PartialEq)]
pub struct For {
    pub variable: String,
    pub words: Vec<Expr>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Case {
    pub value: Expr,
    pub items: Vec<CaseItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaseItem {
    pub patterns: Vec<Expr>,
    pub body: Vec<Statement>,
    pub fallthrough: bool, // ;& = fall through to next body
    pub resume: bool,      // ;;& = resume pattern matching
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    pub return_type: VarType,
    pub body: Vec<Statement>,
    /// DD#18: local fn — scoped to enclosing function.
    pub is_local: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Redirected {
    pub body: Box<Statement>,
    pub redirects: Vec<Redirect>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Redirect {
    pub fd: Option<u32>,
    pub op: RedirectOp,
    pub target: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RedirectOp {
    Write,      // >
    Append,     // >>
    Read,       // <
    DupOutput,  // >&
    DupInput,   // <&
    ReadWrite,  // <>
    HereDoc,    // <<
    HereString, // <<<
}

#[derive(Debug, Clone, PartialEq)]
pub struct List {
    pub op: ListOp,
    pub left: Box<Statement>,
    pub right: Box<Statement>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ListOp {
    And,        // &&
    Or,         // ||
    Background, // &
}

#[derive(Debug, Clone, PartialEq)]
pub struct Subshell {
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub body: Vec<Statement>,
}

// ── Expression types ──────────────────────────────────────────

/// An expression — anything that produces a value (word, string,
/// variable expansion, command substitution, arithmetic, array).
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// A literal word: hello, *.txt, -f
    Literal(String),

    /// A double-quoted string with interpolated parts
    DoubleQuoted(Vec<Expr>),

    /// A single-quoted (raw) string
    SingleQuoted(String),

    /// $var or $1 or $@ etc.
    SimpleExpansion(String),

    /// ${var}, ${var:-default}, ${var%%pattern}, etc.
    ParameterExpansion(ParameterExpansion),
    /// Associative array subscript: ${MAP[$KEY]}
    Subscript { name: String, index: Box<Expr> },

    /// $(command)
    CommandSubstitution(Vec<Statement>),

    /// $((expr)) — structured arithmetic expression
    Arithmetic(Box<ArithExpr>),

    /// $((expr)) — raw text (legacy, used by old lower/ module)
    ArithmeticExpansion(String),

    /// Concatenation of adjacent expressions: "$var"/path
    Concat(Vec<Expr>),

    /// Array literal: ("a" "b" "c")
    Array(Vec<Expr>),
    /// Associative array literal: ([key1]=val1 [key2]=val2)
    AssocArray(Vec<(String, Expr)>),

    /// Test expression inside [[ ]] or [ ]
    Test(TestExpr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParameterExpansion {
    pub name: String,
    pub operator: Option<String>, // :-, :=, :+, %, %%, #, ##, etc.
    pub operand: Option<Box<Expr>>,
}

/// Test expression inside [[ ]] conditionals.
#[derive(Debug, Clone, PartialEq)]
pub enum TestExpr {
    /// -f file, -n str, -z str, etc.
    Unary { op: String, operand: Box<Expr> },
    /// str = str, num -eq num, etc.
    Binary {
        left: Box<Expr>,
        op: String,
        right: Box<Expr>,
    },
    /// ! expr
    Not(Box<TestExpr>),
    /// expr && expr
    And(Box<TestExpr>, Box<TestExpr>),
    /// expr || expr
    Or(Box<TestExpr>, Box<TestExpr>),
    /// ( expr ) — grouping
    Paren(Box<TestExpr>),
}

// ── Arithmetic expressions ─────────────────────────────────────────────

/// Structured arithmetic expression produced by the Pratt parser.
#[derive(Debug, Clone, PartialEq)]
pub enum ArithExpr {
    /// Integer literal: 0, 42, 0xFF, 0o17, 2#101
    Literal(i64),
    /// Variable reference: x, count (bare name in arithmetic context)
    Var(String),
    /// $var or $1 — explicit dollar-prefixed variable
    DollarVar(String),
    /// Binary operation: left op right
    BinOp {
        left: Box<ArithExpr>,
        op: IrArithOp,
        right: Box<ArithExpr>,
    },
    /// Unary prefix: -x, +x, !x, ~x
    UnaryOp {
        op: IrUnaryArithOp,
        operand: Box<ArithExpr>,
    },
    /// Post-increment/decrement: x++, x--
    PostIncDec {
        var: String,
        inc: bool, // true = ++, false = --
    },
    /// Pre-increment/decrement: ++x, --x
    PreIncDec {
        var: String,
        inc: bool,
    },
    /// Assignment: var = expr
    Assign {
        var: String,
        value: Box<ArithExpr>,
    },
    /// Compound assignment: var += expr, var -= expr, etc.
    CompoundAssign {
        var: String,
        op: IrArithOp,
        value: Box<ArithExpr>,
    },
    /// Ternary: cond ? true_val : false_val
    Ternary {
        cond: Box<ArithExpr>,
        true_val: Box<ArithExpr>,
        false_val: Box<ArithExpr>,
    },
    /// Comma operator: expr, expr, ...
    Comma(Vec<ArithExpr>),
    /// Function call (DD#58 sexpr): (name arg1 arg2 ...)
    FuncCall {
        name: String,
        args: Vec<ArithExpr>,
    },
    /// Parenthesized grouping
    Paren(Box<ArithExpr>),
}

/// Binary arithmetic operators (IR level).
/// Mirrors ssa::ArithOp but lives in the IR for clean layering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrArithOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Shl,
    Shr,
    BitAnd,
    BitOr,
    BitXor,
}

/// Unary arithmetic operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrUnaryArithOp {
    Neg,    // -
    Pos,    // +
    Not,    // !
    BitNot, // ~
}
