//! DD#86: give each lexical local one identity before typing and lowering.

use crate::ir::*;
use std::collections::{HashMap, HashSet};

type Scope = HashMap<String, String>;

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct SourceNames(HashMap<String, String>);

impl SourceNames {
    pub(crate) fn original<'a>(&'a self, name: &'a str) -> &'a str {
        self.0.get(name).map_or(name, String::as_str)
    }

    pub(crate) fn restore(&self, message: &str) -> String {
        let mut out = String::with_capacity(message.len());
        let mut start = 0;
        for (i, ch) in message.char_indices() {
            if !ch.is_ascii_alphanumeric() && ch != '_' {
                out.push_str(self.original(&message[start..i]));
                out.push(ch);
                start = i + ch.len_utf8();
            }
        }
        out.push_str(self.original(&message[start..]));
        out
    }
}

pub(crate) fn resolve(program: &Program, source: &str) -> (Program, SourceNames) {
    let globals = global_scope(program);
    let mut resolver = Resolver {
        globals: globals.clone(),
        functions: program
            .statements
            .iter()
            .filter_map(|s| match &s.kind {
                StmtKind::Function(f) => Some(f.name.clone()),
                _ => None,
            })
            .collect(),
        used: source
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .map(str::to_string)
            .collect(),
        names: SourceNames::default(),
        next_scope: 0,
    };
    let mut program = program.clone();
    resolver.statements(&mut program.statements, &globals, "top");
    if let Some(end) = &mut program.end_block {
        resolver.statements(end, &globals, "top");
    }
    (program, resolver.names)
}

fn global_scope(program: &Program) -> Scope {
    let mut globals = HashSet::new();
    crate::validate::collect_scope_names(&program.statements, &mut globals);
    if let Some(end) = &program.end_block {
        crate::validate::collect_scope_names(end, &mut globals);
    }
    globals.into_iter().map(|n| (n.clone(), n)).collect()
}

struct Resolver {
    globals: Scope,
    functions: HashSet<String>,
    used: HashSet<String>,
    names: SourceNames,
    next_scope: usize,
}

impl Resolver {
    fn local(&mut self, owner: &str, name: &str) -> String {
        let base = format!("__{owner}_{name}");
        let mut qualified = base.clone();
        let mut suffix = 0;
        while !self.used.insert(qualified.clone()) {
            suffix += 1;
            qualified = format!("{base}_{suffix}");
        }
        self.names.0.insert(qualified.clone(), name.to_string());
        qualified
    }

    fn child_owner(&mut self, owner: &str) -> String {
        let id = self.next_scope;
        self.next_scope += 1;
        format!("{owner}_capture{id}")
    }

    fn scope(&mut self, body: &mut [Statement], parent: &Scope, owner: &str) {
        let mut introduced = HashSet::new();
        crate::validate::collect_scope_names(body, &mut introduced);
        let mut introduced: Vec<_> = introduced.into_iter().collect();
        introduced.sort();
        let mut scope = parent.clone();
        for name in introduced {
            if local_name(&name) && !scope.contains_key(&name) {
                scope.insert(name.clone(), self.local(owner, &name));
            }
        }
        self.statements(body, &scope, owner);
    }

    fn statements(&mut self, body: &mut [Statement], scope: &Scope, owner: &str) {
        for stmt in body {
            self.statement(stmt, scope, owner);
        }
    }

    /// Total over `StmtKind`; do not add a wildcard.
    fn statement(&mut self, stmt: &mut Statement, scope: &Scope, owner: &str) {
        match &mut stmt.kind {
            StmtKind::Function(f) => {
                self.function(f);
                return;
            }
            StmtKind::Assignment(a) => {
                rename(&mut a.name, scope);
                self.optional_expr(a.index.as_mut(), scope, owner);
                self.expr(&mut a.value, scope, owner);
            }
            StmtKind::Declaration(d) => self.declaration(d, scope, owner),
            StmtKind::Command(c) => self.command(c, scope, owner),
            StmtKind::For(f) => {
                rename(&mut f.variable, scope);
                self.exprs(&mut f.words, scope, owner);
            }
            StmtKind::Case(c) => {
                self.expr(&mut c.value, scope, owner);
                self.case_patterns(&mut c.items, scope, owner);
            }
            StmtKind::Redirected(r) => self.redirects(&mut r.redirects, scope, owner),
            StmtKind::ArithTest(a) => self.arith(a, scope, owner),
            StmtKind::Pipeline(_)
            | StmtKind::If(_)
            | StmtKind::While(_)
            | StmtKind::List(_)
            | StmtKind::Group(_)
            | StmtKind::Suppressed(_)
            | StmtKind::Negated(_)
            | StmtKind::ReturnProducer(_) => {}
        }
        for body in stmt.structural_bodies_mut() {
            self.statements(body, scope, owner);
        }
    }

    fn function(&mut self, f: &mut Function) {
        let globals = self.globals.clone();
        let owner = f
            .name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>();
        self.scope(&mut f.body, &globals, &owner);
    }

    fn declaration(&mut self, d: &mut Declaration, scope: &Scope, owner: &str) {
        for a in &mut d.assignments {
            rename(&mut a.name, scope);
            self.optional_expr(a.value.as_mut(), scope, owner);
        }
    }

    fn case_patterns(&mut self, items: &mut [CaseItem], scope: &Scope, owner: &str) {
        for item in items {
            self.exprs(&mut item.patterns, scope, owner);
        }
    }

    fn redirects(&mut self, redirects: &mut [Redirect], scope: &Scope, owner: &str) {
        for redirect in redirects {
            self.expr(&mut redirect.target, scope, owner);
        }
    }

    fn command(&mut self, c: &mut Command, scope: &Scope, owner: &str) {
        let predicate_scope = self.predicate_scope(c, scope, owner);
        let scope = &predicate_scope;
        rename_command_operands(c, scope, &self.functions);
        self.expr(&mut c.name, scope, owner);
        self.exprs(&mut c.args, scope, owner);
    }

    fn predicate_scope(&mut self, c: &Command, scope: &Scope, owner: &str) -> Scope {
        let mut scope = scope.clone();
        let Some((form, _)) = classify_attest(&c.args)
            .filter(|_| crate::type_check::expr_to_name(&c.name) == "attest")
        else {
            return scope;
        };
        for binder in attest_binders(form) {
            if local_name(&binder) {
                scope.insert(binder.clone(), self.local(owner, &binder));
            }
        }
        scope
    }

    fn optional_expr(&mut self, expr: Option<&mut Expr>, scope: &Scope, owner: &str) {
        if let Some(expr) = expr {
            self.expr(expr, scope, owner);
        }
    }

    fn exprs(&mut self, exprs: &mut [Expr], scope: &Scope, owner: &str) {
        for expr in exprs {
            self.expr(expr, scope, owner);
        }
    }

    /// Total over `ExprKind`; do not add a wildcard.
    fn expr(&mut self, expr: &mut Expr, scope: &Scope, owner: &str) {
        match &mut expr.kind {
            ExprKind::SimpleExpansion(name) => rename(name, scope),
            ExprKind::ParameterExpansion(pe) => {
                rename(&mut pe.name, scope);
                self.optional_expr(pe.index.as_deref_mut(), scope, owner);
                self.optional_expr(pe.operand.as_deref_mut(), scope, owner);
            }
            ExprKind::Subscript { name, index } => {
                rename(name, scope);
                self.expr(index, scope, owner);
            }
            ExprKind::CommandSubstitution(body) => {
                let child = self.child_owner(owner);
                self.scope(body, scope, &child);
            }
            ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) | ExprKind::Array(parts) => {
                self.exprs(parts, scope, owner);
            }
            ExprKind::AssocArray(pairs) => self.assoc_values(pairs, scope, owner),
            ExprKind::SlurpFile(path) => self.expr(path, scope, owner),
            ExprKind::Arithmetic(a) => self.arith(a, scope, owner),
            ExprKind::Test(t) => self.test(t, scope, owner),
            ExprKind::Literal(_) | ExprKind::SingleQuoted(_) => {}
        }
    }

    fn assoc_values(&mut self, pairs: &mut [(String, Expr)], scope: &Scope, owner: &str) {
        for (_, value) in pairs {
            self.expr(value, scope, owner);
        }
    }

    /// Total over `TestExpr`; do not add a wildcard.
    fn test(&mut self, test: &mut TestExpr, scope: &Scope, owner: &str) {
        match test {
            TestExpr::Unary { operand, .. } => self.expr(operand, scope, owner),
            TestExpr::Binary { left, right, .. } => {
                self.expr(left, scope, owner);
                self.expr(right, scope, owner);
            }
            TestExpr::Not(t) => self.test(t, scope, owner),
            TestExpr::And(a, b) | TestExpr::Or(a, b) => {
                self.test(a, scope, owner);
                self.test(b, scope, owner);
            }
        }
    }

    /// Total over `ArithExpr`; do not add a wildcard.
    fn arith(&mut self, a: &mut ArithExpr, scope: &Scope, owner: &str) {
        match a {
            ArithExpr::Var(name)
            | ArithExpr::DollarVar(name)
            | ArithExpr::PostIncDec { var: name, .. }
            | ArithExpr::PreIncDec { var: name, .. } => rename(name, scope),
            ArithExpr::Assign { var, value } | ArithExpr::CompoundAssign { var, value, .. } => {
                rename(var, scope);
                self.arith(value, scope, owner);
            }
            ArithExpr::Subscript { name, index } => {
                rename(name, scope);
                self.arith(index, scope, owner);
            }
            ArithExpr::SubscriptAssign { .. } => unreachable!(
                "arithmetic subscript mutation is lowered to a statement; nested/value mutations are rejected before binding resolution"
            ),
            ArithExpr::BinOp { left, right, .. } => {
                self.arith(left, scope, owner);
                self.arith(right, scope, owner);
            }
            ArithExpr::UnaryOp { operand, .. } | ArithExpr::Paren(operand) => {
                self.arith(operand, scope, owner)
            }
            ArithExpr::Ternary {
                cond,
                true_val,
                false_val,
            } => {
                self.arith(cond, scope, owner);
                self.arith(true_val, scope, owner);
                self.arith(false_val, scope, owner);
            }
            ArithExpr::Comma(parts) | ArithExpr::FuncCall { args: parts, .. } => {
                self.arith_parts(parts, scope, owner);
            }
            ArithExpr::Expansion(e) => self.expr(e, scope, owner),
            ArithExpr::Literal(_) => {}
        }
    }

    fn arith_parts(&mut self, parts: &mut [ArithExpr], scope: &Scope, owner: &str) {
        for part in parts {
            self.arith(part, scope, owner);
        }
    }
}

fn local_name(name: &str) -> bool {
    is_plain_identifier(name)
        && !is_discard_binder(name)
        && !is_collection_slot(name)
        && name != "args"
        && !crate::resolve::is_reserved_name(name)
}

fn rename(name: &mut String, scope: &Scope) {
    if let Some(qualified) = scope.get(name) {
        *name = qualified.clone();
    }
}

fn attest_binders(form: AttestForm<'_>) -> Vec<String> {
    match form {
        AttestForm::Scalar { .. } => vec![],
        AttestForm::Indexed { binder, .. } => vec![binder.to_string()],
        AttestForm::Map { key, val, .. } => vec![key.to_string(), val.to_string()],
    }
}

fn rename_command_operands(c: &mut Command, scope: &Scope, functions: &HashSet<String>) {
    let name = crate::type_check::expr_to_name(&c.name);
    let indices = command_name_operands(c, &name);
    if functions.contains(&name) && indices.is_empty() {
        rename_collection_args(&mut c.args, scope);
    }
    rename_name_operands(&mut c.args, indices, scope);
}

fn rename_name_operands(args: &mut [Expr], indices: Vec<usize>, scope: &Scope) {
    for i in indices {
        if let Some(arg) = args.get_mut(i) {
            rename_name_operand(arg, scope);
        }
    }
}

fn command_name_operands(c: &Command, name: &str) -> Vec<usize> {
    let last = c.args.len().checked_sub(1);
    match name {
        "read" | "readline" | "argv" => last.into_iter().collect(),
        "match" | "matchall" if c.args.len() == 3 => vec![2],
        "keys" => (0..c.args.len()).collect(),
        "len" | "join" => vec![0],
        "unset" => (0..c.args.len()).collect(),
        "attest" => (0..c.args.len().saturating_sub(1)).collect(),
        _ => vec![],
    }
}

fn rename_collection_args(args: &mut [Expr], scope: &Scope) {
    for arg in args {
        if let Some(name) = collection_arg_name(arg).and_then(|n| scope.get(&n)) {
            arg.kind = ExprKind::Literal(format!("={name}"));
        }
    }
}

fn rename_name_operand(arg: &mut Expr, scope: &Scope) {
    if let ExprKind::Literal(name) = &mut arg.kind {
        rename_literal_operand(name, scope);
    }
}

fn rename_literal_operand(name: &mut String, scope: &Scope) {
    if let Some((base, suffix)) = name.split_once('[') {
        if let Some(qualified) = scope.get(base) {
            *name = format!("{qualified}[{suffix}");
        }
    } else {
        rename(name, scope);
    }
}

#[cfg(test)]
mod invariant_tests {
    use super::*;

    #[test]
    #[should_panic(expected = "arithmetic subscript mutation is lowered to a statement")]
    fn binding_resolution_refuses_unlowered_subscript_mutation() {
        let program = Program {
            statements: vec![Statement {
                kind: StmtKind::ArithTest(Box::new(ArithExpr::SubscriptAssign {
                    name: "items".to_string(),
                    index: Box::new(ArithExpr::Literal(0)),
                    op: None,
                    value: Box::new(ArithExpr::Literal(7)),
                })),
                span: Span::new(1, 1),
            }],
            end_block: None,
        };
        resolve(&program, "");
    }
}
