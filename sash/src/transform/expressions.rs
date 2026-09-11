//! Expression → SSA value conversion.
//!
//! SECURITY: expr_to_value converts IR expressions to SsaValue.
//! This is where taint originates — $(getenv), $(argv), command
//! substitution all produce tainted SsaValues.

use crate::ir::*;
use crate::ssa::*;

pub(super) fn expr_to_value(expr: &Expr, state: &VersionState) -> SsaValue {
    match &expr.kind {
        ExprKind::Literal(s) => SsaValue::Literal(s.clone()),
        ExprKind::SimpleExpansion(var) => simple_expansion_to_value(var, state),
        ExprKind::SingleQuoted(s) => SsaValue::Literal(s.clone()),
        ExprKind::Arithmetic(a) => {
            super::expr_handlers::handle_structured_arithmetic(a, expr, state)
        }
        ExprKind::ParameterExpansion(pe) => parameter_expansion_to_value(pe, state),
        ExprKind::DoubleQuoted(parts) => dq_to_value(parts, state),
        ExprKind::AssocArray(pairs) => assoc_array_to_value(pairs, state),
        ExprKind::Subscript { name, index } => subscript_to_value(name, index, state),
        ExprKind::Concat(parts) => concat_to_value(parts, state),
        ExprKind::Array(elems) => lower_array_literal(elems, state),
        _ => super::expr_handlers::handle_command_substitution(expr, state),
    }
}

/// Lower a command-*argument* expression to an `SsaValue`, recognising
/// the `@a` / `"${a[@]}"` array splat (a `ParameterExpansion` whose
/// operator is `arrvals`) as [`SsaValue::ArraySplat`] so it expands to
/// one verbatim argv entry per element at codegen time (DD#20/DD#22).
/// Outside command-arg position `@a` keeps its joined-string meaning via
/// [`expr_to_value`].
pub(super) fn arg_to_value(expr: &Expr, state: &VersionState) -> SsaValue {
    if let ExprKind::ParameterExpansion(pe) = &expr.kind {
        if let Some(disposition) = splat_disposition(pe, state) {
            return SsaValue::ArraySplat {
                array: resolve_args_name(&pe.name, state),
                disposition: Some(disposition),
            };
        }
    }
    match command_arg_splat(&expr.kind) {
        Some(array) => SsaValue::ArraySplat {
            array: resolve_args_name(&array, state),
            disposition: None,
        },
        None => expr_to_value(expr, state),
    }
}

/// DD#69: `@{a:-WORD}` / `@{a:?WORD}` → the empty-disposition (operator
/// `arrvals_default` / `arrvals_abort`; the WORD is the interpolating
/// operand, absent for the bare `@{a:-}`). `None` for a plain `arrvals`.
fn splat_disposition(
    pe: &ParameterExpansion,
    state: &VersionState,
) -> Option<crate::ssa::ArraySplatDisposition> {
    // DD#91 addendum: the disposition and the `!` modifier are independent —
    // the operator says what happens when the array is empty, `taint_off` says
    // whether the elements are trusted.
    let abort = match pe.operator.as_deref() {
        Some("arrvals_default") => false,
        Some("arrvals_abort") => true,
        _ => return None,
    };
    let taint_off = pe.taint_off;
    let has_word = pe.operand.is_some();
    let word = pe
        .operand
        .as_ref()
        .map(|e| expr_to_value(e, state))
        .unwrap_or_else(|| SsaValue::Literal(String::new()));
    Some(crate::ssa::ArraySplatDisposition {
        word: Box::new(word),
        has_word,
        abort,
        taint_off,
    })
}

/// Recognise `@a` and the equivalent single-part `"${a[@]}"` as an array
/// splat, returning the array name.
fn command_arg_splat(kind: &ExprKind) -> Option<String> {
    match kind {
        ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrvals") => {
            Some(pe.name.clone())
        }
        ExprKind::DoubleQuoted(parts) if parts.len() == 1 => command_arg_splat(&parts[0].kind),
        _ => None,
    }
}

/// A statically-known operand's comparison/pattern string: strip outer
/// quotes so `'a b'` is the pattern `a b`, not `'a b'`, and concatenate
/// multi-part (`Concat`) operands (e.g. bare `a(b`). Only meaningful when
/// `expr_is_dynamic` is false.
pub(super) fn operand_pattern_string(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::SingleQuoted(s) => s.clone(),
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) => {
            parts.iter().map(operand_pattern_string).collect()
        }
        _ => expr_to_string(expr),
    }
}

/// True if `expr` has any runtime-valued part (an expansion / cmdsub /
/// arithmetic / subscript) — i.e. it is not a compile-time-constant string.
pub(super) fn expr_is_dynamic(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Literal(_) | ExprKind::SingleQuoted(_) => false,
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) => {
            parts.iter().any(expr_is_dynamic)
        }
        _ => true,
    }
}

/// DD#62/#71: pre-lower a `[[ ]]` `TestExpr` into an `SsaTest` whose value
/// operands are `SsaValue` (so codegen uses the ordinary value path, with
/// `assoc_arrays`-correct subscript key-typing). A **static** `=~` right
/// operand is a raw `Pattern` string (comptime-validatable); a **dynamic**
/// one (interpolated) is a runtime `Value` built and handed to regcomp.
pub(super) fn test_to_ssa(test: &TestExpr, state: &VersionState) -> SsaTest {
    match test {
        TestExpr::Unary { op, operand } => SsaTest::Unary {
            op: op.clone(),
            operand: expr_to_value(operand, state),
        },
        TestExpr::Binary { left, op, right } => {
            let rhs = if op == "=~" && !expr_is_dynamic(right) {
                SsaTestRhs::Pattern(operand_pattern_string(right))
            } else {
                SsaTestRhs::Value(expr_to_value(right, state))
            };
            SsaTest::Binary {
                left: expr_to_value(left, state),
                op: op.clone(),
                right: rhs,
            }
        }
        TestExpr::Not(inner) => SsaTest::Not(Box::new(test_to_ssa(inner, state))),
        TestExpr::And(a, b) => SsaTest::And(
            Box::new(test_to_ssa(a, state)),
            Box::new(test_to_ssa(b, state)),
        ),
        TestExpr::Or(a, b) => SsaTest::Or(
            Box::new(test_to_ssa(a, state)),
            Box::new(test_to_ssa(b, state)),
        ),
    }
}
fn simple_expansion_to_value(var: &str, state: &VersionState) -> SsaValue {
    if var == "EPOCHSECONDS" {
        return SsaValue::CmdSubst {
            argv: vec![SsaValue::Literal("epochseconds".to_string())],
            redirects: vec![],
            suppressed: false,
        };
    }
    if let Some(v) = positional_to_value(var, state) {
        return v;
    }
    SsaValue::Var(state.lookup(var))
}

/// DD#75 §5: a positional read (`$1..$9` → `${args[0]}..${args[8]}`, `$#` →
/// `${#args}`) resolved against the current function's uniquely-named `args`
/// array. Returns None for any non-positional name (an ordinary variable read).
/// Used by both the expression path and the arithmetic path so `$1`/`$#` mean
/// the same thing inside and outside `$(( ))`.
pub(super) fn positional_to_value(var: &str, state: &VersionState) -> Option<SsaValue> {
    if let Some(idx) = positional_arg_index(var) {
        return Some(SsaValue::Subscript {
            array: state.current_args_name.clone(),
            index: Box::new(SsaValue::Literal(idx.to_string())),
            is_integer_key: true,
            is_integer_value: false,
        });
    }
    if var == "#" {
        return Some(SsaValue::ParamExpansion {
            var: state.lookup(&state.current_args_name),
            operator: "arrlen".to_string(),
            operand: String::new(),
        });
    }
    None
}

/// DD#75 §5: a user-written `args` (in `@args`, `${args[$i]}`, `${#args}`,
/// `$(len args)`, …) names the *current* function's uniquely-named positional
/// array. Any other name passes through unchanged. Outside a function
/// (`current_args_name` empty) `args` is left as-is (the validator rejects a
/// top-level positional/`args` use).
pub(super) fn resolve_args_name(name: &str, state: &VersionState) -> String {
    if name == "args" && !state.current_args_name.is_empty() {
        state.current_args_name.clone()
    } else if let Some(target) = state.collection_slots.get(name) {
        // DD#100: a collection-parameter slot resolves to the collection the
        // caller passed. Substitution is the whole mechanism — reads, writes
        // and taint all then apply to the caller's name.
        target.clone()
    } else {
        name.to_string()
    }
}

/// The NAME a bulk-fill target denotes, resolved through DD#100's collection
/// slots so `read A` / `argv N A` / `matchall … A` / `mapfile A` fill the
/// **caller's** collection. Unresolved, these silently filled a function-local
/// array literally named `A` and the caller's collection was left untouched —
/// which is precisely the out-parameter form Part E exists for.
pub(super) fn fill_target_name(e: &Expr, state: &VersionState) -> String {
    resolve_args_name(&expr_to_string(e), state)
}

/// A single-digit positional name `"1".."9"` → its 0-based `args` index (n-1).
/// `$0` (the program name) is not a positional and is handled elsewhere.
fn positional_arg_index(var: &str) -> Option<usize> {
    let b = var.as_bytes();
    if b.len() == 1 && (b'1'..=b'9').contains(&b[0]) {
        Some((b[0] - b'1') as usize)
    } else {
        None
    }
}

fn parameter_expansion_to_value(pe: &ParameterExpansion, state: &VersionState) -> SsaValue {
    // The disposition subject is a plain var for a scalar `${s:OP}`, or the cell
    // read for `${a[i]:OP}` (DG: a cell behaves like a string variable).
    let source = pe_source_value(pe, state);
    let Some(op) = pe.operator.as_ref() else {
        // DD#91: `${x:!}` — no disposition, just the `!` modifier. Wrap so the
        // SSA taint backstop can see the sanctioned bypass; without a node the
        // backstop sees a bare tainted read and rejects what the CST allowed.
        if pe.taint_off {
            return SsaValue::Trusted {
                source: Box::new(source),
            };
        }
        return source;
    };
    // `:-`/`:?` (default/require) and substring-on-a-cell carry the subject as a
    // value; the remaining operators lower to a plain `ParamExpansion`.
    if let Some(v) = source_carrying_value(pe, op, source, state) {
        return v;
    }
    let resolved = resolve_args_name(&pe.name, state);
    let var = state.lookup(&resolved);
    let operand = pe_operand_string(pe);
    // DD#75 §2: `${#x}` is the length of x dispatched on type — for a
    // collection it is the element/entry count (`arrlen`), not the byte length
    // of a scalar slot (`strlen`, which would read 0). `${#s}` stays strlen.
    let operator = if op == "strlen" && state.collections.contains(&resolved) {
        "arrlen".to_string()
    } else {
        op.clone()
    };
    SsaValue::ParamExpansion {
        var,
        operator,
        operand,
    }
}

/// The `${var:OP}` forms whose subject is a value (scalar var read or cell
/// read): `:-` (DD#65 default), `:?` (DD#77 require), and substring on a cell
/// (`${a[i]:o:l}`, edge 4). `source` is the already-computed subject. Returns
/// `None` for the scalar operators that lower to a plain `ParamExpansion`.
fn source_carrying_value(
    pe: &ParameterExpansion,
    op: &str,
    source: SsaValue,
    state: &VersionState,
) -> Option<SsaValue> {
    match op {
        ":-" => {
            let default = pe_operand_value(pe, state);
            if source_is_int(&source, pe, state) {
                // DD#62 + DG: `:-` triggers on the *identity element* of the
                // subject's type — `""` for strings, `0` for ints (zero stays a
                // meaningful value; `:-` just means "fall back if identity"). So
                // `${int:-d}` is `x == 0 ? d : x`, emitted as a ternary the SSA
                // backends already handle (bug-cjanet-default-on-int-var).
                Some(SsaValue::Ternary {
                    cond: Box::new(SsaValue::ArithBinOp {
                        left: Box::new(source.clone()),
                        op: crate::ssa::ArithOp::Eq,
                        right: Box::new(SsaValue::Expr("0".to_string())),
                    }),
                    true_val: Box::new(default),
                    false_val: Box::new(source),
                })
            } else {
                Some(SsaValue::DefaultValue {
                    source: Box::new(source),
                    default: Box::new(default),
                })
            }
        }
        ":?" => Some(SsaValue::RequireValue {
            source: Box::new(source),
            message: Box::new(pe_operand_value(pe, state)),
        }),
        // DD#84: `${x:~}` / `${a[i]:~}` (elide) — carry the subject; the argv /
        // array-literal builders drop it when empty. Rejected outside an
        // elidable position at validate (SB5046), so it only reaches a list.
        // DD#91 addendum: `!` is a modifier, so `:~` and `:!~` share this arm
        // and differ only in the flag. Bare `:!` is not an elide at all — it
        // has no operator, so it never reaches here.
        ":~" => Some(SsaValue::ElideEmpty {
            source: Box::new(source),
            taint_off: pe.taint_off,
        }),
        // The scalar `${s:o:l}` (no index) keeps the `ParamExpansion` path.
        "substring" if pe.index.is_some() => Some(SsaValue::Substring {
            source: Box::new(source),
            spec: pe_operand_string(pe),
        }),
        _ => None,
    }
}

/// True if the `${…:-…}` subject is an int-typed value — a scalar `int`
/// variable, or an int-valued array/map cell (`is_integer_value`). For such a
/// subject, `:-` triggers on `0` (the additive identity) rather than string
/// emptiness — see the `:-` arm.
fn source_is_int(source: &SsaValue, pe: &ParameterExpansion, state: &VersionState) -> bool {
    match source {
        SsaValue::Var(_) => {
            pe.index.is_none() && state.type_map.get(&pe.name) == Some(&crate::ir::VarType::Int)
        }
        SsaValue::Subscript {
            is_integer_value, ..
        } => *is_integer_value,
        _ => false,
    }
}

/// The `${var:OP}` operand as a full interpolating value (the `:-`/`:?` body).
fn pe_operand_value(pe: &ParameterExpansion, state: &VersionState) -> SsaValue {
    pe.operand
        .as_ref()
        .map(|e| expr_to_value(e, state))
        .unwrap_or_else(|| SsaValue::Literal(String::new()))
}

/// The `${var:OP}` operand stringified (substring `off:len`, glob operand, …).
fn pe_operand_string(pe: &ParameterExpansion) -> String {
    pe.operand
        .as_ref()
        .map(|e| expr_to_string(e))
        .unwrap_or_default()
}

/// The subject of a `${…:OP}` disposition: a scalar var read, or the cell read
/// when the expansion carries an array/map index (`${a[i]:OP}`). A positional
/// name (`${N:OP}`, `${N}`) reads the current function's `args[N-1]` cell — NOT
/// a scalar variable named "N" (which would always be empty, so every `${N:-}`
/// defaulted and every `${N:~}` elided regardless of the argument).
fn pe_source_value(pe: &ParameterExpansion, state: &VersionState) -> SsaValue {
    match &pe.index {
        Some(index) => subscript_to_value(&pe.name, index, state),
        None => positional_to_value(&pe.name, state)
            .unwrap_or_else(|| SsaValue::Var(state.lookup(&pe.name))),
    }
}

fn assoc_array_to_value(pairs: &[(String, Expr)], state: &VersionState) -> SsaValue {
    let vals: Vec<(String, SsaValue)> = pairs
        .iter()
        .map(|(k, v)| (k.clone(), expr_to_value(v, state)))
        .collect();
    SsaValue::AssocArray(vals)
}

pub(super) fn subscript_to_value(name: &str, index: &Expr, state: &VersionState) -> SsaValue {
    let resolved = resolve_args_name(name, state);
    let is_integer = !state.assoc_arrays.contains(&resolved);
    let is_integer_value = state.type_map.get(&resolved).copied() == Some(crate::ir::VarType::Int);
    SsaValue::Subscript {
        array: resolved,
        index: Box::new(expr_to_value(index, state)),
        is_integer_key: is_integer,
        is_integer_value,
    }
}

fn concat_to_value(parts: &[Expr], state: &VersionState) -> SsaValue {
    let mut vals: Vec<SsaValue> = parts.iter().map(|p| expr_to_value(p, state)).collect();
    if vals.len() == 1 {
        return vals.pop().unwrap();
    }
    SsaValue::Concat(vals)
}

/// `"…"` double-quoted: single-part is the part itself; all-literal parts
/// flatten to a single literal; otherwise build a `Concat`.
fn dq_to_value(parts: &[Expr], state: &VersionState) -> SsaValue {
    if parts.len() == 1 {
        return expr_to_value(&parts[0], state);
    }
    let mut lit_concat = String::new();
    let mut all_literal = true;
    for part in parts {
        match &part.kind {
            ExprKind::Literal(lit) => lit_concat.push_str(lit),
            _ => {
                all_literal = false;
                break;
            }
        }
    }
    if all_literal {
        return SsaValue::Literal(lit_concat);
    }
    let concat_parts: Vec<SsaValue> = parts.iter().map(|p| expr_to_value(p, state)).collect();
    SsaValue::Concat(concat_parts)
}

/// `arr=(…)` array literal: detect two sash-native shorthand patterns
/// (regex filter and single-glob) before falling back to IndexedArray.
fn lower_array_literal(elems: &[Expr], state: &VersionState) -> SsaValue {
    // An array-literal element is an elidable list position like a command
    // argument, so `@a` SPLICES its elements — `(p @a q)` is p,x,y,z, not a
    // 3-element array whose middle entry is the joined string.
    let vals: Vec<SsaValue> = elems.iter().map(|e| arg_to_value(e, state)).collect();
    if let Some(v) = try_array_regex_filter(&vals) {
        return v;
    }
    if let Some(v) = try_array_single_glob(&vals) {
        return v;
    }
    SsaValue::IndexedArray(vals)
}

/// arr=(* =~ 'pat') → sash_glob(".") | grep -E 'pat' shorthand.
fn try_array_regex_filter(vals: &[SsaValue]) -> Option<SsaValue> {
    if vals.len() != 3 {
        return None;
    }
    let (SsaValue::Literal(star), SsaValue::Literal(op), SsaValue::Literal(pat)) =
        (&vals[0], &vals[1], &vals[2])
    else {
        return None;
    };
    if star != "*" || op != "=~" {
        return None;
    }
    Some(SsaValue::ParamExpansion {
        var: VarRef::new(".", 0),
        operator: "glob_regex".to_string(),
        operand: pat.clone(),
    })
}

/// arr=(*.sh) → sash_glob(".", "*.sh") shorthand.
fn try_array_single_glob(vals: &[SsaValue]) -> Option<SsaValue> {
    if vals.len() != 1 {
        return None;
    }
    let SsaValue::Literal(s) = &vals[0] else {
        return None;
    };
    if !s.contains('*') {
        return None;
    }
    Some(SsaValue::ParamExpansion {
        var: VarRef::new(".", 0),
        operator: "glob".to_string(),
        operand: s.clone(),
    })
}

/// Convert an expression to a simple string representation.
pub(super) fn expr_to_string(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(s) => s.clone(),
        ExprKind::SimpleExpansion(var) => format!("${}", var),
        ExprKind::SingleQuoted(s) => format!("'{}'", s),
        ExprKind::DoubleQuoted(parts) => {
            let mut s = String::from("\"");
            for p in parts {
                s.push_str(&expr_to_string(p));
            }
            s.push('"');
            s
        }
        ExprKind::ParameterExpansion(pe) => {
            let mut s = format!("${{{}", pe.name);
            if let Some(op) = &pe.operator {
                s.push_str(op);
            }
            s.push('}');
            s
        }
        ExprKind::CommandSubstitution(_) => "$(...)".to_string(),
        ExprKind::Arithmetic(_) => "$(( ... ))".to_string(),
        ExprKind::Concat(parts) => parts.iter().map(expr_to_string).collect(),
        ExprKind::Array(elems) => {
            let inner: Vec<String> = elems.iter().map(expr_to_string).collect();
            format!("({})", inner.join(" "))
        }
        ExprKind::AssocArray(pairs) => {
            let inner: Vec<String> = pairs
                .iter()
                .map(|(k, v)| format!("[{}]={}", k, expr_to_string(v)))
                .collect();
            format!("({})", inner.join(" "))
        }
        ExprKind::Subscript { name, index } => {
            format!("${{{}[{}]}}", name, expr_to_string(index))
        }
        ExprKind::Test(_) => "[[ ... ]]".to_string(),
    }
}
