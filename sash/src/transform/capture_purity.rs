//! Purity analysis for `$( … )` capture sites.
//!
//! A capture site is **pure** when its output can be collected in a userspace
//! buffer instead of a kernel object — that is, when every byte written to the
//! site's stdout comes from an in-process `sash_write` the runtime can divert.
//! The single disqualifying condition is:
//!
//! > some process forks with *this* capture's stdout as its fd 1, an `exec`
//! > replaces this one, or fd 1 is rebound underneath the buffer.
//!
//! The distinction that matters, and the one that makes this worth doing: a
//! body that merely *wraps* an external (`fn emit { /bin/cat f; }`) is
//! disqualified, because the child inherits our stdout. A body that *consumes*
//! external output and yields its value with `echo` (`fn wrap { str t=$(/bin/…);
//! echo "got-$t"; }`) is **pure** — the inner `$( )` hands its child a pipe of
//! its own, so nothing but our own `echo` ever reaches our stdout. Nested
//! captures are therefore not disqualifying in either direction; the runtime's
//! capture stack sequences them.
//!
//! A body's **value calls** run inside it too: `$(h $(( (g) )))` inlines the
//! `-> int` function `g` right here, before `h` runs (DD#58 Option C; the one
//! classification is [`crate::resolve::walk_value_calls`]). So a value call's
//! body is judged exactly like a bare-called function's: `fn g -> int {
//! /bin/echo G; … }` forks with this capture's stdout, and a buffered site
//! would let `G` escape to the real fd 1 while the capture came back without it.
//!
//! Recursion cannot occur (SB5037 rejects it, because sash inlines functions),
//! but the walk carries a visiting set anyway so that a malformed program
//! cannot hang the compiler: a cycle falls back to "impure", which is always
//! safe.

use crate::ir::{Command, Statement, StmtKind};
use crate::ssa::BuiltinCmd;
use std::collections::{HashMap, HashSet};

/// Can this capture body's stdout be collected in a userspace buffer?
/// `value_fns` names the `-> int` functions, whose value calls run inline.
pub(super) fn capture_body_is_pure(
    body: &[Statement],
    function_bodies: &HashMap<String, Vec<Statement>>,
    value_fns: &HashSet<String>,
) -> bool {
    Purity {
        bodies: function_bodies,
        value_fns,
        visiting: HashSet::new(),
    }
    .stmts(body)
}

/// One purity walk: the program's function tables, and the functions whose
/// bodies are being walked.
struct Purity<'a> {
    bodies: &'a HashMap<String, Vec<Statement>>,
    value_fns: &'a HashSet<String>,
    visiting: HashSet<String>,
}

impl Purity<'_> {
    fn stmts(&mut self, stmts: &[Statement]) -> bool {
        stmts.iter().all(|s| self.stmt(s))
    }

    fn stmt(&mut self, stmt: &Statement) -> bool {
        self.shape_is_pure(stmt) && self.value_calls_are_pure(stmt)
    }

    fn shape_is_pure(&mut self, stmt: &Statement) -> bool {
        match &stmt.kind {
            // Every stage of a pipeline runs in a forked child, and the terminal
            // stage inherits this capture's stdout. (A one-stage `! cmd` pipeline
            // is swept up here too — deliberately conservative, and cheap to
            // relax later if it ever shows up in a hot path.)
            StmtKind::Pipeline(_) => false,
            // A redirect may rebind fd 1 by number. The buffer is keyed on "fd 1"
            // at the `sash_write` boundary and cannot see a `dup2` underneath it,
            // so `echo hidden > f` inside a buffered body would be swallowed and
            // the file left empty. Disqualify rather than guess which redirects
            // are safe.
            StmtKind::Redirected(_) => false,
            StmtKind::Command(c) => self.command_is_pure(c),
            _ => stmt.structural_bodies().iter().all(|b| self.stmts(b)),
        }
    }

    /// The value calls `stmt` evaluates run their bodies where it stands —
    /// inside this capture.
    fn value_calls_are_pure(&mut self, stmt: &Statement) -> bool {
        let value_fns = self.value_fns;
        let mut called = Vec::new();
        crate::resolve::walk_value_calls(stmt, &|name| value_fns.contains(name), &mut |name| {
            called.push(name.to_string())
        });
        called.iter().all(|name| self.body_is_pure(name))
    }

    fn command_is_pure(&mut self, cmd: &Command) -> bool {
        let name = match &cmd.name.kind {
            crate::ir::ExprKind::Literal(s) => s.as_str(),
            // Parsed/resolved heads are Literal (DD#23). A broader API input
            // cannot be proved safe for buffering; declining is conservative.
            _ => return false,
        };
        // DD#23: `resolve` rewrites an external head to an absolute path, and
        // nothing else acquires a slash — intrinsics, builtins and user
        // functions keep bare names. So a slash means `fork` + `exec` with our
        // stdout. The `exec` builtin is the one bare name that does the same
        // without the fork: `exec CMD` replaces this process with CMD, which
        // writes fd 1 itself, so on a buffered site its bytes went to the real
        // stdout, past the capture (cap.35, cap.36).
        if name.contains('/') || BuiltinCmd::from_name(name) == BuiltinCmd::Exec {
            return false;
        }
        // This lookup succeeds only for a capture whose body *bare-calls*
        // another user function — `$(outer)` where `fn outer { inner }`. It
        // cannot fire for the captured function's own body, because a capture
        // of `fn emit { /bin/cat f; }` is disqualified by the slash test above
        // before any walk. But the capture path hands this pass the callee's
        // body **unlowered**, before inlining has flattened the calls inside
        // it, so the walk is what makes the disqualification correct for the
        // nested case: without it `fn inner { /bin/echo hi }` would fall
        // through to the `true` below, the site would be judged pure, and the
        // child's output would leak to the real fd 1 while the capture came
        // back empty.
        //
        // Journal 365 measured 156 calls reaching here with none succeeding
        // and recorded that as "this lookup never succeeds". That was a true
        // statement about the suite and a misleading one about the code — the
        // shape simply had no spec. Specs cap.19-cap.21 cover it now, cap.20
        // being the one that fails if this walk is removed.
        if !self.bodies.contains_key(name) {
            // A builtin or intrinsic: its stdout writes funnel through
            // `sash_write`.
            return true;
        }
        self.body_is_pure(name)
    }

    /// Whether user function `name`'s body is pure. A cycle is impure, and so
    /// is a body not yet recorded: a value call reaches here by name alone, and
    /// its definition is missing only from a function template lowered before
    /// it (never run — calls inline) or from a forward call SB5041 rejects.
    fn body_is_pure(&mut self, name: &str) -> bool {
        let bodies = self.bodies;
        let Some(body) = bodies.get(name) else {
            return false;
        };
        if !self.visiting.insert(name.to_string()) {
            return false;
        }
        let pure = self.stmts(body);
        self.visiting.remove(name);
        pure
    }
}

#[cfg(test)]
mod api_contract_tests {
    use super::*;
    use crate::ir::{Expr, ExprKind, Span};

    fn call(kind: ExprKind) -> Statement {
        Statement {
            kind: StmtKind::Command(Command {
                name: Expr {
                    kind,
                    span: Span::default(),
                },
                args: vec![],
            }),
            span: Span::default(),
        }
    }

    #[test]
    fn nonliteral_heads_cannot_be_proved_pure() {
        for kind in [
            ExprKind::SimpleExpansion("command".into()),
            ExprKind::SingleQuoted("echo".into()),
            ExprKind::DoubleQuoted(vec![Expr {
                kind: ExprKind::Literal("echo".into()),
                span: Span::default(),
            }]),
        ] {
            assert!(!capture_body_is_pure(
                &[call(kind)],
                &HashMap::new(),
                &HashSet::new()
            ));
        }
    }

    #[test]
    fn recursive_function_graphs_are_impure() {
        for callee in ["outer", "inner"] {
            let mut bodies = HashMap::new();
            bodies.insert("outer".into(), vec![call(ExprKind::Literal(callee.into()))]);
            if callee == "inner" {
                bodies.insert(
                    "inner".into(),
                    vec![call(ExprKind::Literal("outer".into()))],
                );
            }
            assert!(!capture_body_is_pure(
                &[call(ExprKind::Literal("outer".into()))],
                &bodies,
                &HashSet::new()
            ));
        }
    }

    /// `(len …)` / `(strlen …)` take command words, not arithmetic, so a bare
    /// `count` there is a name and cannot disqualify the buffer; its sibling
    /// `(count)` is a value call of a body that runs an external. Program
    /// output cannot tell the two apart — the memfd path captures correctly
    /// too — so only the classification shows the prune.
    #[test]
    fn intrinsic_operands_are_names_not_value_calls() {
        let src = "fn count -> int { /bin/echo ext; return 7 }
fn call { int n=$(( (count) )) }
fn name { int n=$(( (strlen count) )) }";
        let (program, _) = crate::parse::parse(src).unwrap_or_else(|d| panic!("{d:?}"));
        let bodies: HashMap<String, Vec<Statement>> = program
            .function_defs()
            .map(|f| (f.name.clone(), f.body.clone()))
            .collect();
        let value_fns = HashSet::from(["count".to_string()]);
        assert!(!capture_body_is_pure(&bodies["call"], &bodies, &value_fns));
        assert!(capture_body_is_pure(&bodies["name"], &bodies, &value_fns));
    }
}
