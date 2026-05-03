title: DD#24 — Taint Quarantine and Bless
status: implemented

All external input is tainted. Tainted variables cannot be used at any
site until blessed. This is the canonical taint/bless decision,
incorporating DD#43, DD#44, DD#47, DD#49.

## Taint sources

A variable is tainted iff its defining instruction is one of:

- `x=$(getenv VAR)` — environment variable read
- `x=$(argv N)` / `x=$(argv)` — program positional params (DD#46)
- `read x [...]` from any inherited fd, opened fd, or pipe
- `mapfile -t x` / `readarray` from any inherited fd, opened fd, or pipe
- `$(cmd)` where `cmd` is an external command

`read` / `mapfile` from a heredoc or here-string of clean data
produce clean values.

User-defined sash functions are trusted: `$(user_func ...)` capture is
clean. A function that touches a tainted source must bless it before
use or the function itself fails to compile.

## Clean sources

Literals, arithmetic from clean operands, `$0`, `$#`, `$?`, `$$`,
variables assigned from clean expressions.

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
port=$(argv 1)
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
