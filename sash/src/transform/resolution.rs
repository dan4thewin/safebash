//! Command resolution.
//!
//! TRUST: resolve_pipeline_command uses compile-time PATH resolution (DD#23).
//! No runtime PATH lookup — the resolved absolute path is embedded in the binary.

pub(super) fn resolve_pipeline_command(name: &str) -> String {
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
