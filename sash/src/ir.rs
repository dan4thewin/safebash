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

    /// The command-substitution bodies this expression owns directly: the
    /// per-expression half of [`Statement::cmdsub_bodies`], for a statement
    /// whose expressions do not all run at one time — a declaration evaluates
    /// and binds each initializer before it evaluates the next.
    pub fn cmdsub_bodies(&self) -> Vec<&[Statement]> {
        let mut bodies = Vec::new();
        walk_expr_tree(self, &mut |e| push_cmdsub_body(e, &mut bodies));
        bodies
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
    ///
    /// **A `$( … )` body is one of these.** DD#86 makes it an anonymous
    /// function body, so its statements are statements and every statement
    /// rule applies to them. They used to be reachable only from the
    /// *expression* side ([`walk_expr_tree`] descended through
    /// `CommandSubstitution`), which left 42 of the 45 statement walkers in
    /// `validate.rs` blind to them: `$(unset zz)` and `$(break)` and
    /// `$(for args in …)` were accepted in silence, `$(mapfile -m q)` slipped
    /// past its DD#99 retirement, and `$(signal INT ignore)` reached a
    /// transform that had been promised a validated signal number and
    /// panicked on it.
    ///
    /// The two sides are exclusive on purpose. `walk_expr_tree` now stops at
    /// the substitution node, so a body's expressions are reached exactly once
    /// — through this recursion — and a diagnostic raised from an expression
    /// callback is emitted once, not depth+1 times (journal 308).
    pub fn child_bodies(&self) -> Vec<&[Statement]> {
        let mut bodies = self.structural_bodies();
        bodies.extend(self.cmdsub_bodies());
        bodies
    }

    /// The command-substitution bodies this statement owns directly.
    ///
    /// Split out of [`Self::child_bodies`] for the one analysis that must tell
    /// the two apart: a value intrinsic is legal as the whole of a `$( … )`
    /// and nowhere else, so `validate` needs to know which body a statement
    /// came from. Everything else wants `child_bodies`.
    ///
    /// A `$( … $( … ) … )` yields only the outer body: the inner substitution
    /// belongs to a statement *inside* that body, and is reached when the
    /// recursion gets there.
    pub fn cmdsub_bodies(&self) -> Vec<&[Statement]> {
        let mut bodies = Vec::new();
        walk_own_exprs(self, &mut |e| push_cmdsub_body(e, &mut bodies));
        bodies
    }

    /// The bodies named by this statement's own syntax — an `if` arm, a loop
    /// body, a function body. The other half of [`Self::child_bodies`].
    pub fn structural_bodies(&self) -> Vec<&[Statement]> {
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

    /// DD#122: does this pipeline final stage contain a reader the pipe binds
    /// to?
    ///
    /// A pipeline delivers its data to the final stage one of two ways: a
    /// *forked* stage inherits the pipe as fd 0, or a reader running in the
    /// *parent* is pointed at the pipe's read end. A `while` final stage runs
    /// in the parent on purpose — `cmd | while readline v; do (( n = n + 1 ));
    /// done` leaves `n` set where bash loses it — so the pipe reaches such a
    /// stage only through a `readline` or a `read`.
    ///
    /// Deliberately does NOT descend into a nested `Pipeline`, a `Function`
    /// definition, or a `Redirected` body: a reader in any of those binds to
    /// something else (that pipeline's own pipe, a later call, the redirect).
    /// Lowering's lexical reader binding follows these same scope boundaries;
    /// it is suspended across calls, captures, redirects and background children.
    pub fn binds_a_pipe_reader(&self) -> bool {
        if self.is_pipe_reader() {
            return true;
        }
        if matches!(
            self.kind,
            StmtKind::Pipeline(_) | StmtKind::Function(_) | StmtKind::Redirected(_)
        ) {
            return false;
        }
        self.structural_bodies()
            .iter()
            .any(|body| body.iter().any(Statement::binds_a_pipe_reader))
    }

    /// The two DD#99 readers that consume a pipe in the parent process:
    /// `readline NAME` (one line → scalar) and `read ARRAY` (NUL-split).
    pub(crate) fn is_pipe_reader(&self) -> bool {
        let StmtKind::Command(c) = &self.kind else {
            return false;
        };
        matches!(&c.name.kind, ExprKind::Literal(n) if n == "readline" || n == "read")
    }

    /// A pipeline stage's `-` (DD#55) and `!` (DD#70) marks, peeled: the
    /// statement beneath them, whether the stage is suppressed, and whether it
    /// is negated. A stage carries at most one of them — the parser refuses
    /// `- !` and `! -` alike (SB4019) — but the peel does not rely on that.
    pub fn stage_marks(&self) -> (&Statement, bool, bool) {
        let (mut cur, mut suppressed, mut negated) = (self, false, false);
        loop {
            match &cur.kind {
                StmtKind::Suppressed(inner) => {
                    suppressed = true;
                    cur = inner;
                }
                StmtKind::Negated(inner) => {
                    negated = true;
                    cur = inner;
                }
                _ => return (cur, suppressed, negated),
            }
        }
    }

    /// The statement a pipeline stage actually runs, beneath its `-` / `!`
    /// marks and its redirect. **Every stage classifier peels through this.**
    /// Private copies of this loop once hid suppressed external producers
    /// from both taint passes. DD#127 now taints every pipe read, but stage
    /// resolution and grammar checks still need the same underlying command.
    pub fn stage_body(&self) -> &Statement {
        let mut cur = self;
        loop {
            match &cur.kind {
                StmtKind::Suppressed(inner) | StmtKind::Negated(inner) => cur = inner,
                StmtKind::Redirected(r) => cur = &r.body,
                _ => return cur,
            }
        }
    }

    /// Mutable mirror of [`Self::structural_bodies`] — deliberately not of
    /// [`Self::child_bodies`], which also yields `$( … )` bodies. A rewriting
    /// pass that wants those has to say so, because a substitution body is a
    /// *value* and rewriting it in place is not the same operation as
    /// rewriting a loop body.
    pub fn structural_bodies_mut(&mut self) -> Vec<&mut [Statement]> {
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

    /// Mutable mirror of [`Self::cmdsub_bodies`]: the `$( … )` bodies this
    /// statement owns directly, in the same order. This is how a rewriting
    /// pass says it wants them (see [`Self::structural_bodies_mut`]).
    ///
    /// A rewriting pass must reach exactly the bodies the checkers reach.
    /// DD#23's command resolution once kept a private copy of this walk, and
    /// the copy never learned that a body can sit in a parameter-expansion
    /// index (`${a[$(cat f)]:-x}`), under `$(( … ))`, or in a `(( … ))`
    /// condition. The canonical walk was fixed for each; the copy was not.
    /// Those heads stayed bare, the taint pass judges a bare head by its
    /// spelling, and an external program's output was bound clean. Pinned
    /// against [`Self::cmdsub_bodies`] by
    /// `cmdsub_twin_tests::cmdsub_bodies_mut_mirrors_cmdsub_bodies`.
    pub fn cmdsub_bodies_mut(&mut self) -> Vec<&mut [Statement]> {
        let mut bodies = Vec::new();
        for root in self.own_expr_roots_mut() {
            push_cmdsub_bodies_mut(root, &mut bodies);
        }
        bodies
    }

    /// The roots of the expressions this statement owns itself, mutably: the
    /// [`Self::direct_exprs`] set in its order, plus the `${…}` expansions of a
    /// `(( … ))` statement, which [`walk_own_exprs`] adds on the immutable
    /// side.
    ///
    /// **Total over `StmtKind` on purpose — do not add a wildcard**, for the
    /// reason `direct_exprs` gives: a kind that gains an `Expr` must be a
    /// compile error here, not a body this walk silently skips.
    fn own_expr_roots_mut(&mut self) -> Vec<&mut Expr> {
        match &mut self.kind {
            StmtKind::Assignment(a) => std::iter::once(&mut a.value)
                .chain(a.index.as_mut())
                .collect(),
            StmtKind::Declaration(d) => d
                .assignments
                .iter_mut()
                .filter_map(|a| a.value.as_mut())
                .collect(),
            StmtKind::Command(c) => std::iter::once(&mut c.name).chain(&mut c.args).collect(),
            StmtKind::For(fo) => fo.words.iter_mut().collect(),
            StmtKind::Case(c) => std::iter::once(&mut c.value)
                .chain(c.items.iter_mut().flat_map(|item| &mut item.patterns))
                .collect(),
            StmtKind::Redirected(r) => r.redirects.iter_mut().map(|r| &mut r.target).collect(),
            StmtKind::ArithTest(arith) => {
                let mut roots = Vec::new();
                walk_arith_expansion_roots_mut(arith, &mut |e| roots.push(e));
                roots
            }
            StmtKind::Pipeline(_)
            | StmtKind::If(_)
            | StmtKind::While(_)
            | StmtKind::Function(_)
            | StmtKind::List(_)
            | StmtKind::Group(_)
            | StmtKind::Suppressed(_)
            | StmtKind::Negated(_)
            | StmtKind::ReturnProducer(_) => Vec::new(),
        }
    }

    /// The child statement bodies of an `if` (condition, then, each elif's
    /// condition + body, and else). Factored out of
    /// [`Self::structural_bodies`] to keep that dispatcher simple.
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
    ///
    /// **Total over `StmtKind` on purpose — do not add a wildcard.** The point
    /// of this getter is that adding a statement kind which owns an `Expr` is a
    /// compile error here rather than a check that silently stops running. A
    /// wildcard put ten variants beyond anyone's notice; the nine that own only
    /// `Statement`s are spelled out below so that fact is asserted, not assumed.
    ///
    /// `ArithTest` is the one variant that owns `Expr`s and still yields
    /// nothing. Its children hang off an `ArithExpr`, reachable only through
    /// `ArithExpr::Expansion`, and the passes that want them also want
    /// `ArithExpr` nodes that are not `Expr`s at all (`DollarVar`), so they walk
    /// the arith tree themselves via `walk_arith_expansion_roots`. Surfacing the
    /// `Expansion` payloads here would double-report for those callers. That is
    /// a deliberate seam, not an oversight — see
    /// `notes/tasks/getters-still-ending-in-a-wildcard.md`.
    pub fn direct_exprs<'a>(&'a self, f: &mut impl FnMut(&'a Expr)) {
        match &self.kind {
            StmtKind::Assignment(a) => assignment_exprs(a, f),
            StmtKind::Declaration(d) => decl_exprs(d, f),
            StmtKind::Command(c) => command_exprs(c, f),
            StmtKind::For(fo) => visit_each(&fo.words, f),
            StmtKind::Case(c) => case_exprs(c, f),
            StmtKind::Redirected(r) => redirect_target_exprs(&r.redirects, f),
            // Own only `Statement`s — their exprs belong to `child_bodies()`.
            StmtKind::Pipeline(_)
            | StmtKind::If(_)
            | StmtKind::While(_)
            | StmtKind::Function(_)
            | StmtKind::List(_)
            | StmtKind::Group(_)
            | StmtKind::Suppressed(_)
            | StmtKind::Negated(_)
            | StmtKind::ReturnProducer(_) => {}
            // Owns `Expr`s under an `ArithExpr`; see the note above.
            StmtKind::ArithTest(_) => {}
        }
    }
}

/// Collect `e`'s body if it is a `$( … )`: the test
/// [`Statement::cmdsub_bodies`] applies at each node [`walk_own_exprs`]
/// visits, named so that [`push_cmdsub_bodies_mut`] can say which test it
/// mirrors.
fn push_cmdsub_body<'a>(e: &'a Expr, bodies: &mut Vec<&'a [Statement]>) {
    if let ExprKind::CommandSubstitution(stmts) = &e.kind {
        bodies.push(stmts.as_slice());
    }
}

/// Collect, mutably, every `$( … )` body under `e` that [`push_cmdsub_body`]
/// collects through [`walk_expr_tree`]: the same nodes, in the same order,
/// stopping at each substitution as that walk does.
///
/// A mutable walk cannot hand a callback a node and then descend into it, so
/// this is the recursion itself rather than a callback on a
/// `walk_expr_tree_mut`. **Total over `ExprKind`, as that walk is — do not add
/// a wildcard.**
fn push_cmdsub_bodies_mut<'a>(e: &'a mut Expr, bodies: &mut Vec<&'a mut [Statement]>) {
    match &mut e.kind {
        ExprKind::CommandSubstitution(stmts) => bodies.push(stmts.as_mut_slice()),
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) | ExprKind::Array(parts) => {
            push_each_cmdsub_body_mut(parts.iter_mut(), bodies)
        }
        ExprKind::AssocArray(pairs) => {
            push_each_cmdsub_body_mut(pairs.iter_mut().map(|(_, v)| v), bodies)
        }
        ExprKind::ParameterExpansion(pe) => push_each_cmdsub_body_mut(
            pe.index
                .as_deref_mut()
                .into_iter()
                .chain(pe.operand.as_deref_mut()),
            bodies,
        ),
        ExprKind::Test(t) => {
            walk_test_operands_mut(t, &mut |operand| push_cmdsub_bodies_mut(operand, bodies))
        }
        ExprKind::Subscript { index, .. } | ExprKind::SlurpFile(index) => {
            push_cmdsub_bodies_mut(index, bodies)
        }
        ExprKind::Arithmetic(arith) => {
            walk_arith_expansion_roots_mut(arith, &mut |root| push_cmdsub_bodies_mut(root, bodies))
        }
        ExprKind::Literal(_) | ExprKind::SingleQuoted(_) | ExprKind::SimpleExpansion(_) => {}
    }
}

/// [`push_cmdsub_bodies_mut`] over each expression a form holds, whatever the
/// shape of its container: a list, the values of a pair list, or a pair of
/// options.
fn push_each_cmdsub_body_mut<'a>(
    exprs: impl Iterator<Item = &'a mut Expr>,
    bodies: &mut Vec<&'a mut [Statement]>,
) {
    for e in exprs {
        push_cmdsub_bodies_mut(e, bodies);
    }
}

/// Call `f` on each expression in turn.
///
/// Not [`walk_each`], which recurses through [`walk_expr_tree`]. Everything
/// reached from [`Statement::direct_exprs`] is deliberately one level deep —
/// combining the two visits each nested expression once per enclosing level
/// (journal 308).
fn visit_each<'a>(items: &'a [Expr], f: &mut impl FnMut(&'a Expr)) {
    for it in items {
        f(it);
    }
}

fn assignment_exprs<'a>(a: &'a Assignment, f: &mut impl FnMut(&'a Expr)) {
    f(&a.value);
    if let Some(idx) = &a.index {
        f(idx);
    }
}

fn command_exprs<'a>(c: &'a Command, f: &mut impl FnMut(&'a Expr)) {
    f(&c.name);
    visit_each(&c.args, f);
}

fn case_exprs<'a>(c: &'a Case, f: &mut impl FnMut(&'a Expr)) {
    f(&c.value);
    for item in &c.items {
        visit_each(&item.patterns, f);
    }
}

fn redirect_target_exprs<'a>(redirects: &'a [Redirect], f: &mut impl FnMut(&'a Expr)) {
    for r in redirects {
        f(&r.target);
    }
}

/// Visit the expressions a statement owns *itself*: its [`Statement::direct_exprs`]
/// each expanded through [`walk_expr_tree`], plus the `${…}` expansions inside a
/// `(( … ))` statement's `ArithExpr` tree. Does **not** descend into child
/// statements.
///
/// An analysis that already recurses over `child_bodies()` must use this rather
/// than [`walk_all_exprs`]. Combining the two visits every nested expression
/// once per enclosing level, so a diagnostic raised from the callback is emitted
/// depth+1 times (journal 308).
pub fn walk_own_exprs<'a>(stmt: &'a Statement, f: &mut impl FnMut(&'a Expr)) {
    stmt.direct_exprs(&mut |expr| walk_expr_tree(expr, f));
    // `(( … ))` statement / `if`/`while` condition — its `${…}` expansions live
    // in an `ArithExpr` tree, not a direct `Expr`, so surface them here (mirrors
    // the `ExprKind::Arithmetic` arm in walk_expr_tree for `$(( … ))`).
    if let StmtKind::ArithTest(arith) = &stmt.kind {
        walk_arith_expansions(arith, f);
    }
}

/// Recursively walk all expressions in a statement tree.
///
/// Visits every `Expr` node in the statement and all nested statements.
/// The callback receives each expression (including sub-expressions
/// inside DoubleQuoted, Concat, CommandSubstitution, etc.).
pub fn walk_all_exprs<'a>(stmt: &'a Statement, f: &mut impl FnMut(&'a Expr)) {
    walk_own_exprs(stmt, f);
    for body in stmt.child_bodies() {
        for s in body {
            walk_all_exprs(s, f);
        }
    }
}

/// The single command whose value a substitution resolves. Suppression changes
/// checking, not its value or type; redirects require ordinary body capture.
pub(crate) fn single_value_command(stmts: &[Statement]) -> Option<&Command> {
    let [stmt] = stmts else {
        return None;
    };
    value_command(stmt)
}

/// The command one statement runs, looking through any suppression prefix:
/// `- argv 1` runs `argv 1` (DD#55 — suppression changes checking, not what
/// is run or what it yields).
///
/// The shared peel for a capture-shape recogniser: [`single_value_command`]
/// and the signature scan's `cmdsub_reads_whole_args` go through it, so
/// `$(- join args)` reads the whole argument list just as `$(join args)` does.
/// A recogniser that matches `StmtKind::Command` itself forgets `Suppressed`;
/// the ones that still do are listed in
/// `notes/tasks/bug-peel-family-classifiers.md`.
///
/// A `Redirected` statement is not peeled, and for these callers there is
/// nothing to peel: every intrinsic refuses a redirect at parse time (SB4046),
/// so a redirected body is never one of the commands they look for.
pub(crate) fn value_command(stmt: &Statement) -> Option<&Command> {
    match &stmt.kind {
        StmtKind::Command(c) => Some(c),
        StmtKind::Suppressed(inner) => value_command(inner),
        _ => None,
    }
}

/// Recognise a nullary intrinsic call — the `$(name)` shape that
/// `parse::expr::intrinsic_expansion` builds, including a suppression prefix
/// which does not change which intrinsic is read. Returns the intrinsic's name.
///
/// **This is the inverse of `parse::expr::intrinsic_expansion` and the two
/// must change together**; `parse::tests::intrinsic_expansion_round_trips`
/// fails if they drift. That pairing is the point: DD#111 makes the intrinsic
/// the real thing and the punctuation (`$?`, `$$`, `$!`) mere sugar, so every
/// pass that used to look for `SimpleExpansion("?")` now has to recognise this
/// shape instead. A recogniser that silently stopped matching would not fail a
/// spec — it would just make its caller see nothing, which is exactly how
/// SB5012's availability state machine could have gone vacuous.
///
/// Requires no arguments: `$(status 5)` is a different thing (SB4045 rejects
/// it — `status` is a value intrinsic of arity 0), not a status read.
pub fn intrinsic_call_name(expr: &Expr) -> Option<&str> {
    let ExprKind::CommandSubstitution(stmts) = &expr.kind else {
        return None;
    };
    let c = single_value_command(stmts)?;
    if !c.args.is_empty() {
        return None;
    }
    match &c.name.kind {
        ExprKind::Literal(n) => Some(n),
        _ => None,
    }
}

/// The literal text of a word, or `None` when the word is not a compile-time
/// literal.
///
/// A bare, single-quoted or double-quoted literal yields its **value** (not
/// its source form), and `''`/`""` yield the empty string. Anything richer —
/// an expansion, a concatenation, a command substitution — yields `None`.
///
/// `None` is the whole point. The predecessor of this function fell back to
/// rendering the expression's *source text*, so `$(join a "$sep")` silently
/// joined with the four characters `"$sep"`. A slot that requires a
/// compile-time constant must be able to say "this is not one" rather than
/// invent a plausible string. Callers that need a constant pair this with a
/// validator rejection, so the `None` case never reaches lowering.
pub fn literal_word_text(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Literal(s) | ExprKind::SingleQuoted(s) => Some(s.clone()),
        ExprKind::DoubleQuoted(parts) => match parts.as_slice() {
            [] => Some(String::new()),
            [p] => match &p.kind {
                ExprKind::Literal(s) => Some(s.clone()),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

/// The one definition of a for-head `$( … )` that iterates a collection:
/// exactly `$(keys NAME)`. Yields the map argument, or `None` for every other
/// substitution body.
///
/// This exists so the transform **reads** the shape instead of re-deriving it.
/// It used to re-derive it, as four `let … else { unreachable!() }` guards that
/// re-asked SB5009's own question about the very same `ExprKind` SB5009 had
/// already inspected — a second copy of one assertion, which is not a backstop
/// (DG, 2026-10-05: a backstop earns its place only when it catches the problem
/// *orthogonally*; one that re-asks the gate's question on unchanged data is no
/// more trustworthy than the gate). The duplicate predicate was also free to
/// drift from the rule it quoted.
///
/// `None` is an ordinary answer, not a silent wrong one: it is the same `None`
/// every non-collection for-in word yields, and it routes to the word-list path
/// that SB5009 has already vetted. The bodies this rejects do not reach here —
/// `$(cat f; keys m)` and `$(keys m; keys m)` are SB5073, `$(keys)` is SB4045
/// (probed 2026-10-05) — and if one ever did, it would be iterated as words
/// rather than mis-lowered as a collection.
pub fn for_head_keys(stmts: &[Statement]) -> Option<&Expr> {
    let [stmt] = stmts else { return None };
    let StmtKind::Command(c) = &stmt.kind else {
        return None;
    };
    let ExprKind::Literal(name) = &c.name.kind else {
        return None;
    };
    if name != "keys" {
        return None;
    }
    c.args.first()
}

/// Walk an expression tree, visiting the node and all sub-expressions.
///
/// **Stops at a `$( … )`**: the node is visited, its body is not. DD#86 makes
/// the body an anonymous function body, so its statements belong to the
/// statement recursion — [`Statement::child_bodies`] yields it, and that is
/// where its expressions are reached. Descending here as well would visit
/// every expression in a body once per enclosing level and emit each
/// diagnostic depth+1 times (journal 308).
///
/// Total over `ExprKind` on purpose — do not add a wildcard.
pub fn walk_expr_tree<'a>(expr: &'a Expr, f: &mut impl FnMut(&'a Expr)) {
    f(expr);
    match &expr.kind {
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) => walk_each(parts, f),
        ExprKind::Array(elems) => walk_each(elems, f),
        ExprKind::AssocArray(pairs) => walk_each_pair(pairs, f),
        ExprKind::ParameterExpansion(pe) => {
            walk_opt(pe.index.as_deref(), f);
            walk_opt(pe.operand.as_deref(), f);
        }
        ExprKind::Test(t) => walk_test_exprs(t, f),
        ExprKind::Subscript { index, .. } => walk_expr_tree(index, f),
        // `$(( … ))` — descend into any `${…}` expansions embedded in the arith
        // tree (`ArithExpr::Expansion`), so expr-level checks (e.g. the DD#65
        // pruned-operator SB4034 reject) apply inside arithmetic too. Without
        // this, `$(( ${s:?…} ))` bypassed the reject and ICE'd in codegen.
        ExprKind::Arithmetic(arith) => walk_arith_expansions(arith, f),
        // `$(<file)` — the path is an ordinary expression and may itself be an
        // expansion (`$(<$f)`), so expr-level checks must reach it. Omitting
        // this arm used to be silent behind a wildcard, leaving the subtree
        // unvisited. The match is now exhaustive.
        ExprKind::SlurpFile(path) => walk_expr_tree(path, f),
        ExprKind::Literal(_)
        | ExprKind::SingleQuoted(_)
        | ExprKind::SimpleExpansion(_)
        | ExprKind::CommandSubstitution(_) => {}
    }
}

/// Walk each expr in a slice via `walk_expr_tree`.
fn walk_each<'a>(items: &'a [Expr], f: &mut impl FnMut(&'a Expr)) {
    for it in items {
        walk_expr_tree(it, f);
    }
}

/// Walk the value of each `(key, value)` pair (assoc-array literal).
fn walk_each_pair<'a>(pairs: &'a [(String, Expr)], f: &mut impl FnMut(&'a Expr)) {
    for (_, v) in pairs {
        walk_expr_tree(v, f);
    }
}

/// Walk an optional operand expr.
fn walk_opt<'a>(operand: Option<&'a Expr>, f: &mut impl FnMut(&'a Expr)) {
    if let Some(e) = operand {
        walk_expr_tree(e, f);
    }
}

/// Visit every statement in a tree, including those in nested bodies.
pub fn walk_stmt_tree(stmts: &[Statement], f: &mut impl FnMut(&Statement)) {
    walk_scoped_stmts(stmts, "", &mut |stmt, _| f(stmt));
}

/// Visit every statement in a tree, tracking the name of the enclosing
/// function — `""` at the top level.
///
/// Function bodies are reached through `child_bodies` like any other nested
/// body, so a nested function simply rebinds the name for its own subtree. A
/// visitor that cares only about top-level statements tests `in_fn.is_empty()`
/// rather than skipping `StmtKind::Function` itself.
pub fn walk_scoped_stmts<F: FnMut(&Statement, &str)>(stmts: &[Statement], in_fn: &str, f: &mut F) {
    for stmt in stmts {
        f(stmt, in_fn);
        let inner = match &stmt.kind {
            StmtKind::Function(fun) => fun.name.as_str(),
            _ => in_fn,
        };
        for body in stmt.child_bodies() {
            walk_scoped_stmts(body, inner, f);
        }
    }
}

/// [`walk_scoped_stmts`] restricted to [`Statement::structural_bodies`] — it
/// does **not** enter a `$( … )` body.
///
/// For a visitor that only reads, entering one is right: a body shares the
/// enclosing function's `args`, so `${args[0]}` inside `$( … )` in `fn g` is
/// `g`'s first argument, and `in_fn` naming `g` is the true answer.
///
/// For a visitor that is building a set of names *in scope*, it is wrong. A
/// name first introduced inside a body is local to that body (DD#86), so
/// collecting it as though it were declared in the enclosing scope invents a
/// binding. `collect_global_names` did exactly that once bodies joined the
/// statement recursion, and an unrelated function's own local was reported as
/// tainting a global that does not exist.
pub fn walk_scoped_structural_stmts<F: FnMut(&Statement, &str)>(
    stmts: &[Statement],
    in_fn: &str,
    f: &mut F,
) {
    for stmt in stmts {
        f(stmt, in_fn);
        let inner = match &stmt.kind {
            StmtKind::Function(fun) => fun.name.as_str(),
            _ => in_fn,
        };
        for body in stmt.structural_bodies() {
            walk_scoped_structural_stmts(body, inner, f);
        }
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

/// DD#100: the number of collection-parameter slots a function may receive.
pub(crate) const MAX_COLLECTION_SLOTS: usize = 5;

/// DD#100: `A`..`E` are the reserved collection-parameter slots. A call passes
/// a collection with `=name`; the callee sees it as `A`..`E` in call order.
/// Reserved on the `args` precedent (DD#75 §5) — they can't be declared as
/// user variables, and outside a function they name nothing at all.
pub(crate) fn is_collection_slot(name: &str) -> bool {
    name.len() == 1 && matches!(name.as_bytes()[0], b'A'..=b'E')
}

/// The slot name for the `i`th collection argument (`0` → `"A"`).
pub(crate) fn collection_slot_name(i: usize) -> String {
    ((b'A' + i as u8) as char).to_string()
}

/// DD#100: the collection named by a `=name` call argument, or `None` if this
/// argument is not the sigil form.
///
/// Only a **bare** `Literal` counts. `f '=foo'` and `f "=foo"` are
/// `SingleQuoted`/`DoubleQuoted` and pass the literal string `=foo` — that is
/// the escape hatch. The sigil is a position marker, not a value, so it does
/// not compose: the tail must be a plain identifier, which rejects `==a`,
/// `=a[0]`, `=@a` and `=` alone.
pub(crate) fn collection_arg_name(e: &Expr) -> Option<String> {
    let text = match &e.kind {
        ExprKind::Literal(s) => s.clone(),
        // The scanner emits `=` as its own token, so a bare `=name` argument
        // reaches the AST as a Concat of literal pieces, not one Literal.
        ExprKind::Concat(parts) => {
            let mut out = String::new();
            for p in parts {
                let ExprKind::Literal(s) = &p.kind else {
                    return None; // `=$x`, `=@a` — a value crept in, so not a sigil
                };
                out.push_str(s);
            }
            out
        }
        _ => return None,
    };
    let name = text.strip_prefix('=')?;
    is_plain_identifier(name).then(|| name.to_string())
}

/// The `=name` arguments of a call, in order.
pub(crate) fn collection_args(args: &[Expr]) -> Vec<String> {
    args.iter().filter_map(collection_arg_name).collect()
}

/// A plausible user identifier — excludes punctuation specials (`$?`, `$$`,
/// `$!`, `$#`, `$@`, `$*`, `$-`) and positionals (`$1`…) by construction.
pub(crate) fn is_plain_identifier(s: &str) -> bool {
    let mut bytes = s.bytes();
    match bytes.next() {
        Some(c) if c.is_ascii_alphabetic() || c == b'_' => {}
        _ => return false,
    }
    bytes.all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

/// Walk the `${…}` expansions embedded in an `ArithExpr` tree, applying `f` to
/// each imported `Expr` (via `walk_expr_tree`).
pub(crate) fn walk_arith_expansions<'a>(arith: &'a ArithExpr, f: &mut impl FnMut(&'a Expr)) {
    walk_arith_expansion_roots(arith, &mut |e| walk_expr_tree(e, f));
}

/// The same traversal as [`walk_arith_expansions`], but visiting each embedded
/// `${…}` **once at its root** instead of descending into it.
///
/// For a caller that recurses itself — `validate::check_expr_taint` does —
/// the descending form visits every node twice and reports each nested read
/// twice (`$(( ${a[$x]} ))` would name `$x` once from the callback's own
/// recursion and again from `walk_expr_tree`'s). Two entry points into one
/// traversal, rather than a second copy of the structure that can drift.
pub(crate) fn walk_arith_expansion_roots<'a>(arith: &'a ArithExpr, f: &mut impl FnMut(&'a Expr)) {
    match arith {
        ArithExpr::Expansion(inner) => f(inner),
        ArithExpr::BinOp { left, right, .. } => {
            walk_arith_expansion_roots(left, f);
            walk_arith_expansion_roots(right, f);
        }
        ArithExpr::UnaryOp { operand, .. } | ArithExpr::Paren(operand) => {
            walk_arith_expansion_roots(operand, f)
        }
        ArithExpr::Assign { value, .. } | ArithExpr::CompoundAssign { value, .. } => {
            walk_arith_expansion_roots(value, f)
        }
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => {
            walk_arith_expansion_roots(cond, f);
            walk_arith_expansion_roots(true_val, f);
            walk_arith_expansion_roots(false_val, f);
        }
        ArithExpr::Comma(items) | ArithExpr::FuncCall { args: items, .. } => {
            for it in items {
                walk_arith_expansion_roots(it, f);
            }
        }
        // A subscript's index (and an assignment's value) are arithmetic
        // subtrees too — skipping them let every expr-level check inside
        // `a[…]` go unrun, e.g. `(( y = a[${x:~}] ))`.
        ArithExpr::Subscript { index, .. } => walk_arith_expansion_roots(index, f),
        ArithExpr::SubscriptAssign { index, value, .. } => {
            walk_arith_expansion_roots(index, f);
            walk_arith_expansion_roots(value, f);
        }
        // Total on purpose — these five are spelled out rather than left to a
        // wildcard. Every one carries a `String` or an `i64`, never an `Expr`,
        // so this walker (whose whole contract is "hand `f` each embedded
        // `Expr`") has nothing to give. Checked against `ArithExpr::children()`
        // below, which is total and is the ground truth for this enum.
        //
        // The wildcard was sound. It is gone anyway because the cost of being
        // wrong went up: there are now *three* callers, all taint checks
        // (`check_arith_taint`, `arith_uses_tainted`, `reject_scalar_rhs_read`),
        // so an `ArithExpr` variant added later with an `Expr` inside it would
        // silently skip taint in three places at once and compile. That is the
        // exact shape that produced the SB5021 oracle in journal 423.
        //
        // `PreIncDec` cannot arrive at all — `reject_prefix_incdec` raises
        // SB4020 in the *parser* (PREFIX_INCDEC_INVARIANT). It stays a no-op
        // rather than an ICE because the assertion belongs where the invariant
        // is load-bearing, and that is `children()`, which already has it.
        ArithExpr::Literal(_)
        | ArithExpr::Var(_)
        | ArithExpr::DollarVar(_)
        | ArithExpr::PostIncDec { .. }
        | ArithExpr::PreIncDec { .. } => {}
    }
}

/// Visit every node of an `ArithExpr` tree, outermost first. Does not descend
/// into embedded `${…}` expansions: those hold `Expr`s, and an `Expr` walk
/// reaches any `$(( … ))` inside them.
///
/// **The one arithmetic node walker.** Descent is delegated to
/// [`ArithExpr::children`], which is total over the enum, so a caller only
/// names the variants it cares about.
pub(crate) fn walk_arith_nodes<'a>(a: &'a ArithExpr, f: &mut impl FnMut(&'a ArithExpr)) {
    walk_arith_nodes_pruned(a, &mut |n| {
        f(n);
        true
    });
}

/// [`walk_arith_nodes`] for a caller that does not want every subtree: `f`
/// answers whether to descend below the node it was handed. The operands of
/// `(len …)` / `(strlen …)` are rebuilt as command words, not arithmetic, so a
/// caller asking what a tree *evaluates* stops there.
pub(crate) fn walk_arith_nodes_pruned<'a>(
    a: &'a ArithExpr,
    f: &mut impl FnMut(&'a ArithExpr) -> bool,
) {
    if f(a) {
        for child in a.children() {
            walk_arith_nodes_pruned(child, f);
        }
    }
}

/// Invoke `f(name, args)` for each sexpr `FuncCall` in an `ArithExpr` tree,
/// outermost first, through [`walk_arith_nodes`].
///
/// **The one arithmetic call walker.** It replaces three private copies.
/// type_check's spelled out its own recursion behind a `_ => {}` that
/// swallowed `Subscript`/`SubscriptAssign`, whose indices are arithmetic, so
/// `$(( a[(toupper x)] ))` passed every string-callee rule and printed `1`,
/// and a slot-using function called under a subscript escaped the signature
/// check and reached a native codegen ICE (B73). validate's two copies
/// already delegated to `children`.
pub(crate) fn walk_arith_funccalls(a: &ArithExpr, f: &mut impl FnMut(&str, &[ArithExpr])) {
    walk_arith_nodes(a, &mut |n| {
        if let ArithExpr::FuncCall { name, args } = n {
            f(name, args);
        }
    });
}

/// Mutable mirror of [`walk_arith_expansion_roots`]: the same arms in the same
/// order, for a rewriting pass. Total for the same reason — do not add a
/// wildcard.
fn walk_arith_expansion_roots_mut<'a>(arith: &'a mut ArithExpr, f: &mut impl FnMut(&'a mut Expr)) {
    match arith {
        ArithExpr::Expansion(inner) => f(inner),
        ArithExpr::BinOp { left, right, .. } => {
            walk_arith_expansion_roots_mut(left, f);
            walk_arith_expansion_roots_mut(right, f);
        }
        ArithExpr::UnaryOp { operand, .. } | ArithExpr::Paren(operand) => {
            walk_arith_expansion_roots_mut(operand, f)
        }
        ArithExpr::Assign { value, .. } | ArithExpr::CompoundAssign { value, .. } => {
            walk_arith_expansion_roots_mut(value, f)
        }
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => {
            walk_arith_expansion_roots_mut(cond, f);
            walk_arith_expansion_roots_mut(true_val, f);
            walk_arith_expansion_roots_mut(false_val, f);
        }
        ArithExpr::Comma(items) | ArithExpr::FuncCall { args: items, .. } => items
            .iter_mut()
            .for_each(|it| walk_arith_expansion_roots_mut(it, f)),
        ArithExpr::Subscript { index, .. } => walk_arith_expansion_roots_mut(index, f),
        ArithExpr::SubscriptAssign { index, value, .. } => {
            walk_arith_expansion_roots_mut(index, f);
            walk_arith_expansion_roots_mut(value, f);
        }
        ArithExpr::Literal(_)
        | ArithExpr::Var(_)
        | ArithExpr::DollarVar(_)
        | ArithExpr::PostIncDec { .. }
        | ArithExpr::PreIncDec { .. } => {}
    }
}

/// Visit every operand expression of a test tree, without descending into the
/// operands themselves. The single shared shape of the `TestExpr` recursion:
/// callers supply what an operand *means* (a read position, a name to resolve,
/// a subtree to walk). It was hand-written four separate times before this.
pub(crate) fn walk_test_operands<'a>(t: &'a TestExpr, f: &mut impl FnMut(&'a Expr)) {
    match t {
        TestExpr::Unary { operand, .. } => f(operand),
        TestExpr::Binary { left, right, .. } => {
            f(left);
            f(right);
        }
        TestExpr::Not(inner) => walk_test_operands(inner, f),
        TestExpr::And(l, r) | TestExpr::Or(l, r) => {
            walk_test_operands(l, f);
            walk_test_operands(r, f);
        }
    }
}

/// Mutable mirror of [`walk_test_operands`].
fn walk_test_operands_mut<'a>(t: &'a mut TestExpr, f: &mut impl FnMut(&'a mut Expr)) {
    match t {
        TestExpr::Unary { operand, .. } => f(operand),
        TestExpr::Binary { left, right, .. } => {
            f(left);
            f(right);
        }
        TestExpr::Not(inner) => walk_test_operands_mut(inner, f),
        TestExpr::And(l, r) | TestExpr::Or(l, r) => {
            walk_test_operands_mut(l, f);
            walk_test_operands_mut(r, f);
        }
    }
}

/// Walk expressions inside a TestExpr.
fn walk_test_exprs<'a>(t: &'a TestExpr, f: &mut impl FnMut(&'a Expr)) {
    walk_test_operands(t, &mut |e| walk_expr_tree(e, f));
}

/// A complete script — a sequence of statements.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub statements: Vec<Statement>,
    /// DD#56: dedicated END block (parsed as first-class syntax).
    pub end_block: Option<Vec<Statement>>,
}

impl Program {
    /// Every function definition, for passes after structural validation.
    /// SB5071 admits `fn` only as a direct statement of the top level or of
    /// the DD#56 `END { }` body, so these two lists hold all of them. Private
    /// scans of `statements` alone left END's functions unknown twice: the
    /// transform exec'd them as commands and binding resolution never renamed
    /// their collection arguments.
    pub(crate) fn function_defs(&self) -> impl Iterator<Item = &Function> {
        std::iter::once(self.statements.as_slice())
            .chain(self.end_block.as_deref())
            .flatten()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::Function(f) => Some(f),
                _ => None,
            })
    }
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
    pub assignments: Vec<DeclAssignment>,
}

/// Every initializer a declaration owns. A bare declaration contributes none.
fn decl_exprs<'a>(d: &'a Declaration, f: &mut impl FnMut(&'a Expr)) {
    for a in &d.assignments {
        if let Some(v) = &a.value {
            f(v);
        }
    }
}

/// One `name` or `name=value` within a declaration list.
///
/// Deliberately *not* an [`Assignment`]. A declaration creates a binding, so
/// three of `Assignment`'s shapes are meaningless here and are made
/// unrepresentable rather than merely unused:
///
/// * **no initializer** is `value: None`, not an in-band `Literal("")`. The
///   sentinel made `array a` (bare, works) and `array a=""` (a scalar into a
///   collection, segfaulted) indistinguishable to every reader that did not
///   also know the trick.
/// * **`+=`** is rejected by the parser (SB4065): a declaration creates, it
///   cannot append.
/// * **an index** (`array a[0]=x`) never parsed in the first place.
#[derive(Debug, Clone, PartialEq)]
pub struct DeclAssignment {
    pub name: String,
    /// `None` for a bare declaration (`int x`), which has no initializer at
    /// all — as distinct from `Some(Literal(""))`, an explicit empty string.
    pub value: Option<Expr>,
}

impl DeclAssignment {
    /// View a declaration's initializer as the equivalent plain `Set`
    /// assignment, for the passes that treat `int x=1` and `x=1` identically
    /// (taint, slot rebinding). `None` for a bare declaration, which assigns
    /// nothing and so has nothing for those passes to look at.
    ///
    /// One adapter rather than a parallel code path per pass: the equivalence
    /// is stated once here instead of being re-derived at each call site,
    /// where the two could drift apart silently.
    pub fn as_assignment(&self) -> Option<Assignment> {
        self.value.as_ref().map(|v| Assignment {
            name: self.name.clone(),
            index: None,
            op: AssignOp::Set,
            value: v.clone(),
        })
    }
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

/// An operand position in an arithmetic tree that is **evaluated as an
/// integer** — the positions DD#77 D1 governs.
pub(crate) enum ArithOperand<'a> {
    /// A bare name read: `x` / `$x`.
    Bare(&'a str),
    /// A `${…}` value-import: `${a[i]}`, `${m[k]}`, `${s:-5}`.
    Imported(&'a Expr),
}

/// What [`walk_arith_int_operands`] does at a single node: either this node
/// **is** an int operand, or these are the sub-expressions that hold its int
/// operands (empty for the nodes that have none).
enum ArithStep<'a> {
    Leaf(ArithOperand<'a>),
    Children(Vec<&'a ArithExpr>),
}

/// Classify one `ArithExpr` node for [`walk_arith_int_operands`].
///
/// Every variant is listed here exactly once, which is the point: the question
/// "is this position evaluated as an integer?" has one answer site, and adding
/// a variant forces an answer rather than inheriting a default.
///
/// **Total over `ArithExpr` on purpose — do not add a wildcard.**
fn arith_int_operand_step(a: &ArithExpr) -> ArithStep<'_> {
    match a {
        ArithExpr::Var(name) | ArithExpr::DollarVar(name) => {
            ArithStep::Leaf(ArithOperand::Bare(name))
        }
        ArithExpr::Expansion(inner) => ArithStep::Leaf(ArithOperand::Imported(inner)),
        ArithExpr::BinOp { left, right, .. } => ArithStep::Children(vec![left, right]),
        ArithExpr::UnaryOp { operand, .. } | ArithExpr::Paren(operand) => {
            ArithStep::Children(vec![operand])
        }
        ArithExpr::Assign { value, .. } | ArithExpr::CompoundAssign { value, .. } => {
            ArithStep::Children(vec![value])
        }
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => ArithStep::Children(vec![cond, true_val, false_val]),
        ArithExpr::Comma(exprs) => ArithStep::Children(exprs.iter().collect()),
        // A bare subscript's *index* is itself an arithmetic operand, so a bare
        // string used there (`a[s]`) is the same string→int crossing (DD#77 D1).
        // The array *name* is handled by the bare-subscript cell check, not here.
        ArithExpr::Subscript { index, .. } => ArithStep::Children(vec![index]),
        ArithExpr::SubscriptAssign { index, value, .. } => ArithStep::Children(vec![index, value]),
        // NOT descended: a sexpr argument is not an int operand. `(strlen
        // ${a[1]})` and `${#s}` = `len(s)` pass a *string*, and the intrinsic's
        // own signature is what types it — flagging either would be a false
        // positive. Verified by specs `sx.26`/`sx.28`/`sx.35`, which are exactly
        // the shape that regressed when the imported view first descended here.
        //
        // Nothing to report either for a literal (no name) or an inc/dec target
        // (a mutation lvalue, tracked as an arith-native int).
        ArithExpr::FuncCall { .. }
        | ArithExpr::Literal(_)
        | ArithExpr::PostIncDec { .. }
        | ArithExpr::PreIncDec { .. } => ArithStep::Children(Vec::new()),
    }
}

/// The one traversal of the int-operand positions of an `ArithExpr`.
///
/// [`walk_arith_var_reads`] and [`walk_arith_imported_operands`] are two views
/// of this, in the same "two entry points into one traversal, rather than a
/// second copy of the structure that can drift" shape as
/// [`walk_arith_expansions`] / [`walk_arith_expansion_roots`]. The two views
/// must agree about *which positions are int operands* — they are halves of
/// one rule (a bare string and an imported string cell are the same crossing),
/// so a position reached by one and not the other is a hole by construction.
/// That is not hypothetical: `${s[0]}` evaded DD#77 D1 for six weeks because
/// the imported half had no walker at all.
///
/// The per-variant decisions live in [`arith_int_operand_step`]; this is only
/// the driver.
fn walk_arith_int_operands<'a>(a: &'a ArithExpr, f: &mut impl FnMut(ArithOperand<'a>)) {
    match arith_int_operand_step(a) {
        ArithStep::Leaf(op) => f(op),
        ArithStep::Children(kids) => {
            for kid in kids {
                walk_arith_int_operands(kid, f);
            }
        }
    }
}

/// DD#77 D1: invoke `f(name)` for each bare variable *read* (`x` / `$x`) in an
/// `ArithExpr` tree. Does NOT descend into `Expansion` (a `${…}` disposition or
/// subscript is not a bare operand) and does NOT visit assignment/inc-dec
/// targets (those are mutation lvalues, tracked as arith-native ints).
pub fn walk_arith_var_reads(a: &ArithExpr, f: &mut impl FnMut(&str)) {
    walk_arith_int_operands(a, &mut |op| {
        if let ArithOperand::Bare(name) = op {
            f(name);
        }
    });
}

/// The companion view of [`walk_arith_var_reads`]: invoke `f` on each `${…}`
/// **value-import sitting in an int-operand position**, at its root.
///
/// Distinct from [`walk_arith_expansion_roots`], which visits every embedded
/// `${…}` anywhere in the tree *including* sexpr arguments. The difference is
/// the whole point: `(strlen ${a[1]})` contains an expansion but not an
/// integer operand.
pub(crate) fn walk_arith_imported_operands<'a>(a: &'a ArithExpr, f: &mut impl FnMut(&'a Expr)) {
    walk_arith_int_operands(a, &mut |op| {
        if let ArithOperand::Imported(e) = op {
            f(e);
        }
    });
}

/// Invoke `f(name)` for every bare subscript (`ArithExpr::Subscript` /
/// `SubscriptAssign`) *subject* name in an arith tree, descending into every
/// sub-expr. The complement of [`walk_arith_var_reads`], which descends into a
/// subscript's index but deliberately skips its subject.
/// Visit the **bare** subscript cells (`a[i]`) in an arithmetic tree, handing
/// `f` each collection name.
///
/// Deliberately does **not** descend into [`ArithExpr::Expansion`]: a `${a[i]}`
/// is the braced value-import, and its callers (the model-B bare-map reject,
/// the bare string-cell reject) are about the *bare* spelling specifically. The
/// imported spelling is checked by `type_check::check_arith_expansion_string_cell`.
///
/// **Total over `ArithExpr` on purpose — do not add a wildcard.** The `_ => {}`
/// this replaces is what let `${s[0]}` evade DD#77 D1 for six weeks: the arm
/// that swallowed `Expansion` read exactly like the arms that swallow
/// `Literal`, so nothing distinguished "carries no cell" from "carries a cell
/// we are not looking at". Spelling them out makes that distinction a comment
/// rather than a silence.
pub fn walk_arith_subscripts(a: &ArithExpr, f: &mut impl FnMut(&str)) {
    match a {
        ArithExpr::Subscript { name, index } => {
            f(name);
            walk_arith_subscripts(index, f);
        }
        ArithExpr::SubscriptAssign {
            name, index, value, ..
        } => {
            f(name);
            walk_arith_subscripts(index, f);
            walk_arith_subscripts(value, f);
        }
        ArithExpr::BinOp { left, right, .. } => {
            walk_arith_subscripts(left, f);
            walk_arith_subscripts(right, f);
        }
        ArithExpr::UnaryOp { operand, .. } | ArithExpr::Paren(operand) => {
            walk_arith_subscripts(operand, f)
        }
        ArithExpr::Assign { value, .. } | ArithExpr::CompoundAssign { value, .. } => {
            walk_arith_subscripts(value, f)
        }
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => {
            walk_arith_subscripts(cond, f);
            walk_arith_subscripts(true_val, f);
            walk_arith_subscripts(false_val, f);
        }
        ArithExpr::Comma(exprs) | ArithExpr::FuncCall { args: exprs, .. } => {
            for e in exprs {
                walk_arith_subscripts(e, f);
            }
        }
        // Carries a cell, skipped on purpose — see the doc comment. `${a[i]}`
        // is the value-import, not a bare cell.
        ArithExpr::Expansion(_) => {}
        // Carry a `String` or an `i64`, never a subscript.
        ArithExpr::Literal(_)
        | ArithExpr::Var(_)
        | ArithExpr::DollarVar(_)
        | ArithExpr::PostIncDec { .. }
        | ArithExpr::PreIncDec { .. } => {}
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
/// The variables an arithmetic expression **writes**: `x=…`, `x op= …`, `x++`,
/// `++x`. The companion to [`walk_arith_var_reads`], which collects reads.
///
/// Subscript writes (`a[i]=…`) are excluded on purpose — they write an element
/// of a collection that must already exist, so they introduce no new name.
pub fn walk_arith_var_writes(e: &ArithExpr, f: &mut impl FnMut(&str)) {
    match e {
        ArithExpr::Assign { var, value } | ArithExpr::CompoundAssign { var, value, .. } => {
            f(var);
            walk_arith_var_writes(value, f);
        }
        ArithExpr::PostIncDec { var, .. } | ArithExpr::PreIncDec { var, .. } => f(var),
        ArithExpr::BinOp { left, right, .. } => {
            walk_arith_var_writes(left, f);
            walk_arith_var_writes(right, f);
        }
        ArithExpr::UnaryOp { operand, .. } | ArithExpr::Paren(operand) => {
            walk_arith_var_writes(operand, f)
        }
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => {
            walk_arith_var_writes(cond, f);
            walk_arith_var_writes(true_val, f);
            walk_arith_var_writes(false_val, f);
        }
        ArithExpr::Comma(parts) | ArithExpr::FuncCall { args: parts, .. } => {
            for p in parts {
                walk_arith_var_writes(p, f);
            }
        }
        ArithExpr::SubscriptAssign { index, value, .. } => {
            walk_arith_var_writes(index, f);
            walk_arith_var_writes(value, f);
        }
        ArithExpr::Subscript { index, .. } => walk_arith_var_writes(index, f),
        ArithExpr::Literal(_)
        | ArithExpr::Var(_)
        | ArithExpr::DollarVar(_)
        | ArithExpr::Expansion(_) => {}
    }
}

pub fn arith_references_var(a: &ArithExpr, name: &str) -> bool {
    let mut found = false;
    walk_arith_var_reads(a, &mut |n| {
        if n == name {
            found = true;
        }
    });
    found
}

/// The target name of a `readline` command — the SINGLE source of truth,
/// shared by the transform (SSA lowering), validation (target collection +
/// taint) and type checking.
///
/// DD#99 retired the whole flag surface (`-a`, `-d`, `-f`, `-r`) and the
/// multi-target form; SB5057 rejects every one of them at validation, so by
/// the time a program reaches lowering a `readline` is exactly ONE bare
/// target name. The return type says so.
///
/// It used to be a `ReadTarget { var_names: Vec<String> }` — the shape of the
/// retired multi-target form — fed by a private `read_arg_value` that was a
/// copy of [`literal_word_text`] with `None` collapsed to `""`. That cost
/// three things: the quote-unwrapping arms were unreachable (SB5057 admits
/// only `ExprKind::Literal`, because it gates on `expr_literal_str`, which is
/// non-empty for no other variant); a non-constant word would have been
/// *invented* as the empty name, the exact failure [`literal_word_text`]'s
/// doc comment exists to forbid; and because the vec could hold any count,
/// both call sites carried a `let [name] = … else { return }` that silently
/// dropped a `readline` no input can produce.
///
/// # Panics
/// If SB5057 did not run, or stopped rejecting a shape it rejects today.
/// That is the intent: the alternative is emitting no `ReadLine` at all, so
/// the program would read nothing and say nothing about why.
pub fn readline_target(args: &[Expr]) -> String {
    let [target] = args else {
        panic!("SB5057 validated `readline` takes exactly one target");
    };
    literal_word_text(target).expect("SB5057 validated the target is a bare name")
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
    /// The position of the `elif` keyword. An error about the clause reports
    /// it, and the `If` each clause becomes when the chain is nested carries it.
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct While {
    pub condition: Vec<Statement>,
    pub body: Vec<Statement>,
    pub until: bool, // true for `until` loops
}

impl While {
    /// The keyword the user wrote, for a diagnostic that names the loop.
    pub fn keyword(&self) -> &'static str {
        if self.until {
            "until"
        } else {
            "while"
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct For {
    pub variable: String,
    pub words: Vec<Expr>,
    pub body: Vec<Statement>,
}

/// The **DD#66 dispatch form** `[[ value { pat) … } ]]` — *not* bash
/// `case`/`esac`, which is not in the subset and has no parser production.
/// The name is a holdover from when it was lowered from bash `case`; the sole
/// producer today is `parse::parser::parse_dispatch`, and the parser's
/// `dispatch_in` treats every `StmtKind::Case` as a dispatch. Do not read a
/// surviving `case` statement into it.
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

impl Redirect {
    /// The fd this redirect actually affects: the explicit one when written
    /// (`2> f`), else bash's default for the operator — stdout for the
    /// write-ish ops, stdin for the read-ish ones (`<>` is a `<` form, so it
    /// defaults to stdin, not stdout).
    ///
    /// Shared by the transform and both backends.
    pub fn resolved_fd(&self) -> i64 {
        i64::from(self.fd.unwrap_or(self.op.default_fd()))
    }
}

/// DD#88: does this redirect list leave the capture pipe with a writer?
///
/// Inside `$( … )` the command starts with fd 1 attached to the capture pipe.
/// A redirect list can move that pipe around, and the capture-condition form
/// (`if x=$(cmd …)`) is only meaningful while *some* fd still holds it — DG's
/// rule, 2026-10-12. Simulated left-to-right, exactly as the kernel applies
/// them:
///
/// - `cmd 2>/dev/null` — fd 1 untouched. The capture reads stdout. **Legal.**
/// - `cmd 2>&1 >f` — fd 2 takes the pipe, then fd 1 goes to the file. The
///   capture reads *stderr*, which is the whole point of the idiom. **Legal.**
/// - `cmd >f` — the pipe is abandoned, so `x` is unconditionally empty.
///   **Rejected**, as is the other order `cmd >f 2>&1` (fd 2 dups the file,
///   not the pipe) and `&>f`, which the parser desugars to a plain `>f`.
///
/// Totally decidable: a dup target is a literal fd number or the parser
/// already rejected it (SB4048), so there is no dynamic case to be
/// conservative about. An invalid target is a broken compiler invariant,
/// not evidence that a descriptor no longer holds the pipe.
pub fn redirects_preserve_capture<'a>(redirects: impl IntoIterator<Item = &'a Redirect>) -> bool {
    // The fds still pointing at the capture pipe. Starts as just stdout.
    let mut holding: Vec<i64> = vec![1];
    for r in redirects {
        let fd = r.resolved_fd();
        let now_holds = redirect_inherits_capture(r, &holding);
        holding.retain(|&held| held != fd);
        if now_holds {
            holding.push(fd);
        }
    }
    !holding.is_empty()
}

/// **Total over `RedirectOp` on purpose — do not add a wildcard.** A new
/// operator must say whether its destination inherits the capture pipe.
fn redirect_inherits_capture(r: &Redirect, holding: &[i64]) -> bool {
    match r.op {
        RedirectOp::Write | RedirectOp::Append | RedirectOp::Read | RedirectOp::ReadWrite => false,
        RedirectOp::DupOutput | RedirectOp::DupInput => {
            holding.contains(&validated_fd_number(&r.target))
        }
    }
}

/// The literal fd named by `>&`/`<&` (SB4048) or `close` (SB4052): decimal
/// digits in the nonnegative i32 range. A broken invariant must not silently
/// remove a capture writer or omit a descriptor from the private-fd exclusion set.
pub(crate) fn validated_fd_number(target: &Expr) -> i64 {
    let parsed = match &target.kind {
        ExprKind::Literal(s) if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) => {
            s.parse::<i32>().ok()
        }
        _ => None,
    };
    i64::from(parsed.expect("SB4048/SB4052 validated a literal descriptor in 0..2147483647"))
}

/// An unquoted word an int sink takes as a literal: decimal, optionally
/// signed, within i64 (`exit 1`, `exit -1`). The int-sink check in
/// `type_check` and `parse_return`'s value/producer split share it, so a
/// `return` word is a status value exactly when a sink would accept it as one.
pub(crate) fn is_int_sink_literal(text: &str) -> bool {
    text.parse::<i64>().is_ok()
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

impl RedirectOp {
    pub(crate) fn default_fd(self) -> u32 {
        match self {
            Self::Write | Self::Append | Self::DupOutput => 1,
            Self::Read | Self::DupInput | Self::ReadWrite => 0,
        }
    }

    /// Runtime redirect modes, not platform open(2) flags.
    pub(crate) fn open_flags(self) -> Option<i64> {
        match self {
            Self::Write => Some(1),
            Self::Append => Some(2),
            Self::Read => Some(0),
            Self::ReadWrite => Some(3),
            Self::DupOutput | Self::DupInput => None,
        }
    }
}

/// Private backups must not occupy a named target or a closed dup source.
pub(crate) fn redirect_referenced_fds(redirects: &[Redirect]) -> Vec<i64> {
    let mut fds = Vec::with_capacity(redirects.len() * 2);
    for redir in redirects {
        fds.push(i64::from(redir.fd.unwrap_or(redir.op.default_fd())));
        if matches!(redir.op, RedirectOp::DupOutput | RedirectOp::DupInput) {
            fds.push(validated_fd_number(&redir.target));
        }
    }
    fds.sort_unstable();
    fds.dedup();
    fds
}

/// Keep runtime bookkeeping clear of source fds in every scope, including
/// captures, functions and END. Ordinary numeric operands are not fd names.
pub(crate) fn program_referenced_fds(program: &Program) -> Vec<i64> {
    let mut fds = std::collections::BTreeSet::new();
    for body in std::iter::once(program.statements.as_slice()).chain(program.end_block.as_deref()) {
        walk_stmt_tree(body, &mut |stmt| collect_stmt_fds(stmt, &mut fds));
    }
    fds.into_iter().collect()
}

/// Total over statement metadata; do not add a wildcard.
fn collect_stmt_fds(stmt: &Statement, fds: &mut std::collections::BTreeSet<i64>) {
    match &stmt.kind {
        StmtKind::Redirected(r) => fds.extend(redirect_referenced_fds(&r.redirects)),
        StmtKind::Command(c) => {
            if matches!(&c.name.kind, ExprKind::Literal(name) if name == "close") {
                let fd = c
                    .args
                    .first()
                    .map(validated_fd_number)
                    .expect("SB4052 requires a literal descriptor number");
                fds.insert(fd);
            }
        }
        StmtKind::Assignment(_)
        | StmtKind::Declaration(_)
        | StmtKind::Pipeline(_)
        | StmtKind::If(_)
        | StmtKind::While(_)
        | StmtKind::For(_)
        | StmtKind::Case(_)
        | StmtKind::Function(_)
        | StmtKind::List(_)
        | StmtKind::Group(_)
        | StmtKind::Suppressed(_)
        | StmtKind::Negated(_)
        | StmtKind::ArithTest(_)
        | StmtKind::ReturnProducer(_) => {}
    }
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
    /// The spelling the group stands for. The parser builds two of the three
    /// itself, so a check that judges what the user wrote must ask this
    /// rather than read every group as a `{ }`.
    pub origin: GroupOrigin,
}

/// Which source spelling a [`Group`] stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupOrigin {
    /// `{ … }`, written as such.
    Braced,
    /// One `(( a, b ))` whose comma carries a mutation, split so each part is
    /// its own statement (DD#31). It is still the single arithmetic command
    /// the user wrote.
    ArithComma,
    /// A C-style `for (( init; cond; step ))`, lowered to its init followed by
    /// the loop. It is still the one loop the user wrote.
    CStyleFor,
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

    /// `$(<file)` — the slurp form. A first-class node, NOT a desugar.
    ///
    /// It used to be rewritten at parse time into `$(cat file)`, which forked
    /// a process and embedded `/usr/bin/cat` in a supposedly self-contained
    /// static binary. The entire point of the spelling in bash is that it does
    /// not fork, so the syntax was promising an optimization the
    /// implementation inverted.
    SlurpFile(Box<Expr>),

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

/// Why every downstream `PreIncDec` arm is an ICE. The node exists only
/// between `parse_pre_incdec`, which builds it so the diagnostic can name the
/// variable, and `reject_prefix_incdec`, which raises SB4020 at both arith
/// entry points (`parse_arith_expansion`, `parse_arith_slot` — the latter
/// covering all three C-style `for` slots) and at both `parse_arith_str`
/// bypass callers. No pass after the parser can receive one.
///
/// That last sentence was **false for ten shapes** until 2026-09-19.
/// `reject_prefix_incdec` walked the tree by hand and ended in `_ => Ok(())`,
/// so it never entered a ternary branch, a unary operand, or the value of a
/// subscript assignment — `(( 1 ? ++x : 0 ))`, `(( y = -(++x) ))` and
/// `(( a[0] = ++i ))` all reached a downstream arm and crashed the compiler.
/// It now delegates to `ArithExpr::children()`, which is total, so a new
/// variant cannot silently reopen the hole. Specs `31.107`-`31.116`.
///
/// An invariant asserted in a doc comment and enforced by a partial walk is
/// not an invariant. If you add a `PreIncDec` arm anywhere, keep it an ICE —
/// but know that what makes it unreachable is one function, and check it.
///
/// It gets its own arm rather than riding along with `PostIncDec` because
/// `x++` is ALLOWED and `++x` is REJECTED: merging them makes a regression in
/// the parse-time rejection silently *succeed*, lowering `++x` into
/// `x = x + 1` and emitting working code for a form the language forbids.
pub(crate) const PREFIX_INCDEC_INVARIANT: &str =
    "prefix ++/-- never survives the parser: reject_prefix_incdec raises \
     SB4020 at both arith entry points";

/// Why `transform::statements::hoist_mutation_node` can assume a `(( … ))`
/// statement or condition holds only plain and compound assignments.
///
/// `validate::check_arith_test_placement` (SB4020) runs over every `ArithTest`
/// statement and rejects the whole tree if `arith_has_incdec`,
/// `arith_has_subscript_assign`, or `arith_mutation_under_cond` holds. Those
/// are whole-tree predicates, so by the time the transform hoists an embedded
/// store there can be no `++`/`--` and no `a[i]` mutation anywhere in it.
///
/// Handling them anyway would be worse than ICEing: the hoist would emit a
/// store for a form the compiler has already told the user it cannot apply,
/// which is how a rejected construct turns into silently working code.
/// Why `emit_ir` never serialises an arithmetic `++`/`--` or `a[i]` mutation.
///
/// `emit_ir` renders the post-validate AST, so three things have already
/// happened to every such node:
///
///   - a top-level `(( a++ ))` / `(( a[i] += 5 ))` was rewritten into a
///     `Statement::Assignment` by `parse::lower_arith_command`, and a C-style
///     `for` header was desugared the same way, so no `ArithExpr` survives;
///   - inside a `$(( … ))` **value**, `validate::check_arith_value_mutation`
///     (SB4020) rejects any mutation, whole-tree;
///   - inside a `(( … ))` statement or condition,
///     `validate::check_arith_test_placement` (SB4020) rejects `++`/`--` and
///     `a[i]` mutations specifically, whole-tree. Plain and compound assigns
///     are the exception it allows, which is why `Assign` / `CompoundAssign`
///     do reach the serialiser and these two do not.
///
/// The one route that bypassed all three was DD#93's attest carve-out, closed
/// in journal 344 by completing `parse::reject_nested_mutation`.
pub(crate) const ARITH_IR_MUTATION_INVARIANT: &str =
    "`++`/`--` and `a[i]` mutations are lowered away at parse time or rejected \
     by validate (SB4020) before emit_ir renders the AST";

pub(crate) const ARITH_TEST_HOISTABLE_INVARIANT: &str =
    "`++`/`--` and `a[i]` mutations are rejected in a `(( … ))` statement or \
     condition by validate::check_arith_test_placement (SB4020) before the \
     transform runs, so only plain and compound assignments can be hoisted";

impl ArithExpr {
    /// The immediate sub-expressions of this node, in source order.
    ///
    /// Naming the children in one place rather than re-deriving the shape in
    /// every walker keeps each walker a single loop: a traversal only has to
    /// spell out the variants it treats *specially* and can delegate the rest.
    /// `Expansion` holds an `Expr`, not an `ArithExpr`, so it has no children
    /// here — a walker that needs to descend into it must say so itself.
    ///
    /// **Total over `ArithExpr` on purpose — do not add a wildcard.**
    /// `reject_prefix_incdec` and [`walk_arith_nodes`] both
    /// delegate their own totality claim to this function, so the no-wildcard
    /// rule has to be enforced *here* or it is enforced nowhere.
    pub(crate) fn children(&self) -> Vec<&ArithExpr> {
        match self {
            ArithExpr::PreIncDec { .. } => unreachable!("{}", PREFIX_INCDEC_INVARIANT),
            ArithExpr::BinOp { left, right, .. } => vec![&**left, &**right],
            ArithExpr::SubscriptAssign { index, value, .. } => vec![&**index, &**value],
            ArithExpr::Ternary {
                cond,
                true_val,
                false_val,
            } => vec![&**cond, &**true_val, &**false_val],
            ArithExpr::UnaryOp { operand: e, .. }
            | ArithExpr::Paren(e)
            | ArithExpr::Subscript { index: e, .. }
            | ArithExpr::Assign { value: e, .. }
            | ArithExpr::CompoundAssign { value: e, .. } => vec![&**e],
            ArithExpr::Comma(es) | ArithExpr::FuncCall { args: es, .. } => es.iter().collect(),
            ArithExpr::Literal(_)
            | ArithExpr::Var(_)
            | ArithExpr::DollarVar(_)
            | ArithExpr::Expansion(_)
            | ArithExpr::PostIncDec { .. } => Vec::new(),
        }
    }
}

/// True if `a` contains an arithmetic mutation (`x = …`, `x += …`, `x++`/`x--`,
/// `a[i] = …`) anywhere in its tree. Shared by `validate` (to reject a mutation
/// that can't take effect where it sits — SB4020) and by the transform (to
/// decide whether a `(( … ))` needs its embedded stores hoisted). A mutation
/// node short-circuits to `true` without recursing into its rvalue.
pub(crate) fn arith_has_mutation(a: &ArithExpr) -> bool {
    mutation_target_in(a).is_some()
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

/// The **source spelling** of an arithmetic operator — `Add` → `"+"`.
///
/// Total over `IrArithOp` on purpose. The renderers that quote an expression
/// back to the user used to cover a handful of operators and fall to `"?"`
/// for the rest, so `(( y + (x *= 2) ))` was reported as `y ? ...` — an
/// operator the author never wrote. A brief is read as a quotation; showing
/// a different operator than the source is worse than showing less.
///
/// `emit_ir::arith_op` is **not** a rival (DD#116): it renders Janet keywords
/// (`:add`) for the DD#104 IR seam, which is a different output language.
///
/// **Total over `IrArithOp` on purpose — do not add a wildcard.**
pub fn arith_op_str(op: IrArithOp) -> &'static str {
    match op {
        IrArithOp::Add => "+",
        IrArithOp::Sub => "-",
        IrArithOp::Mul => "*",
        IrArithOp::Div => "/",
        IrArithOp::Mod => "%",
        IrArithOp::Pow => "**",
        IrArithOp::Eq => "==",
        IrArithOp::Ne => "!=",
        IrArithOp::Lt => "<",
        IrArithOp::Le => "<=",
        IrArithOp::Gt => ">",
        IrArithOp::Ge => ">=",
        IrArithOp::And => "&&",
        IrArithOp::Or => "||",
        IrArithOp::Shl => "<<",
        IrArithOp::Shr => ">>",
        IrArithOp::BitAnd => "&",
        IrArithOp::BitOr => "|",
        IrArithOp::BitXor => "^",
    }
}

/// How an assignment is quoted in a diagnostic: `x = ...`.
///
/// The RHS is deliberately elided — the brief names the *shape*, not the
/// value. This is the **only** place that string is built. It was previously
/// written out at four independent sites, which is why `if (( x = 5 ))` and
/// the arm that looked like it produced that message were indistinguishable
/// from the output alone; telling them apart took instrumentation.
pub fn assign_brief(var: &str) -> String {
    format!("{var} = ...")
}

/// How a compound assignment is quoted in a diagnostic: `x *= ...`.
///
/// Derived from [`arith_op_str`], so a new operator cannot be spelled `?=`
/// here by omission. (Comparison variants cannot reach this — `x === ...`
/// is not a thing a parser can build — but the mapping is total anyway.)
pub fn compound_assign_brief(var: &str, op: IrArithOp) -> String {
    format!("{var} {}= ...", arith_op_str(op))
}

/// How a post-increment is quoted in a diagnostic: `x++`.
pub fn incdec_brief(var: &str, inc: bool) -> String {
    format!("{var}{}", if inc { "++" } else { "--" })
}

/// The *target* of a mutation, named without claiming how it was spelled.
///
/// For the sites that only ever see the **decomposed** form. Every mutation
/// spelling — `x = e`, `x op= e`, `x++`, `a[i] op= e` — reaches
/// `validate_assignment_stmt` as `Assignment { op: Set, value: BinOp }`,
/// because `lower_arith_mutation_structured` collapses all four on purpose.
/// The operator is therefore *gone* before the diagnostic is built.
///
/// That site used to call [`assign_brief`] regardless, so SB5007 reported
/// `mutation x = ...` for `if (( x++ ))` — which is not an assignment at all —
/// and `mutation a = ...` for `if (( a[0] -= 2 ))`, naming the whole array
/// rather than the element. A brief is read as a quotation, and showing a
/// different operator than the source is worse than showing less
/// (see [`arith_op_str`]), so this names the target and claims no operator.
///
/// Reconstructing the spelling from the decomposed `BinOp` is deliberately
/// *not* done: `(( x = x - 2 ))` decomposes identically to `(( x -= 2 ))`, so
/// any such guess misquotes one of the two.
///
/// The index is elided as `a[…]` rather than rendered, matching how every
/// other diagnostic names a cell (`bare_string_cell_error`,
/// `scalar_subscript_diag`, `reject_scalar_rhs_read`, SB5006). Since it is
/// never printed, `subscripted` carries only its *presence* — which lets the
/// two callers pass indices of different types (an `Expr` on the decomposed
/// path, an `ArithExpr` on the nested one) without a second renderer.
pub fn mutation_target_brief(name: &str, subscripted: bool) -> String {
    if subscripted {
        format!("{name}[…]")
    } else {
        name.to_string()
    }
}

/// The target of the first mutation inside `a`, or `None` if it holds none.
///
/// **The *whether* and the *which* are one traversal.** DG's rule is that a
/// mutation diagnostic names its target (`mutation of x`) rather than quoting
/// the surrounding expression, so every such site needs both answers; deriving
/// them separately is how a predicate and its renderer drift apart, which is
/// the defect this whole family already had three times over. `arith_has_mutation`
/// is therefore defined as `is_some()` on this, not as a parallel match.
///
/// Order is "first found, depth-first". An expression with two mutations is
/// rejected on either name — the diagnostic says *that* a mutation is here,
/// and one target is enough to point at it.
pub(crate) fn mutation_target_in(a: &ArithExpr) -> Option<String> {
    match a {
        ArithExpr::PreIncDec { .. } => unreachable!("{}", PREFIX_INCDEC_INVARIANT),
        ArithExpr::Assign { var, .. }
        | ArithExpr::CompoundAssign { var, .. }
        | ArithExpr::PostIncDec { var, .. } => Some(mutation_target_brief(var, false)),
        ArithExpr::SubscriptAssign { name, .. } => Some(mutation_target_brief(name, true)),
        _ => a.children().into_iter().find_map(mutation_target_in),
    }
}

/// Quote an arithmetic expression back to the user inside a diagnostic.
///
/// **The only renderer.** There were three — `validate::arith_brief`,
/// `parse::arith::brief_arith`, and the dispatch half of
/// `expansion_mutation_brief` — all built on the same four primitives above
/// and all dispatching separately, so each knew a different subset of
/// `ArithExpr`. The same defect was then found and fixed *twice*,
/// independently, in two of them: `expansion_mutation_brief` once printed
/// `x = ...` for a compound assign (its own doc records the fix), and this
/// function once rendered fourteen of nineteen operators as `?` (spec 31.63).
/// Neither fix reached `brief_arith`, which still had no `CompoundAssign` arm
/// years later. A shared vocabulary with duplicated dispatch gets repaired one
/// copy per bug report, forever.
///
/// **This does not report mutations.** Every site that rejects a *mutation*
/// names its target instead ([`mutation_target_in`], DG 2026-09-23), because a
/// quotation invites the reader to compare it with their source and four of
/// the five sites could not produce one that matched. What is left here are
/// the two sites that quote a **non**-mutation: a bare-statement comparison,
/// and a non-final comma part that was required to be a mutation and is not.
/// For those the expression *is* the complaint, so quoting it is the message.
///
/// **Total over `ArithExpr` on purpose — do not add a wildcard.** A new variant must decide how it is
/// quoted instead of inheriting `...` from a `_` arm — that is how the `?`
/// regression above happened. The variants that do render `...` are listed
/// explicitly: eliding is honest here, because it says *less* rather than
/// something *else*, and the elided sub-expression is no longer ever the thing
/// being rejected — that was the `Paren` complaint, and naming the target
/// answered it.
///
/// **Which mutation arms are live is not obvious, and has been mis-measured
/// three times.** They are reached only as an operand of the non-mutation
/// being quoted, and only via the **unparenthesised right operand** of a
/// binary operator — a parenthesised one arrives wrapped in `Paren` and elides:
///
/// ```text
/// if (( 1 > x = 2,  x > 0 ))   → Assign
/// if (( 1 > x += 2, x > 0 ))   → CompoundAssign
/// if (( x++ > 0,    x > 0 ))   → PostIncDec
/// ```
///
/// Nobody writes those, because bash binds `=` loosest and rejects them as an
/// assignment to a non-variable; sash's parser groups them as `1 > (x = 2)`
/// instead. Every context still refuses the program (SB5007/SB4020/SB4041), so
/// the precedence difference is not observable — but the arms run, and specs
/// `31.69`-`31.72` keep them honest. The two earlier measurements used a plain
/// `if (( … ))`, which stopped reaching them the moment that site began naming
/// its target: a shape that reaches a cold arm today is not a property of the
/// arm, it is a property of its callers.
pub fn arith_brief(expr: &ArithExpr) -> String {
    match expr {
        // Rendered faithfully.
        ArithExpr::Literal(n) => n.to_string(),
        ArithExpr::Var(name) => name.clone(),
        ArithExpr::Assign { var, .. } => assign_brief(var),
        ArithExpr::CompoundAssign { var, op, .. } => compound_assign_brief(var, *op),
        ArithExpr::PostIncDec { var, inc } => incdec_brief(var, *inc),
        ArithExpr::BinOp { left, op, right } => format!(
            "{} {} {}",
            arith_brief(left),
            arith_op_str(*op),
            arith_brief(right)
        ),

        // `++x` never survives the parser; see `PREFIX_INCDEC_INVARIANT`. The
        // two other renderers already ICEd here and this one alone fell
        // through to `...`; the invariant says every downstream arm is an ICE,
        // so the disagreement resolves that way rather than by softening it.
        ArithExpr::PreIncDec { .. } => unreachable!("{}", PREFIX_INCDEC_INVARIANT),

        // Elided on purpose: a parenthesized non-mutation is quoted as `...`
        // by both statement and non-final comma diagnostics (irbrief.1–2).
        ArithExpr::DollarVar(_)
        | ArithExpr::UnaryOp { .. }
        | ArithExpr::Ternary { .. }
        | ArithExpr::Comma(_)
        | ArithExpr::FuncCall { .. }
        | ArithExpr::Paren(_)
        | ArithExpr::Expansion(_)
        | ArithExpr::Subscript { .. }
        | ArithExpr::SubscriptAssign { .. } => "...".to_string(),
    }
}

/// Unary arithmetic operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrUnaryArithOp {
    Neg,    // -
    Pos,    // +
    Not,    // !
    BitNot, // ~
}

/// The should-never-run assertions in this file guard shapes that an earlier
/// pass makes unreachable: SB5057 for [`readline_target`]'s two, and the
/// parser's `reject_prefix_incdec` for every `ArithExpr::PreIncDec` arm. DG's
/// rule for the coverage campaign: an unreachable should-never-run assertion
/// earns a unit test proving it catches what it claims. These call each
/// accessor directly with the arguments the earlier pass would never let
/// through. A test that stops panicking means that pass has a hole.
#[cfg(test)]
mod invariant_tests {
    use super::{
        arith_brief, mutation_target_in, readline_target, validated_fd_number, ArithExpr, Expr,
        ExprKind, Span,
    };

    fn word(kind: ExprKind) -> Expr {
        Expr {
            kind,
            span: Span::new(1, 1),
        }
    }

    /// `++x`. The parser raises SB4020 at both arith entry points, so no
    /// walker can meet one — which is the claim each test below asserts.
    fn prefix_incdec() -> ArithExpr {
        ArithExpr::PreIncDec {
            var: "x".into(),
            inc: true,
        }
    }

    #[test]
    #[should_panic(expected = "prefix ++/-- never survives the parser")]
    fn children_rejects_prefix_incdec() {
        let _ = prefix_incdec().children();
    }

    #[test]
    #[should_panic(expected = "prefix ++/-- never survives the parser")]
    fn mutation_target_in_rejects_prefix_incdec() {
        let _ = mutation_target_in(&prefix_incdec());
    }

    #[test]
    #[should_panic(expected = "prefix ++/-- never survives the parser")]
    fn arith_brief_rejects_prefix_incdec() {
        let _ = arith_brief(&prefix_incdec());
    }

    #[test]
    #[should_panic(expected = "exactly one target")]
    fn readline_target_rejects_the_retired_multi_target_form() {
        // `readline a b` — the DD#99-retired multi-target form. SB5057's
        // arity check rejects it; reaching past that, the arity is 2.
        let args = [
            word(ExprKind::Literal("a".into())),
            word(ExprKind::Literal("b".into())),
        ];
        let _ = readline_target(&args);
    }

    #[test]
    #[should_panic(expected = "a bare name")]
    fn readline_target_rejects_a_non_constant_target() {
        // `readline $x` — a value, not a binding. SB5057's sigil check
        // rejects it because `expr_literal_str` is empty for every variant
        // but `Literal`. Reaching past that, there is no name to define,
        // and inventing `""` is what this replaced.
        let args = [word(ExprKind::SimpleExpansion("x".into()))];
        let _ = readline_target(&args);
    }

    #[test]
    fn duplicate_descriptors_cannot_have_unvalidated_targets() {
        for kind in [
            ExprKind::SimpleExpansion("fd".into()),
            ExprKind::DoubleQuoted(vec![word(ExprKind::Literal("1".into()))]),
            ExprKind::Literal(String::new()),
            ExprKind::Literal("not-a-descriptor".into()),
            ExprKind::Literal("-1".into()),
            ExprKind::Literal("-0".into()),
            ExprKind::Literal("+1".into()),
            ExprKind::Literal("2147483648".into()),
            ExprKind::Literal("9223372036854775808".into()),
        ] {
            let target = word(kind);
            let payload = std::panic::catch_unwind(|| validated_fd_number(&target))
                .expect_err("an invalid dup target must not become an optional descriptor");
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .expect("invariant text");
            assert!(
                message.contains("SB4048/SB4052 validated a literal descriptor in 0..2147483647"),
                "{message}"
            );
        }
    }
}

/// Totality arms that no *program* reaches, because an earlier pass rejects
/// every spelling that would build them. Under DG's coverage rule these are
/// the unit-test half rather than exempt work: "no spec can warm it" is a
/// statement about the spec suite, not a licence to leave it unexecuted.
///
/// Unlike [`invariant_tests`] above, these arms are not ICEs — they do real
/// work, and the test asserts the work is *right*. That matters more than it
/// sounds: an unexecuted correct-looking arm is the shape that let
/// `SsaInst::Test` escape the SSA taint pass for its whole life.
#[cfg(test)]
mod shadowed_arm_tests {
    use super::*;

    #[test]
    fn malformed_attest_shapes_are_not_recognized() {
        let word = |kind| Expr {
            kind,
            span: Span::new(1, 1),
        };
        let name = || word(ExprKind::Literal("x".into()));
        let predicate = word(ExprKind::Test(TestExpr::Unary {
            op: "-n".into(),
            operand: Box::new(name()),
        }));
        for args in [
            vec![name(), name()],
            vec![
                word(ExprKind::SimpleExpansion("x".into())),
                predicate.clone(),
            ],
            vec![predicate.clone()],
            vec![name(), name(), name(), name(), predicate],
        ] {
            assert!(classify_attest(&args).is_none(), "{args:?}");
        }
    }

    /// `walk_arith_subscripts`' `SubscriptAssign` arm — `a[i] = v` as an
    /// arithmetic assignment target. Cold because both callers
    /// (`type_check::check_arith_bare_map_subscript`,
    /// `validate::push_arith_subjects`) sit behind rules that reject the
    /// spellings which build one: measured 2026-10-11 by replacing the arm
    /// with a panic, where `(( m[k] = 5 ))` raised SB4030 from a different
    /// walker and a full native sweep never fired the probe.
    ///
    /// The arm's whole distinction from the `Subscript` arm is that it
    /// descends into `value` as well as `index`. That third line is the one
    /// a reader would most plausibly drop, and it is what this pins.
    #[test]
    fn subscript_assign_yields_its_subject_and_descends_both_sides() {
        // (( outer[inner[0]] = other[1] ))
        let tree = ArithExpr::SubscriptAssign {
            name: "outer".into(),
            index: Box::new(ArithExpr::Subscript {
                name: "inner".into(),
                index: Box::new(ArithExpr::Literal(0)),
            }),
            op: None,
            value: Box::new(ArithExpr::Subscript {
                name: "other".into(),
                index: Box::new(ArithExpr::Literal(1)),
            }),
        };
        let mut seen = Vec::new();
        walk_arith_subscripts(&tree, &mut |n| seen.push(n.to_string()));
        assert_eq!(
            seen,
            vec!["outer", "inner", "other"],
            "the subject, then the index subtree, then the value subtree — \
             a missing `other` means the walk into `value` was dropped"
        );
    }
}

/// [`Statement::cmdsub_bodies_mut`] must reach exactly the bodies
/// [`Statement::cmdsub_bodies`] reaches, in the same order — the same
/// allocations, compared by address. Each case also states its count, so a
/// position that BOTH walks miss still fails.
#[cfg(test)]
mod cmdsub_twin_tests {
    use super::{
        ArithExpr, Command, Expr, ExprKind, ParameterExpansion, Span, Statement, StmtKind,
    };

    fn body_addresses(stmt: &mut Statement) -> (Vec<*const Statement>, Vec<*const Statement>) {
        let shared = stmt.cmdsub_bodies().iter().map(|b| b.as_ptr()).collect();
        let unique = stmt
            .cmdsub_bodies_mut()
            .iter()
            .map(|b| b.as_ptr())
            .collect();
        (shared, unique)
    }

    /// Arithmetic cases reach their captures through `${…}` because a capture
    /// is never an arithmetic operand (DD#80) — a bare `$( … )` there does not
    /// parse.
    #[test]
    fn cmdsub_bodies_mut_mirrors_cmdsub_bodies() {
        let cases: &[(&str, usize)] = &[
            ("echo $(a) \"x$(b)\" y$(c)", 3),
            ("echo $(a $(b))", 1),
            ("$(a) b", 1),
            ("echo ${v[$(a)]:-d}", 1),
            ("echo ${v[$(a)]}", 1),
            ("echo $(( ${w[$(a)]:-0} + w[${v[$(b)]:-0}] ))", 2),
            ("echo $(( ${v[$(a)]:-0} ))", 1),
            ("[[ -n $(a) && $(b) == c ]]", 2),
            ("[[ $(a) == $(b) { echo } || { echo } ]]", 2),
            ("x[$(a)]=$(b)", 2),
            ("str y=$(a) z=$(b)", 2),
            ("int n=$(( ${w[$(a)]:-0} + 1 ))", 1),
            ("for z in $(a) $(b) { echo }", 2),
            ("echo hi >${x[$(a)]:-f}", 1),
            ("(( ${w[$(a)]:-0} > 0 ))", 1),
            ("(( n = w[${v[$(a)]:-0}] ? ${w[$(b)]:-0} : 0 ))", 2),
            ("(( w[${v[$(a)]:-0}] = ${w[$(b)]:-0} ))", 2),
            ("array q=($(a) $(b))", 2),
            ("map m=([x]=$(a))", 1),
            ("y=$(<$(a))", 1),
        ];
        for &(src, want) in cases {
            let (mut program, _) =
                crate::parse::parse(src).unwrap_or_else(|d| panic!("{src}: {d:?}"));
            let stmt = &mut program.statements[0];
            let (shared, unique) = body_addresses(stmt);
            assert_eq!(shared.len(), want, "{src}: cmdsub_bodies count");
            assert_eq!(shared, unique, "{src}: cmdsub_bodies_mut diverges");
        }
    }

    fn word(kind: ExprKind) -> Expr {
        Expr {
            kind,
            span: Span::new(1, 1),
        }
    }

    fn command(name: &str, args: Vec<Expr>) -> Statement {
        Statement {
            kind: StmtKind::Command(Command {
                name: word(ExprKind::Literal(name.into())),
                args,
            }),
            span: Span::new(1, 1),
        }
    }

    fn capture(name: &str) -> Box<Expr> {
        Box::new(word(ExprKind::CommandSubstitution(vec![command(
            name,
            vec![],
        )])))
    }

    /// No parse puts a `$( … )` in a parameter expansion's operand: every
    /// operand the parser builds is literal text. `:-` and `:?` take a literal
    /// (a `$` there is SB4070), substring offsets and a retired operator's
    /// text (SB4034) are kept verbatim, and `:s/…/` desugars to a nested
    /// `$(sub …)` in place of the expansion. The arm is still part of the
    /// shape both walks mirror, so it is pinned here, operand after index.
    #[test]
    fn cmdsub_bodies_mut_mirrors_a_parameter_operand() {
        let pe = ParameterExpansion {
            name: "v".into(),
            operator: Some(":-".into()),
            operand: Some(capture("b")),
            index: Some(capture("a")),
            taint_off: false,
        };
        let mut stmt = command("echo", vec![word(ExprKind::ParameterExpansion(pe))]);
        let (shared, unique) = body_addresses(&mut stmt);
        assert_eq!(shared.len(), 2, "cmdsub_bodies count");
        assert_eq!(shared, unique, "cmdsub_bodies_mut diverges");
    }

    /// `${v[$( name )]:-0}` as an arithmetic leaf — the shape a parsed capture
    /// takes there (see the table above).
    fn defaulted_capture(name: &str) -> Box<ArithExpr> {
        let pe = ParameterExpansion {
            name: "v".into(),
            operator: Some(":-".into()),
            operand: Some(Box::new(word(ExprKind::Literal("0".into())))),
            index: Some(capture(name)),
            taint_off: false,
        };
        Box::new(ArithExpr::Expansion(Box::new(word(
            ExprKind::ParameterExpansion(pe),
        ))))
    }

    /// No program reaches the `SubscriptAssign` arm of either walk. The parser
    /// lowers a statement `(( w[i] = v ))` or `w[i]++` to an assignment (the
    /// `(( w[…] = … ))` row above), and every mutation nested in a larger
    /// expression is SB4020/SB5007 in `validate`, before resolution runs;
    /// bindings ICEs on one that survives. The arm is the canonical walk's
    /// shape, which descends into both sides, so that is what is pinned: a
    /// capture in each.
    #[test]
    fn cmdsub_bodies_mut_mirrors_a_subscript_assignment() {
        let arith = ArithExpr::SubscriptAssign {
            name: "w".into(),
            index: defaulted_capture("a"),
            op: None,
            value: defaulted_capture("b"),
        };
        let mut stmt = command("echo", vec![word(ExprKind::Arithmetic(Box::new(arith)))]);
        let (shared, unique) = body_addresses(&mut stmt);
        assert_eq!(shared.len(), 2, "cmdsub_bodies count");
        assert_eq!(shared, unique, "cmdsub_bodies_mut diverges");
    }
}
