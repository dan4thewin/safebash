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

**Regex engine:** Plan 9 regexp (POSIX ERE subset). Supports `.`, `*`,
`+`, `?`, `|`, `()`, `[]`, `[^]`, `^`, `$`. Does **not** support
`{n,m}` counted repetition, `\d`/`\w` shortcuts, or lookahead/lookbehind.
Literal patterns are validated at compile time; unsupported syntax is a
compile error.

### Arithmetic (DD#31)

Three disjoint contexts, each with a single purpose:

| Context | Purpose | Accepts | Rejects |
|---------|---------|---------|---------|
| `(( ))` statement | Mutation | `=`, `+=`, `-=`, `++`, `--` | Comparisons |
| `if (( ))` condition | Comparison | `==`, `!=`, `<`, `>`, `<=`, `>=` | Mutations |
| `$(( ))` expression | Pure evaluation | Arithmetic on clean operands | Mutations |

**Arithmetic semantics:** All arithmetic is signed 64-bit integer (`i64`).
Division by zero prints an error and sets `$?=1` (matching bash).
Overflow on add/sub/mul wraps silently today; DD#25 designs runtime
traps but they are not yet implemented. String-to-integer conversion
of non-numeric strings yields 0 (matching bash).

`$(( ))` is a read context — mutation inside it (`$((x=10))`) is a
compile error. `(( x > 0 ))` as bare statement is a compile error.
`if (( x++ ))` is a compile error.

**Bash-mode coercion:**
- `$((x=10))` → `((x=10))`
- `$((x=10, x<y))` → `((x=10)); $((x<y))`
- `let "expr"` → `(( expr ))`
- `[[ $x -gt 5 ]]` → `(( x > 5 ))` (numeric comparisons belong in `(( ))`)

`[[ ]]` is string/file/boolean only. `(( ))` is arithmetic only.
No operator set overlap.

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

`[[ ]]` only — `[ ]` coerced to `[[ ]]` (DD#29).

| Operator | Type |
|----------|------|
| `-n`, `-z` | String null tests |
| `-f`, `-d`, `-e`, `-r`, `-w`, `-x`, `-s`, `-L` | File tests |
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

`$(cmd)` only — backticks coerced to `$(cmd)` (DD#29).

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
`trap`, `cd`, `unset` (array/map element removal and clearing),
`command` (flags `-p -v --`
handled).

### Background

`cmd &` — fork.

### Globs

`*` only for file matching — `?` and `[...]` rejected (DD#39).

---

## Rejected Constructs (Compile Errors)

| Code | Construct | Reason |
|------|-----------|--------|
| SB4001 | `eval` | Dynamic code execution |
| SB4002 | `source` / `.` | Use `sash-expand file \| mapfile -m var` + `bless` instead |
| SB5001 | `export` / `declare -x` | Use `FOO=bar cmd` instead |
| SB5002 | `$(cmd)` inside heredoc bodies | Extract to variable, bless, then reference |
| — | `${!var}` indirect expansion | Opaque variable name at compile time |
| SB4015 | `${var@...}` transforms | Use `${var^^}` / `${var,,}` instead |
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

### Containers (DD#61)

Arrays and maps are containers of variables; each element carries its
own taint. A tainted value may **not** be placed into an array through
program syntax — array/map literals, `+=` append, and `a[i]=` subscript
assignment all reject a tainted right-hand side (SB5021):

```bash
array a=( $(getenv HOME) )    # rejected — SB5021
a+=( $(getenv X) )            # rejected
a[0]=$(getenv X)              # rejected
```

A tainted array therefore only arises from a **bulk external read** —
`read -a`, `mapfile` / `readarray` (and the planned `array a=$(argv)`).
Every element of such an array shares one origin, so the array is
homogeneous and whole-array taint is exact. To put external data into
an array slot, bless a scalar first and assign the clean value:

```bash
h=$(getenv HOME); bless h [[ $h =~ ^/ ]]; array a=( "$h" )    # ok
```

**Bless-all** validates every element of a tainted array/map against a
per-element predicate — the only bless form for collections (scalar
`bless a [[…]]` on an array/map is a type error):

```bash
mapfile -t a < file
bless x in a [[ $x =~ ^[0-9]+$ ]]          # indexed: binds each value

mapfile -m m < file
bless k v in m [[ $k =~ ^[A-Z]+$ && $v =~ ^[[:print:]]*$ ]]   # map: key + value
```

A failing element aborts (exit 1); the `if bless …` form takes the
false branch and leaves the collection tainted. An empty collection
passes vacuously. The binder (`x`, or `k`/`v`) is fresh, scoped to the
test, and shadows any same-named outer variable.

### Known Externals (Auto-Blessed)

- **26 environment variables:** HOME, USER, PATH, etc.
- **25 commands:** hostname, date, whoami, etc.

---

## Coercions

Coercions are transformations applied by `sash-convert` when
converting Bash to sash-native source.

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
