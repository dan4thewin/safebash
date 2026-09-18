//! Purity analysis for `$( … )` capture sites.
//!
//! A capture site is **pure** when its output can be collected in a userspace
//! buffer instead of a kernel object — that is, when every byte written to the
//! site's stdout comes from an in-process `sash_write` the runtime can divert.
//! The single disqualifying condition is:
//!
//! > some process forks with *this* capture's stdout as its fd 1, or fd 1 is
//! > rebound underneath the buffer.
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
//! Recursion cannot occur (SB5037 rejects it, because sash inlines functions),
//! but the walk carries a visiting set anyway so that a malformed program
//! cannot hang the compiler: a cycle falls back to "impure", which is always
//! safe.

use crate::ir::{Command, Statement, StmtKind};
use std::collections::{HashMap, HashSet};

/// Can this capture body's stdout be collected in a userspace buffer?
pub(super) fn capture_body_is_pure(
    body: &[Statement],
    function_bodies: &HashMap<String, Vec<Statement>>,
) -> bool {
    let mut visiting = HashSet::new();
    stmts_are_pure(body, function_bodies, &mut visiting)
}

fn stmts_are_pure(
    stmts: &[Statement],
    bodies: &HashMap<String, Vec<Statement>>,
    visiting: &mut HashSet<String>,
) -> bool {
    stmts.iter().all(|s| stmt_is_pure(s, bodies, visiting))
}

fn stmt_is_pure(
    stmt: &Statement,
    bodies: &HashMap<String, Vec<Statement>>,
    visiting: &mut HashSet<String>,
) -> bool {
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
        StmtKind::Command(c) => command_is_pure(c, bodies, visiting),
        _ => stmt
            .child_bodies()
            .iter()
            .all(|b| stmts_are_pure(b, bodies, visiting)),
    }
}

fn command_is_pure(
    cmd: &Command,
    bodies: &HashMap<String, Vec<Statement>>,
    visiting: &mut HashSet<String>,
) -> bool {
    let name = match &cmd.name.kind {
        crate::ir::ExprKind::Literal(s) => s.as_str(),
        // A head this pass cannot read as a literal is a head it cannot prove
        // anything about.
        _ => return false,
    };
    // DD#23: `resolve` rewrites an external head to an absolute path, and
    // nothing else acquires a slash — intrinsics, builtins and user functions
    // keep bare names. So a slash means `fork` + `exec` with our stdout.
    if name.contains('/') {
        return false;
    }
    // Measured (journal 365): this lookup never succeeds in a full sweep —
    // 156 calls reach here and every one falls through to the `true` below.
    // Function inlining runs before this pass, so a capture of `fn emit {
    // /bin/cat f; }` arrives with `/bin/cat` already spliced in and is
    // disqualified by the slash test above, not by walking a body. The walk
    // is kept because it is what makes the disqualification *correct* for
    // any call this pass sees before inlining has flattened it.
    if let Some(body) = bodies.get(name) {
        let owned = name.to_string();
        if !visiting.insert(owned.clone()) {
            return false;
        }
        let pure = stmts_are_pure(body, bodies, visiting);
        visiting.remove(&owned);
        return pure;
    }
    // A builtin or intrinsic: in-process, so its stdout writes funnel through
    // `sash_write` and the buffer catches them.
    true
}
