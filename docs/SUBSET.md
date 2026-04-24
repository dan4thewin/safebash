# safebash — Safe Subset Specification

*This document describes the safe subset of Bash that the sash
compiler accepts. It reflects the current implementation, not
aspirational rules.*

**A note on authorship:** This document was written by an AI (Claude,
Anthropic) under the direction of DG.

---

## Design Principles

1. **No implicit behavior.** Sash has no word splitting and no
   implicit globbing. These entire classes of bugs are eliminated
   by the value model, not by linting.

2. **No dynamic code generation.** `eval` and indirect variable
   expansion `${!var}` prevent static analysis and are rejected.

3. **Explicit data boundaries.** External data is tainted by default
   (DD#24). Tainted data cannot be used until validated via `bless`.

4. **Silent coercion for bash idioms.** The compiler transforms
   legacy constructs silently — no diagnostics, no user action
   required. This is compilation, not linting (DD#40).

5. **No minimum bash version.** Since sash compiles to a native
   binary, the target system's bash version is irrelevant.

---

## Accepted Constructs

### Variables

| Construct | Description |
|-----------|-------------|
| `var=value` | Assignment |
| `$var`, `${var}` | Expansion |
| `${var:-default}` | Default value |
| `${var:+alt}` | Alternate value |
| `${var:?msg}` | Error if null/unset |
| `${var:=default}` | Assign default |

Non-colon forms (`${var-x}`, `${var+x}`, etc.) are silently coerced
to colon forms (`${var:-x}`, `${var:+x}`, etc.) — the unset/null
distinction is irrelevant in sash since nothing is ever unset.

### String Operations

| Construct | Description |
|-----------|-------------|
| `${#var}` | Length |
| `${var#pat}`, `${var##pat}` | Prefix strip (shortest, longest) |
| `${var%pat}`, `${var%%pat}` | Suffix strip (shortest, longest) |
| `${var:offset:length}` | Substring (negative length supported) |
| `${var^^}`, `${var,,}`, `${var^}` | Case conversion (upper, lower, first) |

### Pattern Replacement

| Construct | Description |
|-----------|-------------|
| `${var/pat/rep}` | Replace first |
| `${var//pat/rep}` | Replace all |
| `${var/#pat/rep}` | Replace prefix |
| `${var/%pat/rep}` | Replace suffix |

Patterns are internally coerced to regex via glob→regex conversion.

### Sash-Native Regex

| Form | Description |
|------|-------------|
| `${var:s/re/str/flags}` | Expression form — returns value, doesn't mutate |
| `var =~ s/re/str/flags` | Statement form — mutates in place |

Flags: `g` (global).

### Arithmetic

| Construct | Description |
|-----------|-------------|
| `$(( expr ))` | Arithmetic expansion |
| `(( expr ))` | Arithmetic command |
| `let "expr"` | Coerced to `(( expr ))` |

### Declarations

| Keyword | Description |
|---------|-------------|
| `int` | Integer (coerced from `declare -i`) |
| `array` | Indexed array (coerced from `declare -a`) |
| `map` | Associative array (coerced from `declare -A`) |
| `const` | Readonly (coerced from `readonly` / `declare -r`) |
| `local` | Local scope |

Stacked declarations supported: `local const int x=5`.

### Tests

`[[ ]]` only — `[ ]` is rejected (coerced to `[[ ]]`).

| Operator | Type |
|----------|------|
| `-n`, `-z` | String null tests |
| `-f`, `-d`, `-e`, `-r`, `-w`, `-x`, `-s`, `-L` | File tests |
| `-eq`, `-ne`, `-lt`, `-le`, `-gt`, `-ge` | Numeric comparison |
| `==`, `!=`, `<`, `>` | String comparison |
| `=~` | Regex match |
| Unquoted RHS in `==` | Glob matching |

### Control Flow

- `if` / `elif` / `else` / `fi`
- `while` / `until` / `do` / `done`
- `for x in list`
- `for (( ;; ))`
- `case` with `;;`, `;&`, `;;&`
- `break`, `continue`, `return`

### Functions

| Form | Description |
|------|-------------|
| `f() { }` | Standard |
| `function f { }` | Bash-style |
| `fn f -> type { }` | Typed return (sash-native) |

Positional params: `$1`–`$9`, `$#`, `$@`, `$*`.

### Command Substitution

`$(cmd)` only — backticks rejected (coerced to `$(cmd)`).

### Pipelines

| Construct | Description |
|-----------|-------------|
| `cmd \| cmd` | Pipe stdout |
| `cmd \|& cmd` | Pipe stderr |
| `! pipeline` | Negation |

### Redirections

`>`, `>>`, `<`, `2>`, `2>&1`, `&>`

### Heredocs

| Form | Description |
|------|-------------|
| `<<EOF` | Expanding |
| `<<"EOF"` | Literal (no expansion) |
| `<<-EOF` | Tab-stripping |
| `<<<str` | Herestring |

### Lists and Subshells

- `cmd && cmd`, `cmd || cmd`, `cmd; cmd`
- `( cmd )` — subshells (limited, prefer explicit)

### Builtins

`echo`, `printf`, `read`, `test`, `true`, `false`, `exit`, `shift`,
`trap`, `cd`, `unset` (assigns empty), `command` (flags `-p -v --`
handled).

### Background

`cmd &` — fork.

### Globs

`*` only for file matching — `?` and `[...]` restricted (DD#39).

---

## Rejected Constructs (Compile Errors)

| Code | Construct | Reason |
|------|-----------|--------|
| SB4001 | `eval` | Dynamic code execution |
| SB4002 | `source` / `.` | Would need safe loader |
| SB5001 | `export` / `declare -x` | Use `FOO=bar cmd` instead |
| SB5002 | `$(cmd)` inside heredoc bodies | Extract to variable, bless, then reference |
| — | `${!var}` indirect expansion | Opaque variable name at compile time |
| — | `[ ]` single bracket | Coerced to `[[ ]]` |
| — | `` `cmd` `` backticks | Coerced to `$(cmd)` |

**Note:** Unquoted `$var` is safe in sash (no word splitting/globbing)
but shellcheck may warn.

---

## Taint Model (DD#24)

### Taint Sources

All external input is tainted:
- `$(cmd)` — command output
- `$(getenv VAR)` — environment variables
- `$(argv N)` — positional parameters
- `read var` — user input

### Blessing

Tainted variables cannot be used until blessed:

```bash
bless var [[ condition ]]
```

### Known Externals (Auto-Blessed)

- **26 environment variables:** HOME, USER, PATH, etc.
- **25 commands:** hostname, date, whoami, etc.

### Taint Rules

- `(( ))` arithmetic context is a taint read — tainted var in
  `(( ))` is a compile error
- Taint is flow-sensitive — reassignment re-taints

---

## Coercions

Coercions are silent transformations visible via `--emit-source`.

| Input | Output |
|-------|--------|
| `declare -i` | `int` |
| `declare -a` | `array` |
| `declare -A` | `map` |
| `readonly` / `declare -r` | `const` |
| `typeset` | Bare declaration |
| `let "expr"` | `(( expr ))` |
| `local -i` | `local int` |
| `declare -ri` | `const int` |
| `${var-x}` | `${var:-x}` (non-colon → colon) |
| `` `cmd` `` | `$(cmd)` |
| `[ ]` | `[[ ]]` |

---

## Out of Scope

- **`select` loops:** Interactive construct — permanently excluded
- **Coproc:** Concurrency construct — permanently excluded
- **External program argument analysis:** safebash does not inspect
  arguments to `xargs`, `find -exec`, `parallel`, etc. The set of
  programs capable of executing arbitrary code is unbounded
- **Style suggestions:** (useless cat, useless echo, etc.) — not
  safety issues
