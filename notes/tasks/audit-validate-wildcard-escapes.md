title: audit: sweep validate.rs for wildcard arms that silently skip checks
status: done
priority: P1
kind: audit
depends: []

The same defect shape has now produced five separate escapes in the taint
pass alone, four of them user-reachable and one of them a shipped taint
bypass (journal 400):

| site | variant that fell through | effect |
|---|---|---|
| `check_expr_taint` | `ExprKind::Test` | `[[ ]]` operands unchecked |
| `check_expr_taint` | `ExprKind::SlurpFile` | `[[ $(<f) = x ]]` silently legal |
| `check_expr_taint` | `ExprKind::Arithmetic` | **taint bypass** — `echo "$(( ${x:-0} ))"` |
| `taint_stmt` | `StmtKind::ArithTest` | `if (( … ))` condition unchecked |
| `reject_scalar_rhs_read` | `ExprKind::Arithmetic` | RHS copy not rejected; target tainted, then attest-able |

`check_expr_taint` and `reject_scalar_rhs_read` are now total over
`ExprKind` and cannot regress this way again. Nothing has audited the rest.

The task: enumerate every `match` in `validate.rs` (and the other
per-statement passes) whose wildcard arm is a **silent no-op** over an AST
enum, and classify each one — "no reads by construction" or "unclassified".
A wildcard that returns a *diagnostic* is fine; a wildcard that returns
`{}` on a node type that can contain reads is the bug shape.

Two known ones deliberately left alone by journal 400, both needing a
verdict rather than a reflex (STILL OPEN — see the verdicts section below):

- `ir::walk_arith_expansion_roots`'s `_ => {}`. Its remaining variants
  (`Literal`, `Var`, `DollarVar`, …) genuinely have no `Expr` children, but
  it is shared by several passes, so making it total is a wider change than
  that journal's scope.
- Bare `ArithExpr::Var`/`DollarVar` taint. Unreachable today — a *string*
  name in arithmetic is SB4030 (DD#77 wants a disposition) and an *int* name
  cannot be tainted once journal 400's fix is in (every construction route is
  SB4030, SB5020, or an array via DD#99 `read`). `check_arith_taint` names
  itself in a comment as the place that needs the arm if that ever changes.
  Worth a spec that pins the SB4030 shield rather than relying on the note.

Method: this is exactly the "validator bypass testing is the highest
priority" item from the run-1 audit. Prefer probing behaviour over reading
call sites — Bug A in journal 400 was invisible from the code and obvious
from one execution.

## Progress (journal 422)

**Enumeration done.** 146 wildcard arms in the validate/type_check passes;
**34 are silent no-ops**, 31 of those over AST enums (the other 3 are over
`&str`, which is fine). Ground truth established for classification:

- `ExprKind` has 13 variants; only `Literal`, `SingleQuoted` and
  `SimpleExpansion` lack `Expr` children — and `SimpleExpansion` *is itself*
  a read, so "no children" does not mean "nothing to check".
- **Every one of `StmtKind`'s 16 variants has children.** Any silent
  wildcard over `StmtKind` is therefore suspect on its face.
- `ArithExpr::children()` is already total, with no wildcard.

**Stmt-level cohort: clean.** Probed with a harness that wraps each
violating program in eleven containers (top level, `if` body, `else` body,
`while` body, `for` body, function body, group, negation, output
suppression, `&&` list, `[[ … { … } ]]` dispatch). `scan_const_stmt`
(SB4044), `scan_stmt_redecl` (SB5049), `subscript_subjects` /
`check_subscript_in_stmt` (SB4030, both array-string and map-int), and a
`check_expr_taint` control (SB5020) all fired in every container.

**Expression-level cohort: SWEPT, one bug found and fixed (journal 423).**
13 containers x 7 violating expressions = 91 squares; **one** was a live
taint bypass and the other 90 were clean.

The find: `expr_uses_tainted`'s `_ => false` swallowed
`ExprKind::Arithmetic`, so `array z=($(( ${h:-0} )))` never entered `z` into
the tainted set. `echo "${z[0]}"` hit the SSA backstop (no SB code, no
span); a `[[ ]]` operand is not a command argument, so
`if [[ ${z[0]} == 4242 ]]` compiled, ran, and reported whether the secret
matched. **The same enum variant journal 400 fixed in `check_expr_taint`** —
that fix made one walker total and did nothing for its sibling.
`expr_uses_tainted` is now total (specs `atint.13`-`atint.16`).

Verified NOT a second bug: a genuinely tainted array (`read arr`) is SB5020
in the identical `[[ ]]` position, so the validator covers that position
fine. One root cause.

The original list of suspects, for the record — the prediction was good: `SlurpFile` and `Arithmetic` are the recurring blind spots:

- skipping `SlurpFile`: `check_elide_expr`, `check_positionals_expr`,
  `dollar_at_expr`, `check_argv_splat_expr`, `check_value_cmdsub_reads`,
  `type_check::walk_value_default`, `type_check::check_subscript_types`
- skipping `Arithmetic`: `validate_expr`, `check_unquoted_glob`,
  `check_argv_splat_expr`, `check_value_cmdsub_reads`,
  `type_check::check_subscript_types`

Violating expressions to drive an expression-container harness, with the
code each should produce: `$1` → SB5024, `${a[@]}` → SB4038, `$@` →
SB5045, `${x:~}` → SB5046, `$a` on a collection → SB5064, a tainted read →
SB5020. Containers to wrap them in: bare, `"…"`, concatenation, array
literal, assoc value, subscript index, `$(( ))`, `$(<…)` filename, and a
`[[ ]]` operand.

**The audit's first find was one layer up, in the parser, and was not a
wildcard.** A double-quoted string had its own command-substitution parser
which had no `<` probe and a return type that could not carry a diagnostic
(`-> Vec<Statement>`, ending `Err(_) => break`). That produced a bash
divergence, silent program truncation, and a taint bypass —
`str v="$(<f)"` laundered file contents while the unquoted spelling was
SB5020. Fixed by converging the two paths; journal 422, specs `sl.12`,
`sl.13`, `cap.22`, `cap.23`.

It is worth recording *how* it surfaced, because the audit's method is the
finding: it was not visible in any wildcard arm. It appeared as an SB5050
dead-variable false positive on a probe program, three passes downstream of
its cause.

### Harness notes for whoever resumes

- `common.sh` **appends** `target/release` to `PATH`, so `SASH=` does not
  override sister binaries. Always prepend (now in AGENTS.md).
- `case` is removed from the language (SB4029) — so `StmtKind::Case` may be
  dead, which is its own verdict to give.
- `fn` is reserved; use `_myfn`. `if [[ 1 == 1 ]]` is const-folded (SB5086);
  use `[[ -e /proc ]]`. A bare `!` must be consumed (SB4018). Redirect or
  pipe on a compound command is SB4021/SB4022. `/bin/false` aborts under
  DD#55; `(( 0 ))` is the non-aborting way to set a failing status.
- Validation phases exit on the first non-empty batch, so a malformed
  *wrapper* masks the diagnostic you are probing for. Validate every wrapper
  with a trivial body first.


## Remaining, after journal 423

**`check_value_cmdsub_reads` still has `_ => {}`**, covering 5 of 13
`ExprKind` variants. Probed, every uncovered variant that could carry a
`$( )` read into a collection element is closed by an earlier rule:

| shape | result |
|---|---|
| `array z=($(( $(strlen $h) )))` | SB4020 — cmdsub in arithmetic rejected outright |
| `array z=($(<$(echo $h)))` | SB5021 |
| `array z=(${q[$h]})` | SB5021 |
| `array z=(${x:-$h})` | SB5021 |
| `array z=($(<$h))` | SB5021 |

So it is not currently exploitable. It is still the bug shape, and the
argument journal 423 makes about sibling walkers applies to it verbatim.
Making it total is cheap in code and awkward against the complexity gate
(cc 7 / **cog 5**, so it gets no dispatcher exemption and the 3x cog bar is
5.58). That is the reason it is filed rather than done, and it should be
done as its own change where the refactor can be shaped properly rather
than bolted onto a bug fix.

## The three owed verdicts — all given (journal 424)

**1. `ir::walk_arith_expansion_roots`'s `_ => {}` — sound, and removed
anyway.** `ArithExpr` has 16 variants; the wildcard swallowed five
(`Literal`, `Var`, `DollarVar`, `PostIncDec`, `PreIncDec`) and **not one of
them carries an `Expr`**, checked against `ArithExpr::children()`, which is
total and is the ground truth for the enum. A walker whose contract is
"hand `f` each embedded `Expr`" has nothing to give for any of them, so this
was never the `expr_uses_tainted` bug.

It is spelled out regardless, because the cost of being wrong went up: there
are now **three** callers and all three are taint checks
(`check_arith_taint`, `arith_uses_tainted` — added by journal 423 — and
`reject_scalar_rhs_read`). A variant added later with an `Expr` inside it
would silently skip taint in three places at once. Cost of the change: zero.
One `|`-group replaces one wildcard, so cc and cog are unchanged at 11/3.

`PreIncDec` stays a no-op rather than an ICE. It genuinely cannot arrive —
`reject_prefix_incdec` raises SB4020 in the *parser* — but the assertion
belongs where the invariant is load-bearing, and `children()` already has
it.

**2. Bare `ArithExpr::Var`/`DollarVar` taint — the shield is real, and is
now pinned by specs instead of a comment.** Probed every construction route
for a tainted int:

| route | result |
|---|---|
| `x=$(arg 1); $(( x ))` | SB4030 — DD#77 wants a disposition |
| `x=$(arg 1); $(( $x ))` | SB4030 |
| `int n=$(arg 1)` | SB4030 |
| `int n=$(getenv …)` | SB4030 |
| `read arr; int n=${arr[0]}` | SB5020 |

No tainted int is constructible, so neither walker needs the arm.
`atint.17`-`atint.21`.

**One of those five nearly went in vacuous, and the check that caught it is
worth copying.** After writing them I ran each program's *clean*
counterpart. `x=7; echo "$(( x ))"` is **also** SB4030 — so `atint.17` and
`atint.18` would pass with an untainted `x` and prove nothing about taint.
They are kept, because nothing else in the suite asserts the bare-name case
at all and it is genuinely the shield's first half, but their comments now
say plainly that they are not taint discriminators. `atint.21` is the sharp
one: clean gives SB4030, tainted gives SB5020 — two codes for one shape.

**3. `StmtKind::Case` is NOT dead — do not delete it.** It is the **DD#66
dispatch form** `[[ value { pat { … } } ]]`. `case`/`esac` the bash keyword
is what SB4029 rejects, and it has no parser production at all; the node
name is a holdover. The sole producer is `parse::parser::parse_dispatch`,
`stmt_is_dispatch` is literally `matches!(kind, StmtKind::Case(_))`, and
`--emit-ir` on `[[ $x == bar { echo bar } || { echo other } ]]` emits
`:kind :case`. The struct's own doc comment already says all of this
("Do not read a surviving `case` statement into it") — the suspicion in this
charter was mine, and it was wrong.

## Still open

**`check_value_cmdsub_reads` still has `_ => {}`**, covering 5 of 13
`ExprKind` variants — the one remaining instance of the shape in this
cohort. See the probe table above: not currently exploitable, and awkward
against the complexity gate (cc 7 / **cog 5**, so no dispatcher exemption
and the 3x cog bar is 5.58). It wants its own change, shaped properly.
