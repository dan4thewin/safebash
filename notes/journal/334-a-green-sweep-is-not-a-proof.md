# 334 — a green sweep is not a proof

DG: *"we have git, so deleting is reversible. we explicitly want to find and
purge dead code. file a task to consider the use-site audit."*

The task is filed (`notes/tasks/audit-dd24-use-site-list.md`, P1). This is the
purge. Six candidate sites went in; **four were purged, one turned out to be
live, one I declined to purge**. Sweep **5103 passing, 10 skipped, 0 failing** —
exactly the predicted total, which for a purge means *nothing moved*, which is
the whole point.

## The near-miss

The tempting way to do this is to delete, run the sweep, and let green stand in
for proof. That would have been wrong here, and I nearly proved it on myself.

Four of the candidates were `DoubleQuoted` arms in the splat helpers:

```rust
ExprKind::DoubleQuoted(parts) if parts.len() == 1 => is_any_arrvals_splat(&parts[0].kind),
```

My first argument for deleting them was: *a quoted splat can't produce an
`arrvals` operator, because inside double quotes `@a` is literal text.* The
first half is true. The second half is not the whole story — `"${a[@]}"` parses
to a parameter expansion whose operator is **`arrvals_retired`**, and

```rust
op.starts_with("arrvals")
```

matches `arrvals_retired` perfectly well. By my own stated reason, the arm was
reachable and I was about to delete a live branch.

It is dead, but for a different reason: **SB4038 is emitted at parse time**.
`retired_bracket_operator_diag` (`parse/parser.rs:2894`) is the only SB4038 site
in the codebase, and it fires in the parser, so an AST carrying
`arrvals_retired` never reaches `validate` at all. The arms are unreachable
because of *where the rejection lives*, not because of *what the operator is
spelled*.

Both facts are now in the doc comment, because the next person to read
`starts_with("arrvals")` will have exactly the doubt I had.

Two probes settle it empirically, and they are the two I should have run first:

```
$ cat t5.sh                     $ cat t6.sh
array a=(x y)                   array a=(x y)
/bin/echo "@a"                  /bin/echo "${a[@]}"

t5.sh:1:1: error [SB5050]       t6.sh:2:11: error [SB4038] the `[@]`/`[*]`
variable `a` is assigned but    array splat is retired — use `@a` …
never read (dead variable)
```

`SB5050` is the proof for the first: the compiler is telling me `a` was never
read, so `"@a"` really is inert text. `SB4038` is the proof for the second, and
its line:col shows it came from the parse of line 2.

## What was purged

All four helpers had exactly one real caller plus self-recursion, so merging
each predicate into its resolver at the call site deleted the predicate
outright and the now-constant guard with it:

```rust
-   if !is_any_arrvals_splat(&arg.kind) { continue; }
-   let collection = splat_collection_name(&arg.kind).unwrap_or("a");
+   let Some(collection) = splat_collection_name(&arg.kind) else { continue; };
```

```rust
-   if !is_bare_arrvals_splat(&arg.kind) { return None; }
    …
-   if bare_arrvals_splat_name(&arg.kind) == Some("args") { return None; }
+   if bare_arrvals_splat_name(&arg.kind)? == "args" { return None; }
```

`is_any_arrvals_splat` and `is_bare_arrvals_splat` are gone. `cargo build`
emitted **no unused-function warning**, which is the check that the merge was
complete — a leftover caller would have been an error, a leftover function a
warning.

The fifth purge is `parse_optional_return_type`:

```rust
-   if w == "->" { scanner.next(); return consume_int_token(scanner); }
```

The doc comment advertised "both the single `->` word token and the split `-`
`>` form depending on how the lexer classified the bytes". The lexer does not
have a choice: it always emits `>` as its own token, so all four spellings
(`->int`, `-> int`, `()  ->  int`, `  ->  int`) take the split path — 89 hits
on the split branch, 0 on the word branch. The string literal `"->"` appears
**exactly once** in all of `sash/src`, and it is this comparison. Nothing
constructs the token it tests for.

## The one that wasn't dead

`parse_negated` had

```rust
let pipeline_operand = matches!(scanner.peek().token, Tok::Pipe)
    || matches!(&stmt.kind, StmtKind::Pipeline(p) if p.commands.len() > 1);
```

with both branch records at zero, which reads as "the pattern never matched, so
the guard never ran". I had it on the purge list. Then `! ! /bin/true` flipped
`1054|0|1` — the pattern *does* match, the guard *does* run, and it is simply
never true. The operand was reachable all along; only its outcome was not.

That is a materially different claim and it needed a different argument:
`stmt` comes from `dispatch_statement`, which by contract never folds `|`, so
the pipeline it returns always has exactly one command and the rest is still
ahead as a trailing `|` token — caught by the first operand. Deleting the
second operand is therefore behaviour-preserving, and `! a | b` is still
SB4040. Purged, with the contract written into the comment in place of the
vague "an already-folded multi-stage pipeline".

**Both-branches-zero does not mean unreachable.** It means *nothing in the
current spec suite reaches it*. Distinguishing "no test exercises this" from
"no program can exercise this" is the entire job here, and the only thing that
settles it is an argument about the code, sometimes backed by a probe that
actually flips the counter.

## The one I declined

`validate_arith_comma_in_condition` was on my list with a note that it was
unreachable. That note came from **stale line numbers** — I mined a cold list,
then edited the file, and read the old line numbers against the new source. It
is live: it fires SB5007 four times in the sweep.

Its own doc comment said closing an earlier bug "left this duplicate cold".
That is false, and it is the kind of false that propagates: I believed it, and
it is what put the function on a deletion list in the first place. Corrected.

What *is* unreachable is the false arm of its mutation test, and here deleting
would change the rule the code states. A comma list containing a mutation
anywhere is decomposed upstream, so everything arriving is mutation-free and
every non-final part is an error — the test is a tautology *given the current
pass ordering*. Collapsing it to an unconditional error would encode "non-final
parts of a comma list in a condition are always wrong" in place of DD#31's
"non-final parts must be mutations". Those are different statements about the
language, and only one of them is the one we decided.

So: a branch that is dead because of an upstream pass is not the same as a
branch that is dead because nothing can construct its input. The first kind
stays, documented. Left cold deliberately, with the reason in the source.

## A duplicate the purge revealed

Removing the `DoubleQuoted` arm from `bare_arrvals_splat_name` made its body
byte-identical to `bare_splat_source`, four thousand lines further down the same
file:

```rust
let ExprKind::ParameterExpansion(pe) = kind else { return None; };
(pe.operator.as_deref() == Some("arrvals")).then_some(pe.name.as_str())
```

Two functions, one real caller each — SB5032 (`@args` into an external) and
SB5058 (`map t = @a` must state its odd-count disposition). They differed only
in taking `&Expr` versus `&ExprKind`. Consolidated onto the better-named one.

Worth noting *how* this surfaced: not from reading the file, but from clippy
rejecting my `.then(|| …)` and a `grep` for the fixed spelling returning a third
hit I had not written. The pre-commit gate found a duplicate by accident. I
would not have seen it otherwise, which is an argument for the `block_dupes`
view getting a pass over `validate.rs` at some point.

## Method

The rule I want to hold to after this one:

- **Verify statically, then probe, then delete.** Green is necessary and not
  sufficient; the sweep never exercised these paths, so it cannot testify about
  them.
- **Re-measure before mining.** A cold list goes stale the moment you edit, and
  `cov-probe.sh` answers "no branch records at that line" rather than failing
  loudly.
- **State the reason for death, not just the verdict.** "Unreachable" is not
  reviewable. "Unreachable because SB4038 fires in the parser" is, and it is
  what caught my wrong first reason.

## Numbers

- purged: 5 branch sites, 3 whole functions (`is_any_arrvals_splat`,
  `is_bare_arrvals_splat`, `bare_splat_source`)
- kept, documented: 1 (`validate_arith_comma_in_condition`'s mutation test)
- doc comments corrected: 3, all of which had been actively misleading
- units 83 green; sweep **5103 / 10 skipped / 0 failing**, predicted exactly
- behaviour delta: none, spot-checked across five splat spellings

Four of the six dead-code sites were introduced next to a comment that
described a generality the code did not have. That is the same shape as bugs
#12 and #13, and it is now six for six on "the comment was the tell".
