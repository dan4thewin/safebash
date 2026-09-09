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

impl Expr {
    /// Shorthand: literal string expression.
    pub fn literal(s: impl Into<String>, span: Span) -> Self {
        Self {
            kind: ExprKind::Literal(s.into()),
            span,
        }
    }

    /// Shorthand: parameter expansion with operator.
    pub fn param(
        name: impl Into<String>,
        operator: Option<&str>,
        operand: Option<Box<Expr>>,
        span: Span,
    ) -> Self {
        Self {
            kind: ExprKind::ParameterExpansion(ParameterExpansion {
                name: name.into(),
                operator: operator.map(|s| s.to_string()),
                operand,
                index: None,
                taint_off: false,
            }),
            span,
        }
    }
}

impl Statement {
    /// Shorthand: a simple command with name and args.
    pub fn command(name: Expr, args: Vec<Expr>, span: Span) -> Self {
        Self {
            kind: StmtKind::Command(Command { name, args }),
            span,
        }
    }

    /// Shorthand: the `true` builtin (used as placeholder in list ops).
    pub fn true_cmd(span: Span) -> Self {
        Self::command(Expr::literal("true", span), vec![], span)
    }

    /// Return all nested statement bodies (for generic recursion).
    ///
    /// This returns slices of child statements — NOT the statement itself.
    /// Use this as a default recursion mechanism in analyses that only need
    /// custom handling for specific StmtKind variants.
    pub fn child_bodies(&self) -> Vec<&[Statement]> {
        match &self.kind {
            StmtKind::If(i) => Self::if_child_bodies(i),
            StmtKind::While(w) => vec![&w.condition, &w.body],
            StmtKind::For(f) => vec![&f.body],
            StmtKind::Case(c) => c.items.iter().map(|item| item.body.as_slice()).collect(),
            StmtKind::Function(f) => vec![&f.body],
            StmtKind::Pipeline(p) => vec![&p.commands],
            StmtKind::Redirected(r) => vec![std::slice::from_ref(r.body.as_ref())],
            StmtKind::List(l) => vec![
                std::slice::from_ref(l.left.as_ref()),
                std::slice::from_ref(l.right.as_ref()),
            ],
            StmtKind::Group(g) => vec![&g.body],
            StmtKind::Suppressed(inner)
            | StmtKind::ReturnProducer(inner)
            | StmtKind::Negated(inner) => {
                vec![std::slice::from_ref(inner.as_ref())]
            }
            StmtKind::Assignment(_)
            | StmtKind::Declaration(_)
            | StmtKind::Command(_)
            | StmtKind::ArithTest(_) => vec![],
        }
    }

    /// Mutable mirror of [`Self::child_bodies`]: the child statement bodies
    /// of this statement, for passes that rewrite nested statements in place.
    pub fn child_bodies_mut(&mut self) -> Vec<&mut [Statement]> {
        match &mut self.kind {
            StmtKind::If(i) => Self::if_child_bodies_mut(i),
            StmtKind::While(w) => vec![&mut w.condition, &mut w.body],
            StmtKind::For(f) => vec![&mut f.body],
            StmtKind::Case(c) => c
                .items
                .iter_mut()
                .map(|item| item.body.as_mut_slice())
                .collect(),
            StmtKind::Function(f) => vec![&mut f.body],
            StmtKind::Pipeline(p) => vec![&mut p.commands],
            StmtKind::Redirected(r) => vec![std::slice::from_mut(r.body.as_mut())],
            StmtKind::List(l) => vec![
                std::slice::from_mut(l.left.as_mut()),
                std::slice::from_mut(l.right.as_mut()),
            ],
            StmtKind::Group(g) => vec![&mut g.body],
            StmtKind::Suppressed(inner)
            | StmtKind::ReturnProducer(inner)
            | StmtKind::Negated(inner) => {
                vec![std::slice::from_mut(inner.as_mut())]
            }
            StmtKind::Assignment(_)
            | StmtKind::Declaration(_)
            | StmtKind::Command(_)
            | StmtKind::ArithTest(_) => vec![],
        }
    }

    /// The child statement bodies of an `if` (condition, then, each elif's
    /// condition + body, and else), mutable. Factored out of
    /// [`Self::child_bodies_mut`] to keep that dispatcher simple.
    fn if_child_bodies(i: &If) -> Vec<&[Statement]> {
        let mut bodies: Vec<&[Statement]> = vec![&i.condition, &i.then_body];
        for elif in &i.elif_clauses {
            bodies.push(&elif.condition);
            bodies.push(&elif.body);
        }
        if let Some(ref eb) = i.else_body {
            bodies.push(eb);
        }
        bodies
    }

    fn if_child_bodies_mut(i: &mut If) -> Vec<&mut [Statement]> {
        let mut bodies: Vec<&mut [Statement]> = vec![&mut i.condition, &mut i.then_body];
        for elif in &mut i.elif_clauses {
            bodies.push(&mut elif.condition);
            bodies.push(&mut elif.body);
        }
        if let Some(ref mut eb) = i.else_body {
            bodies.push(eb);
        }
        bodies
    }

    /// Visit all expressions directly owned by this statement (non-recursive).
    ///
    /// Does NOT descend into child statements — use `child_bodies()` for that.
    /// Visits: command names/args, assignment values/indices, declaration values,
    /// for-loop words, case value/patterns, redirect targets.
    pub fn direct_exprs(&self, f: &mut impl FnMut(&Expr)) {
        match &self.kind {
            StmtKind::Assignment(a) => {
                f(&a.value);
                if let Some(ref idx) = a.index {
                    f(idx);
                }
            }
            StmtKind::Declaration(d) => {
                for a in &d.assignments {
                    f(&a.value);
                }
            }
            StmtKind::Command(c) => {
                f(&c.name);
                for arg in &c.args {
                    f(arg);
                }
            }
            StmtKind::For(fo) => {
                for w in &fo.words {
                    f(w);
                }
            }
            StmtKind::Case(c) => {
                f(&c.value);
                for item in &c.items {
                    for p in &item.patterns {
                        f(p);
                    }
                }
            }
            StmtKind::Redirected(r) => {
                for redir in &r.redirects {
                    f(&redir.target);
                }
            }
            _ => {}
        }
    }
}

/// Recursively walk all expressions in a statement tree.
///
/// Visits every `Expr` node in the statement and all nested statements.
/// The callback receives each expression (including sub-expressions
/// inside DoubleQuoted, Concat, CommandSubstitution, etc.).
pub fn walk_all_exprs(stmt: &Statement, f: &mut impl FnMut(&Expr)) {
    stmt.direct_exprs(&mut |expr| walk_expr_tree(expr, f));
    // `(( … ))` statement / `if`/`while` condition — its `${…}` expansions live
    // in an `ArithExpr` tree, not a direct `Expr`, so surface them here (mirrors
    // the `ExprKind::Arithmetic` arm in walk_expr_tree for `$(( … ))`).
    if let StmtKind::ArithTest(arith) = &stmt.kind {
        walk_arith_expansions(arith, f);
    }
    for body in stmt.child_bodies() {
        for s in body {
            walk_all_exprs(s, f);
        }
    }
}

/// Walk an expression tree, visiting the node and all sub-expressions.
pub fn walk_expr_tree(expr: &Expr, f: &mut impl FnMut(&Expr)) {
    f(expr);
    match &expr.kind {
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) => walk_each(parts, f),
        ExprKind::Array(elems) => walk_each(elems, f),
        ExprKind::AssocArray(pairs) => walk_each_pair(pairs, f),
        ExprKind::CommandSubstitution(stmts) => walk_each_stmt(stmts, f),
        ExprKind::ParameterExpansion(pe) => walk_opt(pe.operand.as_deref(), f),
        ExprKind::Test(t) => walk_test_exprs(t, f),
        ExprKind::Subscript { index, .. } => walk_expr_tree(index, f),
        // `$(( … ))` — descend into any `${…}` expansions embedded in the arith
        // tree (`ArithExpr::Expansion`), so expr-level checks (e.g. the DD#65
        // pruned-operator SB4034 reject) apply inside arithmetic too. Without
        // this, `$(( ${s:?…} ))` bypassed the reject and ICE'd in codegen.
        ExprKind::Arithmetic(arith) => walk_arith_expansions(arith, f),
        _ => {}
    }
}

/// Walk each expr in a slice via `walk_expr_tree`.
fn walk_each(items: &[Expr], f: &mut impl FnMut(&Expr)) {
    for it in items {
        walk_expr_tree(it, f);
    }
}

/// Walk the value of each `(key, value)` pair (assoc-array literal).
fn walk_each_pair(pairs: &[(String, Expr)], f: &mut impl FnMut(&Expr)) {
    for (_, v) in pairs {
        walk_expr_tree(v, f);
    }
}

/// Walk every expr in each statement of a command substitution.
fn walk_each_stmt(stmts: &[Statement], f: &mut impl FnMut(&Expr)) {
    for s in stmts {
        walk_all_exprs(s, f);
    }
}

/// Walk an optional operand expr.
fn walk_opt(operand: Option<&Expr>, f: &mut impl FnMut(&Expr)) {
    if let Some(e) = operand {
        walk_expr_tree(e, f);
    }
}

/// DD#98: `_` is the don't-care binder. In any binding position (assignment
/// LHS, declaration, `for` variable, `read`/`mapfile`/`argv` target, attest
/// binder) it means "bind this and discard it": exempt from the DD#86
/// dead-variable gate, and taint-safe because its value can never be observed
/// — every read of `_` is SB5056. It still *binds*, so `read _ b` consumes a
/// field and `b` gets the next one.
pub(crate) fn is_discard_binder(name: &str) -> bool {
    name == "_"
}

/// Walk the `${…}` expansions embedded in an `ArithExpr` tree, applying `f` to
/// each imported `Expr` (via `walk_expr_tree`).
pub(crate) fn walk_arith_expansions(arith: &ArithExpr, f: &mut impl FnMut(&Expr)) {
    match arith {
        ArithExpr::Expansion(inner) => walk_expr_tree(inner, f),
        ArithExpr::BinOp { left, right, .. } => {
            walk_arith_expansions(left, f);
            walk_arith_expansions(right, f);
        }
        ArithExpr::UnaryOp { operand, .. } | ArithExpr::Paren(operand) => {
            walk_arith_expansions(operand, f)
        }
        ArithExpr::Assign { value, .. } | ArithExpr::CompoundAssign { value, .. } => {
            walk_arith_expansions(value, f)
        }
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => {
            walk_arith_expansions(cond, f);
            walk_arith_expansions(true_val, f);
            walk_arith_expansions(false_val, f);
        }
        ArithExpr::Comma(items) | ArithExpr::FuncCall { args: items, .. } => {
            for it in items {
                walk_arith_expansions(it, f);
            }
        }
        // A subscript's index (and an assignment's value) are arithmetic
        // subtrees too — skipping them let every expr-level check inside
        // `a[…]` go unrun, e.g. `(( y = a[${x:~}] ))`.
        ArithExpr::Subscript { index, .. } => walk_arith_expansions(index, f),
        ArithExpr::SubscriptAssign { index, value, .. } => {
            walk_arith_expansions(index, f);
            walk_arith_expansions(value, f);
        }
        _ => {}
    }
}

/// Walk expressions inside a TestExpr.
fn walk_test_exprs(t: &TestExpr, f: &mut impl FnMut(&Expr)) {
    match t {
        TestExpr::Unary { operand, .. } => walk_expr_tree(operand, f),
        TestExpr::Binary { left, right, .. } => {
            walk_expr_tree(left, f);
            walk_expr_tree(right, f);
        }
        TestExpr::Not(inner) => walk_test_exprs(inner, f),
        TestExpr::And(l, r) | TestExpr::Or(l, r) => {
            walk_test_exprs(l, f);
            walk_test_exprs(r, f);
        }
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

    /// { statements; }
    Group(Group),

    /// DD#55: `- cmd` — suppressed command (no implicit exit check)
    Suppressed(Box<Statement>),

    /// DD#70: `! cmd` as a *pipeline stage* (`a | ! b`) — a per-segment
    /// negation of the (terminal) stage's status, symmetric with `Suppressed`.
    /// The stage still runs; its status is inverted (0↔nonzero) before the
    /// DD#55 worst-of aggregate. Only ever appears inside a multi-stage
    /// pipeline's `commands`; the standalone `! cmd` uses `Pipeline.negated`.
    Negated(Box<Statement>),

    /// (( expr )) used as a condition — arithmetic test.
    /// Distinct from [[ ]] which is string/file tests only.
    ArithTest(Box<ArithExpr>),

    /// DD#74: `return <producer>` — a predicate return whose status is a
    /// producer (a command, `[[ … ]]`, or `(( … ))`), not a literal code.
    /// The producer is evaluated (consumed — it never aborts), and the
    /// function returns its exit status. `return N` stays a `Command`.
    ReturnProducer(Box<Statement>),
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
    Export,
    Readonly,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    pub name: Expr,
    pub args: Vec<Expr>,
}

/// DD#61: the parsed form of a `attest` command. The parser encodes the
/// form positionally in `Command::args`; `classify_attest` decodes it.
///
/// | args (excluding trailing Test) | form                          |
/// |--------------------------------|-------------------------------|
/// | `[var]`                        | `Scalar` — `attest v [[…]]`    |
/// | `[binder, array]`              | `Indexed` — `attest x in a …` |
/// | `[key, val, map]`              | `Map` — `attest k v in m …`   |
#[derive(Debug, Clone, PartialEq)]
pub enum AttestForm<'a> {
    Scalar {
        var: &'a str,
    },
    Indexed {
        binder: &'a str,
        array: &'a str,
    },
    Map {
        key: &'a str,
        val: &'a str,
        map: &'a str,
    },
}

fn attest_lit(e: &Expr) -> Option<&str> {
    match &e.kind {
        ExprKind::Literal(s) => Some(s.as_str()),
        _ => None,
    }
}

/// DD#77 D1: invoke `f(name)` for each bare variable *read* (`x` / `$x`) in an
/// `ArithExpr` tree. Does NOT descend into `Expansion` (a `${…}` disposition or
/// subscript is not a bare operand) and does NOT visit assignment/inc-dec
/// targets (those are mutation lvalues, tracked as arith-native ints).
pub fn walk_arith_var_reads(a: &ArithExpr, f: &mut impl FnMut(&str)) {
    match a {
        ArithExpr::Var(name) | ArithExpr::DollarVar(name) => f(name),
        ArithExpr::BinOp { left, right, .. } => {
            walk_arith_var_reads(left, f);
            walk_arith_var_reads(right, f);
        }
        ArithExpr::UnaryOp { operand, .. } | ArithExpr::Paren(operand) => {
            walk_arith_var_reads(operand, f)
        }
        ArithExpr::Assign { value, .. } | ArithExpr::CompoundAssign { value, .. } => {
            walk_arith_var_reads(value, f)
        }
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => {
            walk_arith_var_reads(cond, f);
            walk_arith_var_reads(true_val, f);
            walk_arith_var_reads(false_val, f);
        }
        ArithExpr::Comma(exprs) => {
            for e in exprs {
                walk_arith_var_reads(e, f);
            }
        }
        // Descend into a bare subscript's *index* (and a subscript-assign's
        // index/value): the index is itself an arithmetic operand, so a bare
        // string used there (`a[s]`) is the same string→int crossing (DD#77 D1).
        // The array *name* is handled by the bare-subscript cell check, not
        // here.
        ArithExpr::Subscript { index, .. } => walk_arith_var_reads(index, f),
        ArithExpr::SubscriptAssign { index, value, .. } => {
            walk_arith_var_reads(index, f);
            walk_arith_var_reads(value, f);
        }
        // NOT descended: `FuncCall` args (e.g. `${#s}` = `len(s)` passes the
        // *name*, not an int operand — flagging it would be a false positive);
        // `Expansion` (a `${…}` disposition/subscript); `Literal` /
        // Pre/PostIncDec (no bare read to flag).
        _ => {}
    }
}

/// What justifies an `attest` — the thing the author offers as evidence.
///
/// Normally a `[[ ]]` predicate. DD#93 Part D adds a second kind: an
/// int-assignment whose success *is* the validation, `attest numstr
/// (( myint = numstr ))` — the conversion can only succeed on a valid,
/// representable i64, so performing it proves the property.
#[derive(Debug, Clone, PartialEq)]
pub enum AttestPredicate<'a> {
    Test(&'a TestExpr),
    /// `target` is the int name bound clean to `value`'s result. `value` may be
    /// any expression that **references** the attested name (DG 2026-09-07):
    /// a wider RHS still requires the conversion to succeed, so the `i?` gate
    /// proves exactly as much as it does for the plain `(( t = s ))`.
    ArithConvert {
        target: &'a str,
        value: &'a ArithExpr,
    },
}

/// DD#93 Part D: decode `(( TARGET = EXPR ))` as an attest predicate. `EXPR`
/// must **reference** the attested name — computing it requires converting that
/// name, so the `i?` gate proves the same thing whether the RHS is the bare
/// source (`(( t = s ))`) or a wider expression (`(( t = s * 2 ))`).
///
/// An RHS that never mentions the attested name proves nothing about it, and is
/// not this form; `validate` reports those (SB5054).
fn attest_arith_convert<'a>(a: &'a ArithExpr, attested: &str) -> Option<(&'a str, &'a ArithExpr)> {
    let ArithExpr::Assign { var, value } = a else {
        return None;
    };
    arith_references_var(value, attested).then(|| (var.as_str(), value.as_ref()))
}

/// True if `a` reads `name` (`name` or `$name`) anywhere.
pub fn arith_references_var(a: &ArithExpr, name: &str) -> bool {
    let mut found = false;
    walk_arith_var_reads(a, &mut |n| {
        if n == name {
            found = true;
        }
    });
    found
}

/// Decoded `read` command flags — the SINGLE source of truth for how a
/// `read`'s argument words are classified, shared by the transform (SSA
/// lowering), validation (target collection + taint), and type checking.
/// Recognized flags: `-a ARR` (array target), `-d DELIM` (record delimiter),
/// `-f SEP` (field separator). A byte flag's value is joined (`-dX`) or split
/// (`-d X`); an EMPTY value (`-d ''`, or a bare trailing `-d`) means NUL
/// (`\0`) — the bash `read -d ''` convention. `-r` (raw) is accepted as an
/// ignored no-op (sash reads are always raw). Any other `-flag` is recorded in
/// `unknown_flag` (the transform turns that into SB5015).
#[derive(Debug, Default)]
pub struct ReadFlags {
    pub delim: Option<u8>,
    pub field_sep: Option<u8>,
    pub array_name: Option<String>,
    pub var_names: Vec<String>,
    pub unknown_flag: Option<String>,
}

/// The literal VALUE of a `read` argument word (unwrapping one layer of
/// quotes; an empty `''`/`""` is the empty string). Non-literal words
/// (`$x`, `$( )`, …) — never valid read targets or delimiters — yield "".
fn read_arg_value(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Literal(s) | ExprKind::SingleQuoted(s) => s.clone(),
        ExprKind::DoubleQuoted(parts) => match parts.as_slice() {
            [] => String::new(),
            [p] => match &p.kind {
                ExprKind::Literal(s) => s.clone(),
                _ => String::new(),
            },
            _ => String::new(),
        },
        _ => String::new(),
    }
}

/// Classify a `read` command's argument words. See [`ReadFlags`].
pub fn classify_read_flags(args: &[Expr]) -> ReadFlags {
    let words: Vec<String> = args.iter().map(read_arg_value).collect();
    let mut f = ReadFlags::default();
    let mut i = 0;
    while i < words.len() {
        i += 1 + classify_read_word(&words, i, &mut f);
    }
    f
}

/// Classify ONE `read` argument word at `words[i]`, updating `f`. Returns the
/// number of EXTRA words consumed (the value that follows a split `-a ARR` or
/// `-d DELIM`/`-f SEP`).
fn classify_read_word(words: &[String], i: usize, f: &mut ReadFlags) -> usize {
    let w = &words[i];
    if w == "-a" {
        take_read_str_flag(words, i, &mut f.array_name)
    } else if let Some(rest) = w.strip_prefix("-d") {
        take_read_byte_flag(rest, words, i, &mut f.delim)
    } else if let Some(rest) = w.strip_prefix("-f") {
        take_read_byte_flag(rest, words, i, &mut f.field_sep)
    } else if w == "-r" {
        // sash reads are ALWAYS raw (no backslash processing), so `-r`
        // requests the default — accept it as an ignored no-op.
        0
    } else if w.starts_with('-') {
        f.unknown_flag.get_or_insert_with(|| w.clone());
        0
    } else {
        f.var_names.push(w.clone());
        0
    }
}

/// Resolve a split string-valued flag (`-a ARR`) into `*slot`, returning EXTRA
/// words consumed (1 if a value word follows, else 0).
fn take_read_str_flag(words: &[String], i: usize, slot: &mut Option<String>) -> usize {
    if i + 1 < words.len() {
        *slot = Some(words[i + 1].clone());
        1
    } else {
        0
    }
}

/// Resolve a `-X` byte flag value into `*slot`, returning EXTRA words consumed
/// (0 for joined `-dX`, 1 for split `-d VALUE`). An empty value (`-d ''` or a
/// bare trailing `-d`) means NUL (`0`).
fn take_read_byte_flag(joined: &str, words: &[String], i: usize, slot: &mut Option<u8>) -> usize {
    if !joined.is_empty() {
        *slot = Some(joined.as_bytes()[0]);
        0
    } else if i + 1 < words.len() {
        *slot = Some(words[i + 1].as_bytes().first().copied().unwrap_or(0));
        1
    } else {
        *slot = Some(0);
        0
    }
}

/// Decode the args of a `attest` command into a [`AttestForm`] plus the
/// predicate that justifies it. Returns `None` if the args don't match any
/// attest shape (e.g. missing predicate, unexpected arity).
pub fn classify_attest(args: &[Expr]) -> Option<(AttestForm<'_>, AttestPredicate<'_>)> {
    let (last, lead) = args.split_last()?;
    match &last.kind {
        // DD#93 Part D: `attest SOURCE (( TARGET = SOURCE ))`.
        ExprKind::Arithmetic(a) => classify_attest_convert(lead, a),
        ExprKind::Test(t) => Some((attest_form_from_lead(lead)?, AttestPredicate::Test(t))),
        _ => None,
    }
}

/// DD#93 Part D: the scalar-only conversion form. The lead is a single name,
/// and the assignment's source must be that same name.
fn classify_attest_convert<'a>(
    lead: &'a [Expr],
    a: &'a ArithExpr,
) -> Option<(AttestForm<'a>, AttestPredicate<'a>)> {
    let [only] = lead else {
        return None;
    };
    let var = attest_lit(only)?;
    let (target, value) = attest_arith_convert(a, var)?;
    Some((
        AttestForm::Scalar { var },
        AttestPredicate::ArithConvert { target, value },
    ))
}

/// The attest form implied by the names before the predicate: one is a scalar,
/// two an indexed array, three a map (DD#61).
fn attest_form_from_lead(lead: &[Expr]) -> Option<AttestForm<'_>> {
    Some(match lead.len() {
        1 => AttestForm::Scalar {
            var: attest_lit(&lead[0])?,
        },
        2 => AttestForm::Indexed {
            binder: attest_lit(&lead[0])?,
            array: attest_lit(&lead[1])?,
        },
        3 => AttestForm::Map {
            key: attest_lit(&lead[0])?,
            val: attest_lit(&lead[1])?,
            map: attest_lit(&lead[2])?,
        },
        _ => return None,
    })
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
    /// DD#66: the dispatch match operator — `"=="` (exact) or `"=~"` (regex).
    pub op: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaseItem {
    pub patterns: Vec<Expr>,
    pub body: Vec<Statement>,
    /// DD#66/#71: the `|| { … }` default arm. `*` is an ordinary literal
    /// pattern (dispatch has no wildcards), so the default is flagged, not
    /// a synthetic `*` pattern.
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    pub return_type: VarType,
    pub body: Vec<Statement>,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RedirectOp {
    Write,     // >
    Append,    // >>
    Read,      // <
    DupOutput, // >&
    DupInput,  // <&
    ReadWrite, // <>
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
    /// `${a[i]:OP}` — an array/map cell subject. `None` for a plain scalar
    /// `${name:OP}`; `Some(index)` when the disposition applies to a cell
    /// (DG: a string cell behaves like a string variable). Only the `:-`/`:?`
    /// dispositions carry a cell subject; other operators reject upstream.
    pub index: Option<Box<Expr>>,
    /// DD#91 (2026-09-09 addendum): the `!` in `${x:!}` / `${x:!~}` / `@{a:!-W}`.
    ///
    /// A **modifier**, not a disposition: `-`/`?`/`~` say what to do when the
    /// value is absent, `!` says how to treat the value we have (trusted, so
    /// the taint guard does not apply). It is a separate flag precisely so the
    /// two cannot be conflated again — the retired `arrvals_trust` operator
    /// meant "elide AND trust", which forced the trust to inherit the elide's
    /// position restriction.
    pub taint_off: bool,
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
    PreIncDec { var: String, inc: bool },
    /// Assignment: var = expr
    Assign { var: String, value: Box<ArithExpr> },
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
    FuncCall { name: String, args: Vec<ArithExpr> },
    /// Parenthesized grouping
    Paren(Box<ArithExpr>),
    /// A `${…}` normal-expansion rvalue imported into arithmetic — e.g.
    /// `${a[i]}` / `${m[k]}` (a subscript cell read). The inner `Expr` is
    /// parsed by NORMAL expansion rules (so a map's string key works) and
    /// lowered by the ordinary `expr_to_value` path; it is rvalue-only (never
    /// an assignment target). See notes/tasks/arith-subscript-values.md.
    Expansion(Box<Expr>),
    /// DD arith-subscript model B (Stage 2): a **bare** int-array cell in
    /// arithmetic — `a[i]`, `a[i+1]`, `a[0]`, `a[$i]`. The index is a full
    /// arithmetic expression (int-valued). Rvalue read here; the lvalue forms
    /// (`a[i]++`, `a[i]=…`, `a[i] op= …`) are `SubscriptAssign`. A bare `m[k]`
    /// on an associative array is a type error (string key not expressible
    /// under arith rules — use `${m[k]}` to read).
    Subscript { name: String, index: Box<ArithExpr> },
    /// DD arith-subscript model B (Stage 2): a bare int-array cell as an
    /// assignment target — `a[i] = v` (`op` None), `a[i] op= v` (`op` Some),
    /// and `a[i]++`/`a[i]--` (lowered to `op = Add/Sub`, `value = 1`). `index`
    /// and `value` are arithmetic expressions.
    SubscriptAssign {
        name: String,
        index: Box<ArithExpr>,
        op: Option<IrArithOp>,
        value: Box<ArithExpr>,
    },
}

/// True if `a` contains an arithmetic mutation (`x = …`, `x += …`, `x++`/`x--`,
/// `a[i] = …`) anywhere in its tree. Shared by `validate` (to reject a mutation
/// that can't take effect where it sits — SB4020) and by the transform (to
/// decide whether a `(( … ))` needs its embedded stores hoisted). A mutation
/// node short-circuits to `true` without recursing into its rvalue.
pub(crate) fn arith_has_mutation(a: &ArithExpr) -> bool {
    match a {
        ArithExpr::Assign { .. }
        | ArithExpr::CompoundAssign { .. }
        | ArithExpr::PostIncDec { .. }
        | ArithExpr::PreIncDec { .. }
        | ArithExpr::SubscriptAssign { .. } => true,
        ArithExpr::BinOp { left, right, .. } => {
            arith_has_mutation(left) || arith_has_mutation(right)
        }
        ArithExpr::UnaryOp { operand, .. } => arith_has_mutation(operand),
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => {
            arith_has_mutation(cond)
                || arith_has_mutation(true_val)
                || arith_has_mutation(false_val)
        }
        ArithExpr::Comma(es) => es.iter().any(arith_has_mutation),
        ArithExpr::Paren(inner) => arith_has_mutation(inner),
        ArithExpr::FuncCall { args, .. } => args.iter().any(arith_has_mutation),
        ArithExpr::Subscript { index, .. } => arith_has_mutation(index),
        ArithExpr::Literal(_)
        | ArithExpr::Var(_)
        | ArithExpr::DollarVar(_)
        | ArithExpr::Expansion(_) => false,
    }
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
