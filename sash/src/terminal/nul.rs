//! DD135: demand-driven proofs over the bytes actually read in ERR.

use super::{live_blocks, live_insts, live_successors, Facts, Proof, VarType};
use crate::ir::HandlerKind;
use crate::ssa::*;
use std::collections::{BTreeSet, HashMap, HashSet};

type Point = (BlockId, usize);

#[derive(Clone, Copy, PartialEq, Eq)]
struct Cursor {
    point: Point,
    condition: Option<bool>,
    status: Option<bool>,
}

impl Cursor {
    fn predecessor(self, previous: Self) -> Option<Self> {
        Some(Self {
            condition: combine_outcomes(self.condition, previous.condition).ok()?,
            status: self.status,
            ..previous
        })
    }

    fn take_result(&mut self) -> Result<Option<bool>, ()> {
        combine_outcomes(self.condition.take(), self.status.take())
    }

    // A contradiction is an infeasible path, not a proof about its value.
    fn rewind_result(&mut self, inst: &SsaInst) -> Result<Option<bool>, ()> {
        match inst {
            SsaInst::Test(..) => self.take_result(),
            SsaInst::Assign { target, value } if target.name == "?" => {
                let required = self.take_result()?;
                let actual = literal_status(value);
                combine_outcomes(required, actual)?;
                Ok(None)
            }
            SsaInst::Command {
                kind: BuiltinCmd::SetTestFromStatus,
                ..
            } => {
                self.status = self.take_result()?;
                Ok(None)
            }
            SsaInst::Command {
                kind: BuiltinCmd::NegateTestResult,
                ..
            } => {
                self.condition = self.condition.map(|truth| !truth);
                self.status = self.status.map(|truth| !truth);
                Ok(None)
            }
            SsaInst::SourceOrigin(..)
            | SsaInst::LifetimeStart { .. }
            | SsaInst::CollectionPublish { .. }
            | SsaInst::AttestRetag { .. }
            | SsaInst::Command {
                kind: BuiltinCmd::SignalCheckpoint,
                ..
            } => Ok(None),
            SsaInst::Assign { .. }
            | SsaInst::Command { .. }
            | SsaInst::Pipeline { .. }
            | SsaInst::PipelineFinish { .. }
            | SsaInst::SubscriptAssign { .. }
            | SsaInst::ArrayAppend { .. }
            | SsaInst::MapFromArray { .. }
            | SsaInst::ReadLine { .. }
            | SsaInst::ReadArray { .. }
            | SsaInst::SplitValue { .. }
            | SsaInst::ArgvFill { .. }
            | SsaInst::KeysFill { .. }
            | SsaInst::MatchInto { .. }
            | SsaInst::CaptureStart { .. }
            | SsaInst::CaptureEnd { .. }
            | SsaInst::RedirectPush { .. }
            | SsaInst::RedirectPop { .. }
            | SsaInst::Attest { .. }
            | SsaInst::AttestAll { .. } => {
                self.condition = None;
                self.status = None;
                Ok(None)
            }
        }
    }
}

#[derive(Clone, PartialEq)]
enum Form {
    Literal(String),
    Binding(String),
    Known(Proof),
    Concat,
    Default,
    Case(bool),
    Substring(String),
    Parameter(String, String),
    Arithmetic(ArithOp),
    Length,
    IntCell,
    Select(bool, bool),
    ValueAt,
    KeyAt,
    ArgvAt,
    Splat,
    Array,
    Map(Vec<String>),
    Match,
    Replace(bool),
    Ternary,
    Keys,
    MapFromArray(u8),
    // Mutation summaries bound bytes; they do not identify individual cells.
    Store(bool),
    Append,
    Subset,
    Matches,
}

#[derive(Clone, PartialEq)]
struct Value {
    form: Form,
    parts: Vec<Value>,
}

impl Value {
    fn leaf(form: Form) -> Self {
        Self {
            form,
            parts: Vec::new(),
        }
    }

    fn binding(var: &VarRef) -> Self {
        Self::named(&var.name)
    }

    fn named(name: &str) -> Self {
        Self::leaf(Form::Binding(name.to_string()))
    }

    fn apply(form: Form, parts: Vec<Self>) -> Self {
        if form == Form::Concat {
            return Self::concat(parts);
        }
        Self { form, parts }
    }

    fn concat(parts: Vec<Self>) -> Self {
        let mut joined = Vec::new();
        for item in parts.into_iter().flat_map(Self::flatten_concat) {
            Self::append_concat(&mut joined, item);
        }
        match joined.len() {
            0 => Self::leaf(Form::Literal(String::new())),
            1 => joined.pop().unwrap(),
            _ => Self {
                form: Form::Concat,
                parts: joined,
            },
        }
    }

    fn flatten_concat(self) -> Vec<Self> {
        if self.form == Form::Concat {
            self.parts
        } else {
            vec![self]
        }
    }

    fn append_concat(joined: &mut Vec<Self>, item: Self) {
        match item.form {
            Form::Literal(text) if text.is_empty() => {}
            Form::Literal(text) => match joined.last_mut() {
                Some(Value {
                    form: Form::Literal(last),
                    ..
                }) => last.push_str(&text),
                _ => joined.push(Self::leaf(Form::Literal(text))),
            },
            _ => joined.push(item),
        }
    }

    fn exact(&self) -> bool {
        !matches!(
            self.form,
            Form::Known(_) | Form::Store(_) | Form::Append | Form::Subset | Form::Matches
        ) && self.parts.iter().all(Self::exact)
    }

    fn reads(&self, name: &str) -> bool {
        matches!(&self.form, Form::Binding(n) if n == name)
            || self.parts.iter().any(|part| part.reads(name))
    }

    fn replace(self, name: &str, replacement: &Self) -> Self {
        if matches!(&self.form, Form::Binding(n) if n == name) {
            return replacement.clone();
        }
        Self::apply(
            self.form,
            self.parts
                .into_iter()
                .map(|part| part.replace(name, replacement))
                .collect(),
        )
    }

    fn byte_parts(&self) -> Option<Vec<&Self>> {
        match self.form {
            Form::Concat
            | Form::Default
            | Form::Array
            | Form::Map(_)
            | Form::Splat
            | Form::Append => Some(self.parts.iter().collect()),
            Form::Case(_)
            | Form::Substring(_)
            | Form::Match
            | Form::Select(..)
            | Form::ValueAt
            | Form::Subset
            | Form::Matches
            | Form::MapFromArray(_) => Some(vec![&self.parts[0]]),
            Form::Store(_) => Some(vec![&self.parts[0], &self.parts[1]]),
            Form::Replace(_) => Some(vec![&self.parts[0], &self.parts[2]]),
            Form::Ternary => Some(vec![&self.parts[1], &self.parts[2]]),
            _ => None,
        }
    }
}

#[derive(Clone, PartialEq)]
enum Goal {
    Nul(Value),
    Equal(Value, Value),
}

impl Goal {
    fn exact(&self) -> bool {
        match self {
            Self::Nul(value) => value.exact(),
            Self::Equal(left, right) => left.exact() && right.exact(),
        }
    }

    fn reads(&self, name: &str) -> bool {
        match self {
            Self::Nul(value) => value.reads(name),
            Self::Equal(left, right) => left.reads(name) || right.reads(name),
        }
    }

    fn replace(self, name: &str, value: &Value) -> Self {
        match self {
            Self::Nul(old) => Self::Nul(old.replace(name, value)),
            Self::Equal(left, right) => {
                Self::Equal(left.replace(name, value), right.replace(name, value))
            }
        }
    }
}

pub(super) struct Prover<'a, 'p> {
    program: &'p SsaProgram,
    facts: &'a Facts<'p>,
    root: BlockId,
    predecessors: HashMap<BlockId, Vec<Cursor>>,
    writes: HashSet<&'p str>,
    integers: HashSet<&'a str>,
    stable: HashMap<String, Proof>,
}

impl<'a, 'p> Prover<'a, 'p> {
    pub(super) fn new(program: &'p SsaProgram, facts: &'a Facts<'p>) -> Self {
        let handler = &program.handlers[&HandlerKind::Err];
        let live = live_blocks(program, handler.block);
        let writes = live
            .iter()
            .flat_map(|id| live_insts(program.block(*id)))
            .filter_map(instruction_write)
            .collect::<HashSet<_>>();
        let stable = handler
            .inputs
            .iter()
            .filter(|(name, _)| !writes.contains(name.as_str()))
            .map(|(name, var)| (name.clone(), facts.variable(var)))
            .collect();
        let integers = facts
            .definitions
            .keys()
            .filter(|var| var.var_type == VarType::Int && !facts.collections.contains(&var.name))
            .map(|var| var.name.as_str())
            .collect();
        Self {
            program,
            facts,
            root: handler.block,
            predecessors: predecessors(program, &live),
            writes,
            integers,
            stable,
        }
    }

    pub(super) fn proves(&self, point: Point, source: &SsaValue) -> bool {
        let Some(value) = self.value(source) else {
            return false;
        };
        // Version numbers are not snapshots: both backends reuse a binding's
        // storage. Existing facts are a fast path only when no ERR write can
        // replace the value those facts describe.
        if !self.facts.value(source).may_nul && !self.writes.iter().any(|name| value.reads(name)) {
            return true;
        }
        self.solve(
            Cursor {
                point,
                condition: None,
                status: None,
            },
            Goal::Nul(value),
            &mut Vec::new(),
        )
    }

    fn solve(&self, cursor: Cursor, goal: Goal, active: &mut Vec<(Cursor, Goal)>) -> bool {
        if self.obvious(cursor, &goal) {
            return true;
        }
        if let Some((_, previous)) = active.iter().rev().find(|(old, _)| *old == cursor) {
            // An unchanged loop obligation is inductive, but every independent
            // entry path is still checked. A changing expression cannot justify
            // itself by repeatedly unwinding the loop.
            return goal.exact() && *previous == goal;
        }
        if self.parts_prove(cursor, &goal, active) {
            return true;
        }
        active.push((cursor, goal.clone()));
        let result = self.rewind(cursor, goal, active);
        active.pop();
        result
    }

    fn rewind(&self, mut cursor: Cursor, mut goal: Goal, active: &mut Vec<(Cursor, Goal)>) -> bool {
        while cursor.point.1 > 0 {
            cursor.point.1 -= 1;
            let inst = &self.program.block(cursor.point.0).insts[cursor.point.1];
            if let Some(result) = self.rewind_inst(&mut cursor, &mut goal, inst, active) {
                return result;
            }
        }
        if cursor.point.0 == self.root {
            return self.obvious(cursor, &goal);
        }
        let Some(predecessors) = self.predecessors.get(&cursor.point.0) else {
            return false;
        };
        predecessors
            .iter()
            .filter_map(|previous| cursor.predecessor(*previous))
            .all(|previous| self.solve(previous, goal.clone(), active))
    }

    fn rewind_inst(
        &self,
        cursor: &mut Cursor,
        goal: &mut Goal,
        inst: &SsaInst,
        active: &mut Vec<(Cursor, Goal)>,
    ) -> Option<bool> {
        let Ok(truth) = cursor.rewind_result(inst) else {
            return Some(true);
        };
        if self.witness(*cursor, inst, goal, truth) {
            return Some(true);
        }
        let name = instruction_write(inst).filter(|name| goal.reads(name))?;
        let Some(value) = self.written_value(inst) else {
            return Some(false);
        };
        *goal = goal.clone().replace(name, &value);
        (self.obvious(*cursor, goal) || self.parts_prove(*cursor, goal, active)).then_some(true)
    }

    fn obvious(&self, cursor: Cursor, goal: &Goal) -> bool {
        match goal {
            Goal::Equal(left, right) => left.exact() && left == right,
            Goal::Nul(value) => self.safe(value, cursor.point == (self.root, 0)),
        }
    }

    fn parts_prove(&self, cursor: Cursor, goal: &Goal, active: &mut Vec<(Cursor, Goal)>) -> bool {
        let Goal::Nul(value) = goal else {
            return false;
        };
        value.byte_parts().is_some_and(|parts| {
            parts
                .into_iter()
                .all(|part| self.solve(cursor, Goal::Nul(part.clone()), active))
        })
    }

    fn binding_proof(&self, name: &str, entry: bool) -> Proof {
        if let Some(var) = self.program.handlers[&HandlerKind::Err]
            .inputs
            .get(name)
            .filter(|_| entry)
        {
            return self.facts.variable(var);
        }
        self.stable
            .get(name)
            .cloned()
            .unwrap_or_else(Proof::unknown)
    }

    fn safe(&self, value: &Value, entry: bool) -> bool {
        match &value.form {
            Form::Literal(text) => !text.contains('\0'),
            Form::Binding(name) => {
                self.integers.contains(name.as_str()) || !self.binding_proof(name, entry).may_nul
            }
            Form::Known(proof) => !proof.may_nul,
            Form::Arithmetic(_) | Form::Length | Form::IntCell | Form::ArgvAt => true,
            Form::KeyAt | Form::Keys => self.keys_safe(&value.parts[0], entry),
            Form::Parameter(op, word) => self.parameter_safe(op, word, &value.parts[0], entry),
            _ => value
                .byte_parts()
                .is_some_and(|parts| parts.into_iter().all(|part| self.safe(part, entry))),
        }
    }

    fn parameter_safe(&self, op: &str, word: &str, source: &Value, entry: bool) -> bool {
        matches!(op, "strlen" | "arrlen") || (!word.contains('\0') && self.safe(source, entry))
    }

    fn keys_safe(&self, value: &Value, entry: bool) -> bool {
        match &value.form {
            Form::Binding(name) => !self.binding_proof(name, entry).keys_may_nul,
            Form::Known(proof) => !proof.keys_may_nul,
            Form::Map(keys) => keys.iter().all(|key| !key.contains('\0')),
            Form::MapFromArray(0) => {
                self.safe(&value.parts[0], entry) && self.safe(&value.parts[1], entry)
            }
            Form::MapFromArray(_) => self.safe(&value.parts[0], entry),
            Form::Store(false) => {
                self.keys_safe(&value.parts[0], entry) && self.safe(&value.parts[2], entry)
            }
            Form::Subset => self.keys_safe(&value.parts[0], entry),
            _ => false,
        }
    }

    fn witness(&self, cursor: Cursor, inst: &SsaInst, goal: &Goal, truth: Option<bool>) -> bool {
        let (Goal::Nul(value), Some(truth)) = (goal, truth) else {
            return false;
        };
        let SsaInst::Test(test, _) = inst else {
            return false;
        };
        self.test_proves(test, truth, cursor, value)
    }

    fn test_proves(&self, test: &SsaTest, truth: bool, cursor: Cursor, value: &Value) -> bool {
        match test {
            SsaTest::Unary { op, operand }
                if (op == "0?" && !truth) || (truth && nul_free_test(op)) =>
            {
                self.same(cursor, value, operand)
            }
            SsaTest::Binary { op, left, right }
                if truth && matches!(op.as_str(), "-nt" | "-ot" | "-ef") =>
            {
                self.same_operand(cursor, value, left, right)
            }
            SsaTest::Not(inner) => self.test_proves(inner, !truth, cursor, value),
            SsaTest::And(left, right) | SsaTest::Or(left, right) => {
                let either = matches!(test, SsaTest::And(..)) == truth;
                self.junction_proves((left, right), truth, either, cursor, value)
            }
            SsaTest::Unary { .. } | SsaTest::Binary { .. } => false,
        }
    }

    fn junction_proves(
        &self,
        (left, right): (&SsaTest, &SsaTest),
        truth: bool,
        either: bool,
        cursor: Cursor,
        value: &Value,
    ) -> bool {
        let left = self.test_proves(left, truth, cursor, value);
        let right = self.test_proves(right, truth, cursor, value);
        if either {
            left || right
        } else {
            left && right
        }
    }

    fn same_operand(
        &self,
        cursor: Cursor,
        value: &Value,
        left: &SsaValue,
        right: &SsaTestRhs,
    ) -> bool {
        self.same(cursor, value, left)
            || matches!(right, SsaTestRhs::Value(right) if self.same(cursor, value, right))
    }

    fn same(&self, cursor: Cursor, value: &Value, tested: &SsaValue) -> bool {
        self.value(tested).is_some_and(|tested| {
            self.solve(
                Cursor {
                    condition: None,
                    status: None,
                    ..cursor
                },
                Goal::Equal(value.clone(), tested),
                &mut Vec::new(),
            )
        })
    }

    /// Total over SSA instructions; do not add a wildcard.
    fn written_value(&self, inst: &SsaInst) -> Option<Value> {
        match inst {
            SsaInst::LifetimeStart { .. } => {
                Some(Value::leaf(Form::Known(self.facts.instruction(inst))))
            }
            SsaInst::CollectionPublish { source, .. } => Some(Value::binding(source)),
            SsaInst::Assign { value, .. } => self.value(value),
            SsaInst::ReadArray { .. } | SsaInst::ArgvFill { .. } | SsaInst::SplitValue { .. } => {
                Some(Value::leaf(Form::Known(Proof {
                    keys_may_nul: true,
                    ..Proof::c_string()
                })))
            }
            SsaInst::KeysFill { source, .. } => {
                Some(Value::apply(Form::Keys, vec![Value::named(source)]))
            }
            SsaInst::MapFromArray {
                source, key, mode, ..
            } => Some(Value::apply(
                Form::MapFromArray(*mode),
                vec![Value::binding(source), self.value(key)?],
            )),
            SsaInst::ReadLine { .. } | SsaInst::CaptureEnd { .. } | SsaInst::Pipeline { .. } => {
                Some(Value::leaf(Form::Known(Proof::unknown())))
            }
            SsaInst::SubscriptAssign {
                array,
                index,
                value,
                is_integer_key,
            } => Some(Value::apply(
                Form::Store(*is_integer_key),
                vec![Value::named(array), self.value(value)?, self.value(index)?],
            )),
            SsaInst::ArrayAppend { array, value } => Some(Value::apply(
                Form::Append,
                vec![Value::named(array), self.value(value)?],
            )),
            SsaInst::MatchInto { subject, .. } => {
                Some(Value::apply(Form::Matches, vec![self.value(subject)?]))
            }
            SsaInst::Command {
                kind: kind @ (BuiltinCmd::TableDelete | BuiltinCmd::TableDeleteIdx),
                args,
                ..
            } => {
                let [table, _] = kind.expect_ssa_args(args);
                Some(Value::apply(
                    Form::Subset,
                    vec![Value::binding(kind.expect_var_arg(table))],
                ))
            }
            SsaInst::Test(..)
            | SsaInst::Command { .. }
            | SsaInst::PipelineFinish { .. }
            | SsaInst::CaptureStart { .. }
            | SsaInst::RedirectPush { .. }
            | SsaInst::RedirectPop { .. }
            | SsaInst::Attest { .. }
            | SsaInst::AttestRetag { .. }
            | SsaInst::AttestAll { .. }
            | SsaInst::SourceOrigin(..) => None,
        }
    }

    /// Total over the SSA value language; do not add a wildcard.
    fn value(&self, source: &SsaValue) -> Option<Value> {
        let form = match source {
            SsaValue::Literal(text) | SsaValue::Expr(text) => {
                return Some(Value::leaf(Form::Literal(text.clone())));
            }
            SsaValue::Var(var) => return Some(Value::binding(var)),
            SsaValue::Trusted { source }
            | SsaValue::ElideEmpty { source, .. }
            | SsaValue::RequireValue { source, .. } => return self.value(source),
            SsaValue::ParamExpansion {
                var,
                operator,
                operand,
            } => {
                return Some(Value::apply(
                    Form::Parameter(operator.clone(), operand.clone()),
                    vec![Value::binding(var)],
                ));
            }
            SsaValue::ArithExpr { var, op, operand } => {
                return Some(Value::apply(
                    Form::Arithmetic(*op),
                    vec![
                        Value::binding(var),
                        Value::leaf(Form::Literal(operand.to_string())),
                    ],
                ));
            }
            SsaValue::Subscript {
                array,
                index,
                is_integer_key,
                is_integer_value,
            } => {
                return Some(Value::apply(
                    Form::Select(*is_integer_key, *is_integer_value),
                    vec![Value::binding(array), self.value(index)?],
                ));
            }
            SsaValue::TableValue { array, index } | SsaValue::TableKey { array, index } => {
                return Some(Value::apply(
                    if matches!(source, SsaValue::TableKey { .. }) {
                        Form::KeyAt
                    } else {
                        Form::ValueAt
                    },
                    vec![Value::binding(array), Value::binding(index)],
                ));
            }
            SsaValue::ArgvIndex(index) => {
                return Some(Value::apply(Form::ArgvAt, vec![Value::binding(index)]));
            }
            SsaValue::ArraySplat { array, disposition } => {
                let mut parts = vec![Value::binding(array)];
                if let Some(d) = disposition.as_ref().filter(|d| d.has_word && !d.abort) {
                    parts.push(self.value(&d.word)?);
                }
                return Some(Value::apply(Form::Splat, parts));
            }
            SsaValue::CmdSubst { .. } => {
                return Some(Value::leaf(Form::Known(self.facts.value(source))));
            }
            SsaValue::FuncCall { .. }
            | SsaValue::IntFuncCall { .. }
            | SsaValue::CmdSubstBody { .. }
            | SsaValue::SlurpFile(_) => return None,
            SsaValue::Concat(_) => Form::Concat,
            SsaValue::ArithBinOp { op, .. } => Form::Arithmetic(*op),
            SsaValue::RegexReplace { global, .. } => Form::Replace(*global),
            SsaValue::RegexMatch { .. } => Form::Match,
            SsaValue::CaseConvert { upper, .. } => Form::Case(*upper),
            SsaValue::StrLen { .. } => Form::Length,
            SsaValue::DefaultValue { .. } => Form::Default,
            SsaValue::Substring { spec, .. } => Form::Substring(spec.clone()),
            SsaValue::AssocArray(parts) => {
                Form::Map(parts.iter().map(|(key, _)| key.clone()).collect())
            }
            SsaValue::IndexedArray(_) => Form::Array,
            SsaValue::IntCell(_) => Form::IntCell,
            SsaValue::Ternary { .. } => Form::Ternary,
        };
        let parts = source
            .child_values()
            .into_iter()
            .map(|part| self.value(part))
            .collect::<Option<Vec<_>>>()?;
        Some(Value::apply(form, parts))
    }
}

fn combine_outcomes(left: Option<bool>, right: Option<bool>) -> Result<Option<bool>, ()> {
    if matches!((left, right), (Some(a), Some(b)) if a != b) {
        Err(())
    } else {
        Ok(left.or(right))
    }
}

fn literal_status(value: &SsaValue) -> Option<bool> {
    let (SsaValue::Literal(text) | SsaValue::Expr(text)) = value else {
        return None;
    };
    text.parse::<i64>().ok().map(|status| status == 0)
}

fn predecessors(program: &SsaProgram, live: &BTreeSet<BlockId>) -> HashMap<BlockId, Vec<Cursor>> {
    let mut predecessors: HashMap<BlockId, Vec<Cursor>> = HashMap::new();
    for id in live {
        let block = program.block(*id);
        for (next, truth) in predecessor_edges(block) {
            predecessors.entry(next).or_default().push(Cursor {
                point: (*id, live_insts(block).len()),
                condition: truth,
                status: None,
            });
        }
    }
    predecessors
}

fn predecessor_edges(block: &BasicBlock) -> Vec<(BlockId, Option<bool>)> {
    live_successors(block)
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .flat_map(|next| {
            edge_outcomes(&block.terminator, next)
                .into_iter()
                .map(move |truth| (next, truth))
        })
        .collect()
}

fn edge_outcomes(terminator: &Terminator, next: BlockId) -> Vec<Option<bool>> {
    let (then_block, else_block) = match terminator {
        Terminator::Branch {
            true_block: then_block,
            false_block: else_block,
            ..
        }
        | Terminator::Loop {
            body_block: then_block,
            exit_block: else_block,
            ..
        } => (*then_block, *else_block),
        Terminator::Goto(_) | Terminator::Fork { .. } => return vec![None],
        Terminator::Return | Terminator::ChildExit => return Vec::new(),
    };
    [(then_block, true), (else_block, false)]
        .into_iter()
        .filter(|(block, _)| *block == next)
        .map(|(_, truth)| Some(truth))
        .collect()
}

fn nul_free_test(op: &str) -> bool {
    matches!(op, "i?" | "p?" | "g?") || crate::codegen::file_test_op_code(op).is_some()
}
