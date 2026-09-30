# 455 — the second desugar path

DG read back three of my conclusions from the for-in session and corrected
one of them outright:

> Later you say transform rewrites getpid / status to `$` / `?` but we have a
> no-magic-variable rule - any desugar must go the other direction

He is right, and DD#111 says it in as many words: *"Variables are not magic in
sash… `$?` is punctuation sugar for the `status` intrinsic… The intrinsic is
the real thing."* I had found the rewrite, noticed its codegen arms were cold,
and concluded the arms were **dead by construction and should be deleted**.
That is backwards. The arms were cold because a pass upstream was routing
around them in the wrong direction; the fix is to stop the rewrite, not to
delete what the rewrite starves.

## Three names, two behaviours

The rewrite table mapped three intrinsics to magic slots. Splitting them by
whether anything ever *writes* the slot separates them cleanly:

| intrinsic | slot | `define` sites | verdict |
|---|---|---|---|
| `status` | `?` | 4 (`control_flow.rs:1114`, `statements.rs:795`, `stmt_handlers.rs:425`, `functions.rs:518`) | stateful — reading the slot **is** the lowering |
| `getcpid` | `!` | 1 (`statements.rs:878`, background jobs) | stateful, same |
| `getpid` | `$` | **0** | a pure rename — the DD#111 violation |

So `magic_intrinsic_var` was two things wearing one name. It is now
`stateful_intrinsic_slot`, holding only the two entries that name real
compiler-managed state, and `getpid` joins the stateless list in
`resolve_nullary_sexpr` alongside `getppid`/`epochseconds`. `nullary_magic_var`
became `nullary_stateful_slot` for the same reason. The four `"$"`
special-cases in `codegen/values.rs`, `emit_c.rs` and `ssa_taint.rs` went with
it — including `special_var_as_int`'s now-unused `module`/`externs` parameters,
which existed only to build the `getpid` call the arm no longer makes.

`--emit-ir`, the DD#104 public seam, was **already clean**: `$$`/`$?`/`$!` all
render as `getpid`/`status`/`getcpid`. The violation lived entirely below it.

## "Provably dead by grep" is not proof

I claimed those four special-cases were dead by construction, on the strength
of a grep for `lookup("$")` and `define("$")`. I deleted them, rebuilt, and
probed. `echo $(( $$ > 0 ))` printed **0**.

There was a **second, independent desugar path**. `parse/arith.rs`'s
`scan_arith_dollar` has its own single-character scan for `$#`/`$?`/`$$`/`$!`/
`$@`/`$*`, producing `ATok::DollarVar("$")` and then `ArithExpr::DollarVar("$")`.
It never consulted `magic_sugar_expansion` — the word path's table — so the
word form and the arithmetic form of the very same read had been desugaring by
different rules the whole time. The grep could not have found it: the arith
lexer builds the name a character at a time and never spells the call site the
way I searched for.

Fixed by giving both paths one table. `magic_sugar_intrinsic(name) ->
Option<&'static str>` is split out of `magic_sugar_expansion`, and
`parse_prefix_atom` maps a magic `DollarVar` to `ArithExpr::FuncCall { name,
args: [] }` — bit-for-bit the node `(getpid)` already produced.

## The invariant guard was blind by type

`assert_no_magic_sugar` exists precisely to make a missed construction site
loud instead of silent. Its own doc predicts the consequence of a miss:

> that one context would keep a magic variable, every spec would still pass,
> and the checks that look for the intrinsic form — SB5012's `$?` state
> machine above all — would simply stop seeing reads there.

It walks `ExprKind`. `ArithExpr::DollarVar` is a **different type**. No amount
of thoroughness in an `Expr` walk reaches it, so the guard proved its property
for the word path and was structurally incapable of proving it for the
arithmetic one. It now walks `ExprKind::Arithmetic` too, via
`ir::walk_arith_var_reads`.

The prediction was not hypothetical. It had already come true.

## SB5012 was blind to arithmetic status reads

With no preceding command:

| spelling | before |
|---|---|
| `echo "$?"` | SB5012 ✓ |
| `echo "$(status)"` | SB5012 ✓ |
| `echo $(( $? ))` | **accepted**, yields 0 |
| `echo $(( (status) ))` | **accepted**, yields 0 |
| `int n=$(( (status) ))` | **accepted** |

Confirmed pre-existing at HEAD by `git stash` + rebuild — this is not fallout
from the desugar. The desugar is only what makes it *findable*: the read is now
`FuncCall("status")` rather than an opaque `DollarVar("?")`.

Same root cause as the guard above, one layer down. `find_status_reads` runs
`walk_exprs_in_stmt`, which visits `Expr` nodes; an arithmetic `(status)` is an
`ArithExpr::FuncCall`. `$?` in a word and `$?` in arithmetic are the same read
to a user and two unrelated types to the compiler, and only one of them was
being checked.

`find_status_reads` now also walks `ExprKind::Arithmetic`. The arithmetic tree
has no per-node span, so the enclosing `$(( … ))` carries the diagnostic — and
carries it once however many reads it holds, since repeating one span prints
the same line twice.

### and the walker had a wildcard

The obvious fix — reuse `walk_arith_intrinsic_calls`, which already existed —
would have left a third of the bug in place. That walker spelled out its own
recursion behind a `_ => {}`, and the wildcard swallowed `Subscript` and
`SubscriptAssign`, both of which carry `ArithExpr` children. `$(( a[(status)]
))` stayed blind even after the `Expr`-level half was fixed.

`ArithExpr::children()` is already total over the enum and already names those
children. The walker now delegates to it and is four lines long. The omission
is not fixed so much as made unrepresentable — which is the only kind of fix
worth making for this, because its one other caller is the sexpr **arity**
check, blind under a subscript index for exactly as long.

That is the fourth non-total IR walk to be the bug in recent sessions. The
wildcard was not hiding a variant nobody added yet; it was hiding two variants
that had been there all along.

## What it cost

`spec/arithmetic.sh` gains `as.1`–`as.8` (`plan=338` → `346`): the four
forms that were accepted, the subscript regression guard, the
one-diagnostic-per-expansion rule, and three that pin the *unchanged* half —
a preceding command still makes the arithmetic read legal, and all three
spellings still agree on the value.

None of the five existing specs that read status inside arithmetic
(`argv.sh:244`, `arithmetic.sh` 31.22/31.23/sx.5, `convert.sh:944`) needed
touching: every one of them has a preceding command, which is the whole point
of the rule.

Sweep **7163** (3684 native, 3466 c, 13 skipped) — predicted exactly, twice,
zero failures, no retirements. `mk test` rc=0.

Three functions tripped the complexity gate on the way in
(`parse_prefix_atom` 13/6, `assert_no_magic_sugar` 8/8, `find_status_reads`
6/7). Extracting `dollar_var_atom`, `assert_expr_no_magic_sugar` and
`expr_reads_status` took them to 11/4, 4/2 and 3/2. The gate was right: each
of the three had grown a second job.

## The method note

The thing worth keeping from this session is the shape of the mistake, because
I made it twice in a row on the same code.

Both times I reasoned about what the compiler does from a search over how it is
*written*, and both times the answer depended on a construction site that did
not spell itself the way I searched. The first time a grep for `define("$")`
missed a lexer that assembles the name character by character. The second time
a guard that walks `ExprKind` missed a node that is not an `Expr`.

A search over source text proves a property of the text. Executing the program
proves a property of the program. The probe found both in about a minute each,
and the sweep would have found the first one anyway — spec `31.20` had been
pinning `$(( $$ > 0 ))` the entire time.
