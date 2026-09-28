# 440 — The guard that was keyed on syntax

The coverage queue pointed at `codegen/dispatch.rs::emit_match`, five cold
lines that are not an ICE guard but a *silent fallback*:

```rust
// Arity is validated up front; defensively treat a malformed call
// as a non-match rather than indexing out of bounds.
let (Some(subject), Some(pattern)) = (args.first(), args.get(1)) else {
    … test_result = 0; $? = 1; return;
};
```

Under yesterday's rule that arm has to be classified: unreachable by
construction, or merely unexercised? Deciding it meant testing the comment's
claim — "arity is validated up front" — rather than believing it.

The claim holds, and holds well. SB4045 (`match_arity_diag`) rejects any count
but 2 or 3; `check_match_arity_stmt` recurses through `child_bodies()`, which
is total and includes if/elif/while conditions and both sides of `&&`/`||` —
that is to say, every position DD#70's must-consume rule leaves for a `match`.
The 3-arg form is intercepted earlier by `try_handle_match_into`. So two args
always. Probes agreed in all five consuming positions.

## What the probe found instead

The probe I wrote to *corroborate* that proof compiled this:

```sh
array a=(b)
str s=abc
if match "$s" @a; then echo y; else echo n; fi
```

and the compiler panicked:

```
ICE: ArraySplat (@a) must be expanded by the argv builder,
     not lowered as a scalar value
```

Nothing to do with arity. The cold line was never the bug — it was the reason
I looked.

## The shape of it

Scoping it across every builtin found exactly two holes: `cd @a` and
statement-position `match … @a`. Everything else rejects a splat properly —
SB5030 for `echo`, SB5032 for externals, SB5070 for the string intrinsics,
SB5063, SB5074, SB4030, SB4038.

SB5070 is the one that should have claimed both, and here is the thing:
**`"match"` was already in its list.** `is_scalar_operand_intrinsic` named it
explicitly. The list was right. The dispatch was wrong:

```rust
let ExprKind::CommandSubstitution(stmts) = &expr.kind else { return };
```

The check was keyed on the `$( )` **syntax** instead of on the command. But
`match` is legal in two positions — a `$( )` value and a DD#70 boolean
statement — and only the first was ever scanned. `cd` is statement-only, so it
was never scanned at all, and was not in the list either.

The file's own doc comment had already warned about this exact landing site:
recognising the `arrvals_*` family is what "keeps a dispositioned splat from
slipping past the splat guards into a scalar value lowering (**which ICEs**)".
The hazard was known. The gap was that a guard which knows the right rule
still only fires where it is invoked from, and it was invoked from one of the
two places the rule applies.

That is the same failure the same doc comment records about `strlen`: a list
"keyed on the family `strlen` had just left" was not updated when DD#102 moved
it. Twice now the knowledge was correct and the wiring was not.

## The fix

One operand scan, `check_command_splat_operand`, consulted from both
positions: the existing expression walk for `$( )`, and a new
`check_stmt_splat_operand` that recurses via `child_bodies()` for statements.
`cd` joins the list, and the list's doc comment now says why it is one list
for both positions — keeping a second is how the two drift.

`in_cmdsub` picks only the syntax the remedy is written in, and the trailing
`…` is now tied to the real argument count, so `cd` is advised
`cd "$(join a SEP)"` rather than a two-argument form it does not have.

The statement walk deliberately does **not** descend into `$( )` bodies —
those are expressions, already scanned — so nothing is reported twice. Spec
75.116 pins that by counting the diagnostic rather than matching it.

## Verification

Sweep **6750 passing, 13 skipped, zero failing** — 6763 rows, exactly the
predicted +10 (5 specs × 2 modes). **Zero flips.** `mk test` 71 unchanged.
fmt, clippy, totality, complexity clean.

Specs 75.112–75.116: a splat in either operand of a statement `match`, `cd @a`,
the nested fn+if case that exercises the `child_bodies()` recursion, and the
no-double-report count for the `$( )` form.

## And the arm itself

With the claim verified, `emit_match`'s fallback converts under yesterday's
rule. It was never an ICE guard — it silently set `test_result = 0` and
`$? = 1`, reporting a *non-match* it had not checked. That is precisely the
fault SB4045 exists to fix: the journal entry for SB4045 records that the
2-arg lowering "ignored the surplus, and the command reported a SUCCESS it had
not checked". The fallback was the same bug wearing a comment.

Sweep after conversion: 6750 passing, zero failing, **zero flips** — across
6763 executions the panic never fires.

## Still open

The same probe found a second, wider hole of the same class, which is a
design question rather than a patch, so it is DG's: DD#84 elision `${p:~}`
ICEs in eleven commands — `cd`, `match`, `sub`, `gsub`, `toupper`, `tolower`,
`strlen`, `basename`, `dirname`, `return`, `exec`. The decision at
`validate.rs:3706` is

```rust
let elidable = !matches!(expr_literal_str(&c.name), "readline" | "read");
```

— allow-by-default with two exceptions, both added reactively after they
ICEd ("Before this it reached codegen and panicked"). `exec` is the
interesting one: it is genuinely variadic, so DD#84's own words make
`exec cmd ${p:~}` legal, and it ICEs anyway.
