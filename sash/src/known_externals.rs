/// Known-safe external values — env vars and commands with predictable output shapes.
///
/// When the taint tracker sees a `getenv` or command substitution that matches
/// a known entry, it auto-blesses the result with the associated regex pattern.
/// This eliminates boilerplate bless annotations for common, well-understood externals.
///
/// Patterns are conservative: no spaces, no shell metacharacters, no control chars.
/// The goal is to prevent injection if the value flows to another shell via an
/// external command — not to validate the value's semantic correctness.

/// A known environment variable with its validation pattern.
pub struct KnownEnvVar {
    pub name: &'static str,
    pub pattern: &'static str,
    pub description: &'static str,
}

/// A known command with its expected output pattern.
pub struct KnownCommand {
    pub name: &'static str,
    pub args: &'static [&'static str],  // empty = any args
    pub pattern: &'static str,
    pub description: &'static str,
}

/// Well-known environment variables with safe content patterns.
///
/// Patterns reject: spaces, semicolons, backticks, pipes, $, &, (, ), {, },
/// newlines, quotes, and other shell metacharacters.
pub const KNOWN_ENV_VARS: &[KnownEnvVar] = &[
    KnownEnvVar {
        name: "HOME",
        pattern: "^/[a-zA-Z0-9._/-]+$",
        description: "home directory path",
    },
    KnownEnvVar {
        name: "USER",
        pattern: "^[a-zA-Z0-9._-]+$",
        description: "username",
    },
    KnownEnvVar {
        name: "LOGNAME",
        pattern: "^[a-zA-Z0-9._-]+$",
        description: "login name",
    },
    KnownEnvVar {
        name: "PATH",
        pattern: "^[a-zA-Z0-9._:/-]+$",
        description: "executable search path",
    },
    KnownEnvVar {
        name: "PWD",
        pattern: "^/[a-zA-Z0-9._/-]+$",
        description: "working directory",
    },
    KnownEnvVar {
        name: "OLDPWD",
        pattern: "^/[a-zA-Z0-9._/-]+$",
        description: "previous working directory",
    },
    KnownEnvVar {
        name: "SHELL",
        pattern: "^/[a-zA-Z0-9._/-]+$",
        description: "user's login shell",
    },
    KnownEnvVar {
        name: "HOSTNAME",
        pattern: "^[a-zA-Z0-9._-]+$",
        description: "system hostname",
    },
    KnownEnvVar {
        name: "LANG",
        pattern: "^[a-zA-Z0-9._-]+$",
        description: "locale",
    },
    KnownEnvVar {
        name: "LC_ALL",
        pattern: "^[a-zA-Z0-9._-]*$",
        description: "locale override (may be empty)",
    },
    KnownEnvVar {
        name: "TERM",
        pattern: "^[a-zA-Z0-9._-]+$",
        description: "terminal type",
    },
    KnownEnvVar {
        name: "EDITOR",
        pattern: "^[a-zA-Z0-9._/-]+$",
        description: "default editor",
    },
    KnownEnvVar {
        name: "VISUAL",
        pattern: "^[a-zA-Z0-9._/-]+$",
        description: "default visual editor",
    },
    KnownEnvVar {
        name: "PAGER",
        pattern: "^[a-zA-Z0-9._/-]+$",
        description: "default pager",
    },
    KnownEnvVar {
        name: "TMPDIR",
        pattern: "^/[a-zA-Z0-9._/-]+$",
        description: "temporary directory",
    },
    KnownEnvVar {
        name: "XDG_RUNTIME_DIR",
        pattern: "^/[a-zA-Z0-9._/-]+$",
        description: "XDG runtime directory",
    },
    KnownEnvVar {
        name: "XDG_CONFIG_HOME",
        pattern: "^/[a-zA-Z0-9._/-]+$",
        description: "XDG config directory",
    },
    KnownEnvVar {
        name: "XDG_DATA_HOME",
        pattern: "^/[a-zA-Z0-9._/-]+$",
        description: "XDG data directory",
    },
    KnownEnvVar {
        name: "XDG_CACHE_HOME",
        pattern: "^/[a-zA-Z0-9._/-]+$",
        description: "XDG cache directory",
    },
    KnownEnvVar {
        name: "UID",
        pattern: "^[0-9]+$",
        description: "user ID",
    },
    KnownEnvVar {
        name: "EUID",
        pattern: "^[0-9]+$",
        description: "effective user ID",
    },
    KnownEnvVar {
        name: "SHLVL",
        pattern: "^[0-9]+$",
        description: "shell nesting level",
    },
    KnownEnvVar {
        name: "DISPLAY",
        pattern: "^:[0-9]+(\\.[0-9]+)?$",
        description: "X11 display",
    },
    KnownEnvVar {
        name: "SSH_AUTH_SOCK",
        pattern: "^/[a-zA-Z0-9._/-]+$",
        description: "SSH agent socket",
    },
    KnownEnvVar {
        name: "SSH_CONNECTION",
        pattern: "^[0-9.:a-fA-F ]+$",
        description: "SSH connection info",
    },
    KnownEnvVar {
        name: "MAIL",
        pattern: "^/[a-zA-Z0-9._/-]+$",
        description: "mail spool path",
    },
];

/// Well-known commands with predictable output patterns.
///
/// Empty args slice means "any arguments" — the output shape is always safe.
/// Non-empty args means the pattern only applies with those specific arguments.
pub const KNOWN_COMMANDS: &[KnownCommand] = &[
    KnownCommand {
        name: "hostname",
        args: &[],
        pattern: "^[a-zA-Z0-9._-]+$",
        description: "system hostname",
    },
    KnownCommand {
        name: "whoami",
        args: &[],
        pattern: "^[a-zA-Z0-9._-]+$",
        description: "current username",
    },
    KnownCommand {
        name: "uname",
        args: &["-r"],
        pattern: "^[a-zA-Z0-9._-]+$",
        description: "kernel release",
    },
    KnownCommand {
        name: "uname",
        args: &["-s"],
        pattern: "^[a-zA-Z]+$",
        description: "kernel name",
    },
    KnownCommand {
        name: "uname",
        args: &["-m"],
        pattern: "^[a-zA-Z0-9_]+$",
        description: "machine architecture",
    },
    KnownCommand {
        name: "uname",
        args: &["-n"],
        pattern: "^[a-zA-Z0-9._-]+$",
        description: "network hostname",
    },
    KnownCommand {
        name: "id",
        args: &["-u"],
        pattern: "^[0-9]+$",
        description: "user ID",
    },
    KnownCommand {
        name: "id",
        args: &["-g"],
        pattern: "^[0-9]+$",
        description: "group ID",
    },
    KnownCommand {
        name: "id",
        args: &["-un"],
        pattern: "^[a-zA-Z0-9._-]+$",
        description: "username",
    },
    KnownCommand {
        name: "id",
        args: &["-gn"],
        pattern: "^[a-zA-Z0-9._-]+$",
        description: "group name",
    },
    KnownCommand {
        name: "date",
        args: &["+%Y-%m-%d"],
        pattern: "^[0-9]{4}-[0-9]{2}-[0-9]{2}$",
        description: "ISO date",
    },
    KnownCommand {
        name: "date",
        args: &["+%Y%m%d"],
        pattern: "^[0-9]{8}$",
        description: "compact date",
    },
    KnownCommand {
        name: "date",
        args: &["+%s"],
        pattern: "^[0-9]+$",
        description: "epoch seconds",
    },
    KnownCommand {
        name: "date",
        args: &["+%Y-%m-%dT%H:%M:%S"],
        pattern: "^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}$",
        description: "ISO datetime",
    },
    KnownCommand {
        name: "date",
        args: &["+%H:%M:%S"],
        pattern: "^[0-9]{2}:[0-9]{2}:[0-9]{2}$",
        description: "time",
    },
    KnownCommand {
        name: "basename",
        args: &[],
        pattern: "^[a-zA-Z0-9._-]+$",
        description: "filename component",
    },
    KnownCommand {
        name: "dirname",
        args: &[],
        pattern: "^[a-zA-Z0-9._/-]+$",
        description: "directory component",
    },
    KnownCommand {
        name: "pwd",
        args: &[],
        pattern: "^/[a-zA-Z0-9._/-]+$",
        description: "working directory",
    },
    KnownCommand {
        name: "wc",
        args: &["-l"],
        pattern: "^[0-9 ]+$",
        description: "line count",
    },
    KnownCommand {
        name: "wc",
        args: &["-c"],
        pattern: "^[0-9 ]+$",
        description: "byte count",
    },
    KnownCommand {
        name: "wc",
        args: &["-w"],
        pattern: "^[0-9 ]+$",
        description: "word count",
    },
    KnownCommand {
        name: "nproc",
        args: &[],
        pattern: "^[0-9]+$",
        description: "CPU count",
    },
    KnownCommand {
        name: "tty",
        args: &[],
        pattern: "^(/dev/[a-zA-Z0-9/]+|not a tty)$",
        description: "terminal device",
    },
    KnownCommand {
        name: "arch",
        args: &[],
        pattern: "^[a-zA-Z0-9_]+$",
        description: "architecture",
    },
    KnownCommand {
        name: "cat",
        args: &["/etc/hostname"],
        pattern: "^[a-zA-Z0-9._-]+$",
        description: "hostname from file",
    },
];

/// Look up a known env var by name. Returns the validation pattern if found.
pub fn lookup_env_var(name: &str) -> Option<&'static KnownEnvVar> {
    KNOWN_ENV_VARS.iter().find(|e| e.name == name)
}

/// Look up a known command by name and args. Returns the validation pattern if found.
/// Empty args in the table means "any args match".
pub fn lookup_command(name: &str, args: &[String]) -> Option<&'static KnownCommand> {
    KNOWN_COMMANDS.iter().find(|c| {
        c.name == name && (c.args.is_empty() || {
            // Match if command args start with the known args
            args.len() >= c.args.len()
                && c.args.iter().zip(args.iter()).all(|(a, b)| *a == b.as_str())
        })
    })
}
