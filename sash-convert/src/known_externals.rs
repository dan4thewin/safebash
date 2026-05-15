/// Known-safe external values — env vars and commands with predictable output shapes.
///
/// When the converter sees a command substitution that matches a known entry,
/// it emits a `bless` line with the associated regex pattern.
/// For unknown externals, it emits a `# BLESS:` hint comment.

/// A known environment variable with its validation pattern.
pub struct KnownEnvVar {
    pub name: &'static str,
    pub pattern: &'static str,
    pub description: &'static str,
}

/// A known command with its expected output pattern.
pub struct KnownCommand {
    pub name: &'static str,
    pub args: &'static [&'static str],
    pub pattern: &'static str,
    pub description: &'static str,
}

pub const KNOWN_ENV_VARS: &[KnownEnvVar] = &[
    KnownEnvVar {
        name: "HOME",
        pattern: r"^/[\w./\-]+$",
        description: "home directory path",
    },
    KnownEnvVar {
        name: "USER",
        pattern: r"^[\w.\-]+$",
        description: "username",
    },
    KnownEnvVar {
        name: "LOGNAME",
        pattern: r"^[\w.\-]+$",
        description: "login name",
    },
    KnownEnvVar {
        name: "PATH",
        pattern: r"^[\w.:/\-]+$",
        description: "executable search path",
    },
    KnownEnvVar {
        name: "PWD",
        pattern: r"^/[\w./\-]+$",
        description: "working directory",
    },
    KnownEnvVar {
        name: "OLDPWD",
        pattern: r"^/[\w./\-]+$",
        description: "previous working directory",
    },
    KnownEnvVar {
        name: "SHELL",
        pattern: r"^/[\w./\-]+$",
        description: "user's login shell",
    },
    KnownEnvVar {
        name: "HOSTNAME",
        pattern: r"^[\w.\-]+$",
        description: "system hostname",
    },
    KnownEnvVar {
        name: "LANG",
        pattern: r"^[\w.\-]+$",
        description: "locale",
    },
    KnownEnvVar {
        name: "LC_ALL",
        pattern: r"^[\w.\-]*$",
        description: "locale override",
    },
    KnownEnvVar {
        name: "TERM",
        pattern: r"^[\w.\-]+$",
        description: "terminal type",
    },
    KnownEnvVar {
        name: "EDITOR",
        pattern: r"^[\w./\-]+$",
        description: "default editor",
    },
    KnownEnvVar {
        name: "VISUAL",
        pattern: r"^[\w./\-]+$",
        description: "default visual editor",
    },
    KnownEnvVar {
        name: "PAGER",
        pattern: r"^[\w./\-]+$",
        description: "default pager",
    },
    KnownEnvVar {
        name: "TMPDIR",
        pattern: r"^/[\w./\-]+$",
        description: "temporary directory",
    },
    KnownEnvVar {
        name: "XDG_RUNTIME_DIR",
        pattern: r"^/[\w./\-]+$",
        description: "XDG runtime directory",
    },
    KnownEnvVar {
        name: "XDG_CONFIG_HOME",
        pattern: r"^/[\w./\-]+$",
        description: "XDG config directory",
    },
    KnownEnvVar {
        name: "XDG_DATA_HOME",
        pattern: r"^/[\w./\-]+$",
        description: "XDG data directory",
    },
    KnownEnvVar {
        name: "XDG_CACHE_HOME",
        pattern: r"^/[\w./\-]+$",
        description: "XDG cache directory",
    },
    KnownEnvVar {
        name: "UID",
        pattern: r"^\d+$",
        description: "user ID",
    },
    KnownEnvVar {
        name: "EUID",
        pattern: r"^\d+$",
        description: "effective user ID",
    },
    KnownEnvVar {
        name: "SHLVL",
        pattern: r"^\d+$",
        description: "shell nesting level",
    },
    KnownEnvVar {
        name: "DISPLAY",
        pattern: r"^:\d+(\\.\d+)?$",
        description: "X11 display",
    },
    KnownEnvVar {
        name: "SSH_AUTH_SOCK",
        pattern: r"^/[\w./\-]+$",
        description: "SSH agent socket",
    },
    KnownEnvVar {
        name: "SSH_CONNECTION",
        pattern: r"^[\d.:a-fA-F ]+$",
        description: "SSH connection info",
    },
    KnownEnvVar {
        name: "MAIL",
        pattern: r"^/[\w./\-]+$",
        description: "mail spool path",
    },
];

pub const KNOWN_COMMANDS: &[KnownCommand] = &[
    KnownCommand {
        name: "hostname",
        args: &[],
        pattern: r"^[\w.\-]+$",
        description: "system hostname",
    },
    KnownCommand {
        name: "whoami",
        args: &[],
        pattern: r"^[\w.\-]+$",
        description: "current username",
    },
    KnownCommand {
        name: "uname",
        args: &["-r"],
        pattern: r"^[\w.\-]+$",
        description: "kernel release",
    },
    KnownCommand {
        name: "uname",
        args: &["-s"],
        pattern: r"^[a-zA-Z]+$",
        description: "kernel name",
    },
    KnownCommand {
        name: "uname",
        args: &["-m"],
        pattern: r"^[\w]+$",
        description: "machine architecture",
    },
    KnownCommand {
        name: "uname",
        args: &["-n"],
        pattern: r"^[\w.\-]+$",
        description: "network hostname",
    },
    KnownCommand {
        name: "id",
        args: &["-u"],
        pattern: r"^\d+$",
        description: "user ID",
    },
    KnownCommand {
        name: "id",
        args: &["-g"],
        pattern: r"^\d+$",
        description: "group ID",
    },
    KnownCommand {
        name: "id",
        args: &["-un"],
        pattern: r"^[\w.\-]+$",
        description: "username",
    },
    KnownCommand {
        name: "id",
        args: &["-gn"],
        pattern: r"^[\w.\-]+$",
        description: "group name",
    },
    KnownCommand {
        name: "date",
        args: &["+%Y-%m-%d"],
        pattern: r"^\d\d\d\d-\d\d-\d\d$",
        description: "ISO date",
    },
    KnownCommand {
        name: "date",
        args: &["+%Y%m%d"],
        pattern: r"^\d\d\d\d\d\d\d\d$",
        description: "compact date",
    },
    KnownCommand {
        name: "date",
        args: &["+%s"],
        pattern: r"^\d+$",
        description: "epoch seconds",
    },
    KnownCommand {
        name: "date",
        args: &["+%Y-%m-%dT%H:%M:%S"],
        pattern: r"^\d\d\d\d-\d\d-\d\dT\d\d:\d\d:\d\d$",
        description: "ISO datetime",
    },
    KnownCommand {
        name: "date",
        args: &["+%H:%M:%S"],
        pattern: r"^\d\d:\d\d:\d\d$",
        description: "time",
    },
    KnownCommand {
        name: "basename",
        args: &[],
        pattern: r"^[\w.\-]+$",
        description: "filename component",
    },
    KnownCommand {
        name: "dirname",
        args: &[],
        pattern: r"^[\w./\-]+$",
        description: "directory component",
    },
    KnownCommand {
        name: "pwd",
        args: &[],
        pattern: r"^/[\w./\-]+$",
        description: "working directory",
    },
    KnownCommand {
        name: "wc",
        args: &["-l"],
        pattern: r"^[\d ]+$",
        description: "line count",
    },
    KnownCommand {
        name: "wc",
        args: &["-c"],
        pattern: r"^[\d ]+$",
        description: "byte count",
    },
    KnownCommand {
        name: "wc",
        args: &["-w"],
        pattern: r"^[\d ]+$",
        description: "word count",
    },
    KnownCommand {
        name: "nproc",
        args: &[],
        pattern: r"^\d+$",
        description: "CPU count",
    },
    KnownCommand {
        name: "tty",
        args: &[],
        pattern: r"^(/dev/[a-zA-Z0-9/]+|not a tty)$",
        description: "terminal device",
    },
    KnownCommand {
        name: "arch",
        args: &[],
        pattern: r"^[\w]+$",
        description: "architecture",
    },
    KnownCommand {
        name: "cat",
        args: &["/etc/hostname"],
        pattern: r"^[\w.\-]+$",
        description: "hostname from file",
    },
];

pub fn lookup_env_var(name: &str) -> Option<&'static KnownEnvVar> {
    KNOWN_ENV_VARS.iter().find(|e| e.name == name)
}

pub fn lookup_command(name: &str, args: &[&str]) -> Option<&'static KnownCommand> {
    KNOWN_COMMANDS.iter().find(|c| {
        c.name == name
            && (c.args.is_empty() || {
                args.len() >= c.args.len() && c.args.iter().zip(args.iter()).all(|(a, b)| a == b)
            })
    })
}
