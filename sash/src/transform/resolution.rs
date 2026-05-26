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
    // DD#23: compile-time PATH resolution
    resolve_command_path(name).unwrap_or_else(|| name.to_string())
}

/// DD#23: Search PATH for a command, return absolute path.
/// Respects --sysroot for cross-compilation.
pub(super) fn resolve_command_path(name: &str) -> Option<String> {
    let sysroot = crate::get_sysroot();
    let path_var = std::env::var("PATH").unwrap_or_default();
    for dir in path_var.split(':') {
        let target_path = format!("{}/{}", dir, name);
        let exists = if let Some(ref sr) = sysroot {
            crate::sysroot_file_exists(sr, &target_path)
        } else {
            std::path::Path::new(&target_path).is_file()
        };
        if exists {
            return Some(target_path);
        }
    }
    None
}
