/// safebash type checker (DD#16).
///
/// Operates on the IR after lowering. Reports type errors for:
/// - Cross-type assignment without explicit conversion
/// - Numeric comparison operators on string variables
/// - String comparison operators on int variables
/// - `exit` with unbounded int variable
/// - `if func` where func is `-> int`
use crate::ir::*;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub struct TypeError {
    pub message: String,
    pub span: Span,
}

impl std::fmt::Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "type error at {}:{}: {}",
            self.span.line, self.span.col, self.message
        )
    }
}

/// Environment tracking variable and function types.
struct TypeEnv {
    vars: HashMap<String, VarType>,
    funcs: HashMap<String, VarType>,
    /// Variables that are intrinsically int ($#, $?)
    intrinsic_int: HashSet<String>,
    /// Variables declared as array or map (collections)
    collections: HashSet<String>,
    /// Variables declared as map (associative array) — string keys
    assoc_arrays: HashSet<String>,
}

impl TypeEnv {
    fn new() -> Self {
        let mut intrinsic_int = HashSet::new();
        intrinsic_int.insert("#".to_string()); // $#
        intrinsic_int.insert("?".to_string()); // $?
        Self {
            vars: HashMap::new(),
            funcs: HashMap::new(),
            intrinsic_int,
            collections: HashSet::new(),
            assoc_arrays: HashSet::new(),
        }
    }

    fn var_type(&self, name: &str) -> VarType {
        if self.intrinsic_int.contains(name) {
            VarType::Int
        } else {
            self.vars.get(name).copied().unwrap_or(VarType::String)
        }
    }

    fn func_type(&self, name: &str) -> VarType {
        self.funcs.get(name).copied().unwrap_or(VarType::String)
    }

    fn is_collection(&self, name: &str) -> bool {
        self.collections.contains(name)
    }

    fn declare(&mut self, name: &str, vtype: VarType) {
        // Don't downgrade Int to String — once declared Int, it stays Int
        if vtype == VarType::String {
            if let Some(existing) = self.vars.get(name) {
                if *existing == VarType::Int {
                    return; // keep Int
                }
            }
        }
        self.vars.insert(name.to_string(), vtype);
    }
}

/// Infer the type of an expression in context.
fn expr_type(expr: &Expr, env: &TypeEnv) -> VarType {
    match &expr.kind {
        ExprKind::Literal(s) => {
            // Bare numbers are ambiguous — treat as string (bash default)
            // Only explicit `declare int` or $(()) makes something int
            let _ = s;
            VarType::String
        }
        ExprKind::SimpleExpansion(var) => env.var_type(var),
        ExprKind::ArithmeticExpansion(_) => VarType::Int,
        ExprKind::CommandSubstitution(parts) => {
            // $(func ...) — check if func is int
            if let Some(first) = parts.first() {
                if let StmtKind::Command(c) = &first.kind {
                    let name = expr_to_name(&c.name);
                    // Compiler keywords that return int
                    if matches!(
                        name.as_str(),
                        "signum" | "argc" | "status" | "getpid" | "getppid" | "getcpid" | "dtoi"
                    ) {
                        return VarType::Int;
                    }
                    return env.func_type(&name);
                }
            }
            VarType::String
        }
        ExprKind::DoubleQuoted(_) => VarType::String,
        ExprKind::SingleQuoted(_) => VarType::String,
        ExprKind::ParameterExpansion(pe) => {
            if let Some(op) = &pe.operator {
                if op == ":-" || op == "-" {
                    // Default value — type follows the variable
                    env.var_type(&pe.name)
                } else {
                    // Strip ops always produce strings
                    VarType::String
                }
            } else {
                env.var_type(&pe.name)
            }
        }
        _ => VarType::String,
    }
}

fn expr_to_name(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(s) => s.clone(),
        ExprKind::SimpleExpansion(v) => v.clone(),
        _ => String::new(),
    }
}

/// Type-check a program. Returns a list of type errors (empty = OK).
pub fn type_check(program: &Program) -> Vec<TypeError> {
    let mut errors = Vec::new();
    let mut env = TypeEnv::new();

    // First pass: collect function types
    for stmt in &program.statements {
        if let StmtKind::Function(f) = &stmt.kind {
            env.funcs.insert(f.name.clone(), f.return_type);
        }
    }

    // Second pass: check statements
    for stmt in &program.statements {
        check_stmt(stmt, &mut env, &mut errors);
    }

    errors
}

/// DD#33: Check expressions for subscript key type mismatches.
fn check_subscript_types(expr: &Expr, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    match &expr.kind {
        ExprKind::Subscript { name, index } => {
            let is_map = env.assoc_arrays.contains(name);
            let is_collection = env.collections.contains(name);
            if is_collection {
                if is_map {
                    // Map: reject integer-only keys (looks like indexed access)
                    if is_integer_literal(index) {
                        errors.push(TypeError {
                            message: format!(
                                "integer key on map '{}' — maps use string keys. Use ${{{}[\"key\"]}}",
                                name, name,
                            ),
                            span: expr.span.clone(),
                        });
                    }
                } else {
                    // Indexed array: reject non-integer keys
                    if is_string_key(index) {
                        errors.push(TypeError {
                            message: format!(
                                "string key on indexed array '{}' — use integer indices. Declare as `map` for string keys",
                                name,
                            ),
                            span: expr.span.clone(),
                        });
                    }
                }
            }
            check_subscript_types(index, env, errors);
        }
        ExprKind::DoubleQuoted(parts) => {
            for p in parts {
                check_subscript_types(p, env, errors);
            }
        }
        ExprKind::Concat(parts) => {
            for p in parts {
                check_subscript_types(p, env, errors);
            }
        }
        ExprKind::CommandSubstitution(stmts) => {
            for s in stmts {
                check_subscript_in_stmt(s, env, errors);
            }
        }
        _ => {}
    }
}

fn check_subscript_in_stmt(stmt: &Statement, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    match &stmt.kind {
        StmtKind::Command(c) => {
            check_subscript_types(&c.name, env, errors);
            for arg in &c.args {
                check_subscript_types(arg, env, errors);
            }
        }
        StmtKind::Assignment(a) => {
            check_subscript_types(&a.value, env, errors);
        }
        _ => {}
    }
}

/// Check if an expression is a plain string literal (non-numeric)
fn is_string_key(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Literal(s) => !s.is_empty() && !s.chars().all(|c| c.is_ascii_digit()),
        ExprKind::SimpleExpansion(_) => false, // variable — could be either
        _ => false,
    }
}

/// Check if an expression is an integer literal
fn is_integer_literal(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Literal(s) => !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()),
        _ => false,
    }
}

fn check_stmt(stmt: &Statement, env: &mut TypeEnv, errors: &mut Vec<TypeError>) {
    match &stmt.kind {
        StmtKind::Assignment(a) => {
            let rtype = expr_type(&a.value, env);
            let ltype = env.var_type(&a.name);
            // Bare assignment: variable gets the RHS type on first use
            // But if already declared with a type, check compatibility
            if env.vars.contains_key(&a.name) && ltype != rtype {
                errors.push(TypeError {
                    message: format!(
                        "cannot assign {} value to {} variable `{}`{}",
                        type_name(rtype),
                        type_name(ltype),
                        a.name,
                        conversion_hint(rtype, ltype),
                    ),
                    span: stmt.span.clone(),
                });
            } else {
                env.declare(&a.name, rtype);
            }
        }

        StmtKind::Declaration(d) => {
            for a in &d.assignments {
                let rtype = expr_type(&a.value, env);
                if d.var_type == VarType::Int && rtype == VarType::String {
                    // String assigned to int variable — need conversion
                    // Exception: literal numbers in quotes are OK (common pattern)
                    if !is_numeric_literal(&a.value) {
                        errors.push(TypeError {
                            message: format!(
                                "cannot assign string value to int variable `{}`{}",
                                a.name,
                                conversion_hint(VarType::String, VarType::Int),
                            ),
                            span: stmt.span.clone(),
                        });
                    }
                } else if d.var_type == VarType::String && rtype == VarType::Int {
                    errors.push(TypeError {
                        message: format!(
                            "cannot assign int value to string variable `{}`{}",
                            a.name,
                            conversion_hint(VarType::Int, VarType::String),
                        ),
                        span: stmt.span.clone(),
                    });
                }
                env.declare(&a.name, d.var_type);
            }
            // Track collections
            if d.flags.contains(&"-a".to_string()) || d.flags.contains(&"-A".to_string()) {
                for a in &d.assignments {
                    env.collections.insert(a.name.clone());
                    if d.flags.contains(&"-A".to_string()) {
                        env.assoc_arrays.insert(a.name.clone());
                    }
                }
            }
        }

        StmtKind::Command(c) => {
            let cmd_name = expr_to_name(&c.name);
            if cmd_name == "unset" {
                // DD#30: unset on scalar variables is rejected
                for arg in &c.args {
                    let name = expr_to_name(arg);
                    // If the name contains '[', it's a subscript (array/map element) — allowed
                    if !name.contains('[') && !name.is_empty() {
                        let vtype = env.var_type(&name);
                        if vtype == VarType::String || vtype == VarType::Int {
                            // Check if it's declared as array/map — those are OK
                            if !env.is_collection(&name) {
                                errors.push(TypeError {
                                    message: format!(
                                        "unset on scalar '{}' is not allowed — use {}=\"\" to clear",
                                        name, name,
                                    ),
                                    span: stmt.span.clone(),
                                });
                            }
                        }
                    }
                }
            }
            if cmd_name == "exit" {
                // exit with int variable → error (unbounded)
                if let Some(arg) = c.args.first() {
                    let atype = expr_type(arg, env);
                    if atype == VarType::Int {
                        if let ExprKind::SimpleExpansion(var) = &arg.kind {
                            errors.push(TypeError {
                                message: format!(
                                    "exit with int variable `${}` — value may exceed 0-255. \
                                     Use `exit $(({}<256?{}:1))`",
                                    var, var, var,
                                ),
                                span: stmt.span.clone(),
                            });
                        }
                    }
                }
            }

            // DD#33: check subscript key types in command args
            for arg in &c.args {
                check_subscript_types(arg, env, errors);
            }
        }

        StmtKind::If(if_stmt) => {
            // Check condition for int-function-in-boolean-context
            check_condition_type(&if_stmt.condition, env, errors);
            for s in &if_stmt.condition {
                check_stmt(s, env, errors);
            }
            for s in &if_stmt.then_body {
                check_stmt(s, env, errors);
            }
            for elif in &if_stmt.elif_clauses {
                for s in &elif.condition {
                    check_stmt(s, env, errors);
                }
                for s in &elif.body {
                    check_stmt(s, env, errors);
                }
            }
            if let Some(else_body) = &if_stmt.else_body {
                for s in else_body {
                    check_stmt(s, env, errors);
                }
            }
        }

        StmtKind::For(for_stmt) => {
            for s in &for_stmt.body {
                check_stmt(s, env, errors);
            }
        }

        StmtKind::While(while_stmt) => {
            for s in &while_stmt.condition {
                check_stmt(s, env, errors);
            }
            for s in &while_stmt.body {
                check_stmt(s, env, errors);
            }
        }

        StmtKind::Function(f) => {
            // Type-check the body
            for s in &f.body {
                check_stmt(s, env, errors);
            }
        }

        StmtKind::Case(case_stmt) => {
            for item in &case_stmt.items {
                for s in &item.body {
                    check_stmt(s, env, errors);
                }
            }
        }

        _ => {}
    }
}

/// Check if a condition block uses an int function in boolean context.
fn check_condition_type(condition: &[Statement], env: &TypeEnv, errors: &mut Vec<TypeError>) {
    for stmt in condition {
        if let StmtKind::Command(c) = &stmt.kind {
            let name = expr_to_name(&c.name);
            if env.func_type(&name) == VarType::Int {
                errors.push(TypeError {
                    message: format!(
                        "int function `{}` used in boolean context — \
                         int functions return values, not success/failure",
                        name,
                    ),
                    span: stmt.span.clone(),
                });
            }
        }
    }
}

fn is_numeric_literal(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Literal(s) => s.chars().all(|c| c.is_ascii_digit() || c == '-'),
        _ => false,
    }
}

fn type_name(t: VarType) -> &'static str {
    match t {
        VarType::Int => "int",
        VarType::String => "string",
    }
}

fn conversion_hint(from: VarType, to: VarType) -> String {
    match (from, to) {
        (VarType::Int, VarType::String) => " — use `\"$var\"` to convert int to string".to_string(),
        (VarType::String, VarType::Int) => {
            " — use `$(($var))` to convert string to int".to_string()
        }
        _ => String::new(),
    }
}

/// Run type inference and return the variable type map.
pub fn infer_types(program: &Program) -> HashMap<String, VarType> {
    let mut env = TypeEnv::new();

    // Collect function types
    for stmt in &program.statements {
        if let StmtKind::Function(f) = &stmt.kind {
            env.funcs.insert(f.name.clone(), f.return_type);
        }
    }

    // Walk statements to discover variable types
    for stmt in &program.statements {
        infer_stmt(stmt, &mut env);
    }

    env.vars
}

fn infer_stmt(stmt: &Statement, env: &mut TypeEnv) {
    match &stmt.kind {
        StmtKind::Assignment(a) => {
            let t = expr_type(&a.value, env);
            env.declare(&a.name, t);
        }
        StmtKind::Declaration(d) => {
            if d.var_type == VarType::Int {
                for a in &d.assignments {
                    env.declare(&a.name, VarType::Int);
                }
            } else {
                for a in &d.assignments {
                    let t = expr_type(&a.value, env);
                    env.declare(&a.name, t);
                }
            }
        }
        StmtKind::If(i) => {
            for s in &i.then_body {
                infer_stmt(s, env);
            }
            for elif in &i.elif_clauses {
                for s in &elif.body {
                    infer_stmt(s, env);
                }
            }
            if let Some(e) = &i.else_body {
                for s in e {
                    infer_stmt(s, env);
                }
            }
        }
        StmtKind::While(w) => {
            for s in &w.body {
                infer_stmt(s, env);
            }
        }
        StmtKind::For(f) => {
            for s in &f.body {
                infer_stmt(s, env);
            }
        }
        StmtKind::Function(f) => {
            for s in &f.body {
                infer_stmt(s, env);
            }
        }
        _ => {}
    }
}
