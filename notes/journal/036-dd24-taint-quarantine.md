# Design Decision #24 (candidate): Taint quarantine and bless

**Date:** 2025-04-16

**Principle:** Taint doesn't exist inside the program. External data is
quarantined until blessed. The quarantine is enforced by the compiler refusing
any use of unblessed external data.

### What is tainted

Only truly external data:
- **Program entry:** `$1`–`$N`, `$@` from the OS
- **Runtime input:** `read`, `$(external command)`, `sash_getenv`

What is NOT tainted:
- `$0` — known at compile time
- `$#`, `$?` — integers, constrained by type system (DD#16)
- Literals, arithmetic results from untainted operands
- Function parameters whose arguments were untainted at the call site

### No propagation

Tainted values cannot be used for any purpose except blessing.
No assignment, no echo, no passing to functions, no command arguments.
The compiler rejects all uses of unblessed external data at compile time.

```bash
x=$1              # compile error
echo $1           # compile error
cmd $1            # compile error
process $1        # compile error
bless [[ $1 =~ ^[0-9]+$ ]]  # ok — $1 is now untainted
echo $1           # ok — blessed
```

This is simpler than Perl's taint mode. Perl tracked taint through every
operation (concat, substring, etc.). Safebash has no taint inside the
program — external data is quarantined at the boundary.

### Bless syntax

`bless` combines validation and untainting in one construct. The `[[ ]]`
or `(( ))` test expression validates; the LHS variable is untainted on
success. Only one variable per `bless`, `&&`/`||` ok within the test so
long as the same variable is tested throughout.

**Three forms, distinguished by context:**

| Form | Failure behavior |
|------|-----------------|
| `bless [[ $1 =~ ... ]]` bare | fatal exit |
| `if bless [[ $1 =~ ... ]]` | falsy, take else branch |
| `bless [[ ${1:~default} =~ ... ]]` | substitute default |

```bash
# Fatal on bad input
bless [[ $1 =~ ^[a-z]+$ ]]

# Handle bad input gracefully
if bless [[ $1 =~ ^[0-9]+$ ]]; then
    declare int port=$(($1))
else
    echo "bad port"
    exit 1
fi

# Fall back to default on bad input
bless [[ ${port:~8080} =~ ^[0-9]+$ ]]

# Arithmetic validation
bless (( ${timeout:~30} > 0 ))
```

### The `:~` operator

New parameter expansion operator, valid only inside `bless`. Means
"if unset *or if validation fails*, use this default."

- `${1:-8080}` — if unset, use 8080 (existing bash, unchanged)
- `${1:~8080}` — if unset or invalid, use 8080 (bless-only)

Outside `bless`, `:~` is a compile error. One context, one meaning.

### Objections acknowledged

1. `bless` does three things depending on context (fatal, truthy, substitute).
   Same overloading concern as `set -e` behavior varying by context.
   Counter: the three forms are syntactically distinct — bare statement,
   `if` condition, `:~` present. The compiler and the reader can distinguish
   them trivially.

2. `:~` mutates the variable to a value the caller never provided.
   Original input is destroyed. Functions that need both the original
   and the validated value can't get both.
   Counter: the author explicitly chose substitution by writing `:~`.
   If they need the original, they use the `if` form instead.

3. Context-only syntax (`:~` only in `bless`) is a learning trap.
   Counter: safebash's value proposition is "mechanical rewrite passes
   your security audit." The new syntax is the audit trail — `bless`,
   `:~`, `-` are security-relevant markers that bash lacks. Each makes
   a security property visible in source.

### Value proposition (clarified)

safebash's value is NOT "you already know the syntax." It's:
- You have a bash script in production
- It needs to pass a security audit
- Rewriting in Go/Rust is months of work
- safebash: mechanical changes, same structure, compiles to static binary, passes audit
- Every piece of new syntax (`bless`, `:~`, `-`, `-> int`) is an auditable
  security marker that makes the script's safety properties visible

