use crate::diagnostic::Diagnostic;
use crate::rule::Rule;
use tree_sitter::Node;

/// SB5017: Reject bash magic variables that don't exist in sash.
/// These are either meaningless (BASH_*), dropped by design (RANDOM,
/// SECONDS, PIPESTATUS), or require explicit getenv/command calls.
pub struct RejectedVars;

/// Variables rejected outright — no sash equivalent.
const DROPPED: &[&str] = &[
    "BASH_VERSION",
    "BASH_VERSINFO",
    "BASHOPTS",
    "SHELLOPTS",
    "BASH_SUBSHELL",
    "BASHPID",
    "BASH_SOURCE",
    "BASH_LINENO",
    "BASH_COMMAND",
    "BASH_ALIASES",
    "BASH_ARGV",
    "BASH_ARGC",
    "BASH_REMATCH",
    "SECONDS",
    "PIPESTATUS",
    "OLDPWD",
    "SHLVL",
    "_",
    "RANDOM",
    "SRANDOM",
    "EPOCHREALTIME",
];

/// Variables that should be accessed via external commands.
const COERCE_TO_COMMAND: &[&str] = &[
    "HOSTNAME", "UID", "EUID", "GROUPS", "HOSTTYPE", "OSTYPE", "MACHTYPE",
];

/// Variables that should be accessed via $(getenv NAME).
const USE_GETENV: &[&str] = &[
    "HOME",
    "PWD",
    "PATH",
    "USER",
    "LOGNAME",
    "SHELL",
    "TMPDIR",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TERM",
    "DISPLAY",
    "XDG_RUNTIME_DIR",
    "XDG_DATA_HOME",
    "XDG_CONFIG_HOME",
    "XDG_CACHE_HOME",
    "XDG_STATE_HOME",
    "IFS",
    "PPID",
];

impl Rule for RejectedVars {
    fn node_kinds(&self) -> &'static [&'static str] {
        &["simple_expansion", "expansion"]
    }

    fn check(&self, node: Node, source: &[u8]) -> Vec<Diagnostic> {
        let var_name = extract_var_name(node, source);
        if var_name.is_empty() {
            return vec![];
        }

        if DROPPED.contains(&var_name.as_str()) {
            return vec![Diagnostic::error(
                node,
                "SB5017",
                format!("${} does not exist in sash", var_name),
            )];
        }

        if COERCE_TO_COMMAND.contains(&var_name.as_str()) {
            return vec![Diagnostic::error(
                node,
                "SB5017",
                format!(
                    "${} is not available — use an external command instead",
                    var_name
                ),
            )];
        }

        if USE_GETENV.contains(&var_name.as_str()) {
            return vec![Diagnostic::error(
                node,
                "SB5018",
                format!(
                    "${} is an environment variable — use $(getenv {})",
                    var_name, var_name
                ),
            )];
        }

        // $0 rejected — converter rewrites to $(argv 0)
        if var_name == "0" {
            return vec![Diagnostic::error(
                node,
                "SB5017",
                "$0 is not available in sash — use $(argv 0)".to_string(),
            )];
        }

        vec![]
    }
}

fn extract_var_name(node: Node, source: &[u8]) -> String {
    for child in node.children(&mut node.walk()) {
        if child.kind() == "variable_name" || child.kind() == "special_variable_name" {
            return child.utf8_text(source).unwrap_or("").to_string();
        }
    }
    String::new()
}
