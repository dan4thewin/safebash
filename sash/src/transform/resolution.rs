//! Command resolution.
//!
//! TRUST: resolve_pipeline_command uses compile-time PATH resolution (DD#23).
//! No runtime PATH lookup — the resolved absolute path is embedded in the binary.

/// Resolve an external command head to the path the binary will `exec`, and
/// record it for `--manifest`. This is the single funnel every external form
/// passes through — direct commands, pipeline stages, and `$( … )` argv[0] —
/// so recording here cannot miss an external nor leak a builtin/internal
/// sigil: inline builtins (`echo`/`true`/`false`/`:`) and unresolved bare
/// names have no `/` and are skipped; sigils and user functions never reach
/// this function at all.
pub(super) fn resolve_pipeline_command(name: &str, state: &crate::ssa::VersionState) -> String {
    let resolved = resolve_command_head(name);
    if resolved.contains('/') {
        state.external_deps.borrow_mut().insert(resolved.clone());
    }
    resolved
}

fn resolve_command_head(name: &str) -> String {
    if name.contains('/') {
        return name.to_string();
    }
    // Inline builtins handled directly in codegen — no resolution needed
    if matches!(name, "echo" | "true" | "false" | ":") {
        return name.to_string();
    }
    // DD#23: compile-time PATH resolution (shared with the front-end pass).
    crate::resolve::resolve_command_path(name).unwrap_or_else(|| name.to_string())
}
