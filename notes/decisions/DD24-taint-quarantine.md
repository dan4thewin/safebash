title: DD#24 — Taint Quarantine and Bless
status: implemented

All external input is tainted. Tainted variables cannot be used at any
site until blessed. This is the canonical taint/bless decision,
incorporating DD#43, DD#44, DD#47, DD#49.

## Taint sources

A variable is tainted iff its defining instruction is one of:

- `x=$(getenv VAR)` — environment variable read
- `x=$(arg N)` — single program positional param (DD#60)
- `array a=$(argv)` / `array a=$(argv N)` — whole/sliced argv snapshot (DD#60)
- `read x [...]` from any inherited fd, opened fd, or pipe
- `mapfile -t x` / `readarray` from any inherited fd, opened fd, or pipe
- `$(cmd)` where `cmd` is an external command

`read` / `mapfile` from a heredoc or here-string of clean data
produce clean values.

User-defined sash functions are trusted: `$(user_func ...)` capture is
clean. A function that touches a tainted source must bless it before
use or the function itself fails to compile.

## Clean sources

Literals, arithmetic from clean operands, `$#`, `$?`, `$$`, `$!`,
`$LINENO` (compile-time constant), variables assigned from clean
expressions.

Function positional params (`$1`..`$9`, `$@`) are clean within
functions — the compiler sees the caller's arguments and knows
their taint status. Top-level positionals are program argv (tainted,
use `$(arg N)`).

`$0` is not supported by the compiler — the converter rewrites it
to `$(arg 0)`.

For-in iteration is a **bind** or a **read** by iterand. Iterating a
*source* (`for x in $(argv)`, `while read x`) binds each element to the
loop var from the source — assignment-from-source, so per-iteration
body-bless is valid. Iterating a stored tainted *variable* (`for x in @a`
where `a` is a tainted named array) reads its elements (the `b=$a` case)
and is rejected; bless-all the array first (DD#61).

## Quarantine (DD#44)

Tainted data cannot be used at ALL use sites — not just dangerous
ones. Every operation other than direct assignment-from-source counts
as a read and is rejected:

- `$x`, `${x}`, `${x:op}`, `${x[i]}` in any expression
- `b=$a` — RHS read of tainted `a`
- `cmd "$x"` — command argument
- `arr[$x]=v` — index expression
- `(( x ))` / `$(( x ))` — arithmetic operand
- Passing as a function argument
- `join @arr` when any element is tainted

No propagation — taint is a property of a definition, not a flow.
Per-definition classification with use-site rejection.

## Bless (DD#43)

Assign first, validate second:

```bash
port=$(arg 1)
bless port [[ $port =~ ^[0-9]+$ ]]
```

`bless VAR [[ test ]]` — VAR explicitly named, must appear in test
expression (DD#49: SB4012 rejects tests not containing `$VAR`).
Additionally, `$VAR` must appear on the LHS of the test operator, not
the RHS — `bless x [[ "$x" == "$x" ]]` is rejected because the var
appears on the RHS (tautological, no real validation).

The test expression is a `[[ ]]` test — string comparisons, regex
match, file tests only. No external commands. No arithmetic. The
tainted VAR is readable inside the test expression. Other tainted
vars are not (standard use-site rejection applies).

## Runtime enforcement (DD#47)

Two forms:

| Form | On failure |
|---|---|
| `bless VAR [[ test ]]` | Writes "bless: validation failed" to stderr, exits 1 |
| `if bless VAR [[ test ]]` | False branch taken, VAR remains tainted |

On success, VAR becomes clean from this point forward (new SSA
version). A later re-assignment from a tainted source re-taints it.

## Auto-bless

`known_externals.rs` provides auto-bless patterns for 26 environment
variables and 25 commands with conservative regex patterns.

## See also

- DD#42 — getenv/argv as explicit-origin builtins
- DD#53 — Env var isolation (sash vars and env vars are separate namespaces)

## Containers

Arrays and maps: each element carries its own taint by the same
rules as a scalar. Reads of tainted elements (including `join`,
iteration, subscript) are rejected unless blessed.

## Clarification (2026-05-24): Pipeline taint and exec boundary

"Read from pipe" taint depends on whether the pipe SOURCE execs:

- `echo "hello" | read x` → clean (echo is a builtin, no exec,
  compiles to internal buffer)
- `printf "k=v\n" | mapfile -m m` → tainted (printf execs)
- `/bin/cmd | read x` → tainted (absolute path, execs)
- `user_func | read x` → clean (sash function, no exec)

The principle: taint comes from exec boundaries (fork+exec), file
reads, and env var reads. A pipe from code that runs inline within
the sash process is not an external source.
