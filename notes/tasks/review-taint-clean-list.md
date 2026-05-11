title: Review taint clean list and magic variable policy
status: done
depends: []
severity: design
gates: [24.1, 24.2, 24.3, 24.4, 24.5, 24.6, 24.7, 24.8, 24.14, 24.15, 42.2]
  - tests
  - commit

taint.rs lines 104-109 has a large clean list that was never
reviewed by DG. Most of it should not exist.

## Policy

Three categories:

### 1. Keywords (frequent use, clear syscall)

Accessed via `$(keyword)`. Compiler-managed. Bash-mode coerces.

| Keyword | Replaces | Syscall/mechanism |
|---------|----------|-------------------|
| `epochseconds` | `$EPOCHSECONDS` | `clock_gettime` |
| `getpid` | `$$` | `getpid(2)` |
| `getppid` | `$PPID` | `getppid(2)` |
| `getcwd` | `$PWD` | `getcwd(2)` |
| `signum` | (new, DD#56) | signal handler global |
| `argv` | `$1..$N`, `$@` (top-level) | program arguments |
| `getenv` | env var access | `getenv` |

`$$` should be coerced to `$(getpid)` in bash-mode.
`$PPID` coerced to `$(getppid)`.
`$PWD` coerced to `$(getcwd)`.

### 2. Reject or coerce to external command (one-shot use)

Bash variables that map to commands the programmer should call
explicitly. Reject in sash-native, coerce in bash-mode.

| Variable | Coerce to |
|----------|-----------|
| `$HOSTNAME` | `$(hostname)` (external command) |
| `$UID` | `$(id -u)` |
| `$EUID` | `$(id -u)` (or reject — root checks are code smell) |
| `$GROUPS` | `$(id -G)` |
| `$HOSTTYPE` | `$(uname -m)` |
| `$OSTYPE` | `$(uname -s)` or reject |
| `$MACHTYPE` | `$(uname -m)` or reject |

### 3. Drop entirely (bash-specific, meaningless in sash)

| Variable | Reason |
|----------|--------|
| `BASH_VERSION` | sash isn't bash |
| `BASH_VERSINFO` | sash isn't bash |
| `BASHOPTS` | no shopt in sash |
| `SHELLOPTS` | no set options in sash |
| `BASH_SUBSHELL` | no subshells (DD#38) |
| `BASHPID` | use $(getpid) |
| `SRANDOM` | use /dev/urandom |
| `EPOCHREALTIME` | use $(epochseconds) or add epochrealtime keyword |

### RANDOM — drop, not coerce

`$RANDOM` / `sash_random()` is implemented (sash-rt, LCG seeded from
`getrandom`). Drop it. The LCG is predictable and unsuitable for
anything security-sensitive. A safety-focused project shouldn't ship
a bad PRNG. Use `head -c N /dev/urandom` for random bytes. Remove
`sash_random()` from sash-rt when this lands.

### 4. Needs individual decision

| Variable | Question |
|----------|----------|
| `SECONDS` | Useful for timing. Keyword $(seconds)? |
| `LINENO` | Useful for diagnostics. Keyword $(lineno)? |
| `PIPESTATUS` | Array of pipeline exit codes. Keep? |
| `OLDPWD` | Used by `cd -`. Internal to cd implementation? |
| `IFS` | Sash has no word splitting. Reject? |
| `SHLVL` | Nesting depth. Useful? |
| `_` | Last argument of previous command. Drop? |

## Action

1. Review and decide each item in category 4
2. Rewrite taint.rs clean list to match policy
3. Add coercion rules to lowerer for bash-mode
4. Add rejection rules to validator for sash-native
5. Implement any new keywords
