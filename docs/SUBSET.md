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

### Arithmetic (DD#31)

Three disjoint contexts, each with a single purpose:

| Context | Purpose | Accepts | Rejects |
|---------|---------|---------|---------|
| `(( ))` statement | Mutation | `=`, `+=`, `-=`, `++`, `--` | Comparisons |
| `if (( ))` condition | Comparison | `==`, `!=`, `<`, `>`, `<=`, `>=` | Mutations |
| `$(( ))` expression | Pure evaluation | Arithmetic on clean operands | Mutations |

`$(( ))` is a read context — mutation inside it (`$((x=10))`) is a
compile error. `(( x > 0 ))` as bare statement is a compile error.
`if (( x++ ))` is a compile error.

**Bash-mode coercion:**
- `$((x=10))` → `((x=10))`
- `$((x=10, x<y))` → `((x=10)); $((x<y))`
- `let "expr"` → `(( expr ))`

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

### Lists

- `cmd && cmd`, `cmd || cmd`, `cmd; cmd`

### Subshells (bash-mode coercion)

`( cmd )` is not valid sash-native syntax. In bash mode, subshells
are mechanically coerced to sash primitives:

- Variable isolation → synthetic function with `local` declarations
- Directory isolation → save/restore cwd around body
- Process isolation → `func &` (explicit fork, DD#38)

The subshell syntax is the bash declaration of "these effects are
temporary." The compiler decomposes it into the appropriate sash
primitive for each effect.

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

A value is tainted iff it originated, *directly*, from an external
source. Taint is a property of a definition, not a flow. There is no
propagation: tainted values cannot be copied, concatenated, indexed
with, or otherwise read except inside a `bless` test expression. The
only legal operation on a tainted value is to validate it.

### Taint Sources (the only ones)

A variable is tainted iff its defining instruction is one of:

- `x=$(getenv VAR)` — environment variable read
- `x=$(argv N)` / `x=$(argv)` — program positional params (DD#46)
- `read x [...]` from any inherited or opened fd, or any pipe
- `mapfile -t x` / `readarray` from any inherited or opened fd, or any pipe
- `$(cmd)` where `cmd` is an external command

Note: `$1..$9`, `$@`, `$*`, `$#` are **function-local only** in
sash-native mode (DD#46) — they refer to the nearest enclosing call's
arguments, never to program argv. Top-level use is a compile error;
bash-mode input is coerced to `$(argv N)` etc. Reading `$K` where `K`
exceeds the call's actual `$#` is a compile error in both modes.

`read` / `mapfile` from a heredoc or a here-string of clean data
produce clean values.

User-defined sash functions are **trusted**: they cannot introduce
taint. A function that internally touches a tainted source must bless
it before use, just like top-level code; if it doesn't, the function
itself fails to compile. Therefore `$(user_func ...)` capture is
clean.

### Reads (rejected unless inside `bless`)

Every operation other than direct assignment-from-source counts as a
read of the tainted value, and is rejected:

- `$x`, `${x}`, `${x:op}`, `${x[i]}` in any expression
- `b=$a` — RHS read of tainted `a`
- `cmd "$x"` — command argument
- `arr[$x]=v` — index expression
- `(( x ))` / `$(( x ))` — arithmetic operand
- `join @arr` when any element of `arr` is tainted
- Passing as a function argument (the call binds the parameter, which is a read)

The only permitted read is inside the test expression of `bless x [[ ... ]]`.

### Blessing

```bash
bless x [[ condition ]]
```

If the test passes, `x` becomes clean from this point forward
(SSA: a new clean version of `x` reaching downstream uses). A later
re-assignment to `x` from a tainted source re-taints it.

### Containers

Arrays and maps are containers of variables. Each element carries its
own taint, by the same rules as a scalar. `arr+=("$(getenv X)")`
stores a tainted element; subsequent reads of that element (including
`join`, iteration, subscript) are rejected unless blessed.

### Known Externals (Auto-Blessed)

- **26 environment variables:** HOME, USER, PATH, etc.
- **25 commands:** hostname, date, whoami, etc.

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
