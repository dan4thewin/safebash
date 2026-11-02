//! DD#127: kernel reads introduce taint; compiler-owned copies do not.

use crate::capture_purity::capture_body_is_pure;
use crate::ir::*;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(crate) struct OutputTaint {
    bodies: HashMap<String, Vec<Statement>>,
    value_fns: HashSet<String>,
    int_returns: HashSet<String>,
}

impl OutputTaint {
    pub fn new(program: &Program) -> Self {
        let mut output = Self::default();
        let mut string_returns = HashSet::new();
        for f in program.function_defs() {
            output
                .bodies
                .entry(f.name.clone())
                .or_default()
                .extend(f.body.iter().cloned());
            if f.return_type == VarType::Int {
                output.value_fns.insert(f.name.clone());
            } else {
                string_returns.insert(f.name.clone());
            }
        }
        // All definitions contribute, as they do to the independent name
        // summaries. One integer definition cannot hide another's stdout.
        output.int_returns = output
            .value_fns
            .difference(&string_returns)
            .cloned()
            .collect();
        output
    }

    pub fn capture(&self, body: &[Statement]) -> bool {
        single_value_command(body)
            .and_then(|c| self.value_source(c))
            .unwrap_or_else(|| !capture_body_is_pure(body, &self.bodies, &self.value_fns))
    }

    fn value_source(&self, c: &Command) -> Option<bool> {
        let name = match &c.name.kind {
            ExprKind::Literal(name) => name.as_str(),
            _ => return Some(true),
        };
        if matches!(name, "getenv" | "arg" | "argv") {
            return Some(true);
        }
        // Value calls return a value, not their stdout. Any input they use
        // is checked separately; stdout effects belong to the outer capture.
        crate::resolve::evaluates_args_in_place(name, &|n| self.int_returns.contains(n))
            .then_some(false)
    }
}
