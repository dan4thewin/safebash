title: DD#103 — An unrecognized symbol in arithmetic context is an error
status: implemented

DG, 2026-09-13: *"Shouldn't any unrecognized symbol in arithmetic
context be an error except an assignment?"*

Yes. In `$(( … ))` and `(( … ))`, every name must resolve to something —
a variable in scope, a sash intrinsic, or a user function. The sole
exception is an **assignment target**, which introduces the name:
`(( x = 5 ))`, `(( x += 1 ))`, `(( x++ ))`, and the `for (( i=0; … ))`
initializer all define `i`/`x` rather than reading an undefined one.
(That matches bash, which is the oracle.)

## What was already true

Measured 2026-09-13 — every read spelling already obeyed the rule:

| spelling            | undefined name | code   |
|---------------------|----------------|--------|
| `$(( zz ))`         | rejected       | SB5031 |
| `$(( $zz ))`        | rejected       | SB5031 |
| `$(( ${zz} ))`      | rejected       | SB5031 |
| `$(( zz[0] ))`      | rejected       | SB5031 |
| `$(( a[zz] ))`      | rejected       | SB5031 |
| `(( zz ))`          | rejected       | SB5031 |
| **`$(( (zz) ))`**   | **`0`**        | —      |
| **`$(( (zz 1) ))`** | **`0`**        | —      |

The DD#58 sexpr node `ArithExpr::FuncCall` was the **only** hole, and it
leaked in both shapes.

## Root cause

`validate.rs` deliberately refused to check the name: a sexpr `(name …)`
may be a variable (`(x)` grouping), a user function (`(g)`), or an
intrinsic (`(len a)`), so it called `collect_only` — record for dead-var,
never flag for SB5031. The transform then resolved the same node
*positively*: `resolve_nullary_sexpr` treats an unknown nullary name as a
variable read, and anything else falls through to `IntFuncCall`, where
`inline_int_func` finds no user body and returns `0`.

So the two passes disagreed about what the node meant, and an unresolved
name fell through the gap as a plausible constant — the same recurring
class as `len $s` (DD#75) and the `(strlen s)`/`(strlen $s)` sexpr
rebuild (DD#102): **one concept, two spellings, a guard on only one.**

## The rule, as implemented

At that one node, decide instead of abstaining:

- **the name is a sash intrinsic** → `collect_only`, as before. Arity and
  type are somebody else's job (SB4045, SB4030).
- **nullary, not an intrinsic** → it *is* a variable read, because that is
  exactly what the transform will make of it. Record it normally. `known`
  already merges variable names with function names
  (`check_undeclared_reads` calls `collect_functions` into `globals`), so
  `(g)` on a real function stays clean and only a true unknown is flagged
  → **SB5031**.
- **carries arguments, not an intrinsic** → unambiguously a call, and not
  a callable thing. A user function with arguments does not even parse in
  a sexpr (SB4014), so an intrinsic is the only legal callee there
  → **SB5011**.

`resolve::is_intrinsic` was added as the single source of truth for
"is this name the compiler's": `BuiltinCmd::from_name(name) != Other ||
INTRINSICS.contains(&name)`.

## Why those two codes

They are the codes the *same construct* already gets in every other
position, which is the whole point of the rule:

- `$(( zz ))` is SB5031, so `$(( (zz) ))` is SB5031.
- `dtoi 2 3` and `x=$(dtoi 2 3)` are SB5011, so `$(( (dtoi 2 3) ))` is
  SB5011.

No new diagnostic code, and no bespoke message for a name that used to
exist — consistent with DG's ruling on removed builtins ("error just as
for any other unknown token in cmd position").

## How it was found

While removing `dtoi`/`itod` (DD#36, superseded). Those two names had
been *masking* the with-args half: `(dtoi 2 3)` was rejected SB4030 only
because `dtoi` was a **known** string producer. Delete the name and the
same expression silently became `0`. The hole itself predates the
removal — `(nosuchfn 1)` always returned 0 — and is a residual of
`bug-arith-sexpr-intrinsics-zero`, whose fix made `(x)` resolve as a
variable read without wiring up the undeclared-read check to match.

Specs: `arithmetic.sh` sx.17–sx.23 (both shapes, both `$(( ))` and
`(( ))`, plus no-false-positive controls for `(x)` grouping and a
nullary user int function).
