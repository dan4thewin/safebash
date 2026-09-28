## ══ STATUS (2026-09-29) — a splat ICE, found by auditing a cold line ══

Sweep **6750 passing, 13 skipped, zero failing** (6763 total — the predicted
+10: 5 specs × 2 modes). Zero flips. `mk test` **71** unchanged. All gates
clean. Journals 439 (coda) and 440.

Two landed, one open.

**1. Four copies of the `TestExpr` recursion collapsed to one.** Deleting
`test_uses_tainted` left three; there were actually four, because
`walk_test_operands` already existed in validate.rs — I had told DG a shared
walker would need *adding*. It did not. Moved to `ir.rs`; `walk_test_exprs`
and `check_test_taint` are both one-liners over it now. Closed three cold
lines by **deleting** code: the helper's `Not`/`And`/`Or` arms were cold
because an `attest` predicate is never a compound test, and `check_test_taint`
drives all three.

**2. `cd @a` and `if match S @a` ICEd the compiler.** Found while testing —
not believing — `emit_match`'s comment that "arity is validated up front" (it
is, airtight). SB5070's operand list **already named `match`**; the check was
keyed on `$( )` *syntax* rather than on the command, and `match` is legal in
two positions. `cd` is statement-only and was never scanned. One scan, both
positions. Specs 75.112–75.116.

> A guard that knows the right rule still only fires where it is invoked
> from. Key it on the thing it guards, not on the syntax you found it in.

**3. Open for DG — DD#84 elision ICEs in eleven commands.** Same seam, wider:
`${p:~}` panics in `cd`, `match`, `sub`, `gsub`, `toupper`, `tolower`,
`strlen`, `basename`, `dirname`, `return`, `exec`. `validate.rs:3706` reads

```rust
let elidable = !matches!(expr_literal_str(&c.name), "readline" | "read");
```

allow-by-default, two exceptions, both added reactively after they ICEd.
Patch the denylist, or invert to an allowlist of genuinely variadic commands?
`exec` is the sharp edge: it *is* variadic, so DD#84's own words make
`exec cmd ${p:~}` legal — and it ICEs. Details in journal 440.

## ══ STATUS (2026-09-29) — cold-by-construction taint arms now ICE ══

Sweep **6740 passing, 13 skipped, zero failing** (6753 total, unchanged — no
specs added). Zero flips. `mk test` **71 passed** (was 67). clippy, fmt,
totality and complexity gates clean. Journal 439.

DG's question undid the previous entry's conclusion: **"How is ICE a worse
alternative to a silent unintended change going live?"** It isn't. Every route
to those arms needs a change to the compiler's *own source*, so an ICE can
only fire in the tree of whoever opened the route — the cost I had invoked
does not exist, while the benefit (detection) is real. A "correct" arm nothing
exercises is an unverified claim, the same shape as the `_ => {}` that let
`SsaInst::Test` escape this pass for its whole life.

**The operative line, replacing "is the shape nonsense":**

> **Unreachable by construction → ICE. Merely unexercised → correct arm.**

Coverage cannot tell the two apart, which is what the previous entry's work of
*explaining* each cold arm was for.

**Converted (5 arms, proof by construction-site enumeration):**
`check_value_use`'s `ArithExpr`/`ArgvIndex`/`TableValue`/`TableKey` (all 8
sites are `SsaInst::Assign { value }` in `transform/pipelines.rs`) and
`AssocArray` (`ExprKind::AssocArray` is built only by `parse_array_literal`,
called only from the two assignment parsers); `expr_uses_tainted`'s `Test` arm
(`ExprKind::Test` is built only by `parse_test_command`/`parse_attest`, both
statement-yielding). `check_assoc_use` and `test_uses_tainted` are deleted.

**Rejected by the same test (3 arms, still correct arms):**

- `collect_expr_var_name`'s `${…}`/`$a[i]` — **the previous entry's
  by-construction story for these was wrong.** `cmd_handlers.rs:296` builds an
  `SsaInst::RedirectPush` that bypasses both `spill_scalar_redirect_targets`
  call sites, so an inlined function call's redirect is not spilled.
  Converting would have shipped a crash on a valid program.
- `check_value_use`'s `CmdSubstBody` — the lift's callers are the
  assignment/return/test paths only, not command arguments.
- `cmd_subst_taint`'s user-function head — a set lookup, not a structural
  invariant.

Deleting `test_uses_tainted` also dissolved a pending question: the `TestExpr`
recursion was hand-written three times and the copy nothing ran is now gone.
`check_test_taint` stays — it is the hot diagnostic twin and is what enforces
that a `[[ ]]` operand is a read position.

**Correction to the previous entry:** `check_assoc_use`'s coldness is a
*symptom* of the split `notes/tasks/ssa-taint-full-backstop.md` diagnoses, not
something that task's fix would cure. Its fix is a whole-value check at the
top of `check_value_use`; entry is still gated on `taint_reads`, which it does
not touch. `value_is_tainted` keeps hot arms for all five shapes (2592
executions for `AssocArray`) because it is the derivation half.

**Next:** `coverage.db` is invalidated again by this commit (it renumbers
`ssa_taint.rs` and `validate.rs`) — re-run `mk cov-union` → `mk cov-db` →
`mk cov-reset` before any further line-level ranking.

## ══ STATUS (2026-09-29) — the coverage queue's two cold taint checks ══

Sweep **6740 passing, 13 skipped, zero failing** (6753 total, predicted before
running). `mk test` 67 passed. clippy, fmt, totality and complexity gates
clean. Journal 438.

`coverage.db` was invalid by construction — three commits had renumbered
`validate.rs`, `ssa.rs` and `ssa_taint.rs`, and every row is keyed by `(file,
line)` with no commit recorded. Rebuilt. The union now reads **regions 97.24%
· functions 98.77% · lines 97.95% · branches 91.12%**.

**Only two functions in `sash/src` are entirely cold, and both are taint
checks** — `ssa_taint::check_assoc_use` and `validate::test_uses_tainted`. A
taint check that never runs is the strongest bug hypothesis the queue offers.
Neither is a bug; both are unreachable by construction:

- `SsaInst::Assign` deliberately reads nothing, and `AssocArray`, `ArgvIndex`,
  `TableValue`/`TableKey` and `ArithExpr` are built *only* as an `Assign`
  value — so they never arrive at `check_value_use`.
- An `ExprKind::Test` is never a collection value (`array a=( [[ -n $t ]] )`
  is SB4014 at parse), so `expr_uses_tainted`'s `Test` arm never fires.
- `collect_expr_var_name`'s `${…}` / `$a[i]` arms are cold because
  `spill_scalar_redirect_targets` rewrites every such redirect target to a
  synthetic scalar before the SSA exists. Taint survives the spill via the
  temp's own derivation — measured in both directions.

All of that is now **pinned by five unit tests**, the mechanism the mkfile
already names for should-never-run guards ("the 100% target is the union of
spec- and unit-test reachability"). Three arms were neutered simultaneously to
prove the pins fail.

**Two cold lines were genuine missing coverage**, both in `ssa_taint.rs`:
`elide_taint`'s DD#91 `taint_off` arm, and `splat_taint`'s tainted-*array*
return — the half `a89a530` did not touch. `bkstp.9`-`bkstp.12` close them as
two pairs differing by one character (`${t:!~}` vs `${t:~}`, `@{b:!-z}` vs
`@{b:-z}`).

**`contrib/cov-probe.sh` could not answer the question it is aimed at.** A
branch *body* carries no `BRDA` record, only its condition does, so probing a
cold **line** — which is what the queue ranks — returned "no branch records at
that line", indistinguishable from "not instrumented". Added a `DA` fallback.



Sweep **6732 passing, 13 skipped, zero failing** (6745 total, predicted before
running). `mk test`, clippy, fmt, totality and complexity gates clean.
Journal 437.

**The SB5031 splat bug is fixed, and there was no decision to make.** I filed
it as DG's call — a false rejection traded against a false acceptance. Asked to
explain the decision, I measured the direction I had only reasoned about:

```sh
fn f { array a=(1 2); /bin/echo @{a:-z}; }
f
/bin/echo @{a:-z}          # accepted, and printed `1 2`
```

DD#86 makes that name local to `f`, and the `$y` / `$a[0]` spellings of the
same read both reject. Only the splat leaked, at runtime. So the flat
`program_scopes` union was not conservative — it was wrong in both directions,
and the tradeoff did not exist.

**Why the other spellings were right.** The main SB5031 check is scope-aware:
`check_reads_in_scope` threads `known` per scope and descends into `$( )`
bodies, so DD#86's worked example rejects and the in-body read accepts.
`validate.rs` says the sink exists so read-checks "can never diverge on what
counts as a read". `check_scalar_splat` was bolted on later, beside that
traversal rather than into it, and is the one that diverged.

**The fix.** A splat now records through `ReadSink::record_splat` against the
current scope's `known`. The reason it fell through before is worth keeping: a
disposition is an *operator*, and `record_param` treats any operator as
SB5031's opt-out — which is right for `${x:-…}` and wrong for `@{a:-z}`, where
the disposition covers the array being **empty**, not **undeclared**.
`check_scalar_splat` keeps SB5079 only. Nothing needed widening; the
scope-aware traversal already went everywhere the answer lives.

Specs 75.108-75.111.

**Still open.** `Statement::direct_exprs()` totality (and the `<'a>` it needs
before a borrowed body can escape its callback) — no longer blocking anything
known. 45 walks in `validate.rs` recurse via `child_bodies()`; a hand-written
subset descends into command substitutions; nothing makes a walk choose. Two
filed instances remain: `bug-taint-escape-in-cmdsub-body` (P2) and SB5050 not
reaching a body (unfiled).

## ══ STATUS (2026-09-29) — `child_values` is total; a third escape closed ══

Sweep **6724 passing, 13 skipped, zero failing** (6737 total, predicted before
running). `mk test`, clippy, fmt, totality and complexity gates clean.
Journal 436.

**A third live taint escape, same shape as the first two.**
`SsaValue::child_values()` ended in `other =>` and silently dropped four
children. One of them was a bug: `ArraySplat`'s `:-WORD` default never reached
`value_is_tainted`, because `splat_taint` read only the array name and
`taint_off`. So

```sh
array a=( )
/bin/echo $(str t=$(getenv T); array b=( @{a:-$t} ); echo $b[0])
```

derived `b` **Clean** with a tainted value in it, and compiled. It rejects at
the backstop now.

**`!` vouches for the array, not for the word.** DD#91 scopes `!` to its own
subject's read, so `@{a:!-$t}` still carries `$t`'s taint. A `:?` word is an
abort *message* — the abort path exits, so it yields nothing and contributes
nothing, exactly as `RequireValue`'s message does not. A wordless `@{a:-}` has
only a placeholder and must not be examined at all. Three field combinations,
three answers; the old code had one.

**The getter is total now** — one flat match over all 30 variants, cc 23 /
cog 1, under the `do not add a wildcard` marker. Three hand-compensations came
out with it (`intern_value`, `contains_capture`, and `splat_taint`'s omission).
The other three dropped children were probed and are real but unreachable: no
interner panic at one or two levels of lazy nesting, in a splat word, or in
int arrays/maps/subscript arithmetic.

**Specs `bkstp.5`-`bkstp.8`**, with `splat_word_taint` neutered first to prove
the negatives could fail.

**A second bug found while probing, not fixed — DG's call.** A body-declared
array is rejected as undeclared:

```sh
/bin/echo $(array a=( ); array b=( @{a:-z} ); echo $b[0])   # SB5031
```

`check_scalar_splat` **reports** with `walk_expr_tree`, which reaches inside a
`$( )` body, and **collects** with `program_scopes`, which does not —
`child_bodies()` is total over `StmtKind`, and a `$( )` body hangs off an
expression, so it structurally cannot appear. The same asymmetry as the taint
bug, in a different subsystem.

**Correction, same day.** I first filed this as needing a scoping decision from
DG — a false rejection traded against a false acceptance. That was wrong: only
one direction had been measured. The flat union false-**accepts** too, with a
runtime consequence:

```sh
fn f { array a=(1 2); /bin/echo @{a:-z}; }
f
/bin/echo @{a:-z}          # accepts, and prints `1 2`
```

DD#86 makes a name first assigned in a function body local, so that must be
SB5031 — and `$y` and `$a[0]` spellings of the same read both reject correctly.
Only the splat leaks. DD#86 settles the semantics in both directions, and the
scope-aware `check_reads_in_scope` / `ReadSink` traversal already implements
them; `check_scalar_splat` was bolted on later with its own flat union and is
the one read-check that diverged from the shared sink the file was designed
around. No decision, mandatory fix. Filed as
`bug-sb5031-blind-to-cmdsub-body-decls`.

**45 walks in `validate.rs` recurse via `child_bodies()`**; a separate
hand-written subset descends into command substitutions. Nothing makes a walk
choose. Every one that reports inside a body and collects outside it has this
bug latent.

**`Statement::direct_exprs()` is the remaining getter**, and it now needs more
than totality: an explicit lifetime
(`fn direct_exprs<'a>(&'a self, f: &mut impl FnMut(&'a Expr))`) before a
borrowed body can escape its callback. The SB5031 fix needs exactly that.

## ══ STATUS (2026-09-28) — the taint read path is total; two escapes closed ══

Sweep **6716 passing, 13 skipped, zero failing** (6729 total, predicted before
running). `mk test`, clippy, fmt clean. Journal 435.

**Two live taint escapes are closed.** Both compiled before today:

```sh
/bin/echo $(str h=$(getenv TARGET); echo pwned > $h)     # wrote an env-named file
/bin/echo $(str h=$(getenv TARGET); if [[ -n $h ]] { echo yes })  # branched on input
```

**The finding DG's question produced.** Every taint escape this project has
recorded sits behind a wildcard match arm; **not one has been found behind an
exhaustive one**. The A/B is inside one file: `classify_inst_def` (the write
side) lists all 19 `SsaInst` variants and has never produced an escape;
`check_inst_uses` (the read side) ended in `_ => {}` at 4 of 19 and produced
both of the above. `Test` and `RedirectPush` are named in the exhaustive half
twenty lines above the wildcard that omitted them.

**What changed.** Read positions are enumerated once now, by
`SsaInst::taint_reads()` in `ssa.rs` — total over the enum, no wildcard, with
the doc comment saying so. `check_inst_uses` is a loop over it.
`check_value_use` is total over all 30 `SsaValue` variants; it had 8.

Redirects were the structural half: a `Redirect`'s target is an unlowered
`ir::Expr` and never becomes an `SsaValue`, so no value-oriented walk could
ever have reached it. The getter yields a sum type and the redirect half
resolves by bare name.

**`SsaInst::Test` gained a `TestOrigin`.** The first sweep after the change
failed 14 specs, all `attest` (`24.13`, `bfb.1`-`6`). The caught-attest forms
reach SSA as a bare `Test` in a condition block, indistinguishable from a user
`[[ ]]`, and reading the tainted subject is the point of the form. `Plain` vs
`AttestPredicate`; two of seven construction sites are attest.

**New gate: `contrib/totality-check.py`**, wired into `.githooks/pre-commit`.
Any function whose doc comment says `do not add a wildcard` must have none.
Seven opt in today. Verified it can fail before trusting it. Whole-tree, not
grandfather-on-modify — there is no backlog.

Four new specs, `bkstp.1`-`bkstp.4` in `taint.sh`: two for the escapes, two for
the `TestOrigin` exemption, since those were what the first sweep broke.

**Bookkeeping.** `bug-taint-escape-in-cmdsub-body` P1 → P2: the escape is gone,
what remains is that the rejection has no SB code and no span. Its fix sketch
was corrected — it said a `$( )` body is a subshell, which the DD#86 addendum
retired. `ssa-taint-full-backstop` deferred → next, with a measured table and a
diagnosis.

**Awaiting DG.** The fourth part of the approved plan — routing the front-end
taint pass through `Statement::direct_exprs()` instead of its 20 hand-written
call sites — is filed rather than started. Measuring changed the case for doing
it now: `check_expr_taint` is *already* total over `ExprKind`, so that work
buys call-site completeness, not variant completeness, and it moves diagnostic
ordering across the whole front end. Different size, different risk, and not
what was closing the escapes. See `getters-still-ending-in-a-wildcard`, which
also carries the two structural getters that still end in a wildcard
(`SsaValue::child_values()`, `Statement::direct_exprs()`) and the four
`SsaValue` variants whose children `child_values()` silently drops.

## ══ STATUS (2026-09-28) — P0 fixed; a `$( )` body is an anonymous function ══

Three changes, all green. Sweep **6708 passing, 13 skipped, zero failing**
(6721 total, predicted before running). `mk test`, clippy and fmt clean.

**The P0 is closed.** `bug-autodeclared-collection-scalar-read-leaks-pointer`
is `done`. SB5064's collection set now includes the auto-declaring binders
(`read`, `argv`, 3-arg `match`/`matchall`), so `$x` on one of them no longer
hands a live `SashTable*` to `execve`. The set is computed from the AST, not
shared with `type_check` — validate runs first and its diagnostics abort, so
sharing was not available. It subtracts scalar bindings so that
`x=safe; …; read x` reports the real fault on the `read` line rather than
SB5064 against a correct line above it.

**No rebinding, including by the bulk binders (DD#86).** `str x=hi; read x` is
now SB4030 rather than a silent retype, and `int x=1; read x` is SB4030 rather
than `ICE: expected string var, got int`. One
`register_collection_target` replaces the two unconditional inserts.

**The capture-body snapshot/restore is gone.** DG: *"a `$( )` capture could be
modeled as an anonymous function; all new variables would be local."* The old
save/restore emulated a subshell sash does not have; it contradicted DD#86,
contradicted itself (`$(f)` let a global write survive while an inline body
did not), and its save set was not total. Deliberate bash divergence, recorded
as a DD#86 addendum. Two specs that asserted the old behaviour were rewritten:
`commands.sh` 14.3b, `errors.sh` csc.7.

Specs: `types.sh` 75.101–75.107, `scope.sh` 86.59–86.70. Journal 434.

### Next up

- `bug-taint-escape-in-cmdsub-body` (P1) — still open, and the
  anonymous-function ruling **changes the fix**. The old framing was "clone
  the tainted set for the body"; that was subshell thinking. A body write to a
  global now persists, so taint must persist out of a body too. DG has not
  chosen between hoisting the body walk to the statement layer (where
  `taint_stmt` holds a `SlotCx`) and threading `SlotCx` through
  `check_expr_taint` (~15 call sites).
- **SB5050 does not reach a `$( )` body either** — `$(int x=1; echo hi)`
  compiles with a dead variable. Same "validate does not walk body statements"
  theme. Not filed.
- `collect_bulk_targets` over-collects `readline` (a scalar binder) via
  `collect_read_targets`. Pre-existing, latent, uninvestigated.
- Awaiting DG: the DD#78 amendment, `bug-brace-breaks-unquoted-word` (P2), and
  whether to close the ~30 `janet-*` tasks that DD#104 made obsolete.

## ══ STATUS (2026-09-28) — P0: a heap pointer reaches argv ══

Found while answering DG's question about how a `$( )` body is validated. It
has nothing to do with command substitution — that was only the path that
removed the masking.

```sh
argv x
attest e in x [[ -n $e ]]
/bin/echo $x        # 0x7f3ce6cc2040 — a live mmap address, fresh each run
```

`read x` is the same. **Both backends produce it identically**, so it is the
shared model, not a codegen divergence. The emitted C is four lines:
`x_ptr = (const char*)__t0_tbl; x_len = 0;` puts the table in the *scalar*
slot, then `sash_argv_guard(x_ptr, x_len)` is handed length 0 and validates
nothing while `execve` reads to the first NUL — through the table header,
whose first field is a pointer. It also propagates: `str y=$x` carries it into
an ordinary scalar.

**Two collection sets that disagree.** `type_check.rs` registers auto-declared
targets in `env.collections` (`register_argv_target`, `register_newread_target`
— the DD#60/DD#61 auto-declare). `validate.rs::collect_collection_names` walks
only `StmtKind::Declaration` with `-a`/`-A`. SB5064 — the guard written for
exactly this spelling — consults the incomplete one, so `array x=(a);
/bin/echo $x` is correctly rejected and `argv x; … /bin/echo $x` is not.

Second fault, independent: auto-declare never checks for a conflicting
declaration, though the explicit form does. `str x=safe; array x=(a)` is
SB5049; `str x=safe; read x` silently retypes, and `int x=1; read x` is
`ICE: expected string var, got int`. The `map` case is caught only by accident,
via attest-all's binder arity.

Masked all along by the guards that fire first — SB5020 on the untainted
spelling, SB5050 on the short ones, SB4018 on `matchall`. The leak is reachable
only *after* you satisfy them, which is why no spec found it.

**Not fixed — two questions for DG.** Where the single collection set should
live (the recomputed-worse-view is the cause), and whether `str x=…; read x`
should be SB5049 like `array x=(a)`, or a sanctioned rebind. The `int` ICE must
stop either way.
→ `notes/tasks/bug-autodeclared-collection-scalar-read-leaks-pointer.md`

### What the question actually was: how is a `$( )` body compiled?

Answered, and worth recording because the model is not what the code comments
say. **Codegen does exactly the recursive lowering DG expected** — the body's
statements go through `lower_stmts` into ordinary blocks of the *same* SSA
function (`_blk_1: /* cap_body */`), with no fork when the body is pure.
Isolation is then re-imposed deliberately: `capture_body_into`
(`transform/stmt_handlers.rs:1307`) snapshots every variable the body assigns
and restores it after (`__save_bodycap_x`). So there is no subshell — there is
a compiler-synthesized save/restore, and the comments calling it "subshell
isolation" are describing the effect, not the mechanism.

**Validate is the one layer that does not recurse.** Positionals (SB5024),
elide (SB5046), argv-splat (SB5025), resolve and type-check all walk the body.
Taint alone substitutes `check_cmdsub_inner_reads` — four lines, `if let
StmtKind::Command`, only `c.args` and collection readers, and it never records
a definition. That is the whole of the P1 escape filed earlier.

And the save/restore is itself not total: `collect_assigned_into`
(`stmt_handlers.rs:1359`) handles `Assignment` and `Declaration` only, so a
subscript assignment leaks out of the body (`a[0]=leaked` survives) and so does
a builtin write. Against DG's model — no subshells, writes survive unless a
fork is required — the open question is whether the leak or the restore is the
part that is wrong.

## ══ STATUS (2026-09-28) — a closer beats absorb; P1 TAINT ESCAPE found ══

### ⚠ Open P1 for DG — `check_taint` never walks a `$( )` body

```sh
/bin/echo $(str h=$(getenv TARGET); echo pwned > $h)
```

Compiles clean — no SB5020, no backstop. An environment variable chooses a
filename and the program writes to it. The same two lines at top level are
SB5020.

`check_cmdsub_inner_reads` is the only thing that looks inside a body, and it
is `if let StmtKind::Command` over the outer `tainted` set — not total over
`StmtKind`, and never extended by a definition made *inside* the body. The real
walker (`taint_stmt`) is never applied to a body at all. **Fourth non-total
walk in four sessions**, and the first one in the primary validator rather than
a lowering pass.

The SSA backstop masks most of it (with an internal-sounding message and no SB
code, which is why it read as handled). The two it cannot mask are the two
journal 400 already recorded as escaping it — `[[ ]]` read-back and redirect
targets — and those are exactly the two that compile.

**Not fixed — the approach is a fork.** `check_expr_taint` has no `SlotCx`, so
either thread it through the whole expression-taint layer, or move the body
walk up to the statement layer where `taint_stmt` already holds one. Blast
radius is real: no spec doing work inside a `$( )` body has ever had its
statements taint-checked. → `notes/tasks/bug-taint-escape-in-cmdsub-body.md`

### Landed: `$(int x=1)` is no longer "unterminated `$(`"

`)` is `Tok::RParen`, and DD#71 absorb makes it a literal in an assignment
value — so `int x=1)` ate the substitution's own terminator and the body ran to
end of input. SB4057 was telling the truth about what the parser saw. The tell
was in plain sight all along: top-level `int y=1)` is SB4030 "cannot assign
**string** value to int variable `y`".

**Not a fork — DD#71 settles it.** It scopes absorb "to whitespace / *closer*",
and `]]` already beats absorb for a `[[ ]]` operand (`[[ $x == a]] { … }`
parses; `a]b` absorbs its lone `]`). Inside a body, `)` is the closer.
`Scanner` gains `cmd_sub_depth` mirroring `arith_depth`. A second, separate
site: `Tok::RParen` was missing from the declaration break set that already had
`Tok::RBrace` for the identical `fn f { str x="a" }` failure.

Behaviour outside a body is untouched — `str s=a)b` is still the literal
`a)b`.

### Landed: SB5025 no longer advises a program it rejects

"snapshot into an array with `argv [N] a`" rendered a metavariable as code;
taking it literally is `av.f3`, which the compiler refuses. Now leads with
`argv a`, which compiles as written, and spells the optional start bare to
match the `$(arg N)` in the same sentence.

### Closed on review — three filings were already fixed

`plan.md` had drifted badly. `bug-function-call-redirect-dropped` (**P1**) was
fixed in `ad6be39` on 2026-09-19 with 12 specs and never closed — every gate
re-measured against the live binary before closing.
`bug-getenv-status-unvalidated` was fixed 2026-09-12. The ICE half of
`sb5025-placeholder-reads-as-syntax` was fixed with it. **Reproduce before you
fix**; the queue is a snapshot and nothing ages it.

### Gates

Sweep **6683**, predicted exactly. Zero failing, zero flips. `mk test` green.
`csc.1`-`csc.10` + `av.f11`, two disjoint neuters; csc.8/9/10 are deliberate
regression guards (DD#71 absorb unchanged; real SB4057 still reachable).

Journal 433.

## ══ STATUS (2026-09-28) — the lift was not total; `:-` is now lazy ══

Working the coverage queue head (`lower_value_as_str_aggregate` — six cold
lines, all deliberate ICE guards) into the useful question, "is any of these
arms actually reachable", turned up a live one.

`lift_int_func_calls` materializes every nested `$(f)` capture into a temp,
because an unlifted `FuncCall` reaching codegen is an ICE by construction. It
covered six of `SsaValue`'s thirty-one variants and ended `other => other,
// no IntFuncCall inside`. That comment was the bug: `other` was fifteen more
variants, most of which own sub-values.

```sh
array a=( $(f) )        map m=([k]=$(f))        str z=${arr[$(f)]}
str z=${y:-$(f)}        a[0]=${y:-$(f)}
```

All valid bash; all crashed the compiler. `str z=$(f)` — the same capture not
nested inside anything — worked, which is why it survived. The walk is now
**total over `SsaValue` with no wildcard** (cc 26 / cog 1, dispatcher
exemption), and `handle_subscript_assign` — which never called the lift at all
— now lifts both operands.

**The third non-total match in three sessions** (journal 430's walker pair,
431's `Trusted` wrap, this). The convention holds: make it total, say so in the
doc comment, let the compiler enforce it.

### DG's call: `:-` lowers lazily

`DefaultValue` was not mechanical, because `:-` is lazy — bash evaluates the
operand only when the fallback fires. Recursing eagerly hoists the capture and
runs `f` unconditionally. DG chose **lower lazily** over lift-eagerly or
reject.

**DD#78 had already chosen eager, and its premise was wrong.** It argues eager
is safe because every legal default is side-effect-free — then lists **user
functions** among the allowed defaults. A user function is not side-effect-free
in any enforced sense. Nobody noticed because the compiler *ICEd* on exactly
that construct: the allowance was dead letter, and no spec in the suite
exercised it. Amended, not reversed — captureless defaults stay eager (every
operand the original reasoning contemplated), only a capture-bearing one gets
control flow. Built in transform, so neither backend grew a special case and
DD#78's "avoids per-backend branching" cost is not paid.

### Gates

Sweep predicted 6661, returned 6661 — zero failing, zero flips. `lift.1`-`.12`
in `spec/functions.sh`, each verified against bash first, each landed by one of
six disjoint neuters. `lift.6` is the sole laziness witness and must be: with
the operand hoisted it still prints `[set]`, so only the untaken `f`'s stderr
catches it.

### Filed, not fixed

`${x:?MESSAGE}` is emitted as **literal source text** — sash prints `$(f)`,
bash prints `hi`. DD#78 names the message as a checked read position; it is not
one (`${y:?$h}` with `h` tainted gives **SB5050 dead-variable**, not SB5020 —
the operand is not in the value graph at all). Not a taint leak, since nothing
is expanded and so nothing escapes. → `notes/tasks/bug-require-message-not-interpolated.md`

Also checked and **correct, leave alone**: the many `DD#77` comments on
`RequireValue`. DD#77 §"`:?` un-prune scope" re-added scalar `:?` after DD#65
pruned it; DD#65's "rejected for SCALAR" line is the stale one.

Journal 432.

## ══ STATUS (2026-09-27) — DD#91: `!` is scoped to its own read ══

DG answered the question journal 430 filed rather than prejudged:

> "x=$(getenv A); y=$(getenv B); z=$x:!-$y:! ; the `!` should only apply to the
> read of the variable's stored value"

**The ruling made his own example a bug report.** `parameter_expansion_to_value`
wrapped the subject in `SsaValue::Trusted` only in the no-operator arm; the
moment there was an operator, `source_carrying_value` built `DefaultValue` and
never looked at `taint_off`. So the `!` of **every** `${x:!-…}` reached neither
backend nor backstop, and even the fully-vouched `${x:!-${y:!}}` failed to
compile — on the unspanned SSA backstop.

Three validate sites had the mirror-image fault, treating `taint_off` as
covering the whole expansion including its default: `is_taint_source`
(`!pe.taint_off &&`), `pexp_uses_tainted` and `check_param_expansion_taint`
(early returns that skipped the `:-` operand walk too).

A fourth was self-inflicted: `source_is_int` matches `Var`/`Subscript`
**structurally**, so the new wrapper silently moved `${n:!-7}` off the int
identity-element path. `!` is a taint annotation, not a change of type — it now
sees through `Trusted`. A set non-zero int cannot detect this (both paths return
42); the probe has to be `n=0`.

**Three of five neuters landed nothing on the first pass.** N2 was the vacuous
42. N3/N4 were masked by N5 — in scalar position `check_param_expansion_taint`
rejects first, so the `reject_tainted_into_array` walker pair needed collection
position (`array a=( ${h:!-$p} )`), and `is_taint_source` needed a *source*
rather than a tainted variable because the `||` short-circuits. All five now
land disjoint specs.

**DD#90 rule 5 left as-is, on DG's call.** `$x:!-$y:!` still does not work: the
inner `$y:!` inside an operand is `$y` + literal `:!`, because the operand is an
interpolating parts list. Asked whether to extend brace-less into the operand,
DG: *"I'd actually use attest, so avoid over-indexing on taint-bypass
ergonomics."* Now pinned deliberately — vm.bl24 asserts the silent `abc:!`,
vm.bl25 the braced `abc`, as a pair.

Sweep predicted 6637, got 6637. Zero failing, zero flips. The 20-id retirement
the sweep announced was expected: the block was renumbered 61.13-61.22 →
91.24-91.33, since it tests what `!` scopes over, not the DD#61 walker pair.

DD#91 addendum, DD#90 rule-5 extension, journal 431, task
`dd91-bang-vouches-for-source-in-default` closed.

**coverage.db is stale** (validate.rs and transform/expressions.rs renumbered).

## ══ STATUS (2026-09-27) — TAINT ESCAPE closed (DD#61) ══

Chasing a coverage queue item (`test_uses_tainted`, 7 cold lines) found an
exploitable taint escape. Journal 430.

**The bug.** SB5021 is decided by `is_taint_source(e) || expr_uses_tainted(e,
tainted)` — two walkers read as a pair, which between them must cover the
whole tree. `expr_uses_tainted` was made total in journal 400 and carries a
long comment saying a wildcard leaked three times. **Its twin kept the
`_ => false` that comment is about.** Wherever the twin descended and this did
not, a *source* was invisible to both: the first never arrived, the second
arrived asking a different question.

```sh
str x=""
array a=(q)
a[0]=${x:-$(</tmp/secret)}     # compiled CLEAN — no SB5021, no backstop
echo "${a[0]}"                 # …and printed the file
```

Also `$(getenv …)` and external captures; also via `[[ ]]` read-back, which
journal 400 already recorded escapes the SSA backstop entirely. A tainted
*variable* in the identical position (`${x:-$t}`) was SB5021 throughout —
which is what made it a hole rather than a policy.

**A second hole**, found by the differential: `str z=${b[$(getenv N)]}` was
clean while `echo "${b[$(getenv N)]}"` was SB5020. `reject_scalar_rhs_read`'s
subscript arm says *"the container AND the index are both reads"* and then
checked the index with the **value** walker. The code contradicted its comment.

**Fixed** by making all three walkers in the family total —
`is_taint_source`, `check_value_cmdsub_reads` (it had the same `_ => {}`), and
the index arm of `reject_scalar_rhs_read`. `Subscript` stays false in
`is_taint_source` on purpose (the index is a read, not the stored value).
`Test` stays a correct recursive answer rather than an ICE — unlike journal
429's `:~` arm, nothing *guarantees* its deadness, and this bug is the proof
that such arms become reachable.

Gate: predicted 6613, swept 6613, zero failing, zero flips, 24 new ids. Three
disjoint neuters landing 7 / 1 / 1. Specs `61.1`-`61.12`, three of them
controls.

**Open for DG:** does DD#91's `!` vouch for a *source* materialised in a `:-`
default (`${x:!-$(getenv HOME)}`)? Today it is rejected only by the bare SSA
backstop, because the IR layer honours `taint_off` and the SSA layer does not
know about `!`. The fix deliberately preserved that rather than prejudge it —
`notes/tasks/dd91-bang-vouches-for-source-in-default.md`.

## ══ STATUS (2026-09-27) — coverage queue: disposition_proof cleared ══

First item off the re-ranked queue. 11 cold lines, three match arms, three
different answers — all reached by panic probe, never by reading call sites.

**A bug.** `${p:0:2}` as a redirect target produced *`redirect target
`${psubstring…}` may be empty — `substring` does not prove a filename`*. The
parser stores that operator under the internal NAME "substring"
(`parse/expr.rs:1344`), and the diagnostic interpolated it, inventing syntax
no author wrote or could write. The operator vocabulary makes it general:
`cell_string_op`, `glob_replace`, `s_malformed`, `trust_malformed`,
`arrvals_*` are all names. Fixed with `operator_noun`, following the existing
house style at `parser.rs:3089` which describes `cell_string_op` rather than
echoing it.

**A spec gap.** `14.55` rejects a bare `$p` — but that is a `SimpleExpansion`
reaching `plain_read_proof` from a different call site. Only the braced
`${p}` takes the arm. Two spellings, two paths, one spec. Now `14.68`.

**An unreachable arm.** `:~` never arrives: an elide is legal only in a
variadic position (DD#91), a redirect target never is, and SB5046 rejects all
eight probed spellings a pass earlier. Converted to an ICE per DG's rule
(journal 375), with `elide_redirect_target_is_loud_ice`. An ICE is right
rather than merely conventional here: the arm is reachable only if SB5046
silently stopped applying, which is a compiler bug, not a user error. The
third neuter proved it — disabling SB5046 produced the predicted ICE in the
predicted words. Spec `14.71` pins the precondition so a spec fails before a
user meets the ICE.

Gate: predicted 6589, swept 6589, zero failing, zero flips, 8 new ids.
Three neuters, one failure each, each the right one. Journal 429.

**Next on the queue:** `test_uses_tainted` (7, validate.rs) — note `e8bc470`
found its sibling `expr_uses_tainted` was not total.

## ══ STATUS (2026-09-27) — coverage: function number was wrong by 36 points ══

Answering "what is current coverage" surfaced a bug in `contrib/covdb.py`.
`cov_lines` and `cov_branches` are keyed on things that mean the same in every
build, so they merge correctly across the two builds spliced into
`union.lcov`. `cov_fns` was keyed on `(file, mangled)` — and a Rust v0 mangled
name embeds the **crate disambiguator hash**, which differs between the spec
binary and the unit-test harness. Every function got two rows, one per build,
and the build that did not reach it contributed a permanent zero.

`emit_arith_op` read `count=0` while its own lines were 21/23 covered. Fixed
by stripping the build hashes before keying. **Deliberately not** keyed on
`(file, line)` like `cov_lines`: that gives a better-looking 98.7% and is
wrong, because closures share a start line with their parent and ~574 symbols
would merge — the same class of error, in the flattering direction.

**Only wrong since 2026-09-24, and by a commit that never touched the
counting.** `covdb.py` was introduced defaulting to the *spec-only*
`branch.lcov` — one build, one crate hash, correct. `b4981d7` switched the
default to `union.lcov` for a good reason (the 100% target is spec ∪ unit),
did not touch the keying, and thereby broke it. A key that had been unique
became per-build. Every figure we have ever *quoted* — 98.45% back to
95.03% — came from `cargo llvm-cov report`'s own summary via `mk cov`, which
reads the covmap and never an lcov, so the two numbers share no code path.

It survived because nothing consumes `cov_fns`; the documented triage query
ranks uncovered *lines* by function and was always correct. A number with no
consumer is a number with no test.

Union coverage of `sash/src` (Sep-25 lcov, **two days stale** — 16 measured
files have changed and `coverage.db` records no commit):

| metric | recorded | uncovered | covered |
|---|---|---|---|
| lines | 27538 | 519 | 98.12% |
| branches | 3912 | 333 | 91.49% |
| functions | 2928 | 113 | **96.14%** (was reported 59.57%) |

Journal 428.

## ══ STATUS (2026-09-27) — sash-convert no longer passes bad input silently ══

**A conversion downgrade the user was never told about.** tree-sitter-bash
0.25 cannot parse a glued `=` inside single brackets: `[ a=b ]` reads as an
assignment prefix plus a command named `]`, so no `test_command` node exists,
`convert_test_command` never runs, and the text is emitted verbatim. That
output then **compiles** — sash has no `[` builtin, so DD#23 resolves `[` to
the external `/usr/bin/[`. A bash test became a forked coreutils invocation
with nothing said. Nothing unsound (the manifest discloses the external), but
a downgrade has to be disclosed rather than inferred.

`convert.rs`'s `ERROR` arm handled exactly one grammar hole (`${a[a[0]]}`,
DD#80) and fell through silently for everything else. It now emits
`# MANUAL: could not parse …` for any ERROR it does not recognise.

Two details beyond the one-liner: **only the outermost ERROR is reported**
(tree-sitter fragments a bad region into nested ERRORs — the inner spans are
artefacts of the failed parse), and **the guard suppresses the note, not the
recursion**, so a recognisable construct nested deeper inside a bad span is
still rewritten.

Journal 427. Gate: predicted 6581, swept 6581, zero failing, zero flips, 5 new
ids (`57.173`-`57.177`, native-only — `convert.sh` does not run in c mode).
Controls: `57.174` (same test without the glued `=` converts and is NOT
reported) and `57.176` (the DD#80 ERROR is still recognised and unreported,
pinning that the generic note did not swallow the specific handler). `57.175`
**counts** the notes, the only form that detects the nested duplicate.

The `[ a=b ]`-specific rewrite was deliberately not done — the task made it
conditional on the generic warning proving noisy, and the whole suite produced
zero new notes.

Filed P4: `walk-node-complexity` — `walk_node` is cc 39 / cog 44 against a
`sash/src` mean near 3.8 / 1.9, exempt from the gate only because the gate
scopes to `sash/src/`. Open question for DG in that file: whether it should.

## ══ STATUS (2026-09-27) — DD#120: the invoker's channel ══

**A redirect written on a command is a channel for that command's output, and
never captures the invoker's.** DG's framing: "the setup belongs to the
program invoking cmd, and is not intended to consume the output of the
invoker." Every diagnostic sash emits is the invoker speaking about its own
failure — setup failure, exec failure, the DD#55 status abort, attest,
arithmetic aborts — so none of them can land in `cmd 2> f`. To capture them,
redirect the invoker: `exec 2>> errlog`.

This is a **deliberate divergence from every POSIX shell** (bash, dash and
busybox all route these through the already-applied redirects). That
behaviour is emergent, not designed — the tell is that `cmd 2>capt >bad` and
`cmd >bad 2>capt` put the message in different files. Under DD#120 the order
dependence disappears.

**Mechanism: one object, not a rule at twelve emit sites.** The runtime holds
`SASH_SCOPE_SAVE` — the invoker's own 0/1/2, `F_DUPFD_CLOEXEC` copies taken in
`sash_startup` (which both backends reach via `_start`, so neither needed a
prologue change). `sash_scope_bind()` re-takes them after a persistent `exec`
redirect; `sash_scope_restore()` puts them back on entry to END. CLOEXEC is
load-bearing: the fd survives a *failed* `execve` so the child can report, and
vanishes on a successful one.

**It also fixed a live bug in both backends.** `END { echo cleanup }` after
`/bin/false > out.txt` wrote `cleanup` **into `out.txt`** — the abort
terminates the block before the command's restore, stranding it on an
unreachable edge. Three abort paths leaked (status, exec failure, echo write
failure); only setup failure was safe, and only by accident. DG's question —
"if the diag fd exists, why does END need anything else?" — is what turned a
fd-2 diagnostic channel into the 0/1/2 scope set that fixes all three at
once, rather than a per-site restore rule that this bug proves gets
forgotten.

Journal 426. Gate: predicted 6576, swept 6576, zero failing, zero flips, 18
new ids (`iv.1`-`iv.9` × 2 modes); `rd.5`/`rd.7`/`rd.8` rewritten to the new
model. Every new check is paired with a control — `iv.3` (a running command's
stderr still reaches the redirect), `iv.5` (same diagnostic, same fd, only
the scope differs) and `iv.9` (END follows a persistent `exec >`, so the
restore is a restore and not a reset to the startup fds).

## ══ STATUS (2026-09-27) — redirect diagnostic fixed in BOTH backends ══

**A bug the differential oracle was structurally unable to see.** DD#55's
redirect setup-failure diagnostic must be written through the redirects that
already succeeded — bash puts `cmd 2> capt.txt > /bad`'s message *in*
`capt.txt`. native wrote it after `emit_restore`, so it went to the terminal.

Fixing that turned up **a second copy of the same inversion in the C
backend**, on the function-redirect scope path (`emit_redirect_push`). C's
`emit_redirect_guarded_body` had the order right, so `emit_c.rs` held two
functions doing one job in opposite orders — and the wrong one was paired
against a native function that was also wrong.

**That is why it survived: the oracle compares the backends to each other.**
Two identical wrong answers look exactly like two right ones. The command
form was caught (journal 404) because the backends *disagreed*; the scope
form, one line away, did not. The oracle localises a bug to one backend; it
cannot see a bug in the specification both were written from. Only bash can,
and only when someone writes the differential by hand.

Fix: `emit_open_failure_abort` split into `emit_open_failure_msg` +
`emit_open_failure_terminate`, with `emit_restore` between them — the three
functions had been one, which is why the restore could not simply be moved.

Specs `rd.5`-`rd.8` (`errors.sh`). `rd.6` is the ordering control that makes
`rd.5` mean anything. Neuter is diagnostic, not just non-vacuous: the native
inversion fails rd.5/rd.7/rd.8, the C inversion fails **rd.7 only**.

Not a bug, checked first: silence for a failure caught by `if`/`&&`/`||` is
DD#55 (55.25/55.50).

**Next up:** `convert-silent-passthrough-on-parse-error` (P2) — sash-convert
emits unparseable input verbatim with no `# MANUAL:` note, so a bash `[ a=b ]`
silently becomes a forked `/usr/bin/[` instead of a `[[ ]]`.

Gate: predicted 6558, swept 6558, zero failing, zero flips, eight new ids.
`mk test` 107/0. Clippy clean.

## ══ STATUS (2026-09-26) — wildcard audit closed; three verdicts given ══

**The `audit-validate-wildcard-escapes` charter is complete.** Both cohorts
swept (11 stmt-level containers, 91 expression-level squares), one live bug
found and fixed (journal 423's taint oracle), and the three verdicts the
charter owed are now given. Journal 424.

1. **`walk_arith_expansion_roots`'s `_ => {}` was sound** — the five
   variants it swallowed carry no `Expr`, checked against the total
   `children()`. Made total anyway: it has **three** callers now and all
   three are taint checks, so a later variant with an `Expr` inside would
   skip taint in three places at once. cc/cog unchanged at 11/3.

2. **The bare `ArithExpr::Var`/`DollarVar` SB4030 shield holds** on all six
   probed routes, and is now pinned by `atint.17`-`atint.21` rather than by
   a code comment.

3. **`StmtKind::Case` is NOT dead** — it is the DD#66 dispatch form
   `[[ value { pat { … } } ]]`. SB4029 rejects the bash *keyword*, which has
   no parser production. The struct's doc comment said so already; the
   charter's suspicion was mine and it was wrong.

**Two of the five new specs nearly shipped vacuous.** `atint.17`/`.18`
assert SB4030 for a tainted `$(( x ))` — but the clean `x=7` gives the
identical SB4030, so they would pass with no taint anywhere. Caught by
running the clean counterpart of each. They are kept (nothing else in the
suite pins the bare-name prohibition at all) with comments that say plainly
they are not taint discriminators. The other three are real: `int n=7`
compiles, and `array arr=(7); int n=${arr[0]}` is SB4030 where the tainted
form is SB5020.

**Running a spec is not the check — running its clean counterpart is.**
Second time in three sessions. One extra invocation.

**Next up:** `check_value_cmdsub_reads` is the last `_ => {}` in this
cohort (5 of 13 `ExprKind` variants). Not exploitable today, but cc 7 /
cog 5 means no dispatcher exemption and the 3x cog bar is 5.58 — it needs a
shaped change, not a wildcard swap.

Gate: predicted 6550, swept 6550, zero failing, zero flips, ten new ids.
`mk test` 107/0. Clippy clean.

## ══ STATUS (2026-09-26) — taint oracle closed; expression cohort swept ══

**A working taint oracle, found by the wildcard audit and now closed.**

```sh
str h=$(getenv SECRET)
array z=($(( ${h:-0} )))
if [[ ${z[0]} == 4242 ]] { echo MATCH } else { echo NO }
```

Compiled clean, ran, and reported whether the secret matched — no
diagnostic anywhere. `z` was never entered into the tainted set, because
`expr_uses_tainted` (the predicate behind SB5021) ended `_ => false` and
that swallowed `ExprKind::Arithmetic`.

**It is the same enum variant journal 400 already fixed** — in
`check_expr_taint`, a *different* walker on the same path.
`check_expr_taint` deliberately delegates collection elements to
`reject_tainted_into_array`, which is sound, and which means making one
walker total did nothing for its sibling. That is now written into the
function's own doc comment.

Why a bypass rather than a late catch: `echo "${z[0]}"` was stopped by the
SSA backstop (a bare sentence, no SB code, no span), but a `[[ ]]` operand
is not a command argument. Checked and ruled out as a second bug — a
genuinely tainted array (`read arr`) is SB5020 in that same position, so
the validator covers it; the only reason this ran is that `z` was unmarked.

`expr_uses_tainted` is now **total over `ExprKind`**, with `Arithmetic`,
`SlurpFile` and `Test` arms plus an explicit `CommandSubstitution => false`
that records why (a read inside `$( )` is SB5020, a use site, not SB5021).
cc 14 against a 3x bar of 11.52, passing on the cog <= 2 dispatcher
exemption. Specs `atint.13`-`atint.16`, sitting directly after journal
400's `atint.11` because the adjacency is the finding. `atint.16` guards
the other direction: clean arithmetic into a collection must still compute.

**The expression-level cohort is swept: 13 containers x 7 violating
expressions, 91 squares, one live bug and 90 clean.** With the stmt-level
cohort clean from the previous session, the audit's main body is done.
Journal 423.

Left filed rather than done, with reasons, in
`notes/tasks/audit-validate-wildcard-escapes.md`:
`check_value_cmdsub_reads` still has `_ => {}` (probed — every uncovered
variant is closed by an earlier rule, but it is cc 7 / **cog 5**, so it
gets no dispatcher exemption and wants its own shaped change);
`walk_arith_expansion_roots`'s wildcard and bare `ArithExpr::Var` taint
still owe a verdict; and whether `StmtKind::Case` is dead now that `case`
is removed.

Sweep **6540**, predicted 6540, zero failing, zero flips.

## ══ PREVIOUS (2026-09-26) — one command-substitution parser, not two ══

**The P1 wildcard audit found its first bug one layer up, in the parser.**
A double-quoted string had its **own** command-substitution parser. It had
drifted from the real one in the two ways two implementations of one thing
always drift, and both were user-reachable:

- **No `<` probe.** `"$(<f)"` became a command substitution with an *empty
  body*, so `echo "$(</etc/hostname)"` printed nothing where bash prints
  the file.
- **No way to carry a diagnostic.** Declared `-> Vec<Statement>`, it ended
  `Err(_) => break`. `parse_statement` reported the error correctly every
  time; the caller discarded it. `"$(if)"` compiled silently and
  `"$(echo a; if)"` compiled, exited 0, and dropped its tail.

**The taint bypass is the part that matters.** Unquoted `$(<f)` is SB5020 —
file contents are external input. With an empty body there was nothing to
taint, so `str v="$(<f)"` laundered tainted content into a clean-looking
variable. Two quote characters, straight through the taint model.

**DG settled it by principle, not by case:** interpolation and concatenation
of tainted content both model as a read, and a read of a tainted value can
only happen in `attest`. So a quoted slurp is a real `SlurpFile`, tainted,
identical to the unquoted spelling — and the fix is to stop maintaining a
second parser (option C), not to patch two symptoms. `command_sub_to_expr`
now delegates to `parse_command_substitution`; `parse_sub_script` is gone.
The precedent was already in the same file: the `ArithSub` arm beside the
call site was fixed for the identical bug in `$(( ))`.

**`cov.205` was covering nothing and had to go.** It asserted that
`"$(int x=1)"` compiles clean, on the stated grounds that the quoted path
reached a guard the unquoted one did not. The quoted path had no such probe
at all, and the clean compile it pinned *was the swallowed parse error*. A
spec whose passing depends on a bug converts the bug into something the
suite defends. Replaced with a body that reaches the guard structurally
(`if` lexes to `Tok::If`) and asserts a value rather than silence.

Four new specs (`sl.12`, `sl.13`, `cap.22`, `cap.23`, +8 dual-mode).
Neutering the delegation fails exactly those four. Sweep **6532**, predicted
6532, zero failing, zero flips. Journal 422.

**Next: resume the audit at the expression-level cohort.** The stmt-level
cohort is clean across eleven containers; the expression walkers are
unswept, and `SlurpFile` / `Arithmetic` are the recurring blind spots. The
resumption notes, the container list and the harness traps are in
`notes/tasks/audit-validate-wildcard-escapes.md`.

Filed, not fixed: `$(int x=1)` reports SB4057 "unterminated `$(`", which
misdirects. Pre-existing on the unquoted path, verified at `8f7052a`.

## ══ PREVIOUS (2026-09-26) — DD#111 complete: no magic variable survives the parser ══

**`$?` → `$(status)` is in**, and with it DD#111's desugar is finished. All
three punctuation forms — `$$`, `$!`, `$?` — are gone by the time the parser
returns; downstream sees `$(getpid)`, `$(getcpid)`, `$(status)` and nothing
else.

**`bareword_shortcut_expansion` is deleted outright.** It folded `$(status)`
*into* `$?`, which is precisely the direction DD#111 reverses; `status` was
its last arm. `intrinsic_int`'s `?` seed went too, on the same structural
argument that retired `!`: `assert_no_magic_sugar` forbids a surviving
`SimpleExpansion("?")`, and `${?}` is rejected by SB4034. Ran it to check.

**The reason this one went last: it carried SB5012.** `find_dollar_question`
and `stmt_uses_status` were two identical walks matching
`SimpleExpansion("?")` and nothing else. Their doc claimed to cover
`$(status)` too — true only because the parser folded it into that node.
Reversing the fold without moving them would have left both finding nothing:
**SB5012 would simply never fire again, with a fully green sweep**, because a
rule that never fires breaks no test that does not expect it to. Both now use
`ir::intrinsic_call_name`, collapsed to one walk (`stmt_uses_status` is
`!spans.is_empty()` over the span collector). `intrinsic_expansion_round_trips`
pins the recogniser to its constructor across the module boundary.

**Three neuters, and the third found a real gap.** Neutering the detector
fails 6 specs; neutering the desugar arm fails 5. Neutering `status` out of
`is_int_returning_intrinsic` failed **nothing** — yet it breaks `int x=$?`,
`exit $?` and `wait $?` with SB4030. The int-ness of the most-used magic
variable, at exactly the sinks DG scoped to int-only, was pinned by nothing.
`isink.11` guards `$!`/`wait` and reads as if it covered the family; it does
not. Added **`isink.13`/`.14`/`.15`** (+6, dual-mode). Their own proof was
accidental and complete — they failed against a binary that still had the
neuter compiled in, and passed once it was rebuilt.

**A standalone `convert.sh` reported 62 failures that were pure artifact.**
`common.sh` *appends* `target/release` to `PATH`, so `sash-convert` resolves
to the installed copy — twelve days stale — and `SASH=` does not override it,
being a different binary. The same 62 showed up under a neuter, where they
read as signal. AGENTS.md now says to prepend always, and that a standalone
failure count is not a finding until it survives that.

**Gate:** predicted 6524, swept 6524, zero failing, zero flips, exactly the
six new ids, probe 231. `mk test` and clippy clean.

## ══ STATUS (2026-09-26) — `$!` desugared; the regression check can fail again ══

**`$!` → `$(getcpid)` is in** — the second of DD#111's three, and unlike `$$`
it is **behavior-preserving by construction**. There was no bug to fix,
because `a875d58` had already fixed the symptom by hand: it seeded
`intrinsic_int` with `!` so the punctuation form would agree with the
intrinsic it is sugar for. That worked and left two encodings of one fact.
This removes the second rather than maintaining it.

Two folds ran in the wrong direction and are gone —
`bareword_shortcut_expansion`'s `getcpid` arm and `parse_getcpid_word`'s
`SimpleExpansion("!")`. Both used to reach `SsaValue::Var("!")` via the parser
fold; both now reach it via `resolve_single_cmdsubst` → `nullary_magic_var`.

**The `intrinsic_int` `!` seed is deleted**, and the argument is structural,
not "I looked and found no callers": `var_type` is only reached for a magic
name through `SimpleExpansion`, which `assert_no_magic_sugar` now forbids; and
the only other spelling, `${!}`, is rejected by SB4003 before typing runs.
Checked the second half by running it.

**Neuter: 4 specs across 3 files** — `cov.252b`, `55.19`, `isink.11`, `spv.4`.
The informative result is the one that did *not* fail: `sx.25` in
`arithmetic.sh` passes with the desugar neutered, because inside `$(( ))` a
string operand coerces anyway. It reads like a guard on the two spellings
agreeing and it is not one. Left alone, but recorded.

**No new specs, deliberately.** The four above already pin everything this
change could break and it adds no observable behavior; mirroring the `$$`
block would have inflated the total with ids no neuter can distinguish.

**AGENTS.md's regression check can fail again** (the vacuity found in journal
419). `commit_hash` is not a sweep identity — `runs` stores the git HEAD sha,
so every sweep against a dirty tree records the same one and
`MAX(rowid) … WHERE commit_hash='BASE'` hands you the run you just did. The
replacement compares by **row order**: the baseline is `rowid < cur.rid`,
strictly earlier, so it cannot self-compare however dirty the tree is. Added
the cheap tell to run first (a hash with more rows than distinct ids covers
several sweeps — keep the `* 1.0`, integer division called four sweeps `3`)
and a **non-vacuity probe** that must report hundreds (231 today). Every SQL
block was executed against `spec.db` before committing, and the new check was
then used as this commit's gate.

Sweep **6518**, unchanged and predicted; `flips=0 new=0 total=6518`.
`mk test` and clippy clean. Journals 419, 420.

**Next: `$?` → `$(status)`**, the last and largest. The trap is the whole
risk: `validate::find_dollar_question` (2744) and `stmt_uses_status` (2754)
match `SimpleExpansion("?")` and nothing else. Their doc comment claims to
cover `$(status)` — true only because `bareword_shortcut_expansion` folds it
into that node, and that fold is now the **only arm left** in that function.
Reverse the direction without moving them and SB5012's availability state
machine goes silently vacuous. Same commit, with a neuter proving SB5012 still
fires. Confirm first what `$(status args…)` means — `nullary_magic_var`
declines it on `args.is_empty()`, but nobody has established what the
non-empty form does. Then `intrinsic_int`'s `?` seed goes the way of `!`; `#`
stays.

Also owed: rebuild `coverage.db` (`mk cov-union` → `mk cov-db` → `mk cov-reset`
→ `mk build`) before resuming the coverage queue; `plan.md` at repo root is
stale.

## ══ STATUS (2026-09-26) — `$$` is punctuation sugar ══

**The magic-variable directive has started, `$$` first.** DD#111 §Why already
states the rule — *"Variables are not magic in sash… `$?` is punctuation sugar
for the `status` intrinsic. The intrinsic is the real thing."* — and the
compiler believed the opposite: the parser **folded** `$(getpid)` into a
`SimpleExpansion("$")`. The sugar was the real thing and the intrinsic
collapsed into it. That direction is now reversed for `$$`.

**I was wrong to offer DG a "defer" option** and said so. DD#111 is a stated
design rule, so step 1 of the decision tree applies (settled by a source of
truth) and applies to all three magic variables. The genuine forks were only
mechanism and sequencing; DG ruled **B + assertion**, **unify = true**,
**incremental**.

**DD#119 had already cleared the road.** Its "How it started" records the
*previous* attempt at this desugar and the blocker that killed it
(`> $$` tripping SB4049); DD#119 removed that blocker three commits ago, and
line 60 lists intrinsics as valid redirect targets.

**A typing bug fixed structurally.** `int a=$(getpid)` compiled; `int b=$$`
was SB4030 "cannot assign string value to int variable" — two spellings of one
value with two types, identical in kind to the `$!` bug `a875d58` patched by
seeding `intrinsic_int`. After the desugar there is no `$$` node left to
mistype. Deliberate consequence: `str c=$$` now rejects and directs you to
quote it; `str d="$$"` works. Same migration `cov.252b` took in `a875d58`;
`cov.252a` takes it here.

**One table, because they had already drifted.** `resolve_nullary_sexpr` knew
`getpid`→`$`/`status`→`?`/`getcpid`→`!` but only on the arith sexpr path; the
word form reached the same slot via the parser fold. Now
`magic_intrinsic_var`, read by both. Swept **in isolation** first (6510, zero
failing) so the next sweep's diff was attributable.

**The assertion is what makes option B safe.** `parse()` ends with
`assert_no_magic_sugar`, which ICEs on any surviving `SimpleExpansion` of
`$`/`?`/`!`. Four of eight construction sites can receive a magic name and all
four are routed; the other four are excluded **by construction**, not by
inspection.

**The flip check was vacuous — flagged to DG, not patched.** AGENTS.md's
regression recipe picks the baseline with `MAX(rowid)` at a `commit_hash`.
With a dirty tree, three sweeps shared `cc0b7a9`, so it selected *this* run
and compared it to itself — zero flips, and it could not have reported
anything else. The tell was 19538 rows at one hash (6510 + 6510 + 6518).
Re-run rowid-bounded it passes honestly. **A gate that cannot fail is not a
gate**; the recipe lives in AGENTS.md and the fix is DG's call.

Sweep **6518** — predicted exactly (6510 + 4 dual-mode specs). Zero failing,
zero retirements; vs the first `cc0b7a9` sweep: 0 flips, 8 new, 0 gone.
`mk test` and clippy clean. Both mechanisms neuter-proved; `spv.7` survives
the desugar neuter and is **labelled in place as a non-proof**. Journal 419.

**Next, in DG's order:**

1. **`$!` → `$(getcpid)`** — add the `"!"` arm to `magic_sugar_expansion`;
   delete the `getcpid` arm of `bareword_shortcut_expansion` and
   `parse_getcpid_word`'s `SpecialWord::Returned` fold. Lower risk.
2. **`$?` → `$(status)`** — the largest step, and it carries the trap:
   SB5012's availability state machine finds reads via `find_dollar_question`
   (`validate.rs:2744`) and `stmt_uses_status` (2754), and **both match only
   `SimpleExpansion("?")`**. Their doc comment claims to cover `$(status)` —
   true only because the parser folds it into that node. Reverse the direction
   without moving them and the state machine goes **silently vacuous**. Same
   commit, with a neuter proving SB5012 still fires. Confirm first what
   `$(status args…)` does.
3. **`drop-intrinsic-int`** — `TypeEnv::new`'s `intrinsic_int` `?` seed goes
   dead once `?` is an intrinsic. Confirm `#` still needs it.

Also owed: rebuild `coverage.db` (`mk cov-union` → `mk cov-db` → `mk cov-reset`
→ `mk build`) before resuming the coverage queue; `plan.md` at repo root is
stale and predates all of this work.

## ══ STATUS (2026-09-26) — `-t` is retired ══

**DG's call: `-t` ceases to be a `[[ ]]` test.** It joins `-v`, `-o` and `-R`
in `unsupported_unary_test_hint` — SB5028 at validate. `-t` **stays** in the
parser's `is_unary_test_op`, deliberately: drop it there and `[[ -t 1 ]]`
degrades into two words and reports SB4014 instead. Parser permissive,
validate authoritative.

It was the odd one out on a real axis. The `[[ ]]` unary family is string
shape (`-n`/`-z`) and file predicates; every one takes a string. `-t` took an
**fd number** — the sole int-typed unary test — and `emit_isatty_test` was the
only unary arm that had to `atoi` its operand first. It also asks an
interactivity question, which a compiled static binary has little business
asking.

**`terminal?` is dropped as YAGNI (DG).** Supporting fact: sash arithmetic has
**no** unary predicates at all (`IrUnaryArithOp` = Neg/Pos/Not/BitNot; only
`!` is a predicate; every sexpr intrinsic is value-producing), so it would
have been the first of its kind — in a language whose predicate context
(`[[ ]]`) it would have been declining to use.

**The removal is total**, down through both backends and the runtime: the
`tests.rs` dispatch arm and `emit_isatty_test`, the `setup.rs` extern
declaration, `ExternFuncs::isatty`, the `emit_c.rs` arm and its
`extern i64 sash_isatty(i64)`, and `sash_isatty` itself in `sash-rt`. rustc
found the edge: deleting the dispatch arm left `atoi_call` unused in
`tests.rs`, which is "it was the only int-typed test" arriving from the other
direction.

**The neuter proved a long-standing claim.** `unsupported_unary_test_hint`'s
doc comment has said these operators "must be rejected here (SB5028) before
codegen ICEs on them". Neutering the `-t` arm ICEs at `tests.rs:194`. First
demonstration; the claim was true.

`cov.89` retires with the mechanism it covered — predicted, and the sweep
announced exactly `c:cov.89` / `native:cov.89`. Net spec count unchanged
(one dual-mode out, one in: `62.22`), so the purge naming those two ids was
the sharp prediction rather than the total.

Sweep **6510** (native 3355, c 3142, skipped 13) — predicted exactly. Zero
failing, zero flips vs `a875d58`. `mk test` exit 0. Journal 418.

**DD#89 is closed** (same commit batch): its "Left open" section asked whether
`exit $s` / `return $s` should reject, and all five tracked sites now do —
**every one by rejection, none by coercion**. `$(arg $s)` was the most
dangerous and was recorded as the least known ("untested rather than
settled"): it did not coerce to 0 like its neighbours, it silently returned
**argv[0]**. `docs/sash.1`'s `-d`/`-f` collision table also still listed `-t`
as "is a terminal"; fixed.

**Next:** the parked magic-variable directive (`$$`→`$(getpid)`,
`$!`→`$(getcpid)`, `$?`→`$(status)`); amend DD#89's "Left open" (`exit $s`
now rejects); rebuild `coverage.db` before resuming the coverage queue.

## ══ STATUS (2026-09-26) — exit, wait and arg take a number ══

**DG's int-only directive is in.** `exit`, `wait` and `arg` accept int-type
only, via one shared check that is the exact mirror of
`check_string_intrinsic_operands` (that one rejects an int where a string is
required).

`arg` was the worst by a distance: `$(arg $s)` on a string silently yielded
**argv[0]** — no diagnostic, no trace. `exit $s` / `wait $s` coerced through
atoi to 0, a defensible wrong answer by comparison.

DG's three calls: **`exit ''''''` is rejected** (it read as 0, i.e. *success*, for a
script that meant to fail); **a quoted number is a string**, so `exit "1"` is
rejected — quoting is the int→string conversion everywhere else in sash and
cannot mean the opposite here; **signed literals stay legal**, `exit -1` → 255.
`$(( ${s:?bad} ))` is the accountable conversion and is strictly better than
the atoi it replaces: it aborts with the author'''s message instead of quietly
reporting success. Only slot 0 is a sink — `arg N default` keeps its string
default (isink.10).

**`$!` is now int-typed.** `wait $!` is *the* idiom and it broke: `$!` was
String while `$(getcpid)`, which `$!` is sugar for, was already int-returning.
`intrinsic_int` seeded `?` but not `!`, so `wait $(getcpid)` would have passed
a rule `wait $!` failed — two spellings of one value. The asymmetry was the
bug. It is also a step toward the parked magic-variable directive, and it is
what exposed the C renderer bug fixed in `b251d75`.

**The sequencing was DG'''s call and it mattered.** `arg` is only ever written
as `$(arg …)`, an expression — so wired the old way this check would have
caught `echo $(arg $s)` and missed `str t=$(arg $s)`, leaving the exact case
the directive exists to close open behind a green spec. Fixing the traversal
first, in its own commit (`6013848`), is why this change is small.

Sweep **6510** (native 3355, c 3142, skipped 13) = predicted exactly. Zero
failing, zero flips vs `b251d75`. Non-vacuity proved twice: neutering the
check fails the 5 rejection specs and leaves all 7 positives green; neutering
the `$!` seed alone fails 3 specs across two files.

**Next:**

1. **Retire `-t`** as a `[[ ]]` test, its own commit (DG). Add the arm to
   `validate.rs::unsupported_unary_test_hint` (SB5028), **keep** `-t` in
   `is_unary_test_op` so the code is SB5028 not SB4014, drop
   `emit_isatty_test` and the emit_c arm, migrate `cov.89`, fix the stale doc
   at `validate.rs:1993`.
2. **`terminal?` is dropped** as YAGNI (DG). For the record: sash arithmetic
   has no unary predicates at all, so it would have been the first of its
   kind, and `[[ ]]` is already the predicate context.
3. The parked **magic-variable** directive: `$$` → `$(getpid)`,
   `$!` → `$(getcpid)`, `$?` → `$(status)`. Only those three are magic.
4. **Amend DD#89** — its "Left open" section asked whether `exit $s` should
   reject. It now does; the section is owed an update.

Also owed: rebuild `coverage.db` (`mk cov-union` → `mk cov-db`) before the
coverage queue resumes; `plan.md` at repo root is stale.

## ══ STATUS (2026-09-26) — the second renderer ══

**DD#119's own recommended spelling did not work in the C backend.**
`int n=5; /bin/echo hi > "${n:?msg}"` — the quoted, dispositioned form the
int exemption directs authors to — emitted `n_ptr`/`n_len`, the string slots
of a `VarSlot`, for an `i64` that has neither. A hard **compile error** in the
generated C. Native was correct.

`load_var_or_empty_c` never consulted the variable's type. That is the same
fault as `latest_var_as_str`, fixed one commit earlier — and those are the
**only two** functions in the backend that render a slot as text. Fixing one
and not the other was the real mistake. They now share `var_slot_as_str_c`.

Surfaced only because `$!` became int-typed, so `cov.252b` finally exercised
int + disposition + redirect together and c-mode diverged from native. Third
find by the differential oracle this week.

**Two corrections to my own earlier report, both recorded in journal 416:**

- I said native was *also* broken, writing silently to stdout. It was not. I
  misread my own probe — the `hi` came from the `cat 5` at the end of an `&&`
  chain, not from the binary. Reporting a silent wrong answer that does not
  exist is worse than missing one; it sends the next session after a phantom.
- I called `int n=0; echo "${n:-7}"` printing `7` a bash divergence and a
  stop-everything. It is **deliberate**: DD#62's Model B amendment (DG,
  2026-07-08) establishes that sash has no unset state, so each type's zero
  value *is* its unset marker — `""` for a string, `0` for an int. Checking
  `notes/decisions/` before touching it is what prevented a regression into a
  deliberate design decision.

Sweep **6486** (native 3343, c 3130, skipped 13), the predicted total exactly.
Zero failing, zero flips vs `0a9d0d5`; `mk test` 106 across 7 suites. Four new
specs (14.64-14.67) assert **file contents**, not exit status — a broken
redirect writes somewhere else, which a status-only check cannot see. Two of
the four are labelled in place as *not* proofs of this fix (the `:-` ternary
and the int-cell path do not reach the mended function).

**Next, in order:**

1. **Unstash the int-sink work** — `exit`/`wait`/`arg` accept int-type only.
   Implemented, spec'd and proved non-vacuous already; held back on DG's
   option B so this bug landed first. `git stash list` → "int-sink work",
   also saved as `int-sink-v2.patch` in the session files. Its `isink.*`
   retirement in the last sweep is expected, not a finding.
2. **Retire `-t`** as a `[[ ]]` test, separate commit (DG). Add the arm to
   `validate.rs::unsupported_unary_test_hint` (SB5028), **keep** `-t` in
   `is_unary_test_op` so the code is SB5028 not SB4014, drop
   `emit_isatty_test` and the emit_c arm, migrate `cov.89`.
3. **`terminal?` is dropped** as YAGNI (DG). Supporting fact for the record:
   sash arithmetic has no unary predicates at all, so it would have been the
   first of its kind, and `[[ ]]` is already the predicate context.
4. Then the parked **magic-variable** directive: `$$` → `$(getpid)`,
   `$!` → `$(getcpid)`, `$?` → `$(status)`. Only those three are magic; the
   `$!` int seed added here is a step toward it.

Also owed: rebuild `coverage.db` (`mk cov-union` → `mk cov-db`) before the
coverage queue resumes; amend DD#89's "Left open" once the tightening lands;
`plan.md` at repo root is stale.

## ══ STATUS (2026-09-26) — a target must prove itself ══

**DD#119 is in: a redirect target must be provably non-empty, and
string-typed.** This came out of the magic-variable directive rather than
being aimed at redirects. Desugaring `$$` to `$(getpid)` broke `> $$`, because
the target became a `CommandSubstitution` and SB4049 rejects those at parse.
I offered DG four options; he took none of them and asked instead what if a
redirect target had to carry a disposition. That attacks the **actual** bug
class — `bug-redirect-nonsimple-target-empty` was an *empty* filename, not a
complicated one — and needs no carve-out and no `ExprKind::Intrinsic` node.

Dispositions are the *proof mechanism*, but the rule tests **"does the result
prove non-empty"**, not "did you write a disposition". That distinction is
load-bearing: the looser wording would admit `${p:~}` and `${p:-}` and let the
original bug back in wearing the remedy's clothes.

DG settled three details: **array cells are in** (a cell behaves like a string
variable), **concat is strict** (`> $p.log` with empty `p` silently writes a
hidden `.log`), and ints are **exempt but still need quotes** — quoting is how
int becomes string everywhere else in sash.

**Three bugs found and fixed**, none related to each other:

- A **native ICE from ordinary source** — `int p=5; ... > $p.log`, three
  shapes. `redirect_expr_simple_expansion` called `str_ptr()` unconditionally;
  braced spellings survived only because the transform spills `${p}` targets
  to a scalar temp. The comment at `types.rs:19` *claimed* unreachability and
  listed its audited callers — `redirects.rs` was not among them. Corrected.
- A **missing C `Concat` arm** in `ir_expr_as_str`.
- Behind it, a **silent wrong answer**: `latest_var_as_str` returned `""` for
  int vars, deferred in a comment as "rare enough". Not an ICE — just quietly
  wrong. It surfaced **only** because native 14.59 passed and c:14.59 failed,
  which is exactly the differential oracle's job.

Sweep **6464** (native 3332, c 3119, skipped 13) — the predicted total
exactly. Zero failing, zero flips vs `505b581`; `mk test` 59/27/20.
Non-vacuity took **three** neuters, one per mechanism, each failing exactly
its own cohort (6 / 1 / 1).

**One wart, recorded in the DD rather than papered over:** `lib.rs:387` prints
every `TypeError` as SB4030, so all of DD#119's rejections share that code.
The int exemption needs `env.var_type`, which pins the rule to
`type_check.rs`. A dedicated SB5xxx means duplicating the type environment
into `validate.rs` or teaching `type_check` per-error codes — both larger
than the rule, neither decided.

**Next — the parked directive, now unblocked.** `> $$` is rejected uniformly
as an undispositioned read, so the magic-variable work can proceed: re-apply
`$$` → `$(getpid)`, then `$!` → `$(getcpid)`, then `$?` → `$(status)`. The
last is largest — DD#56's handling rules attach to `?`. Only those three are
magic; everything else already follows DG's principle. The SSA-level `?`/`!`
variables stay; only the front end reverses.

**Then the int-sink directive** (DG): `exit`, `wait` and `arg` accept int-type
only, `exit ''` rejected, signed literals still legal; `-t` ceases to be a
`[[ ]]` test. A `terminal?` predicate is **not** to be implemented — DG wants
more discussion including YAGNI. For that conversation: sash arithmetic has
**no** unary predicates at all, so it would be the first of its kind, and
`[[ ]]` is already the predicate context.

Also owed: rebuild `coverage.db` (`mk cov-union` → `mk cov-db`) before the
coverage queue resumes; amend DD#89's "Left open" once the tightening lands.

## ══ STATUS (2026-09-25) — the key is a type, not a spelling ══

**The strict subscript rules are in.** An array index must be int-typed; a
map key must not be. Both surfaces enforce the same rule, which journal 413
made possible by giving the assignment LHS the read side's index grammar.

The predicate that stood there asked the *negative* question — "is this a
string key" — and could only name the forms it knew. `var_type` ends in
`unwrap_or(String)`, so "declared a string" and "never recorded" are the same
answer and no such test can separate them. It now asks for `Int` and
delegates the typing to `expr_type`, inheriting DD#76's int-array cell,
`Arithmetic => Int`, and the rule that a `:-`/`:?` disposition keeps its
*subject's* type — so a bare disposition on a string is rejected and one on
an int is not, without either case being written here.

**Two things beyond the directive, both for DG:**

- The **map rule is wider than the approved table**. It rejects every
  int-typed key, not just a bare `$i`, because `${m[$((1+1))]}` is the same
  crossing and was going through in silence. I verified the quoted remedy
  compiles for all of them first. Veto if that is not what was meant.
- A **live bug in `attest`**, found adjacent to the work and fixed under the
  bug policy: `attest i (( n = i ))` never declared `n`, so it typed as
  String and `$(strlen $n)` was *accepted* on an int — a DD#105 hole in the
  one construct built to make that crossing accountable. `--emit-ir` showed
  why: plain `(( n = 2 ))` is an `:assignment` (so `check_assignment`
  declares the target) but the attested form stays a `:command`.

The `${a[${s:-hello}]}` ICE is closed **by construction** — the strict rule
makes that codegen sink unreachable. No `check_int_value_defaults` widening
appears owed.

Sweep 6440 (predicted exactly), zero failing, zero flips vs `0e23874`;
`mk test` 106 green. Non-vacuity needed **two** neuter passes, because
`is_int_index` feeds only the array path — one pass would have reported a
clean proof over half the work.

**The DD amendments are written** — DG, 2026-09-26: *"amend the DDs so long
as the edits are well understood."*

- **DD#80** — the type-aware section is superseded in place. The rule is
  decided by *type*, not by an enumerated list of spellings, and it applies to
  `a[K]=v` as well as `${a[K]}`.
- **DD#33** — records the *second* mismatch (int key on a map). DD#80 had been
  citing this DD for a rule its text never stated.
- **DD#93** — addendum: Part D always said the target is bound to an i64; the
  *clean* half was implemented and the *int* half was not.
- **DD#89** — the "coerces… as in every other int context" clause is struck,
  with the measured table in its place. It deliberately does **not** decide
  whether `exit $s` should reject; that is a behaviour change and DG's call.

Every factual claim was re-measured against `761b3f4` rather than carried over
from the notes, and **two tables I drafted from memory were wrong**: I had
recorded `a[${s:?m}]=Q` as working when it parses and is then correctly
SB4030 (a bare disposition keeps its *subject's* type — `$(( … ))` is the
conversion, which is the whole point), and DD#80's before/after table had a
column that was also pre-change, so the amendment contradicted its own second
section. Both corrected. The accountable LHS spelling is
`a[$(( ${s:?msg} ))]=Q`, verified to abort at runtime with the author's
message.

Both stale task files are revised: `array-index-string-by-value-flow` is
**done** — its "string by evidence" route was abandoned because the negative
test cannot be completed, and the positive rule dissolved the question — and
`undispositioned-string-to-int-sites` is down to four sites (`exit`, `wait`,
`[[ -t ]]`, plus `arg` masked by taint).

Also owed: rebuild `coverage.db` (`mk cov-union` → `mk cov-db`) before the
coverage queue resumes.

## ══ STATUS (2026-09-25) — the left side that could not say it ══

**The assignment LHS now accepts every subscript index the read side does.**
DG's directive after the DD#80 work: enforce the strictest forms — array
indexes must be integers, map keys strings — and **"assignment and read should
be uniform in what they accept."** The uniformity clause had to come first,
because the write side was a strictly narrower surface and could not host the
rules.

Measured before: `${a[$(( … ))]}` read fine, `a[$(( … ))]=` was SB5011;
`${a[${s:-2}]}` read fine, `a[${s:-2}]=` was SB4014; `${m["$i"]}` read fine,
`m["$i"]=` was SB4014. **`$(( … ))` is the conversion DD#80 itself names as
the remedy for a string index**, and it was unusable on the left of an `=` —
so an author told to convert had nowhere to put the conversion. The two
failures did not even agree on a code: one reached the parser (SB4014), the
other never looked like an assignment, so `a[` became a command name and the
error was *command not found* for a typed-index problem.

`parse_split_subscript_index` took exactly one token — `Tok::Dollar` or a
`DollarBrace` holding a bare identifier — because the shape probe pinned the
index at `peek_n(2) == "]"`. The fix recovers the raw index text from the
token spans and hands it to **`expr::parse_subscript_index`, the same function
the read path calls**. One list of accepted forms, both sides call it; the
parity is now structural rather than two lists kept in step by hand.

Three existing specs changed, all improvements: `a[ x ]=5` now reports the
real fault (SB4030, string key on an array) instead of SB5011; `m[${k:-z}]=1`
is honoured; and `m[${#k}]=1` reports SB4038, the `${#…}` retirement — the
same message the read side gives for the same text. The LHS did not learn
about `${#…}`; it stopped having an opinion of its own.

Sweep **6422** (predicted exactly), zero failing, **zero flips** vs `aa8fad2`.
8 new specs (80.51–80.58); non-vacuity proved by reverting and rebuilding.
`parse_subscript_index`'s nesting was lifted into a `parse_whole_index` helper
rather than bypass the complexity gate; both are now 3/1.

**Next, and still owed on the directive:** the two type rules (array index must
be int-typed; a map key must be string-typed, so `${m[$i]}` on an int wants
`${m["$i"]}` per DD#105), and the subscript int sink's missing DD#77 D4
default-literal check — `a[${s:-hello}]` still **ICEs** at
`codegen/values.rs:1432`. Journal 413.

## ══ STATUS (2026-09-25) — the type that was only half known ══

**A `str`-declared scalar no longer silently indexes an array at 0.** DG:
"no string subscript for arrays." `str s=abc; ${a[$s]}` printed element 0;
`a[$s]=99` wrote it. The bareword `${a[s]}`, the quoted `${a["abc"]}` and the
arithmetic `(( a[s] ))` all rejected — same array, same variable, two answers
by spelling. Read, write and append close together: `check_assignment` and
`check_subscript_types` share `check_subscript_key_kind`.

**The framing I filed it under was an over-read, and the sweep caught it.**
DD#80 uses "string key" structurally — "a clean bareword or a quoted string" —
not "a variable whose type is string". `$var` is a separate enumerated form
with no type proviso. Rejecting every string-*typed* index cost **8 specs in
both modes**, including two the DDs bless: `attest i i?` (60.70, DD#93's
"documented way through") and `${arr[$1]}` (80.31, "Bash agrees").

The mechanism is one line — `var_type` ends in `unwrap_or(VarType::String)`,
so **"declared a string" and "never recorded" are the same answer**, and a
bare `i=1` records `String` exactly like `str s=abc` (sash types a bare
numeric literal as a string, per bash). So the shipped rule is narrow: a new
`declared_str` set, rejecting only a *positively declared* `str`. The old
`// could be either` comment was right about every case except the one it
could have decided.

**The map side is not a mirror bug.** `${m[0]}` rejects, `int i=0; ${m[$i]}`
does not — the same literal-only shape. DD#80: "any string is a valid key",
and DD#33's bare-integer rejection is anti-ambiguity ("it reads like an array
index"), not a type rule. Checking `notes/decisions/` first stopped that fix.

**A second find, caught by checking a sentence I had already written.** Because
read and write share the check, the write path inherited the read path's advice
— and `a[${s:?m}]=Q` does **not** parse (SB4014; DD#33's LHS takes a bare
`$var` only). The rejection pointed into a wall. The message is now
surface-specific: the read path names the disposition, the write path names an
`int` variable or converting first (`int i=$(( ${s:-2} )); a[$i]=…`). Specs
80.49/80.50 pin it. Lifting the DD#33 LHS restriction is filed, not done.

The SB4030 text for a variable index also stopped recommending `${a[$i]}`,
which sent an author from one SB4030 to the same one.

Sweep **6406** (predicted), zero failing, zero flips vs `aa8fad2`; `mk test` 106.
Journal 412. Neutering fails exactly 80.41–80.45; 80.47 is the backstop for
the deliberate silence.

**Next up — three open items for DG, none started:**
- the status-channel DD (`return`/`exit`), answered `yes` + `rule_form=context`
- the **DD#89 amendment** — "a string argument coerces to int (atoi), as in
  every other int context" is, in DG's words, "plain wrong": nearly every
  other int context rejects
- `-t` as a distinct predicate (`if $(terminal? $fd)`), following SB5019
  having already moved the whole `-eq` family to `(( ))`

DG asked to discuss the scope of these amendments before any drafting —
do not write them unprompted.

## ══ STATUS (2026-09-25) — the guard that did nothing ══

**DG's question — "SB4030 rejects string→int on assignment; why does `return`
atoi silently?" — found a native/c divergence no spec covered.**

`return` lowers to an assignment to `$?`, and native `handle_exit_status_assign`
lowered the value **as a string** then ran the *lenient* `sash_atoi`. It never
consulted the type. `exit` uses type-directed `lower_value_as_int`, which routes
a DD#77 disposition to `sash_atoi_or`/`sash_atoi_or_abort`.

The casualty was not `return $s`. It was the author who did everything right:

| input (`str s=abc`) | native | c |
|---|---|---|
| `return ${s:-7}` | **0** | 7 |
| `return ${s:?bad}` | **0** | abort, rc 1 |

`${s:?…}` is the spelling SB4030 *recommends*. In word context its trigger is
unset-or-empty (DD#65), so `"abc"` passed through and atoi'd to 0 — **an
explicit "abort if not an integer" guard compiled to nothing.**

The c backend was right all along; its comment claims it "mirrors
handle_exit_status_assign", which was backwards. **Fixed** by converging native
onto `lower_value_as_int`. The function got simpler (cc 2/cog 1 → cc 1/cog 0).

**Specs `74.57`-`74.61`.** Neutering fails exactly 74.57/74.58/74.61 and leaves
74.59/74.60 green (valid-integer controls — proof the fix does not over-reach);
c mode stays 138/138 under the same neuter, a second proof the bug was
native-only. 74.61 nearly shipped **vacuous**: `${s:-0}` prints `yes` both
broken and fixed, because lenient `atoi("abc")` and the default are both 0.
The default is `1` so the two answers differ.

**The inventory DG asked for** — six undispositioned string→int sites remain
(`exit`, `wait`, `${a[$s]}`, `a[$s]=`, `[[ -t $s ]]`, `$(arg $s)`); three of
them diverge from bash. Filed as `undispositioned-string-to-int-sites`.

**DD#89 needs an amendment either way.** It blesses the coercion "as in every
other int context" — measured, nearly every other int context *rejects*. The
clause was written settling a **range** question and the type remark rode along
unmeasured. A DD that argues "consistent with X" asserts a fact about the
implementation, and nothing re-checks it.

**DG's steer**: *"Rather than prioritize bash consistency, I want to prioritize
sash self-consistency."* That settles the direction of any tightening — bash
agreement at `${a[$s]}`/`[[ -t $s ]]` is not a reason to keep a hole. **Scope
and the DD are open; DG asked to discuss before tightening.** Blocker to note:
`a[$s]=` has **no** accountable spelling (`a[$(($s:?))]=Q` is SB5011), so it
cannot ship with the rest. DG's `${a[$(($s:?))]}` read form does work;
`$a[$((s:?))]` is SB4041 "ambiguous braced index" — already diagnosed by name.

**Gates**: sweep **6386** (predicted exactly), zero failing, zero flips vs
`4578aed`; `mk test` 106; fmt/clippy/complexity clean.

## ══ STATUS (2026-09-25) — the diagnostic that could not fire ══

**Coverage flagged an error *message* with zero hits, and it turned out to be
unreachable — which was a hole, not a missing test.** SB4014 *"return:
expected a status producer"* could never fire: `parse_return_producer_head`'s
four delegates contain only one `Ok(None)` between them, and the recursion
that feeds it (`parse_negated` ↔ `parse_suppressed` ↔ `dispatch_statement`)
has no base case. Five sibling lines confirm it at 0 hits.

**The input it was written for silently compiled.** `parse_return` decides
"producer?" from the next token (`[[`, `((`, `!`, non-numeric word); a
reserved word is none of those, so it fell through to the *numeric status*
path, which absorbed it as an argument and dropped it:

| input | sash | bash |
|---|---|---|
| `return if` / `while` / `str` / `then` … (20 keywords) | rc 0 | rc 2 + error |
| `return =`, `return -` | rc 0 | rc 2 + error |
| `return - echo hi` | rc 0, **command never runs** | rc 2 + error |

**DG asked whether `int x=…; return $x` already works and whether its
underpinnings could be reused.** Probing that raised two more apparent
divergences — `str s=abc; return $s` → 0, and `return 300` → `$?` 300 vs
bash 44 — and **DD#89 retires both**: string→int is atoi "as in every other
int context", and the status channel is "a full i64 internally, 8-bit at the
process boundary" (verified: the *program* exit is 44 in both shells). Two of
three probe "divergences" were the design working.

**The reuse is real, just not the value path** — `return if` never becomes a
value. It is `keyword_to_str` (parse/expr.rs), by its own doc comment *"the
only list of reserved-word tokens in the word parser"*, so a keyword added
later is covered with no second list to keep in step. That dissolved the
whitelist-vs-blacklist question I had queued.

**Fix**: `parse_return_value_form` rejects reserved word / `-` / `=` via
`return_producer_error`, shared with the (still-cold) `None` arm so the
sentence exists once. DG's call to reject `return - CMD`, consistent with
SB4019's `!`+`-` clash. Extracting the helper was also required by the gate —
inline, `parse_return` hit cog 6 against a 5.64 bar.

**Specs `74.49`-`74.56`** assert the **message**, not the code: recovery emits
a follow-on `SB4014 expected expression` on the orphaned `}`, so asserting the
code alone would pass with the guard deleted. Neutering failed exactly the
five rejection specs and left the three controls green.

**Pre-existing fault found while taking a baseline**: `echo a |` reports
SB4056 at top level but **loses it entirely** inside a function body, showing
only a misleading `expected expression` on the closing brace. Filed as
`parse-error-in-function-body-loses-diagnostic` (P2). It also means a spec
asserting a bare parse-error code in a nested context can be vacuous.

**Gates**: 6376 (predicted exactly), zero failing, zero flips vs `9996b46`;
`mk test` 106; fmt/clippy/complexity clean.

**Next**: coverage queue. `coverage.db` is invalidated by the parser edit —
rebuild with `mk cov-union` → `mk cov-db` before the next pass.

## ══ STATUS (2026-09-25) — the function that ran once ══

**Coverage found a bash divergence: arithmetic `&&` / `||` did not
short-circuit.** `d=0; echo $(( d != 0 && 10 / d > 1 ))` — bash prints `0`,
sash aborted `division by zero`, rc 2. The guard idiom (*test the divisor,
then divide*) did not work, in **either** backend. Bug policy: stopped
everything else.

**The clue was an execution count, not a coverage gap.** `emit_logical_and_or`
ran **once** in 6344 spec programs, always with `And`; the c mirror's `Or` arm
had zero hits. The values were all correct when probed against bash — what
gave it away was the function's own doc comment, *"Short-circuit-free
(operands are already evaluated)"*, read literally.

**It had a twin that was already fixed.** `native-ternary-eager-eval` (done,
journal 193) is the same bug class for `a ? b : c`. So the approach was
settled by established pattern, not an open fork: lower the right operand
*inside a branch*. `lower_logical_short_circuit` mirrors `lower_ternary_int`;
`emit_logical_as_int_c` mirrors `emit_ternary_as_int_c`.

**C needed a real fix too, despite C's `&&` short-circuiting.**
`lower_value_as_int` emits the div-by-zero check and overflow trap as
**statements** before the expression, so they ran unconditionally. Worth
remembering for any future lazy construct.

**Looks like a bug, is not**: a function call in a short-circuited operand
still runs (`lift_int_func_calls` hoists calls out of `ArithBinOp` *and*
`Ternary`). Bash's command substitution inside arithmetic is **also** eager —
verified — so this matches bash. No change.

**The eager emitters were then unreachable.** `emit_arith_op`'s three callers
can no longer pass And/Or: two are the `ArithExpr` arm, and that node is built
at four sites in `transform/pipelines.rs`, all literal `ArithOp::Add`. DG's
call: delete and `unreachable!`, matching the precedent already in the same
`match`. The deciding argument was that leaving them as "cold fallbacks"
leaves a trap — the code they hold computes the *eager* answer, so anything
reaching them again silently reintroduces the divergence.

Specs `31.129`-`31.136`: four divergence shapes plus four that force the RHS
to be needed, because a fix that never evaluated it would pass the first four.
Non-vacuity proven by neutering each fix — exactly the four guard specs fail,
per mode.

**Gates**: 6360 (predicted exactly), zero failing, zero flips vs `cc5f7b0`;
`mk test` 106; fmt/clippy clean; complexity clear against a 3.85 / 1.88 mean.

**Next**: coverage queue. `coverage.db` is invalidated by these edits — rebuild
with `mk cov-union` → `mk cov-db` before the next pass.

## ══ STATUS (2026-09-25) — the citation that pointed at the wrong subject ══

**DG asked one question about the coverage pass** — *is the "left cold on
purpose" backstop checked by a unit test?* No, and it could not have been:
`ssa_taint.rs` has no `#[cfg(test)]` module, and `mk cov` measures the **spec
suite only**, so "cold" in that report always meant "no spec reaches it".

**The justification was a misapplied citation.** I quoted DD#91 verbatim — the
quote is real, at DD#91:50-53 — but it is about `ElideEmpty`/`ArraySplat`
**dispositions**, not the subscript check. A true sentence attached to the
wrong subject, which is exactly why re-grepping for it would have reassured
me. The coverage report held the refutation two lines away: the *sibling*
backstop `check_tainted_var` fires 8 times (sw.4/5/8/9 × 2 modes). Both are
backstops shadowed by the same gate; "it is a backstop, so it is cold" cannot
explain one firing and the other not.

**It was reachable in one line**, via the `--no-taint-cst` idiom
`taint_switch.sh` already uses:
`array a=($(getenv HOME)); echo ${a[0]}` → *tainted array a[...]*.
Now `sw.10` (asserting the **array** message, so it cannot pass on the scalar
backstop) and `sw.11` (clean-array control, must still compile and run).

**The wider hole stays deferred, condition re-verified.** `read`/`mapfile`/
`argv` array fills still compile clean under `--no-taint-cst` — the documented
gap in `ssa-taint-full-backstop`, deferred by DG 2026-09-12. Each is SB5020 in
a normal compile; re-checked, so the condition holds. The boundary is sharper
than the task said: the array **literal** lowers to an `Assign` and *is*
classified; only the bulk-read origins (no `VarRef`) are not. Table appended to
the task.

**Gates:** 6344 specs (predicted), zero failures, zero flips; `mk test` 106
passed; clippy and fmt clean. Non-vacuity by revert. **No compiler source
changed** — specs plus the correction.

## ══ STATUS (2026-09-25) — advice that cost the author the thing they asked for ══

**Coverage queue.** `coverage.db` was invalid (keyed by `(file,line)`, and
three commits had renumbered `parser.rs`/`validate.rs`), so it was rebuilt
first: 538 uncovered lines against `56cb1f2`. The queue has flattened to 4-6
lines per function, mostly cold diagnostics. Four probed; one was a bug.

**Bug — `${x:!~}` was advised into a second error.** `check_elide_pexp` built
its SB5046 message from `pe.operator` alone, but DD#91's 2026-09-09 addendum
moved the `!` into `pe.taint_off`. So the message quoted `` `${x:~}` `` at an
author who wrote `${x:!~}`, and advised `${x:-DEFAULT}` — which **drops the
taint-off**. Verified the chain end to end: `${x:!~}` → SB5046, following the
advice `${x:-D}` → **SB5020**, and the form that works (`${x:!-D}`) was never
mentioned. The sigil now renders from operator **plus** flag (`elide_sigil`)
and the advice keeps the `!`. The dead `Some(":!")` arm went with it — the
parser strips `:!` into the flag, so `pe.operator` can never hold it.

**Reading the whole DD is what prevented a wrong "fix".** DD#91's *body* says
`:!` should be SB5046 outside a list; its **addendum supersedes that** — `!`
is a modifier, so `${x:!}` is legal wherever a bare expansion is. The cold
branch was stale code, not a missing check.

**Two cold diagnostics that were only untested** — both now specced:
- **SB4014 "expected operand in test expression"** (`parse_test_operand`):
  `[[ -n ]]`, `[[ ]]`, `[[ $x == ]]`. A *different* SB4014 from the
  "expected `]]`" one DD#118 added, so the specs assert the text. `ow.24`-`ow.26`.
- **SB4018 on a bare `! X`** (`check_negation_must_consume`): all nine existing
  SB4018 specs covered the *other* site (`[[ ]]`/`match`). A negated pipeline
  hits SB4040 first, and `echo hi | ! cat` is accepted (the stage is unwrapped
  to a `Negated` marker), so the reaching shape is plain `! true`.
  `e.b70.nbare/nif/ndash`.

**~~Left alone deliberately~~ — retracted.** `check_tainted_array` was
untested, not shadowed; the DD#91 citation was misapplied. See the
2026-09-25 entry above and journal 408.

**Gates:** 6340 specs, zero failures, exactly predicted; zero flips;
`mk test` 59/27/20; clippy and fmt clean; complexity unchanged. Non-vacuity by
revert: two specs fail without the fix.

Journal 407.

## ══ STATUS (2026-09-25) — a marker for the glued operator ══

**DD#118 Part D — the glued operator needs a `$` on the left.** Parts A-C
removed the competing *reading* of `[[ ab==$x ]]` but not the divergence:
bash reads the word as a bare word (always true), sash split the operator and
answered a comparison. Part B only made bash's answer inexpressible.

**DG's rule: the glued spelling is recognised only on a `$`-initial word.**
`$foo==abc` and `$foo==$bar` are the template; anything else is **SB4069**.
The premise held on inspection — Part C already rejects a two-static-operand
test, and `@a` is not a legal scalar test operand (SB5025), so a surviving
binary test always carries a `$`. The rule costs nothing Part C had not taken,
and gives the glued form a recognisable left edge.

**The diagnostic offers both fixes for `==`, one for `=~`.** `==` commutes, so
`$x==ab` is a real second fix alongside `ab == $x`; `=~` does not — its
subject must stay on the left — and says so rather than silently omitting it.

**Six casualties, all in `dispatch.sh`'s `dp.mp` block.** `mp4/5/10/12` pin
the multi-part rejoin and only incidentally used a literal subject, so they
respelled to `$s==$x$y`; `mp3`/`mp6` *were* the literal-initial case and
inverted into rejection specs. `mp7` independently keeps the right-side
rejoin covered. `118.21` moved to the spaced spelling — SB4069 is a parse
error, so it now precedes SB5086.

**Verified against the suite, not against the checker.** All 3003 extracted
spec bodies through `sash --emit-ir`: six SB4069 hits, all six the rejection
specs just written.

**Incidental finding, filed not fixed:** tree-sitter-bash 0.25 cannot parse
`[ a=b ]` — an `ERROR` node with `a=b` as an assignment prefix — so
`sash-convert` passes it through verbatim, and it compiles, because DD#23
resolves `[` to the external `/usr/bin/[`. Sound and manifest-disclosed, but a
silent downgrade from a conversion to a fork. `convert.rs`'s `ERROR` arm
handles one known hole and otherwise falls through with no `# MANUAL:` note.
Filed as `convert-silent-passthrough-on-parse-error` (P2). Pre-existing.

**Gates:** 6320 specs, zero failures, exactly predicted; zero flips against
the DD#118 baseline; `mk test` 59/27/20; clippy and fmt clean. Complexity
re-measured cc 3.85 / cog 1.87 — `parse_test_binary_tail` at 11/5 against
11.55/5.62 passes with no room left; the next arm there needs a split.

Journal 406. `notes/tasks/glued-operator-vs-bash.md` closed.

## ══ STATUS (2026-09-25) — a test that could not fail ══

**DD#118.** DG asked whether a single `=` has two meanings. It does —
assignment in statement position, string equality in a `[[ ]]` operator slot —
but the positions are disjoint, so there is no ambiguity for the *parser*.
Probing it for the reader turned up something worse.

**`[[ $x=y ]]` was unconditionally true for every value of `x`.** Glued, it is
one word, which fell to the implicit-`-n` rule; that word always contains a
literal `=y`, so it can never be empty. Spaced, `[[ $x = y ]]` is a real
comparison and answers F. Same three characters, opposite answers, and bash
agrees with both — so the differential oracle could never see it.

**DG settled three changes together:** `==` is the only spelling of equality
(**SB4067**), `-n` is explicit and never implied (**SB4068**), and a test whose
result is fixed by the source text is an error (**SB5086**). Separating
*vacuity* from *implicitness* was the useful move — requiring `-n` makes the
shape honest but leaves `[[ -n foo ]]` and `[[ abc == abc ]]` constant.

**I argued for a narrow vacuity rule and was wrong.** DG: *"isn't
`[[ foo == foo ]]` just a mechanical rewrite to `s=foo; [[ $s == foo ]]`?"* It
is — the rewrite still emits a real `sash_strcmp` against a real variable, so
no coverage is lost. That also *forces* the rule to be **syntactic**: if DG's
escape must keep working, the check cannot const-propagate. It stops at the
first variable. File tests and numeric operators are exempt (the latter so
SB5086 cannot shadow SB5019).

**A checker that reported zero was wrong about its own coverage.** My regex
extractor for `run_sash` bodies stopped at the first `'`, so anything using
the `'"'"'` idiom was truncated — and four `attest e in f [[ $e = $e ]]` lines
sat past the cut. The sweep found them. Replaced it by letting bash do the
quoting: copy the specs beside a stub `common.sh` whose `run_sash` dumps `$1`,
source them, get 2967 exact bodies. "Zero remaining" is a claim about the
checker, not the tree.

**Narrowing a new diagnostic made it more accurate.** SB4068 initially claimed
`[[ a&b == x ]]`, where `&` is the real fault. Guarding it on "the scanner is
at a test terminator" restored SB4014 there *and* for `[[ -i 42 ]]`, removing
two spec changes I had queued.

44 spec sites migrated, all mechanical; `operand_words.sh`'s `ow.14`-`ow.16`
were specifying the retired surface in their own comments. `sash-convert`
gained both rewrites. Gate 6251 → **6306**, exactly as predicted (23 new specs
×2 modes + 9 native-only convert specs); 0 failing. `mk test` 59/27/20.
`check_vacuous_test` hit cognitive 6 against the 5.62 bar and was split into
`check_vacuous_unary`/`check_vacuous_binary`.

**Open, raised with DG:** `[[ $x==a ]]` answers F in sash and T in bash
(`extract_compact_operator` splits the glued operator). Pre-existing, and
distinct from the deliberate no-space extension `[[a==a]]` that bash cannot
parse at all and that `dispatch.sh` specs 28 times.

**Next:** resume the coverage queue — `lower_value_as_str_aggregate` (6),
`emit_match` (5), `lower_value` (5), `emit_degenerate_pipeline` (5).
`parse_test_binary_tail` is now covered by the DD#118 specs. Higher-value
alternative: `audit-validate-wildcard-escapes` (P1) — the run-1 audit named
validator bypass the top priority.

## ══ STATUS (2026-09-25) — redirects are a sequence, not a set ══

**Coverage item 4 found two bash divergences, one with filesystem side
effects.** The cold arm was the second file-open redirect in a list; it is
live, and both backends handled it wrong.

**`cmd > /nonexistent_a/x 2> made.txt` created `made.txt`; bash does not.**
The redirect list was treated as independent operations when it is a sequence
with side effects — `open(O_CREAT|O_TRUNC)` is not something you may perform
speculatively. `cmd > /bad > important.txt` truncated `important.txt`.

**`cmd > good.txt 2> /nonexistent_d/bad` blamed `good.txt`** — a file that had
opened fine — violating DD#70's `<argv0>: <path>: <reason>`. The accumulator
kept the first *open-candidate* path; the comment called it the first
*failing*-candidate path, true of one redirect and false of two.

**DG's call: short-circuit in both backends** (the `native-ternary-eager-eval`
precedent), which subsumes the diagnostic bug — once only one redirect can
fail there is no "which path?" left to get wrong. Native uses `brif` into
do/skip blocks with a three-wide phi (`select` evaluates both operands, so it
cannot short-circuit a call); C uses `?:`, which short-circuits natively.
DD#70 gained the left-to-right/stop-at-first-failure addendum.

**Non-vacuity by revert gave native 2 failures and C 3 — and the difference
was the finding.** Chasing C's extra failure exposed a *third*, pre-existing
divergence: bash writes the setup-failure diagnostic through the redirections
applied so far, C matches, **native restores the fds first and does not**. An
oracle divergence, filed as `notes/tasks/native-redirect-diag-after-restore.md`
rather than smuggled in — `emit_open_failure_abort` fuses message, status and
block termination, so it is not a resequencing.

An empty `$rerr` was the symptom in both C failures. It can mean the
diagnostic went somewhere else, not that nothing went wrong.

Gate 6243 → **6251**, exactly as predicted; 0 failing, 0 flips. `mk test`
59/27/20. Both `emit_redirects_guarded` hit cognitive 6 against the 5.64 bar
and were refactored down (`emit_next_redirect`, `fold_redirect_failure`).

**Next:** resume the coverage queue — `lower_value_as_str_aggregate` (6),
`parse_test_binary_tail` (6), `emit_match` (5), `lower_value` (5),
`emit_degenerate_pipeline` (5). Higher-value alternative:
`audit-validate-wildcard-escapes` (P1) — the run-1 audit named validator
bypass the top priority.

## ══ STATUS (2026-09-25) — a defensive branch that reported success ══

**Coverage item 3 found a bug, not dead code.** Hunting a family of cold
"defensive" `args.first() else { … }` branches in `cd`/`close` across both
backends, three were genuine dead residue behind SB4052 — and the fourth was
**live, reachable, untested, and wrong**.

**Bare `cd` compiled, reported `$? = 0`, and did not move** (bash goes to
`$HOME`); `cd /tmp /var` silently dropped the second argument, where the
sibling `close 1 2` is SB4052. The codegen comment claimed DD#53 licensed
"succeed silently" — DD#53 establishes there is no implicit `$HOME` and says
nothing about `cd`. It was an unratified inference, and the only `cd` specs
were DD#55 abort paths, so nothing contradicted it.

**DG's call: reject both forms (SB4066), plain message, `close` cleanup in the
same commit.** `cd` now takes exactly one argument. Unlike `close`'s fd it need
*not* be a literal — a path is not a runtime-chosen descriptor — and spec cd.3
pins that so "mirror `check_close_arg`" cannot later tighten it.

With the validator strict all four codegen branches became genuinely dead and
are now assertions naming their validator code (the `handle_signal` precedent),
along with the C backend's `_ => ("0","0")` null-diagnostic wildcard.

**The shape is the lesson.** Four sites, identical syntax, three dead and one
live; reading them side by side could not tell them apart — **only execution
did**, and the comment on the live one was the most confident of the four. A
"be defensive anyway" branch either answers a misuse with a lie, or hides a
real validator gap by letting the program run and report success.

**`coverage.db` staleness bit again, inside the session that documented it.**
`57a0494` shortened `emit_c.rs` by three lines, so every coverage row past ~891
was off by three and pointed at the *normal* path. Sharper rule:
**`coverage.db` is invalidated by any commit that renumbers a measured file** —
and the symptom is not always an impossible line. Here every row was plausible
and only the meaning was wrong.

Closed 9 cold lines. Gates: sweep **6243** (predicted), 0 failing, 0 flips vs
`57a0494`; `mk test` 59/27/20 (up 2). Non-vacuity by revert: exactly cd.1/cd.2
fail, cd.3 still passes. `handle_cd`/`handle_close` are now cc 1 / cog 0.

Journal 403. DD#53 gained an addendum recording SB4066.

**Next:** the coverage queue — `emit_redirects_guarded` (6),
`lower_value_as_str_aggregate` (6), `parse_test_binary_tail` (6), `emit_match`
(5), `lower_value` (5), `emit_degenerate_pipeline` (5) … **Re-run
`mk cov-union` first** — three commits have landed since the measurement.
Higher-value alternative: `audit-validate-wildcard-escapes` (P1, journal 400).

## ══ STATUS (2026-09-24) — a conversion the language forbids, in both backends ══

**Coverage item 2: both backends carried an unreachable `CaptureEnd` arm that
ran `sash_atoi` on captured stdout — a string→int conversion with no
disposition, exactly what SB4030 rejects and what journal 400 closed in the
front end.**

DD#58's Option C amendment (`a8ac707`) settles it: `$(…)` is
return-type-polymorphic, and `CaptureEnd` is its *stdout* half, so the target
is always a string — a `-> int` return takes the `__intret_` path. Both Int
arms are pre-amendment residue. Proven by execution as well as argument: a
`panic!` in each, full sweep, 6237 green.

**The native fix was already written.** `VarSlot::str_ptr`/`str_len` exist for
exactly this class of caller — their INVARIANT comment already lists "read/
capture targets" — and the `match` in `dispatch.rs` was re-implementing the
dispatch they centralise. Swapping to the accessors removed four cold lines
and consolidated onto two existing guards which were *also* cold and are pure
`&self` functions, so unit tests reach them trivially. **The cheapest way to
close a cold guard is often to notice it is the third copy of one that already
exists somewhere unit-testable.**

Closed 13 lines: `emit_capture_end_c` 7→0 · `emit_capture_end` 4→0 ·
`VarSlot::str_ptr`/`str_len` 2→0.

**A masked diagnostic nearly sent me after a bug that was not there.**
`int n=$(f | cat)` reports SB5020, which looks like it passed type check; it
had not — `attest n i?` removes the taint error and SB4030 appears underneath.
Third instance of first-diagnostic masking (cf. journal 389, 257).

Gates: sweep **6237**, predicted 6237, **0 failing, 0 flips** vs `37c790b`.
`mk test` **57**/27/20. Capture behaviour byte-identical across backends.

**Next:** queue continues — `handle_close` (6) · `emit_redirects_guarded` (6) ·
`lower_value_as_str_aggregate` (6) · `parse_test_binary_tail` (6) ·
`emit_match` (5) · `handle_cd` (5) · `lower_value` (5). Re-run `mk cov-union`
before trusting those counts again — they are from this session's run, so
still current. Also open: `audit-validate-wildcard-escapes` (P1).

## ══ STATUS (2026-09-24) — the type was the retired surface ══

**Coverage, resumed. The queue was four commits stale and its line numbers
pointed at doc comments — a coverage row naming a non-executable line means
the database is stale, not that the line is cold.** Re-ran `mk cov-union`.
Fresh baseline: **functions 98.80%** (27 missed) · **lines 97.79%** (607) ·
**regions 97.11%** (885) · **branches 91.04%** (349).

Top of the re-ranked queue was `ir.rs::read_arg_value` — 7 cold lines of 11.
It turned out to be DD#99 retired-surface residue, three layers deep:

- **A rival renderer.** `read_arg_value` was a near-exact copy of
  `literal_word_text` with `None` collapsed to `""` — the third rival-renderer
  of this campaign. `literal_word_text`'s doc comment, twenty lines away,
  already names that as the defect it exists to prevent: "a slot that requires
  a compile-time constant must be able to say 'this is not one' rather than
  invent a plausible string."
- **Dead arms hidden inside a live line.** SB5057 admits only
  `ExprKind::Literal` (it gates on `expr_literal_str`, empty for every other
  variant), so the quote-unwrapping arms were unreachable — including the
  `SingleQuoted` half of the *covered* arm, which line coverage cannot see
  because `|`-alternatives share a line.
- **The type was the root cause.** `ReadTarget { var_names: Vec<String> }` is
  the shape of the multi-target form DD#99 retired. Because it could hold any
  count, both call sites carried a silent `else { return }` for a count SB5057
  forbids — a malformed `readline` would have emitted no `ReadLine` at all.

Replaced by `readline_target(args) -> String`, which says what the validator
guarantees. `ReadTarget` and `read_arg_value` deleted; both dead guards gone.
Ten uncovered lines removed by deletion rather than by writing specs for input
that cannot exist. The two residual should-never-run assertions earn unit
tests, per DG's rule for the campaign.

**SB5057 was proven total by execution, not by reading**: `readline "x"`
rejected in all sixteen nesting contexts (function, if/else, while, until,
for, pipeline, &&, ||, brace group, negation, all three DD#66 dispatch bodies,
nested forms).

Gates: sweep **6237**, predicted 6237, **0 failing, 0 flips** vs `8b63c77`.
`mk test` **54**/27/20 (up 2 — the new invariant tests).

**Next:** the re-ranked coverage queue — `emit_capture_end_c` (7) ·
`handle_close` (6) · `emit_redirects_guarded` (6) ·
`lower_value_as_str_aggregate` (6) · `parse_test_binary_tail` (6) ·
`emit_match` (5) · `handle_cd` (5) · `lower_value` (5). Also open:
`audit-validate-wildcard-escapes` (P1, filed in journal 400).

## ══ STATUS (2026-09-24) — a disposition converts; it does not declassify ══

**DG read journal 399's spec block and asked "this doesn't seem right — you
read tainted x to assign n and then attest n?" He was right. The shape was a
taint-quarantine bypass, and those specs had pinned it as correct.**

DD#24 §Quarantine lists `${x:op}` "**in any expression**" as a rejected read,
and `$(( … ))` is an expression. Two bugs, both front-end (native and c agree):

- **Bug A — a validating conversion that did not validate.** A lone
  disposition as the whole arithmetic expression, in value position, emitted
  the raw string: `x=abc; echo "$(( ${x:-7} ))"` printed `abc`, and
  `${x:?msg}` — defined to abort on a non-integer — silently passed it
  through. An operator or an int assignment forced the conversion; a lone leaf
  had neither.
- **Bug B — the bypass.** `check_expr_taint` had no `ExprKind::Arithmetic`
  arm, so `x=$(arg 1); echo "$(( ${x:-0} ))"` compiled **clean** — no SB5020,
  no backstop, no diagnostic — and printed attacker bytes verbatim.

**The same `_ => {}` shape, five times in one pass.** Two were already recorded
in `check_expr_taint`'s own comments (`[[ ]]` operands, `$(<f)`). This session
added `ExprKind::Arithmetic` there, `StmtKind::ArithTest` in `taint_stmt`, and
— found only because the first fix did not close the hole — the same omission
in `reject_scalar_rhs_read`, which let an assignment's arithmetic RHS taint its
target instead of rejecting the copy, so `attest n` then declassified a value
nothing had vouched for. Per DG, both matches are now **total over `ExprKind`**.

**DG settled the design question by pointing at DD#93, not by answering.** It
rules twice: its unifying constraint is the one he defended when he *rejected*
"make every arithmetic result clean", and Part D exists precisely because the
disposition does not declassify — *"`i?` validates-and-branches; the
disposition converts inline; and `attest … (( n = s ))`
validates-converts-declassifies."* No new DD; the DDs were right.

**Consequence:** no int variable can now be tainted (every route is SB4030,
SB5020, or an array via DD#99 `read`), so `attest n [[ … ]]` on an int is
**validation-only** — like DD#93's map 2-binder form, for the same reason.
That makes journal 399's message fix the whole remaining point of the form.

**Gates:** revert proof predicted 9 native failures and got exactly 9. Sweep
6217 → **6237** as predicted, 0 failing, 0 flips vs `625843d`. `mk test`
52/27/20. 14 `coverage.sh` specs used the laundering shape incidentally and now
carry the sanctioned `attest s i?` preamble, with a comment saying so.

**Filed:** `notes/tasks/audit-validate-wildcard-escapes.md` (P1) — sweep every
silent-no-op wildcard in the validator; this is the run-1 audit's stated top
priority. Journal 400.

## ══ STATUS (2026-09-24) — the two backends disagreed about an int ══

**Coverage queue item `emit_attest` (7 cold lines) was an untested feature —
and writing the test found a backend divergence.** All seven cold lines were
the **int** paths: every `attest` spec in the suite attests a *string*.

**The shape is non-obvious.** `int n=$(arg 1)` is SB4030, so a tainted int
arrives through a disposition: `x=$(arg 1); int n=$(( ${x:-0} )); attest n
[[ -n $n ]]`. Drop the attest and the taint backstop rejects it — the feature
works and had no gate. Markers in both arms confirmed by execution.

**The divergence.** On a failing gate, native printed `attest: n failed the
gate`; the C backend printed `attest: n = '9' failed the gate`. Native only
pulled a value out of a `VarSlot::Str` and passed nulls otherwise. Three
sources agree against native: the string path prints `x = 'bad'`, the C
backend prints the int, and `emit_attest`'s own comment says *"Name + value
tell the user which gate failed and why."* Fixed with the existing
`codegen/types.rs::emit_int_to_str` (the same `sash_itoa` the C backend
reaches). Revert proof: native fails exactly `atint.5`, c stays green — the
signature of a one-backend bug.

**Residual:** the fix moved the int case out of `_ =>`, so those 2 lines are
still cold, now meaning "source variable with no slot at all" (`VarSlot` has
exactly two variants, so `_` is `None`). No route found — SB5031, SB5020,
SB5064 and SB5021 each block one. Filed as a residual, **not** deleted: a
green sweep proves "not reached by these inputs", never "unreachable"
(checkpoint 031). 5 of 7 lines now covered.

**Gates:** `mk test` 52/27/20. Sweep 6201 → **6217**, exactly as predicted,
0 failing, 0 flips vs `0bbf67c`. Specs `atint.1`–`atint.8` in `taint.sh`,
journal 399.

**Next:** `emit_capture_end_c` (7), `read_arg_value` (7 of only 11 lines —
densest), `handle_close` (6), `emit_redirects_guarded` (6),
`lower_value_as_str_aggregate` (6), `parse_test_binary_tail` (6), `emit_match`
(5), `handle_cd` (5), `lower_value` (5).

## ══ STATUS (2026-09-24) — a comma is not a newline ══

**The coverage queue found a wrong-answer bug in arithmetic.** `atok_to_text`
(`parse/arith.rs`) had 41 of 52 lines cold. Probing which arms were reachable
by putting each token where an operand is owed turned up one that produced no
diagnostic at all: `$(( 1 + , 2 ))` compiled and printed **3**. sash silently
dropped the comma. Four more shapes did the same — `$(( , 1 ))` → 1,
`$(( 1 ,, 2 ))` → 2, `$(( ,, ))` → 0, `$(( 1, ))` → 1 — all rejected by bash.

**DD#94 amended (DG's call).** The DD said `,`, `;` and newline are
"interchangeable", which made the behaviour arguably deliberate, so it went to
DG. His distinction: `,` is an expression separator, `;`/newline are statement
terminators. What separates the categories is how they treat **emptiness** — a
terminator repeated terminates nothing twice (blank lines, `;;`), while a
separator asserts an expression on each side. The DD had let `,` inherit the
terminator privileges (run-collapsing, being swallowed by a pending operator)
it never had.

**Fix:** `skip_separators` → `skip_terminators` (`Sep` only); the sequence fold
takes at most one comma per boundary and requires an expression on its far
side; the empty-expression escape (there for `for (( ;; ))`) admits terminators
only. Every DD#94 readability feature is untouched — `94.1`–`94.8` prove it.
`(( a = 1 ⏎ , b = 2 ))` is still fine, which is better than I predicted to DG.

**`atok_to_text`'s cold arms are mostly unreachable, by construction.** Two
callers; `expect()` is only ever called with `Colon`/`RParen`; the operand-site
fallback never sees the operand variants, the unary prefixes, `++`/`--` or
`LParen`. The total match is deliberate (a new `ATok` should be a compile error,
not the next leaked variant name) — but totality proves an arm *exists*, not
that it says the right thing. Nothing checked `LtLtEq` spells itself `<<=`. So
it gets a table unit test over every variant plus a count assertion. That takes
the function to 100% under the union run and pins what totality cannot.

**Gates:** `mk test` 52/27/20. Sweep 6181 → **6201**, exactly as predicted,
0 failing, 0 flips vs `16f42ed`. Revert proof: six new-behaviour specs fail
without the fix (I predicted five — `$(( 1, ))` also passed the old parser),
four shields hold either way. Specs `94.31`–`94.40`, journal 398.

**Next:** the coverage queue, rebuilt from the union run at `b4981d7` —
`emit_attest` (7), `emit_capture_end_c` (7), `read_arg_value` (7 of only 11
lines — densest), `handle_close` (6), `emit_redirects_guarded` (6),
`lower_value_as_str_aggregate` (6), `parse_test_binary_tail` (6), `emit_match`
(5), `handle_cd` (5), `lower_value` (5). Treat any queue derived before
`b4981d7` as suspect — it was ranked off spec-only coverage, and 3 of 7 items
in the previous one were phantom.

## ══ STATUS (2026-09-24) — two renderers, one message ══

**A passing spec asserted the message; the only line that spells it was
cold.** The coverage queue flagged `check_scalar_assignment`'s `String ← Int`
arm (*"cannot assign int value to string variable"*). `spec/types.sh::hint.4`
asserts exactly that text and passes, and `git grep` finds exactly one emitter
— so the counter looked wrong. It wasn't.

**The counts said something precise enough to test.** 1007 calls, 505 into the
first arm, 502 evaluations of the `else if`, **zero** times true. A temporary
`eprintln!` settled it: the declaration form `str s = $n` enters the arm;
`hint.4`'s assignment form `s=$v` produces the identical sentence **without
entering it**.

**Two renderers.** `check_assign_type_match` (`type_check.rs:1341`) builds the
message *parameterised* — `"cannot assign {} value to {} variable"` out of
`type_name()` — so a literal grep for the finished string cannot see it.
Assignments go there; declarations go to `check_scalar_assignment`. `hint.4`
covered the first. The second had no spec at all.

**Grep-the-message is not a coverage argument.** The cheap checks all agreed
with each other and were all wrong; only line counting could tell a sentence
from the line that prints it.

**Landed:** `decl.24`-`decl.28` in `types.sh` — the three reaching shapes
(`$n`, `$(( ))`, `$(f)` for `f -> int`) plus two controls (`str s = 5` still
declares; `str s = "$n"`, the advised form, runs). Non-vacuity by **mutation**
(`false &&` on the arm) since there is no source change to revert: exactly
`decl.24/.25/.26` fail, both controls hold.

Sweep 6171 → **6181** as predicted, 0 failing, 0 flips. `mk test` 51/27/20.

**Follow-ups taken (DG):** the duplicate renderer is collapsed —
`check_type_match` is now the only place a scalar string↔int mismatch is
diagnosed, `check_scalar_assignment` is gone, both entry points kept; messages
byte-identical, sweep 6181, **0 flips**. `mk cov-db` now reads
`target/coverage/union.lcov` (the stated 100% target is the union; it had been
hard-coded to the spec-only `branch.lcov`, so every queue so far was ranked off
the wrong file). `covdb.py` now **accumulates** — it keyed `cov_lines` on
`(file,line)` with `INSERT OR REPLACE`, so a repeated lcov entry ending in `0`
silently erased a real hit count and the line read as dead code.

**Queue rebuilt from the union run.** `mk cov-union` + `mk cov-db` done;
binary reset and verified uninstrumented. Union totals: **96.90% lines,
98.79% functions, 97.61% regions, 90.99% branches**.

**Three of the seven queued items were phantom** — `panic_payload_message`,
`joined_form_to_string` and `opaque_form_to_string` are at **0 uncovered**
under the union. They were never cold; the spec-only ranking just could not
see the unit tests that reach them. That is the switch paying for itself
immediately, and a caution about every queue derived before it.

**Next:** `atok_to_text` (41 — read checkpoint 035 on the rival brief
renderers first), then `emit_attest` (7), `emit_capture_end_c` (7),
`read_arg_value` (7 uncovered of 11 lines — the densest), `handle_close` (6),
`emit_redirects_guarded` (6), `lower_value_as_str_aggregate` (6),
`parse_test_binary_tail` (6). Same method: a cold line is a hypothesis, and
`contrib/cov-probe.sh` settles it in a second — aim it at the *condition*,
since it reports branch records.

## ══ STATUS (2026-09-24) — one of six ══

**The check was wired to one of DD#74's six boolean contexts.**
`type_check.rs::check_condition_type` emits SB4030 (*"int function used in
boolean context"*) and had exactly one caller — the direct `if` condition.
`elif`, `while`, `until`, `! f` and `&&`/`||` operands all accepted an int
function **silently**: `return 7` is status 7, which is **falsy**, so
`while f` never looped and `if ! f` took the true branch.

**SB4016's advice never compiled.** It said *"handle with if/||"*; following
that landed on SB4030. I expected this to need a type-aware split and DG
approved one — implementing it showed the premise was wrong. The gate is
`function_non_void`, written only by `analyze_function_returns`, which returns
early for anything not `-> int`. Every name it has printed was int-typed, so
the advice was wrong **every time**, not sometimes. The fix is unconditional;
the conditional I wrote was dead before it ran once. Found by a probe coming
back *clean* where I expected a diagnostic.

**One pass, not five call sites.** DD#74's rule is position-sensitive — in a
condition the whole `&&`/`||` list is a predicate (tail included); in a bare
statement the tail is not. `collect_int_fn_predicates` mirrors the traversal
`validate::mark_predicate_operand` already uses for the definite-return rule,
so the two cannot drift. Collect-then-emit: a statement in a condition is
reached twice and *both visits are correct*.

**A DD collision, escalated.** The sweep — not the probes — found
`e.b70.fndisc`: `f || :` is DD#70's documented discard for an `-> int`
function and, by DD#74, a `||` operand. DD#70's principle was already partly
dead (it prescribes `if` for int functions, which SB4030 has always rejected)
and its own 2026-09-09 addendum already preferred `-`. **DG took the
rejection** — one rule, replacement already legal. DD#70 addendum added,
scoped to `-> int` only; `|| :` is untouched for `match`/`[[ ]]`/externals.

Specs **74.38-74.48** (functions.sh, plan 114→125) + rewritten `e.b70.fndisc`.
74.44 (a *middle* clause) and 74.45 (the tail must draw SB4016, **not**
SB4030) are the load-bearing ones — without 74.45, "reject everything" passes.
Non-vacuity by execution: reverted, exactly seven fail.

Sweep 6149 → **6171** (+22), 0 failing, 0 flips. `mk test` 51/27/20.

**Two bugs from eight cold lines, and neither was the cold lines.**

## ══ STATUS (2026-09-24) — `until` was `while` ══

**A silent miscompile, found by a coverage probe.** `until` with a
**user-function condition** was exactly `while` — inverted against bash in
both directions. Truth table by execution: `until p` with `p` returning 0 ran
3 iterations where bash runs 0, and 0 where bash runs 3. Correct for `(( ))`,
`[[ ]]` and external commands, which is why `cf.u1`/`cf.u2` (both `(( ))`)
passed over it indefinitely.

**The negation was emitted, not dropped.** `--emit-c` showed it present in
both the broken and working cases. It was on the wrong **block**:
`lower_while_stmt` appended the `NegateTestResult` sigil to the loop
*header*, whose comment claimed that "works for ANY condition form" and then
listed the three forms whose condition happens to both start and finish
there. A user-function condition is inlined, so the header ends in a `Goto`
and the negate fired on the previous iteration's value, which the callee then
overwrote.

**Fixed at the forced location.** `lower_while` already computes `cond_exit`
— the block it puts the `Loop` terminator on — so the sigil goes there. This
also fixes a **second instance**: `cmd | until readline v { }` reaches
lowering via `pipelines::lower_pipe_while_read`, which never applied the old
patch at all. That is the spelling SB5069 steers you toward.

**Specs cf.u4-u7** (`control_flow.sh`, plan 66→70). u4/u5 pin *both* rows —
the old behaviour was a swap, so one row alone can be satisfied by a compiler
wrong the other way. u6 is the pipeline route; u7 is a regression shield for
the forms the fix moves the sigil away from. Non-vacuity proven by execution:
with the source fix reverted, u4/u5/u6 fail and u7 passes.

Sweep 6141 → **6149** (predicted +8), 0 failing, 0 flips. `mk test` 51/27/20.

**Filed, not fixed:** `bug-int-fn-boolean-context-one-of-six` — the probe
that started this. `check_condition_type` rejects an `-> int` function in
boolean context from **one** caller; DD#74 names six. Plus a diagnostic loop
(SB4016 advises `if`, SB4030 rejects it). Fixing newly rejects programs that
compile today, in five syntactic positions — **DG's call**.

## ══ STATUS (2026-09-24) — the lookup that never succeeded ══

**Coverage queue: `transform/capture_purity.rs::command_is_pure`, 8 cold
lines.** Seven carried a comment from journal 365: *"this lookup never
succeeds in a full sweep — 156 calls reach here and every one falls through."*
Honest measurement, dangerous comment — it states a fact about the **suite**
while reading as a claim about the **code**, which is exactly what an audit
for dead code acts on.

**It was a spec hole.** `capture_func_into` hands the analysis the callee's
body **unlowered** (inlining is three lines later at `lower_stmts`), so a
capture of a function that *bare-calls* another function reaches the walk —
`fn inner { /bin/echo hi } fn outer { inner } str t=$(outer)`. Nobody had
written that shape. Three specs now do: **cap.19** (pure), **cap.20**
(disqualified), **cap.21** (the doc's headline nested-capture case).

**cap.20 is the load-bearing one, and cap.19 is not.** cap.19 prints `[hi]`
whichever way the analysis answers — a failed lookup falls through to `true`
and still buffers. Proved cap.20 non-vacuous by hand-patching the emitted C
onto the buffered path and linking it: correct `[hi]` vs forced-buffered
`hi\n[]`. Nearly shipped a vacuous version — with a bare `echo "$t"` the wrong
answer prints `hi\n\n`, which the harness's `$( )` strips back to `hi`.
**A coverage test and a regression shield are not the same artifact.**

**The other two cold lines are backstops, proved by execution.** Non-literal
head → SB5011 (checked against a quoted name, a bare variable, a braced
expansion); cycle guard → SB5037 (checked with mutual recursion). Both now
name the code that guarantees them. Worth keeping: a validator guarantees
unreachability and the cost of guessing is a *mis-buffered capture* — a wrong
answer, not a crash.

**Harness bug found and fixed.** `misc.sh`'s `run_sash_emit_ir` called a bare
`sash`, not `${SASH:-sash}` — the only helper in the suite that did. Standalone
it ran the *installed* binary, and mine was stale enough to still emit
`:end-block nil`, so `104.5` failed with precisely the regression it exists to
catch, on a binary not under test. Eight tests affected; AGENTS.md's "pass
`SASH=` explicitly" silently did not work for them. Also given the
`timeout`/`</dev/null` discipline the other helpers have.

Gates: `mk test` 51/27/20; sweep **6141** (predicted +6), 0 failing, 0 flips
against `265b5dd`. Journal 394.

## ══ STATUS (2026-09-24) — AGENTS.md was quoting a dead compiler ══

**DG: "correct it; drop numbers from AGENTS.md that are prone to go stale."**
The residuals table claimed three `sash/src/transform/` functions under-report
against Mozilla by 2–9 points due to a tree-sitter grammar skew. It cost me a
7× over-estimate when costing the `for k v in m` task.

**Re-measured, the table was wrong twice over.** `rust-code-analysis-cli` vs.
the index across all of `sash/src` — 1901 functions, 45 files, 1620 agreeing
exactly. All three named functions agree with Mozilla **exactly** (`lower_one`
17/1, `lower_case` 2/1, `lower_for` 5/4): the magnitudes were stale because
commit `1c0e64a` deleted the CST those giant matches walked, and the skew
rationale does not apply to them either.

**The real residual is one-sided and small.** Cyclomatic: 119 low, **0 high**
— indexnudge never reports a cc above Mozilla's, so an indexnudge cc is a
floor. Cognitive: 158 low, 16 high, the high tail being deeply nested parser
code. All within a few points. None of it touches the enforced gate, which
compares indexnudge against indexnudge.

**Install gotcha now recorded:** `cargo install rust-code-analysis-cli` fails
with E0308 (two incompatible `tree_sitter::Language` types). **`--locked` is
required.** AGENTS.md tells readers to run this tool; the obvious invocation
does not work.

**The general fix.** Table replaced by the direction, the remaining structural
cause, and the re-measure command. Same treatment applied to every
current-state number in the file: DD count, journal file count, `transform/`
and `codegen/` file counts, clause rows, the `c:` cohort size, native-only
test count, `runs` row count and index size, `sash-rt` coverage percentages,
the unit-test breakdown, "4611 tests against 77 units", the cc/cog means (the
re-measure query was already underneath them), and `main.rs`'s "4-line".

**Historical numbers stayed** — the ghost-test incident, `ran=323` vs 321, the
546-test redux breakdown, the 9.0 GB pack. A historical number cannot go
stale; a current-state number always will. Where a number is useful, the file
now carries the query, not the answer.

Journal 393. `notes/tasks/for-map-two-binder-form.md` incidental finding
closed out.

## ══ STATUS (2026-09-23) — correction: a faithful quote, impossible advice ══

**DG caught a bug in the commit below.** `$(keys "config")` can never work —
`"config"` is a string, and `keys` takes only a bare map name. So rendering
the operand as the user spelled it produced a message that names their program
accurately and then advises `for k in $(keys "config")`, which SB5062 rejects.
I fixed the quotation and broke the advice.

**The distinction that settles it.** SB5025 is a *context* rule (this splat is
in the wrong place); SB5062 is an *operand* rule (this is not a map name).
When both are violated the operand rule is more fundamental, and reporting the
context rule *while paraphrasing the bad operand into a suggestion* is worse
than either alone. Since a `Literal` is the only operand spelling that can ever
be valid, it is the only one this diagnostic may quote — it quotes it back **as
advice**. `keys_splat_diag` now returns `Option` and defers everything else.

**It closed a positional inconsistency too.** The same operand error reported
two codes depending on position — `for k in $(keys "config")` said SB5062,
`echo $(keys "config")` said SB5025. Both now say SB5062. Verified by execution
that nothing slips past to the transform: the deferral hands off to a check
that really fires.

**And it reverted the visibility widening.** With the operand restricted to a
`Literal`, `validate.rs` needs no renderer — `ExprKind::Literal(name) =>
Some(name)`. `pub(crate) mod expressions` and `pub(crate) fn expr_to_string`
are both back as they were. The previous commit widened module surface to
reuse a renderer for a job that, correctly understood, needed no rendering.

**Gates.** `mk test` 51/27/20. Sweep **6135** (predicted, +2×1), 0 failing,
0 flips. `33.48` rewritten to pin SB5062 instead of the quoted render;
`33.49` added for the for-in position.

**The lesson.** The entry below credits reading what a probe printed on the way
past. This is the sequel: I read it, fixed what it showed, and never asked
whether the *fixed* output described a program that could exist. A diagnostic
that quotes the user's code makes a claim; one that suggests a replacement
makes a promise. I checked the claim and not the promise.

## ══ STATUS (2026-09-23) — a diagnostic that named the wrong map ══

**Where it came from.** The coverage queue, not a bug report. The cold-line
ranking put `transform/expressions.rs` on top — the `expr_to_string` display
renderer and its two helpers, 21 cold lines. The gap turned out to be an
illusion: `every_expr_kind_renders` covers all thirteen `ExprKind` spellings
and both `unreachable!` guards have catch-unwind tests. **`mk cov` excludes
unit tests by design**, so those lines are cold structurally, not untested.

**The real question about that file.** It is a *display* renderer with 24
callers, several of which make **semantic** decisions on the rendered text —
`is_array_read_last` compares it to `"read"`, `body_calls_user_function` looks
it up in `function_bodies`, `is_attest_left` matches `"attest"`. A
`DoubleQuoted` renders with its quotes, so a quoted command name would defeat
every one of them. Probed: it cannot happen. **SB5011 rejects a quoted command
name outright**, and for `keys` — whose operand is not a command name —
SB5062, SB4045 and SB4030 reject every non-literal operand shape. Real hazard,
closed at four gates. Worth knowing it is closed *deliberately*.

**The bug, found in the probe's control case.** `echo $(keys config)` reported

> `` `$(keys m)` `` is an array splat … (`for k in $(keys m)`)

against a program containing no `m`. Not a lossy rendering like SB5007 — a
**fixed exemplar** compiled into the message, right only for a reader who
named their map `m`. I had, one probe earlier, which is why it took a second
program to see it. Its `$(argv)` sibling four lines up is correct for the
opposite reason: `argv` takes no operand, so fixed text is accurate there.
One of the pair needed rendering, one never did, and both were written as
though the question had not arisen.

`keys_splat_diag` now renders the operand through `expr_to_string` — the same
display renderer, doing the job it is unambiguously right for. `$(keys config)`
names `config`; `$(keys "config")` keeps the quotes; the suggested `for` line
is now pasteable. Needed `pub(crate) mod expressions`, following
`expr_handlers`, already `pub(crate)` and already imported by `validate.rs` —
reuse rather than a fourth rival renderer.

**The complexity gate earned its keep.** `argv_splat_self_diag` was cc 6 /
cog 6 against a 5.7 bar and I had touched it. Extracting
`quoted_splat_self_diag` and `argv_splat_diag` took it to **4 / 3** and left a
flat list of three named cases — what it always was underneath. Not bypassed.

**Gates.** `mk test` 51/27/20. Sweep **6133** (predicted, +2×2), 0 failing,
0 flips. Three id checks clean; source-derived total agrees exactly.
Shields `33.47` (names the declared map, and does not contain `$(keys m)`)
and `33.48` (quoted operand rendered as written).

**Next.** Coverage queue continues: `transform/capture_purity.rs ::
command_is_pure` (8 uncovered, 78.26% lines / 50% branches — thinnest file in
the report), then `type_check.rs :: check_condition_type` /
`check_scalar_assignment`.

## ══ STATUS (2026-09-23) — a mutation diagnostic names its target ══

**DG's ruling.** I reported one loose end from the work below — `arith_brief`
still elided `Paren`, so `if (( y + (x *= 2) ))` read `mutation y + ...` — and
framed it as a narrow choice about parentheses. DG answered past the frame:
**report the target of the mutation, e.g. `x`; like the other error messages.**

That is the rule he had already given for the decomposed site, applied to all
of it. I had treated `mutation of x` as a workaround for the one site that
*could not* reconstruct its operator, and kept quoting where sites could. But
capability was never the question: a quotation invites comparison with the
source, and four of the five mutation sites could not produce one that matched
— one elided the culprit, one quoted an operator the parser synthesised, one
named nothing at all. Naming the target makes all three unrepresentable.

It also makes two spellings of one program agree. `(( x = y -= 2 ))` and
`(( x = (y -= 2) ))` now report identically; they never did.

**The split that falls out.** `arith_brief`'s two surviving callers both quote
a **non**-mutation — a bare comparison, and a comma part required to be a
mutation and isn't. There the expression *is* the complaint. Mutation sites
name; non-mutation sites quote. One question decided both.

**Bug 4 — the fifth site, found by applying the rule exhaustively.**
`reject_nested_mutation` is a mutation reporter, and I had edited it an hour
earlier without noticing. Its body asked `arith_has_mutation` then rendered
with `arith_brief`: a predicate and a renderer derived separately, the exact
shape of every bug below. Collapsing them into `mutation_target_in` (whether
and which, one traversal) put the outer `match` under my eyes — and
`SubscriptAssign` was not in its list. So `(( a[0] = y++ ))` fell past the gate
to `check_arith_value_mutation` and was told *"an assignment or `++`/`--`
inside `$(( … ))` can't take effect"*. There is no `$((` in that program. The
scalar `(( x = y++ ))` named `y` correctly the whole time.

**Two halves of one construct disagreeing is the tell** — and it is the second
time in two days the subscript half was the missing one. Third partial
re-derivation of a tree walk in this file; the remedy is prescribed in a doc
comment thirty lines above the hole. *Writing down the lesson does not apply
it.* Verified pre-existing by rebuilding the stashed tree.

The outer `match` stays **selective** on purpose: it names the nodes that
evaluate a sub-expression as an rvalue, not every node — descending blindly
would reject `(( (a=2) ))`, which the transform hoists and DD#31 allows.
Totality is the goal for a *renderer*; for a *gate* the goal is that the list
be complete for the property it names.

**A green sweep stopped covering three arms, silently.** With the condition
site retargeted, `arith_brief`'s mutation arms dropped to **zero hits across
all 6107 tests**. Third measurement of these arms, third "live, but only from
one caller" — the first two called them dead. Distrusting the zero found the
shape: `classify_arith_expr` recurses only into `Comma`, so a comparison
`BinOp` can still hold a mutation operand, and the **non-final comma** part
reaches all three arms for every operator. The bare-statement site reaches only
the comparison-classified ones — `(( 1 - x = 2 ))` compiles clean — so
rewriting the ten operator specs to use it would have silently retired four.

**A shape that reaches a cold arm is not a property of the arm. It is a
property of its callers**, and it expires when a caller changes.

**Shields.** `31.118`-`31.128`: the `--` direction at the site that still
quotes; the `SubscriptAssign` bug with its scalar control, compound spelling,
a cell as culprit (`b[…]`), a regression shield that a clean cell assignment
still runs, and the borrowed message still belonging to the construct that
earned it; then the ternary, the loop-increment idiom, and two-paths-one-
sentence. Rewrote `31.63`, `31.69`-`31.74`, `31.95`-`31.106`, `31.117`,
`cov.150`, `cov.151`, `cov.247`.

**Gates.** `mk test` 51/27/20. Sweep **6129** (predicted, +2×11), 0 failing,
0 flips against the `d4d13e3` baseline. Three id checks clean; source-derived
total agrees exactly. Journal 392 addendum.

**Next up.** Cold-line queue: the `transform/expressions.rs` cluster
(`joined_form_to_string` 8, `opaque_form_to_string` 7, `expr_to_string` 6 —
probe together; the display renderer behind the `argv` ICE), `check_condition_type`
(8), `check_scalar_assignment` (8), `emit_attest` (7), `panic_payload_message` (7).
`coverage.db` is stale.

## ══ STATUS (2026-09-23) — one renderer, and the invariant it broke ══

**Just completed.** The cold-line queue pointed at `ir::arith_op_str` (10 of 19
operator arms never rendered). The arms were fine. Everything below came from
reading what the probes printed on the way past.

**A probe that could not fail.** First run reported all twelve operators
compiling clean — which would have meant ten live arms were dead code. `sash -c`
takes the script **inline**; I handed it a *pathname*, so every run compiled a
one-word program that happened to be a valid absolute command. **Every probe
harness now leads with a case that must fail**, and I read that line first.

**Bug 1 — SB5007 quoted a program nobody wrote.** Two sites emit identical
text and only one is honest. `validate_assignment_stmt` sees only the
*decomposed* form — `lower_arith_mutation_structured` collapses `x = e`,
`x op= e`, `x++` and `a[i] op= e` into `Assignment { op: Set, value: BinOp }`
on purpose — and called `assign_brief` regardless. So `if (( x++ ))` reported
`mutation x = ...` (not an assignment at all) and `if (( a[0] -= 2 ))` reported
`mutation a = ...` (the whole array). Reconstructing the operator is impossible:
`(( x = x - 2 ))` decomposes identically to `(( x -= 2 ))`.

DG's call: **name the target, claim no operator** — `mutation of x`,
`mutation of a[…]`. One site, no provenance field in the IR. `a[…]` turned out
to be the existing house spelling at six other diagnostics; my first draft
rendered a literal index and was both inconsistent and dead.

**The bug was already written down.** `coverage.sh` cov.150's comment said
"A BARE `if (( x++ ))` reports `x = ...`" and treated it as an obstacle to
route around. That is the Bug policy's named failure mode verbatim. Comment
corrected.

**Bug 2 — three renderers, two independent half-fixes.** `arith_brief`,
`expansion_mutation_brief` and `brief_arith` all dispatched separately over the
same four `ir.rs` primitives. The same defect was found and fixed *twice*, in
two of them, each fix reaching one copy; `brief_arith` still had no
`CompoundAssign` arm. **DG: fix now.** `ir::arith_brief` is the only renderer,
total over all seventeen variants with the elisions listed explicitly rather
than reached through `_` — that `_` is how fourteen operators once printed `?`.

**Bug 3 — an invariant the compiler believed and did not check.** The unified
renderer takes `PreIncDec => unreachable!()`, which two of three rivals already
had. It fired at once. `reject_prefix_incdec` — the single function making
`PREFIX_INCDEC_INVARIANT` true — walked the tree by hand and ended in
`_ => Ok(())`, so **ten `++x` shapes** (ternary slots, unary operands, a
subscript-assign value) survived the parser and ICEd downstream. Verified
pre-existing by rebuilding the stashed tree and reproducing all ten. Now
delegates to `ArithExpr::children()`, which is total. The remedy is the one
`reject_nested_mutation` prescribes thirty lines below it: *"A partial
re-derivation of a shared predicate is how an ICE gets built."*

**An invariant asserted in a doc comment and enforced by a partial walk is not
an invariant.** `PREFIX_INCDEC_INVARIANT`'s doc now says what makes it true.

**Shields.** 28 specs: `31.90`-`31.94` target-naming per spelling,
`31.95`-`31.104` one per cold operator, `31.105`-`31.106` the audited siblings,
`31.107`-`31.116` one per `++x` shape, `31.117` the repaired renderer.

**Gates.** `mk test` 51/27/20. Sweep **6107** (predicted, +2×28), 0 failing,
0 flips against the 6051-row `58cae45` baseline. Three id checks clean;
source-derived total agrees exactly. Journal 392.

**Next up.** Cold-line queue, unchanged below the head: the
`transform/expressions.rs` cluster (`joined_form_to_string` 8,
`opaque_form_to_string` 7, `expr_to_string` 6 — probe together; that is the
display renderer behind the `argv` ICE), `check_condition_type` (8),
`check_scalar_assignment` (8), `emit_attest` (7), `panic_payload_message` (7).
`coverage.db` is now three commits stale.

## ══ STATUS (2026-09-23) — two diagnostics that named Rust, not sash ══

**Just completed.** Worked `tok_to_text` (18 cold arms, top of the queue). It
looked like pure test debt; two of three findings were bugs, and neither was
in a cold line.

Settled the 18 by **instrumenting once** (`eprintln!` at the head of the
function) rather than per-arm `panic!`s — one edit answers all of them, and
the panic-per-arm method does not scale past two or three.

- **5 paren arms** (`(`, `)`, `((`, `))`, `$((`) are intercepted by
  `step_arith_collect` before the sole caller. A DD#116 **rival**, and one
  already *out of step* — the interceptor is depth-sensitive, these were
  unconditional. Now `unreachable!()`, keeping totality.
- **`Comment` / `Eof`** keep: property 3. `Comment => ""` states DD#94's rule
  (comments are trivia inside `(( ))`), not the accident that the scanner
  strips them.
- **11 reachable, untested** → shields `31.75`-`31.85`.

**Bug 1 — SB4020 named a Rust variant.** `(( 1 >& 2 ))` reported
`found Amp` for a character the user typed as `&`; `expect()` said
`expected Colon`. Both built messages with `{:?}`. Fixed with `atok_to_text`,
a total match over all 45 `ATok` variants. Found by reading the output of the
shields I was adding, not by looking at a cold line.

**Bug 2 — a third copy of the keyword spellings.** `expect_word` derived a
keyword-as-identifier's text with `format!("{:?}", tok).to_lowercase()`, a
rival to `keyword_to_str` (the documented *only* reserved-word list) that
agreed solely because all eight variants are single lowercase words. Now calls
`keyword_to_str`; behaviour verified unchanged.

**Deliberately left.** DD#94 makes a comment inside `(( ))` a *deliberate*
divergence from bash, not the silent-drop bug it resembles — checking
`notes/decisions/` first is what stopped a false positive. Note the closing
`))` must be on its own line: a comment runs to end of line inside the
brackets too, so `(( a = 1 # x ))` is **unterminated** (DD#94 says so
explicitly; an earlier draft of journal 391 got this wrong — DG caught it).
Now pinned: `94.11`-`94.13` for `(( ))`/`$(( ))`, `94.29`-`94.30` for `[[ ]]`,
each with the working newline form as a control. The `[` → `$a[i]` hint
misfires on `[[` but is a documented heuristic for the common case.

**Filed, not fixed.** `dd94-bracket-close-hint` — the arith message explains
the comment-ate-your-delimiter cause, `expected ']]'` does not. Unconditional
advice there would misfire on every ordinary missing bracket (the SB4064 trap,
journal 389); an honest fix needs the scanner to report a comment on the line.

**Gates.** `mk test` 51/27/20. Sweep **6051** (predicted, +2×5 shields on top
of +2×15), 0 failing, 0 flips. Three id checks clean; source-derived total
agrees exactly.

**Next up.** Cold-line queue: `arith_op_str` (10), then the
`transform/expressions.rs` cluster (`joined_form_to_string` 8,
`opaque_form_to_string` 7, `expr_to_string` 6 — probe together, that is the
display renderer behind the `argv` ICE), `check_condition_type` (8),
`check_scalar_assignment` (8). `coverage.db` is current as of `1ec7f8d`, so
one commit stale after this one.

## ══ STATUS (2026-09-22) — a narrowing-by-default becomes a rule ══

**Just completed.** DG ruled on the one question 389 left open. `argv $n a`
(a runtime slice start) is **not** in the subset — a compile-time-constant
fill start is the rule, not a gap. `notes/tasks/argv-dynamic-index.md` closes
`wontfix`.

SB5085 now states the rule *and* names the alternatives. DG's call was that
the advice **can be generic**: I had begun splitting the message in two
(quoted index → "drop the quotes"; sigilled → "restructure"), and had moved
`expr_is_dynamic` from `transform/expressions.rs` to `ir.rs` to get the
predicate into `validate`. Both edits reverted. 389's lesson was advice being
*wrong for an input*; advice being *general across inputs* is not that bug.

Both alternatives in the message were measured against the live binary first
— the first draft did not compile, because everything `argv` fills is tainted
and the advice omitted the required `attest`. Advice that does not compile is
the same defect as advice that is wrong.

Also: `struct Case` in `ir.rs` now says in a doc comment that it is the DD#66
dispatch form, not bash `case`/`esac`.

**Gates.** `mk test` 51/27/20. Sweep **6011**, 0 failing (predicted — no specs
added). Three id checks clean.

**Next up.** The cold-line queue, from `coverage.db` — which is now **two**
commits stale; regenerate (`mk cov-branch` → `mk cov-db` → `mk cov-reset` →
`mk build`) before trusting line-level data. Ranked: `tok_to_text` (18),
`arith_op_str` (10), `joined_form_to_string` (8), `check_condition_type` (8),
`check_scalar_assignment` (8), `emit_attest` (7), `panic_payload_message` (7),
`emit_capture_end_c` (7), `read_arg_value` (7), `opaque_form_to_string` (7).

## ══ STATUS (2026-09-22) — a shadowed diagnostic and eight runtime ICEs ══

**Just completed.** Coverage regenerated after `b4549aa` (the Sep-21 database
was two commits stale and my own edits had shifted line numbers across five
files): **96.61% lines, 98.35% functions, 90.80% branches**. Three findings.

1. **A regression from the previous commit.** `check_argv_splat_decl`'s cold
   lines were the whole body of SB5025 — and that function is code DD#117 had
   just touched. SB4064 matches `array a=$(argv 1)` and runs in
   `run_structural_checks`, which **exits** before `run_semantic_checks` where
   SB5025 lives. Proved by execution (comment the call out, SB5025 returns).
   The messages are not interchangeable: SB4064's advice, "write `a=(…)`", is
   wrong here because `array a=($(argv 1))` is itself rejected. SB4064 now
   defers for this one shape. *A new check in an earlier phase silently steals
   every input it matches from the later phases.*

2. **`attest`'s two cold sites (DD#116).** `AttestPredicate` has two variants
   and `classify_attest_convert` only pairs `ArithConvert` with a single-name
   lead — the same call-site argument that has now been wrong twice, so it was
   settled by execution: two `panic!`s, ~20 shapes including every
   non-statement position (pipeline, cmdsub, `&&`, `!`, redirect, function
   body, `if`/`while` condition, `for` body), zero hits. `attest` is a lexer
   keyword with a dedicated production, so a malformed one is SB4014 at *parse*
   time. Predicate site → total `match` (property 2). Tail fallback → a
   **rival** to `parse_attest` with none of the three properties, deleted.
   It left `name` unused.

3. **`argv`: eight spellings compiled clean and ICEd at runtime.** Found by
   taking SB5025's own message literally — its `[N]` means *optional*:

   ```
   argv [1] a   →   sash: bug: bare command passed to sash_exec
   ```

   `try_handle_argv` returned `None` for any shape it could not lower, and
   `None` in `try_builtin` means "not a builtin", so the statement became an
   ordinary command named `argv`. Reached by `argv`, `argv 1 2 a`,
   `argv [1] a`, `argv -1 a`, `argv 1.5 a`, an `i64` overflow, and — worst —
   `argv "1" a` / `argv '1' a`, because the index is read with
   `expr_to_string`, a **display** renderer that keeps the quotes.

   `argv` was the only fill builtin with no shape check (`readline` SB5057,
   `signal` SB4059, `match` SB4045; `argv`'s own *target* got SB5063 in the
   `av.rej4/5` work — same bug class, target half). `check_argv_form` adds
   SB4045 for arity and **SB5085** for the index (bare non-negative decimal
   literal fitting `i64`). `handle_argv` now returns `BlockId` and states its
   bound with `unreachable!` — sound because every path to lowering runs
   `run_semantic_checks` (`lib.rs:176`) before `transform_or_exit` (`:183`).

**Gates.** Sweep **6011** (predicted), 0 failing, **0 flips**. `mk test`
51/27/20. 13 specs: `av.f1`-`av.f10`, `decl.21`-`decl.23`.

**Open for DG.** Rejecting a *sigilled* index (`argv $n a`) is a narrowing by
default, not a considered answer — `ArgvFill` carries a constant `start`.
Filed as `notes/tasks/argv-dynamic-index.md`. Also still open: whether
`sash-convert` should translate retired spellings and bash's `declare -a a=x`.

**Next.** Cold-line queue: `check_condition_type`, `check_scalar_assignment`,
`emit_attest`, `emit_capture_end_c`, `read_arg_value`, `handle_close`,
`emit_redirects_guarded`. (`command_is_pure` is classified: measured-cold,
a DD#116 property-3 keep.)

## ══ STATUS (2026-09-22) — a cold line was hiding a segfault ══

**Just completed.** Cold-line queue item `parse_one_decl_assignment` (the
`+=` arm, never executed). Probing the whole family rather than the one case
found that `array a+=(x)` **compiled clean and segfaulted** — and widening
again found it was never about `+=` at all:

| program | sash (before) | bash |
|---|---|---|
| `array a=x` | **SIGSEGV** | `x` |
| `array a=""` | **SIGSEGV** | empty, one element |
| `array a=$(echo x)` | **SIGSEGV** | `x` |
| `map m=v` | **SIGSEGV** | (accepts) |
| `array a+=x` | **SIGSEGV** | `x` |

`echo $(len a)` on `array a=x` returned **3888644095** — a wild read yielding
uninitialised memory rather than faulting. The mirror was silent: `str s=(x)`
gave `""` where bash gives `x`, and `str s+=(x)` gave `(x)`, so `=` and `+=`
disagreed with each other.

**Mechanism: an in-band sentinel.** `Literal("")` *was* "bare declaration, no
initializer", so bare `array a` worked while `array a=""` (which parses as
`DoubleQuoted([])`) missed the test and stored a SCALAR into a collection
slot. Nothing anywhere checked a declaration's initializer against the kind
declared, in either direction. Same empty-vs-meaningful discriminator as
journal 385.

**DG's ruling → DD#117.** Reject the mismatch both ways (SB4064); reject `+=`
in a declaration (SB4065) — *a declaration creates, it cannot append*, which
applies to `str s+="a"` and `int n+=1` too, not just the crashing collection
cases; and remove the sentinel rather than patch around it.

`Declaration.assignments` is now `Vec<DeclAssignment> { name, value:
Option<Expr> }`. No initializer is `None`; `+=` and an index are
**unrepresentable**. 29 sites touched; the four that legitimately treat
`int x=1` and `x=1` alike go through one `as_assignment()` adapter. The
`--emit-ir` seam drops `:op`/`:index` from declaration assignments.

Sweep **5985** (predicted), 0 failing, **0 flips** vs `9995bf6`. Units
51/27/20. 20 new specs (`decl.1`-`decl.20`, `spec/types.sh` 245 → 265).
Complexity gate caught three at once (`check_decl_value_shape` 9/7,
`slot_rebind` 8/9, `compute_declaration_value` 7/7 vs 11.68/5.83); all came
down by extraction, max now 5/4. Journal 388.

**Carry forward.** Zero coverage did not mean dead code — it meant *nobody had
ever written that input*, which is exactly where undefined behaviour lives.
The crashing spelling was never cold: line 887 ran 55 times. Probing only the
literal cold lines would have found `str s+="a"`, seen it work, and closed the
item.

**Next.** Cold-line queue: `command_is_pure` (documented measured-cold, a
property-3 keep), `handle_attest_stmt`, `check_condition_type`,
`check_scalar_assignment`, `emit_attest`, `emit_capture_end_c`,
`read_arg_value`, `handle_close`, `emit_redirects_guarded`.

**Open for DG.** Whether `sash-convert` should translate retired spellings
(`$'\0'`→`$'\x00'`, `16#ff`→`0xff`, `010`→`8`) — and now also whether it
should reject or rewrite bash's `declare -a a=x`.

## ══ STATUS (2026-09-21) — three cold branches that were never dead ══

**Just completed.** `build_expr_from_parts` (`parse/parser.rs`) had two of
three branches at zero coverage, and a tidy call-site argument said both were
unreachable. Instrumented all four cold regions in one build instead. Three
fired immediately: a compact dispatch operator whose **subject or pattern
spans more than one part** — `[[ $x$y==ab … ]]`, `[[ ab==$x$y … ]]`,
`[[ $x-$y==a-b … ]]` — is ordinary sash that the suite had simply never
written. Every existing compact-operator spec put a single part on each side.

Not a bug: all twelve shapes agree with bash's non-compact spelling
(`[[ "$x$y" == ab ]]`) on match, miss, both-sides, `:`/`-` middle literals,
the `=~` side, and boolean-`[[ ]]` parity. A **coverage gap**, now
`dp.mp1`-`dp.mp12` (`spec/dispatch.sh` plan 58 → 70).

The fourth (`parts.is_empty()`) never fired, and here the argument is closed:
two call sites, both slicing around a middle `i ∈ 1..len-1`. DD#116 says it is
not a rival and has none of the three backstop properties — it is the wrong
side of property 1, since `Literal("")` would manufacture an empty subject out
of a program that has neither. Now a slice `match` with an `unreachable!` `[]`
arm naming the bound (same move as the `is_fd_number` parse in 385).

Sweep **5945** (predicted), 0 failing, **0 flips** vs `ae26dc4`. Units
51/27/20. `build_expr_from_parts` cc 4 / cog 1. Journal 387.

**Carry forward.** Two commits running, a zero count plus a call-site argument
has been wrong about liveness. A zero count means "no input in the suite
reached this". Upgrading that to "no input can" needs an execution probe that
fails to reach it, or a closed argument over *all* call sites — and when the
closed argument holds, DD#116 decides whether the code stays, not the absence
of coverage.

**Next.** Cold-line queue continues: `parse_one_decl_assignment`,
`command_is_pure` (documented measured-cold, a property-3 keep),
`handle_attest_stmt`, `check_condition_type`, `check_scalar_assignment`,
`emit_attest`, `emit_capture_end_c`, `read_arg_value`, `handle_close`,
`emit_redirects_guarded`.

**Open for DG.** Whether `sash-convert` should translate retired spellings
(`$'\0'`→`$'\x00'`, `16#ff`→`0xff`, `010`→`8`).

## ══ STATUS (2026-09-21) — "proved dead", and live ══

**Journal 386.** Sweep **5921** (5908 passing + 13 skipped), 0 failing,
**0 flips vs `f7ba3d2`**, no retirements. Units 51/27/20. Predicted 5921.

The queued deletion of `arith_brief`'s two "dead" arms was **not done, because
they are live**:

```
if (( 1 > x = 2 ))    → Assign arm       if (( 1 > x += 2 )) → CompoundAssign
```

The filing said *proved* — cold in union coverage, then `panic!` in each arm,
then the full sweep, then 14 targeted probes. Every one of those 14 put the
mutation at **top level** (decomposed into statements before validation, so the
message comes from `validate_assignment_stmt`) or **in parens** (arrives as
`ArithExpr::Paren`, falls to `_`). The reaching shape is the unparenthesised
**right operand** of a binary operator — which nobody writes, because bash
binds `=` loosest and rejects `1 > x = 2` outright.

**Methodology finding: panic instrumentation + a green sweep proves "not
reached by these inputs", not "unreachable."** The suite is programs people
actually wrote; it says nothing about a grammar corner no author would enter.
The honest form names the probe set — and doing so is what made the gap
findable.

The precedence difference is **not observable**: all 11 contexts probed reject
(SB5007/SB4020/SB4041). No bug there.

With the arms live, their catch-alls became user-facing. DG chose **totality**:

```
mutation 1 > x ?= ...   →  mutation 1 > x *= ...
mutation y ? ...        →  mutation y + ...
$(( x *= 2 )): x = ...  →  mutation x *= ...
```

`ir::arith_op_str` is total over all 19 `IrArithOp`; `compound_assign_brief`
derives from it. Spec 31.63 had *pinned* the old `?` as a tripwire for exactly
this change — it fired, and is rewritten. The four independent renderings of
`"x = ..."` (validate.rs:2274 + :1050, parse/arith.rs:188 + :84) are now one
`ir::assign_brief`.

Specs 31.69-31.74 reach each arm, so a third mis-measurement costs a red sweep.

**Next:** the cold-line queue — `build_expr_from_parts`,
`parse_one_decl_assignment`, `handle_attest_stmt`, `check_condition_type`,
`check_scalar_assignment`, `emit_attest`, `emit_capture_end_c`,
`read_arg_value`, `handle_close`, `emit_redirects_guarded`. Treat every
"provably dead" note in `notes/tasks/` as *unverified* until re-probed with a
shape the existing probes do not share.

## ══ STATUS (2026-09-21) — `exit 1x` exited 0 ══

**Journal 385.** Sweep **5909** (5896 passing + 13 skipped), 0 failing,
**0 flips vs `8617e11`**, no retirements. Units 51/27/20. Predicted 5909.

DG asked whether substring is the only `var:` operator that takes an int.
It is — and enumerating every integer position in the language to answer
that turned up a second one that was not checked at all:

```
exit 1x        sash: rc 0 (success!)    bash: rc 2, "numeric argument required"
${s:1+1}       sash: abcdef             bash: cdef
```

Both were `parse::<i64>().unwrap_or(0)` at codegen with nothing upstream.
Nine such sites exist; **three** are reachable with garbage, found by
instrumenting all six candidates with distinct `panic!`s in one build.

Running the **whole sweep** under that instrumentation is what gave the fix
its shape: 8 specs failed, all bare `int n` declarations. That is an *empty*
literal, where `unwrap_or(0)` is not a fallback but the zero-init. The
discriminator is **empty = a value, non-empty unparseable = a bug** — not
reachable by inspection, and invisible to `eprintln!` (the harness captures
compiler stderr into `$err`, so the sweep stays green).

DG's ruling: **"the compiler should reject anything not fully understood, so
unparseable means reject."** Applied to both positions as **SB4030** — the
code already carrying "would silently become 0" in `type_check.rs`. No new
code needed. `${s:(-2)}` and `${s:1:}` still work, and must.

The codegen fallbacks are now loud in **both** backends via one shared
`substring_part_int` in `ssa.rs` — a second copy would be a DD#116 rival.
`--emit-c` could not catch this bug because both backends agreed on the
wrong answer.

13 specs: `sub.1`-`sub.10` (`types.sh`), `89.9`-`89.11` (`exit_arg.sh`).

**Next:** the DD#116 cleanups still queued — delete `arith_brief`'s proven-dead
`Assign`/`CompoundAssign` arms, dedupe the four `"{} = ..."` brief renderers.
Then the cold-line queue: `build_expr_from_parts`, `parse_one_decl_assignment`,
`handle_attest_stmt`, `check_condition_type`, `emit_attest`, `read_arg_value`.

## ══ STATUS (2026-09-21) — 35 token kinds were being deleted ══

**Journal 384.** Sweep **5883** (5870 passing + 13 skipped), 0 failing,
**0 flips vs `6dd9e90`**, no retirements. Units 51/27/20. Predicted 5883.

DG's convention question became **DD#116 — backstop vs. rival**: a backstop
sits downstream of an authoritative decision, a rival is a second independent
answer to a settled question. A rival is dead code however defensive it looks.

Applying it to `tok_to_text` found a **bug**, not a tidy deletion. The
function rendered 24 of 59 `Tok` kinds; the other 35 fell to
`_ => String::new()` and **vanished from the collected arithmetic text**
before the re-lexer saw them:

```
(( x = 1 done ))     sash: compiles, x=1      bash: syntax error
```

21 of 22 keywords, in both `(( ))` and `$(( ))`, plus `[[ ]] { } $(( <<< >&
<& <> &>` and `$'…'`. Fixed by **totality** — all 59 variants now render
deliberately, no `_`, so a new `Tok` is a compile error rather than a silent
drop. Specs **`31.64`-`31.68`**.

`;;` is the one token that legitimately reaches the renderer, from a
`for (( ;; ))` header. It was being deleted too, which made
`parse_arith_str`'s long-standing comment about it false; the fix made the
comment true.

**Open bug, awaiting DG.** The sweep DG asked for found a second, separate
divergence: `${s:1 2}`, `${s:1x}`, `${s:1+1}` all silently yield offset 0
(`abcdef` instead of `cdef`), and `${s:0:n}` yields empty. `emit_substring`
and `emit_substring_c` both parse with `unwrap_or(0)` at codegen. Both
backends agree, so `--emit-c` cannot see it. **This one emits a wrong value**,
where the first merely accepted a program. Question: reject the offset, or
support arithmetic there as bash does?

**Still queued from DG's DD#116 decisions:** delete `arith_brief`'s `Assign` /
`CompoundAssign` arms and dedupe the four `"{} = ..."` renderers.

## ══ STATUS (2026-09-21) — `arith_brief`: two dead arms, and one spec that missed ══

**Journal 383 (addendum 2).** Sweep **5873** (5860 passing + 13 skipped),
0 failing, **0 flips vs `980e26b`**, no retirements. Units 51/27/20.
Predicted 5873.

Target: `arith_brief` (validate.rs:2272), 9 cold lines — the renderer for
SB5007 message text. Three different things were behind those nine lines.

**A real gap (closed).** The `==` / `!=` / `>=` operator arms were cold while
`>` / `<` / `<=` beside them were hot; the suite just never spells those three.
Specs **`31.59`-`31.63`** added, pinning the rendered *text*, not only the code.

**Two dead arms (filed, not removed).** `Assign` and `CompoundAssign` are
unreachable. Cold is only a hint, so each was **proved**: arm body replaced
with `panic!`, full sweep plus aimed probes — **0 hits** from 5873 specs and 14
hand-written programs. Every caller is excluded by classification, and the one
that could deliver them is pre-empted by decomposition into
`validate_assignment_stmt`. Filed `arith-brief-dead-arms`, sibling to
`tok-to-text-dead-arms`; **DG's call** whether provably-dead arms come out or
stay as defensive documentation — worth deciding the two together as a
convention.

**Journal 382's mechanism was wrong (conclusion still right).** I wrote that
the *parser* desugars `x op= v`. `try_assignment` (arith.rs:959) does construct
`CompoundAssign`; the rewrite is in `lower_arith_mutation_structured`
(arith.rs:1314) via `lower_arith_command` — still parse-time, still before
validation, so **SB5084's reach is as verified** (re-checked by execution:
`<<=`, `>>=`, `/=`, `%=` all still report). Right conclusion, wrong stated
mechanism — the combination that nothing fails on.

**The trap, hit twice in one day.** I wrote `31.59` claiming it pinned
`arith_brief`'s `Assign` arm, then measured: **0 hits**. The identical text
comes from a hard-coded `format!` in `validate_assignment_stmt`
(validate.rs:1050). `"{} = ..."` is rendered in **three** independent places
(`arith_brief` — dead, `brief_arith` at parse/arith.rs:187, and that hard-coded
one), which is exactly why reading the message cannot tell them apart and why
the dead arm survived. That triplication is the part worth fixing. `31.59` is
kept with a comment naming the real producer.

**The point:** a bug, a spec that missed, and dead code all read identically in
a coverage report — "not covered". Only execution separates them.

**Next:** cold-line queue — `tok_to_text` (4 live arms; 4 filed dead),
`build_expr_from_parts`, `parse_one_decl_assignment`, `command_is_pure`
(documented measured-cold), `handle_attest_stmt`, `check_condition_type`,
`check_scalar_assignment`.

## ══ STATUS (2026-09-21) — union coverage refreshed; a spec that missed its target ══

**Journal 383 (addendum).** Sweep **5863** (5850 passing + 13 skipped),
0 failing, **0 flips vs `5a571bb`**, no retirements. Predicted 5863.

Fresh `mk cov-union` baseline (spec + unit, branch-enabled):
**functions 98.80% · lines 97.49% · branches 90.85%.** `coverage.db` rebuilt
from `target/coverage/union.lcov`; the old one predated four spec files and
was mis-ranking targets.

Re-ranking put `try_nullary_intrinsic` near the top: the `getppid` and
`random` arms cold, `getpid` and `epochseconds` beside them hot. Looked like
the morning's bug; it was not one.

**`cov.331-334` were written to close those exact two arms and never reached
them.** Measured per program via one profraw per compile:

| program | reaches |
|---|---|
| `a=$(getppid)` / `a=$(random)` | `int_intrinsic_i64` |
| `echo $(getppid)` / `echo $(random)` | `try_nullary_intrinsic` |

An **assignment** is int-lowered exactly like the arithmetic spellings. The
string path needs the **unassigned** spelling. `getpid`/`epochseconds` were
hot only because unrelated specs happen to spell them `echo $(…)`.

Added **`cov.335-338`** with the spelling that works, then **re-measured**:
558-561 went 0 → 2, 570-573 went 0 → 3. Confirmed, not assumed. Rewrote the
`cov.331-334` comment to state what they actually reach; ids kept, since the
semantics they pin are worth pinning on the int path too.

`cov.335` is the one worth keeping: `$(getppid)` via the string path and via
the int path must print the **same** number. The value is unpredictable, the
agreement is not — an int/string split is precisely the shape that invites two
implementations to drift.

**Lesson (third confident-comment failure this week):** a green spec is no
evidence it tested what you meant. For a coverage-motivated spec that claim is
mechanical, so measure it.

**Next:** cold-line queue on fresh data — `arith_brief` (9), `tok_to_text` (8,
4 of them the filed dead arms), `build_expr_from_parts`, `parse_one_decl_
assignment`, `command_is_pure`, `handle_attest_stmt`, `check_condition_type`,
`check_scalar_assignment`.

## ══ STATUS (2026-09-21) — the DD#58 sexpr operand slot was never type-checked ══

**Journal 383.** Sweep **5855** (5842 passing + 13 skipped), 0 failing,
**0 flips vs `da15d8c`**, no retirements. Units 51/27/20. Predicted 5855.

A cold-line target in `parse/arith.rs::tok_to_text` led to an ICE, uncompilable
C, and six silently-wrong answers — all behind one root cause.

`check_len_operand` and `check_string_intrinsic_operands` take a `&Command`, so
they only ever saw the `$(len …)` form. The transform reconstructs the
identical `Command` for the DD#58 sexpr form `$(( (len …) ))`, but the
transform runs **after** type check. That slot was the one place in the
language where the DD#75 operand rules did not apply.

| spelling | `$(…)` | `$(( (…) ))` before |
|---|---|---|
| `len s` / `len $s` (str) | SB4030 | 5 |
| `len n` (int) | SB4030 | **native ICE** / uncompilable C |
| `len $a` (array) | SB5064 | 3 |
| `strlen $a` / `strlen $m` | SB5064 | **0** |
| `strlen $n` (int) | SB4030 | 2 |
| `f $a` (user `-> int` fn) | SB5064 | ran |

`strlen $a` → **0** is verbatim the defect the DD#75 task said the len/strlen
split existed to remove. It survived one spelling over.

**Two specs had locked the bug in.** `58.12` and `58.15` asserted the retired
polymorphic `len`, while `convert.sh:560`'s comment says that form "does not
even compile". The suite contradicted itself — a green suite is not a correct
one. `58.14`/`58.15`/`58.16` were also **byte-identical** restatements of
`58.10`/`58.12`/`58.13`; DG chose to repurpose the ids rather than retire them.

**Fix: DG chose (A)** — complete DD#75 stage 2 into the sexpr path. Both halves
share rather than copy: type_check calls the transform's own
`reconstruct_intrinsic_command`, and validate walks arith `FuncCall` args via
`ArithExpr::children()`. The SB5064 half is deliberately **not** limited to
`len`/`strlen` — `$a` on a collection is wrong in any argument slot, which is
what caught the user-int-function case.

Ordinary arithmetic was never affected: `$(( $a + 1 ))` has always been SB4030.
Only the call-argument slot escaped, because `walk_all_exprs` reaches the
`Arithmetic` node but not the `ArithExpr` tree inside it.

**Also found, not acted on:** four `tok_to_text` arms (`LParen`, `RParen`,
`DblParen`, `DblParenClose`) are unreachable — `step_arith_collect` handles
those tokens itself. And `check_command` runs 3 operand checks while
`check_subscript_in_stmt` runs 4; only the latter calls `check_len_operand`.

**Next:** continue the cold-line queue. `coverage.db` is stale for eight files
edited this session — refresh before the next ranked target.

## ══ STATUS (2026-09-21) — SB5084: a proven negative shift count ══

**Journal 382.** Sweep **5847** (5834 passing + 13 skipped), 0 failing,
**0 flips vs `587281c`**, no retirements. Units 51/27/20. Predicted 5847.

DG closed yesterday's open item: *"yes to proven negative as compile-time
error."* A negative shift count the folder can prove is now **SB5084**,
alongside SB5077 for a proven zero divisor.

**The implementation choice was the substance, not the diagnostic.** Copying
`walk_arith_divisors` (34 recursive lines) and swapping two arms would have
worked — and would have created two definitions of "what the folder
proves", drifting the first time someone adds an `ArithExpr` variant and
updates one. The walk **is** the reach of the check. So it generalised to
`walk_arith_binops` + one classifier `const_arith_fault`; both faults are now
filters over a shared traversal. Net **+21 lines** in `validate.rs` for a
second fault check.

That bought two behaviours I did **not** have to decide:

- **Compound assignment needs no arm.** I nearly wrote one; `--emit-ir` shows
  the parser desugars `x <<= -1` into `Assign { value: BinOp { op: Shl } }`,
  so it arrives as an ordinary BinOp. The clue was already visible —
  `(( x /= 0 ))` reports SB5077 today despite the walker having no
  compound-assign arm. An arm would have been dead code that *looked*
  load-bearing.
- **Dead ternary branches error.** `(( x ? 1 : 8 >> -1 ))` is rejected even
  though the branch never runs — inherited from SB5077's long-standing
  behaviour, not chosen. Sharing the walk means it could not have diverged.
  DD#25's "a property of the expression as written" is the justification.

| case | fault |
|---|---|
| `8 >> -1` · `8 >> (1-2)` · `x >>= (1-2)` · dead ternary | SB5084 |
| `int n=-1; x >> n` (not provable) | runtime trap |
| `8 >> 64` | **not a fault** — 0 / sign |

**One stale comment caught by rereading my own work:** spec `25.17`, written
yesterday, said a literal negative count "defers to the runtime trap" — a
path that no longer exists. Second time in three sessions the wrong thing in
the tree was a *comment* (DD#25's stale gap list was the first). A comment
describing an old path is worse than none: it is confident.

Specs `25.18`–`25.21`; `25.15` now names why it is the runtime half.
DD#25's "Open (not decided)" item is closed.

## ══ STATUS (2026-09-20) — Go-like shifts; brace-less @ stays literal ══

**Journal 381.** Sweep **5839** (5826 passing + 13 skipped), 0 failing,
**0 flips vs `4703796`**, no retirements. Units 51/27/20. Predicted 5839.

DG ruled on the last open DD#25 item. "Shift" was hiding **three** questions:

1. **Count masking — changed.** Hardware masks the count to 6 bits, so
   `1 << 64` was `1`, `1 << 200` was `256`, `8 >> 64` was `8`. A register-width
   property, not arithmetic, and the exact silent-wrong class DD#25 rejects. A
   count ≥ 64 now shifts every bit out.
2. **Negative counts — changed, and were worst.** `8 >> -1` was `0`;
   `8 >> -64` was **`8`, an identity**. Now aborts: `negative shift count`,
   exit 2, via `sash_arith_negshift`.
3. **Sign fill on `>>` — deliberately NOT changed.** I argued the other way
   here and DG took it: on a *signed* type the zero fill is the **more**
   representation-dependent choice. `-8 >> 1` = `-4` = `floor(-8/2)`; a zero
   fill gives `9223372036854775804`, meaningful only if you reinterpret the
   sign bit as a value bit. So `>>` saturates to the sign (`0` / `-1`).

Unbounded non-negative counts = **exactly Go's rule**, reached from DG's
framing rather than borrowed from Go (checked after, not reached for).

**Four sites must agree**, pinned by `25.17`: native `emit_shift`, C
`emit_shift_c`, and `fold_arith_op` — which `validate.rs::const_eval_arith`
*shares*. The folder declines to fold a negative count and defers to the
runtime trap, following DD#25's existing rule for proven overflow. In C the
same guard is also what keeps the emitted program free of UB.

**Brace-less `@` stays literal** (DD#90 addendum). `y@{a:-x}` → SB5083;
`y@a:-x` → literal, bash-identical. Decided by one measurement:
**`user@host:~/dir` matches the brace-less trigger exactly** — the
scp/rsync/ssh spelling, not a corner case. A prefix rule would try to splat an
array named `host`. The asymmetry is principled: braces are an *explicit
request* for splat machinery, so the strictness belongs on that spelling.

*Transferable:* I had flagged this for several sessions as "would break a
currently-accepted program" without ever asking **which** program. One probe
turned a shrug into a reason — and reversed the recommendation.

Specs `25.13`–`25.17` (`25.13` is a **rewrite** — it pinned the old
behavior, which is what made this a visible decision) and `gs.52`–`gs.55`.

**Open (not decided):** whether a folder-*proven* negative shift count should
be a compile-time error like SB5077 for `/0`. Recorded in DD#25.

## ══ STATUS (2026-09-20) — nullary intrinsics in string context ══

**Journal 380.** Sweep **5823** (5810 passing + 13 skipped), 0 failing,
**0 flips vs `b4f2a01`**. Units 51/27/20. Predicted 5823 before running.

`try_nullary_intrinsic`: `getppid` and `random` were cold while `getpid` /
`epochseconds` beside them were hot. Both *are* spec'd — but only through
**arithmetic** (`$(( (getppid) > 0 ))`, `int n=$(random)`), which takes a
different lowering than the `emit_int_to_str` string path. A value spec'd as a
number, never as a string. No bug (`$(getppid)` returned the invoking shell's
exact PID; 200 draws all distinct, none negative, both backends).

`cov.331`–`cov.334` assert **semantics, not shape**: getppid is stable within
a run *and* differs from getpid; random varies (catches a stuck generator);
random is non-negative — verified **guaranteed by construction**
(`sash_random` ends `& i64::MAX`, DG 2026-07-07) before pinning it, since
200/200 clean draws alone would be a flake waiting to happen.

*Transferable:* when structurally identical match arms split hot/cold, the
difference is the **context the tests reached them through**, not the code.

## ══ STATUS (2026-09-20) — the compiler's own names, and a rotted gap list ══

**Journal 379.** Sweep **5815** (5802 passing + 13 skipped), 0 failing,
**0 flips vs `c1370ce`**. Units 51/27/20. Predicted 5815 before running.

### `from_name`'s nine sigil arms: reachable only from source. No bug, no spec.

`sigil_str` (enum→string) is hot; `from_name` (string→enum) was cold for all
nine `__`-prefixed opcodes, because the compiler builds them by **enum** and
never parses them back. The only way in is **a user writing the name** — a
namespace-collision surface. Probed all nine in five positions across
bash/native/C: the user's function wins every time, a bare sigil is an
ordinary SB5011. Correct, and now spec'd (`cov.327`–`cov.330`; `cov.327`
closes all nine arms in one program).

### FINDING — DD#25's "Known gap" paragraph was stale in three places

It claimed `Pow` saturates and unary `Neg` doesn't trap. **Both false** — and
the specs that pin the fixes say so in their own comments (`25.9`: *"was:
saturated to i64::MAX silently"*; `25.11`: *"regression: -x → 0 - x"*). The
fixes landed with self-documenting specs; the DD was never updated. It also
still described the **janet** backend, removed by DD#104.

Rewritten as a measured table, one row per op, each citing its spec. *A
"follow-up" that is already done is worse than no note — someone picks it up
and finds it finished.* Same rot the native backlog review named, one level up.

### OPEN FOR DG — `Shl` is the only remainder, and may not be a gap

It doesn't overflow: x86 masks the count to 6 bits. sash matches bash
**exactly** (`1<<63`=i64::MIN, `1<<64`=1, `1<<200`=256). Trapping would be a
deliberate **divergence from bash**, not a bug fix — your call, not a
follow-up to pick up silently. `25.13` pins the current answers so adopting a
trap must be a visible decision.

`CheckedArithOp::from_arith`'s cold `_ => None` was left alone: both callers
guard on `Add|Sub|Mul` then `.expect(…)`, so it is unreachable *and already
loud*. Not every cold line wants an edit.

## ══ STATUS (2026-09-20) — an int cell can't reach int context ══

**Journal 378.** Sweep **5805** (5792 passing + 13 skipped), 0 failing,
**0 flips vs `e53a268`**. Units **51**/27/20. Predicted 5805 before running.

### Cold-line queue: `lower_disposition_as_int` — no bug, one gap, one ICE

Eighteen differential probes across int-cell shapes (arithmetic, index,
condition, div/mod, zero, negative, i64 max/min, interpolation, `[[ = ]]`,
three `+=` spellings, splat, join, `for…in`) — **all match bash**. The cold
arm was a correct-but-silent unwrap, same shape as `intern_or_declare`
(journal 375), so it is now an ICE.

**Unreachable, and re-verifiably so.** `IntCell` is a *storage* annotation
built at exactly four sites (`stmt_handlers.rs:981,991`;
`statements.rs:696,702`), all storage-position; both string-context lowerers
strip it; no site wraps an already-`IntCell`, so nesting can't occur either.
The guard lives on the **type** (`ssa::int_cell_in_int_context`), called by
both backends — one doc comment, one unit test, no `FunctionBuilder` needed.

The distinction vs. `capture_purity` (which I left alone): that one's
unreachability rests on **pass ordering**, invisible to a future refactor.
This one is a **closed enumeration**, re-checkable with a grep. That is the
bar I'll apply from here — not "have I seen it" but "is impossibility
re-verifiable by a command".

**Genuine gap found:** `a[i] += n` on an int collection had **no spec at
all** — every `ivc.*` used plain `=`. Added `ivc.18`–`ivc.22`, including the
**contrast** case where a non-int collection concatenates (`a[0]+=3` → `13`).
A spec that checks only the int path passes just as happily if the int path
silently becomes the string path.

## ══ STATUS (2026-09-20) — prefix splat, and the $'\0' answer ══

**Journal 377.** Sweep **5795** (5782 passing + 13 skipped), 0 failing,
**0 flips vs `d64cee0`**. Units 50/27/20.

### `$'\0'` — retired and landed, journal 371. My flag was stale.

I carried "still unanswered: the `$'\0'` retirement flag" in status.md for
several sessions. Wrong: I raised it at the moment I *implemented* it, and
never retired it once DD#72-applied-to-escapes settled the question. **A note
saying "awaiting DG" is a claim about DG and must be retired as carefully as
it was raised.**

What was actually missing: `sash.1` had **no `$'…'` section at all** — the
whole escape surface, incl. why a NUL is now `$'\x00'`, was governed by DD#115
and by good diagnostics but documented in no man page. Added (accepted set,
rejected set each naming its replacement, and the note that the `\x7f` cap
binds only the source-literal boundary because the runtime is byte-clean).

### OPEN FOR DG — sash-convert passes retired spellings through

`$'\0'` and DD#72's `16#ff` / `010` convert to *themselves*, so a converted
script fails to compile on constructs with exact lossless respellings
(`$'\x00'`, `0xff`, `8`). DD#57's "Mechanical — convert silently" table
already holds this shape (`` `cmd` ``→`$(cmd)`, `${var-x}`→`${var:-x}`), so
the *category* is settled; the *scope* is DG's call. Not started.

### `y@{a:-x}` now reports SB5083, like `@{a:-x}y` (DG's instruction)

It had reported `SB5011 command 'a:-x' not found` — a message about a command,
for a program with no such command. `scan_word` tries the `@`-word scan only
when the word *starts* at `@`; mid-word `@` was ordinary, and the
already-breaking `{` opened a brace GROUP.

Fix is one arm in `word_ends_here` (which already had the pattern for `+=`):
`b'@' => self.source.get(self.pos + 1) == Some(&b'{')`. This does **not** add a
second diagnostic — it makes `y@{a:-x}` lex as `y` + `@{a:-x}`, joined by the
adjacency pass into the **same Concat** the suffix form produces, so both reach
one check. "Reports the same" is structural, not two messages kept in sync.

`y@{a}` follows for free: SB5033, as bare `@{a}`. Word-leading `@{` never
reaches the new arm (`try_scan_at_word` claims it first), so the empty-word
assert cannot fire. Controls spec'd and still accepted: `user@example.com`,
`a@b`, `"x@{a:-y}"`, and brace-less `y@a:-x`.

**Open asymmetry, flagged not decided:** brace-less `y@a:-x` is *currently
accepted* as literal text, so changing it would break an accepted program, and
unlike the braced form it collapses nothing (SB5050 catches the unread array).

### Found: a plan drift I introduced in d9b7598

`glob_splat.sh` had `plan=47` against 48 real `chk` tokens — I added 6 specs
and bumped plan by 5. Nothing caught it because the *sweep total* was right
(I predicted 5783 and got it) and `common.sh` only *notes* a plan mismatch.
Fixed; the plan sum now equals the sweep exactly (5795).

**Run the mechanical spec checks AFTER editing specs, not before.** The
before-run finds someone else's mistake; only the after-run finds your own.

### Next

Resume the cold-line queue against the union numbers in `coverage.db`
(regions 96.66% · functions 98.80% · lines 97.40% · branches 90.85%).

## ══ STATUS (2026-09-20) — the OOM that was a fork bomb ══

**Journal 376. The `mk cov-union` blocker was NOT environmental.** Journal 375
recorded it as an OOM on a busy shared host. Wrong — and every fact supporting
that story was true: signal 9, 7-11 GB free, the heaviest step in the script,
and no improvement from `-j4`/`-j1`.

**The fact that did not fit:** retrying with **115 GB free** produced the same
instant SIGKILL. An OOM kill that lands as readily with 115 GB free as with
400 MB is not an OOM kill. `ps` aggregated by command showed **259,142
`cargo-llvm-cov` processes** and a load average of **13,167**. The "other
tenant" eating the machine was me, once per attempt.

**Cause.** `contrib/coverage.sh` evals `llvm-cov show-env` (which exports
`RUSTC_WRAPPER=cargo-llvm-cov`) and then ran the `llvm-cov` *subcommand*
inside that environment, so it re-entered itself once per rustc invocation.
cargo-llvm-cov printed the warning naming this on every run; I read it as
boilerplate because the `error:` two lines below said SIGKILL and matched the
story I already had. **I was reading the log for confirmation, not for
information.**

**Fix:** a plain `cargo test --release --lib` inside the show-env environment,
as the warning instructs. `LLVM_PROFILE_FILE` is already exported into
`target/`. `--ignore-run-fail` dropped deliberately — a failing unit test
should fail the coverage run, not be measured and discarded. Only `--with-unit`
was ever affected (`mk cov`/`mk cov-branch` use `cargo build`; `report` is
exempt), which is why DG's union definition is what finally exposed it.

**What made it findable:** the silent-failure fix from 375. The warning I
eventually acted on was text that fix had newly put on screen. Second time in
three journals that a no-evidence failure path was the last thing standing
between us and a bug.

### UNION BASELINE — measured, use this from now on

`mk cov-union`. Sweep 5770 passing + 13 skipped = **5783**, 0 failing.

| metric    | spec-only | union  | cold |
|-----------|-----------|--------|-----:|
| regions   | 96.42%    | 96.66% |  999 |
| functions | 98.45%    | 98.80% | **26** |
| lines     | 97.19%    | 97.40% | **698** |
| branches  | 90.70%    | 90.85% | **349** |

Modest, and that is the finding: the union is the correct denominator for the
100% target but does not move the number much. The 698 cold lines are real
work. `coverage.db` now holds union data. All six ICE-guarded functions read
fully covered — the mechanism works end to end.

### capture_purity.rs — probed hard, NO BUG

Redirect and pipeline buried inside every nesting construct (`if`, `for`,
group, `&&`-list, function body), checking captured value and file both. All
correct. `child_bodies()` is an exhaustive match over `StmtKind` with no `_`
arm, so no kind can hide a disqualifying statement. Two cold regions left:

- non-literal-head arm is **unreachable** (SB5011 rejects variable, quoted,
  concat and single-quoted heads; probed in 6 positions) — ICE candidate.
- the `bodies.get(name)` walk is *measured* never-taken but **not proven**
  unreachable; it depends on whether every user-fn call is inlined before this
  pass. Left alone — "never observed" is not "cannot happen", and an ICE there
  would crash a legal program.

### Next

Resume the cold-line queue against the union numbers in `coverage.db`.

Still unanswered: the `$'\0'` retirement flag from journal 371.

## ══ STATUS (2026-09-20) — the guards that only a unit test can reach ══

**Journal 375.** Sweep **5783, 0 failing, 0 flips**, zero ICE panics. Units
**50** (up 5).

**DG's backstop ruling, applied.** "100% would be the union of reachability
from spec tests and unit tests. Unit tests should be able to bypass other
layers. Any code that should be unreachable should halt with an ICE if it ever
ran." Four guards converted, five `*_is_loud_ice` unit tests added:

- `codegen/values.rs::intern_or_declare` — declare-on-the-fly fallback gone.
  It produced *correct* (un-deduplicated) data, which is what made it useless:
  it papered over pre-pass/codegen drift instead of reporting it. Dropping it
  also dropped `&mut ObjectModule` from four call sites.
- `parser.rs::strip_sed_prefix` / `validate_static_sed_re` — `scan_one_sed_edit`
  checks `s/` and both `/` delimiters before the parser sees the payload.
- `expr::parse_at_brace_disposition` — caller guarantees `@{` and the lexer
  guarantees the closer. `find(':')` below stays a real SB5033 (`@{a}`).

**The sweep is the proof**: convert first, sweep second. Reachable guards
would have announced themselves loudly, which is the wanted behaviour anyway.

**Two corrections to my own earlier reporting.** (1) The cold list was
measured spec-only, so `joined_form_to_string` / `opaque_form_to_string` sat
on the queue while already having ICE tests. (2) `expr.rs:230` was NOT
unreachable — `@{a}` reaches it; it was a spec gap, now `gs.38`. Reporting a
line unreachable without probing *that line* is the same error as reading a
call site instead of running it.

**New `mk cov-union`** (`--branch --with-unit`) — no target ran that
combination, and it is what DG's definition requires.

### BLOCKED — union baseline not yet measured

`mk cov-union` is OOM-killed on this host: an instrumented release build of
the lib plus its test harness, with 113 of 125 GB held by another tenant.
`-j4` and `-j1` do not help; the peak is one rustc. **Environmental — retry
when the host has memory.**

Underneath it was a real defect, now fixed: the unit-test pass ran with
`>/dev/null 2>&1` under `set -e`, so the SIGKILL was **completely silent** —
no message, no partial report, indistinguishable from a clean no-op run. Same
shape as the script's own canary rule ("a gate that cannot fail is not a
gate"). It now prints the tail of the failure before exiting.

Spec-only numbers stand and are known to UNDERSTATE the union by at least the
seven ICE guards: regions 96.42% · functions 98.45% · lines 97.19% ·
branches 90.70%.

### Next

Re-measure `mk cov-union` when memory frees, then resume the cold-line queue
against the union numbers rather than the spec-only ones.

Still unanswered: the `$'\0'` retirement flag from journal 371.

## ══ STATUS (2026-09-19) — the splat that collapsed into one argument ══

**Journal 374.** Sweep **5783, 0 failing, 0 flips** (5765 + 9×2, predicted
exactly); units green.

**BUG FIXED (SB5083) — an array splat glued to adjacent text collapsed the
array into ONE argument.** `array a=(p q); printf "[%s]\n" @{a:-x}y` emitted
`p qy`; bash emits two arguments, `p` and `qy`. All three dispositions
(`:-`, `:?`, `:~`), and an adjacent expansion (`@{a:-x}$s`) too. The scalar
analogue `${s:-x}y` was correct, so it was specific to the splat.

**Third instance of one shape.** `validate.rs` already carried the note "Only
the assignment path used to check this, so the others accepted `@a` and then
did the wrong thing silently" — fixed for assignments, then for `[[ ]]`
operands and redirect targets. `Concat` parts were the position nobody came
back for. `$(argv)y` / `$(keys m)y` were already SB5025 with wording that
states the rule outright ("it cannot be joined into a scalar"); only the
`arrvals` family was silent. **DG chose rejection over bash parity** — sash
already diverges deliberately on splat safety (SB5032), and byte-identical
output is a promise about programs sash *accepts*. No spec depended on it.

**BUG FIXED — a hardcoded diagnostic span.** `parts_to_expr` built its SB4014
with a literal `Span { line: 1, col: 1 }`, so every "expected expression" from
that path has pointed at the first byte of the file, forever. `spec/scope.sh:291`
already asserted `$err != *:1:1:*` on an unrelated code — a spec defending
against a symptom whose cause was two functions away.

**Ten bugs from the coverage grind.** Neither of these was on the queue; the
queue supplied the *question* (what reaches `parse_at_brace_disposition`'s
error closures?) and the malformed `@{…}` inputs written to answer it were the
class nobody had tried.

### DG's ruling on unreachable backstops — SETTLED

> 100% would be the union of reachability from spec tests and unit tests. Unit
> tests should be able to bypass other layers. Any code that should be
> unreachable should halt with an ICE if it ever ran, no?

The codebase already has the pattern (`panic!("ICE: …")` + a `*_is_loud_ice`
unit test via `catch_unwind`). Two corrections fall out:

- **The cold-function list was measured wrong.** `mk cov` is spec-only, so
  `joined_form_to_string` / `opaque_form_to_string` looked cold while already
  having ICE tests. The union needs `--branch --with-unit`; no `mk` target
  runs that combination yet.
- **`expr.rs:230` was not unreachable** — `@{a}` reaches it (SB5033). Now
  spec'd as `gs.38`.

### Next

Apply the ICE ruling to `intern_or_declare` (values.rs), `parser.rs:2054`,
`parser.rs:2084`, `expr.rs:227`; add a `mk cov-union` target; re-measure the
union baseline before resuming the cold-line queue.

Still unanswered: the `$'\0'` retirement flag from journal 371.

## ══ STATUS (2026-09-19) — the flags slot that was one character wide ══

**Journal 373.** Sweep **5765, 0 failing, 0 flips** (5747 + 9×2, predicted
exactly); units green (92).

**BUG FIXED — every sed substitution flag except `g` was silently ignored.**
`build_one_sed_sub` had one line of flag handling — `if flags.contains('g')`
— and that was the whole implementation. `s/a/b/x`, `s/a/b/zzz`, `s/a/b/G`,
`s/a/b/gg` and `s/a/b/i` all compiled clean and did nothing. Real `sed` errors
on every one. Two were actively dangerous: `i` reads as a promise of
case-insensitivity that the engine cannot keep (DD#63: no `REG_ICASE`; use
`$(tolower …)` on both sides), and `contains('g')` is a substring test, so
`s/a/b/Xg` quietly became a gsub.

**Settled by a source of truth, not asked.** DD#63 — "`g` is therefore the
`sub`/`gsub` choice, not a flag argument. There are **no other flags**". DD#96
documents the identical silent-acceptance shape fixed for the tail-junk slot
and never for the flags slot beside it. SB4032 now rejects anything that is
not `""` or `g`. One fix covers both the `=~` statement form and the
`${v:s/RE/SUB/flags}` expansion form; the DD#96 chain is unaffected. `gg`
rejected too — reported, open to veto.

**Also fixed — a diagnostic that named the wrong sigil.** An unclosed `@{a:-W`
reported *"unterminated `${`"*. `scan_by_brace_depth` is shared with the DD#69
splat disposition and hardcoded the opener; it now reads the byte the source
actually used.

**How it was found.** Four cold *error closures* (`parser.rs:2054`/`:2084`,
`expr.rs:227`/`:230`) came off the coverage queue. Marked all four with unique
`eprintln!`s to learn which input reaches which — and every malformed `=~`
payload hit the *scanner* instead (`take_sed_stmt_payload` requires `s/` at
that point), making `parser.rs:2054` provably dead. The junk input used to
prove that was the one that should have failed. **The cold lines were
unreachable; the bug was on a surface coverage had always called hot.**

**Eight bugs from the coverage grind.**

### Open for DG — blocks the 100% target

**Policy on provably-unreachable defensive backstops.** Three instances now:
`intern_or_declare`'s miss branch (deliberate, `e4c7422`; measured zero hits
across 5747 specs), `parser.rs:2054`, `parser.rs:2084`/`expr.rs:227`. Leave
cold / delete / make loud / exclude from the target. Tension with the
just-ratified "make the redirect miss an ICE" — though those were silently
*wrong* and these are silently *correct*.

Also still unanswered: the `$'\0'` retirement flag from journal 371.

### Coverage baseline (after `mk cov-branch`, journal 372)

regions 96.42% · **functions 98.45%** (33 cold) · **lines 97.19%** (746 cold)
· **branches 90.70%** (354 cold).

## ══ STATUS (2026-09-19) — the redirect that went into the capture ══

**Journal 372.** Sweep **5747, 0 failing, 0 flips** (5729 + 9×2, predicted
exactly); units green (92).

**P1 FIXED — redirects inside `$( … )` were silently dropped.**
`x=$(/bin/echo hi > side.txt)` captured `hi` and never created the file; bash
captures nothing and writes the file. Every file redirect (`>`, `>>`, `<`,
`2>file`, `>/dev/null`) behaved this way. The dup forms (`2>&1`, `1>&2`)
worked — which is why it survived: the form reached for first was the one that
was fine.

**Two independent defects.**

1. **native** — `Interner::intern_value` walks children via `child_values()`,
   which yields `SsaValue`s. A redirect target is an `ir::Redirect`, so the
   walk *cannot* see it; the path literal never reached the literals map.
   `emit_open_redirect` then did `literals.get(s)?` — **silently skipping the
   whole redirect**, with a doc comment saying so. A missed compiler invariant
   became wrong output instead of a crash.
2. **C backend** — `emit_c.rs` discarded `redirects` with `..` and called
   `sash_capture_status(argv, envp, …)`, which has no redirect parameter.
   **All** capture redirects dropped, `2>&1` included. A pre-existing
   native/C divergence no spec had caught.

**The fix (DG's call).** `$(a | b > f)` already worked in *both* backends —
pipeline captures take the **body** path, where redirects are lowered by the
ordinary statement machinery. Only the single-command fast path was broken,
because it re-implemented redirects itself in one backend. So the fast path's
redirect support is gone: `single_command_subst` declines when the statement
carries a `Redirected` wrapper, and `$(cmd > f)` lowers through `CmdSubstBody`.
Three lines; **8/8 against bash in both backends** (was 2/8 native, 0/8 C).

Both costs I predicted failed to materialise, and I had assumed rather than
measured them: **no extra fork** (one `clone` with the redirect and without),
and no semantic drift — DD#55 abort, `$(- cmd >f)` suppression and taint are
identical to the unredirected form.

**Deleted as now-unreachable** (DG): `SsaValue::CmdSubst.redirects`, the
`emit_redirects` block in native `emit_cmdsub_exec`, and
`CmdSubstBody.redirects` (constructed `vec![]`, every reader destructured it
away — it had never carried anything).

**The silent skip is now an ICE**, matching `redirect_target_ice` already in
the same file. The sweep confirms nothing else relied on it.

**Specs** `csr.1`–`csr.9` in `commands.sh`: builtin and external heads,
`>`/`>>`/`<`/`2>file`/`2>&1`/`>/dev/null`, a non-literal target (the
`redirect_s` path), and the DD#55 abort.

*Five bugs into the coverage grind, the cold line has been the bug once.*
`emit_cmdsub_exec`'s cold block was live and correct — and is now deleted, not
because it was wrong, but because asking what reached it exposed an interner
that could not see redirects and a backend that ignored them. The corollary:
**a silent fallback converts a missed invariant into wrong output.**

**Harness note:** the suite separates compile-time stderr (`err`) from runtime
stderr (`rerr`); a DD#55 abort is a *runtime* error. `csr.9` failed first run
on that, and only because its assertion was inverted — asserting `$err` the
usual way would have passed vacuously.

**Next:** resume the coverage queue. Baseline before this fix: functions
98.45% (33 cold), lines 97.16% (756), branches 90.70% (354).

## ══ STATUS (2026-09-19) — the escape hatch that could not hold a byte ══

**Journal 371.** Sweep **5729, 0 failing, 0 flips** (5689 + 40, predicted
exactly); units green (92).

**P1 FIXED — `$'\xff'` emitted U+FFFD.** Every byte from 0x80 up was
silently replaced. `scan_ansi_c_bytes` accumulates a `Vec<u8>` — its comment
explains carefully why bytes matter — and `scan_ansi_c_quoted` then did
`String::from_utf8_lossy` one line later. A Rust `String` cannot hold a lone
`0xFF`; the bug was a representation boundary, not an error in the escape
logic.

This contradicted a **ratified** claim. DD#64 designated `$'…'` the universal
escape hatch — *"any byte via `\xNN`"*, *"complete literal-byte control"* —
and that was the whole argument for removing every other backslash handler.

Six more silent divergences from bash sat alongside it, including sash's
`\0NNN` counting digits differently from bash's `\nnn` (1-3 digits **total**,
so `\0101` is `\010` then `1`). That is the `16#ff`-truncating-to-`16`
failure of DD#72, found a second time.

**DD#115 — the escape set, DD#72-shaped** (DG: *"we already retired `16#ff`
in favour of more modern/sane formulations"*; *"authoring raw binary is
YAGNI"*). Keep the 12 named escapes; `\xNN` **capped at `\x7f`**; **add
`\uHHHH`/`\UHHHHHHHH`**; reject octal and `\cX` as SB4063; unknown escapes
still pass through byte-identically to bash.

What made the cap cheap: **the runtime is byte-clean** — measured. `read`,
`arg`, `getenv`, capture and external passthrough all carry `61 ff 62`
intact. The UTF-8 constraint binds *only* the source-literal boundary.

**`$'\0'` went too, and that was the cost.** Nine tests failed on it — the
octal family's most-used member, never named in the discussion. It still
goes: `\0` **is** the octal escape, so keeping it because it is popular
would contradict the rule rather than apply it, and `\0` accepted while
`\01` stayed an error is a worse rule to explain. Twelve sites migrated to
`$'\x00'`, verified to still build the interior NUL and still fire the DD#55
abort. *A popular spelling is not a different spelling.*

**Reporting:** `Scanner::bad_escape`, mirroring `unterminated`/`attached_hash`
— an escape resolves in the lexer, which has no diagnostic path. Reported by
`parse` as **SB4063**. Note for next time: `reject_escape` must **not** move
`self.pos` (the digit scanners already consumed); an extra increment ate the
closing `'` and produced a confident, wrong *SB4057 unterminated*.

**Complexity gate fired and was right.** The new block pushed `parse` to
cog 7 (bar 5.85); three `if let … return Err(vec![parse_error(…)])` fatals
had accumulated there. Extracted `scanner_fatal` (parse cog 7→5, helper 3),
which also gives the report order one home and one rationale — the order of
*consequence*, not severity.

**DD#64 amended** — its escape-hatch claim is narrowed to *any ASCII byte,
and any character via `\uHHHH`*. The surrounding argument is untouched: every
structural char it lists (`"` `$` `'` `\`) is ASCII.

**Next:** resume the coverage queue. Baseline before this work: functions
98.44% (33 cold), lines 97.02% (792), branches 90.60% (356). Refresh
`coverage.db` first — `lexer.rs`/`parser.rs` line numbers have moved three
times. Ranked targets: `emit_cmdsub_exec`, `intern_or_declare`,
`lower_disposition_as_int`, `arith_brief`, `from_name`, `try_nullary_intrinsic`,
`tok_to_text`, `build_expr_from_parts`, `parse_one_decl_assignment`,
`command_is_pure`, `joined_form_to_string`, `handle_attest_stmt`,
`check_condition_type`, `check_scalar_assignment`.

**Method note, three bugs running:** the cold line has not once been the bug.
It is a reason to ask "what input reaches this?", and the answer keeps being
somewhere else — `parse_fd_prefixed` was fully covered, and `simple_escape`
really was just a gap while its *neighbours* were broken.

## ══ STATUS (2026-09-18) — the digit that was not an argument ══

**Journal 370.** Sweep **5689, 0 failing** (5661 + 28, predicted exactly);
units green.

**P1 FIXED — an argument before a redirect was silently eaten.**
`echo hi 2 > f` wrote nothing to the file and printed `hi`; bash writes
`hi 2`. The parser read the `2` as an fd prefix and redirected stderr. bash
binds an fd prefix only when the digits **touch** the operator — `2>f` is
fd 2, `2 >f` is the argument `2` plus a stdout redirect. The parser tested
only the *kind* of the next token, never its position, so any one- or
two-digit last argument before a redirect vanished. No error, no warning,
wrong output, both backends.

Spans were available on both sides all along, and `looks_like_assignment`
forty lines up already uses exactly this test to tell `x=1` from `x =1`.
Both fd sites now share `fd_prefix_binds`.

**How coverage found it — worth noting.** The cold line was in
`parse_trailing_redirects`, a *second* copy of the fd logic. Asking what
input reaches it produced `echo hi 2 > f`; running that against bash
answered a question I had not asked. The bug was in the other copy,
`parse_fd_prefixed`, **fully covered the whole time**. Coverage found this
by forcing the question "what is this line for?", not by being low.

**Knocked loose:** `x=1 2 >f` was SB4046 (the `2` had been eaten); it is now
SB5011 `command '2' not found` — which is bash's reading of that line too.

**Also: one genuinely dead arm removed.** `parse_split_subscript_index`'s
missing-`]` check cannot fire — `looks_like_dynamic_subscript_assign` is the
only route in and already pins `peek_n(2)` to `]`. Its sibling arm
(`m[${k:-z}]=1`) is live and was merely untested. Two adjacent cold arms,
two different verdicts: **cold is a question, not an answer.**

**Specs:** `fd.1`–`fd.11` in `commands.sh` (plan 112 → 123), `80.38`–`80.40`
in `subscript.sh` (plan 37 → 40).

**Coverage baseline (this sweep):** functions 98.44% (33 cold), lines
97.02% (792), regions 96.27%, branches 90.60% (356).

**Next:** continue the coverage queue — `emit_cmdsub_exec`,
`intern_or_declare`, `lower_disposition_as_int`, `simple_escape`,
`arith_brief`, `try_nullary_intrinsic`, `tok_to_text`,
`build_expr_from_parts`, `parse_one_decl_assignment`, `command_is_pure`,
`joined_form_to_string`, `handle_attest_stmt`, `check_condition_type`,
`check_scalar_assignment`. Refresh `coverage.db` before ranking again.

---

## ══ STATUS (2026-09-18) — the only other possibility ══

**Journal 369.** Sweep **5661, 0 failing** (5645 + 16, predicted exactly);
units green.

**FIXED — a false diagnostic on attest-all over an undeclared name.**
`attest x in zz [[ … ]]` said *"'zz' is a scalar; use scalar attest"*. `zz`
was declared nowhere, so every clause after the dash was false, and the
prescribed repair fails in turn under a different rule. The check asked
`!collections.contains(name)` and named the only *other* possibility it knew
about — but absent-from-collections holds two populations, declared scalars
and names that do not exist.

`check_len_operand`, in the same file, already carries a comment diagnosing
exactly this ("an undeclared name is SB5039's to report … masked the real
diagnostic with a misleading one"). The attest checks did not inherit it. A
comment is not a mechanism.

`len` defers to SB5039 by staying silent; attest **cannot** — nothing
downstream rejects an attest-all over an undeclared name, so silence admits
rather than defers, and the program iterates a collection that was never
introduced. New `attest_unknown_name_error` reports it in SB5039's wording,
asked *before* the kind question (a different fault, not a refinement).

**Worth knowing:** the scalar form `attest zz [[ … ]]` already reported
SB5031 correctly — but not because the kind check does anything. SB5020
forces the condition to reference the attested variable, so `$zz` is read,
and the *read* is what SB5031 catches. Correct by a rule about something
else.

**Specs:** `ud.30`–`ud.37` in `undeclared.sh` (plan 29 → 37) — DD#62's typo
footgun file. Four pin the new reject, four pin that declared scalars/arrays
still get the kind advice they always got.

**Next:** resume the coverage queue — refresh `coverage.db` (`mk cov-branch`
→ `mk cov-db` → `mk cov-reset` → `mk build`), then `extract_func_args`,
`insert_case_phis`, `detect_array_iter`, `emit_wait`, `lower_if_attest`,
`strip_ssa_version`, `analyze_function_returns`,
`parameter_expansion_to_value`, the `arith.rs` cluster. Also re-check the
five cold lines in `attach_redirects_to_result` /
`attach_redirects_to_last_inst` — reachable now, still dead, or delete.

---

## ══ STATUS (2026-09-18) — two holes under one loop ══

**Journal 368.** Sweep **5645, 0 failing** (5629 + 16, predicted exactly);
units green; clippy clean.

**P1 FIXED — a failure inside `END { }` no longer loops forever.** Filed in
367 as a redirect bug; it was two holes feeding one loop, and the second was
wider. DD#55 aborts route *through* END so cleanup runs — from inside END
that is a jump to the block already executing (rc=124, 380k diagnostic
lines).

1. `mark_unchecked_in_end_blocks` selected blocks by **label**, but
   `install_end_block` lowers the trap body with `lower_stmts`, so only the
   *entry* block is named `__end__`. Any failing command in a nested block
   inside END — `END { if … { /bin/false } }`, `END { for … }` — was left
   checked and looped. **Not a redirect bug at all.** The flat case worked,
   which is why it survived.
2. The redirect-open abort keys on positional `in_condition` and never
   consults `checked`, so the marking could not reach it even when correct.

`SsaProgram::end_region()` = `reachable_from(end_block)` replaces the label
test, and the transform and **both** backends now share it — same pattern as
`condition_inst_index`. DG chose **A1**: diagnose, restore, finish the
cleanup. Verified on native and `--emit-c`.

**Near-miss worth keeping.** The open-failure path disagreed with the
*exec*-failure path in three of four positions — a persuasive table, because
the two really are the same kind of event. `errors.sh` 55.26 pins the
suppressed case as deliberate ("`-` binds to the command's write verdict,
not the prior open") and 55.25/55.50/55.63/55.64 pin the silent condition
case. Three of four rows are recorded design; only the END row was a bug. A
consistency argument is worth exactly as much as the claim that the two
things compared are the same kind of thing.

**Loose end (not a bug, worth knowing):** a *third* spelling of "inside END"
survives — the signal check-point compares block ids (`ssa_id >= end_block`).
It is correct, but by an accident of allocation order rather than by
construction.

**Specs:** `56.80`–`56.87` in `signals.sh` (plan 40 → 48). `56.86` guards
against over-reach (outside END the abort must still abort, once, through
END); `56.87` pins `exit N` in END beating a failed open. `run_sash` is
time-limited, so a regression fails as a hang, loudly.

**Next up:** the approved attest-diagnostic fix (an *undeclared* name in
`attest-all` reports the false "'zz' is a scalar" instead of "introduced
nowhere (possible typo)"), then the coverage queue.

## ══ STATUS (2026-09-18) — the redirect scope, and a loop that would not end ══

**Journal 367.** Sweep **5629, 0 failing** (5605 + 24, predicted exactly);
units green; clippy clean; complexity gate clear on every touched function.

**P1 FIXED — `f > out.txt` works.** DG chose option A (support). The body of
a user function is a *compound* body, so the redirect is a **scope**:
`RedirectPush`/`RedirectPop`, modelled directly on the `CaptureStart`/
`CaptureEnd` pair that already brackets an inlined body for `$(f)`, with
per-site slots because scopes nest. Verified by 22 bash differentials across
native **and** `--emit-c`: every operator, args, multi-command bodies,
`return`, externals, pipelines, dynamic targets, loops, `while` conditions,
`||`, nesting three deep, and inside `END`.

The decisive measurement: `fn f { echo a; echo b }; f > out.txt` must write
**both** lines. Per-command attachment would truncate the first away — that
one differential kills every shortcut.

**Why support and not reject:** SB4021 reads *"a redirect on a compound
command isn't allowed — wrap the compound in a function"*. The compiler was
directing users to the broken path. Rejecting would have closed the loop.

`--emit-c` localised the one implementation fault in a single command: the C
output was exactly right, so the fault was native-only — the literal
`Interner` never scanned `RedirectPush`'s targets, and `emit_open_redirect`
**silently skips a redirect whose literal is missing from the map**.

**NEW P1 — redirect-open failure inside `END { }` never terminates.**
`END { echo hi > /nonexistent/x; echo tail }` → rc=124, 380k repeated error
lines. The abort routes through the END cleanup block, which from inside END
is a jump to itself. `mark_unchecked_in_end_blocks` exists to prevent this
but clears `checked`, while the abort keys on positional `in_condition`.
Redirect-specific: command failure, exec failure and divide-by-zero in END
all behave. DG chose **A1** — diagnose, restore, continue the cleanup.
Filed: `notes/tasks/bug-end-block-redirect-open-loops.md`. **Next up.**

**A near-miss worth keeping:** the exec-failure path disagreed with the
open-failure path in three of four positions, which looked like one
inconsistency with one fix. `errors.sh` 55.26 pins the suppressed case as
deliberate ("`-` binds to the command's write verdict, not the prior open")
and 55.25/55.50/55.63/55.64 pin the silent condition case. Three of the four
are recorded design. A consistency argument is only as good as the claim
that the two things compared are the same kind of thing.

**Specs:** `fr.1`–`fr.12` in `functions.sh` (dual-mode, plan 102 → 114).
`fr.2` truncate-once, `fr.8` nesting, `fr.10`–`fr.12` mirror 55.24/25/50.

**Still open:** the approved attest-diagnostic fix (an *undeclared* name in
`attest-all` reports the false "'zz' is a scalar" instead of "introduced
nowhere (possible typo)"), then the coverage queue.

## ══ STATUS (2026-09-18) — the dead code was the symptom ══

**Journal 366.** Sweep **5605, 0 failing** (predicted exactly); units green;
clippy clean.

**P1 BUG FOUND — a redirect on a user-function call is silently dropped.**
`fn f { echo hi; }` + `f > out.txt` compiles, prints `hi` to **stdout**, and
never creates the file. Every operator (`>`, `>>`, `<`, `2>`, with args) is
accepted and none takes effect. Builtins, externals, bare pipelines, and a
redirect *inside* the body are all correct — it is specifically the call.

Cause confirmed from `--emit-c` (no `sash_redirect` emitted at all):
`lower_redirected_stmt` attaches redirects to the last inst of `block` and
`result_block`, but an inlined function body lands in an **intermediate**
block (`_blk_2: call_f`) that neither names. The redirect attaches to nothing.

**The five cold lines were the symptom, not over-generality.**
`attach_redirects_to_result` 327/332 and `attach_redirects_to_last_inst`
344/353-355/357 are all unreachable *because* the only shape that would
exercise them is the shape that silently fails. This is the second time the
coverage grind has pointed straight at a half-wired feature (cf.
`bug-redirected-function-def-unregistered`).

**FOR DG — fork, not decided:** `notes/tasks/bug-function-call-redirect-
dropped.md`. The silent drop must go (settled precedent: `x=5 >f` →
SB4046). But: **(A)** support `f > out.txt` — bash-ordinary, unlike the
`f(){…} >file` corner you rejected; or **(B)** reject with a spanned
SB4046-style error — cheap and consistent, but removes a common form from
the subset. Recommend **A** unless you want the subset narrower.

**Stale comment, third time in four journals.** `lower_redirected_stmt` still
documents a `__redirect_push`/`__redirect_pop` bracket for function bodies —
machinery deleted with `lower_redirected_function`. It reads as though the
case were handled.

**Landed:** `attest_map_form_error`'s "requires a map" diagnostic had never
once been produced (10 calls, all returned early). Reachable and correct;
specs `93.55`–`93.58` now cover indexed-array, scalar, int, and a map
control. The masking trap struck again — if the attest condition does not
validate *both* binders, an earlier check fires instead.

**Second, smaller question for DG:** against an *undeclared* name that same
message says "'zz' is a scalar", where every other read site says "assigned
nowhere (possible typo)" (SB5031/SB5039). False statement, and it misdirects
a typo'd map name to "use scalar attest". Error-code surface, so not changed.

## ══ STATUS (2026-09-18) — three cold regions, one covered and two explained ══

**Journal 365.** Sweep **5597, 0 failing** (predicted exactly: +3 specs in a
dual-mode file = +6); units green; clippy clean; no id collisions.

**Covered:** `looks_like_dynamic_subscript_assign` (parse/parser.rs) — both
cold early-outs reached by ordinary input, specs `80.35`–`80.37`. `a[ x ]=5`
(token after `[` is a word, not a `$` — the lexer only splits `name[` off when
the subscript holds an expansion) and `a[$i` truncated (the token where `]`
belongs is end-of-input). Both reject cleanly with SB5011; no hang, no ICE.

**Explained, not changed:** `command_is_pure` (transform/capture_purity.rs).
The `visiting` cycle guard cannot fire because SB5037 rejects recursion —
verified direct, mutual, and a 3-function cycle — and the module header
already says the set exists so a malformed program cannot hang the compiler.
The `bodies.get(name)` lookup **succeeds zero times in 156 calls**, which
looked serious: the doc insists a body that merely wraps an external must be
disqualified. It is — by inlining, which runs first and splices `/bin/cat`
into the capture so the slash test catches it. Confirmed by output against
bash across six shapes (wrap, consume+echo, two levels, echo-then-external,
redirected away, captured twice) — all SAME. Measurement recorded in the
source.

**Recurring trap, twice now.** One of the six first read as a divergence
(SB5020 on consume+echo) but that was the taint rule on an unattested `$t` —
a different pass. Same shape as journal 364, where validate's SB5020 masked
the backstop I was actually testing. *A result that looks like it concerns
the pass you are studying may come from an earlier one.*

**Pattern across 364/365:** cold branches that turn out to be bugs are the
ones where the code contradicts its own doc comment; the benign ones are
already explained by theirs. Read the comment first as a filter — then check
it against execution rather than believing it.

## ══ STATUS (2026-09-18) — a name collision that laundered an execve ══

**Journal 364.** Sweep **5591, 0 failing** (predicted exactly: +4 specs in a
dual-mode file = +8); units green; clippy clean.

**Taint bug, found and fixed.** `cmd_subst_taint` (`ssa_taint.rs`, the DD#82
backstop) classified a command-substitution head with
`contains(cmd) || contains(basename)`. The basename fallback matched
`/bin/cat` against a user function named `cat` and returned **Clean for a
real execve** — flatly contrary to the function's own doc ("everything else
runs via execve/execveat — tainted").

**Settled by instrumentation, not by reading.** Printing both membership
tests over a spread of programs showed (a) the basename test is true *only*
in the collision case, so it has no correct use — DD#23 gives externals an
absolute path and user functions keep bare names; and (b) the bare-name test
is **never true at all**: five shapes of user-function capture (args,
pipeline, redirect, one running an external, recursive) never reach the arm.
Never right, sometimes wrong. Kept the bare-name test, dropped the fallback.

**Severity, stated plainly: not user-visible today.** `validate`'s AST taint
pass rejects the unattested collision program with SB5020 first — which is
why the first probe looked clean, and the trap worth remembering: *seeing the
expected rejection is not evidence that the pass you are testing produced
it.* What was broken is the backstop's ability to catch a `validate` bug on
that input class.

Specs `dd82.1`–`dd82.4` in `taint.sh` pin the input class (they pass either
way — the defect has no user-visible signature while validate shadows it).

**Fresh coverage baseline** (`mk cov-branch`, this session): functions
**98.42%** (33 uncovered), lines **96.20%** (1114), regions 96.95%, branches
**90.40%** (360). The old ranking was stale — it had sent me to `emit_wait` as
having no cold branches when it has three. Re-ranked; `coverage.db` rebuilt.

Of the 33 uncovered functions, most are cold closures/monomorphizations; the
named ones are three ICE guards plus `opaque_form_to_string`, which a prior
session already measured and pinned in unit tests that `mk cov` excludes by
design. Not targets.

## ══ STATUS (2026-09-18) — a function that undid its only caller ══

**Journal 363.** Sweep **5583, 0 failing**; units green; clippy clean. Net
change is a deletion: one codegen helper removed, one wrong comment corrected,
one fork filed for DG.

The branch queue surfaced twin helpers `base_var_name` (inst_handlers.rs) and
`parse_var_base_version` (redirects.rs) — same shape, three cold branches
each. The collision hypothesis (a variable literally named `x_2` versus `x` at
version 2) was **disproved by execution**, 3/3 bash differentials SAME:
`rfind` takes the *last* underscore, always the one the key format appended.

**The finding was the call site, not the function.** Every map key is
`VarRef::display()` = `format!("{}_{}", name, version)` over a `u32`, so
`base_var_name(&target.display())` built `name_N` only to parse `_N` back off
and recover `target.name` — a value the caller already held. Now `&target.name`;
the helper is deleted. Provable for all names and versions, and the sweep is
byte-identical.

**Both doc comments were wrong** — they claimed version 0 is spelled bare
`name`. Nothing has ever emitted that. The false invariant is precisely what
made the collision look plausible and the fallback look load-bearing. A stale
comment does not merely fail to help; it manufactures work. The surviving
comment now states the real key format and why `x_2` and `x` stay distinct.

**For DG — `notes/tasks/codegen-var-key-pair-encoding.md`.**
`parse_var_base_version` survives (its caller iterates keys, where the pair
really is flattened) with three permanently-cold branches. They are *not* ICE
guards: an unparseable key yields a silently-wrong `(key, 0)`. Options: (a)
leave and accept; (b) key the map on `(String, u32)` so the parse ceases to
exist — behaviour-preserving but touches five codegen files; (c) delete the
fallback and panic, reaching 100% by inventing a new failure mode.
**Recommend (b) when codegen's var map is next opened, (a) until then.**

**Also still open from earlier blocks:** DD#71's table lists `[[ ]]` comparison
operands "(both sides)" as operand words, but only the `=~` RHS and dispatch
patterns use `AbsorbMode::Operand` — doc-vs-implementation, behaviour sound.

## ══ STATUS (2026-09-18) — the registry stops being permanent ══

**Journal 362.** Sweep **5583, 0 failing** — and the number is now the gate's
own, matching `sum(plan)` exactly for the first time. **Units 77, clippy
clean.**

**DG chose option (c)** from the previous block: `run.sh` now retires registry
ids the sweep no longer produces. Three properties, and two of them are guards
against a purge that would silently empty the registry:

- **Mode-scoped** — only ids whose mode prefix appears in this sweep. Without
  it, the documented `SPEC_MODES="native"` fast path reads as a retirement of
  all 2697 `c:` tests.
- **Capped** at `SPEC_PURGE_MAX` (default 25) — a partial or aborted sweep is
  indistinguishable from a mass retirement by inspecting the TSV, so past the
  cap it reports the cohort and deletes nothing.
- **`runs` rows kept** — they are the cross-commit history; dropping the
  `tests` row alone clears the id from `test_status`.

Dry-ran all three against a copy before any DELETE touched the real database:
full sweep retires exactly the 2 ghosts; native-only retires 1 and leaves 2697
`c:` rows; partial sweep wants 2788 and deletes 0. The purge announces what it
retired — **an unexpected retirement is a finding**, not tidy bookkeeping.

**Self-inflicted bug, caught and fixed.** The explanatory comment I wrote
inside the SQL heredoc used the project's backtick prose style. That heredoc
is *unquoted*, so bash ran `tests`, `runs` and `75.99` as commands and spliced
the empty results into the SQL ("tests: command not found", ×4). Harmless here
— it hit comment text — but everything in that heredoc is rewritten before
sqlite3 sees it. Backticks are now banned in that file with the reason stated
in place. The general lesson: a prose convention that is right everywhere else
in the repo is a defect in this one context.

**Still open (carried):** DD#71's table lists `[[ ]]` comparison operands
"(both sides)" as operand words, but only the `=~` RHS and dispatch patterns
use `AbsorbMode::Operand` — doc-vs-implementation, language surface, DG's
call. And the `check_subscript_key_kind` purge moved a *precondition* to its
callers (reported, not asked); veto is one line.

**Next:** back to the branch/line queue — `emit_wait`, `base_var_name`,
`parse_var_base_version`, `cmd_subst_taint`, `command_is_pure`,
`detect_array_iter`, `insert_case_phis`, `lower_if_attest`. Baseline before
this stretch: lines 96.93%, functions 98.42%, branches 90.07%.

## ══ STATUS (2026-09-18) — the gate had been counting ghosts ══

**Journal 361.** Sweep **5583 executed, 0 failing** (16 specs: 10 `ow.14`–
`ow.23`, 4 `80.31`–`80.34`, 2 `cov.325`/`cov.326`, all dual-mode — predicted
exactly, see below). **Units 77, clippy clean.** One line of code deleted.

**1. The sweep summary over-reports, and `run.sh` is the source.** I already
documented that `test_status` is `tests LEFT JOIN runs` over a permanent
registry, so retired specs report their last status forever. What I missed is
that **`run.sh` prints its summary from that view.** The sweep said 5585;
5583 checks actually ran. The two ghosts are `c:75.99` / `native:75.99` —
the duplicate deleted last commit, last genuinely run at `5cfeb24`.

Corrected ground truth (now in AGENTS.md): **sum of every file's `plan=`**,
valid only because `plan` is separately verified against each file's real
`chk` count. Both recipes documented, plus the standing rule that each
retired spec inflates the summary by one per mode forever.

**→ OPEN FOR DG.** The gate's own number drifts further with every retirement.
Three options, none taken: **(a)** leave it and subtract by hand (zero risk,
the drift grows); **(b)** have `run.sh` summarise only ids present in the
current sweep's TSV (accurate, small change, loses the "what did we retire"
signal); **(c)** purge retired ids from `tests` when a sweep no longer sees
them (accurate and self-maintaining, but destructive to a registry that is
permanent by design). I lean (b). Not touched.

**2. Cold-branch work in `parse/expr.rs` — three reachable-untested paths.**
- `absorb_text`: the `&&`/`||`/`&` guards are only reachable *adjacent*
  (`a=foo&&bar`), because `parse_word_parts` breaks on non-adjacency. DD#71
  lines 74-78 already decided these are literal values — a deliberate bash
  divergence that had no specs at all. `ow.14`–`ow.23`.
- `dynamic_subscript_var`: the non-identifier-index path. `${arr[$1]}` /
  `${arr[$#]}` match bash; the non-brace `$arr[foo]` declines reject SB5064
  (decision B, journal 115 — checked, not a bug). `80.31`–`80.34`.
- `try_at_transform`: `${arr[@U}` leaves `[@U` as the remainder, so the `[@`
  guard is the only thing stopping an `@U` transform reading off an
  unterminated subscript. `cov.326`. Plus `${v@1}` → SB4047, `cov.325`.

**3. Deleted one provably inert guard.** `try_at_transform` tested
`!rest.contains("[@]") && !rest.contains("[@")`. `"[@]"` contains `"[@"`, so
the first test could never decide an outcome the second did not. Cold not
because it was hard to reach but because it was **incapable of mattering** —
a different category from the ICE guards we keep, which are reachable and
load-bearing. Removed; the reasoning is in the doc comment.

**4. Standalone spec runs test the wrong binary.** `common.sh` uses
`${SASH:-sash}`; only `run.sh` prepends `target/release`. Running a file
directly exercised `~/.local/bin/sash`, five days stale, and produced 49
failures unrelated to the tree. Same shape as the coverage-harness faults:
it measures the wrong artifact and still prints a clean result. Documented.

**Method note.** The first `absorb_text` probe round was invalid — I wrote
bare `[[ … ]]`, which is SB4018 ("result unused"), and briefly believed I had
found a `&&` rejection. I had measured my own syntax error. Second occurrence
this stretch: **copy the spec syntax, do not invent it.**

**Still open from before:** DD#71's table lists `[[ ]]` comparison operands
"(both sides)" as operand words, but only the `=~` RHS and dispatch patterns
use `AbsorbMode::Operand`. Observable behaviour is sound — bash also rejects
`a|b` / `a(b` / `a)b` in an `==` RHS; sash additionally rejects `a{b` / `a}b`,
a subset restriction. Doc-vs-implementation gap, language surface, DG's call.
Also still open: the `check_subscript_key_kind` purge moved a *precondition*
to its callers (reported, not asked) — veto is one line.

**Coverage baseline before this batch:** lines 96.93%, functions 98.42%,
branches 90.07%. Queue continues: `emit_wait`, `base_var_name`,
`parse_var_base_version`, `cmd_subst_taint`, `command_is_pure`,
`detect_array_iter`, `insert_case_phis`, `lower_if_attest`.

## ══ STATUS (2026-09-18) — a minus sign decided when the error arrived ══

**Journal 360.** Sweep **5494 → 5553, 0 failing** (16 specs `cov.309`–`cov.324`
+ 7 `14.47`–`14.53` + 6 `75.94`–`75.100`, less one duplicate, all dual-mode —
predicted exactly). **Units 77, clippy clean.**

**1. BUG — SB5077 could not prove a unary divisor.** `const_eval_arith` had
arms for `Literal`/`Paren`/`BinOp` and `_ => None` for the rest, so
`ArithExpr::UnaryOp` was unevaluable. Measured over 18 spellings of zero:
every **binary** form was caught (`2 - 2`, `2 * 0`, `0 << 2`, …), every
**unary** form escaped (`1 + -1`, `-1 + 1`, `-0`). One fault reported at
compile time or at runtime, decided by a minus sign — the exact
spelling-dependent inconsistency `cov.158`'s comment says SB5077 exists to
remove, reintroduced one level down. DD#25 already stated the reach
("literals and operators over literals, through parens"); the code did not
match it. Addendum written.

**Why it drifted:** `const_eval_arith`'s doc says it shares `fold_arith_op`
rather than keeping "a second copy of the semantics." That held for binary
operators only — `try_fold_unop` worked on `SsaValue` strings, this on `i64`,
so no sharing was possible and none was written. Fix extracts
`fold_unary_op` (i64 core) and has **both** call it. `Neg` uses `checked_neg`,
so `-i64::MIN` defers to the runtime trap per DD#25. `cov.313` pins the
converse (`5 / -1` still folds) so the wider reach cannot over-reject.

**2. 11 operators had no constant-folded spec.** `fold_arith_op`'s cold arms
were whole families: `Ne Lt Le Gt Ge`, `Shl Shr BitAnd BitOr BitXor`, `Pow`.
A 19-op × 6-operand-pair differential (folded vs runtime vs bash, 228
comparisons) found **all agree with bash**; only div/mod-by-zero diverge, per
DD#25. Not a bug — `cov.314`–`cov.324`. Also `$(echo …)` captured as a *value*
with >1 argument was unspecced (`14.47`–`14.53`).

**3. The collision check has a blind spot — and it bit.** `coverage.sh` ends
with a loop assigning `cov.306`–`cov.308` via `case` and calling `chk $_id`.
The documented check greps `chk <literal>`, so those ids looked free; my new
block started at `cov.306` and the loop, running last, **silently overwrote
three specs written to pin the bug fix**. Both sets passed. Found only by a
`ran=323` vs 321-greppable-token discrepancy that had no innocent
explanation. Renumbered to `cov.309`–`cov.324`; second check added to
AGENTS.md; suite audited — one such site, three ids.

**4. `test_status` counts retired tests forever.** It is `tests LEFT JOIN
runs`, and `tests` is a permanent registry — the deleted `75.99` still
reported passing. The prediction discipline matched for weeks only because
every prior sweep purely *added* specs. Authoritative total is `run.sh`'s
output; ground truth is the weighted `chk` count (agrees exactly: 5553).
Also fixed `coverage.sh`'s stale `plan=` (308 → 323, off since before this
work).

`fold_arith_op` **27/9 → 25/3**; `const_eval_arith` 9/1; both under the
11.76/5.92 bars. `75.99` dropped — exact duplicate of the older `cov.141`.

## ══ STATUS (2026-09-17) — a feature with no specs, and state with no readers ══

**Journal 359.** Sweep **5480 → 5494, 0 failing** (7 specs `75.94`–`75.100`,
dual-mode, +14 — predicted). **Units 92.** Coverage regenerated first.

**1. `len args` works and had zero specs.** The `|| name == "args"` arm of
`check_len_operand` never fired — not dead, *untested*. `args` is the
function-local argv collection (DD#75 §5); `fn f { echo "$(len args)" }; f a b c`
prints **3**. Probed: agrees with `$#`, counts empty-string arguments, 0 for no
arguments, per-frame across nesting, `keys args` correctly SB4030. **No bug.**

The near-miss: `join args` *is* covered (cf.f11) and 22 spec files mention
`args`, so the area looks covered from any distance. The only `len args` hits
are two **convert** specs asserting the converter does **not** emit it. A
negative assertion about a spelling reads just like coverage of it.

**2. `loop_vars` was dead state — purged entire.** One writer, one reader, and
the reader could no longer decide: `check_for_binding_type` (mine, 3 commits
ago) puts every loop variable into `vars`, which short-circuits first. Second
time in two journals a branch went cold because we **fixed what reached it**.

Its doc comment gave a deliberate rationale (*"tracked separately rather than
declared, so that adding them cannot perturb… a name the pass deliberately
leaves untyped"*) — superseded by **DG's own ruling this session** (*"a for
variable survives the loop, making the for an implicit declaration"*).
Provenance checked before acting, per the journal-356 correction.
`infer_types` builds a **fresh** `TypeEnv`, so the inference pass still leaves
loop variables untyped in the map codegen reads — unchanged.

`check_len_operand` **8/4 → 5/3** across the two journals.

**Coverage regenerated:** lines **96.85%**, functions **98.28%**, branches
**89.91%** (375 cold). `cov-reset` + rebuild done.

**The function metric is at its practical ceiling.** Of 56 "uncovered
functions": ~22 are cold **monomorphizations** of live generics (`direct_exprs`
has 38 instantiations, 434k hits, 1 cold), ~32 are cold **closures** inside
live functions, and only 4 are truly never called — three of those ICE helpers
cold **by design**, the fourth covered by a unit test `mk cov` excludes.
A literal 100% needs `mk cov-unit` (off by design) or deleting ICE guards.
**Branches (89.91%) are where the remaining signal is.**

**Next:** `fold_arith_op` (`expr_handlers.rs:758`, 4 cold, **cc 27/cog 9** —
budget a refactor or the gate rejects), `join_intrinsic` (4 cold, 5/4),
`absorb_text`, `dynamic_subscript_var`, `insert_case_phis`, `detect_array_iter`.

## ══ STATUS (2026-09-17) — the guards validate already keeps ══

**Journal 358.** Sweep **5480, 0 failing** (predicted; no spec changes).
**Units 92.** Coverage queue — **five dead guards purged, no bugs found.**

Resumed at `check_len_operand`'s cold branches. The blocked probes from last
session were **malformed, not unreachable**: the DD#100 call-site sigil is
`=name`, not `@name` (`@name` is the splat). Copying the form from
`spec/collparam.sh` fixed every probe at once.

`env.is_slot(&name)` there guarded against a slot letter arriving as a loop
variable — a route **SB5080 closed three commits ago**. The branch went cold
because we fixed what reached it, not because it was always dead.

Checked the whole `is_slot` family rather than the one function. Two of four
sites live; the other two, plus three neighbouring disjuncts, all **blocked by
a validate rule that runs before type check** — each proven by execution:

| guard | rejected first by |
|---|---|
| `len`/`keys` no args | SB5073, SB4045 |
| `keys` non-name operand | SB5062 (all six forms tried) |
| `keys`/`len` slot operand | SB5066 / SB5080 / SB5065 |
| subscript on a non-collection | SB5068, read **and** write paths |

**Branch counts, not line counts, found this.** Line 999 shows a count of 26
and looks warm while holding two disjuncts that never fire.

Purged from `type_check.rs`: `check_len_operand` → `is_slot` +
`name.is_empty()` (the latter redundant *twice over* — `""` is never a key in
`vars`/`loop_vars`); `check_keys_operand` → the same two (here
`name.is_empty()` was **not** redundant — it suppressed an empty-name
message); `check_subscript_key_kind` → `!collections.contains(name)`.

Complexity fell on all three: **8/4→6/4**, **6/4→4/4**, **8/4→6/4**. Every
probe's diagnostic is byte-identical after the purge.

**No ICE guards** — the standing exception is for a regression that would
produce a *silent wrong answer*; all five would produce a misleading error
message instead.

**Reported, not asked:** the `check_subscript_key_kind` removal moves a
**precondition** to the caller rather than dropping a redundant disjunct.
Judged reversible and low-consequence; one line to restore.

**Irreducible:** the `let Some(arg) = c.args.first() else` arms in both
functions stay cold. An `Option` destructure has two arms however it is
written; neither purge nor ICE removes one.

**Next:** coverage data is **stale** (SB5080/5081/5082, the for-binding check
and these purges all post-date it). `mk cov-branch` → `mk cov-db` before
trusting the cold queue again, then `mk cov-reset && mk build`.

## ══ STATUS (2026-09-17) — a variable should never change type ══

**Journal 357.** Sweep **5476 → 5480, 0 failing** (20 specs `86.39`–`86.58`,
dual-mode). **Units 92.** **Two bugs, one principle from DG.**

> DG: *"I don't think a variable should ever change type."*
> DG: *"the following should also create an int by assignment: `((x=7))`"*

A name's type is fixed at **first introduction**, declaration or binding. The
three binding forms — bare assignment, arithmetic assignment, `for` header —
all fix it. `x=5` introduces a **string**; `(( x = 7 ))` introduces an **int**;
`for x in a b` a string; `for (( i = 0; … ))` an int.

**1. The `for` binding was never type-checked.** `int x=5; for x in a b` was
accepted and bound **0** each iteration, while `x=hello` was already SB4030.
`check_for` checked the header words for bad subscripts and never the binding.
Now SB4030, via the same comparison assignment uses. (`const`/SB4044 and
collections/SB5064 already gated it correctly — `int` was the only hole.)

**2. A declaration reached backwards in time — SB5082.** Declarations are
collected program-wide, so `x=hello; echo "$x"; int x=7` printed **`0`**, a
value never written. Hidden because `x=5` prints `5 7` and looks right.
`(( x = 7 )); echo "$x"; str x="a"` is the mirror, printing `7 0`. Needed
`ir::walk_arith_var_writes` so `(( x = 7 ))` counts as an introduction.

**Still legal, per DG:** declare-then-loop writes the existing variable; two
loops share one variable; instantiation-by-assignment is untouched. None
changes a type.

**Cost:** a word-list loop variable is a string, so `for x in 10 20 30 {
(( sum += x )) }` is SB4030. Measured at **two** sash sites (`cf.f5` plus one
spec written the same day; the other 18 hits are in `suites/`, i.e. bash's own
corpus, oils and shellcheck — not sash code). Both exits pre-existed:
`${x:-0}` (the disposition SB4030 names) and `for (( i = 10; i < 40; i += 10 ))`.
`cf.f5` was rewritten to the latter on DG's call.

⚠ **Two AI errors corrected by DG this session**, both worth reading in
journal 357: (a) the ICE-guard rationale in journal 356 was asserted from
reading, not execution — the fallback renders `${aarrvals}`, not `"@a"`;
(b) DD#86's `for`-variable "exemption" was an **implementation note** from
commit `06912b0`, not a decision, and it was cited to reverse an explicit DG
instruction. Retracted; provenance-check before citing a source of truth.

**Next:** coverage queue — re-probe `check_len_operand:1047/1055/1058`, then
`fold_arith_op` (**cc 27/cog 9, budget a refactor**), `emit_wait`,
`base_var_name`, `parse_var_base_version`, `absorb_text`,
`dynamic_subscript_var`.

## ══ STATUS (2026-09-17) — two arms a decision orphaned ══

**Journal 356.** Sweep **5440, 0 failing** (unchanged, as predicted — the
change is unreachable-code-only). **Units 89 → 91.**

Chasing `join_intrinsic`'s four cold branches found two of them were **dead
code orphaned by DD#100 Part D**, which removed the splat operand: `join @a ,`
is SB5062 (specs `75.69`/`75.70`), so the `ParameterExpansion(arrvals)` arm is
unreachable from any accepted program. **Checked the family in the same pass**
— `try_len_intrinsic` had the identical dead arm from the identical cause.
`keys` has no separate intrinsic, so the family is exactly two.

**Converted to ICE guards rather than deleted** — the dead-code exception DG
granted — because the fallback each arm shadows degrades **silently**:
`expr_to_string` renders `@a` as `"${aarrvals}"`, the collection lookup
misses, and `len` emits `strlen` of an undefined var (0 where the element
count belongs). Three unit tests: two assert the panic names SB5062, one pins
the silent-degradation shape so the guards can be reconsidered if it ever
fails loudly.

⚠ **Two corrections, both from DG, both to claims I asserted from reading
instead of running** — see journal 356's postscript:
1. The fallback does **not** render the literal text `"@a"`, as the first
   version of this block, the commit message, the DD#100 addendum and the
   code comments all said. Corrected by execution; conclusion unchanged.
2. DD#86 does **not** sanction a `for`-variable exemption from no-shadowing.
   That sentence entered in implementation commit `06912b0` on the mechanical
   grounds that a for variable is its own `StmtKind` — and I cited it last
   commit to **reverse an explicit instruction from DG**. Retracted in DD#86.
   DG's model: the loop variable survives the loop, so `for` is an implicit
   declaration. **Open — `notes/tasks/dd86-for-is-an-implicit-declaration.md`.**

The `arrvals` arms in `control_flow.rs:574` (for-in splat) and
`expressions.rs:103` (splat as command arg) are **live** and untouched.

**Next:** coverage queue — lines 96.85%, functions 98.27%, branches 89.81%.
Re-probe `check_len_operand:1047/1055/1058` (1058 may be a clean ICE-guard
candidate now that SB5080 rejects slot-named loop vars), then `fold_arith_op`
(4 cold, **cc 27/cog 9 — budget a refactor**), `emit_wait`, `base_var_name`,
`parse_var_base_version`, `absorb_text`, `dynamic_subscript_var`.

## ══ STATUS (2026-09-17) — the loop that ran once ══

**Journal 355.** Sweep **5404 → 5440, 0 failing** (18 specs `cf.n1`–`cf.n18`,
dual-mode; predicted +36, got +36). **Units 89.** **Three bugs, all in loop
lowering, all fixed.**

All three came from one cold branch: `type_check.rs::check_len_operand:1058`
(`env.is_slot`), reachable only via a loop variable, since `env.loop_vars` is
populated only by `check_for`. Writing the program to see whether that was
constructible produced bug 1; writing bug 1's *test case* produced bugs 2 and 3.

**1. A `for` variable could take a reserved collection name (SB5080).**
`for A in @B` inside a function gave one name two live meanings — `$A` the loop
variable, `len A`/`join A`/`${A[0]}` the slot. DD#100 Part B reserves `A`–`E`
"rejected as user variables", but a for variable falls between DD#86's explicit
exemption ("a `for` variable is not a declaration") and SB5052's *write* gate.

**2. Nested `@`-splat loops ran the outer loop once.** `lower_for_array` named
its induction variable with the hardcoded string `"__ai"` — one name per
compiler, not per loop — so an inner splat loop destroyed the outer's index.
`lower_for_argv_slice` did the same with `"__fi"`/`"__argc"`. Arrays, maps and
`@args` all affected; 3-deep gave 2 lines instead of 8. `--emit-c` showed one
`__ai_ptr` pair serving the whole function. Fixed with
`format!("__ai{}", header)` — the convention `transform/functions.rs` already
uses for `__args{counter}`.

**3. `break` and `continue` were silent no-ops in every splat loop.** Not a
nesting artifact — inert in a single unnested loop too. Neither splat lowering
pushed `state.loop_stack`, and `handle_break_stmt` guards on `depth > 0` and
otherwise *discards the statement*. Fixed with a **latch block** holding the
index increment: `continue` targets the latch (targeting the header would skip
the increment and spin forever), `break` targets the exit, and the latch is now
the back-edge predecessor for phi insertion. `break`/`continue` outside any
loop hit the same path — DG's call: reject, now **SB5081**.

**The audit reversed an instruction.** DG chose the general form for bug 1
(extend no-shadowing to loop variables vs any declared name) pending an audit
of every construct that introduces a name without a declaration. DD#86 says in
terms: *"A `for` variable is not a declaration and is untouched."* A loop
variable reusing a global's, a local's, or an outer loop variable's name is one
name with **one** meaning, byte-identical to bash. The general rule would have
rejected working programs and contradicted an accepted DD. Shipped the narrow
rule, which DD#100 had already decided. Addenda recorded in both DDs.

**Why a 5404-test suite missed all three:** `grep '^\s*break\b' spec/*.sh`
matched **nothing**. Not one spec executed a `break`; nested splat loops were
equally unrepresented.

**Next:** resume the coverage push (branch 89.81%, a flat long tail — top
entries `fold_arith_op` 4, `join_intrinsic` 4, `emit_wait` 3). `join_intrinsic`
is unexamined and sits in the family that produced four bugs; probe it next.
`detect_array_iter`'s 3 cold branches are now partly explained. Coverage data
needs regenerating — `mk cov-branch` then `mk cov-db` (in that order; the lcov
is unrecoverable after `cov-reset`).

## ══ STATUS (2026-09-17) — three of the five were never cold ══

**Journal 354.** Sweep **5404, 0 failing** (unchanged — nothing here touches
behaviour). **Units 83 → 89.**

DG, on the arms no accepted program can reach: *"use a unit test simulating a
regression to produce an ICE."* That dissolves the question I had framed as
delete-vs-exempt. Both of those accept the lines are uncoverable. They are not
— just not from a *program*: the compiler is a library, and a unit test can
hand it the IR that validation makes unconstructible from source. That is
exactly the regression the guard exists to catch, so the test is the guard's
own assertion rather than a coverage dodge. `codegen/redirects.rs` already did
this; I read the file this week and did not connect it.

Applied to `transform/expressions.rs` (3 tests — all 13 `ExprKind` variants
rendered, plus both helpers' `unreachable!`) and `emit_c.rs` (3 tests, one per
`ice_unreachable_command` arm, asserting the panic *text* so a regression that
lands in the wrong arm fails rather than passes). Verified by measurement:
**every line of `expressions.rs` 475–530 is now covered** and
`opaque_form_to_string` went from fully cold to 4 hits.

**The unexpected finding.** Of the five functions the data called fully cold,
**three were never cold**: `panic_payload_message` already had three unit tests
covering all three branches, `redirect_target_ice` was already covered via its
caller's test, and `type_check.rs::fmt` was **dead code** (nothing formats a
`TypeError` — `type_check_or_exit` writes the fields by hand; Rust does not
warn on an unused trait impl). They read as cold because `mk cov` runs without
`--with-unit`, which is off by default precisely so should-never-run
assertions stay visible. Sound rationale, and it produced this: the queue I was
working down was substantially a **measurement artifact**, and the session's
first pass investigated functions that were already done.

**Recommendation for DG (no default changed).** Not "switch to `--with-unit`"
— run **both**. `mk cov` (specs only) answers *what does the language
surface reach*, which is the bug-finding view that produced 351/352/353 and
whose value comes from ICE guards appearing in it. `mk cov-unit` answers *what
is covered at all*, which is what the 100% target is about. **The gap between
them is exactly the should-never-run set** — watch it as a quantity: it
should be small, every member deliberate, and a *new* entry appearing in it is
the interesting event.

**Dead code purged:** `impl Display for TypeError`.

## ══ STATUS (2026-09-17) — the list that kept the old classification ══

**Journal 353.** Sweep **5404 passing, 13 skipped, 0 failing** (+8; predicted
+8). Units 83. **Bug fixed: `strlen @a` silently joined the collection.**

DG, reading the 352 summary: *"strlen should only work on scalars, not
collections."* I had just landed a spec asserting the opposite — `75.88`
claimed `strlen @a` gives 6 — because that is what it did and I had checked
it was *coherent*. Coherent is not intended.

Every sibling rejects a splat there: `toupper @a`/`tolower @a` are SB5070,
`len @a`/`join @a` are SB5062. `strlen @a` alone returned the byte length of
the space-joined values — it quietly meant `strlen "$(join a \' \')"`.
Arity checked out because a splat is one *syntactic* argument, which is the
exact mechanism SB5070 exists to catch.

**Why it was missed.** SB5070's operand list is keyed on the name-taking
family, and **DD#102 moved `strlen` out of that family five days earlier**
("strlen should behave like toupper"). `type_check.rs::string_operand_slots`
was updated with the decision; `validate.rs::is_scalar_operand_intrinsic` was
not. The general form now recorded in DD#102: **a decision that moves an
operator between families has to move it in every list keyed on the old
family.**

That is journal 352's failure one level up — both are an *incomplete
application of something already decided*, which is what AGENTS.md warns about
under Decision authority. Twice in two days, from the leaving-it-unapplied
side.

**A second defect surfaced with it**, pre-existing and covering all eight
SB5070 operators: it fired on anything splat-*shaped* without asking what the
subject was, so `toupper @s` asserted "`@s` is an array splat" about a string
and advised `join s SEP`, which is itself rejected. Now gated on the subject
being a real collection (declarations, bulk-fill targets, DD#100 slots,
`args`), deferring a scalar to SB5079 and an unintroduced name to SB5031 —
one correct diagnostic each. Specs `75.88`, `75.90`–`75.93`.

**What nothing was going to catch.** The behaviour was self-consistent, the
spec suite agreed with it, and I had inspected it the day before and called it
fine. It took DG reading one sentence.

**Coverage directives from DG, in flight:**
1. The provably-cold `expr_to_string` arms get **unit tests that simulate a
   regression and assert the ICE** — covering the lines *and* pinning the
   behaviour. Not exempted, not deleted. **Not yet done.**
2. Work **entire uncovered functions first** — the lowest-hanging fruit —
   and expect progress on one queue to move the others.

## ══ STATUS (2026-09-17) — the survey that named it ══

**Journal 352.** Sweep **5396 passing, 13 skipped, 0 failing** (+18; predicted
+18). Units 83. **Bug fixed: splatting a non-collection segfaulted (SB5079).**

Found the same way as 351 — probing whether a non-literal could reach
`expr_to_string`'s cold arms. It cannot, but `strlen @s` dumped core. `@name`
lowers to a collection traversal, so a scalar subject reads a `char*` as a
`SashTable*`. Five faces: `for x in @s`, `strlen @s` and `f @s` segfaulted;
`for x in @i` and `for x in @nosuch` ICE'd in codegen. `echo @s` was masked by
SB5030, but that is a rule about *where* a splat may appear, not what it may
name.

**This is SB5068 with the operator changed.** That bug — subscripting a
scalar — was fixed 2026-09-10, and its postmortem names the survey that
found it: `len`, `join`, subscript, **and for-in**. For-in is the splat. The
inventory listed the next bug in writing and it was fixed one operator at a
time anyway, seven days later. The rule now recorded: **when a survey
enumerates siblings, check them all in the same pass.**

The fix reuses SB5068's machinery unmodified (`program_scopes`,
`collect_subscriptable_names`, `subscript_subject_is_scalar`) — the payoff
for that fix having put the scope logic in `validate.rs` rather than
`type_check.rs`. One asymmetry needed thought: SB5068 could leave undefined
names to SB5031, but **SB5031 enumerates scalar reads and a splat is not one**,
which is exactly why `@nosuch` ICE'd instead of being caught for free. SB5079
reports undefined splats under SB5031 itself, matching
`splat_not_subscriptable_diag`. Specs `75.81`–`75.89`.

**A second look came back clean.** `try_len_intrinsic`'s `strlen` fallback runs
67× although SB4030 rejects `len` on a scalar, so by its comment it should
be dead. Instrumented and swept: `args`, the DD#100 slots `A`–`E`/`f`, and
the DD#58 arithmetic `(len s)`. The slots are genuine collections absent from
`state.collections`, and the answers are right — `handle_param_expansion`
lowers `"strlen"` to *read the slot's length field*, which for a collection
holds the element count. A misnomer, not a bug; comment rewritten before it
licensed someone to delete a live branch.

**For DG.** The six `expr_to_string` arms plus `opaque_form_to_string` are now
measured unreachable (0 hits across 2791 native specs and 20 probes) but their
exhaustiveness is deliberate — a new `ExprKind` must be a compile error
there first. They cannot be covered and should not be deleted, so they are a
**floor under the 100% coverage target**. The only mechanism that removes them
is the filed `eliminate-ice-arms-via-types` task. Raising it rather than
routing around it.

## ══ STATUS (2026-09-17) — four silent drops in one function ══

**Journal 351.** Sweep **5378 passing, 13 skipped, 0 failing** (+24; predicted
+24). Units 83. **Bug fixed: `signal` was entirely unvalidated.**

Found while probing a *coverage* question — whether a non-literal can reach
`expr_to_string`'s six cold arms — by executing one probe per call site rather
than reading twenty-one of them. It could not, but `signal $s …` compiled, and
so did every other malformed spelling: `signal`, `signal 2`, `signal TERM`,
`signal 2 BOGUS`, `signal 2 IGN extra`, `signal 9 IGN`, `signal $n EXIT`. All
compiled clean and did nothing.

`signal` had **no entry in `validate.rs` or `type_check.rs` at all**. The only
mention of it in the validator was SB4043's message, which tells the user to
write `signal N IGN|DFL|FLAG|EXIT` — the compiler named a form and then
accepted every wrong spelling of it in silence. `handle_signal` held four
silent drops (`args.len() >= 2`, `parse().unwrap_or(0)`, `signum > 0`,
`_ => {}`) and the runtime a fifth (`rt_sigaction`'s `-1` discarded by both
backends).

**Note the shape.** This is *not* the missing-arm pattern of 344/347/348/350 —
these are branches that exist precisely to do nothing, so no omission detector
would flag them and `_ => {}` reads as ordinary defensive code.

DG settled three open questions: the DD#56 "Supported signals" table is
**illustrative, not a whitelist** (accept 1-64 less 9/KILL and 19/STOP, the two
the kernel will not let a process catch); a **non-literal number is rejected
for all four dispositions**, not just `EXIT` (whose `exiting_signals` set is
compile-time and has no choice); and the **runtime aborts** on a rejected
registration rather than leaving a program without a disposition it believes it
has.

Four rules in `validate.rs` beside `check_trap`: **SB4059** arity, **SB4060**
non-literal/non-numeric number, **SB4061** uncatchable or out-of-range,
**SB4062** unknown disposition. All four gates then came out of
`handle_signal`, which fell from **cc 8 / cog 6 to cc 6 / cog 1** — the nesting
was essentially all of its apparent complexity. DD#56 records the enforcement
and both thresholds.

Specs `56.40`-`56.51` (12, dual-mode). `plan=` needed correcting 39→40 before
the run — I wrote twelve and counted eleven; a wrong `plan` only prints a note,
so the predicted-total discipline is what caught it. Second time this stretch.

**Still open, and the reason the probe was running at all:** `expr_to_string`
has six cold arms and `transform/expressions.rs::opaque_form_to_string` is
entirely cold. Next: finish classifying those, then `type_check.rs::fmt`, then
the lines and branches queues.

## ══ STATUS (2026-09-16) — the same arm, written twice ══

**Journal 350.** Sweep unchanged at **5354 passing, 13 skipped, 0 failing**
(pure refactor). Units 83. Coverage **96.85% lines / 89.82% branches**
(from 96.80 / 89.82).

Top coverage target `codegen/values.rs::lower_value_as_int` had 12 cold lines,
all one arm (`SsaValue::ArithExpr`). **The diagnosis came from the other
backend, not from the zero:** the c emitter's identical arm was hit **57 times**
on the same programs. Zero against 57 is not "untested" — it means the two
backends route the same SSA differently and one route is dead.

Native reached `ArithExpr` via `lower_value` (51 hits), whose arm did the whole
job itself and then stringified, so `lower_value_as_int`'s arm never ran.
`lower_value` now delegates instead of recomputing: one arithmetic lowering,
formerly-dead arm live 51×, **zero** uncovered lines in that function.

**The two copies were not identical** — one passed `exit_status_var` to
`emit_arith_op`, the other `None`. It made no difference because the parameter
is dead along its whole length: `emit_div_or_mod_with_zero_check` names it
`_exit_status_var` and ignores it. A dead parameter is exactly what lets two
copies of the same code disagree without consequence. Purged from both
signatures, four call sites, and the `Variable` import.

This is the **third instance this stretch** of a canonical answer with a
narrower duplicate beside it (344, 347, 348, 350). It is now the first thing to
check on any new target.

`lower_value_as_int` was a pre-existing gate violator (cc 12/cog 7);
grandfather-on-modify fired and the `CmdSubst` arm moved out to
`lower_cmdsubst_as_int`, leaving the dispatcher at cc 10/cog 2. No bypass.

**Note for target picking:** branches did **not** move (89.82% both sides) —
the arm was straight-line, so it was line-region cold, not branch cold.
Uncovered *lines* and uncovered *branches* are different queues; this stretch
has been working the first while quoting the second.

**Next up** (by uncovered lines): `transform/expr_handlers.rs:fold_arith_op`
(12 cold, **cc 27/cog 9** — the gate will bite, budget a refactor),
`codegen/value_handlers.rs:emit_cmdsub_exec` (10),
`codegen/values.rs:intern_or_declare` (10),
`parse/parser.rs:parse_split_subscript_index` (10),
`parse/parser.rs:parse_trailing_redirects` (10).

## ══ STATUS (2026-09-16) — a test in a pipeline died at run time ══

**Journal 349.** Sweep **5354 passing, 13 skipped, 0 failing** (+16, predicted
exactly). Units 83 green.

**BUG FIXED: `[[ … ]]` as a pipeline stage compiled clean and aborted at run
time.** A bare test parses to a `Command` *named* `"[["`; every ordinary
context dispatches that name to test lowering, but the pipeline path lowers
stages generically, so `build_argv` → `expr_to_value(Test)` fell through `_`
to a stringified `SsaValue::Expr`. Codegen emitted a command literally named
`[[` and the runtime's own assertion fired: `sash: bug: bare command passed to
sash_exec`. DD#23 resolution exempts `[[` from SB5011, so nothing upstream
objected either.

**DG called for an audit before the fix, and it found a cohort of five.**
`[[ … ]]` (both positions) and `attest …` aborted at run time; `(( n = 2 )) |
cat` was quieter still — it compiled, ran, exited 0 and **silently lost the
assignment**, because a stage is a forked child. `if`/`for`/`match` were
already rejected, which is the tell: **SB4022 already existed** ("a pipe on a
compound command isn't allowed") and simply never classified these three as
compound.

Fix: `reject_statement_form_pipeline_stage` in `parse_trailing_pipeline`,
unwrapping `Suppressed`/`Negated`/`Redirected` so `- [[ … ]] | cat` cannot
smuggle a test through. Deliberately narrow — **only the pipe is rejected**;
`[[ … ]] && …` and `|| …` are legitimate and still work (pf.23 guards the
over-rejection). Specs `pf.16`–`pf.23` in `spec/pipe_functions.sh`, whose
header already documented the identical runtime error for functions as stages.

**How it was found — and the honest scorecard.** DG asked whether any tool beats
rereading. A prototype **sibling-cohort omission detector** flagged three
functions compiler-wide; I triaged the third as a false positive by reasoning
from its constructors, then tested it and found this bug. Measured results,
filed in `notes/tasks/research-structural-defect-detectors.md`:

- the **language-surface matrix** — which I ranked first *on speculation,
  without running it* — is the worst option: 63% of cells empty, and the one
  cell that mattered was a false positive from a regex `\s` matching a newline.
  It would have missed journal 347 and reported green.
- the **omission detector** gives a 3-line report over the whole compiler and
  found a real bug, but its decisive threshold (0.80 vs 0.85) rests on a single
  labelled positive, and regex brace-counting silently gave a false 0 hits.
- **mutation testing** could not have found 347; the `--emit-c` oracle is blind
  to front-end bugs by construction (both backends share the parser).

Nothing beat rereading as a *discovery* mechanism. Coverage's value is
**attention allocation**, and it remains the best allocator tested.

**Next:** resume the branch-coverage loop. `coverage.db` is now four commits
stale — re-measure (`mk cov-reset && mk cov-branch && mk cov-db`) before
picking targets. Ranked: `codegen/values.rs:lower_value_as_int` (12 cold),
`transform/expr_handlers.rs:fold_arith_op` (12 cold, cc 27/9 — budget a
refactor, the complexity gate bites), then `emit_cmdsub_exec`,
`intern_or_declare`, `parse_split_subscript_index`, `parse_trailing_redirects`.

## ══ STATUS (2026-09-16) — the front end ran three times ══

**Journal 348.** Sweep **5338 passing, 13 skipped, 0 failing** (+6, predicted
exactly). Units 83 green. **Native compile time −43%** (0.37s → 0.21s on a
3000-line program).

Every native compile ran `type_check` + `transform` **three times**:
`enforce_transform_diags` kept the diags and threw away the SSA,
`compile_native` kept the SSA and threw away the diags, and
`codegen::compile_to_object` **re-parsed the source from scratch** and built a
third — the one codegen actually used.

**Not a correctness bug; I checked the obvious one.** The re-parsed program is
*unresolved* (DD#23 `resolve_commands` mutates only the caller's copy), so
codegen could have been exec'ing bare `cat`. It wasn't —
`transform/resolution.rs` re-resolves idempotently through the same
`resolve_command_path`.

**Fixed because of the seam, not the speed.** The taint gate and codegen ran on
*different SSA objects* that nothing forced to agree, and `compile_to_object`
was a `pub`, re-exported, validate-bypassing entry point whose doc claimed
"the ONLY entry point from validated source to object code" while accepting raw
source and calling no validator. That is exactly how the deleted fail-open
harness looked defensible.

`transform_or_exit` now runs the transform **once** and returns the SSA;
`cli_main` threads it through `compile_native` into
`compile_to_object(&Program, &SsaProgram, &str, bool)`. cc 12 → 4. Its only
remaining failure mode is the `catch_unwind` ICE path. Specs `cov.306-308` pin
that transform diags still gate `--emit-ir` / `--manifest` / `--emit-c`.

DG ruled "pass the SSA in", which on its face addresses only the third call
site; sites 1 and 2 are the same duplication one layer up, so all three went.

**Three cold-line diagnoses in three commits, same measurement, three
meanings** — 346 unreachable by construction (delete the variants), 347
genuinely untested (and hiding a bug), 348 pre-empted by the caller (delete the
duplication). **The cold count tells you where to look and nothing more. Every
one was settled by reading the caller.**

**Next:** `codegen/values.rs:lower_value_as_int` (12 cold, cc 12/7) and
`transform/expr_handlers.rs:fold_arith_op` (12, cc 27/9 — top non-dispatcher
hotspot, and the only near-target the complexity gate would bite on). Thinnest
*files* by branch coverage: `transform/capture_purity.rs` (50.00%),
`transform/pipelines.rs` (66.67%), `transform/expr_handlers.rs` (74.44%).
`coverage.db` now predates three commits and its `expr.rs`, `lib.rs` and
`codegen/entry.rs` line numbers are stale — re-measure before trusting it.

## ══ STATUS (2026-09-16) — the word `str` was a parse error ══

**Journal 347.** Sweep **5332 passing, 13 skipped, 0 failing** (+14, predicted
exactly). Units 83 green.

**BUG FIXED.** `str` is a reserved word, and every reserved word is usable as a
bare word — except `str`, which was an **SB4014 parse error in every word
context**: `echo str`, `str s=str`, `[[ str == str ]]`, `for x in str`. Only the
exact spelling (`echo str2` and `echo strs` were always fine).

`literal_token_text` carried a 21-token alternation duplicating
`keyword_to_str`'s 21-arm list, while `lexer::classify_word_token` promotes
**22** spellings. `Tok::Str` was in neither, so it fell through to
`absorb_text`, which returns `None` for it, and the token terminated the word.

It survived because `str` appears constantly as a *declaration* — handled at
`parser.rs:231`, long before word parsing — and essentially never as data.

**Fixed at the cause, not the symptom.** `keyword_to_str` is now the only list;
it returns `Option<&'static str>` and `literal_token_text` chains
`.or_else(absorb_text)`. The old `_ => ""` sentinel is gone — it was
unreachable, and had anything reached it, it would have produced a silently
**empty word** rather than an error.

Specs `cov.299-305` pin all 22 spellings plus the three broken non-argument
contexts, and guard that a reserved word usable as a bare word still introduces
its own construct.

**This is the fourth instance this session of one shape**: a canonical answer
with a narrower duplicate written beside it (344 Ã 2 bugs, 346 dead code, 347
this bug). It is now the first thing to check on any new target.

**Method note that earned its keep.** 346 proved a cold dispatcher
*unreachable*; the reflex was to expect the same here. `plan.md` carried an
explicit warning not to assume symmetry — `Tok` is live where `LogosTok` was
not — and that is what turned 15 cold lines into a bug instead of a deletion.
**A cold count cannot distinguish "untested" from "impossible". Only the caller
can.**

**Coverage baseline refreshed** (four commits stale): **95.85% lines / 89.63%
branches**, up from 95.52 / 89.53. `coverage.db` rebuilt — but its `expr.rs`
line numbers predate this fix, so re-measure before picking that file again.

**Next:** `codegen/entry.rs:compile_to_object` (14 cold),
`codegen/values.rs:lower_value_as_int` (12),
`transform/expr_handlers.rs:fold_arith_op` (12, cc 27/9 — top non-dispatcher
hotspot and the only one the complexity gate would bite on). Thinnest *files*
by branch coverage: `transform/capture_purity.rs` (50.00%),
`transform/pipelines.rs` (66.67%), `transform/expr_handlers.rs` (74.44%).

## ══ STATUS (2026-09-16) — twenty-seven tokens that could never match ══

**Journal 346.** Sweep **5318 passing, 13 skipped, 0 failing**. Units 83 green.

**27 of `LogosTok`'s 45 variants deleted** — the 22 keywords plus
`DblBracketOpen`, `DblBracketClose`, `HereString`, `PlusAssign`, `Bang`.
`to_tok` now returns `Tok` rather than `Option<Tok>`; its `Comment` arm is an
ICE citing a new `LOGOS_DISPATCH_INVARIANT`, and the sole caller has lost its
`.expect()`. `to_tok` cc 46 → 19, cog 1.

`Scanner::next_token` hands logos exactly ten bytes — newline `#` `;` `|` `&`
`(` `)` `{` `}` `>` — and logos matches from position 0, so a pattern starting
with any other byte *cannot* fire. Keywords start with a letter and route to
`scan_word` → `classify_word_token`; the five operators each have their own
hand-rolled scanner. The cold-line set from the coverage run matched that
constraint exactly, which is what turned an observation into a proof.

**This was a ruling, not a judgement call.** The module doc said the keyword
variants were deliberately retained as guard rails for a possible future
migration of keyword promotion under logos, and the equivalence tests it cited
were still live — so the justification had not rotted. "Purge dead code" and a
documented deliberate choice genuinely disagreed, so it went to DG, who ruled
**purge all 27 and tighten the signature**.

The retention was worse than inert: the keyword list was a second, unchecked
copy of `classify_word_token`'s real table, free to drift invisibly. Same shape
as both bugs in journal 344 — a canonical answer with a narrower duplicate
beside it — except this one had not caused a bug *yet*.

Also settled: `to_tok` was never a coverage gap. Its 28 cold lines were covered
by unit tests, i.e. squarely inside DG's "modulo unit tested regression guards"
carve-out. It was a dead-code finding wearing a coverage gap's clothes.

Specs `cov.284-298` pin that the hand-rolled paths still lex every deleted
token — including `case`/`esac` (SB4029) and `<<<` (SB4050), retired surfaces
that must still lex or the diagnostics retiring them could not fire.

**Correction to SOP:** `spec/coverage.sh` is a **dual-mode** file (+2N per
spec), not native-only. Predicted +15, got +30; the collision check was clean
and all 15 ids appeared under both `c:` and `native:`, so the wrong model was
mine, not the tree's.

**Next:** back to the branch-coverage loop. `coverage.db` now predates four
commits — refresh with `mk cov-reset && mk cov-branch && mk cov-db` before
picking the next target. Ranked: `parse/expr.rs:keyword_to_str` (15 cold,
cc 23/1 — **likely the same dead-dispatcher shape, and it sits right beside
the table just deleted**), `codegen/entry.rs:compile_to_object` (14),
`codegen/values.rs:lower_value_as_int` (12),
`transform/expr_handlers.rs:fold_arith_op` (12, cc 27/9 — top non-dispatcher).

## ══ STATUS (2026-09-16) — two emitters, gone ══

**Journal 345.** Sweep **5288 passing, 13 skipped, 0 failing** (+3 native-only,
predicted exactly). Units 83 green.

`emit_ir::ar_incdec` and `ar_subscript_assign` **deleted**; both arms now ICE
against a new `ir::ARITH_IR_MUTATION_INVARIANT`.

The probe from 344 settled it: never fired across 5259 compiled programs, with
a control in `ar_assign` proving the mechanism worked (27 hits) and `emit_ir`
running on *every* compile, not just `--emit-ir`. The suite contains 25
C-style `for` headers and two `a[i]` assignment statements, so the obvious
sites were exercised.

Structurally, `emit_ir` renders the post-validate AST, and by then
`parse::lower_arith_command` has rewritten a top-level `(( a++ ))` /
`(( a[i] += 5 ))` and desugared a C-style `for` header into
`Statement::Assignment`, while SB4020 has rejected the rest whole-tree. That
last gate allows plain and compound assigns, which is why `ar_assign` is live
and these two were not.

They **ICE rather than being deleted** because `emit_ir::arith`'s match is
exhaustive — no catch-all to fall through to, so an arm can only be replaced.
The opposite of `hoist_mutation_node` (343), where deletion was unsafe
*because* `_ => None` would have swallowed the node silently.

Specs 104.6-104.8 pin the lowering. `notes/tasks/dead-arith-mutation-emitters.md`
closed — both of its predictions about `hoist_mutation_node` and about removing
the `ArithExpr` variants outright were wrong.

### Next

Back to the branch-coverage loop, targets ranked by uncovered lines:
`parse/logos_tokens.rs:to_tok` (28), `parse/expr.rs:keyword_to_str` (15),
`codegen/entry.rs:compile_to_object` (14), `codegen/values.rs:lower_value_as_int`
(12), `transform/expr_handlers.rs:fold_arith_op` (12 — 27/9, the top
non-dispatcher hotspot).

**Settled — the complexity gate's flat-dispatcher dead zone.** The rubric said
"high cc, low cog — accept as-is" but the gate's only dispatcher exemption was
`cog − cc ≤ −15`, unreachable below cc 17 at cog 2 — so every dispatcher with
fewer than ~17 arms was rejected by the hook and excused by the rubric.

**DG ruled 2026-09-16: accept as-is for cog ≤ 2.** The gate now carries two
dispatcher exemptions, one per shape: the delta test for *wide* ones
(`to_tok`, cc 46 / cog 1) and `cognitive > 2` for *flat* ones. Frees 7
functions (all cc 12–15 / cog 1–2, every one a flat lookup); 48 stay gated,
so it is a correction rather than a relaxation.

`notes/journal/108-cog-cc-rubric.md` had said since it was written that a
dispatcher is signalled "for free via cog ≤ 3" — the hook simply never
implemented that half. Amendment appended there; AGENTS.md and
`.githooks/pre-commit` updated to match.

## ══ STATUS (2026-09-16) — four bugs behind an attest ══

**Journal 344.** Sweep **5285 passing, 10 skipped, 0 failing** (+26, predicted
exactly). Units 83 green.

### A reachability probe found a bug cluster

`emit_ir::ar_incdec` / `ar_subscript_assign` are **confirmed dead** — a
`probe_hit()` over the whole sweep fired only its control, across 5259
compiled programs. (`emit_ir` runs on *every* compile, not just `--emit-ir`.)
That work lands separately; establishing *why* they were dead is what turned
up the bugs.

DD#93 exempts an attest's `(( TARGET = SOURCE ))` predicate from
`check_arith_value_mutation`, and the predicate is an `ExprKind::Arithmetic`,
not a `StmtKind::ArithTest`, so `check_arith_test_placement` misses it too.
Four defects behind that one carve-out, all pre-existing at HEAD:

| # | spelling | was |
|---|---|---|
| 1 | `attest n (( c = n + (v[0] = 3) ))` | **ICE** (also ternary / subscript-index / unary operands) |
| 2 | `attest n (( c = n + v[d] ))` | false SB5050 — valid program rejected |
| 3 | `attest n (( c = n + zz ))` | **accepted; undefined `zz` silently read as 0** |
| 4 | `attest n (( c = n + vv[0] ))` | **ICE** in codegen on the undefined array |

**Bug 1** — `parse::has_mutation_inner` was a fifth hand-rolled copy of
`ir::arith_has_mutation` (343 collapsed four copies of the walk it is built
on). It stopped at `BinOp`/`Paren` and omitted `SubscriptAssign` outright, so
the mutation reached the `unreachable!()` in `transform::expr_handlers`.
Deleted; delegates to the canonical predicate.

**Bugs 2-4** — the arm collected reads with `ir::walk_arith_var_reads`, which
is *complete* but answers a different question: DD#77 D1's "which bare int
operands cross from string?". It therefore skips a subscript subject name and
a sexpr callee name **by design**, and checks no name for definedness. Now
uses `check_arith_reads`, the collector a plain `(( … ))` already uses.

Bug 3 is the serious one: every other spelling of an undefined read reports
SB5031, and validate.rs carries a comment enumerating that list and calling
the sexpr node "the sole hole". An attest predicate was a second hole — and a
silent zero is worst exactly where the construct exists to *prove* something.

13 specs `cov.271-283`, covering every node kind the walk descends into plus
the positive controls that widening it must not break.

### Next

`ar_incdec` / `ar_subscript_assign` removal, then back to the branch-coverage
loop (`to_tok` 28 cold lines, `keyword_to_str` 15, `compile_to_object` 14).

## ══ STATUS (2026-09-16) — four copies of the same walk ══

**Journal 343.** Sweep **5245 passing, 10 skipped, 0 failing** (+16, predicted
exactly). Units 83 green.

### `hoist_mutation_node` triaged three ways

28 cold lines across three arms. The filing called it "a genuine spec gap, not
dead code" — a third right:

| arm | verdict |
|---|---|
| `CompoundAssign` | **reachable and working**, zero coverage → 6 specs |
| `PostIncDec` | rejected by SB4020 before transform (12/12 spellings) → ICE |
| `SubscriptAssign` | rejected by SB4020 before transform (7/7) → ICE |

`(( (a+=2) ))`, `(( 1 + (a+=2) ))`, `(( -(a+=2) ))`, `(( (a+=2) + (b+=3) ))`
all work and none was tested, which is why the whole arm read as dead.

The two ICE arms cite a new `ir::ARITH_TEST_HOISTABLE_INVARIANT`. Unlike the
prefix case in 342, the guarantee is **structural**: `check_arith_test_placement`
rejects the whole tree via three whole-tree predicates, not spot checks at two
entry points. cov.262-263 pin those rejections so the invariant stays true.

Deleting instead of ICEing would have been wrong — the arms sit above a
`_ => None`, so a mutation that slipped through would be silently *not*
hoisted: the store dropped, the program quietly different.

`hoist_mutation_node` 10/6 → **6/1**.

### Four copies of one walker

Chasing those predicates turned up `validate::arith_children` — which is what
I added to `ir.rs` yesterday as `ArithExpr::children()`, believing it new.
There were four:

- `ir::ArithExpr::children` (mine) · `validate::arith_children` ·
  `validate::arith_operands` · `transform::functions::arith_children`

`arith_operands`' own doc says it "mirrors `transform::functions::arith_children`"
— the duplication was documented rather than fixed, twice, and I added a
fourth without noticing. All collapsed onto `ArithExpr::children()`, five call
sites repointed. Mine survives on merit: the other three end in `_ => vec![]`,
so a new `ArithExpr` variant silently gets no children and every walker stops
descending. The `ir.rs` one is exhaustive — a new variant is a compile error in
exactly one place.

**Method note:** `block_dupes` would have caught this. Before adding a helper,
grep for the *shape*, not the name — `arith_children`, `arith_operands` and
`children` are three names for one function.

### Next

Remeasure before picking the next target — `coverage.db` predates vm.bl19-23
and cov.250-263: `mk cov-reset && mk cov-branch && mk cov-db`, then
`mk cov-reset && mk build` before any sweep. Then `fold_arith_op`
(expr_handlers.rs, 4 cold, cc 27 / cog 9 — the top hotspot that is not a pure
dispatcher), `compile_to_object` (4), `check_len_operand` (4, probably
shadowed), then the tail of 3s. Triage rubric is in `plan.md`.

## ══ STATUS (2026-09-16) — the invariant that was not ══

**Journal 342.** Sweep **5229 passing, 10 skipped, 0 failing** (+6, predicted
exactly). Units 83 green.

### DG overruled the PreIncDec recommendation — and it found a bug

I had recommended keeping the eight merged `| PreIncDec` alternatives because
deleting them buys no coverage. DG:

> deleting a merged alternative would prevent it matching in the future, yes?
> given a disallowed usage is merged with an allowed usage - that's
> undesirable. That it cannot be measured doesn't prevent reasoning about it.

`x++` is allowed, `++x` is rejected; merged, the arm is correct only by
coincidence. Split into its own `unreachable!()` arm at nine sites, citing a
new `ir::PREFIX_INCDEC_INVARIANT`.

**Two of ten prefix spellings immediately ICEd** — `"$(( ++x ))"` and
`"${x:-$(( ++x ))}"`. The task file's claim (repeated twice) that the two
`parse_arith_str` bypass callers "carry their own rejection" was **false**.
They call it raw; their SB4020 came from `validate`, which caught the prefix
form *only* because `arith_has_mutation` lumped it in with the mutations. The
merged alternative was load-bearing — exactly the hazard DG described.

User-visible symptom: the identical construct got the specific `prefix ++x is
not allowed` message unquoted, and the vague generic mutation message inside
double quotes.

**Fix:** `reject_prefix_incdec` moved from the two entry points into
`parse_arith_str`, the one funnel every route passes through — structural
rather than asserted-where-remembered. Both entry-point calls deleted as
redundant. `cov.253-255` pin the three formerly-bypassing routes and assert
the **message**, not just the code.

### Complexity: two grandfathered violators brought down

Nine new arms tripped the gate on three functions, two already over the bar.
`ArithExpr::children()` names the shape once so each walker is a single loop;
the lowerer split into `arith_assign_stmt` / `binop_on_var` /
`subscript_assign_rhs` (four arms had built a byte-identical `Assignment`).

| function | before | after |
|---|---|---|
| `arith_has_mutation` | 14 / 2 | **4 / 1** |
| `check_positionals_arith` | 14 / 3 | **6 / 3** |
| `lower_arith_mutation_structured` | 12 / 6 | **10 / 4** |

Note the rubric and the enforced gate disagree for wide-shallow walkers: the
dispatcher exemption needs `cog - cc <= -15`, so at cog 2 a dispatcher only
escapes at cc 17. A twelve-arm walker with cog 2 is in a dead zone. The
answer is to stop hand-rolling the walk, not to widen the exemption.

### Method

Journal 341 said a cold branch is not a dead branch. This is the mirror: **a
hot branch is not a justified branch.** An arm carrying 1305 executions says
nothing about whether the alternative you care about ever matched. A merged
alternative is a question coverage is structurally incapable of asking — the
only way to ask it is to separate the alternatives and see what breaks.

### Next

Resume the coverage loop. Best target remains `hoist_mutation_node`
(`transform/statements.rs`) — 28 cold lines, a genuine spec gap
(`CompoundAssign` / `PostIncDec` / `SubscriptAssign` arms all zero). Then
`fold_arith_op`, `compile_to_object`, `check_len_operand`. Remeasure with
`mk cov-reset && mk cov-branch && mk cov-db` — `coverage.db` predates
vm.bl19-23, cov.250-255 and the pre-scan purge.

## ══ STATUS (2026-09-16) — purging a pre-scan that could not insert ══

**Journal 341.** Sweep **5223 passing, 10 skipped, 0 failing** — unchanged,
as predicted for a pure deletion. Units 83 green.

### Task A done: the codegen positional pre-scan (DG's purge ruling)

Deleted the closed five-function subtree from `codegen/values.rs` — **85
lines** (`maybe_insert_positional`, `scan_value_for_vars`,
`scan_composite_children`, `composite_children`, `scan_test_for_vars`),
whose only callers were each other. `setup.rs` collapsed to a redirect-only
walk, lost five dead parameters and its `too_many_arguments` allow, and the
survivors now say what they do: `init_undeclared_redirect_vars` /
`collect_redirect_target_vars` / `init_one_redirect_var`.

Both cold `continue` guards went too. The `vars.contains_key` dedup is
provably dead — `VersionState::define` increments before use, so the first
SSA version is 1 and `{name}_0` is this pass's private namespace
(`types.rs:182` / `dispatch.rs:530` only read the convention). The `?`/`$`
guard only became dead *after* journal 340's fix; before it, it was
load-bearing and wrong.

### Task B stopped: the `PreIncDec` filing is partly wrong — needs DG

Not executed. The arms are unreachable, but they are not interchangeable:

- **Documented keeps.** `validate.rs:1711` relies in a comment on
  `classify_arith_expr` (arith.rs:239) and `lower_arith_mutation_structured`
  (arith.rs:1292) as the second line of defence behind
  `reject_prefix_incdec`. Deleting them invalidates a backstop another
  pass's comment cites.
- **Silent-weakening.** Dropping the alternative from `has_mutation_inner`
  (`_ => false`) or `arith_has_incdec` yields no compile error if the
  invariant breaks — it answers "not a mutation". `validate.rs:3442` has no
  catch-all, so keeping it there *is* the compile-time guard.
- **Clean deletes:** only `brief_arith` (arith.rs:189) and `emit_ir.rs:648`,
  by the precedent `validate.rs:2081` already documents.

**Question for DG:** take the two safe deletes and close, or close the task
as "deliberate, documented"?

### Loop state

Coverage last measured at **396 cold branches**, before `vm.bl19`–`23`,
`cov.250`–`252`, and this purge (which removes ~9 cold branches by deleting
their code). Remeasure next: `mk cov-reset && mk cov-branch && mk cov-db`.

Next targets after that: `fold_arith_op` (4), `compile_to_object` (4),
`check_len_operand` (4, probably shadowed by earlier validate rejections),
then the tail of 3s.

## ══ STATUS (2026-09-16) — the redirect target with no slot ══

**Journal 340.** Sweep **5223 passing, 10 skipped, 0 failing** — predicted
exactly (3 specs × 2 modes = +6). Units 83 green.

### Bug fixed: `> $$` ICEd, `> $!` redirected to the empty filename

Found while probing the cold `continue` guards in `init_positional_params`
ahead of the purge. Redirect-target codegen is slot-based, but `$$` and `$!`
are served by intrinsic codegen and have no `VarSlot` — so `> $$` hit the
guard that deliberately skips `$`, got no slot, and raised the
unresolved-target ICE, while `> $!` had no guard, got an **empty** slot, and
silently redirected to `""` (the `bug-redirect-nonsimple-target-empty`
class, leaking through a predicate that did not name it).

Fix is the mechanism already in place: `target_needs_spill` now returns true
for the slotless specials, so they bind to a synthetic scalar via
`expr_to_value` exactly like `${x:-f}` / `$a[i]`. `> $$` now means what
`str s=$$; > $s` always meant; `> "$$.log"` falls out free (the predicate
already recurses into `Concat`/`DoubleQuoted`). Both backends.

Specs `cov.250`–`cov.252`.

**Method note: a cold branch is not a dead branch.** It is an unasked
question. This one was reachable, load-bearing, and wrong.

### Purge status (DG: "purge dead code; only exception would be ICE")

Not yet started — the bug above pre-empted it. Both task files hold full
evidence:

- `notes/tasks/purge-dead-positional-prescan.md` — the four `values.rs`
  scanners (~46 lines) plus the `setup.rs` collapse and rename. The
  `?`/`$` guard is **now** genuinely dead (the spill rewrites the target to
  `__redir_target_N` before codegen's scan sees it), so it comes out with
  the rest rather than standing alone.
- `notes/tasks/purge-preincdec-dead-node.md` — `ArithExpr::PreIncDec` never
  survives the parser; keep the variant and `parse_pre_incdec`, drop the ~10
  downstream arms, `unreachable!()` where a match must stay exhaustive.

Coverage last measured at **396 cold branches**, before `vm.bl19`–`23` and
`cov.250`–`252`. Remeasure after the purge.

## ══ STATUS (2026-09-16) — a filter that cannot pass ══

**Journal 339.** Sweep **5217 passing, 10 skipped, 0 failing** — predicted
exactly (5 specs × 2 modes = +10). Units 83 green. Coverage **396 cold
branches** (was 410, was 422).

### Specs: `skip_dollar_var` (vm.bl19–vm.bl23)

The brace-less disposition operand's own `$name` scanner (DD#90/#91) was
called three times in the whole sweep, always a single alphabetic byte
followed by a terminator — seven cold branches. `vm.bl16` only *looks* like
it covers this; `${…}` there takes `skip_braced_to_close` instead.

Useful mechanic: **`run_sash` passes source with `-c`, not via a file**, so
a spec string ending mid-name exits the scan loop on end-of-input. No
staging needed for that branch.

### Find: the codegen positional pre-scan cannot insert anything

`collect_positional_refs` walks every value of every instruction of every
block, every compile. Its only filter (`maybe_insert_positional`) ran **89
times and inserted 0 times** — all four TRUE branches cold.

The consumer *does* run (10 times), which is what made it confusing: the set
has **two producers with different contracts** and only the other one
contributes — `redirects.rs::scan_redirect_for_vars` inserts every redirect
target name, unfiltered. That is the live path, serving DD#53.

`init_one_positional`'s own doc comment says why the filter cannot pass: the
numeric / `$#` / `$@` strategies **were retired** (SB5024/SB5045 reject
top-level positionals; function ones resolve through `args`, DD#60).
Probed — SB5017, SB5024, SB5024, SB5045.

~46 lines in `values.rs` plus most of `scan_inst_for_vars` are dead.

**Not deleted — filed.** The retirement is *documented*, so the scanner was
left standing knowingly or unnoticed, and those readings lead to opposite
actions ("always ask before changing something that might be deliberate").
`notes/tasks/purge-dead-positional-prescan.md`: delete / assert
`unreachable!()` / leave, recommending delete.

### Filed: cross compilation and `setjmp.S` on mac

`notes/tasks/cross-compile-and-setjmp-on-mac.md`. `sash-rt/csrc/setjmp.S` is
43 lines of unguarded x86_64 AT&T assembly — no `#if`, no `__aarch64__`, no
`__APPLE__`. Fails on mac twice over (Apple Silicon is aarch64; Mach-O
mangles C symbols with a leading underscore, so the `_setjmp`/`setjmp` pair
aliases the wrong thing).

It is an inconsistency *inside* `sash-rt`: `src/lib.rs:1259` already has a
deliberate `#[cfg(target_arch = "aarch64")]` `_start` whose comment records a
real drift bug. Rust half portable, C half not.

Filed with the wider gap because `setjmp.S` alone buys nothing:
`cranelift_native::builder()` is host-ISA-only, 41 sites hardcode
`CallConv::SystemV`, `lib.rs:652` shells out to GNU `ld -static`,
`embed_sash_source` writes an ELF section — and Apple does not support static
linking at all, which collides with byte-identical static-ELF output.
**Scope is DG's call** (sash-rt merely builds on mac / sash runs on mac /
sash cross-compiles are three different projects).

### Two open questions for DG

1. `purge-dead-positional-prescan.md` — delete the dead filter subtree?
2. `purge-preincdec-dead-node.md` (journal 338) — `ArithExpr::PreIncDec`
   never survives the parser; ~10 cold branches across four passes.

Both are "dead code found by the coverage loop, but the code might be
deliberate". They are the two things standing between the loop and the
functions it cannot otherwise clear.

### Coverage state — 396 cold

Named exceptions (no spec wanted): `panic_payload_message` (4) is covered by
three unit tests and `mk cov` excludes unit tests — DG's "modulo unit tested
regression guards"; `join_intrinsic` (4) is two `unreachable!()` invariant
assertions.

Next targets: `fold_arith_op` (4), `compile_to_object` (4),
`check_len_operand` (4, likely shadowed by earlier validate rejections —
`$(len)` → SB4045 and `$(len "abc")` → SB5062 both fire before type check),
then a tail of 3s (`emit_wait`, `base_var_name`, `parse_var_base_version`,
`absorb_text`, `dynamic_subscript_var`, `try_at_transform`,
`try_split_subscript_arm`, `looks_like_dynamic_subscript_assign`,
`cmd_subst_taint`, `command_is_pure`, `detect_array_iter`,
`insert_case_phis`).

---

## ══ STATUS (2026-09-16) — an unterminated `$(` compiled ══

**Journal 338.** Sweep **5207 passing, 10 skipped, 0 failing** — predicted
exactly (9 specs × 2 modes = +18). Units 83 green.

### The bug

An unclosed `$(` in unquoted position was silently discarded:

| input | before |
|---|---|
| `echo $(echo hi` | compiled, printed `hi` |
| `echo $(status` | compiled, then `sash: bug: bare command passed to sash_exec` |
| `echo "$(status` | SB4057 — correct all along |

`parse_command_substitution` (`parse/expr.rs:1503`) broke out of its loop on
`at_eof()` and returned a well-formed `CommandSubstitution`; nothing
downstream could tell "closed" from "ran out of input".

**This is journal 313's bug one layer up.** That round audited every
scan-to-closer helper in the *lexer* — the `if self.pos < len { pos += 1 }`
idiom where the guard *is* the detection and silently does nothing — and
fixed eleven of them (57u.1–57u.12). `parse_command_substitution` has the
identical shape but is not a lexer helper, so it was out of scope.
**The pattern, not the module, is what wants auditing.**

Fix: a `closed` flag set only on the `RParen` arm, SB4057 otherwise —
same code and wording as the quoted path. Specs 57u.13–57u.18, filed
next to the lexer family.

### Found by the coverage loop, as intended

All four cold branches in `parse_status_word` were the *same* `while`
condition — meaning the loop was never entered, not that an arm was
unexercised. Every spec used `$(status)`, which returns from the
`RParen` fast-path above it. Probing for a spelling that gets past the
fast-path is what surfaced the unterminated case.

`cov.247–249` cover the rest: `(( x = y-- ))` (the `--` arm of
`brief_arith`), `$(status /bin/true)` (SB4045), `$(status; echo b)`
(SB5073).

### Second find — filed, not acted on

`ArithExpr::PreIncDec` **never survives the parser**.
`reject_prefix_incdec` runs first at both arithmetic entry points and
recurses through the same node set as everything after it; the two
bypassing callers have their own rejection. So the variant exists only
to carry a diagnostic, and its arms in `transform/`, `emit_ir.rs`,
`validate.rs` and `brief_arith` are unreachable (~10 cold branches).

`notes/tasks/purge-preincdec-dead-node.md` — three options (delete /
convert to `unreachable!()` / leave), recommending the
`unreachable!()`-with-named-invariant convention already used by
`join_intrinsic`. **Spans four passes; DG's call.**

### Coverage state

Remeasured after the previous three commits: **410 cold branches** (was
422). The 9 specs above are not yet reflected — remeasure next.

Remaining dense cold functions (line numbers will have shifted):
`skip_dollar_var` (7), `compile_to_object` (4), `panic_payload_message` (4),
`maybe_insert_positional` (4), `fold_arith_op` (4), `check_len_operand` (4),
then a tail of 3s.

`check_len_operand`'s four are all early-return guards that appear to be
shadowed by earlier validate rejections (`$(len)` → SB4045, `$(len "abc")`
→ SB5062 both fire before type check) — likely dead conditions rather than
spec gaps. `join_intrinsic`'s four are its two `unreachable!()` invariant
assertions; those stay.

### Method note

Predicting the sweep total before running it caught nothing this round
because it matched — but it is what makes "it matched" mean anything.
Three batches in a row now.

---

## ══ STATUS (2026-09-16) — an ICE on `${1[0]}` ══

**Journal 337.** Sweep **5165 passing, 10 skipped, 0 failing** — predicted
exactly (6 specs × 2 modes = +12). Units 83 green.

### Coverage mining found an ICE

`${[0]}`, `${0[0]}`, `${1[0]}`, `${9[2]}`, `${12[0]}` panicked the compiler
(`codegen/types.rs:190`, "ICE: undefined array"), in both the plain and the
arithmetic context. Every all-digit collection name and the empty name.

**Cause:** two definitions of "name". `scan_param_name` scans a run of
`[A-Za-z0-9_]` — which admits a leading digit and the empty run — while
`is_plain_identifier` (ir.rs:408) requires an alphabetic or `_` first byte.
`record_param`'s SB5031 typo check skips `!is_plain_identifier(name)`, correctly
(`$1`/`$@` are handled by SB5024/SB5045), but that skip is written for a scalar
read and also excused a *subscript's collection name* — which nothing else
checked. The AST reached `find_array_slot` and panicked.

**Fix — DG chose option C** (reject at parse, reuse the existing SB4047
braced-body family) over a new validate code or relaxing SB5031's deliberate
skip. It needed only one diagnostic site: tightening the arithmetic scanner
`try_braced_subscript` to `is_plain_identifier` makes it return `None`, which
routes the body to the real parser (DD#77) — the same `try_subscript` that
handles the plain spelling. SB4047's stated principle is the one the ICE
violated: *"the compiler must never emit code for input it doesn't fully
understand."*

Specs `var_modifiers.sh` **vm.un7–un12**, next to the existing `vm.un1–un6`
SB4047 family. Each reject asserts `$err != *ICE*` as well as the code.
`vm.un12` guards the other direction — `${a1[1]}` must keep working, since the
reject is a *leading* digit, not any digit.

### Coverage-loop ledger so far

A ghost operator, three dead predicates, two `unset` bugs, and an ICE — none of
which the suite was failing on. Cold branches are where the compiler has never
been asked a question.

### Next

Coverage is **stale** — last measured at 88.66% / 422 cold, before the `unset`
and SB4047 work. Re-run `mk cov-reset && mk cov-branch && mk cov-db`, then
`mk cov-reset && mk build` before any behavioural test.

Probed and ready to spec (confirmed against the live binary, not yet written):
- `$(len args)` inside a function → prints 2 (flips `check_len_operand`
  1069|0|0)
- `$(len)` → SB4045; `$(len "abc")` → SB5062
- `unset i` on an int → SB4030 (flips `check_unset_args` 1429|1|3)
- compiling with no `-o` → writes `<input>` stripped of `.sh`; `-c 'script'` →
  `a.out` (flips all three cold branches in `lib.rs::native_output_path`)
- `$(( ++5 ))` / `$(( --5 ))` → SB4020 "expected variable after ++/--" (flips
  `parse_pre_incdec` 1114|0|1, 1115|0|0, 1115|0|1)

Next `coverage.sh` id is **cov.241** (plan=239). Remaining dense cold
functions: `codegen/entry.rs::compile_to_object` + `panic_payload_message`,
`lib.rs::sysroot_file_exists` (needs `--sysroot` with `.squashfs`/`.sqsh`),
`parse/expr.rs::parse_status_word`, `transform/expr_handlers.rs::fold_arith_op`
+ `join_intrinsic`, `parse/lexer.rs::skip_dollar_var` (still 7 cold after
cov.223–225).

**Environment:** always `TMPDIR=~/sashscratch/tmp bash spec/run.sh` — `/tmp` is
swept and a vanished mktemp yields a *green partial* sweep.

---

## ══ STATUS (2026-09-16) — two bugs in `unset` ══

**Journal 336.** Sweep **5153 passing, 10 skipped, 0 failing** — predicted
exactly (7 specs × 2 modes = +14). Units 83 green. Branch coverage **88.66%,
422 cold** as measured before this batch (was 431).

Coverage mining of `type_check.rs::check_unset_args` found a live bug, and
fixing it exposed a second one beside it. Both are fixed.

### 1. `unset a[i]` on an indexed array did nothing

An indexed array keys its entries by the **8 raw bytes of an i64**; a map keys
by the string. `handle_unset` emitted a string key unconditionally, so on an
array the delete hashed a key that cannot exist and returned not-found, which
nothing checks. `unset m[k]` worked, which is what proves the array case was an
omission — and DD#30's own `type_check` comment says subscript forms are
allowed.

Fixed by carrying the encoding in the IR the way every other subscript path
already does (`SsaValue::Subscript` / `SubscriptAssign` both hold an
`is_integer_key` set from `!state.assoc_arrays.contains(name)`). New
`BuiltinCmd::TableDeleteIdx`; both backends branch on it exactly as
`emit_subscript_value` / `lower_subscript_raw` do.

**No new semantics.** Sparse arrays already existed (`a[5]=q` on 3 elements →
`len=4`, hole at 3), so "length drops, slot reads empty" follows from the
model in place. No DD needed.

### 2. `unset a[0] a[1]` discarded every target after the first

`var_args.join("")` flattened distinct targets into one: `unset a[0] a[1]`
became a single target `a` with key `0]a[1`. `unset m[a] m[b]` deleted
**nothing**; `unset a[0] m[k]` left the map untouched. The join exists because
the parser can split `["m", "[b]"]` — now an arg only re-attaches when it
*starts with* `[`.

### The spec that could not see it

`types.sh` 30.41 "unset arr[N] removes element" asserts `${a[0]}` and
`${a[2]}` after `unset a[1]` — the two indices that do not change either way.
It passed throughout. Same shape as 334's lesson: a spec's title says what
someone meant to test, not what it tests.

`subscript.sh` 80.27 is the precedent — it exists because `unset m[K]` was once
a no-op **in the C backend**. That was the map half of this bug; the array half
survived because 30.41 could not detect it.

### Next

Resume the coverage loop at the 422 cold branches. Already-analysed, not yet
specced: `$(len args)` inside a function (flips `check_len_operand` 1069|0|0),
`unset i` on an int → SB4030 (flips `check_unset_args` 1429|1|3), `$(len)`,
`$(len "lit")` → SB5062. Next `coverage.sh` id is **cov.241** (plan=239).

Densest remaining cold functions (re-derive line numbers — they shifted):
`codegen/entry.rs::compile_to_object` + `panic_payload_message`,
`codegen/values.rs::maybe_insert_positional`, `lib.rs::sysroot_file_exists`
(reachable via `--sysroot`), `parse/expr.rs::parse_status_word`,
`transform/expr_handlers.rs::fold_arith_op` + `join_intrinsic`.

**Environment:** always `TMPDIR=~/sashscratch/tmp bash spec/run.sh` — `/tmp` is
swept and a vanished mktemp file yields a *green partial* sweep. `mk cov-reset`
deletes `target/release`, so the next `mk build` is a full rebuild.

---

## ══ STATUS (2026-09-16) — the ghost operator ══

**Journal 335.** Sweep **5139 passing, 10 skipped, 0 failing** — predicted
exactly (18 specs × 2 modes = +36). Units 83 green. Branch coverage **88.34%,
431 cold** as measured before this batch.

### `arrkeys` was handled in six places and constructed in none

DD#75 §3 made `$(keys m)` the canonical key splat and retired `${!a[@]}`.
`arrkeys` was `${!a[@]}`'s operator. The parser now emits `arrkeys_retired`
(rejected at parse, SB4038) — so retiring the surface deleted the **producer**
and left every **consumer** in place.

`$(keys m)` never produces an operator at all: it is a command substitution,
classified structurally by `detect_array_iter`'s cmdsub arm. Two mechanisms,
one unused.

Purged from four passes: `validate.rs` (×2), `transform/control_flow.rs`,
`codegen/value_handlers.rs` (×2), `emit_c.rs` (arm + `emit_arr_keys_joined` +
`emit_arr_join_loop`'s now-constant `keys` parameter). Five doc comments that
listed `arrkeys` as live were corrected, including `parse/expr.rs`'s expansion
table, which still claimed `${!var[@]} → arrkeys`.

Covered by DG's existing ruling in `remove-retired-surface-implementations`:
*"Retired surfaces should not keep working implementations."*

**The extern does NOT die with it.** `codegen/dispatch.rs:615` uses
`externs.table_key` for the real `$(keys m)` loop, so `sash_table_key` stays.
Only the selector goes. Verified after: `for k in $(keys m)` → `k=x / k=y`,
`$(join m)` → `1 2`, `$(join a)` → `p q`.

Coverage confirmed the constant parameter independently: `4048|0|0`=0,
`4048|0|1`=33 — thirty-three joins emitted, not one of them keys.

### 18 specs — `cov.223`–`cov.240`

Six drive `parse/lexer.rs::skip_dollar_var`, reached only from
`scan_braceless_operand` (the operand of a brace-less `@{a:-…}`), which had
never seen anything but a one-character identifier: `@{a:-$xy}` (multi-char),
`@{a:-$_z}` (`_`-led, the alternative an alphabetic lead short-circuits past),
`@{a:-$#}` (single-char special — consumed **before** the `#` word-terminator
check, which is the non-obvious part).

Three drive `detect_array_iter`'s non-splat paths: `for x in "$v"` (quoted, one
part), `for x in "$v$w"` (two parts → prints `ab`), `for x in ${v:-d}`.

The rest pin one-line rejections no feature spec writes: `$0` → SB5017 (not the
function-local family), `$@`/`$*` → SB5045 with **different directives at top
level vs in a function**, `$1`/`$#` → SB5024 likewise, `foo/bar` + `./x` →
SB5051, `echo*` → SB5011. Plus `${m["k"]}`, `${m['k']}`, `${a[$((1))]}` → `y`.

### Trap: SB5011 is emitted from two passes with identical text

`echo*` reports "command 'echo\*' not found" even though `is_valid_command` has
`if name.contains('*') { return true; }`. Not a bug — that arm is the same
deliberate deferral as the `/` arm above it, and the message comes from
**`resolve.rs:244`**, not `validate.rs:2887`. Two passes, byte-identical
diagnostic. Makes a trace ambiguous from the outside; know it before chasing it.

### Reported, not acted on

`emit_c.rs::arrjoin_separator` reads `4036|0|0`=33, `4036|0|1`=0 — it is only
ever called with `operator == "arrjoin"`, so `"arrvals"` never reaches the C
join despite `emit_c.rs:2820` matching `"arrvals" | "arrjoin"`. Possible second
ghost. Filed as `notes/tasks/trace-arrvals-c-backend-join.md` (P2) — it wants
the same trace-before-cutting treatment, not a guess.

### Coverage loop — next

**Re-measure first** (`mk cov-reset` → `mk cov-branch` → `mk cov-db`, ~40 min).
The 431-cold list predates both this purge and these 18 specs, so it is stale,
and a stale list makes `cov-probe.sh` answer "no branch records at that line".

`spec/coverage.sh` `plan=239`, next free id **`cov.241`**. `spec/taint.sh`
`plan=200`.

Next densest cold functions from the 2026-09-16 measure (all pre-purge):
`codegen/entry.rs::compile_to_object` 4 + `panic_payload_message` 4,
`codegen/values.rs::maybe_insert_positional` 4, `lib.rs::sysroot_file_exists` 4,
`parse/expr.rs::parse_status_word` 4, `transform/expr_handlers.rs::fold_arith_op`
4 + `join_intrinsic` 4, `type_check.rs::check_len_operand` 4 +
`check_unset_args` 4.

Four probes that did **not** reach their target — trace the caller rather than
guess the input: `flag_positional`, `dollar_at_pexp`, `is_valid_command`,
`resolve_on_path` (line numbers all shifted; re-derive from the new measure).

### Environment

**`/tmp` is cleaned aggressively on this host.** Always
`TMPDIR=~/sashscratch/tmp bash spec/run.sh` — `run.sh` uses `mktemp` under
`set -eu` with an EXIT trap, so a vanished TSV yields a *green partial* sweep.
Side build for probing while a coverage run holds `target/release`:
`cd sash && CARGO_TARGET_DIR=~/sashscratch/side cargo build --release -q`.

`mk cov-reset` leaves `target/release` **deleted**, so the next `mk build` is a
full from-scratch rebuild (~6 min), not incremental.

## ══ STATUS (2026-09-16) — dead code purged, one site kept ══

**Journal 334.** Sweep **5103 passing, 10 skipped, 0 failing** — exactly the
predicted total. For a purge, "nothing moved" *is* the result. Units 83 green.

### DG's instruction

> we have git, so deleting is reversible. we explicitly want to find and purge
> dead code. file a task to consider the use-site audit.

Task filed: `notes/tasks/audit-dd24-use-site-list.md` (P1) — the cross product
of DD#24's use-site list with the amendment's position list. **Filed "to
consider"; not started.**

### Purged — 5 branch sites, 3 whole functions

| site | why dead |
|---|---|
| `validate.rs` `is_any_arrvals_splat` | **function deleted** — merged into `splat_collection_name` at its one caller |
| `validate.rs` `is_bare_arrvals_splat` | **function deleted** — merged into `bare_arrvals_splat_name` |
| `splat_collection_name` DoubleQuoted arm | a quoted splat never reaches `validate` (below) |
| `bare_arrvals_splat_name` DoubleQuoted arm | same |
| `parser.rs` `parse_optional_return_type` `w == "->"` | the lexer always splits the arrow; `"->"` appears once in `sash/src` and it is this comparison |
| `parser.rs` `parse_negated` `\|\| …commands.len() > 1` | `dispatch_statement` never folds `\|`, so the guard cannot be true |

| `validate.rs` `bare_splat_source` | **function deleted** — body became byte-identical to `bare_arrvals_splat_name` once the dead arm went |

`cargo build` gave **no unused-function warning** — the check that each merge
was complete. The duplicate above surfaced only because clippy rejected a
`.then(|| …)` and a grep for the fixed spelling returned a hit I had not
written; `block_dupes` deserves a pass over `validate.rs`.

### The near-miss worth remembering

The first reason I had for deleting the four `DoubleQuoted` arms was **wrong**.
`"${a[@]}"` produces operator `arrvals_retired`, which *does* satisfy
`starts_with("arrvals")`. They are dead for a different reason: **SB4038 is
emitted at parse time** (`retired_bracket_operator_diag`, the only SB4038 site),
so an AST carrying `arrvals_retired` never reaches `validate`. Both facts are
now in the doc comment.

Two probes settle it: `"@a"` → SB5050 *"variable `a` is assigned but never
read"* (proves quoted `@a` is inert text); `"${a[@]}"` → SB4038 at line 2
(proves the parse-time rejection).

### Kept deliberately — `validate_arith_comma_in_condition`

**My earlier status entry calling this unreachable was wrong** — it came from
**stale line numbers** read against edited source. It is live; it fires SB5007
four times in the sweep. Its doc comment claiming an earlier fix "left this
duplicate cold" was false and is corrected.

Its mutation test's false arm *is* unreachable, but only because a comma list
with a mutation anywhere is decomposed upstream. Collapsing it would encode
"non-final parts are always wrong" in place of DD#31's "non-final parts must be
mutations" — a different statement about the language. Left cold, with the
reason in the source.

**The distinction to carry forward:** a branch dead because of an upstream pass
is not the same as a branch dead because nothing can construct its input. The
first kind stays, documented.

### Method correction

**Both-branches-zero does not mean unreachable.** `parse_negated`'s `||` read
that way and I had it on the purge list; `! ! /bin/true` then flipped
`1054|0|1`, proving the pattern matches and the guard runs. It was still
purgeable, but the argument had to change from "never evaluated" to "never
true, by `dispatch_statement`'s contract".

Verify statically → probe → delete. **A green sweep is not a proof** of
unreachability: the suite never exercised these paths, so it cannot testify
about them.

### Coverage loop — next

Branch coverage was **88.17% / 443 cold** at `73693de`. Stale now: `cov.219`–
`cov.222` landed after it, and this purge removes cold branches directly.
**Re-measure before mining** (`mk cov-reset` → `mk cov-branch` → `mk cov-db`,
~40 min) — a stale cold list makes `cov-probe.sh` answer "no branch records at
that line" and wastes a cycle.

`spec/coverage.sh` `plan=221`, next free id **`cov.223`**. `spec/taint.sh`
`plan=200`.

Verified-behaviour leads collected but not yet specced: `- /bin/true | /bin/cat`
compiles; `echo "$0"` → SB5017; `echo "${@}"`/`"${*}"` → SB5045 (different
message inside a function); `echo "$1"`/`"$#"` → SB5024; `foo/bar` and `./x` →
SB5051; `@a` → SB5032, `@{a:-x}` accepted; `${m["k"]}`, `${m['k']}`,
`${a[$((0))]}` accepted.

Four probes that did **not** reach their target — trace the caller rather than
guess the input: `flag_positional` 3496, `dollar_at_pexp` 3594,
`is_valid_command` 3066, `resolve_on_path` 3078.

Next files by cold count: `parse/lexer.rs` 35, `type_check.rs` 32,
`parse/arith.rs` 31, `parse/parser.rs` 28, `parse/expr.rs` 27, `emit_c.rs` 26,
`transform/control_flow.rs` 25.

### Environment

**`/tmp` is cleaned aggressively on this host.** Always
`TMPDIR=~/sashscratch/tmp bash spec/run.sh` — `run.sh` uses `mktemp` under
`set -eu` with an EXIT trap, so a vanished TSV yields a *green partial* sweep.
Side builds: `cd sash && CARGO_TARGET_DIR=~/sashscratch/side cargo build --release -q`.

`contrib/cov-probe.sh --build` hardcodes `target/release` and leaves the tree
**instrumented** — `mk cov-reset` + `mk build` before any behavioural test or
sweep. Confirm with: compile anything and check no `*.profraw` appears.

## ══ STATUS (2026-09-16) — `..` was the whole bug ══

**Journal 333. DD#114 accepted.** Sweep **5095 passing, 10 skipped, 0
failing** — exactly the predicted total. Units 83 green.

### Bug #13 — a tainted subscript index was not a read

DD#24's use-site list contains `arr[$x]=v` ("index expression"), and its
2026-08-26 amendment says the list is **position-uniform**. `validate` did not
implement it. All three taint walkers did this:

```rust
ExprKind::Subscript { name, .. } => push_taint_use(tainted, name, …)
```

`..` was the whole bug.

| position, tainted index | before | after |
|---|---|---|
| `/bin/echo "${a[$i]}"` | SSA backstop | SB5020 |
| `echo "${a[$i]}"` | SSA backstop | SB5020 |
| `a[$i]=v` | SSA backstop | SB5020 |
| **`s="${a[$i]}"`** | **accepted** | SB5020 |
| **`[[ -n ${a[$i]} ]]`** | **accepted** | SB5020 |

The last two are a laundering bypass — an attacker-chosen index selects a cell
and the result is treated as clean, no diagnostic anywhere. The rest were
rejected only by the SSA backstop, which is documented as defense-in-depth
behind an authoritative CST gate and whose message has no SB code, no
line:col, and an SSA-internal name (`i_1`). Reaching it is the symptom.

Fixed in `check_expr_taint`, `reject_scalar_rhs_read`, `expr_uses_tainted` and
`taint_assignment` (the write form). The diagnostic names the **index**
variable, not the container — with a clean `m` and tainted `k`, blaming `m`
sends the author to an `attest` that cannot help.

**Not decided, and not a fork:** whether a tainted index taints the retrieved
value. DD#24 has no propagation and the index read is rejected first, so the
question never arises.

### Bug #13b — every subscript index span was 1:1

The new diagnostic pointed at line 1 of a 3-line program.
`parse_subscript_index` re-parses the captured index text with a **fresh
`Scanner`**, and `Span` is `{line, col}` rather than a byte offset, so it
numbers from the start of the index substring. Every index expression the
compiler has built has carried a relative span — invisible until something
finally reported on one.

The index expression is re-anchored to the enclosing `${name[…]}` span. Exprs
nested *deeper* inside an index still carry relative spans; filed as
`notes/tasks/subscript-index-nested-span.md` (P3) with three options, because
the real fix is either a deep re-span or a `Scanner` that takes a base
position and that is worth a look at other substring re-parsers first.

Spec 60.68 pins the line number.

### Bug #12 (this session, earlier) — see journal 332, DD#113

`attest k v in m (( 1 ))` compiled clean and exec'd a bare `attest`. The
`(( … ))` predicate is scalar-only; a attest-all lead made `classify_attest`
return `None`, and every attest pass is `let Some(...) else { return }`.

### The pattern, now six for six

| DD | the text described | the code handled |
|----|--------------------|------------------|
| #72 | numeric literals are rejected | only in arithmetic context |
| #64 | line continuations are handled | two of three scanner contexts |
| #111 | value intrinsics are exempt inside `$( )` | any body, not just the rewritten one |
| #112 | the `arg` index is checked | the sign, not the digits |
| #113 | a non-scalar lead is "a different error" | no error at all |
| #114 | use-sites are **position-uniform** (DD#24) | the positions someone tried |

#114 is the first where the authoritative text is a **decision document**, not
a code comment. Worth auditing DD#24's use-site list against the code
directly — the same list names `join @arr`, arithmetic operands and function
arguments, and only the subscript index was checked this time.

### Method note

The sweep was **unchanged at 5071** by the bug #13 fix — not one existing spec
used a tainted subscript index. Coverage found this, but not through the
number: the cold branches being mined (`splat_collection_name`,
`ambiguous_index_diag`) are not the ones that were wrong. What found it was
being forced to enumerate the shapes that reach a cold branch and trying one
nobody had tried. **The cold list is a to-do list, not a detector.**

### Still open for DG (unchanged)

Three apparently-unreachable sites, reported not deleted: `validate.rs:1788`/
`1792`, `parser.rs:701`, `parser.rs:1054`.

Filed P2/P3s: `bug-brace-breaks-unquoted-word.md`,
`diag-negative-subscript-called-string-key.md`,
`subscript-index-nested-span.md`.

### Tooling note

**Run sweeps as `TMPDIR=~/sashscratch/tmp bash spec/run.sh`** — `/tmp` on this
host is cleaned aggressively and `run.sh`'s `mktemp` TSV can vanish mid-sweep,
producing a *green* partial load. Keep predicting the total; it is the only
check that catches a sweep which lost tests rather than failed them.

### Coverage this pass

Re-measured at `73693de`: **88.17% branch / 443 cold** (was 87.71% / 456).

`cov.219`–`cov.222` cover `reject_tainted_into_array`'s value walk — a `${x:-D}`
default holding the tainted read, the mirror where the *name* is clean and the
default is not, and both arms of the `Subscript` case DD#114 added. All four
probe-verified. Sweep **5103**, exactly predicted.

**Three more apparently-unreachable sites, reported not deleted** (joining the
earlier three):

- `validate.rs:756|0|1` and `763` — `splat_collection_name`'s guard-false and
  `DoubleQuoted` arms. It is only ever called behind `is_any_arrvals_splat`,
  which has already established the kind is a bare `@a` ParameterExpansion.
- `validate.rs:3884` — `bare_arrvals_splat_name`'s `DoubleQuoted` arm, same
  shape.

Note the *sibling* predicates are live: `is_any_arrvals_splat:747` runs
390/10 and `is_bare_arrvals_splat:3871` runs 318/92 on the same arm. The arm
appears to have been copied into the two *resolver* functions, where the
caller has already excluded the case. Their doc comments still advertise
`single-part "${a[@]}"`, a form DD#75 §4 retired.

Probes that did **not** reach their target (inputs rejected earlier in the
pipeline, route still unknown): `flag_positional` 3496 via `[[ -n $0 ]]`,
`dollar_at_pexp` 3594 via `echo "${@}"`, `is_valid_command` 3066 and
`resolve_on_path` 3078 via `foo/bar`. These need the caller traced rather than
the input guessed.

### Next

Continue the coverage loop.

Verified-behaviour leads already collected for `validate.rs`, not yet specced:

- `"${a[@]}"` quoted splat → SB4038 (`splat_collection_name` 756/763)
- `- /bin/true | /bin/cat` compiles (`stmt_is_external_command` 1018/1021)
- `echo "$0"` → SB5017 (`check_positionals_test` 3474)
- `echo "${@}"` → SB5045, and a *different* message inside a function
  (`dollar_at_expr` 3572)
- `echo "${*}"` → SB5045; `echo "$1"`/`"$#"` → SB5024
- `foo/bar` and `./x` → SB5051 (`is_valid_command` 3056)
- `@a` → SB5032, `@{a:-x}` accepted (`external_splat_diag` 3859/3862)
- `${m["k"]}`, `${m['k']}`, `${a[$((0))]}` accepted (`ambiguous_index_diag`
  7373)

Then by cold count: `parse/lexer.rs` 35, `type_check.rs` 32, `parse/arith.rs`
31, `emit_c.rs` 26, `transform/control_flow.rs` 25.

`spec/coverage.sh` `plan=217`, next free id `cov.219`. `spec/taint.sh`
`plan=200`.

---

## ══ PRIOR (2026-09-16) — the fifth time the comment was the bug ══

**Journal 332. DD#113 accepted, SB5054 widened.** Sweep **5071 passing, 10
skipped, 0 failing** — exactly the predicted total. Units 83 green. Branch
coverage re-measured at `2ce5a5c`: **456 → 444 cold**.

### Bug #12 — a attest-all with a `(( … ))` predicate exec'd a bare `attest`

```sash
map m=([k]=v)
attest k v in m (( 1 ))
/bin/echo ${m[k]}
```

compiled clean, then at runtime printed `sash: bug: bare command passed to
sash_exec (expected absolute path)` and exited 127. Same class as bug #9 and
bug #10, third distinct route in.

DD#93 Part D's `(( TARGET = EXPR ))` predicate is scalar-only —
`classify_attest_convert` requires a one-name lead. A attest-all lead makes
`classify_attest` return `None`, and *every* attest-specific pass is written
as `let Some(...) = classify_attest(...) else { return }`, so type check,
taint, transform and scope collection all declined in turn. What reached
codegen was a `Command` named `attest` with no resolution, and codegen emitted
an exec for it.

The parser lets it in: `parse_attest` parses the lead first, then accepts
`((` regardless of what the lead turned out to be — under a comment saying the
form is scalar-only.

Fixed in `validate`, where every other attest shape rule lives.
`attest_arith_near_miss` returns an enum now, so every lead arity reaches a
diagnostic. SB5054 with its own wording for the attest-all case — the scalar
message tells you to make `EXPR` read the attested name, which is not the
problem and not a fix anyone with a `k v in m` lead can apply.

The assignment spelling (`attest e in a (( t = e ))`) *was* rejected before,
but only incidentally — SB4020, "an assignment inside a non-statement
arithmetic position", which is true and says nothing about attest.

`spec/taint.sh` 93.49–93.54 (`plan=188`). 93.50 pins the *message*, not just
the code: merging the two SB5054 texts back together fails it.

### The tell, now five for five

| DD | comment described | code handled |
|----|-------------------|--------------|
| #72 | numeric literals are rejected | only in arithmetic context |
| #64 | line continuations are handled | two of three scanner contexts |
| #111 | value intrinsics are exempt inside `$( )` | any body, not just the rewritten one |
| #112 | the `arg` index is checked | the sign, not the digits |
| #113 | a non-scalar lead is "a different error" | no error at all |

Four of the five are specifically a **delegating comment** — "handled
elsewhere", "a different error", "caught by X" — sitting next to a `return
None` or an early exit. That is a claim about code you cannot see from where
you are standing, it was true when written, and nothing ages it. Worth
grepping for directly rather than waiting for coverage to walk past it.

### Still open for DG (unchanged)

Three apparently-unreachable sites, reported not deleted: `validate.rs:1788`/
`1792`, `parser.rs:701` (single `->` word token), `parser.rs:1054` (second
operand of `pipeline_operand`).

Two filed P2s: `notes/tasks/bug-brace-breaks-unquoted-word.md`,
`notes/tasks/diag-negative-subscript-called-string-key.md`.

### Tooling note — a sweep that lost tests and reported green

`run.sh` takes its scratch TSV from `mktemp`, and **`/tmp` on this host is
being cleaned aggressively**; `set -eu` plus the EXIT trap turns a vanished
TSV into a partial load with no complaint. One sweep reported 5001 passing / 0
failing having silently dropped an entire spec file.

**Run sweeps as `TMPDIR=~/sashscratch/tmp bash spec/run.sh`** and keep
predicting the total — AGENTS.md frames prediction as a collision check, but
it is also the only thing that catches a sweep which *lost* tests rather than
failed them.

### Next

Continue the coverage loop. `validate.rs` is still the largest block; 1849/
1853 are closed by this fix. Remaining unchased `validate.rs` leads:
`splat_collection_name` 756/763 (quoted `"${a[@]}"` splat → SB4038),
`stmt_is_external_command` 1018/1021 (`- /bin/true | /bin/cat` compiles),
`mark_predicate_operand` 2578/2582, `resolve_on_path` 3056, `flag_positional`
3474 (`$0` → SB5017), `dollar_at_pexp` 3572 (`"${@}"`/`"${*}"` → SB5045, and a
second message inside a function), `bare_arrvals_splat_name` 3859/3862,
`check_slot_arity` 6119/6131, `index_is_identifier` 7373.

Then by cold count: `parse/lexer.rs` 35, `type_check.rs` 32, `parse/arith.rs`
31, `emit_c.rs` 26, `transform/control_flow.rs` 25.

`spec/coverage.sh` `plan=217`, next free id `cov.219`.

---

## ══ PRIOR (2026-09-16) — the typo guard that guessed ══

**Journal 331. DD#112 accepted, SB5078 added.** Sweep **5059 passing, 10
skipped, 0 failing**. Branch coverage **87.71% / 456 cold** at the last
measure (`5cbe9b7`), before the 23 specs added since.

### Bug #11 — a malformed `arg` index silently read a different argument

The runtime parses `$(arg N)`'s index by taking leading digits and discarding
the rest, and nothing rejected a literal that was not a number:

| written | actually read | exit |
|---|---|---|
| `$(arg abc)` | argument 0 — the **program name** | 0 |
| `$(arg -)` / `$(arg -x)` | argument 0 | 0 |
| `$(arg 1x)` | argument 1 | 0 |
| `$(arg 2abc)` | argument 2 | 0 |
| `$(arg +1)` | argument 1 | 0 |

`$(arg 1x)` is the dangerous one — it looks like it worked, and a typo in the
second digit changes which argument you get with no signal at all.

Not a fork: spec `av.t6` already pins the governing intent for the
neighbouring case — "`$(arg N)` for N >= argc aborts at runtime (no silent
empty)". An out-of-range index refuses to guess; a malformed one was guessing.

`arg_index_diag` already had a typo guard for *negative* literals (DD#60). It
had none for "is this a number at all". SB5078 sits beside it, ordered after
so `-1` keeps SB5038, and restricted to literals so `$(arg $i)` stays a
runtime concern — the literal/computed split the existing comment already
draws.

`spec/argv.sh` `av.t8`–`av.t8i` (`plan=57`). `av.t8g` pins the ordering;
`av.t8h`/`av.t8i` pin what must not change (`01` is a fine index, a computed
one is untouched).

**Fifth bug this session with the same shape:** a rule implemented for the
spelling someone reported and left open for the rest of its class, living
*next to* an existing check that handles the neighbouring case. DD#72 (arith
context only), DD#64 (two of three scanner contexts, then not at EOF),
`bareword_shortcut_expansion` (single-statement bodies only), and now an index
guard that checked the sign but not the digits.

### Filed, not fixed

`notes/tasks/diag-negative-subscript-called-string-key.md` (P2) — `${a[-1]}`
is rejected correctly but reported as a *string key* with the advice "use an
integer". `is_string_key` calls a literal a string when it is not all digits,
so a leading `-` qualifies. Behavior is right and memory-safe (a negative
subscript is a distinct key, not an offset — `-100000` harms nothing); only
the explanation is wrong.

### Coverage this pass

`cov.207`–`cov.216` went after `parse/parser.rs` (37 cold, the second-largest
block); nine are probe-verified flips — empty `elif` condition (486), dispatch
as a `while` condition (519), `for x in a` at end-of-input (560), a bare word
where `-> int` would go (705), dispatch with a unary subject (1287) and with
`!=` (1294), the attest-all binder list (1779), `<&` with a non-fd target
(2616).

`cov.217`/`cov.218` cover `check_cmdsub_value_intrinsic`, the function added
for bug #10 — which should have been covered when it was written.

`coverage.sh` `plan=217`.

**For DG — three sites that look genuinely unreachable.** Reported, not
deleted:

- `validate.rs:1788`/`1792` — may have been made unreachable by bug #6's own
  fix.
- `parser.rs:701` — `parse_optional_return_type` accepts both a single `->`
  word token and the split `-` `>` pair, "depending on how the lexer
  classified the bytes". Every spelling tried (`-> int`, `->int`, `() ->int`)
  takes the split path.
- `parser.rs:1054` — the second operand of `pipeline_operand`, an
  "already-folded multi-stage pipeline". `!` parses its operand with
  `dispatch_statement`, which by contract does not fold `|`. The first operand
  covers every reachable case and is pinned by cov.213.

### Tooling note — a sweep that lost tests and reported green

`run.sh` takes its scratch TSV from `mktemp`. **`/tmp` on this host is being
cleaned aggressively**, and `set -eu` plus the EXIT trap turns a vanished TSV
into a partial load with no complaint: one sweep reported 5001 passing / 0
failing having silently dropped an entire spec file.

Only predicting the total caught it. **Run sweeps as
`TMPDIR=~/sashscratch/tmp bash spec/run.sh`** until this is addressed, and
keep predicting the total — AGENTS.md frames that as a collision check, but it
is also the only thing that catches a sweep which *lost* tests rather than
failed them.

### Next

Re-measure (`mk cov-reset` → `mk cov-branch` → `mk cov-db`, ~40 min) — 23 new
specs since the last one. Then continue: `validate.rs` 61 cold,
`parse/lexer.rs` 35, `type_check.rs` 32, `parse/arith.rs` 31, `emit_c.rs` 26,
`transform/control_flow.rs` 25.

Unchased `validate.rs` leads: `splat_collection_name` 756/763,
`stmt_is_external_command` 1018/1021, `attest_arith_near_miss` 1849/1853,
`mark_predicate_operand` 2578/2582, `resolve_on_path` 3056, `flag_positional`
3474, `dollar_at_pexp` 3572, `bare_arrvals_splat_name` 3859/3862,
`check_slot_arity` 6119/6131, `index_is_identifier` 7373.

---

## ══ PRIOR (2026-09-16) — an exemption wider than its justification ══

**Journal 330. DD#111 accepted.** Sweep **5017 passing, 10 skipped, 0
failing** (was 4983). Bug #10 closed; nine coverage specs landed alongside.

### Bug #10 — a value intrinsic mid-`$( )` leaked an internal assertion

```sh
/bin/echo "$(echo hi; status)"
```

compiled clean and printed `sash: bug: bare command passed to sash_exec
(expected absolute path)` into the **program's own output**, exit 0.

`bareword_shortcut_expansion` is what gives `$(status)` a meaning — it
recognises the substitution and rewrites it into the value the intrinsic
produces. It requires the body to be exactly one statement. The validator
meanwhile exempted value intrinsics from SB5073 *anywhere* inside `$( )`.
**The exemption was wider than the rewrite that justified it.** Two
statements and nothing rewrote the intrinsic; it was dispatched as an
external and aborted inside `sash_exec`.

That failure is described verbatim in `bareword_shortcut_expansion`'s own doc
comment, by whoever fixed it the first time — for the single-statement case
only. Third instance this session of a fix aimed at a reported spelling
instead of at the rule (DD#72 numeric literals, DD#64 line continuation, now
this).

**DG ruled: narrow the exemption**, not widen the rewrite. The deciding fact
was that `$(echo hi; echo $?)` already prints `hi`/`0` correctly today —
widening would have bought a second spelling for something that already had
one, and given `status` a statement-position meaning the language says it
does not have.

Same code, SB5073, new wording: the statement-position message ("only valid
inside a command substitution") is misleading for a use that *is* inside one.

`spec/commands.sh` 23.25–23.32, placed directly under 23.24, which pins the
exemption and still passes. `plan=105`.

**Framing correction from DG, worth keeping:** `status` is the intrinsic;
`$?` is punctuation sugar that reads it. Not the other way round. Variables
are not magic in sash — they only look magic where they are sugar for an
intrinsic. SB5012's message already said this and I still inverted it.

### Coverage, this pass

`cov.198`–`cov.206` in `spec/coverage.sh` (`plan=205`), each probe-verified to
flip a cold branch in `parse/expr.rs`: `${#}`/`${##}` (794), `${s@Uu}` (1254),
a bare `${a[@]}` splat (1046), the `[key]=` map-literal arms (1719), and the
three command-sub guards at 678/679/685 via `"$()"`, `"$(int x=1)"` and
`"$($s)"`. The quoted spellings are load-bearing — an unquoted `$( )` is
rewritten by `try_special_word` and never reaches `command_sub_to_expr`.

`cov.204` uses `"$()"` deliberately: an empty body flips branch 678 without
encoding the bug that probing it uncovered.

### A partial sweep that looked green

The first sweep of this change reported 5001 passing / 0 failing and was
**wrong** — `run.sh` has `set -eu` and a trap that removes the TSV, so a sweep
that dies partway silently records only the files it reached. `commands.sh`
was missing entirely. Nothing in the output said so.

Caught only by predicting the total first: 4983 + 9×2 + 8×2 = 5017, not 5001.
Re-running gave exactly 5017. **Keep predicting the total — it is the only
check that catches a sweep which lost tests rather than failed them.**

### Coverage, after the re-measure

**87.71% branch / 456 cold** at `5cbe9b7` (was 87.33% / 475 at the start of
the previous pass). `parse/expr.rs` 35 -> 27.

`cov.207`-`cov.216` then went after `parse/parser.rs`, the second-largest cold
block (37). Ten specs, nine of which are probe-verified to flip a specific
cold branch: the empty-`elif` condition (486), the dispatch form used as a
`while` condition (519), `for x in a` running into end-of-input (560), a bare
word where `-> int` would go (705), dispatch with a unary subject (1287) and
with `!=` (1294), the attest-all binder list (1779), and `<&` with a non-fd
target (2616). The tenth pins SB4040 for `! a | b`.

Needs a re-measure
(`mk cov-reset` → `mk cov-branch` → `mk cov-db`, ~40 min).

Cold leaders then: `validate.rs` 59, `parse/parser.rs` 37, `parse/expr.rs` 35,
`parse/lexer.rs` 35, `type_check.rs` 32, `parse/arith.rs` 31, `emit_c.rs` 26,
`transform/control_flow.rs` 25.

Open `expr.rs` leads not yet chased: 121/122/123 (Amp/AndAnd/OrOr guards
outside Assign absorb mode), 580 + 591 + 634/635 (`try_split_subscript_arm`,
`dynamic_subscript_var`), 942, 960|1|2, 1231, 1259/1260, 1390, 1453, 1473,
1609/1610. `lexer.rs:936` (`skip_paren_to_close`) still needs a route in.

**For DG — three sites that look genuinely unreachable.** Reported, not
deleted:

- `validate.rs:1788`/`1792` — may have been made unreachable by bug #6's own
  fix.
- `parser.rs:701` — `parse_optional_return_type` accepts both a single `->`
  word token and the split `-` `>` pair, "depending on how the lexer
  classified the bytes". Every spelling tried (`-> int`, `->int`, `() ->int`)
  takes the split path; no input found that produces a single `->` token.
- `parser.rs:1054` — the second operand of `pipeline_operand`, an
  "already-folded multi-stage pipeline". `!` parses its operand with
  `dispatch_statement`, which by contract does not fold `|`, so the statement
  can never be a multi-command pipeline. The first operand (a trailing `|`)
  covers every reachable case and is now pinned by cov.213.

---

## ══ PRIOR (2026-09-16) — I wrote a spec that pinned a bug ══

**Journal 329. DD#64 amended twice.** Sweep **4983 passing, 10 skipped, 0
failing**. Branch coverage **87.33% / 475 cold** at the start of this pass
(was 86.76% / 495). Three lexer bugs closed today.

### Bug #9 — a trailing `\` at end-of-input, and the spec that hid it

`/bin/echo a\` with no final newline: bash prints `a`, sash printed `a\`.
Same rule as bug #8 with the newline missing — DD#64's "end-of-line" covers
the last line whether or not it is newline-terminated. Both `scan_word_chars`
and `skip_whitespace` now treat `\`-then-newline and `\`-then-nothing alike.

**I introduced the spec that would have hidden it.** 64.15, added in the bug
#8 commit, asserted `out = 'a\'` — written from the implementation, never
compared to bash, and it passed. Found an hour later while probing
`lexer.rs:297`, where the same shape one byte over (`/bin/echo a \`) got a
bash comparison out of habit.

The failure mode is worth naming: **a spec written after the verification,
from the implementation, asserts what the implementation does.** The 328
verification table did run bash on a case labelled "trailing at EOF" — but
that input carried a newline, because the helper appended one. The no-newline
variant only appeared when I wrote the spec, by which point I had stopped
comparing.

Dividend: 64.15 no longer needs its native-only guard. It needed one because
the two harnesses disagreed about the answer, and they disagreed *because* the
answer was wrong. Skips 11 → 10. A spec that needs a mode guard is sometimes
telling you the behavior is wrong, not that the harnesses differ.

`spec/wordsplit.sh` 64.9–64.18.

### Coverage, this pass

Six lexer branches closed: `<<-EOF` (cov.193), unterminated backtick and bare
backtick-before-newline (cov.194/195), `<<` at end-of-input (cov.196), a `\`
as the last byte of a backtick body (cov.197), and both continuation branches
via 64.16–64.18. `plan=196` in coverage.sh, `plan=133` in wordsplit.sh.

Cold-branch leaders after the last full measure: `validate.rs` 59 (was 74),
`parse/lexer.rs` 43, `parse/parser.rs` 37, `parse/expr.rs` 35, `type_check.rs`
32, `parse/arith.rs` 31, `emit_c.rs` 26.

### Next

**Re-measure before probing** — `coverage.db` predates cov.193–197 and
64.9–64.18, and two lexer edits shift line numbers.

Probed without success, still open: `lexer.rs:936` (`skip_paren_to_close`
never ran for `/bin/echo $(abc` — needs a different route in).
`lexer.rs:272|0|1` suggests `next()` is never called with an empty peek
buffer; defensive, do not delete.

Also to report: `validate.rs:1788`/`1792` may have been made unreachable by
bug #6's own fix. Do not delete unilaterally.

### Open for DG

`notes/tasks/bug-brace-breaks-unquoted-word.md` — `/bin/echo a}b` is a compile
error where bash prints `a}b`. Same shape as bugs #7–#9 but the fix is a
language question (three options, possible collision with brace expansion).

---

## PRIOR

## ══ PRIOR (2026-09-16) — the same bug, one byte over ══

**Journal 328. DD#64 amended.** Sweep **4966 passing, 11 skipped, 0 failing**.
Two lexer bugs closed today; branch-coverage loop resuming.

### Bug #8 — `\<newline>` did not continue a bare word, silently

```sh
/bin/echo start\
/bin/echo second
```

bash prints `start/bin/echo second`. sash compiled clean and printed two
lines. Same function as bug #7, same shape: a byte `scan_word_chars` handled
wrong. It had no `\` arm at all, so the backslash went through as a verbatim
byte and the newline then ended the statement.

**Not a fork.** DD#64 already says `\` at EOL is a line continuation and
"Kept (structural)"; `skip_whitespace` and `scan_dq_escape` both implemented
it. `scan_word_chars` was the one context of three that did not. DD#64's
"Already compliant (no change needed)" paragraph now records that it was right
about `a\b` and wrong about `a\<newline>`. The non-EOL divergence stays
deliberate: `a\b` is `a\b` here, `ab` in bash.

`spec/wordsplit.sh` 64.9–64.15. 64.15 is native-only — `_run_sash_c` writes
source with `printf '%s\n'`, so a trailing `\` gains a newline and becomes a
continuation; unreachable through that harness, not different. Skips 10 → 11.

**Method note:** my first probe errored (`command 'b' not found`) and I nearly
filed this as cosmetic. A divergence is only loud if it is loud for *every*
input — one more probe found the silent case.

### Next

Resume the branch-coverage loop. `coverage.db` is stale as of `6b5b7be`
(86.76%, 495 cold): 46 specs have landed since and two lexer edits shift
`parse/lexer.rs` line numbers. **Re-measure before probing** — a `mk cov-branch`
was started and killed mid-run for exactly this reason.

Still unspecced from the pass before last: `lexer.rs:510` b1/br2 (`<<-EOF`),
`645` b0/br1 (unterminated backtick), `664` b0/br1 (backtick then newline).
`285` b1/br2 (line continuation) is now covered by 64.9–64.11.

Also to report: `validate.rs:1788`/`1792` may have been made unreachable by
bug #6's own fix. Do not delete unilaterally.

### Open for DG

`notes/tasks/bug-brace-breaks-unquoted-word.md` — `/bin/echo a}b` is a compile
error where bash prints `a}b`. Same shape as bugs #7/#8 but the fix is a
language question (three options, possible collision with brace expansion).

---

## PRIOR

## ══ PRIOR (2026-09-16) — half a fix, scoped to the place it was found ══

**Journal 327. DD#110.** Sweep **4953 passing, 10 skipped, 0 failing**
(+30 this pass). Branch-coverage loop, paused for a bug, now resuming.

### Bug #7 — a `#` truncated every unquoted word, silently

`/bin/echo a#b` printed `a`. `/bin/echo 16#ff` printed `16`.
`/bin/echo ab#cd ef` ate the rest of the line. No diagnostic.

Found from the coverage loop: `parse/lexer.rs:432` is DD#72's guard, and its
own comment says it prevents exactly this truncation — but it is scoped to
`arith_depth > 0`. The 2026-07-05 fix was scoped to the place the bug was
found, not to the rule it violated. DD#72 now records that.

**DD#110 (DG's rule): a `#` opens a comment only after a blank**, or at the
start of the source. Simpler than bash's token-boundary rule, and strictly
narrower — it diverges only in the safe direction. It also needs no scanner
state (it reads `source[pos-1]`), which is why it gets the expansion-adjacent
cases right where a bash-faithful emulation does not: `$(…)` closes on an
`RParen`, so a "previous token was word-like" flag misclassifies `$(p)#z`.

**SB4058** makes the one divergence loud. bash comments after an operator
(`echo a;# c` → `a`); sash reads the `#` as a word, which would have surfaced
as a baffling `SB5011 command '#' not found`. The scanner records the first
`#` glued to `; & | < > (` and `parse` reports it first and alone — same
precedent as `unterminated`, since once tokenization is wrong every later
diagnostic is a consequence. `)` and `}` are deliberately out of the set:
they close an expansion far more often (`${s}#y`, `$(p)#z`).

`spec/comments.sh` cmt.12–cmt.26. DD#72's SB4035 for `(( n = 16#ff ))` is
unaffected — the arith guard runs first (cmt.26 pins it).

### Found on the way, filed not fixed

`/bin/echo a}b` is a compile error; bash prints `a}b`. No `#` involved — `}`
is in `scan_word_chars`' break set the same way `#` was. Not silent (it is a
hard error, so no binary disagrees with bash), so P2 not P1, but `a{b`
reports "expected `}`" which sends the author looking for a brace that was
never opened. Three real options and a possible collision with brace
expansion — **DG's call**: `notes/tasks/bug-brace-breaks-unquoted-word.md`.

### Next

Resume the branch-coverage loop. `coverage.db` is stale as of `6b5b7be`
(86.76%, 495 cold) — 31 specs have landed since and the lexer edit shifts
`parse/lexer.rs` line numbers past ~394, so re-measure before probing.

Flipped but never specced, from the last pass — write these first:
`lexer.rs:285` b1/br2 (line continuation), `510` b1/br2 (`<<-EOF`), `645`
b0/br1 (unterminated backtick), `664` b0/br1 (backtick then newline).

Also to report: `validate.rs:1788`/`1792` may have been made unreachable by
bug #6's own fix. Do not delete unilaterally.

---

## PRIOR

## ══ PRIOR (2026-09-15) — a rule enforced twice is enforced once ══

**Journal 326.** Sweep **4891 passing, 10 skipped, 0 failing** (+18 this pass).
Branch-coverage loop, continuing.

### Two dead `PreIncDec` arms removed from `validate.rs`

`validate_arith_context`'s SB5007 "prefix ++x is not allowed" and
`arith_brief`'s prefix spelling. Cold across eight and two spellings, and
structurally unreachable two independent ways: `reject_prefix_incdec` fires in
**both** parse entry points (`parse_arith_slot` for `(( … ))` and all three
`for` slots, `parse_arith_expansion` for `$(( … ))`), and a top-level
`PreIncDec` classifies as a mutation so it becomes an `Assignment`, never the
`ArithTest` that is the validator's only caller.

That leaves `reject_prefix_incdec` as the single enforcement point for a DD#31
clause — which is the right shape. A check no input can reach enforces
nothing; what defends the rule is a spec. All four parse entry points are now
pinned: `cov.23`, `cov.163`–`165`, and `cov.176` (new, `$(( ++x ))`).

Nine more `PreIncDec` sites across `ir.rs`, `parse/`, `transform/`,
`emit_ir.rs` deliberately **untouched** — filed as
`audit-downstream-preincdec-arms` (P4), one at a time with a probe, not a
batch purge. Some are certainly live.

### Seven more branches closed, none of them bugs

`for x in $@` / `$*` (SB5045); `for x in a[bc]d` (SB5006) and its two
half-guards `for x in abc` / `for x in a[bc`; `for w in pre$s*post` (the
`Concat` arm of the glob scan); and both operands of the ternary check —
`if (( 1 ? (x=2) : 3 ))` **and** `if (( 1 ? 3 : (x=2) ))`.

The ternary pair is the lesson: the two arms are the two operands of one
`||`, so a spec that covers "the ternary case" reaches exactly half of it.
Short-circuit operators hide a branch behind a branch.

Specs `cov.168`–`cov.176`.

### Next

**The branch table is stale.** Three commits have moved `validate.rs` line
numbers (`7a63e68` and this one), so `coverage.db` offsets no longer line up —
`contrib/cov-probe.sh` needs current line numbers, looked up by content in
`.indexnudge-rust.db`. A full `mk cov-branch` re-measure is the next step;
last authoritative figure was **86.35%, 512/3752** at `b9ca092`.

Still-cold leads carried forward: `splat_collection_name`'s `DoubleQuoted`
arm (both halves — `"@a"` is not a splat, it is SB5050 dead-variable, so the
arm may be unreachable); `first_read_span`'s `found.is_none()` false arm;
`program.end_block` in the predicate-function scan.

## ══ PRIOR (2026-09-15) — the slot that skipped the desugaring ══

**Journal 325.** Sweep **4873 passing, 10 skipped, 0 failing**. Bug #6, and
the sixth time a cold branch was a bug.

### The C-style `for` test slot was the one condition context of four

`validate.rs:1755` block 0 branch 0 — the `Comma` arm of
`validate_arith_in_condition` — was cold. Only a C-style `for` header reached
it, and that was the finding: the other three contexts desugar the comma
before validation, and the `for` header did not.

`lower_c_style_for` lowered its test slot with `lower_arith_test_structured`,
bypassing `lower_arith_command` and so the `Comma` → `Group` split. The
mutation stayed embedded in one `ArithTest`, where
`check_arith_test_placement` rejects it.

| | sash (before) | bash |
|---|---|---|
| `while (( k++, k<3 ))` | `1 2` | `1 2` |
| `if (( k++, k<3 ))` | `1` | `1` |
| `until (( k++, k>2 ))` | `1 2` | `1 2` |
| `for (( ; k++, k<3; ))` | **SB4020** | `1 2` |

The header's other slots are not a substitute: `for (( ; k<3; k++ ))` prints
`0 1 2`, not `1 2`. **Fix: route the slot through `lower_arith_command`.**
Measured cost, old binary vs patched: one message changes
(`for (( ; k<3, k++; ))` goes from three diagnostics to one, still SB5007 and
now the same one `while` gives). Every other shape is byte-identical.

### DG ruled: delete the check the fix made dead

A comma holding a mutation classifies as `Mutation` and decomposes, so the
comma arm now only sees mutation-free commas — making
`validate_arith_comma_in_condition`'s final-part check unreachable (verified
across five spellings post-fix). **The DD#31 clause it enforced is still
enforced**, by `validate_assignment_stmt` on the decomposed form, for all four
contexts. It was a `for`-only duplicate that the bug kept warm. The surviving
half — every non-final part must be a mutation — has no other route, because
after decomposition there is no comma left to look at.

DD#31 amended: "condition position" names all four contexts, and enforcement
is explicitly split across the desugaring.

Specs `31.53`–`31.58` pin the whole matrix.

### Next

Still on the branch-coverage loop. Confirmed-and-unresolved from this pass:
**`validate.rs:1729` — the `ArithExpr::PreIncDec` arm of
`validate_arith_context` looks dead**; `reject_prefix_incdec` in the parser
fires first (SB4020) on every spelling tried. Not yet proven, do not delete on
this evidence alone. Flipped and awaiting specs: `validate.rs:1161` both arms
(`for x in $@` / `for x in $*` → SB5045).

## ══ PRIOR (2026-09-15) — the operand changes kind when you move it ══

**Journal 324.** Sweep **4861 passing, 10 skipped, 0 failing** (+8 specs,
native-only — `convert.sh` runs in one mode). 27 `sash-convert` tests green.

### `convert-prefix-incdec` — done, DG chose (a)

DD#31 forbids prefix `++`/`--`; `sash-convert` passed it through verbatim in
every context. DG's ruling: **rewrite where the rewrite is sound, `CHECK:`
where it is not.** The line is whether anything reads the value.

| context | disposition |
|---|---|
| `(( ++j ))`, `(( x++, ++k ))` | rewrite to postfix |
| `for (( ++i; …; ++i ))` — `initializer`/`update` | rewrite to postfix |
| `for (( ; ++i < 3; ))` — `condition` | `CHECK:` |
| `if (( ++i ))`, `while (( --n ))` | `CHECK:` |
| `$(( ++i ))`, `(( y = ++i ))` | `CHECK:` |

`(( ++i ))` as a statement and as an `if` condition are **the same node with
the same children** — only the parent's field name tells them apart.

**The bug inside the fix:** the first cut did nothing at all to a `for`
header. tree-sitter-bash yields the operand as a `variable_name` inside
`(( … ))` but as a **`word`** inside a `for` header, so the lookup returned
`None` and the function returned early — no patch, no `CHECK:`, no
diagnostic. An early `return` on a `None` looks exactly like having handled
the case. Specs `57.156`–`57.163` pin all of it.

### Next

Back on the branch-coverage loop. Last full baseline **86.35%, 512/3752** at
`b9ca092` — stale now (the for-header fix removes three branches, and the
converter work adds none to `sash/`). Fattest remaining target is
`validate.rs` at 85 cold branches.

## ══ PRIOR (2026-09-15) — three cold branches, one silent infinite loop ══

**Journal 323.** Sweep **4853 passing, 10 skipped, 0 failing** (+16: 8 new
specs × 2 modes). 36 sash unit tests. DD#25 amendment ratified by DG; back on
the branch-coverage loop.

Fresh branch baseline at `b9ca092`: **86.35%, 512 uncovered** of 3752 — up
from 510 only because SB5077 added 40 new branches, most immediately covered.

### `lower_c_style_for` threw away every parse error

Three rows of the branch table — `parse/arith.rs` 295, 304, 314, all the
`Err` arm — were the three `if let Ok(...)` slots of the C-style `for`
header. Discarding those errors did not merely lose a diagnostic, it **built a
different program**:

| source | before | bash |
|---|---|---|
| `for (( i=0; i <* 3; i++ ))` | compiled → **infinite loop** | syntax error |
| `for (( i=0; i < 3; i +*+ ))` | compiled → **infinite loop** | syntax error |
| `for (( i=*0; i < 3; i++ ))` | compiled → silently wrong start | syntax error |
| `(( i <* 3 ))` — same garbage | SB4020 | — |

A malformed condition fell back to `true_cmd`. **Fifth time in this campaign
that the cold branch WAS the bug.**

### DD#31 has no for-header exemption

Reached by the other route — checking whether `ArithExpr::PreIncDec` was dead
before declaring it so. It is not: `for (( i=0; i < 3; ++i ))` and
`for (( ++i; ... ))` **compiled and ran**, while the condition slot gave
SB5007 for an unrelated reason and every other context gives SB4020. Three
slots, three dispositions, one construct. DD#31 says *"Only postfix `x++` and
`x--`. No prefix forms."* and names no exemption, so this was not a carve-out
(checked `notes/decisions/` first, as the rule requires).

Same root cause: `lower_c_style_for` was the one arithmetic entry point that
skipped the shared checks. Fixed with one `parse_arith_slot` (parse +
`reject_prefix_incdec` + `reject_nested_mutation`) now used by
`parse_arith_command` and all three `for` slots; `lower_c_style_for` returns
`Result`. An empty slot is still `true` — `for (( ;; ))` is deliberate, and is
now the *only* thing that can become `true`.

Specs `cov.160-167`, plan 158 → 166.

### OPEN FOR DG

- **`sash-convert` emits prefix `++`/`--` verbatim in every context**, so it
  produces sash that does not compile. **Pre-existing and general, not a
  regression** — `$(( ++i ))` and `(( ++i ))` have always been SB4020; the
  for-header case merely used to be accepted by the bug fixed above. Filed as
  `convert-prefix-incdec` (P2). Not a pure rewrite: prefix in a *value*
  context has no postfix equivalent in sash, so the options are (a) rewrite
  where sound + `CHECK:` where not, (b) `CHECK:` everywhere, (c) abort with a
  message. **Converter policy — DG's call**, same family as DD#57.
- `janet-arith-divzero` — deferred at DG's word ("TBD much later").
- Still carried: the `[[ … ]]` exit-status rule is **provisional**. DG floated
  reserving non-zero exits for `exit` and aborts, which would flip `[[ ]]`,
  `(( ))` and external-command guards together; the four `e.b70.tandf*` specs
  are written so that flip must move them as a set.

### Known-but-left-alone

`reject_prefix_incdec` does not recurse into Ternary, UnaryOp, FuncCall or
Subscript (`_ => Ok(())`). The hole is real but **shadowed** — a second,
broader check catches embedded mutations first, so `$(( 1 ? ++x : 3 ))` and
`$(( a[++i] ))` are already errors. Left as-is rather than changing which of
two correct diagnostics fires.


## ══ PRIOR (2026-09-15) — one fault, two answers ══

**Journal 322.** Sweep **4837 passing, 10 skipped, 0 failing** (+20 over
journal 321: `cov.150-156` from the tail of that cycle, plus 3 new specs × 2
modes here). 36 sash unit tests.

DG's standing instruction is unchanged: *"we're targeting 100% branch
coverage. Add spec tests and remeasure in a loop until we reach full coverage
modulo unit tested regression guards."* This cycle interrupted that loop to
close the div-by-zero question that had been sitting in OPEN FOR DG.

### Every arithmetic fault now aborts (DD#25 amended)

`emit_div_or_mod_with_zero_check` disposed of x86's `#DE` **two opposite ways
fifteen lines apart in one function**: `rhs == 0` wrote bash's `division by 0`
to stderr, yielded `0` and continued; `i64::MIN / -1` — the other cause of the
same hardware exception — called `sash_arith_overflow` and exited 2. DG found
the seam by asking whether we were adding an explicit check. We were, and the
check was why the trap never fired.

Measured gcc before proposing, since the old behaviour had been justified as
bash-matching: a literal `1/0` makes gcc emit **`ud2` as the entire body of
`main`** (SIGILL, rc 132); a runtime zero is a raw `idiv` → SIGFPE (rc 136). C
never wraps, never guards, never yields a value — **sash was the outlier in
the opposite direction from the one I assumed.**

DG's call, now implemented in both backends:

- **Runtime** — `sash_arith_divzero` → `<argv0>: division by zero`, exit 2.
  The explicit check is **deliberate, not a fallback**: bare SIGFPE loses
  `argv0`, says "floating point" for an integer divide, and would push
  `i64::MIN / -1` back out of the message path. That reasoning is a comment in
  both backends.
- **Compile time** — **SB5077** when the folder can prove the divisor is zero
  (`$(( 1 / 0 ))`, `$(( 1 + 8 / (4 - 4) ))`).

The two are **complementary, not redundant** — I corrected my own framing on
this. There is no const-propagation through variables, so `int z = 0;
$(( 1 / z ))` is not provable and still reaches the runtime. Neither path can
be removed.

`check_arith_div_by_zero` calls the transform's own `fold_arith_op` rather
than reimplementing constant evaluation, so the validator cannot drift from
the folder about what "provable" means; only the tree walk is local, and that
walk is the weaker half by design. It is wired into **both** the `$(( … ))`
value path and the `(( … ))` statement path.

Closed on the way past: *"could any `--emit-c` path emit a literal `/0`?"* is
now closed by construction (SB5077 rejects before the C backend runs), which
given gcc's `ud2` mattered. DD#25's "known gap" list was also stale — it named
`Div` (`i64::MIN / -1`) as not-yet-trap-with-message when that was already
implemented; verified and struck.

### OPEN FOR DG (resolved)

- **RATIFIED by DG.** The SB5077 walk
  descends into ternary arms, so `$(( 1 ? 5 : 10/0 ))` is rejected even though
  that arm never evaluates (bash prints 5). This is the only place the check
  is *stronger* than the folder. I kept it because sash already rejects dead
  code as an error — SB5050 refuses a variable assigned and never read — and a
  literal fault in an unreachable branch is the same family. Pinned at
  `cov.159`, so a reversal moves one spec.
- **Janet's disposition for a zero divisor is deferred** at DG's word ("TBD
  much later") — filed as `janet-arith-divzero`. The SB5077 half already
  applies to janet (it fires in `validate.rs`, before any backend); only the
  runtime half is open.
- Still carried: the `[[ … ]]` exit-status rule from two cycles ago is
  **provisional**. DG floated reserving non-zero exits for `exit` and aborts,
  which would flip `[[ ]]`, `(( ))` and external-command guards together; the
  four `e.b70.tandf*` specs are written so that flip must move them as a set.

### Next

Back to the coverage loop. Branch coverage last measured **86.27%, 510
uncovered** at `c5df3b7`; `cov.150-159` have landed since and are not yet
reflected. Use `contrib/cov-probe.sh` (0.47s per miss) for per-miss
verification before batching a full `mk cov-branch` re-measure.


## ══ PRIOR (2026-09-15) — three bugs behind three cold branches ══

**Journal 321.** Sweep **4817 passing, 10 skipped, 0 failing** (+114 over
journal 320: 38 new specs × 2 modes, plus the 28 from the prior cycle). 36
sash unit tests. Pushed through `c5df3b7`.

DG: *"we're targeting 100% branch coverage. Add spec tests and remeasure in a
loop until we reach full coverage modulo unit tested regression guards."*

The cycle produced **three bugs, and in each case the cold branch WAS the
bug**. None were diagnosable from the counts — every classification took a
probe. The working signal is not bash divergence (DG: safebash has **no**
bash-identity goal) but disagreement between two of sash's *own* spellings of
one idea.

- **DD#76 int cells faulted on every positional read** (`621cb0c`). An int
  element is `{val_ptr: null, val_len: <i64>}`. `${a[0]}` worked;
  `$(join a)` **segfaulted** and `for x in @a` gave EFAULT, int maps too.
  Keyed reads go through `sash_table_get` (codegen knows the static type);
  splat/join/`for`-in go through `sash_table_value`, which has none and
  handed the null out. Fixed at the *found* site only — the not-found
  fallthrough returns `(null, 0)` for a genuinely absent key and must keep
  doing so. Cost-free: the pure-number path never reaches that accessor. An
  empty string element is `("", 0)`, a real pointer, so null is an
  unambiguous discriminator. Specs `ivc.12-17`.
- **`sash_redirect` closed the fd it had just installed** (`621cb0c`).
  `open()` returns the lowest free fd and `dup2(src, fd)` with `src == fd` is
  a POSIX no-op, so the unconditional `close(src)` destroyed the redirect
  whenever the target *was* the lowest free fd — i.e. fd 3. `exec 3> f` left
  fd 3 closed, `close 3` reported EBADF, and every later `>&3` **silently
  fell through to the real stdout**. `cl.1` uses fd 9, where the close is
  correct; that one choice is why it shipped. Specs `cl.9-11`.
- **An unterminated `$((` inside `"…"` was evaluated anyway** (`c5df3b7`).
  `echo "$(( 1 + 2"` printed **3**. The scan ended on "ran out of source" and
  then used *another* length test to decide whether to skip the closing `))`
  — one test doing two jobs, so truncation failed both together and left no
  evidence. Now gated on `depth == 0`, with the overrun recorded through the
  Scanner's existing `unterminated` channel (SB4057, span on the `$`). Bare
  `$(( 1 + 2`, `"${ s"` and `"$( cmd"` always rejected it; this scanner was
  the one hole. Specs `cov.130-133`.

**The complexity gate fired on the fix and was not bypassed.**
`consume_dq_arith_sub` hit cog=12 against a ~8.8 bar; extracting
`step_dq_arith_sub` — matching `consume_dq_cmd_sub`/`step_dq_cmd_sub` next
door — brought it under in one edit.

**Lead 1 from the previous cycle is closed, and the measurement is
trustworthy.** `is_identifier_or_subscript`'s unclosed-subscript arm went
0 → 8 hits after `aw.27`/`aw.28`, so `contrib/covdb.py` is *not*
mis-attributing branch indices on two-`let`-`else` lines. That was the
finding that gated the whole 100% target. Its two remaining arms are now
covered by `cov.146-147`.

### Coverage state

520 missed branches (85.37%) measured at `57c0d53`. **38 specs have landed
since that measurement** (24 in the prior batch, 20 in this one, minus
overlap), so `mk cov-branch` needs a re-run to set the new mark. The seam
list is unchanged: `validate.rs` 89, `parse/expr.rs` 51, `parse/lexer.rs` 49,
`parse/parser.rs` 41, `parse/arith.rs` 39, then a long tail. Distribution is
**flat** — max 8 per function — so batches must stay large (20-30) to amortize
the ~40-minute measurement. Probe with a separate uninstrumented binary
(`CARGO_TARGET_DIR=/tmp/sashprobe`) while it runs.

### OPEN FOR DG (resolved — see the current block above)

- ~~**Constant folding swallows a diagnostic.**~~ **CLOSED.** Resolved in two
  steps: `a39aa18` made `fold_arith_op` return `None` for a zero divisor (the
  one-line reconciliation described here), and journal 322 then replaced the
  reconciled behaviour outright — a proven zero divisor is now **SB5077** at
  compile time and an unprovable one **aborts** at runtime. `cov.68/69` and
  `cov.138/139` both moved.
- Still carried: the `[[ … ]]` exit-status rule from the prior cycle is
  **provisional**. DG floated reserving non-zero exits for `exit` and aborts,
  which would flip `[[ ]]`, `(( ))` and external-command guards together; the
  four `e.b70.tandf*` specs are written so that flip must move them as a set.



**Journal 320.** Sweep **4703 passing, 10 skipped, 0 failing** (+4: two new
specs × two modes). 36 sash unit tests (+1), 27 sash-convert tests.

DG: *"lets do the profraw for sash-rt and keep ratcheting the branch
coverage."* Both done.

### sash-rt coverage now works — first numbers ever

`mk rt-cov`, `mk rt-cov-branch`, `mk rt-cov-reset`; driver in
`contrib/rt-coverage.sh`, dump in `sash-rt/src/coverage.rs` (feature
`coverage`).

| file | lines | branches | functions |
|---|---|---|---|
| `lib.rs` | 82.43% | 67.93% | 91.34% |
| `inner.rs` | 67.81% | 66.67% | 76.47% |
| **total** | **81.51%** | **67.83%** | **89.58%** |

2925 profraws from a full sweep. The old claim that `sash-rt` was
"not measurable this way at all" was wrong: instrumentation always worked
(2762 `__profc` symbols in the archive), only the *write* was missing.
`__llvm_profile_write_buffer()` needs no libc — seven objects from rustc's own
`libprofiler_builtins`, whose closure is `__ehdr_start` plus `memcpy`,
`memset`, `memcmp`, all three already defined in `sash-rt/src/lib.rs`.
**Nothing here knows the profraw format**, so an LLVM bump cannot silently
corrupt it.

Taken from the **rustc sysroot**, not brew clang: the header version and the
counter layout must come from one toolchain or they disagree.

**`sash_shutdown` now ships unconditionally.** The normal exit path was two
instructions of naked assembly after `call main`, so no Rust code could hook
it; the seven `exit_group` calls all route through the new function instead,
mirroring `sash_startup`. Only `coverage::dump()` inside it is feature-gated.

### Compiler branches: 85.34%, 551 missed

Re-measured at `4fe07a2` (was 85.12% / 562). Lines 95.33%, functions 97.51%.

- **`wrap_with_redirects` never wrapped** — 18574 of 18594 calls arrived with
  no redirects, the other 20 all returned SB4046, and the whole
  `StmtKind::Redirected` construction was unexecuted. A command absorbs its own
  redirects in `parse_simple_command`. Deleting it silently would have let a
  future redirect **vanish from the AST**, so it became `redirect_reject_noun`,
  which panics, with `redirect_reject_noun_is_loud_ice` to prove it.
- **Two unreachable `let`-`else` arms commented, not asserted** —
  `resolve::resolve_command` and `validate::check_echo_splat`. Both are
  unreachable because a non-`Literal` head is SB5011 in
  `run_structural_checks`, which **exits** before either runs. Cross-pass, so
  by the standing rule they earn a comment, not an ICE. The first was settled
  by instrumenting, not by reading.
- **`is_resolvable_external`'s `name.is_empty()` removed** — `''` and `""` lex
  as quoted forms, so an empty `Literal` head cannot exist, and the fallback
  was already SB5011 "command '' not found".
- **`is_identifier_or_subscript`'s unclosed-subscript arm** was reachable with
  no test — `aw.27`/`aw.28`.

Three arms resisted diagnosis from counts alone and are filed in
`notes/tasks/branch-coverage-ambiguous-arms.md`. One of them is a possible
real finding: `embed_artifacts` never skips, so all three of `.sash_source`,
`.sash_ir` and `.sash_manifest` are always present even though `--manifest` is
opt-in.

## ══ STATUS (2026-09-14) — DG's six-part directive, four of six landed ══

**Journals 317–319.** Sweep **4699 passing, 10 skipped, 0 failing**.
35 unit tests, 27 sash-convert tests. Pushed through `d8763b9`.

DG's directive: *drop the unreachable compound code; drop backticks as sugar;
update status; explain "eleven negation spellings"; use abort with message
disposition for converter; regex already has counted repetition, yes?; give
sash-rt options.*

- **Compound-redirect machinery removed** (journal 317, `22a898f`).
  `RedirectPush`/`RedirectPop` were a save/restore pair for redirects on
  compound statements — 141 lines over six files, **two** parallel
  implementations of one protocol, for a case SB4021 forecloses in the parser.
  The `ice_unreachable_command` arm claimed `Write | RedirectPush |
  RedirectPop` were all "rewritten before codegen"; only `Write` is. Sweep
  unmoved, which is the point: there was nothing to move.
- **Backticks dropped as sugar** (journal 318, `783d884`). DD#67's table read
  "~yes (modulo nesting) | review" for two years; the modulo *was* the answer,
  since nesting needs `\``, which DD#64 removed. All spellings now **SB5076**
  — a new code, not the taint code SB5020 the nested case had been borrowing.
  `sash-convert` already rewrote backticks to `$( )`, so only the compiler
  half was missing. DD#67 updated; the 2026-06-21 flat-keep resolution struck.
  - **Bug found while removing the token.** `command_sub_to_expr` carved out
    content starting with a backtick and kept it as a verbatim `Literal` — a
    holdover from when backticks were a token. Its surviving caller is `$( )`
    inside `"…"`, so `` echo "$(`/bin/echo hi`)" `` **compiled**: the whole
    substitution became a literal, dodging the backtick rejection *and*
    `check_taint`, and diverging from bash, which runs the command. `cov.103`.
- **Converter abort disposition** (journal 319, `d8763b9`). DG chose option
  (b): a bash string variable read in arithmetic converts to `${i:?msg}`.
  Three spellings (`i`, `$i`, `${i}`) plus the textual subscript path;
  `declare -i` names, env vars and *writes* are declined. The documented cost
  stands — bash reads a non-numeric string as 0, so the converted program
  diverges on exactly those inputs, deliberately. Specs 57.148–57.155.
  - **Bug caught by existing specs**: the `${i}` arm first matched on child
    count, which is also true of `${#s}` — it dropped the length operator.
    Note the asymmetry that made this dangerous: `${#s}` is *bash* input
    (SB4038 retired it in sash, so the converter's whole job there is to emit
    `(strlen $s)`), and the **buggy output was legal sash**. `s="42"` would
    have compiled and printed 43 instead of 3. Now matches on node text.

**Three legacy carve-outs in three journals.** 317's dead protocol, 318's
`starts_with('`')`, 319's child-count test. Each was an explicit special case
someone added on purpose, each outlived its motivating construct, and each
kept firing on whatever still matched its predicate. A predicate that is
structurally true of the thing you want is not a predicate true of *only* it.

## ══ STATUS (2026-09-14) — the guard that declined to act ══

**Journals 311–315.** Sweep **4667 passing, 10 skipped, 0 failing**.
35 unit tests. Pushed through `7564412`.

**Journals 314–315 continued the same ranking, with a different split.** Not
every zero-hit arm is a defect — but the count alone never decided which.

- **SB5002 cascade** (journal 314) — a bare heredoc in command-name position
  put the internal lexer sentinel `__HEREDOC__` into a user-facing "command
  not found", then re-reported each heredoc body line as SB5011.
  `validate_command` already guards this, but the command-head check is a
  separate walk, so the early return had no reach into it. Fixed at the batch
  level in `run_structural_checks` — when SB5002 is present it is reported
  alone, which is what the `lib.rs` comment already claimed pass ordering
  guaranteed. Specs `cov.101`/`cov.102`.
- **Two dead arms removed** (journal 315) — `transform/expressions.rs`'s
  `$EPOCHSECONDS` synthesis contradicted `validate.rs:1447`, which rejects the
  spelling outright and runs first; and `ssa_taint.rs`'s `attest` exemption
  never fired because every attest form is lowered by `lower_if_attest` into
  branch machinery, never an `SsaInst::Command`. The second was confirmed by
  instrumenting the guard across four spec files, not by inspection, and its
  reasoning stays as a comment naming when the exemption must return.
- **Unreachable but kept** — `resolve.rs`'s two guards (`name.is_empty()`,
  the `let…else` on a non-literal head) are unreachable only because
  `run_structural_checks` exits first. That is a *cross-pass* invariant, not a
  local one, so unlike the lexer arms in journal 312 they were left as returns
  rather than promoted to panics. See
  `notes/tasks/resolve-guards-unreachable-by-earlier-pass.md`.
- **`cov.103`** — an escaped backtick inside a backtick substitution earns a
  distinct SB5020 wording; nothing in the suite had ever written one.

**The rule that emerged: a local invariant earns an assertion; a cross-pass
one earns a comment.**

**SB4046 widened — no meaningless redirects** (journal 316, DG 2026-09-14).
The redirect-drop fork is settled: reject. `(( … )) > f` and `[[ … ]] > f`
produce no output, so bash's create-and-truncate-then-write-nothing is a wart;
sash now rejects them instead of parsing the redirect and silently discarding
it. Two nouns added to `noncommand_redirect_target`; all five spellings from
the task table turn out to be one case, because `-` and `return` wrap the
*redirect*, not the test. Specs `rdr.1`–`rdr.10`. A bare `[[ … ]] > f` now
reports SB4046 rather than SB4018 — the more specific of the two, and `rdr.7`
pins that removing the redirect still leaves SB4018.

**`is_bare_redeclare_noop` purged** (journal 315 addendum, DG). SB5049 stays
as written — one declaration per name per scope, no exceptions.

**Coverage has not been re-measured since `36e5f0e`** — SB4057, `cov.101–103`
and the `-[[ ]]` specs should convert several arms. Re-run `mk cov-branch` →
`mk cov-db` → `mk cov-reset` before the next ranking pass.

Three sessions of branch-coverage asymmetry work, and every one of them turned
up a defect rather than a test gap. The shape repeats: **a guard that computes
the right condition and then declines to act on it.**

- **SB4056** (journal 311) — `echo a &&`, `echo a ||` and `echo a |` all
  compiled and ran, printing `a`. `parse_trailing_list_op` *substituted*
  `Statement::true_cmd` for a missing right operand; the pipeline loop fell
  out and kept only the left side. `background_list_op`'s `true_cmd` stays —
  a trailing `&` is a terminator, not an infix operator, and bash accepts it.
- **Three lexer arms** (journal 312) — `scan_word_chars`'s bare leading `=`
  was genuinely dead (the dispatcher claims `b'='` for `scan_eq_punct`) and is
  gone with its `Option` wrapper. `Scanner::next`'s direct-pull arm is real,
  just never taken, and now has two unit tests pinning the no-peek contract.
  Proving the first required enumerating every route into `scan_word`, which
  surfaced `scan_via_logos` falling back to a word scan on a DFA miss — all
  ten dispatched bytes are word-break bytes, so it returned `Tok::Eof`
  **mid-source** and silently truncated the program. Both are invariant panics
  now, each with a `#[should_panic]` test, plus a test proving the DFA covers
  every dispatched byte so they stay unreachable.
- **SB4057** (journal 313) — eight scan-to-closer helpers ended with
  `if self.pos < self.source.len() { self.pos += 1; }`. That guard *is* the
  run-off-the-end detection and it did nothing, so `/bin/echo "hello` and
  `echo ${x` compiled, ran, and printed. bash rejects all of them. `Scanner`
  now has one `unterminated` field and one `require_closer` helper replacing
  the idiom at every site; `parse` reports SB4057 alone, ahead of everything
  else, with the span on the opener.

`misc.sh` 104.4 failed against SB4057 and was right to: its source was
`x="a\"b"`, but DD#64 removes shell backslash escaping, so that is three
tokens ending in a quote that opens a string running to end-of-source. The
test was correct about `emit_ir` and wrong about how to spell its input.

**Coverage 84.80% branches, 576 missed.** Note `lexer.rs:235` will stay at 0
in `coverage.db` forever — coverage is built from spec-sweep profraws, and
that arm is covered by a unit test.

**The method, for next time.** Rank zero-hit arms by *sibling* traffic, not by
count. A 0-against-80,000 split is a path the compiler has structurally never
taken; a low-traffic miss is merely untested. Five for five on real defects.
Read **both** arms before concluding, and when an arm looks dead, enumerate
every caller — twice now the enumeration, not the query, found the bug.

```sql
WITH pairs AS (
  SELECT a.file, a.line, a.count AS dead, MAX(b.count) AS live
  FROM cov_branches a JOIN cov_branches b
    ON a.file=b.file AND a.line=b.line AND a.block=b.block AND a.branch<>b.branch
  WHERE a.count=0
  GROUP BY a.file, a.line, a.block, a.branch
)
SELECT file, line, live FROM pairs WHERE live > 1500 ORDER BY live DESC;
```

**Next up:** re-run `mk cov-branch` / `mk cov-db` / `mk cov-reset` (SB4057
should convert several arms), then work the ranking down. Unexamined hot
lines: `resolve.rs:207`, `resolve.rs:105`, `validate.rs:3985`,
`validate.rs:1222`, `transform/expressions.rs:172`, `lexer.rs:674`,
`ssa_taint.rs:200`, `codegen/entry.rs:486`.

## ══ OPEN FOR DG ══

**Nothing.** All four items that stood here are closed.

- **`ir::Program::end_block`** — DG said *wire it up*, and `4d7f6d6` did.
  `:end-block` now carries the body and `:statements` no longer duplicates it
  as a `__sash_end__` function; `transform/entry.rs:33` reads the field.
  *(This item was left standing here by mistake after it had already been
  done — it was stale, not open.)*
- **`sash-rt` coverage** — DG chose (a); implemented, see the top block.
- **The compound machinery** is deleted.
- **Backticks** are dropped, which dissolves the `${var:-`cmd`}` question
  entirely: there is no backtick spelling left to be inconsistent about.

## ══ STATUS (2026-09-14) — coverage: DD#109 landed, uncovered remainder dispositioned ══

**Journals 306, 307.** Sweep **4568 passing, 9 skipped, 0 failing**.
DD#109 committed and pushed (`203936f`).

## Disposition pass (journal 307) — NEXT UP: three questions for DG

| metric | pre-DD#109 | post-DD#109 | + vm.bl16-18 |
|---|---|---|---|
| lines | 93.31% | 94.66% | **94.79%** |
| regions | 91.63% | 93.79% | **93.92%** |
| branches | 83.77% | 83.97% | **84.21%** (599 missed) |
| functions | 95.44% | 97.11% | **97.25%** (56 missed) |

**Done.** Three zero-coverage lexer helpers on the brace-less disposition
operand path (`skip_braced_to_close`, `skip_simple_quote`,
`skip_backtick_to_close`) were reachable and simply untested — specs
vm.bl16-18 added; `parse/lexer.rs` functions now **100%**. Two stale doc
comments corrected and DD#90 rule 4 marked SUPERSEDED (the DD#91 follow-up
allows `$( )`/`$(( ))`/backtick brace-less operands; vm.bl13 recorded it, the
DD file never did).

**Bug fixed: `mk build` never built `sash-expand`.** It only ever worked
because `mk test` builds it as a side effect. Any `rm -rf target/release`
(which `mk cov` and `mk cov-reset` both do) made spec 54.1 fail — obscurely,
since `which sash-expand` yields an empty path and the error surfaces as a
compile failure about a config file.

**Also fixed: `contrib/coverage.sh` never emitted lcov**, so every triage so
far lived in `/tmp` and could not be regenerated after `cov-reset` drops the
covmap. Now `--lcov`, written to `target/coverage/`, plus `contrib/covdb.py`
(promoted from `/tmp`, now parses branch records) and `mk cov-db`.

**Awaiting DG — three questions (journal 307, "Open for DG"):**

1. **Remove the compound-redirect machinery?** `bracket_compound_with_redirects`
   is the only producer of `BuiltinCmd::RedirectPush`/`RedirectPop` and cannot
   run — SB4021 rejects a redirect on any compound, SB4046 on a function. Dead
   across parser, transform, codegen and emit_c. It was live until journal 268
   gave `ReadArray` its own redirects.
2. **`lower_negated_stmt`: delete or ICE?** `Negated` is built only as a
   pipeline stage and always peeled; eleven negation spellings, zero hits.
   Recommend ICE, per journal 281's rule.
3. **Backtick vs `$( )` in a `${var:-WORD}` default.** DD#67 makes flat
   backticks 1-to-1 sugar for `$(…)`; DG 2026-08-28 makes them literal inside
   quotes; DD#65 makes the default body a quotes body. The intersection is
   consistent but the two spellings are not interchangeable there.

**Still open, not started.** `emit_ir::ar_incdec`/`ar_subscript_assign` plus
`transform/statements::hoist_mutation_node` form a second suspected-dead
cluster: the parser's `lower_arith_mutation_structured` rewrites `PostIncDec`,
`PreIncDec` and `SubscriptAssign` away before the AST reaches anyone. Needs
the same trace before removal. And **sash-rt coverage** (DG asked for it) is
still blocked: instrumentation compiles and maps, but a no_std/no-libc binary
writes no profraw at exit — options not yet put to DG.

## Coverage work restarted (DG: full line AND branch coverage)

Goal: full line and branch coverage from spec tests. Whatever is left over is
either dead code (delete) or a should-never-run assertion (keep + unit-test
that it catches what it claims).

**Two rotted gates found first.** `cargo llvm-cov` was not installed (rsync
casualty). `mk cov-spec` exports `SASH=`, but `spec/run.sh` reads
`SASH_RELEASE_DIR` — the variable it sets is not the one the runner consumes,
and its `target/llvm-cov-target` path is not where cargo-llvm-cov builds now.
Stale since 2026-08-30; **not yet fixed**.

Then the same shape bit me: `cargo llvm-cov clean --workspace` does *not*
invalidate `target/release`, so the build was reused and the sweep produced
**0 profraws** — a measurement of nothing, reported as a report. Use
`rm -rf target/release`, and gate every run on a **canary** (one `sash` call
must drop a profraw, else abort).

**Baseline** (spec-only, compiler only; branch coverage is nightly-only, so
nightly measures and stable ships — both leave the suite green):

| metric | baseline | after DD#109 |
|---|---|---|
| lines | 93.31% | **94.66%** |
| regions | 91.63% | **93.79%** |
| branches | 83.77% (615 missed) | 83.97% (608) |
| functions | 95.44% | **97.11%** |

`coverage.db` (untracked) joins lcov data to indexnudge `symbols`, so
uncovered lines rank **by function** instead of as flat line numbers.

## DD#109 — every native compile embeds its IR and manifest

DG's call, and it replaced the spec-writing I was about to do. `emit_ir.rs`
was the largest hole in the compiler — 30.8% regions, 409 missed lines, **35
never-executed functions**, all IR node emitters — because `--emit-ir` was a
terminal stdout mode that four specs exercised over three tiny programs.

`emit_ir.rs` is deliberately exhaustive (no wildcard arms, DD#104), but
totality only buys the `match` arm: an emitter only ever *run* on `echo hello`
still has its escaping, nesting and node shapes unchecked.

Native compiles now embed `.sash_source` (DD#45), `.sash_ir` and
`.sash_manifest` as ELF note sections, read back with
`sash --extract <binary> [source|ir|manifest]`. DG chose sections over sidecar
files, kept `--emit-ir`/`--manifest` as stdout modes, and scoped it to native
(`--emit-c` has no ELF to embed in).

Result: emit_ir.rs **33.7% → 93.4% lines**, 35 → 2 unexecuted functions.

**One bug in it, caught by asking what the common case does.** A zero-length
section does not survive into the linked ELF, so an all-builtin program — the
common case — had *no* `.sash_manifest`, and `--extract prog manifest` called
that an error. Fixed on the read side (absent reads as empty, scoped by
`.sash_source` so a non-sash binary still errors); padding the section would
have broken byte-identity with `--manifest`. Specs 109.4/109.7 pin both.

`spec/cli.sh` 109.1–109.7, plan 26 → 33. Predicted 4548 + 7×2 = 4562, observed
4562.

## Next

**The disposition work has not started.** Remaining fully-uncovered functions
are a small analysable set — `bracket_compound_with_redirects`,
`lower_negated_stmt`, `emit_redirect_push`/`pop`, three lexer `skip_*`
helpers, `opaque_form_to_string`, `ice_unreachable_command`. All have live
call sites, so none is trivially dead; each needs the journal-189 treatment
(shadowed by an earlier check, or merely untested?).

**`sash-rt` is not measured at all** (DG raised it). `#![no_std]`,
`panic = "abort"`, own `_start`, no libc. Instrumentation *compiles* — verified
2801 counter regions and full `__llvm_covmap` — but nothing writes the
`.profraw` at exit. Covering it needs a profile dumper inside the runtime using
raw syscalls, plus clang for ~1359 lines of vendored C. **Options not yet put
to DG.**


---

## ══ STATUS (2026-09-14) — DD#108: the array regex-filter shorthand is gone ══

**Journal 305.** Sweep **4548 passing, 9 skipped, 0 failing**. `mk test` rc=0.

## `arr=(GLOB =~ 're')` retired — SB5075

DG's call. I filed the bug with two fix options; DG picked neither, because
the syntax was the defect, not the guard implementing it.

| form | meant |
|---|---|
| `arr=(* =~ re)` | glob then filter |
| `arr=(*.ff =~ re)` | **3 un-globbed literal words, silently** ← the filed bug |
| `arr=(x =~ y)` | a plain 3-element array |
| `arr=(* =~ a b)` | a plain 4-element array, silently |

Generalizing element 0 (my option (a)) dies on a measurement made after
filing: **quoting was invisible to the desugar.** `(* =~ re)`, `(* '=~' re)`,
`('*' =~ re)` and `("*" =~ re)` all produced identical output. Wiring the
escape would mean letting quotes change a list's **arity and meaning** rather
than a value — the bash trap DG explicitly refused. The form also could not
name what it matched (runtime tested the bare filename; elements were DD#73
absolute paths).

**Replacement, expressible all along:**
```text
array a
for f in *.ff { if [[ $f =~ 're' ]] { a += ($f) } }
```
It names `$f`, so the match target is written down instead of inferred.

**The rule is uniform** — a bare `=~` anywhere in an array literal is SB5075.
A **quoted** `=~` stays an ordinary element (`arr=(x '=~' y)` → 3 elements);
that is the escape, and the diagnostic names it. `arr=(*.sh)` is untouched.

**`sash_glob_regex` deleted.** DG: *"the function name itself seems a
contradiction"* — two pattern languages doing one job, and my proposed fix
would have added the argument that made the name actively false.
`sash_glob(dir, pattern)` is now the single glob primitive.

specs: glob.sh 11 → 14 (g.11 is the filed bug's exact form; g.14 compiles the
loop the diagnostic advises).

## Closed

`bug-array-regex-filter-silent-literal` (P2) — resolved by removal, DD#108.

## Filed

`sb5025-placeholder-reads-as-syntax` (P4) — SB5025 renders the metavariable
`[N]` inside backticks, so it reads as literal syntax; the correct form is
`argv 2 a`. I followed it literally and misreported a bug. Also: malformed
`argv [2] a` compiles to a bare-command exec and **ICEs** at runtime instead
of being rejected.

## ══ STATUS (2026-09-14) — externalized true/false get a CHECK ══

**Journal 302.** Sweep **4500 passing, 9 skipped, 0 failing, 0 flips** (+7,
convert.sh native-only). `sash-convert-check-comment-true-false` is **done**.

**The filing was accurate** — first one in four that was. Re-measured anyway:
bare `false` aborts and the next statement never runs; `false || echo x` does
not abort; `while true` costs **11.1 s / 20,000 iterations** (~0.55 ms/pass)
against a free bash builtin.

## Two messages, not one

The gate asked for a hint; it wanted two, because the hazards differ in size:

| position | message |
|---|---|
| `false`, status discarded | **ABORTS** (DD#55) — use `! :` or a fn returning 1 |
| everything else | status matches bash, but forks and execs each pass |

`status_is_consumed` walks out through `list`, `negated_command`,
`command_substitution`, `pipeline`, and the **condition field** of
`if`/`while`/`until`/`elif` — a command in a loop *body* is discarded-status
like any other statement, so the field check is load-bearing.

My first draft told a bare `true` that "a bare `false` ABORTS" — confusing
and false. The abort wording is now gated on the command being `false`.

## Still open for DG

`harness-diagnostic-advice-compiles` (P3) — three options for making the
"advice must compile" gate structural. The one that works changes the
`Diagnostic` type (suggestions as data, not prose), which is a surface
decision. **DG's call.**

---

## ══ STATUS (2026-09-14) — auditing the advice a diagnostic gives ══

**Journal 301.** Sweep **4493 passing, 9 skipped, 0 failing, 0 flips** (+12:
6 hint specs × 2 modes). `audit-diagnostic-suggested-spellings` is **done**.

## Third instance, and the one that stings

`type_check::conversion_hint` told every SB4030 string→int mismatch to
"use `$(($var))`". That is **itself SB4030** — a string→int conversion must
carry a disposition. The compiler rejected an assignment, gave a fix, and
rejected the fix, with the right answer sitting one diagnostic away.

Working forms: `$(( ${s:-0} ))` / `$(( ${s:?msg} ))`. Note `int n=${s:-0}` —
disposition *without* arithmetic — is also rejected, so the advice must name
both halves. Placeholders changed `$var` → `VAR` (they read as real names).

## Audit method: read stderr, not source

Two confident findings were **false positives**. `` use `@{name}` `` looks in
source like the invalid braced splat (SB5033); it is a `format!` placeholder
rendering as `@a`, which is correct. The message a user sees exists only after
interpolation.

Four of my first thirteen "failures" were my fixtures' fault, not the
compiler's (`while readline v` needs the redirect off the compound; `a[i]` in
arithmetic needs `int array`; `map m = @{a:DISP}` needs its result read).

~25 suggestions compiled clean. One bug in twenty-five.

## The gate is NOT closed — needs DG

Manual sweeps find today's instances; the class re-opens whenever a DD retires
a form, because a message is a string literal and nothing compiles it.

Filed **`harness-diagnostic-advice-compiles` (P3)** with three options and
deliberately did not pick. The one that makes this structural — carrying
suggestions as **data on `Diagnostic`** instead of baking them into prose —
changes the `Diagnostic` type, which is a surface decision. **DG's call.**

Meanwhile the spec shape to copy is `hint.1`–`hint.6` (types.sh) and
`gs.36`/`gs.37`: assert the message text **and run what it recommends**.
Text-only assertions are how SB5030 survived DD#100 recording it.

---

## ══ STATUS (2026-09-14) — `$@` in functions, and a gate that rotted twice ══

**Journal 300.** Sweep **4481 passing, 9 skipped, 0 failing, 0 flips** (+16:
12 converter native-only, 2 glob_splat × 2 modes). Fail-first predicted the
exact set of 12 and observed it.

`converter-dollar-at-migration` is **done**. In-function `$@`/`$*` were
entirely unconverted; the top-level half had been done long ago.

## Context decides the target

| bash (in a function) | sash |
|---|---|
| `for x in "$@"` | `for x in @args` (quotes must go) |
| `g "$@"` | `g @args` |
| `echo "$@"` | `echo "$(join args)"` — `echo` joins; a splat is SB5030 |
| `j="$@"` | `j="$(join args)"` — a value, not an argument list |
| `"pre $@ post"` | **MANUAL** — no sash form; `"@args"` is a plain literal |
| `"$*"` / `${*}` | `$(join args)` |

Unquoted `$@` converts with a CHECK (bash re-splits on IFS; sash catches the
shape that matters as SB5042, so it is not silent).

## The filing's `$#` gate was wrong in two directions

It read "`$#` → `${#args}` per DD#75 sugar, still valid".

- `${#args}` is **SB4038** — DD#75 §4 retired that sugar after filing.
- The obvious repair, `$(len args)`, is **also wrong**. DD#46 keeps `$#`
  function-local *and sash-native*. `fn f { echo $# }` compiles; so does
  `$(( $# + 1 ))`.

Bare `$#` needs **no conversion**. Only braced `${#}` moves (SB4034 → `$#`).

Caught by three specs failing — `conv.f1`/`conv.f3`/`conv.x8` all pinned
"in-function positionals pass through untouched". `conv.f1` genuinely was a
stale pin of the gap this task closed. **`conv.f3` and `conv.x8` were right**,
about the exact thing I had just got wrong. Reading the DD before overriding a
spec is the only reason it did not ship.

Third materially-wrong filing in four days. **Re-measure before implementing.**

## SB5030 recommended a form the compiler rejects — second instance

`echo $(join @a)` is SB5062; `join` takes the array by name. DD#100 line 119
already knew and deferred it into `dd75-len-collection-only-strlen`, which is
marked **done with that gate unmet** — the bug survived being found, written
down, and assigned.

Same defect as journal 299's SB4035. Filed
**`audit-diagnostic-suggested-spellings` (P2)** to compile every suggested
spelling in every message. `gs.36` pins the message text; `gs.37` **runs** the
form it recommends — pinning text alone is how this class persists.

---

## ══ STATUS (2026-09-13) — the overlapping-patch audit, and five bugs under it ══

**Journal 299.** Sweep **4465 passing, 9 skipped, 0 failing, 0 flips** (+14:
8 converter native-only, 6 arith × 2 modes). Fail-first predicted 7 converter
failures (57.128 is a control that passes clean) and 6 arith × 2 — all as
observed.

Journal 297 wrote down a bug shape in the abstract. Journal 298 found a second
instance by accident and said "writing it down is not the same as checking for
it." **This was the check. Four more instances.**

## The corruption (the cheap find)

A handler that replaces its **whole node** and a patch produced **inside** that
node cannot both be applied — the inner patch re-emits bytes the outer one
already consumed. Silent, not an error.

| input | was |
|---|---|
| `declare -i n=$?` | `int n=$?$(status)` |
| `declare -a a=("$HOME")` | `array a=("$HOME")$home")` |
| `let "x = $?"` | `(( x = $? ))$(status)"` |
| `mapfile -t a < "$HOME"` | `… < "$HOME" \| read a$home"` |

Fixed by composition (`render_range`, new shared helper) in
`convert_declaration`, `convert_command`'s `let` arm, `convert_mapfile` and the
`trap` arms — plus a **structural backstop** in `apply_patches` that drops any
patch contained in an earlier patch's span. A future handler that forgets to
compose now *loses* a conversion (a bug the spec suite catches) instead of
corrupting the file.

## Five bugs underneath, in a chain

Fixing the `let` arm required deciding what `let "x = ${#s}"` converts to. Each
answer exposed the next layer:

1. **`${#…}` in arithmetic did not compile.** `$(( ${#s} + 1 ))` converted to
   `$(( $(strlen $s) + 1 ))` — SB4020, a command substitution is not an
   arithmetic operand. `length_expansion` now takes the context.
2. **The sexpr the fix emits was rejected by the parser.** `(strlen ${a[1]})`
   → SB4020, while **SB4035 explicitly recommends `${a[i]}`**.
   `is_func_arg_start` omitted `Subscript`/`BareSubscript`/`ParamExpansion`.
   They are operands, never binary operators, so admitting them adds no
   grouping ambiguity.
3. **A cell operand then evaluated to 0.** `reconstruct_intrinsic_command`
   returns `None` for a complex operand and the caller falls back to the *user*
   int-function path, which finds no body for `strlen` and yields 0. **Same
   sentence as yesterday's `getcpid`: a fallback written for one category of
   name, reached by a name from another.**
4. **A parenthesised operand too** — `(strlen ($s))` → 0.
5. **Wrong arity in an arith sexpr was accepted silently.** `$(strlen a b)` has
   reported SB4045 all along; `$(( (strlen a b) ))` reported nothing, because
   `check_cmdsub_arity` matches `CommandSubstitution` and a sexpr is
   `Arithmetic`.

**One process note.** I wrote (5) in `type_check.rs` first. It worked and
reported **SB4030**, because that is the code `type_check` emits for
everything — so the identical user error answered to two codes depending on
spelling. That is the defect I had just finished complaining about,
reintroduced in the fix for it. Moved to `validate.rs` next to
`check_cmdsub_arity`.

**Second process note.** `plan=` in a spec file is **per-mode**, not
per-sweep. I set arithmetic's to 217 (205 + 6×2) when it should be 211;
`common.sh` reports `[plan=…, ran=…]` rather than failing, so only predicting
the total caught it.

## Nothing new is open for DG

All six were bug-policy mandatory. The one judgement call — widening
`is_func_arg_start` — is settled by SB4035's own message, which has been
promising `${a[i]}` works.

## ══ STATUS (2026-09-13) — keyword specials, and two silent-corruption bugs ══

**Journal 298.** `converter-special-var-sugar` **done**. Sweep **4445 passing,
9 skipped, 0 failing, 0 flips** (+14). Fail-first predicted 8 of 10 converter
specs (2 are controls) and 2 arith specs × 2 modes — all as observed.

`$?`/`$$`/`$!` → `$(status)`/`$(getpid)`/`$(getcpid)`. **Two canonical
spellings, not one**: in arithmetic it is the DD#58 sexpr `(status)`, because
`$(status)` inside `$(( ))` is SB4020. `$#` already had this shape and the
comment warning about it sat six lines above the code I was editing. These
three are process-wide, so unlike `$@`/`$*`/`$#` they convert inside a function
body too.

**Gate 2 named SUBSET.md, gone since 2026-08-29.** The manpage is where
language surface is taught now, and it did not mention `getpid`, `getcpid` or
`status` **at all** — the new `.SH SPECIAL VALUES` is their first
documentation.

## Two silent-corruption bugs, both pre-existing

**`$(( (getcpid) ))` was 0** while `$(getcpid)` was correct.
`bug-arith-keyword-pid-intrinsic-zero` is marked **done** and names that exact
gate. `resolve_nullary_sexpr` even admits it: "`getcpid` (`$!`) is excluded —
filed", pointing at the filing that says otherwise. One arm; it reuses `$!` the
way `getpid` reuses `$$`.

*Why silent:* the fallback treats an unknown nullary name as a variable
(correct — `$(( (x) ))` is grouping), and a typo there is caught by SB5031. But
`validate` **exempts every intrinsic name** from the typo check. So `getcpid`
was waved past the check that would have caught it *because* it is an
intrinsic, then not handled by the transform that handles intrinsics. An
exemption granted on one pass's knowledge, consumed by a pass that lacks it.

*Why it outlived the fix:* **no spec.** `sx.1`–`sx.11` pin every other
intrinsic. `getcpid` was in the gate text and nowhere in `spec/`. A gate that
is only prose does not fail. Now `sx.24`/`sx.25`, native and c.

**`[ … ]` conversion corrupted its own output** — unfiled, and hitting one of
the commonest lines in shell:

```
if [ "$1" = x ]; then echo y; fi
  ->  if [[ $1 = x ]]$arg1" = x ]; then echo y; fi
```

Every `[ … ]` holding a known env var, a positional or a `${#s}`. This is the
exact shape I described *in the abstract* in journal 297 one day earlier while
fixing the patch sort order — and did not go looking for other instances of.
Writing down the general shape is not the same as checking for it.

Fixed by composition, not suppression: `convert_test_command` renders its
children to text, transforms that, and tells `walk_node` not to recurse.
Two traps found by testing — the decision must be made on raw text while the
output uses the splice (else the hoisted `getenv`+`attest` line duplicates),
and `prefix_line` rides on the `Patch`, so a spliced patch skips `convert`'s
hoist loop and the pair goes silently missing.

## Process

**A spec-id collision was caught only by a count mismatch.** `sx.12`/`sx.13`
already existed further down `arithmetic.sh`; later `chk` wins, both passed,
nothing reported it. Totals were 4 short of prediction (2 ids × 2 modes).
AGENTS.md now carries the mechanical duplicate check and a note to **predict
the sweep total before running it** — a green sweep with an unexpected total
means specs collided, vanished or were skipped.

## ══ STATUS (2026-09-13) — here-strings convert now ══

**Journal 297.** `converter-herestring-to-pipe` **done**. Sweep
**4431 passing, 9 skipped, 0 failing, 0 flips** (+9, `57.102`–`57.110`).
Round-trip byte-identical to bash. Fail-first predicted 9, observed 9.

`cmd <<< WORD` → `producer | cmd`. DD#83 removed `<<<` (SB4050) and the
converter passed it through.

**The filing's `$(sub)` recommendation was right for the wrong reason.** It
preferred `cmd <<< "$(sub)"` → `sub | cmd` as "fewer moving parts, same
bytes". Not the same bytes — bash's `$(…)` collapses trailing newlines and
`<<<` re-adds one, so `while read … done <<< "$(printf 'a\n\n\n')"` iterates
**once** in bash and **three times** through a pipe. And not a preference:
`echo $(sub) | cmd` does not compile (SB5020, tainted substitution inline).
The pipe is the only form; the converter now emits a `# CHECK:` naming the
divergence. Measuring the *rejected* option is what produced the note.

**One literal word is not `echo`:** `cat <<< -n` → `printf "%s\n" -n | cat`.
Both bash's and sash's `echo` read `-n` as the retired flag regardless of
quoting (SB5059 bare, silent when quoted). Only `-n` — `echo -e` prints `-e`,
and `echo $x` with `x=-n` prints `-n` (sash does no runtime flag parsing;
bash does, so sash is the faithful one here).

## Two bugs found underneath

**`read v <<< "$x"` drew a false MANUAL** ("target is not a plain name",
DD#99). tree-sitter-bash puts a here-string redirect *inside* the `command`
node — unlike `<` and `>`, which wrap it in a `redirected_statement` — so
`read_args` counted it as an argument. `mapfile` had the same hole. Now one
named helper, `herestring_child`. `57.107` pins the absence of the note.

**Patch ordering was push-order-dependent.** `apply_patches` sorted by `start`
alone; at equal `start` a consuming replacement could sort before a zero-width
insertion, driving `pos` *backwards* and re-emitting consumed source. Silent
garbage, not a crash. Now `(start, end)`. Latent before this change — the
here-string rewrite is the first pass to emit both shapes at one byte.

## ══ STATUS (2026-09-13) — the retired bracket forms convert now ══

**Journal 296.** `converter-retire-bracket-splat` **done**. Sweep
**4422 passing, 9 skipped, 0 failing, 0 flips** (+9 specs, `57.93`–`57.101`).
Round-trip verified: converted output compiles and is byte-identical to bash.

`${a[@]}` / `${a[*]}` / `${!a[@]}` are rejected by the compiler (SB4038) and
the converter was passing them through, so any bash script using an array —
most of them — converted to something that would not compile.

**There is no lookup table.** The three sash spellings are not
interchangeable: `@a` is a splat legal only as a for-in source, `$(join a)` is
a scalar, `$(keys m)` is a splat that cannot be joined at all (SB5025). So
`"${a[@]}"` has four right answers by context — `@a` in a for-in source,
`$(join a)` for `echo` and for value contexts, and a MANUAL for any other
command argument (the same SB5032 wall `"$@"` hits; still DG's call in
`converter-splat-empty-case`).

**Gate 3 was stale:** it asked for `${#a[@]}` → `${#a}`, which is itself
retired. The only target is `$(len a)`, which the converter already emitted.
Pinned by `57.101` — the one new spec that passed under fail-first.

**A third wrong-diagnostic-by-shared-arm, fixed:** `${!a[@]}` drew
`# MANUAL: ${!var} indirect expansion`. It is not indirect expansion, it is
the keys form, which sash supports for maps. The check was
`starts_with("${!")`. Same shape as SB5011 (journal 288) and "expected `]]`"
(294). `57.99` asserts the new note **and the absence of the old one**.

**Declined, deliberately:** `$a[@]` / `$a[*]` (named in gates 1–2). Bash
reads those as `$a` followed by literal `[@]`, and tree-sitter agrees.
Converting them would invent a meaning bash does not give them.

## ══ STATUS (2026-09-13) — GUIDE.md and AGENTS.md were a pipeline behind ══

**Journal 295.** `guide-architecture-stale-cst` **done**. Docs-only; sweep
unchanged at **4413 passing, 9 skipped, 0 failing**.

The filing named three stale lines and added "worth a single pass over the
whole document". The note was the task; the three lines were a sample.

**What the pass found, beyond the filing:**

- The crate tour named **ten files that do not exist** and omitted `parse/`
  — the entire front end — along with `resolve.rs`, `ssa_taint.rs`,
  `regex_validate.rs` and `emit_c.rs`.
- **The C backend was not in the Backends section at all.** Half the spec
  suite runs through it. Now documented as the differential oracle it is.
- "Phase 4: Lower" described a deleted pass. Rewritten to say where the
  coercions went rather than deleted — "where do coercions happen" is a
  question a reader still has.
- "For New Contributors" pointed at `JOURNAL.md` and `TODO.md` (neither
  exists) and told people to run `mk test` (which hangs at HEAD). Rewritten
  around `notes/status.md`, the spec suite, `notes/decisions/`, and
  **re-measure the filing before acting on it**.

**→ `AGENTS.md` corrected too, and one item there matters beyond tidiness.**
It listed `emit_janet.rs` / `emit_source.rs` / `oracle.rs` and
`oracle_tests.rs` / `janet_tests.rs` (none exist), had every Rust test count
wrong, and said "57 design decisions … up to DD#69" against an actual 95 up
to DD#107. The consequential one: it **hard-coded the complexity means as
cc 12.46 / cog 6.58**, which are now **4.04 / 2.05** — so the documented 3×
bar reads 37/20 where the real gate is 12.1/6.2. Anyone sizing a refactor
against the quoted figures is ~3× too generous and finds out at the commit
hook. Replaced with the query, and a note not to quote a ratcheting
threshold by value.

**Both gates verified mechanically**, not by reading: every `*.rs` named in
the structure block exists, and every file in `sash/src` is named. Those two
checks took thirty seconds to write and would have caught two-thirds of it.

## ══ STATUS (2026-09-13) — two regex limitations, one of which was lying ══

**Journal 294.** Sweep **4413 passing, 9 skipped, 0 failing, 0 flips**.

**1. The interior-NUL limitation is now written down.** DG decided it YAGNI on
2026-09-10, but the filing's gate ("stated in the DD#63 regex documentation")
was never closed, and the man-page section from journal 292 shipped without
it — the one regex divergence that returns a **wrong answer silently**
instead of rejecting. Re-measured first: `strlen` on `a\0bcd` is 5, `=~ cd`
is false, `gsub` is a no-op. Now in `DD63` (addendum, with the runtime detail)
and `docs/sash.1` (§REGULAR EXPRESSIONS, "Interior NUL bytes"). The filing's
own repro used `$(len s)` and no longer compiled — corrected.

**2. `regex-literal-double-paren` defect 1 fixed.** `[[ $s =~ ^((a))$ ]]` was
rejected as "expected `]]`" — naming a delimiter the input does not lack.
`parse_binary_rhs` now checks for `Tok::DblParen` after the `=~` operand word
and says the real cause. The `[[ … { … } ]]` dispatch form improved from the
same edit (both route through `parse_binary_rhs` — DD#71's "the `=~` RHS is
an ordinary operand word" paying off). Specs `39.32`–`39.35`.

**The man page had been documenting the misleading message as if it were the
rule.** Updated to state the real trigger: adjacency, not nesting —
`^(a(b))$` has the same depth and compiles.

**→ Still open for DG on that task (now `blocked`):** whether an unquoted `=~`
pattern may contain `((` at all. Lifting it means the `=~` RHS tracks paren
depth like `scan_word_chars` does for subscripts — small and local, but it
widens what a bare word may contain against a real `$(( ))` ambiguity. The fix
above forecloses nothing; it just makes leaving the restriction in place
cheaper.

## ══ STATUS (2026-09-13) — DD#80 subscripts in the converter ══

**Journal 293.** `converter-braced-subscript-index` **done**. Sweep
**4405 passing, 9 skipped, 0 failing, 0 flips** (+7 specs, `57.86`–`57.92`).

`sash-convert` now rewrites bash arithmetic subscripts to DD#80 direct keys:
`${a[i]}` → `${a[$i]}`, `${a[i+1]}` → `${a[$(( i+1 ))]}`, `${a[a[0]]}` →
`${a[${a[0]}]}`. Integer literals and map barewords are already direct and are
left alone — `CollectionPass` now also tracks which names were declared `-A`,
since a bareword key means opposite things on a map and on an array.

**Two things the filing did not anticipate, both worth knowing:**

1. **tree-sitter-bash 0.25 cannot parse `${a[a[0]]}`.** It yields an `ERROR`
   node instead of an `expansion`, so no tree-dispatching pass can ever see
   it. `walk_node` has an `ERROR` arm that runs the rewrite off the raw span
   text and returns early. This is almost certainly not the only construct the
   grammar is blind to; it is the only one a spec happened to catch.

2. **`${a[@]}` was drawing a spurious `# MANUAL: ${var@X}` note** —
   pre-existing, from testing `inner.contains('@')` when `@` in a subscript is
   a splat. So the converter has been telling people to hand-convert the most
   common array idiom in bash. MANUAL markers are advisory, so nothing
   measured it. Fixed (`is_param_transform`), pinned by `57.92`.

**→ New for DG: `notes/tasks/converter-string-var-arithmetic.md` (P2,
blocked).** The converter emits non-compiling sash for plain
`i=1; echo $(( i + 1 ))` — bash's implicit string→int coercion has no
converter policy, so SB4030. Not a subscript issue (`${a[$(( i+1 ))]}` is the
correct spelling and fails for the same general reason). Four options costed;
recommendation is **infer `int i=1` when every assignment to the name is a
numeric literal, falling back to a `# MANUAL:` marker** — the only option
that neither invents a value nor diverges from bash. DG's call.

## ══ STATUS (2026-09-13) — the regex man-page section (and what writing it found) ══

**Journal 292.** `docs-regex-dialect-writeup` **done**. Sweep unchanged at
**4398 passing, 9 skipped, 0 failing**.

`docs/sash.1` now has `.SH REGULAR EXPRESSIONS` between TEST OPERATORS and
SASH-CONVERT. All five gates met; every claim measured against the binary,
none transcribed from a DD.

**The task's gate — "each claim is verified against the binary, not against
DD text" — found four defects:**

1. the `{n,m}` heap overflow (journal 290, fixed);
2. `(ab){2}` silently matching the wrong thing (290, now rejected);
3. every non-ASCII literal double-encoded (291, fixed);
4. `[[ $s =~ ^((a))$ ]]` rejected as "expected `]]`" — a literal `((` goes
   to the arithmetic lexer. **Filed:
   `notes/tasks/regex-literal-double-paren.md`.**

**Corrections to the source material** (recorded in the closed task):
DD#39's top section still denies `{n,m}` exists; the task's own note on
which files diverge from upstream Plan 9 **omits `lib9.h`**, whose
eight-line ASCII-only `chartorune` stub is the reason the dialect is
byte-oriented — the most user-visible divergence in the section.

**One divergence is not like the others:** every unsupported construct is
*rejected* (SB5023) except `[[:alpha:]]`, which is read as an ordinary
bracket expression plus the literal `:]` and so **silently fails to
match**. Called out explicitly in the man page.

**Open for DG (2 new):** `regex-counted-group.md` (keep the `(ab){2}`
rejection, or support it by replaying a token span) and
`regex-literal-double-paren.md` (the diagnostic misdiagnoses regardless;
whether to *accept* `((` in a bare word is the surface question).

**Next:** back to the `notes/tasks/` queue — `audit-taint-bypass-report`
(P2; its "Shape (to design with DG)" section is an explicit design fork,
to be deferred not decided), then the P3 converter cohort.

## ══ STATUS (2026-09-13) — `echo café` printed `cafÃ©` ══

**Journal 291.** Sweep **4398 passing, 9 skipped, 0 failing**, 0 flips.

Every non-ASCII byte in sash source outside single quotes was
**double-encoded**. `push(byte as char)` is a Latin-1 decode followed by a
UTF-8 encode: `0xC3` → `U+00C3` → `C3 83`. So `café` (5 bytes) became 7 bytes
in the binary. Three sites: the double-quoted body, the bare-word scanner,
and `$'…'`. Assignments and expansions inherited it.

`scan_single_quoted` was correct all along — it slices the source range and
`from_utf8_lossy`s once. The right pattern was thirty lines above a broken
site.

**Why it survived:** the corruption is a function, so everything internally
consistent still works. `[[ $v = café ]]` passed the whole time — both sides
mangled identically. Only leaving the program's own frame catches it:
**writing bytes out**, or **asking the length**. `$(strlen café)` returned 7.

Fix: a `push_source_char` helper (whole UTF-8 sequence, `from_utf8_lossy`),
three call sites. `$'…'` needed more — `\xc3\xa9` is two escapes spelling
one character, so it now accumulates `Vec<u8>` and decodes once, which makes
it agree with bash.

Specs `utf8.1`–`utf8.11` in `spec/misc.sh` (plan 65→76). `utf8.3` pins the
single-quote path that was *already right*; `utf8.6` the byte length;
`utf8.10` the equality that kept passing and proved nothing.

**Found by** the `docs-regex-dialect-writeup` gate ("verify each claim
against the binary"). `café =~ ^caf.$` matched neither the byte model nor
the character model — an answer fitting *no* coherent model means the error
is further back than the thing under test. A C harness against the shipped
`regcomp.c` returned the right answer, which cleared the engine and pointed
at the subject.

**Dialect consequence:** regex is now coherently **byte**-oriented
(`lib9.h`'s `chartorune` is an ASCII-only stub), so `.` matches one byte
where bash matches one character. A real divergence, for the manpage.

**Note for DG:** the complexity-gate means have ratcheted well below the
numbers AGENTS.md quotes. Over `sash/src` they are now **cc 4.04 / cog 2.06**
(1733 functions), not 12.46 / 6.58 — so the 3× bar is **12.1 / 6.2**. The
gate caught `scan_ansi_c_quoted` at cc=6/cog=7 on this commit; refactored
(extracted `scan_ansi_c_bytes` + `at_escape_lead`), not bypassed. AGENTS.md's
quoted means are stale.

**Next:** still `docs-regex-dialect-writeup` — probing is done, the
`.SH REGULAR EXPRESSIONS` section of `docs/sash.1` is not yet written.

## ══ STATUS (2026-09-13) — regex: a heap overflow reachable from the environment ══

**Journal 290.** Sweep **4376 passing, 9 skipped, 0 failing**, 0 flips.
One task filed for DG.

Probing the regex dialect for `docs-regex-dialect-writeup` (whose gate is
"verify each claim against the binary") crashed the compiler: `a{1,17}`
tripped a glibc malloc assertion. `a{1,16}` was fine.

`regcomp1` sizes its arena at `6*sizeof(Reinst)*strlen(s)` — Pike's
per-character budget, an invariant every operator *he* wrote maintains by
construction. DD#39's `{n,m}` is ours and doesn't: six characters in,
seventeen atom copies out. The budget was never re-derived when the feature
landed.

**It was not just a compiler crash.** A pattern need not be a literal —
`p=$(getenv RE); [[ $s =~ $p ]]` compiles, and `sash-rt` ships the same
`regcomp.c`. `RE='a{1,17}'` against 34 `a`s printed **`N`** (sash-rt's own
heap absorbs the overflow instead of asserting); `a{2,32}` killed the
process. Environment-controlled input, silently wrong answers.

Three fixes in `sash-rt/csrc/regcomp.c` + `sash/build.rs`:

- **Sizing**: `6*strlen + 8`, plus 128 slots per `{`.
- **Guard** (the one that matters): `newinst` bounds-checks `freep` against a
  new `freeend` and `rcerror`s. The arithmetic is now a performance decision,
  not a correctness one — a future expanding construct that skips the budget
  gets a diagnostic, not a corrupt heap.
- **Build**: `sash/build.rs` had no `cargo:rerun-if-changed` for the
  out-of-package C sources and was linking a **stale object** — the first
  post-fix test still crashed identically. `sash-rt/build.rs` already had the
  loop, with a comment recording the same incident under DD#63.

**Second bug found on the way**: `(ab){2}` → `N`, bash `Y`. `(` and `)` are
operators, so `prev_atom_start` after a group points at its *last* atom; the
expansion built `(ab)b`. Shipping, silent. Now **rejected** with a
diagnostic; supporting it means replaying a token span, which changes the
language surface → **`notes/tasks/regex-counted-group.md`, DG's call.**

Specs `39.20`–`39.31` (plan 68→80). `39.27` first passed for the wrong
reason — `RE` was exported in the probe shell; `env -u RE` failed it. It now
sets and unsets its own env.

**Next:** resume `docs-regex-dialect-writeup` — the probing is largely done
(measured: `{n,m}` both bounds capped at 32, `{n,}` unbounded; `\d \w \s \D
\W \S` supported, a deliberate divergence from bash ERE; no lookaround, no
backrefs, no POSIX classes, no `\b`), and needs writing into a
`.SH REGULAR EXPRESSIONS` section of `docs/sash.1`.

## ══ STATUS (2026-09-13) — converter positionals: three bugs behind a feature ══

**Journal 289.** Sweep **4352 passing, 9 skipped, 0 failing**, 0 flips.
Two tasks closed, one filed for DG.

Implementing DG's 2026-07-08 resolution on
`converter-positional-use-needs-bless` (hoist + placeholder attest + MANUAL)
turned up three shipped bugs in `sash-convert`:

- **`$#` printed −1.** It emitted `$(($(argc) - 1))`; a command substitution
  is not an arithmetic operand, and until journal 286 the arith parser
  recovered it as literal `0`. The DD#58 sexpr `(argc)` is the spelling.
- **`for x in "$@"` ran one extra iteration**, with the program's own path as
  the first value: `$(argv)` includes argv[0] (DD#60), bash's `"$@"` is
  argv[1:]. `conv.q1` had pinned the bug.
- **`echo "$1 $1"` lost the space.** Stock tree-sitter-bash 0.25 gives the
  2nd+ `simple_expansion` in a string a span including the preceding
  whitespace, so rewriting it deleted the separator — in every expansion
  family the converter touches, `$HOME` included.

Found by **round-tripping**: `conv.x1`-`conv.x9` convert, compile the
conversion, run it with `a b c`, and compare to bash. Every existing substring
assertion passed the whole time — `57.13b` asserted the exact text that
printed −1.

Also closed `oracle-blessall-mapfile` (done 2026-08-20; filed in the retired
`bless` vocabulary) and corrected the stale native-guard comment it left in
`spec/taint.sh`.

**For DG:** `converter-splat-empty-case.md` — `cmd "$@"` has no faithful
conversion, because SB5032 (which postdates the resolution) requires stating
the empty case and neither spelling matches bash's "run with zero arguments".
Currently MANUAL-only.

**Next up.** `docs-regex-dialect-writeup` (P2), `audit-taint-bypass-report`
(P2, shape needs DG), `s9-manpage`/`docs/sash.1` retired flag surface.

## ══ STATUS (2026-09-13) — SB5011 told two different mistakes apart ══

**Journal 288.** Sweep **4342 passing, 9 skipped, 0 failing**, 0 flips.
One task closed.

`a-b=1` was told "a variable or expansion can't be resolved at compile
time" — naming a cause the line does not contain. A malformed assignment
and a DD#23 runtime-chosen command (`$x foo`) both reach the same
non-`Literal`-head arm, because an unquoted `=` splits the word into a
`Concat`, and the arm could only describe one of them.

Split by the question that actually separates them: is there an expansion
to the *left* of the `=`? If not, the word is a failed assignment and the
target is named (`` `a-b` is not a valid variable name… ``). If so
(`9x[$i]=1`), the word really does contain an expansion and keeps the
original wording. Code stays SB5011 — the existing `aw.*` specs assert the
code and deliberately not the text, which is what left the text free.

Specs `aw.21`–`aw.26`; fail-first predicted 21/22/23 failing **and**
24/25/26 passing, and got exactly that.

**Next up.** The `next` queue: `audit-taint-bypass-report` (P2 hardening),
`converter-positional-use-needs-bless` (P2), `docs-regex-dialect-writeup`,
`oracle-blessall-mapfile`, `s9-manpage` (`docs/sash.1` still documents the
retired flag surface).

## ══ STATUS (2026-09-13) — `--trace` was dead on aarch64; startup unified ══

**Journal 287.** Sweep **4330 passing, 9 skipped, 0 failing**, 0 flips.
One task closed.

aarch64 `_start` called `sash_init_heap` but never `sash_trace_init`, so
`sash_trace_flag` stayed 0 and every `--trace` site tested false: a binary
that was larger, slower and **silent**. x86_64 called both.

Fixed at the cause rather than the symptom. The startup sequence now lives
in one plain-Rust `sash_startup()`; each `_start` makes a single
`call`/`bl` to it and keeps only the ABI-specific part (stashing
`argc`/`argv`/`envp`). The two assembly bodies no longer enumerate init
steps, so they cannot disagree about them again. Both symbols were
referenced *only* from those two blocks, so the C backend needed no change.

**Gate not met, and not meetable here:** "`tr.1`–`tr.3` pass on aarch64".
No aarch64 hardware, no `qemu-user`, no `aarch64-linux-gnu-gcc` — and
`sash-rt` vendors C through `cc-rs`, so even a compile-only cross build
needs that toolchain, which needs root. The aarch64 edit
(`bl sash_init_heap` → `bl sash_startup`) is reviewed, not run. **Someone
should run `spec/trace.sh` on real aarch64 hardware.** Recorded in the task
file rather than ticked.

x86_64 *is* fully verified: `SASH_TRACE=1` on a `--trace` binary emits
`+ branch → if.then`, unset emits nothing, and every compiled binary in the
suite now goes through `sash_startup` — which is what makes 0 flips
meaningful here.

No spec added: the suite runs on x86_64, so a spec for an aarch64-only bug
would pass for the wrong reason, and the drift is now prevented by
construction rather than detected.

## ══ STATUS (2026-09-13) — arithmetic: two silent-`0` recoveries removed ══

**Journal 286.** Sweep **4330 passing, 9 skipped, 0 failing**, 0 flips
(4304 + 26). One task closed, one filed for DG.

**`$(( 1 + ))` printed `1`.** Two independent defects, each masking the
other's reach:

| input | before | now |
|---|---|---|
| `$(( 1 + ))` | **1** | SB4020 |
| `$(( 1 +* 2 ))` | **0** | SB4020 |
| `"$(( @@@ ))"` | **0** | SB4035 |
| `"$(( $(f) + 1 ))"` | **0** | SB4020 |

1. `parse_prefix_atom` **consumed** an unusable operand token and returned
   `0`. Consuming it is why DD#72's `reject_trailing_token` — the check
   built for exactly this — never fired.
2. `emit_string_part` did `parse_arith_str(…).unwrap_or(Literal(0))`, so
   *every* arith diagnostic inside a double-quoted string was discarded.
   The same expression outside quotes was already a hard error, so the two
   positions disagreed.

Fixing (2) meant making the parameter-expansion layer fallible — 13
signatures through `parse_default_word` / `build_sed_sub` /
`parse_disposition_operator` / `try_subscript` / `parse_parameter_expansion`.
That layer was infallible by construction and a doc comment said so, but
the parser threads `Result<_, Diagnostic>` everywhere else, so this applies
an existing convention rather than choosing a new one.

`for (( ;; ))` reaches the arith parser as bare separators and needed an
explicit all-separators early return, or the new EOF error would have
broken it. Specs 31.49–31.52 pin that and the other unchanged behaviour.

**Found while re-measuring `cmdsub-followups-taint-intrinsics-c`**, which
is now **closed** — all three gates met. The C backend's Phase-4 cmdsub-exec
TODO is implemented (zero `TODO` markers emitted; `$(/usr/bin/printf hi)`
captures `hi` in C, byte-identical to native), and `$(getpid)`-class
intrinsics work in every cmdsub position.

**For DG:** `arith-cmdsub-support.md` — bash evaluates `$(( $(cmd) + 1 ))`;
sash now rejects it *consistently* rather than in four of five positions,
but that is not the same as having decided it. Three options, with the taint
consequence spelled out.

**Also re-measured, already fixed by DD#107's heap work** (no code change
needed): `bug-capture-64k` (200 KB capture exact; the 64 KiB boundary exact
at 65535/65536/65537) and `bug-cmdsub-trailing-newlines` (all trailing
newlines stripped, matching bash).

## ══ STATUS (2026-09-13) — converter: `${#…}` conflation + missing DD#97 keyword ══

**Journal 285.** Sweep **4304 passing, 9 skipped, 0 failing**, 0 flips
(4292 + 12 new; convert.sh is native-only). Two task files closed.

**`${#a}` and `${#a[@]}` are different questions** — element 0's byte length
vs. the element count — and `sash-convert` answered both `$(len a)`, printing
3 where bash prints 1. All four rows now land:

| bash | means | emits |
|---|---|---|
| `${#s}` | byte length of a scalar | `$(strlen $s)` |
| `${#a}` | byte length of element 0 | `$(strlen ${a[0]})` |
| `${#a[@]}`/`[*]` | element count | `$(len a)` |
| `${#a[i]}` | byte length of element i | `$(strlen ${a[i]})` |

The bare row is the only one whose meaning depends on the variable's *type*,
so a pre-pass collects the names bash declares as collections. **A wrong guess
cannot pass silently** — `$a` on a collection is SB5064, `s[0]` on a scalar is
SB5068, both naming the fix — which is the only reason a heuristic was
acceptable here.

**The filing had rotted.** It said the scalar case was "correct only by
coincidence"; by today DD#75 had made `len` collection-only, so `$(len s)` was
**SB4030 — output that does not compile**. The compiler split and the
converter was never told.

**A spec was holding the bug in place.** `57.13c` asserted *both* halves of
the conflation as the expectation, so the suite was green throughout. Rewritten
in place — same id, correct assertion, comment saying what it used to require.
Note it produces **no flip**: it passed before and passes now, only the meaning
changed. "0 flips" cannot see that.

**Second bug, found by demanding an end-to-end demonstration.** The converted
program would not compile, for an unrelated reason: **the converter never
introduced the DD#97 `array`/`map` keyword**. Explicit `declare -a` worked, but
bash declares *implicitly* — `a=(x y z)`, `a[0]=x`, `mapfile -t a`, `read -a a`
— which is how most real scripts build an array. Three symptoms, one cause, one
fix: introduce the keyword at first mention, inline as a zero-width insert for
a literal (so patches on words inside it still apply), on its own line for the
forms that cannot take it inline. Filed as
`notes/tasks/bug-convert-no-collection-keyword.md`.

Round trip now matches bash exactly (`1 3 3 1 5`). Fail-first predicted ten
failures on revert and observed exactly those ten.

## ══ STATUS (2026-09-13) — `$(<file)` no longer execs `/usr/bin/cat` ══

**Journal 284.** Sweep **4292 passing, 9 skipped, 0 failing**, 0 flips —
4270 + 22 new rows (`sl.1`–`sl.11` in both modes). Task
`dollar-lt-file-forks` closed.

**Unblocked without noticing.** The task was blocked on `sash_read_all`'s
64 KiB silent truncation. That closed 2026-09-13 with DD#107 (256 MB heap,
real error handling). Re-checking the blocker before starting the work is
what found it.

**`$(<file)` was literally `$(cat file)`** — synthesised in
`parse_slurp_form`, with the comment saying so. The spelling exists in bash
*because* it does not fork; ours forked, execed `/usr/bin/cat`, and embedded
that path in a "static" binary. That is a runtime dependency on the target
machine (and, under DD#23, on the build host) that nothing in the source
mentions. DD#99 had since put it on the common path — every migrated
`mapfile -t` is `s=$(<f); matchall …`.

Now a first-class node: `ExprKind::SlurpFile` → `SsaValue::SlurpFile` → **one
`sash_slurp_file` call in both backends**. It delegates the trailing-newline
strip to `sash_capture_drain` — the same code the general `$( )` capture uses
— so the backends cannot drift, and `bug-cmdsub-trailing-newlines` will change
both paths at once if it ever lands. Same move as the `[[ ]]` comparators.

**Gates:** 1 execve (was 2), zero clone/pipe; value byte-identical (5/1/10/0);
zero `/usr/bin/cat` in either backend's binary; **still tainted**, spanned
SB5020.

**Two more bugs the desugar was hiding.**

1. **Argument injection via filename.** A file named `-n`: `$(<-n)` became
   `cat -n`, `cat` read **stdin** instead, slurp yielded nothing. bash yields
   the contents. A direct `open` has no argv to inject into. This is `sl.11`,
   and it was found only because fail-first was weak (1 of 10) and I went
   looking for a sharper discriminator.
2. **A taint hole I introduced, caught by a before/after matrix.**
   `[[ $(<f) = x ]]` was rejected before and silently **accepted** after —
   `check_expr_taint` has a `_` fall-through. No existing test would have
   said a word. Fixed; pinned as `sl.8`.

**Exhaustive matches paid for themselves.** The new variants broke 6 compile
sites — including `ssa_taint.rs`, which *forced* the taint call: `SlurpFile`'s
child is the **path**, so the any-tainted-composite rule would have judged
`$(<f.txt)` **clean**. The six integration points with `_` fall-throughs all
failed **silently** and each had to be found by running something.

**Complexity gate, two fair catches and one artifact.** The new variant forced
an arm into three dispatchers. `expr_to_string` (cc 17) and `resolve_expr`
(cog 7 — two `for` loops and an `if let` inside arms) were fair and are now
9/1 and 10/1. `expr_to_value` was cc 13 / **cog 1** — thirteen delegating
arms, nothing to simplify — and had to be split anyway, because the
dispatcher exemption (`delta ≤ -15`) is unreachable for a pure dispatcher
until cc ≥ 16 while rejection starts at 12.1. **Nine functions sit in that
dead zone.** Filed with a recommendation:
`notes/tasks/complexity-gate-dispatcher-dead-zone.md`. Gate not touched.

**For DG:** `notes/tasks/dollar-lt-file-policy.md` — `$(<f)` on a missing
file **aborts** (bash warns and yields `""`); behaviour preserved, but it is
now our decision and wants a DD. Also whether `$(<f)` should word-split in
`for-in` at all, and a diagnostic wording change there.

## ══ STATUS (2026-09-13) — fail-open C arms closed; `<&`/`<>` implemented ══

**Journal 283.** Sweep **4270 passing, 9 skipped, 0 failing**, 0 flips (all 22
new tests are additions, nothing changed status). c **2066 → 2085**, native
**2166 → 2185**.

**Three bugs found, all the same shape.** DG asked a scoping question — *what
native and C work remains before the test redux?* — and re-measuring the
backlog against the live binary (rather than trusting the filings) turned up
three fail-open `/* TODO */` arms in `emit_c.rs`: arms that emit a comment,
return an empty value, compile cleanly, and **assert a value the compiler
never computed**. The earlier P1 (`bf96d25`) had covered only command dispatch.

**1. `[[ a < b ]]` was constant `false` in c — five operators, not two.**
Native's `lower_test_binary` implements `<`, `>`, `-nt`, `-ot`, `-ef`; c
handled only `=`/`==`/`!=`/`=~`. Fixed via the **same runtime comparators
native uses** (`sash_strcmp`, plus newly-externed `sash_file_newer` /
`sash_file_same_dev`) so the backends cannot drift. `ft.37`–`ft.46`
(`spec/filetest.sh`, plan 36 → 46). Fail-first failed **exactly 6 of 8** —
`ft.38` and `ft.44` are negative cases, which a constant-`false` arm answers
right by accident.

Byte ordering is now pinned: **2026-09-13 addendum to DD#62** (byte compare,
`LC_ALL=C`, so `[[ Z < a ]]` is true — `ft.40`). While writing it I went to
record `-nt` as whole-seconds and measured instead: it is **nanosecond**
(timespec) in bash and in both our backends. `ft.45`/`ft.46` pin it.

**2. The DD#23 head check never ran inside `$( )`.** `$c hi` is SB5011 in
statement position, but `$($c hi)` was *accepted* → native runtime-abort,
c silent empty capture. `check_cmd_in_stmt` only visited `StmtKind::Command`
and `child_bodies()` is empty for Assignment/Declaration/Command, so cmdsub
bodies (which live inside **expressions**) were never walked. The first fix
over-rejected: the full `check_commands` turned `$(getenv HOME)` into SB5073
("value intrinsic in statement position"), right in statement position and
wrong inside `$( )`. **Only the head-literal rule transfers.** `23.20`–`23.24`
(plan 92 → 97); `23.24` pins the distinction, `23.22` pins single-reporting.

**3. `<&` and `<>` were accepted and silently did nothing in BOTH backends.**
DG chose implement. `<&N` is the same op as `>&N` (`dup2`), differing only in
default fd, so one emitter (`emit_dup_fd`) serves both; `<>` needed runtime
flag code `3 = O_RDWR|O_CREAT` (create, **don't** truncate). Testing against
bash then found a second bug: **`<>` defaults to fd 0, not 1** — it is a `<`
form. With fd 1 it overwrote the head of the file and looked plausible.
`ex.12`–`ex.15` (`spec/exec.sh`, plan 19 → 23); `ex.14` is the fd-0 test.

**The remaining arms were closed by enumeration, not probing.** `SsaValue` has
28 variants; c handled 21; of the 8 missing, 3 are intercepted upstream and 5
are exactly native's ICE set. Those 5 plus an ICE terminal, and the same for
`ir_expr_as_str`, the param-expansion dispatch, `emit_cmdsub` and the redirect
fall-through — **`emit_c.rs` now has no fail-open arm**. Nothing hit an ICE
across 4270 tests.

**Also closed:** `c-argv-index-isint-parity` (P3) — `emit_c_argv_value` now
routes through `lower_var_as_int_c` instead of hard-coding
`sash_atoi({iv}_ptr,{iv}_len)`, which named slots an int var does not have.
Verified byte-identical C output today (`__fi` is still a string var), so it
is parity, not behaviour.

**Tasks closed:** `bug-c-string-ordering-always-false` (P1),
`fail-open-todo-arms-remaining` (P2), `bug-fd-dup-input-redirects-broken`
(P2), `c-argv-index-isint-parity` (P3).

**Standing lesson.** All three bugs were found by measuring a construct nobody
had pinned. `[[ a < b ]]` had no spec test in any backend, ever, across 4270
tests. **The absence of a spec test is not evidence of correctness — it is the
absence of evidence.**

**Deferred by DG, not forgotten:** `ssa-taint-full-backstop` (explicitly
deferred hardening), `heap-multi-region-growth` (undecided),
`join-dynamic-separator` (would retire SB5074 / reverse DD#106 — its own
session).

## ══ STATUS (2026-09-13) — DD#55 channel complete in c; PATH twins closed ══

**Journal 282.** Sweep **4232 passing, 9 skipped, 0 failing**. 17 flips (55.30
un-gated + 16 new tests). c **2057 → 2066**, native **2158 → 2166**.

**55.30 fixed, and two more paths behind it.** The filed gap was one missing
`sash_emsg_status` call on c's external-abort path. Fixing it exposed that
errors.sh had eight *inline* `{ ! in_modes native || … }` message guards — not
skips, so the tests ran in c but never asserted the message. Five went green
immediately; the other three were different abort paths: **command
substitution** (`emit_cmdsub_exec_c`) and **pipeline** (`emit_pipeline`), both
aborting with the right status and no diagnostic. Both fixed.

The pipeline case names the **failing** stage, so the name rides the
first-nonzero fold alongside the status (c's version of native materializing
all stage names for `wait_for_children`). `55.35` (`- /bin/true | /bin/false`)
is the proof: it must report the second stage, not the suppressed first.

**errors.sh now has zero mode guards and passes 181/181 in both backends.**

**PATH twins closed (P1, DG's scope B).** `VALUE_ONLY_INTRINSICS`' doc comment
claimed the other value intrinsics "already get SB5011 in statement position".
SB5011 is *command not found* — it only fires when PATH lookup misses. So
`basename`, `dirname` and `join` silently forked their coreutils twins, and
the rest were safe only because this host has no `/usr/bin/len`. All 11 are
now in **both** `BUILTINS` (never PATH-resolved) and `VALUE_ONLY_INTRINSICS`
(statement position = SB5073). `via.13-18` pin three names with a real twin
and three without, so the property is host-independent rather than
accidentally true.

**Tasks closed:** `value-intrinsics-exec-path-twin`, `c-dd55-abort-channel`,
`dd55-abort-channel-remaining`.

**Repo hygiene:** four spec harness artifacts (`out`, `err`, `.input.sh`, a
superseded `31.4.sh.draft.2`) had been tracked in git; removed, with a scoped
`.gitignore` block so an interrupted run cannot put them back.

**The 9 remaining skips are not c defects:** `104.x` and `57.46x` invoke the
`sash` CLI directly so `SASH_MODE` never reaches them, and `32.x` asserts an
ELF section only the Cranelift-linked binary has.

## ══ STATUS (2026-09-13) — the taint backstop never ran on the c path ══

**Journal 281.** Sweep **4215 passing, 10 skipped, 0 failing**. 16 flips, all
skipped→passing. c coverage **2041 → 2057**; skips **26 → 10**.

DG's directive was *fix p1 and plan, audit 26*. All three done.

**The audit found a taint bypass.** `sw.4/5/8/9` run under `--no-taint-cst`
to exercise the SSA backstop in isolation. Native rejects all four; **c
printed the tainted values** (`/home/dg`, `/HOME/DG`, the hostname).
`emit_c_to_stdout` is five lines and never called `run_ssa_taint_gate` —
the gate had exactly one call site, extracted for `compile_native`'s
complexity budget, and the c path was never given it. Not exploitable under
default flags (the CST pass still rejects), but a backstop exists for when
layer 1 has a gap, and the c backend is the **differential oracle** — an
oracle running different gates is not comparing the same compiler. Fixed.

**16 of the 26 skips were stale.** Ten (`cov.26`–`cov.75`) measure
byte-identical in both backends; their guards only ever said "native.",
which was the mode the author ran, not a claim about c. `89.8` cited a bug
fixed in `29b0b43`; `55.31` cited a gap it does not have. That is **seven**
stale guards across three sessions, every one hiding something:

> A skip is a bug report that suppresses its own evidence. It never fails,
> so it never expires.

The 10 survivors now record what was measured and when; the one real gap
(`55.30`, missing only the second stderr line) cites `c-dd55-abort-channel`.

**P1 `fail-open-todo-arms-in-emit-c` closed.** DG's challenge — *"why would
there be unsupported in C backend?"* — retired the task's recommended option
(A). No DD sanctions a c subset, so a fallback arm is either a real gap (a
bug) or an impossible state (an ICE); "unsupported" would have invented a
third category. `emit_command_body` (41 `BuiltinCmd` variants) and
`emit_inst` (17 `SsaInst`) are now exhaustive with ICE terminals, verified
by a dummy variant producing `E0004`.

**Next up:** `value-intrinsics-exec-path-twin` (P1, DG approved scope B) —
`join`/`basename`/`dirname` PATH-resolve and exec the system binary in
statement position; the others escape only because no twin happens to exist,
making the safe set host-dependent.

## ══ STATUS (2026-09-13) — compound stages in c; a blank line ate the input ══

**Journal 280.** Sweep **4199 passing, 26 skipped, 0 failing**. All 29 flips
intended (two un-gates recovered, 27 new tests); **+29**, skips down from 28.

| # | bug | scope | was |
|---|---|---|---|
| 7 | compound pipeline stage unimplemented | c backend | `fn f { … } \| cat` → `sash: bug: bare command passed to sash_exec`, rc 127/141 |
| 8 | `readline` EOF written to `__exit_status` | c backend | the loop branches on `__test_result` → condition constant → **infinite loop**, not the filed "extra iteration" |
| 9 | `readline` EOF tested with `len > 0` | **native** | a blank line read as EOF → **silent truncation** of the input at the first one |

**Bug 9 is the serious one.** `sash_read_line` returns `(NULL, 0)` at true EOF
but `(non-NULL, 0)` for an empty line, and says so in a comment. Native tested
the *length*, which cannot tell them apart:

```
printf "a\n\nb\n" | while IFS= read -r x; do echo "[$x]"; done
bash:   [a] [] [b]
native: [a]            ← exit 0, no diagnostic
```

**DD#99 settles it without asking DG:** the DD fixes `readline` as
*byte-for-byte* `IFS= read -r`, in those words, and that succeeds on an empty
line. Decision-tree step 1. Both backends now test `ptr != NULL` and both match
bash.

Backend-vs-backend parity could not have caught this: **both** backends were
wrong, in opposite directions, in the same six lines — native stopped a line
early, c never stopped. Bash was the only oracle, and only for an input (an
interior blank line) that no spec contained.

Bug 7 was three lines: the c emitter already lays every SSA block down as a
`_blk_<id>:` label, so native's `jump(compound_cl)` has an exact analogue in
`goto _blk_<id>`. Finding bug 8 cost the rest of the session — it surfaced only
because un-gating `pipe_functions.sh` left one straggler, and the straggler was
not about compound stages at all.

**Two more stale native-only guards** (`wordsplit.sh:rl.7`,
`commands.sh:14.46`), both gated on bug 8, both un-gated. That is **five** in
three sessions. `14.46`'s comment named the mechanism — "does not terminate
under the c backend" — so the hang was documented in the repo for as long as it
existed. **A skip is a silent bug report**: it suppresses the evidence that
would refute it, so it never expires. When a guard encodes a known defect, the
defect needs a task and the guard should point at it.

**Un-gated:** `pipe_functions.sh` (11), `rl.7`, `14.46`. **New specs:**
`pf.12`–`pf.15`, `rt99.15`–`rt99.18` (`rt99.18` is labelled a control — it
passed unfixed, because the one-shot `readline` never consulted the predicate).

Remaining native-only spec files are all legitimate: `convert.sh` (88,
backend-independent), `trace.sh` (3, `--trace` is native-only by design).

**For DG:**
- `fail-open-todo-arms-in-emit-c` (P1) — still awaiting a call. The `_ =>` arm
  emits a C *comment* for an unimplemented builtin, so the statement silently
  vanishes. Caused three of the nine bugs.
- **New:** `spec/wordsplit.sh` declares `plan=118` and runs 123. Pre-existing
  and unrelated, so untouched — but `plan` exists to catch silently lost tests
  and a stale one has stopped doing that. Suggest a sweep of every spec file's
  plan count as its own task.

## ══ STATUS (2026-09-13) — END routing, DD#89 bare exit, brace un-gated ══

**Journal 279.** Sweep **4170 passing, 28 skipped, 0 failing**. One flip, the
intended one (`c:56.31 skipped → passing`); **+31**.

| # | bug | scope | was |
|---|---|---|---|
| 5 | `exit N` skipped the END block | c backend | `END { }` + `exit 5` ran no cleanup; native routes through END |
| 6 | bare `exit` kept the previous status | c backend | violates **DD#89** ("no argument → `exit 0`"); `- /bin/false; exit` → 1 in c, 0 in native |
| — | `brace.sh` native-only guard | spec | premise was never true — 27/27 pass in c, first try |

Bug #6 was **in the same six lines** as #5 and nothing pointed at it: the C
emitter's doc comment asserted "bare `exit` keeps the previous status", which
is the rule DD#89 decided against, and the code implemented the comment.

`brace.sh`'s guard is the second stale native-only gate in two sessions
(`exec.sh` was the first). A native-only guard is a *claim* about the backend,
and because the guard suppresses the evidence, nothing re-checks it.

**Measured, already filed, left alone:** the c backend emits no DD#55
`<argv0>: <cmd>: command failed (exit N)` diagnostic for an external-command
abort (it does emit the `cd:` errno one). That is
`dd55-abort-channel-remaining`; `spec/errors.sh` guards those stderr
assertions `native`-only on purpose.

**Next:** `c-compound-pipeline-stage` — a `fn` or `{ }` group as a pipeline
stage reaches `sash_exec("__compound_5", …)` in c and dies with `sash: bug:
bare command passed to sash_exec`. Native routes it through the DD#14
`__compound_<id>` mechanism; the C emitter has no handling for it. This is a
feature, not a one-line divergence, and it unblocks `pipe_functions.sh` (11
tests).

## ══ STATUS (2026-09-13) — four bugs behind one un-gating ══

**C-backend `exec`/`close`/`unset m[k]` implemented; a heap-corruption bug in
the map runtime fixed.** Journal 278. Sweep **4138 passing, 29 skipped, 0
failing**, zero flips, **+31 specs**.

DG's direction was "known bugs, worst first, then the sweep." The worst filed
bug (`c-exec-support`) was real; fixing it and dropping `spec/exec.sh`'s
native-only guard surfaced three more.

| # | bug | scope | was |
|---|---|---|---|
| 1 | `BuiltinCmd::Exec` unimplemented in c | c backend | `exec CMD` ran nothing; `exec > file` un-did its own redirect |
| 2 | `BuiltinCmd::Close` unimplemented in c | c backend | `close 9` exited 0 instead of aborting with EBADF |
| 3 | `BuiltinCmd::TableDelete` unimplemented in c | c backend | `unset m[k]` a silent no-op |
| 4 | `TABLE_DELETED` sentinel mishandled | **runtime, both backends** | `get` faulted (EFAULT); `set` wrote through a wild pointer → **SIGSEGV** |

**#4 is the serious one.** `table_find_slot_inner` returns the first DELETED
(−2) slot when a key is absent, but `sash_table_get` tested only for EMPTY
(−1) and `sash_table_set` tested `!= EMPTY` — so the sentinel was used as a
live entry index. Reachable from ordinary shell code (set a key, unset it,
repeat) and live in **both** backends. Both guards now test `idx < 0`.

It hid because a *never-set* key is fine — the bug needs an absent key whose
probe chain crosses a deleted slot. And bug #3 masked it in c: spec `80.23`
*passed* in c while both were broken, because a no-op `unset` left the entry
live.

`spec/exec.sh` is now un-gated (19 tests in c). New specs `80.22`–`80.27`
(both modes) pin delete / re-set / churn against bash.

**Filed, not fixed — needs a DG call:** `fail-open-todo-arms-in-emit-c` (P1).
Bugs #1–#3 all came from `emit_c`'s `_ =>` arm emitting a *TODO comment* for
an unimplemented builtin, so the C compiles and the statement silently
vanishes — a fail-open default in a fail-closed compiler. Three options
written up (hard error / runtime abort / exhaustive match); (C)+(A) compose
and are the only ones that prevent recurrence rather than report it.

**Next, per DG's order:** `bug-c-end-exit` (`END { }` skipped when the main
body ends via `exit N`, c only), then `c-compound-pipeline-stage` (a user
function as a pipeline stage → rc 127 in c, unblocks `pipe_functions.sh`),
then un-gate `brace.sh` (27 tests; all eight constructs already verified
identical).

## ══ STATUS (2026-09-13) — a pure capture costs zero syscalls ══

**`$(f)` over a pure body is now a userspace buffer.** Journal 277. Sweep
**4107 passing, 29 skipped, 0 failing**, zero flips, +4 specs.

A 1000-line captured function: **3014 syscalls → 4** (`execve`, one heap
`mmap`, and the two writes of the program's own final `echo`). The capture
itself is zero.

**DG's purity rule, implemented in `transform/capture_purity.rs`:** a site is
disqualified iff a fork inherits *this* capture's stdout. A body that merely
wraps an external (`fn emit { /bin/cat f; }`) is out; a body that consumes
external output through an inner `$( )` and yields its value with `echo` is
**in**, because the inner capture hands its child a pipe of its own. Impure
means: a pipeline, a redirect, or a command head containing `/` (DD#23).
Calls recurse into function bodies; a visiting set makes a malformed program
fall back to impure rather than hang the compiler.

**The runtime is a stack, not a flag.** An impure capture can nest inside a
pure one, and it really does redirect fd 1 — so `sash_capture_start` pushes a
*transparent* frame that suspends diversion, popped on end. Depth is static
(SB5037 bans recursion), so a fixed array suffices.

**Journal 276's objection was right about its mechanism and wrong about the
idea.** "The sink must stay an fd" holds only if the decision is made at
*runtime* on the fd number. Made at emission time, the compiler already knows
whether the body forks or redirects, and both objections evaporate.

**One observable changed, deliberately and visibly.** A buffered site never
redirects fd 1, so a pure body probing its own stdout sees the caller's real
stdout (PIPE) where an impure one still sees a memfd (NOTPIPE). I proposed
concealing this by disqualifying bodies that mention `/dev/stdout` or `-t`.
DG rejected it — *"it should be observable rather than a taboo we only do
when we won't be caught."* `cap.12` and `cap.17` pin both halves side by
side. The exact sweep delta was two rows out of 4132.

**`cap.18` was vacuous on its first draft** and passed with the mechanism it
guards deleted. An external's writes reach fd 1 from a forked child and never
pass through `sash_write`, so they cannot be mis-diverted either way; the
test needs an *in-process* write to have anything to get wrong. Fail-first is
not a formality even when the test looks like one.

## ══ STATUS (2026-09-13) — the capture sink stops being a pipe ══

**The `$(f)` deadlock is fixed.** Journal 276. Sweep **4095 passing, 29
skipped, 0 failing**, zero flips, +12 specs. **One divergence from bash
awaits DG's sign-off** (below).

DG did not pick from the four options in the filing. He asked whether a
pipe could be replaced by a buffer when both sides are non-external. We
have no such optimization — I looked — and the precondition would not have
covered the bug anyway, because the filing had the scope wrong.

**The deadlock is not about in-process writes.** An *external* command
inside a captured body hangs identically: a forked child inherits fd 1 and
blocks on the same 64 KiB wall. It is about **nobody reading**, and the
reader cannot start until the body returns. Any fix scoped to in-process
writes — including the literal userspace-buffer reading — leaves that case
hanging.

**The sink has to stay an fd.** A userspace buffer keyed on "fd == 1"
swallows `echo hidden > f` inside a captured body, because fd 1 was
temporarily rebound by the redirect and the buffer cannot know. Nothing
else in the program would notice. Pinned as `cap.10`.

So the requirement is narrower than "replace the pipe with a buffer": it is
**keep the fd, lose the blocking**. `memfd_create` is exactly that — a
regular file with no filesystem behind it, so writes never block, yet it is
still an fd. It is also option B from the filing with its cost deleted: no
path, no `$TMPDIR`, nothing to clean up on the abort paths.

Entirely inside `sash-rt`. The `(saved, handle)` pair is unchanged, so the
SSA instructions, Cranelift signatures, and `emit_c` are untouched — c-mode
picked the fix up for free. `dup2` makes fd 1 and the handle share one file
description, so a child's writes advance the same offset the parent's do
and the interleaving is correct by construction (`cap.11`).

| case                               | before        | after    | bash     |
|------------------------------------|---------------|----------|----------|
| in-process ~500 KB (`cap.7`)       | hang (rc 124) | `508889` | `508889` |
| external in body 100 KB (`cap.8`)  | hang (rc 124) | `100000` | `100000` |

### A documented divergence

The sink is a regular file, so a body probing its own stdout sees one.
`[[ -p /dev/stdout ]]` inside `$(f)` is `NOTPIPE`; bash says `PIPE`, and so
did sash before this change — measured on both sides, not reasoned about.
`-t 1` still agrees. I escalated this as needing sign-off on the grounds
that the north star is byte-identical output; **that goal was retired a
while back** (bash has too many indefensible quirks), so it is a choice to
document, not a debt. Pinned as `cap.12`. The alternatives that keep the
pipe do not survive anyway: a larger pipe moves the wall to 1 MB and leaves
it a wall; forking the body is bash-exact but costs assignment visibility.

### Next: capture sites get their own identity

DG's two pushbacks converge on one change.

**The syscall objection was aimed at the wrong thing.** Measured, 1000-line
captured function: pipe = **3014** syscalls total, memfd = **3014**. The
memfd traded `pipe`+`close` for `memfd_create`+`lseek` and touched nothing
else. The cost is the body's **3002 `write`s** — three per `echo "line-$i"`
— which happen capture or no capture. So the buffer DG wants is not a rival
to the memfd; it is a different and larger prize: **zero** syscalls, not
twelve. memfd keeps the general case correct; a buffer makes the provable
case free.

My objection to the buffer (keying on "fd == 1" swallows a redirect)
evaporates when the choice is made at *emission* time. Bodies are inlined,
so codegen already knows which `sash_write`s sit inside a capture — and can
decline the fast path for a body holding a redirect or an external command.
The runtime never guesses what fd 1 means because nobody asks it.

**And nested capture is just a sequencing bug**, as DG read it. SB5037 says
it outright: recursion is rejected *because* sash inlines functions, and the
one legal self tail-call lowers to a loop. Depth is always static, so the
runtime stack I recommended was over-engineering; per-site slots suffice.
Filing corrected.

Both items want the same plumbing: **give each capture site its own
identity and its own state.** Either one alone builds most of the other.

### New bug found

**Nested function capture is broken.** `h() { i=$(g); }; o=$(h)` fails with
`sash: read failed`. Codegen holds the capture handles in a *single* pair of
slots, so the inner capture clobbers the outer's. Pre-existing — identical
on the pipe runtime — and not from this work. P2, ready, options in
`bug-nested-function-capture-clobbers-handles`. It surfaced because a test
written to guard redirects happened to nest two captures.

### Pattern worth naming

Third filing in a row that undersold its own bug. This one described the
reproduction it happened to trip over rather than the class, and the
in-process framing pointed straight at the one fix that does not work.
Reproducing is necessary and not sufficient; the question after "does it
reproduce" is "what else is in this class".

## ══ STATUS (2026-09-13) — capture collapsed onto one drain ══

**Two capture bugs fixed; a third found and blocked on DG.**
Journal 275. Sweep **4083 passing, 29 skipped, 0 failing**, zero flips.

The filing said "truncate at 64 KiB". It was worse: the reader stopped,
the child took **SIGPIPE**, and DD#55 aborted the program with
`/bin/cat: command failed (exit 141)` — blaming the one component that
behaved correctly. Both backends. `$(cat 100kb)` now returns all of it.

Second: bash strips **every** trailing newline from `$( )`; two copies
stripped one, so `$(printf 'a\n\n\n')` was `a\n\n` where bash gives `a`.
A byte-for-byte divergence in something as ordinary as capturing output.

**There were four copies of that loop, not three.** `grep 65536` found a
fourth in the *compiler* — `codegen/value_handlers.rs` emitted ~45 lines
of inline Cranelift IR with the same two bugs, and it is the path most
native programs take. Only `sash_capture_end` stripped newlines correctly
(a `while` where the others had an `if`). All four now call one
`sash_capture_drain`, built on `sash_read_all`.

**`sash_capture` was dead** — declared, stored in `ExternFuncs`, never
called, and the field carried `#[allow(dead_code)]` so the compiler had
already noticed and been silenced. Superseded by `sash_capture_status`.
Deleted; the slot is now `capture_drain`.

### Blocked on DG

**`bug-fn-capture-pipe-deadlock`** (P2) — `$(f)` on a user function hangs
forever above 64 KiB. There is no fork: the body runs in-process writing
to a pipe nobody drains, so its own `write` blocks waiting for a reader
that cannot start until it returns. Pre-existing; the 64 KiB reader had
been hiding it (the truncation on that path was *unreachable*).

The fix is a fork with a semantic consequence: **A** fork the body — what
bash does, kills the deadlock, but a forked body's variable assignments
stop being visible to the caller, a language-surface change needing a DD.
**B** spill to a temp file — removes the limit without changing what
`$(f)` means (**recommended**). **D** detect and abort — not a fix, but
turns a hang into an honest error and composes with A or B later.

`cap.6` is labelled in the spec as a **non-regression guard**, not proof
of a fix: it passes on both sides, and its payload stays under the pipe
buffer because the larger case cannot run at all until the above lands.

### Next up

1. **Native** — `bug-convert-hash-array-conflation`,
   `dollar-lt-file-forks` (blocked), `bug-emit-c-exit-skips-end`,
   `cli-usage-and-unknown-flags` (its complexity prediction is stale:
   `parse_cli_args` is 2/1 today, not 11/10).
2. **The c gaps** — the 29 c-mode skips.
3. **The test work** — `r47-validate-rs-per-rule-tests`,
   `audit-string-keyed-builtin-dispatch`, `eliminate-ice-arms-via-types`,
   all three still awaiting honest re-estimation.

## ══ STATUS (2026-09-13) — heap capacity settled (DD#107) ══

**The heap is 256 MB, compiled in, `--heap-size=<size>` to change it.**
Journal 274. Sweep **4071 passing, 29 skipped, 0 failing**, zero flips.

`bug-heap-exhaustion-silent-segv` is **closed** — its fail-loudly half
shipped 2026-09-12, and DG has now taken option **B** for capacity. The
old ceiling was ~5 000 30-byte array elements (not "1 MB of payload" —
growth needs old array + new array + payload live at once). It absorbs
200 000 now. `MAP_NORESERVE` keeps hello-world's peak RSS at 340 kB,
unchanged from the 1 MB heap: the reservation costs address space.

**No `$SASH_HEAP`, deliberately.** DG: *"we go out of our way to prevent
things like PATH from altering runtime behavior so compiled-in seems
better."* A heap size in the environment would let anything that controls
the environment shrink a binary's heap until it aborts, without touching
the program — the same reason bare `$HOME` is rejected in favour of
`$(getenv HOME)`. Mechanism: `heap4.c` defines `sash_heap_size` **weakly**
at 256 MB; `--heap-size` emits a **strong** definition into the generated
object (Cranelift data natively, a C global under `--emit-c`) and the
linker prefers it. Unused, the flag costs nothing.

`hp.1`-`hp.3` were rewritten to compile against `--heap-size=1M`. They
pinned exhaustion by building 30 000 elements, which now *succeeds* — they
would have stayed green while testing nothing. Exhaustion is now pinned to
a **stated** ceiling. `hp.6`/`hp.7` and `cli.13`/`cli.20`-`cli.23` are new.

**`SASH_TRACE` works and was already opt-in** (DG asked). `--trace`
compiles the sites in; `SASH_TRACE=1` activates them; without `--trace`
the variable is ignored — the environment can only switch on machinery the
build included. Branch/loop outcomes only (`tr.1`-`tr.3`, native); values
are unbuilt, which is what keeps DD#32 `partial`. **DD#32 said none of it
was built**, and the adversarial audit reported that back as a gap
(`audit-gemini-pro.md`); corrected, along with the task body.

### Found while answering that

- **`bug-aarch64-trace-init-missing`** (P3) — aarch64's `_start` calls
  `sash_init_heap` but not `sash_trace_init`, so `--trace` compiles in
  trace sites that can never fire. x86_64 calls both. Filed, not fixed:
  different defect, different fix.
- **The heap filing's option A was false.** It claimed heap_4 supports
  `vPortDefineHeapRegions`-style multi-region init. The vendored copy has
  one `xStart`/`pxEnd` pair and a one-region `sash_heap_init`. Corrected
  in place; A is now `heap-multi-region-growth` (P3), still real, just
  allocator surgery rather than configuration.

### Next up

Unchanged from the 2026-09-13 list below, minus the heap item:

1. **Native** — the two `sash-rt` capture bugs (`sash_capture` /
   `sash_capture_status` / `sash_capture_end`: 64 KiB truncation and
   one-newline-vs-all strip, both to route through `sash_read_all`); then
   `bug-convert-hash-array-conflation`, `dollar-lt-file-forks` (blocked),
   `bug-emit-c-exit-skips-end`, `cli-usage-and-unknown-flags` — that last
   filing is **stale too**: it predicts a complexity-gate trip at
   `parse_cli_args` 11/10, but the function is 2/1 today.
2. **The c gaps** — the 29 c-mode skips.
3. **The test work** — `r47-validate-rs-per-rule-tests`,
   `audit-string-keyed-builtin-dispatch`, `eliminate-ice-arms-via-types`,
   all three still awaiting honest re-estimation.

## ══ STATUS (2026-09-13) — two silent coercions closed ══

**DD#105: a string intrinsic rejects an int-typed operand.**
**DD#106: `join`'s separator must be a compile-time literal.**
Journal 273. Sweep **4059 passing, 29 skipped, 0 failing**, zero flips.

**The "Next up" list below had aged.** Checked against the live binary
rather than the file: `bug-getenv-status-unvalidated` was already fixed
2026-09-12 (`$(getenv)` → SB4045, bare `getenv` → SB5073) and the
regex-NUL item is a documented limitation. The one real item, DG's
decision #3, had **no task file at all**.

**`toupper`/`tolower`/`strlen`/`basename`/`dirname`/`match`/`sub`/`gsub`
silently coerced an int operand** — `$(strlen $n)` on `int n=42`
returned 2, the digit count of a rendering the source never asked for.
`join`/`len`/`${n:1:2}`/int-redirect-target already said SB4030; these
were the hole. DG chose the **broad** scope — key on the operand's
*type*, so `$(( … ))`, `$(getpid)` and `int array` elements are caught
too. `"$n"` stays legal as the explicit conversion, the same escape the
substring diagnostic already recommends.

Registering it cost a build: a command inside `$( )` is reached via
`check_subscript_types` → `check_subscript_in_stmt`, **not**
`check_command`. That is why `join`/`len`/`keys` appear in both — it
reads as redundancy until you skip it.

**`$(join a "$sep")` printed `x"$sep"y`** — the separator's source text,
quotes and all. A wrong answer, not a crash: the SSA operand is a
compile-time `String`, so `strip_word_quotes` fell back to rendering the
expression. DG chose reject-now (SB5074), measured against zero corpus
cost, with the dynamic separator filed as `join-dynamic-separator`.

**DG's question changed the fix.** Asked whether `strip_word_quotes` had
a legitimate use at all, the answer was no — so instead of diagnosing
the bad case and leaving the fallback for the next caller, the helper is
gone, replaced by `ir::literal_word_text` returning `Option<String>`
with no fallback. A constant-only slot can now say "this is not one"
instead of inventing one. `check_join_arity` → `check_join_form`,
because it now checks two things.

**The spec found a third bug.** Pinning `$(join a "")` exposed a backend
divergence: native concatenated, `--emit-c` joined with spaces.
`emit_c::arrjoin_separator` keyed on "is the operand empty", conflating
`$(join a)` (no separator → space) with `$(join a "")` (explicitly empty
→ concatenate). Native keyed on the *operator* and was right; c now
matches. Fixed — it was invisible from the native side and sits in the
backend queued as the next work area.

### Next up — DG's order, re-measured

1. **Native** — the two `sash-rt` capture bugs (64 KiB truncation;
   one-newline-vs-all strip, both routing through `sash_read_all`), and
   `bug-heap-exhaustion-silent-segv`, which is **blocked on DG**: the
   fail-loudly half is done, the capacity half (options A/B) needs a
   call. Then the P2 bugs still queued: `bug-convert-hash-array-conflation`,
   `dollar-lt-file-forks` (blocked), `bug-emit-c-exit-skips-end`,
   `cli-usage-and-unknown-flags` (will trip the complexity gate —
   refactor, don't skip).
2. **The c gaps** — the 29 c-mode skips, tracked by the ready `c-*`
   tasks.
3. **The test work** — `r47-validate-rs-per-rule-tests` and the other
   two filings still awaiting honest re-estimation.

## ══ STATUS (2026-09-13) — janet decoupled: `--emit-ir` in, backend out ══

**DG: split janet off entirely so native/c stop paying the janet tax.**
Done, in four steps DG specified: `--emit-ir`, tag `janet-babel`,
**DD#104**, removal. Journal 272.

**Measured before agreeing.** janet ran 1366 passing / **41 failing** /
**540 skipped**; native 2095/0/0, c 1920/0/27. Skip ratio **flat at
27.7–28.0% on every commit sampled** — so the tax was constant, not
accelerating (the weaker form of the claim; I said so). The fact that
decides it is different: **janet was not gating.** "41 failing (all
janet, unchanged)" appears in every sweep for months, and the pre-commit
hook checks fmt/clippy/complexity only. Full coupling cost, zero oracle
benefit.

**`--emit-ir` emits the IR as an inert janet data literal** — data, not
a program. Nothing to execute and nothing to version-match, so a
consumer weeks behind the compiler can still read a capture. That is
also why it was complete on day one: no semantics to defer, hence no
`(sash/TODO …)` — the flaw that stopped `--janet-map` rendering a
7-line program.

Two properties are load-bearing and a change breaking either is a bug:
**no `_` arm anywhere** (a new IR variant must be a compile error, not a
silent drop — including across the `expr` split, whose halves name each
other's variants and route to `unreachable!`), and **tags are IR node
names, never an interpretation** (`attest` is `{:kind :command :name
"attest"}`; interpretation belongs to the consumer).

Verified over the whole corpus, not by sampling: **1974 emissions, every
one parsed by janet** — after first catching that the initial zero came
from a probe that had barely fired.

**Removed:** `emit_janet.rs`, `emit_janet_map.rs`, `sash-rt.janet`,
`janet_tests.rs`, the flags, the `SASH_MODE`. Also `oracle.rs` +
`oracle_tests.rs` — neutral names, entirely janet, nothing else used
them. `cli.13` ("usage covers every advertised flag") failed on the
removal, which is what it is for. Old code preserved at tag
**`janet-babel`** (a quarry, not a branch — see its tag message).

**Sweep: 4019 passing, 0 failing, 31 skipped.** Zero flips.

### Both cleanups landed (DG: strip now; purge)

**The 300 `in_modes native,c` guards are gone** (`6d4eba1`) — with janet
removed they guarded nothing, so this was behaviour-neutral, and
verified as such: the sweep before and after was byte-identical, zero
status diffs, zero rows gained or lost. Four passes, not one regex: the
first pass's strict assertions caught `else skip_chk X; fi` one-liners
and, more dangerously, `run_sash '…'` payloads whose *string content*
holds `if`/`else`/`fi` tokens — a naive depth counter closes the guard
in the wrong place. The 13 it could not prove were done by hand. The ~35
`in_modes native` sites stay: those are real c-backend gaps (the 29
remaining skips).

**The 221 orphaned janet comments are cleaned** — every one referenced a
guard that no longer exists or a task now deferred. Comment-only,
mechanically verified (every added/removed line is a comment or blank),
and all 47 touched test-description lines confirmed intact.

**`spec.db` purged** (DG chose C over the self-cleaning view): janet
gone from `tests` and `runs` — 565k rows — plus 8 rows for tests deleted
earlier (15.1/15.2, 36.2/36.3) that were still reporting verdicts at
commits two and three back. `test_status` is now exactly the live suite:
**4013 passing, 29 skipped, 0 failing, 0 stale rows.**

Worth remembering: the phantom-row problem is *general*, not a janet
artefact. The view has no notion of a deleted test, so any test we
remove lingers with its last verdict. We purged rather than fixed the
view, so this will recur — stale rows are detectable by a `commit_hash`
older than the newest in `runs`.

### Next up — DG's order (SUPERSEDED — see the 273 section above)

**Finish the native work, then close the c gaps, then the test work.**
(The three native items named below were re-measured on 2026-09-13: two
were already resolved, and the third was unfiled. Use the list above.)

1. **Native** — `string-intrinsics-reject-int-vars` (DG's decision #3:
   `toupper $n` where `n=42` silently yields `"42"`; `join n ","` already
   errors SB4030 and is the model), the two `sash-rt` capture bugs
   (64 KiB truncation; one-newline-vs-all strip, both routing through
   `sash_read_all`), and the approved-but-unapplied `HEAP_SIZE` bump.
   Also still open from the backlog review: `bug-getenv-status-unvalidated`
   (P1), `bug-regex-blind-past-interior-nul` (P1).
2. **The c gaps** — the 29 c-mode skips, tracked by the eight ready
   `c-*` tasks, six of which were rescoped out of the old `c-janet-*`
   filings in `47af108`.
3. **The test work** — `r47-validate-rs-per-rule-tests` and the other
   two filings still awaiting honest re-estimation.

## ══ STATUS (2026-09-13) — dtoi/itod dropped; DD#103 ══

**DG: "dtoi and itod are YAGNI."** Removed — ~190 lines over 17 files,
all table rows plus two runtime functions. No removed-builtin table:
they are ordinary unknown tokens now (SB5011 everywhere). DD#36
superseded. Journal 271.

**Verifying the removal surfaced a pre-existing hole**, and DG reframed
it as a rule rather than a bug: *"Shouldn't any unrecognized symbol in
arithmetic context be an error except an assignment?"* Yes — and every
spelling already obeyed it (`$(( zz ))`, `$zz`, `${zz}`, `zz[0]`,
`a[zz]`, `(( zz ))` all SB5031) **except the DD#58 sexpr node**, where
`$(( (zz) ))` and `$(( (nosuchfn 1) ))` both silently yielded 0.
`validate.rs` abstained at that node while the transform resolved it
positively — two passes disagreeing, an unresolved name falling through
as a plausible constant. **DD#103** writes the rule down:

- nullary, not a callee → a variable read → **SB5031**
- with args, not a callee → not callable → **SB5011**
- a callee is an intrinsic or a user function (`resolve::is_intrinsic`)

`dtoi`/`itod` had been *masking* the with-args half: `(dtoi 2 3)` was
rejected only because `dtoi` was a **known** string producer.

**Suite: 5381 passing, 41 failing (all janet), 567 skipped. Zero
non-janet failures.** Flips vs the DD#102 baseline: `36.2`/`36.3`
retired, `sx.18`–`sx.23` new and passing — in all three modes. Fail-first
verified.

### I got the rule wrong first, and the suite caught it

My probe used `fn g(int a) -> int` — typed parameters, which fail in a
sexpr for an unrelated reason (SB4014) — so I concluded user functions
with arguments are not callable there and rejected them. Twelve specs
disagreed: DD#58's convention is positional `$1`/`$2`, and `16.2`,
`27.3`, `58.2`, `58.3`, `58.34`, `58.35` had been pinning `(add 3 4)` all
along. **A probe I write is a hypothesis; the suite is the record.**

### Also fixed

janet emitted `(x)` verbatim for nullary grouping, so janet called the
variable's value. `arith_var_to_janet` already made the right decision
for the bare spelling — one-line reuse, rather than guarding the new spec
to `native,c` and entrenching the gap.

### Still open from the native batch

`getenv-presence-handling` (awaiting DG on the forced abort) and
`dd55-while-condition-redirect-divergence` (DG's DD#55 call). The
approved `HEAP_SIZE` bump is applied-pending; `sash_capture`/
`sash_capture_status`/`sash_capture_end` still truncate at 64 KiB and
strip only one trailing newline (route through `sash_read_all`).

---

## ══ STATUS (2026-09-12) — the native batch, run unattended ══

**DG: "do the whole batch unattended."** Thirteen items from the backlog
review worked in priority order. **Eleven landed; two are blocked on DG.**
Fifteen commits, `cc9aea5..999e014`. Journal 270.

**Suite: 5194 → 5351 passing, 41 failing, 567 skipped. Zero non-janet
failures throughout.** All 41 failures are janet. Every fix was fail-first
verified (stash, rebuild, confirm the new specs fail, restore) — four times
that changed what got written.

### Five bugs nobody had filed, all found by probing the binary

- **Heap exhaustion was a silent SIGSEGV** (P1) — null returned, caller wrote
  through it. `$(<file)` was also truncating at 64 KiB.
- **The C backend was stubbing file tests entirely** — found while fixing the
  `-N` divergence, which was the smaller half of what was wrong.
- **janet's `write` forked `/usr/bin/write`** — no `BuiltinCmd::Write` arm had
  ever existed, so a builtin became the `wall`-style messaging program.
- **janet's lastpipe `| read ARRAY` bound a scalar** — `detect_lastpipe` keyed
  on `BuiltinCmd::from_name`, which maps both `read` and `readline` to `Read`.
- **`9x =~ s/a/b/` compiled to a working binary** — `parser.rs`'s private
  `is_identifier` never checked the first character, contradicting its own doc
  comment. Bash answers "command not found" to that line.

### Three more filings were wrong on contact

Journal 269 found nine of nineteen wrong at review time. Implementation added
three: `remove-retired-surface-implementations` confused a retired *source
spelling* (`echo -n`) with a live *internal lowering* (`write`);
`janet-read-a-element-count` blamed a trailing newline for a
scalar-vs-array binding; `bug-convert-hash-array-conflation` was filed as a
bug and is really a decision. **A filing is a snapshot and nothing ages it.**

### Recognition is not implementation

Removing `BuiltinCmd::Mapfile` looked free — nothing lowers it, the name stays
in `BUILTINS`. But `is_resolvable_external` classifies a head as external
exactly when `from_name` returns `Other`, so `mapfile` became a PATH lookup
and SB5011 preempted SB5060. The variant is back as recognition-only. **A
retired surface keeps its name and its diagnostic; everything between goes.**

### The recurring class is at eleven

Every bug this batch is "a value that could not be resolved resolved to a
plausible constant instead of failing" — null as address, missing opcode as
`false`, unknown builtin as external program, array as string, digit as name.
**Two copies of one predicate accounts for four of the eleven.** Three commits
are literally "one shared definition": the DD#55 condition predicate, the
`strlen` operand guard, `is_identifier`.

### Complexity gate: refactored three times, bypassed zero

`validate_stmt_ctx` (extracted `validate_declaration_stmt`),
`lower_pipe_while_read` (cog 16 → 0), the janet echo/write emitter
(`janet_echo_arg`). Note that touching a **call site** counts as touching the
function.

## ▸ TWO DECISIONS WAITING ON DG

Each blocks exactly one batch item and nothing else.

1. **Heap capacity** (`dollar-lt-file-forks`, `bug-heap-exhaustion-silent-segv`)
   — what should `$(<file)` do when the file exceeds the arena? (A) grow, or
   (B) fail loudly with a diagnostic.
2. **The `strlen` cell spelling** (`strlen-cell-operand-spelling`) — bash's
   `${#a}` means "bytes of element 0" and sash cannot ask that. `${a[0]}`
   already parses as a cell; it is rejected only because `classify_operand`
   buckets it as "not a name", which is correct for `len`/`join`/`keys` and
   open for `strlen`. Blocks the four-row converter mapping.

## ▸ NEXT UP

The `next` queue is **empty**. Remaining backlog is the janet-parity and
converter cohorts (~54 and ~12 filings), plus:

- `rust-tests-hang-on-retired-surfaces` — shrunk by this batch (the `read -d`,
  `read -f`, `mapfile_basic` and `${x^}` fixtures are gone) but not closed.
- `sb5011-misdiagnoses-malformed-assignment` (P4, new) — `a-b=1` is blamed on
  "a variable or expansion" it does not contain. Pre-existing; measured.
- `guide-architecture-stale-cst`, `bug-regex-blind-past-interior-nul` (P1),
  `janet-map-from-array` (P2).
- A second adversarial review is still unscheduled; validator bypass remains
  the highest-value target.

## ══ STATUS (2026-09-12) — native backlog reviewed end to end ══

**DG: "let's review each one at a time, finalize any decisions, normalize
status, and queue for the next implementation batch."** All 19 open
native-only tasks reviewed against the **live binary and the current index**,
not against their filings. 25 ledger rows now (19 + 5 filed during the pass +
1 bug found at the end), all reviewed. **13 queued to `next`, 5 of them bugs.**

**Nine of nineteen filings were materially wrong.** Not carelessness — a
filing is a snapshot of a moving tree and nothing ages it. `validate.rs` grew
from a filed 1087 lines to **7222** (6.6×) while a task sat open describing its
size. `audit-string-keyed-builtin-dispatch` went 36 → **153** sites;
`eliminate-ice-arms-via-types` went 23 → **16**. Two tasks dissolved entirely
(`dd75-args-model-polish`: 4 of 5 items already done;
`attest-map-form-validation-only`: done 2026-09-10, docs-only remainder).
`refactor-dd93-rename-complexity` had 4 of 7 violators self-clear.

**Four bugs, none of them the task being reviewed.** All four came from
probing the binary to check a filing's claim. `getenv`/`status` are the only
two intrinsics with **no `type_check.rs` entry** — where arity is enforced —
while also being exempt from SB5011, so `$(getenv)` compiles and the binary
dies on `bug: bare command passed to sash_exec` (P1). `$(<file)` does not
merely fork: `parse_slurp_form` **desugars it to `$(cat file)` at parse time**,
so the static binary embeds `/usr/bin/cat` and makes two execve calls (P3→P2).
`sash-convert` conflates hashes and arrays (P2). And `[[ -N f ]]` **disagrees
with bash** (P2).

**`-N` is the one that matters.** Sweeping all 18 file-test operators × 3
target types against bash found exactly one divergence. `sash-rt` uses strict
`>` with a nanosecond tiebreak; bash uses `>=` on whole seconds. A freshly
written, never-read file has mtime **exactly** equal to atime — same second,
same nanosecond — which is precisely the case `-N` exists to report true. sash
answers `no`, compiles clean, no diagnostic. That is the
**accepted-but-divergent** class `fuzz-validator` has argued for months is the
real threat and the one the adversarial audit's threat model misses — found by
hand, by exactly the differential-vs-bash method that task proposes, in about
ninety seconds. Nothing has been built for it (no `sash/fuzz/`, no fuzz deps).
Strongest argument yet; recorded in the filing.

**DG's rulings.** The complexity ratchet **holds at 3× indefinitely** — I
brought the "3× → 2× → 1.5×" plan back with numbers (80 violators at 3×,
**357 at 2×, 518 at 1.5×**) and the plan does not survive contact;
grandfather-on-modify is the mechanism working, not a backlog. **No reserved
builtin names** — drop the removed-builtin table, unknown tokens in command
position get the ordinary SB5011 (three separate "nothing is lost" proofs:
`source`/`.` keep SB4001 via `check_dynexec`, the `export` arm is already dead
code, `lexer.rs:1706-1711` are no-ops). **`--no-taint` documented,
`--no-taint-cst` a testing aid**, accepted but unadvertised. **getenv presence
is a lookup status, not a default arg** — *"the intrinsic isn't failing, but
the lookup of a particular env var fails"*, which dissolves the SB4025
objection that intrinsics cannot fail. Forced consequence recorded, not
deferred: DD#42's *"no silent empty return"* means an uncaught
`$(getenv NOPE)` must **abort** — a breaking change, awaiting DG.

**The CLI task is three defects, not one.** Two contradictory usage strings,
no `--help` (falls through to the error branch — stderr, exit 1), and unknown
`--` flags **silently ignored**: `sash --emit-C h.sash` and `sash --jannet
h.sash` both produce a native ELF and exit 0. `docs/sash.1` is complete and
correct — the binary is what disagrees with it, and `sash.1` already lists
this in its own table of inconsistencies. `parse_cli_args` is cc=11/**cog=10**
against a 12.46/6.58 gate, so the fix **will** trip the complexity gate;
recorded in the filing so it is known before starting rather than discovered
at commit time.

**Next up.** The `next` batch: 5 bugs (`bug-getenv-status-unvalidated` P1,
`bug-file-test-N-diverges-from-bash` P2, `bug-convert-hash-array-conflation`
P2, `dollar-lt-file-forks` P2, `cli-usage-and-unknown-flags` P2) then the P4
cleanups. `remove-dead-case-first-runtime` is **not standalone** —
`sash/tests/c_tests.rs:413` still exercises the retired `${var^}`, so it
sequences with the retired-surface cohort. Three tasks left unqueued pending
honest re-estimation (`r47-validate-rs-per-rule-tests`,
`audit-string-keyed-builtin-dispatch`, `eliminate-ice-arms-via-types`).
`extract-instctx-setup` stays deferred — unchanged, and the 2026-06-08
investigation already proved the recommended fix cannot work (E0503).

Journal: `notes/journal/269-native-backlog-review.md`.

## ══ STATUS (2026-09-11) — three P2 native bugs fixed ══

**DG: "fix P2 native bugs now".** The census had turned up exactly three P2s
in the native compiler; all three are fixed. Sweep **5001 / 38 / 512**, zero
non-janet failures, zero flips against `ea0e7a8` and `0ca316a`, all 48 new
test rows passing or deliberately skipped. **Native P2 count is now 0.**

**SB5021's advice named the retired `mapfile` and called a map an array.** The
validator had no idea `m` was a map — `SlotCx` carries no collection kind. I
was about to write the `-A`/`-a` classification when I found
`type_check::collect_collection_kinds` already doing exactly it; made it
`pub(crate)` and reused it. All four write shapes now route through one
`tainted_into_collection_msg`. Every route the new text recommends was run
end-to-end *before* the wording was written — which is the point, since the
old text's advice did not work.

**`match S P ARR` did not auto-declare its out-array.** The filing named three
dispatch sites; there were five, and `matchall` (added by DD#63 *after* the
filing) had the same gap in two of them. All five now guard on
`c.args.len() == 3`, so the boolean form still binds nothing.

**The read family ignored a failed redirect open.** `read a < nosuch` exited 0
with an empty array — a mistyped path was indistinguishable from an empty
file. Root: `SsaInst::ReadArray` carried no redirect list, so the redirect was
applied *around* it by a push/pop pair built from the unguarded
`emit_redirects`. Two fixes were possible; what settled it was finding that
`RedirectPush` is reachable **only** via bulk reads (SB4021/SB4046 reject
redirects everywhere else), and that push/pop bracket an arbitrary span with no
body to skip — so DD#55's "skip the body, flow the status to the condition"
could not be expressed there at all. `ReadArray` now carries its own redirects,
symmetric with `ReadLine`. Also: `emit_block_insts` scanned only
`Command`/`Pipeline` when deciding `in_condition`, so the reads never saw it.

**Three things surfaced and were filed rather than fixed.** A failed open in a
`while` **condition** aborts in native and is caught in c — reproduces at
`c1e9807` with a plain `echo`, so it predates this, and whether a `while`
condition should catch is **DG's DD#55 call**
(`dd55-while-condition-redirect-divergence`). janet writes the auto-declaring
`var` inside the if-condition (`janet-match-outarray-var-in-condition`), and
janet drops input redirects on the read family entirely, reading stdin instead
(`janet-read-family-ignores-input-redirect`). Both janet bugs reproduce on
untouched code paths; both are probably subsumed by `emit-janet-from-ssa`.

**Fail-first, honestly.** 55.60-62, 60.56-59 and 63.mi1-mi4 all failed against
a stashed build. 55.63-55.66 and 63.mi5 passed pre-fix — annotated in the spec
as regression pins, not discriminators. (55.63/55.64 cannot discriminate: the
harness runs binaries with `</dev/null`.)

### Next up

DG's standing direction is to **return to pruning the backlog**. Open P2s after
this: janet 13, c/both 4, converter 1, hardening/audit 3, language surface 3,
docs 1 — native 0. P1s: `rust-tests-hang-on-retired-surfaces` (blocks step 1
of the completion checklist; two forks open for DG) and
`bug-regex-blind-past-interior-nul`.

## ══ STATUS (2026-09-11) — sash-convert learns the DD#99 read family ══

**DG picked `sash-convert-read-dd99`** off the P1 list: the only remaining P1
outside the janet backend, and the only one leaving a shipped tool emitting
programs that do not compile. Done. Sweep **4964 / 38 / 501**, zero non-janet
failures, zero flips against `0ca316a`.

**Two premises in the filing were wrong, and finding that out was the work.**

1. *"The converter has no command-rewriting path — it only inserts annotation
   lines."* False. `let` → `(( ))` and `trap` → `signal N IGN` have both
   replaced whole command nodes via `Patch::replace_node` since DD#57. I wrote
   that sentence during stage 6 without checking and then read my own filing as
   evidence. It made the task look like an addition; it was an edit.

2. *The mapping table copied DD#99's "What is retired" table.* That table is a
   **sash → sash** rename map — its "26 in-tree `read -a` uses" were *sash*
   `read -a`, which already split on NUL. Bash's `read -a arr` splits **one
   line on IFS**; sash's `read arr` slurps **all of stdin on NUL**. Different
   extent, different separator, no error. A migration table and a conversion
   table look identical and are not interchangeable.

**Third finding: `readline` is `IFS= read -r`, not `read -r`.** Measured on
`"   spaced   "` — bash gives `[spaced]`, sash gives `[   spaced   ]`. Bash
strips leading/trailing IFS whitespace. The careful bash idiom
`while IFS= read -r line` therefore converts *exactly*; the sloppy one does not.

**DG's ruling — scope B, trim=check, keep ATTEST.** Converted: `read [-r] VAR`
→ `readline VAR` (+ `# CHECK:` unless `IFS=`-prefixed, which is dropped with
the command it modified); `mapfile/readarray -t A [< f]` →
`/usr/bin/tr "\n" "\0" [< f] | read A`. Annotated `# MANUAL:`: multi-target
`read a b c`, `-a`/`-ra`, `-p/-s/-n/-N/-t/-u/-d`, bare `read` (`$REPLY`), and
`mapfile` without `-t`. The line that decides the cut: the `mapfile` rewrites
guess **nothing**, while `-a` and multi-target must assume `IFS` is default
whitespace — and a wrong guess is the failure mode the converter exists to
prevent.

`mapfile -t a < f` is a `redirected_statement`; replacing only the command
would re-bind `< f` to `read`, so `tr` would get the terminal and `read` the
file with its newlines intact. The rewrite replaces the enclosing statement and
carries the redirect onto `tr`. Two redirects, or a `>`, and it declines.

**Also fixed: hint ordering.** `# ATTEST:` is placed *after* its line
(`line + 1`), `# MANUAL:`/`# CHECK:` *before* (`line`) — the same insertion
point, resolved by push order, so a MANUAL for line N+1 printed above the
ATTEST for line N and read as a complaint about the preceding command.
`inject_attest_hints` now runs first. Pre-existing; this task made it common.
The ATTEST hint also now names the **form** matching the target kind, since a
`mapfile` target attested with the scalar form still fails SB5020.

**Unrelated defect found while verifying: `janet:60.35` had not run in two
days.** `spec/taint.sh`'s `in_modes native,c` else-branch skip list omitted
`60.35`, so in janet mode it was neither run nor skipped — and `test_status`,
which takes each test's `MAX(rowid)` run, kept reporting **passing** from
`9b74bde`. The plan mismatch can't catch this: `tally` deliberately treats
`plan != index` as advisory because `in_modes` guards make per-mode divergence
legitimate. Queried the whole 1.4M-row table for others
(`committed_at < date(latest)`) — exactly one. Fixed; all three modes now
178/178.

**Left where they were, both separately filed:** `done < f` (SB4021,
`converter-compound-postfix`) and `${names[@]}` (`converter-retire-bracket-splat`,
which also misfires a `${var@X}` MANUAL note on it — noted on that task). So a
converted `while read` loop *over a file* still doesn't compile end to end; the
piped form does, and spec 57.73 pins it convert → attest → compile → run.

**Specs:** `convert.sh` 57.37 inverted from asserting the passthrough to
asserting the rewrite, 57.56–57.59 and 57.61–57.73 added; all 18 fail against
the unpatched converter. `convert_tests.rs` +9 → 27.

**Next:** the P1 list is now three janet bugs —
`janet-array-append-string-concat`, `janet-command-substitution-capture-broken`,
`janet-compound-if-condition-ignored`.

**Deferred by DG:** the `len`/`strlen` split, as its own pass (absorbs the
DD#100 Part D splat-operand removal — ~35 spec sites, SB5025's message,
`sash-convert`).

**Still open for DG:** `rust-tests-hang-on-retired-surfaces` — rewrite the
stale fixtures, or make `codegen_tests` run `validate` first?

## ══ STATUS (2026-09-11) — `fn` top-level, `const` not in a loop, splat = values ══

**DG, in one message:** functions must be top-level (counter-argument?),
`const` shouldn't be within a loop, splat/join on a map should deal with
values not keys. All three land here. Sweep **4947 / 38 / 501**, zero
non-janet failures, zero regressions against `4d7d792`; Rust suites diff empty.

**1. SB5071 — `fn` must be top-level.** `if [[ 1 = 2 ]] { fn f { … } }; f`
runs `f` in sash and is "command not found" in bash: the branch never
executes and the function is defined anyway. Same for `false && fn` and for
`fn` as a pipeline stage. It was also the **last remaining taint bypass** —
top-level and fn-in-fn both raise SB5020 on raw `$1`, fn-in-`if` leaked argv.
Now closed by construction.

I argued for the narrow form (allow `fn` inside `fn`) and **withdrew it**: the
argument was cost-based, not principled. DD#86 makes scope positional with no
block scope, so control flow creates no scope and the hoisting is DD#86-
*correct* — the defect is that the placement reads like a scope that doesn't
exist, and that applies to `fn`-in-`fn` identically. Specs 18.3/18.4 were
pinning the wart; they are inverted into rejections.

**2. SB5072 — `const` in a loop body.** Falls out of the same no-block-scope
model: the binding is re-executed, i.e. a `const` reassigned. It was already
rejected, but only by the spanless `VersionState::define` backstop at `1:1` —
and in `while`'s case only as a side effect of the `readonly_vars` leak filed
yesterday. Spec 86.27 asserts the diagnostic is *not* at `:1:1:` so the
backstop can't quietly reclaim it. `const` in a **function** body stays legal
(DD#86 Phase 2 fixed that leak properly; 86.30 pins it).

**3. A splat yields values, not indices.** `join m ,` → `v1,v2` and
`for v in @m` → `v1 v2` were already the behaviour, pinned by nothing. DG:
"arrays and maps differ only in how they're indexed, splat of an array doesn't
produce indices, why would splat of a map…?" Keys are indices, not data. The
DD#75 `len` amendment had already blessed this a row above the line that
contradicted it.

SB5061 (`join m ,` rejected) **survives on new reasoning**: not "silent
mis-pairing" — that presupposed the pairs reading — but **no round trip**.
`(k1 v1 k2 v2)` → map (2 entries) → values `(v1 v2)` → map (1 entry, keyed
`v1`). `a ≠ b`, `m ≠ m2`, silently. Spec af.10 demonstrates it.

**Three traps recorded in journal 266:** a spec passing *vacuously* (af.10's
`rc != 0` short-circuit was firing because the program never compiled);
re-indenting a spec block when wrapping it in `in_modes` breaks
whitespace-sensitive `run_sash` bodies; and a baseline regression query is
vacuous when HEAD hasn't moved, since a dirty re-run records under the same
`commit_hash`.

**Closed:** `taint-fn-in-toplevel-control-flow`, `while-two-pass-readonly-leak`
(DG's ruling makes the leak irrelevant), and `join-map-reject` — **reversed**,
it was not a bug.

**Correction to the entry below:** it lists
`bug-regex-blind-past-interior-nul` as an open P1. There is no such bug —
the filing is `limitation-regex-stops-at-interior-nul`, `kind: limitation`,
P3, `documented-limitation`. Open bug census is **35**, and no P1 remains in
the native compiler; the three P1 bugs left are all janet.

**Next:** resume the P1 prune — `sash-convert-read-dd99` (a task, not a bug:
the converter emits `read -r`, which no longer compiles) and three janet bugs
(`janet-array-append-string-concat`,
`janet-command-substitution-capture-broken`,
`janet-compound-if-condition-ignored`).

**Deferred by DG:** the `len`/`strlen` split, as its own pass. It absorbs the
DD#100 Part D splat-operand removal (~35 spec sites, SB5025's message,
`sash-convert`).

**Still open for DG:** `rust-tests-hang-on-retired-surfaces` — rewrite the
stale fixtures, or make `codegen_tests` run `validate` first?

## ══ STATUS (2026-09-11) — three P1 bugs fixed, one of them a segfault ══

**DG: "fix P1s now."** Three landed. Sweep **4892 / 39 / 495**, zero non-janet
failures, zero flips against `5a5a2c0`.

**1. Segfault — an auto-declared bulk-read target inside a `while` body.**
Found while testing the counter-argument to #2, which is the whole reason DG
asks for counter-arguments. `while … { read ARR }` wrote into a read-only
string literal. `lower_while` lowers the body **twice** — pass 1 discovers
loop-carried variables, `insts.clear()` discards it, pass 2 re-lowers — and it
rolled back the instructions but not `state`. `autodeclare_bulk_target` is
guard-then-emit, so pass 1 registered `ARR` and emitted its empty-collection
`Assign`, the clear threw the `Assign` away, and pass 2 skipped on the guard.
`ARR_ptr` kept its `""` initializer. Fixed by snapshotting `collections` /
`assoc_arrays` around the two passes, exactly as `enter_fn_body_scope` already
does. `lower_while` is the only two-pass lowering, which is why `for` never
reproduced it.

**2. SB5069 — `while read ARR` rejected.** DG called it nonsensical and asked
for the counter-argument first. The honest one was bash's `while read -a arr`
per-line field split — and it collapses: `read $s ARR` splits on **NUL**, not
whitespace, so there is no per-line field split in the language for this to be
the spelling of. `readline v` is the entire replacement, and the diagnostic
says so. `if read ARR` still works; only iterating on a slurp is the error.

**3. SB5070 — a splat in a scalar operand slot.** `$(gsub @a s x)` silently
joined the array and returned a string. Arity checked out because a splat is
one *syntactic* argument. The filing named `gsub`; measuring found **all
seven** scalar-operand intrinsics doing it — `sub`, `gsub`, `toupper`,
`tolower`, `basename`, `dirname`, `match`. Fixing only the filed one would
have left five identical silent wrong answers behind a green suite. `len` /
`join` / `keys` are excluded on principle: they take a collection *by name*,
so `@a` there is not a reinterpretation.

**4. A flaky test, found by the verification.** The name-for-name Rust-suite
diff showed one new failure, `janet_tests:janet_const` — which turned out to be
a pre-existing race, not a regression: two tests passed the same `name` to the
temp-path helper, so under `cargo test`'s parallel threads one ran the other's
program. The stashed baseline failed it two runs in three; a *count-only*
comparison would have read 141-vs-140 and sent me hunting a regression in the
transform. Renamed, and the helpers now `claim()` each path and panic on reuse
so the next collision is a named failure rather than a wrong answer. Five
suites now diff **empty** against baseline: 140 pre-existing failures, same
names, before and after.

**Filed, not fixed:** `while-two-pass-readonly-leak` (P3). `readonly_vars`
leaks across the same two passes, but restoring it would make `while` **accept**
`const` in a loop body while `for` still rejects it — and `for` is right on the
merits there, since it unrolls. Which way both should go is a language question,
so it is DG's. Today they agree; the filing records that `while`'s half rests on
a bug.

**Next:** remaining P1s — `sash-convert-read-dd99` (converter still emits
`read -r`, so its output no longer compiles), `bug-regex-blind-past-interior-nul`,
and four janet P1s.

**Still open for DG:** `rust-tests-hang-on-retired-surfaces` — rewrite the stale
fixtures, or make `codegen_tests` run `validate` first?

## ══ STATUS (2026-09-11) — bare name to a user function stays a string ══

**DG settled the second of three batched decisions.**
`bare-collection-name-operand-asymmetry` is closed — none of the three options
it offered:

> I'm going to just live with the current state that non-sigiled [names]
> provided to a user function are strings, since there is no expectation that
> they should operate on bare variables/collections, unlike `len`, `join`, et
> al. With use, I may reconsider or have an inspiration.

The asymmetry is real — `show a` gets the string `"a"` while `len a` gets the
collection, in one program, with no diagnostic — but it is **regular** once the
two callee kinds are distinguished. `len`/`join`/`keys` are language surface
whose operand slot is *declared* to name a collection. A user function takes
values, and creates no more expectation that `a` names a variable than `f
hello` does that `hello` does.

DD#100 supplied the missing half in the meantime: `=a` is now an unambiguous
way to say "bind the collection", so the bare form no longer carries two jobs.

Explicitly **provisional** — DG left the door open. So `spec/collparam.sh`
100.35–100.37 pin the behaviour rather than bless it, and the DD#100
clarification says so. Sweep **4839 / 39 / 485**, zero non-janet failures,
zero regressions.

**One decision still open:** `rust-tests-hang-on-retired-surfaces` — rewrite
the stale fixtures, or make `codegen_tests` run `validate` first? It bypasses
validation today, so DD#99-retired surfaces compile into binaries that never
terminate (this is why `mk test` cannot currently pass, and why the Rust
suites carry 141 pre-existing failures).

**Next:** resume the P1 prune — `bug-sed-op-on-array-empties-it` part (b)
(`$(gsub @a …)` silently joins the splat), `bug-while-read-array-nonterminating`
(`while read l` is a silent no-op, no diagnostic), `sash-convert-read-dd99`.

## ══ STATUS (2026-09-11) — regex: DG rules the dialect deliberate ══

**DG settled one of the three batched decisions.** `bug-regex-literal-dash-in-
bracket` is **not a bug**:

> the regex library is chosen on faith in the author, Rob Pike. It represents
> his view of a sane pre-feature-creep implementation. POSIX compliance is not
> a goal. dash can be used in a character class with a `\`. I have made small
> changes for convenience, not compliance. It deserves a full write-up in the
> future docs/man page

The filing assumed POSIX and bash define what sash's regex must accept. They
do not. It also offered "document the workaround" as an option **without
testing it** — `\-` works in all three positions and agrees with bash, which
falsifies the filing's central claim that sash accepts a strictly smaller
language. A P1 with a three-way fork was one backslash.

**Confirming it found two stale claims in DD#39.** Its prose said `\d`/`\w`
were unsupported (the 2026-07-08 clarification restated it explicitly). Both
halves were wrong: the vendored `bldshortclass` provides `\d \w \s \D \W
\S`, and `{n,m}` works with a cap of 32. Only lookaround is genuinely absent.
The DD had drifted from the implementation twice in one paragraph — so the
dialect now has specs (`wordsplit.sh` 39.12–39.16) and the next drift fails a
test instead of misleading a reader.

Also established for the write-up: `regexec.c` and `regaux.c` are
**byte-identical** to upstream `contrib/plan9port-regexp/`. Every local change
is in `regcomp.c` (~223 lines), both additions to the *parser*; the matcher is
untouched.

No code change. Sweep **4833 / 39 / 482**, zero non-janet failures, zero
regressions.

**Still awaiting DG (2 of the 3):**
- `rust-tests-hang-on-retired-surfaces` — rewrite the stale fixtures, or make
  `codegen_tests` run `validate` first (it bypasses it today, so DD#99-retired
  surfaces compile into non-terminating binaries).
- `bare-collection-name-operand-asymmetry` — now that `=a` exists, should a
  bare collection name passed to a user function be rejected (directing to
  `=a` or `"a"`)? Today `len a` reads the collection while `show a` passes the
  literal `"a"`, silently.

**Next:** resume the P1 prune — `bug-sed-op-on-array-empties-it` part (b)
(`$(gsub @a …)` silently joins the splat), `bug-while-read-array-nonterminating`
(`while read l` is a silent no-op), `sash-convert-read-dd99`.

## ══ STATUS (2026-09-12) — SB5068: a subscript on a scalar is an error ══

**On `hand-parser`.** `bug-scalar-subscript-segfault` is closed. The filed
reproducer was one spelling; probing found **seven faces of one defect** —
six segfaults (read, dynamic index, `[[ ]]` operand, arithmetic operand, loop
variable, subscript assignment) and a compiler **ICE** for an `int` subject.
All seven are now SB5068 at compile time.

The hole was `check_subscript_key_kind` in `type_check.rs`, which returns
early when the subject has no collection kind — it validates the key, and a
scalar subject has nothing to validate against. The subscript then lowered to
a table read, dereferencing a scalar's `char*` as a `SashTable*`.

**The fix deliberately does not live where the hole is.** `TypeEnv.vars` is
filled from Assignments and Declarations only, so a `for` variable is never in
it; a check at that site would have fixed six faces and left the seventh
segfaulting, with a green suite. SB5068 lives in `validate.rs` next to
SB5064 — its exact dual — and uses `collect_scope_names`, which does include
loop variables.

The allow-set is not the declaration set: `argv g` and `read ARR` introduce a
collection with **no declaration**, so `${g[0]}` and `${ARR[0]}` are valid.
It is declarations ∪ bulk-fill targets ∪ slots `A`–`E` ∪ `args`, deliberately
over-approximated — a false negative leaves a latent crash, a false positive
breaks a working script, and only one of those can be routed around.

`walk_arith_subscripts` moved from `type_check.rs` to `ir.rs`; the arith face
needed its own walk site, as did the assignment face (the subject is a
statement field, in no expression).

**The control found a second bug.** `${args[0]}` under `--janet` emits
`(get args 0)` against a `defn` binding no such symbol. Pre-existing
(confirmed by stashing to `fe71aa1`; this change is validate-only). Filed as
`janet-args-subscript-unbound` — same family as `janet-collection-parameters`.

**Two more fixes came out of the sweep.** A non-janet failure appeared in
`native:14.33` — a pipeline test untouched by any of this, flaking ~2.5%.
`runs` showed it had already flaked once at `fe71aa1`, which is why the
regression query stayed silent. DD#55 settles the behaviour ("SIGPIPE is the
programmer's responsibility"), so the **test** was wrong: it asserted rc=0
from a pipeline the model says may abort. Suppressing the producer with `-`
gives 0/300 failures.

Chasing it found a real defect: `sash_wait` returned a bare **128 for every
signal**, dropping the number. Bash gives 128+signum (143 SIGTERM, 141
SIGPIPE), DD#55 defines the aggregate as pipefail's worst status, and codegen's
`emit_signal_exit_check` already used 128+signum — the runtime was the lone
dissenter. Fixed in `sash-rt`; `spec/errors.sh` 55.57–55.59 pin it.

Sweep **4818 / 39 / 482** — every failure `janet:*`, **zero non-janet
failures**, zero regressions vs `fe71aa1`. Rust suites: the same 141
pre-existing failures, name for name.

**Next:** continue the P1 prune — `bug-sed-op-on-array-empties-it` part (b)
(`$(gsub @a …)` silently joins the splat), `bug-while-read-array-nonterminating`
(`while read l` is a silent no-op), `sash-convert-read-dd99`.

**Awaiting DG (batched):** the `rust-tests-hang-on-retired-surfaces` fork
(rewrite stale fixtures vs. make `codegen_tests` validate first);
`bug-regex-literal-dash-in-bracket` (patch vendored `regcomp9` / normalise /
document); and — now that `=a` exists — whether a bare collection name passed
to a user function should be rejected, directing to `=a` or `"a"`
(`bare-collection-name-operand-asymmetry`).

## ══ STATUS (2026-09-11) — DD#100 implemented; a taint hole found under it ══

**On `hand-parser`.** DD#100 collection parameters land end-to-end:
`=name` at the call site, `A`–`E` in the callee, five slots, scalars first.
`spec/collparam.sh` is the executable spec (34 tests). Sweep **4780 / 39 /
478** — every failure is `janet:*`, **zero non-janet failures**, zero
regressions against `fff2a1b`.

**The find that mattered was not DD#100's.** `taint_function_body` walked a
body **once, at its definition**, with the tainted set as of that point, so
a global tainted *later* was invisible inside it:

```sh
fn user { for v in @g; do echo "$v"; done }
argv g
user            # compiled; printed unattested argv
```

Reproduced at `fff2a1b`. Scalars survive it via the SSA backstop; arrays do
not, because `ssa_taint.rs` deliberately does not re-derive array taint.

**DG's two calls fixed it structurally.** (1) *Forbid a fn from tainting a
global* → **SB5067**; the escape is local → attest → assign out, and this
only finishes a rule SB5021 already applied to `g[0]=$(arg 1)`. With it, the
top level is the only place a global's taint can arrive, so a **two-phase**
walk (top level, then deferred bodies) is sound with no fixpoint. (2) Slots
**allow and propagate** rather than ban — an out-param carrying external
data is the point — resolved in `analyze_slots` with a call-graph fixpoint
in both directions. Slot **kinds** stay strict: inferred from call sites,
disagreement rejected.

**Probing found what reading could not.** Three write paths never called
`resolve_args_name`: `A[0]=z` (ICE), `attest v in A` (ICE), and `read $t A`
— which **silently filled a function-local array**, the out-parameter doing
nothing. A permissive slot-kind shortcut reintroduced two answers DD#75/DD#80
already reject: `keys A` on an array printed garbage, `${A[x]}` returned
element 0.

**Two things for DG to decide** (both filed, neither fixed):

- **`notes/tasks/rust-tests-hang-on-retired-surfaces.md` (P1)** — `cargo test
  --test codegen_tests` **never terminates**. `codegen_tests` bypasses
  `validate`, so pre-DD#99 fixtures (`echo -n … | while read -d: -r word`)
  compile to a binary that spins forever. The compiler itself rejects the
  program (SB5059). Pre-existing at `fff2a1b`. `mk test` cannot complete, so
  checklist step 1 is currently unachievable.
- **`notes/tasks/bug-regex-literal-dash-in-bracket.md` (P1)** — `[[ $s =~
  ^[A-Za-z-]+$ ]]` is rejected `malformed '[]'`. A literal `-` first or last
  in a bracket expression is valid POSIX ERE and bash accepts it. The
  message comes from the vendored Plan 9 `regcomp9`, so the fix is a fork
  (patch the engine / normalise the pattern / document the divergence).

**Also filed:** `janet-collection-parameters` (P2) — the janet backend has no
slot support at all; it renders from the AST and emits `(show "nums" (string
"=" "nums"))`. 17 runtime tests guarded `in_modes native,c`.

**Next up:** DG's call on the two P1s above.

## ══ STATUS (2026-09-11) — DD#100: collection parameters settled ══

**On `hand-parser`.** DG settled the design; written up as
**`notes/decisions/DD100-collection-parameters.md`** (next free number —
DD#99 was the highest).

- **Sigil `=`** at the call site: `show "nums" =nums`. Chosen as evocative of
  assigning the collection to the parameter, and `:symbol`-like without being
  it. `=` was never free — it was *over-claimed*; the symmetry fix reclaimed
  the spaced-left/glued-right shape.
- **`A`–`E` in the callee, 5 max, reserved** on the `args` precedent
  (SB5024/SB5052). Array and map both usable — inlining makes each
  instantiation monomorphic.
- **Scalars first, then collections.** Not needed for parsing (`=`
  self-identifies) but keeps source position = `args` index, so `$1` reads as
  in bash and DD#81's arity inference stays legible. **Forward-compatible**:
  interleaving can be allowed later without breaking any call; tightening
  later could not.
- **Builtins unaffected** — bare names, no splats. `attest`/`array a` are
  syntax, not calls; `read a` is double duty (declares *and* populates).
- **Escape hatch**: `f '=foo'` / `f "=foo"` pass the literal — `SingleQuoted`
  and `DoubleQuoted` are structurally distinct from `Literal` in the AST.
- **`join @a` removal deferred** into `dd75-len-collection-only-strlen`,
  which must decide `len`'s operand set anyway. That task now carries the
  note that SB5025's message *recommends* the spelling being removed.

**Part E (DG):** a slot **is** the caller's collection and `f` may modify it,
removing any need for out-params or multiple return. `f =foo` is **not a
read** (so `attest` defers into `f`), whereas `f @foo` **is** — measured, the
splat form is already SB5020 at the call site. That exclusion is the one
genuinely new validator rule; everything else was measured to work already
via inlining: taint flows into a function body (SB5020), an `attest` inside
discharges it, mutation writes back to the caller's array, and a tainted
write inside taints it outward (SB5021).

**Next up:** implement DD#100. No open blockers.

## ═══ STATUS (2026-09-11) — SB5064: `$a` on a collection is an error ═══

**On `hand-parser`.** DG, mid-discussion: *"`$a` for an array `a` is an
error."* It was not enforced, and the two things it did instead disagreed:

| spelling | before |
|---|---|
| `echo $a`, `str x=$a`, `echo $m` | silently **empty** |
| `len $a` (a=(x y z)) | **3** — resolved as the collection *name* |

One spelling, two wrong answers, neither an error. Now **SB5064**
(`check_collection_scalar_read` in `validate.rs`). Sweep **4694 / 39 / 462**,
zero regressions. Specs `types.sh` 75.39–75.43; `75.38` had been pinning the
accident and was corrected.

**The pair worth noting:** this is the dual of `bug-scalar-subscript-segfault`
(`${s[0]}` on a scalar segfaults). Both are the expansion form disagreeing
with the variable's type — a category the compiler was not validating at all.

**Sigil scope settled (DG, low churn):** builtins keep **bare names, no
splats**; user functions get the new collection slots (5 max?). DG corrected
the inventory — `attest` and `array a` are *language syntax*, not calls, and
`read a` is double duty (declares *and* populates), which is why it resisted
the pattern. `=a` means **connecting `a` to a parameter slot**.

**Open — needs DG:** removing splats from `len`/`join` conflicts with
**SB5025's own message**, which tells users to write `$(join @a)` for a
string (`glob_splat.sh:99`). 17 `join @a` sites + the diagnostic's guidance.
See `collection-parameters-design.md`.

## ══ STATUS (2026-09-11) — `=` as the name sigil; assignment symmetry fix ══

**On `hand-parser`.** DG asked about `=` as the sigil character, and whether it
*"would only be legal as an argument."* Both hold — but the question exposed a
**silent wrong answer** first.

`looks_like_assignment` peeked *word* then `Tok::Eq` with **no adjacency test**,
and the scanner drops whitespace, so any command word followed by a `=`-leading
word became an assignment. `echo =a` printed `a`; bash prints `=a`. No spec
covered it.

The obvious fix (require adjacency) regressed **21 tests** — spaced bare
assignment is *deliberate*, it is DD#86's frictionless write (`fn set_msg
{ msg = hi }`). Three shapes exist and only two were claimed:

| shape | spelling | meaning |
|---|---|---|
| glued both | `x=5` | bash assignment |
| spaced both | `msg = hi` | DD#86 frictionless write |
| **spaced left, glued right** | `echo =a` | **unclaimed — the sigil shape** |

`=` now binds iff symmetric. `echo =a` → `=a`, `f =nums` → `=nums`,
`len =a` → SB5062. **4680 passing / 39 / 461 skipped**, zero regressions
(baseline 4669/39/460). Specs `scope.sh` 86.21–86.24, fail-first verified.

So `=` costs no free character — it was never free, it was **over-claimed**.
Caveat: `[[ $x =b ]]` still evaluates as `= b`; latent (no name slot lives in
`[[ ]]`), but `^` would not levy that readability tax.

Also found: **janet leaves `args` unbound**, so *every* user-function argument
is broken there (`f nums` fails identically) — filed to
`janet-join-intrinsic-gaps.md`; 86.23 guarded `native,c`.

Analysis: `notes/tasks/collection-parameters-design.md`.

## ════ STATUS (2026-09-11) — collection parameters: symbol or reference? ════

**On `hand-parser`.** DG proposed passing collections to functions via a
reserved set `A, B, C…` parallel to the scalar `$1, $2`, and asked whether the
name sigil is more like a **Lisp symbol** or a **reference**.

**It is neither — it is a parameter slot, and sash already ships one.** `args`
is the exact precedent: a reserved, per-function, compiler-managed collection
(DD#75 §5 / DD#81) where `$1` is sugar for `${args[0]}`. Measured: `array
args=(x)` → SB5024 at top level, SB5052 inside a function. So `A, B, C` is not
a new mechanism — it is `args` generalised from scalars to collections, which
is why the **callee needs no new syntax at all** (`len A`, `${A[0]}`, `@A`).

- **Symbol is ruled out by decision, not taste.** A symbol is a *value*; if
  names flowed as values, DD#6's rejection of `${!var}` re-opens and SB5031 /
  SB5050 / taint all become undecidable.
- **Reference is close but overshoots** — it drags in an aliasing algebra
  (ref-to-ref, lifetime, storage) sash never needs. A parameter answers all of
  those by construction, and since sash inlines everything, it has **no runtime
  representation**.
- **Consequence:** the sigil is a *position marker*, not a type constructor. It
  need not compose, it collapses nine slots into one parser rule, and under
  `A, B, C` it is **only ever needed at the call site**.

**The one real gap:** `A, B, C` fixes the callee and leaves the caller
untouched — `f nums` is still ambiguous between the collection and the string,
i.e. `bare-collection-name-operand-asymmetry` relocated, not resolved.

**No-shadowing (DG's flag was right):** DD#86/SB5049 forbid exactly the shape a
reserved `A` would create against a user global `A`. Fix is the `args`
precedent — reserve the names. Cost measured at **3 uses in 2 files** across
real sash sources (`spec/.trace/` is generated; `suites/oils` is an external
bash corpus, not sash).

Full analysis, open questions and the measured sigil-character survey:
`notes/tasks/collection-parameters-design.md`.

## ════════ STATUS (2026-09-10) — SB5062/SB5063: name slots ════════

**On `hand-parser`.** Resuming the backlog prune surfaced a validator bypass
that had been producing **silent wrong answers**. `len "hello"` returned `0`.
So did `len "$s"` — the most idiomatic spelling in shell.

One root cause, three faces:

| spelling | was | now |
|---|---|---|
| `len "$s"`, `len ${a[0]}`, `len "lit"` | `0`, silently | SB5062 |
| `join $a ","` | empty output; `a` not even counted as read | SB5062 |
| `keys @m`, `keys $m` | **ICE** `undefined arrlen: ${marrvals}` | SB5062 |

`try_len_intrinsic` derives a variable *name* by calling `expr_to_string`, a
**source-text pretty-printer**. `"$s"` stringifies to `"$s"` *with the quote
characters*, which is then looked up as a name, misses, and reads as nothing.
`len $s` works only by accident — the formatter happens to strip the sigil.

The validator was meant to catch this. `len_operand_name` matches only
`ExprKind::Literal`; its `else { return }` says *"resolved elsewhere"*. They
are not. That comment was the bypass. Not deliberate either — DD#75 §2 says
`len` of an unknown name is a *compile error, no runtime garbage*, and there
was zero spec coverage for any quoted operand.

**This is rejection, not repair.** `join $a` is now rejected while `len $a`
still works. That inconsistency is deliberate: DG's pending `len`
→ collection-only + `strlen` split and the collection-parameter design rewrite
this exact surface, so guessing the final spelling now would only be undone.

**Why the accepted sets differ at all** — and it is not carelessness: **sash
has no first-class collection values.** `@a` is a splat into N values, not a
handle, so `keys` genuinely cannot take it (values have no keys). That same
absence is the root of the parameterised-function problem: a function can read
*and mutate* a caller's array, but only by hard-coding its name. Contents are
parameterisable (splat, read-only); identity is not. The way out is cheap
because sash **inlines every function** — a collection parameter can be a
compile-time name binding, not a runtime handle.

**A fourth face, in janet.** The control spec pinning what already worked
failed there, and no single spelling works uniformly across all three backends:

| form | native / c | janet |
|---|---|---|
| `len a`, `keys m`, `join @a` | ✓ | ✓ |
| `len $s`, `len $a`, `len @a` | ✓ | compile error: unknown symbol `$s` |
| `join @a ","` | ✓ | bad slot #1, expected string, got array |
| `join a ","` | `x,y,z` | **execs /usr/bin/join** |

All pre-existing, verified against the pre-change build; most already filed in
`janet-join-intrinsic-gaps` since August, the `$(len $x)` command form added as
an addendum. Cause is the one that file keeps circling: `emit_janet` renders
from the **AST**, so it never sees the operand resolution native and c inherit
from SSA (`emit-janet-from-ssa` retires the class). DG deferred the janet
overhaul wholesale; 75.38 guarded to `native,c` per that task's convention.

The `/usr/bin/join` fallthrough is deferred but worth naming: a backend that
silently execs an external binary when it does not recognise a builtin is a
safety property, not a feature gap.

**4639 passing / 39 failing / 460 skipped — zero regressions** against
`272b977`. Specs 75.31–75.38, fail-first verified (7 red, control green).

**SB5063 — the fifth face, found by asking DG's design question.** `argv N "a"`
and `matchall S P "out"` accepted a quoted target and **silently wrote
nowhere**: the array kept its old contents, and since both are tainted
sources, the `attest` obligation vanished with the write. Not a taint escape
(no tainted data flows in) but the program runs on stale data with the
diagnostic that would have caught it suppressed. Now rejected.

**Why this argues for a name sigil.** The language has ~7 name-position slots
and "must be a bare name" had been enforced **ad hoc, three times, with three
unrelated diagnostics** — two slots missed entirely (SB5057 read/readline,
SB4014 attest, SB5062 len/join/keys, then this). SB5057's own wording already
says it: *"the target must be a bare name — it is a binding, not a value."*
`command_writes_args` had even written the inventory down, for an unrelated
purpose. The list existed; enforcement never followed it.

**The enumeration found three more defects, one of them mine.** `match S P "o"`
**ICEd** (`undefined array: "o"` — `match` is `matchall`'s sibling, DD#63 gives
it a 3-arg out-array form; I fixed `matchall` and did not look next door).
`unset "a"` **bypassed a deliberate rejection** — `unset a` on a scalar is
rejected on purpose (SB4030), the quoted form compiled clean and did nothing:
`type_check.rs:1186` skips any operand it cannot read as a name, the third
instance of that exact shape. And my own SB5063 had started preempting
SB4045's much better wrong-arity message for `matchall S P`, caught by probing
arities rather than trusting a green sweep — no spec covered the two-arg form.

**The inventory: nine name slots, six diagnostics, four added only after a bug.**

| slot | kind | enforced by |
|---|---|---|
| `read ARR`, `readline v` | binds | SB5057 (DD#99) |
| `argv [N] a` | binds | SB5063 |
| `match`/`matchall S P a` | binds | SB5063 |
| `unset a`, `unset a[i]` | unbinds | SB5063 |
| `attest x in a`, `attest k v in m` | reads | SB4014 |
| `len a`, `join a S`, `keys m` | reads | SB5062 |
| `map m = @{a:DISP}` | reads | SB5033 |

Correctly *not* name slots, verified not assumed: `for x in "a"`, `getenv
"HOME"` (an env name — different namespace), `arg N`. The argument for a sigil
is now measured: nobody was careless, but "this operand is a name" has no
representation, so every slot re-derives it and each re-derivation can forget.

**Separate bug, found by accident:** `sash prog.sash -o u` failed to link (also
`-o ut`; `-o out` fine). `format!("-o{}", output)` glued the flag, and `ld`'s
getopt_long_only matched `-ou` as `--out-implib` by unambiguous prefix,
swallowing the object file. Fixed — `-o` and the name are separate args now.
Spec `cli.7`.

**Next: design the sigil against the full list above** (DG's call — character
and marker placement both still open). Free leading chars, measured: `:` `%` `^` `~` `+`;
taken: `&` `=` `@` `#` `*` `?` `'`. `:` fits semantically (Janet/Clojure
symbols) but collides with the `:` builtin and `@{a:DISP}`. The invariant that
matters more than the character: the sigil must be **compile-time syntax, never
a runtime value**, or DD#6's `${!var}` rejection stops meaning anything.

Then: collection parameters via compile-time monomorphisation, which subsumes
`bare-collection-name-operand-asymmetry`.

## ════════ STATUS (2026-09-10) — spec.db purged from history ════════

**On `hand-parser`.** `spec.db` and `notes/misses.md` are gone from every
commit on every branch. It was 455 of 1001 commits carrying 10,791 blobs for
**9.0 GB** of pack — every spec run rewrites the database wholesale, so each
touch added a fresh ~20 MB blob that deltas poorly. `.git` went **1.7 GB →
5.7 MB**; a fresh clone from GitHub is **7.3 MB**, largest blob now 292 KB of
`codegen.rs`. All six branches force-pushed with `--force-with-lease`.

**Any other clone must be re-cloned.** Every hash from the first `spec.db`
commit onward changed; an old clone cannot pull.

**431 commit citations across 122 notes/docs files were repaired.**
filter-repo rewrites hashes in commit *messages* but not in file contents.
Rather than regex-replacing hex-looking tokens, the 238 distinct hashes were
snapshotted *before* the rewrite and confirmed to resolve via `cat-file`, then
remapped through the commit-map and re-verified after — zero unmapped, zero
ambiguous at 7 chars. `spec.db`'s own `runs` table got the same treatment
across 1.46M rows. (One value, `0961a2e`, was already dangling *before* the
rewrite — an amended commit, not something the purge broke.)

**The purge exposed that `spec.db` was not purely derived**, which is the part
worth remembering. `run.sh` only ever `INSERT`s — the schema arrived with the
clone — so a fresh clone hit `no such table: tests`. And `clauses` is 231 rows
of **curated** DD clause text that nothing regenerates; `status.md` had
flagged in September that it "exists only inside the binary spec.db". The
9.0 GB was all `runs`, which genuinely is derived; the 15 KB curated
remainder just needed a better home.

Now tracked as text: `spec/clauses.tsv` (231 rows, diffable, reviewable) and
`spec/schema.sql` (tables, views, indexes, all `IF NOT EXISTS`). `run.sh`
applies both idempotently, so the database rebuilds from nothing. Verified by
bootstrapping in a scratch dir: 231 clauses, byte-identical round-trip, stable
across a second run. That test caught a real bug — `.import` into a
non-existent table eats the first row as column names, silently loading 230.

**The regression gate is faster than it ever was.** `git show HEAD:spec.db`
dies with the purge; the replacement was already latent in the schema, since
`runs` records `commit_hash` per run. But `runs` had **no indexes**, so that
query took ~4 minutes. Two indexes later: **9 milliseconds**, at ~95 MB of
local disk — affordable precisely because the file is no longer in git.

**4616 passing / 39 failing / 459 skipped — unchanged, zero flips.** Verified
through the new `run.sh` end to end.

Backups (bundle, raw `.git` tar, live database) are outside the repo at
`/home/dg/safebash-purge-backup-20260910-183332/`.

## ════════ STATUS (2026-09-10) — DD#99 complete (stages 1-8) ════════

**On `hand-parser`.** The read family is redesigned end to end.

```sh
read ARR         # stdin, NUL-separated fields -> array
read $s ARR      # split a value already in hand
readline v       # one line -> scalar
write / echo     # without / with a trailing newline
matchall S P ARR # every match -> array (DD#63, added for this)
map m = @{a:DISP}
```

Retired: every read flag, multi-target `read a b c`, `mapfile`, `readarray`,
`echo -n`, and `array a = @m`. Each with a diagnostic that names the
replacement, never a silent reinterpretation.

**Stages 6-8 this session.** `newread` was a staging alias so nothing changed
meaning while code still depended on it; it is gone. Stage 7 added the argv
interior-NUL guard — argv crosses `execve` as C strings, so a NUL-bearing
value was truncated silently with a zero exit, and DD#99 makes NUL the field
separator, so that went from exotic to an ordinary slip. Stage 8 rejected
`array a = @m`, which was accepted and silently produced an EMPTY array.

**Three defects the stage-6 rename exposed, all mine, each caught by a spec:**
dropping `read` from `is_bulk_read_cmd` made `read ARR < f` ignore its
redirect and read stdin; the lastpipe classifier still treated `read` as the
scalar consumer, so a piped `read ARR` miscounted; and `command_writes_args`
matched `"read"` on its old flag-based arm first, so `read args` stopped being
SB5052 and reached the linker. All three were silent-wrong-answer shapes. That
is the argument for having migrated the spec *vehicles* in stage 5 first.

**The near-miss worth remembering.** Stage 7's first version guarded argv
scalars only; the splat probe still printed `[abc]`. Half a guard is worse
than none — it implies argv NULs abort while `@a` truncates quietly. Splat
elements are now guarded via the length `sash_table_value` already writes out.

**Sweep 4599 passing / 39 failing / 452 skipped, 0 regressions.** All 39
remain janet-only and pre-existing.

**Not done, filed rather than folded in:**
- `sash-convert-read-dd99` (P1) — the converter still passes bash `read`
  through verbatim, so its output no longer compiles. It has no
  command-rewriting path today, only annotation insertion.
- `docs/sash.1` still documents the retired flag surface (mostly deletion).
- `janet-map-from-array` (P2) — janet cannot build a map from external data at
  all, which now costs five previously-green specs.
- `bug-regex-blind-past-interior-nul` (P1) — every regex op ignores bytes past
  an interior NUL. No longer blocking (matchall routed around it), still real.

## ════════ STATUS (2026-09-09) — DD#91: `!` is a modifier, not a disposition ════════

**On `hand-parser`.** DG reported `read $ckd:! opts` returning SB5020. Three
things were wrong underneath it, and the third turned into a design change.

1. **`newread SRC ARRAY` tainted its target unconditionally** — it was lumped
   in with `read`/`mapfile`, whose targets are external by definition. The
   two-arg form splits a value the program already holds, so the source is an
   ordinary read and DD#24's no-propagation makes the result clean. Splitting a
   *clean literal* was equally unusable, which is how I found it had nothing to
   do with `:!`. **My own specs nr.9-nr.11 hid it**: they attested an array
   split from a clean literal, i.e. I routed around the behaviour instead of
   asking why it was there.
2. **`newread ${u:~} f` ICE'd** on an assert whose comment claimed validate's
   guard "was bypassed". It was never reached — every command argument is
   marked elidable and syntactically this is one.
3. That second finding exposed the real defect. **`:!` meant two things at
   once** — elide *and* taint-off — so the half that mattered inherited the
   other's position restriction, and DG's line was unwritable.

**DG's call: split them.** `-`/`?`/`~` say what to do when the value is
**absent**; `!` says how to treat the value we **have**. Grammar
`: [!] [ -WORD | ?MSG | ~ ]`, all four combinations legal.

`!` leads, and **the grammar forces it**: `-`/`?` words run greedily to `}`
(`${u:-foo!}` is the word `foo!`), so a trailing modifier could never be seen.
The whole `:!X` space parsed as an error before, so nothing silently changed
meaning.

`${x:!}` is now **"bare, but trusted"** — and that lands exactly right, because
an empty bare `$u` contributes an *empty argument*, not nothing. Dropping the
elide leaves `:!` behaving identically to the bare expansion. It is therefore
valid wherever a bare expansion is, including a scalar assignment — reversing
this DD's original ban, whose stated justification was *"This mirrors `:~`"*,
i.e. an artifact of the coupling rather than a trust-policy choice.

**`@a:!` is now SB5032**, and that is a fix rather than a cost: a splat must
state its emptiness behaviour (DD#69), and today's `@a:!` silently supplied an
elide the author never wrote — a DD#69 violation hiding in plain sight.

**One thing the split nearly broke quietly.** With `:!` no longer producing a
node, the SSA taint backstop lost the trust and rejected the bypass the CST had
allowed. Bare `:!` now lowers to `SsaValue::Trusted`, a pure annotation that
codegen lowers transparently — the two taint layers agree again. The C backend
needed the same arm: without it `${x:!}` fell to a catch-all that emitted a
`/* TODO */` **comment in place of the value**, silently.

**Specs:** `taint.sh` 91.12-91.23, `wordsplit.sh` nr.15-nr.20; 91.5/91.6/91.10,
`var_modifiers.sh` mp.9 and DD#99's map row migrated.
**Sweep 4514 passing / 39 failing / 438 skipped, 0 regressions** — the failing
set is byte-identical to HEAD's and remains janet-only.

**Next:** DD#99 stage 5, the ~250-site `read`/`mapfile` migration.

## ════════ STATUS (2026-09-09) — DD#14: a stage's input redirect went to its stdout ════════

**On `hand-parser`.** Found while migrating `mapfile` for DD#99. DG's
one-line replacement did nothing:

```sh
/usr/bin/tr '\n' '\0' < f.txt | newread a   # a was empty. No diagnostic.
/bin/cat < f.txt | /bin/cat                 # ...and this hung on a terminal.
```

`emit_one_stage_redirect` (codegen/inst_handlers.rs) defaulted every stage
redirect's implicit fd to **1**, so `< f` was applied to the stage's *stdout*
and fd 0 was left on the pipe — or, for a first stage, on the inherited
terminal. That is the whole failure: empty under `</dev/null`, a hang under an
interactive stdin, silent either way. The `open(2)` mode was never wrong.

The non-pipeline path had always picked the fd from the redirect's
**direction** via `default_fd_for_op`. The stage path now calls that same
function instead of keeping its own idea of the default, so the two cannot
drift apart again — the same "one function, no drift" correction the taint fix
needed the day before.

**Specs:** `commands.sh` 14.32-14.36 (native + c) — a first stage's input
redirect, a middle stage's, the DD#99 idiom end to end, the output direction
still landing on fd 1, and a non-pipeline redirect as a control. Not
reproducible in janet: `emit_janet` renders pipelines from the AST.

**Two process findings, both mine, both the kind that hide a green result:**

1. I first filed these in `exec.sh` — which is **native-only**, so a shared
   codegen bug would have been pinned in one mode. Moved to `commands.sh`.
2. I then appended them at **end of file, after the trailing `set -e; tally`**.
   They ran in native and c and produced no janet rows at all, because `tally`
   exits non-zero when that mode had a failure. The file reported `ran 54`
   against `plan=59` and **nothing flagged the mismatch.** New chk blocks go
   *before* `tally`.

**The harness could not report any of this.** `common.sh` gave every child the
runner's own stdin and no time limit, so a stdin-waiting binary stopped
`bash run.sh` dead — no output, no indication which spec was responsible.
Every child (compiler, gcc, test binary, janet, sash-convert) now gets
`</dev/null` and a `timeout`, and a trip is reported as `spec: … exceeded the
Ns hang guard` rather than an anonymous bad exit status. `harness.sh` ht.1-ht.3
pin all three properties: EOF instead of the runner's stdin, a runaway killed
and named, and **the suite still finishing afterwards**. Cost: the sweep went
1m25 → 1m38, one extra fork per child.

**Sweep 4465 passing / 39 failing / 433 skipped, 0 regressions.** All 39
failures remain janet-only and pre-existing.

**Next:** DD#99 stage 5 — the ~250-site `read`/`mapfile` migration, now
unblocked. 111 single-target `read v` → `readline v` and 5 `read -a x -d ""`
→ `newread x` are mechanical; the 20 space-split `read -a`, 63 multi-target
`read a b c` and 46 `mapfile` sites largely pin behaviour DD#99 *removes*, so
each needs a call: migrate, become a reject-test, or go.

## ════════ STATUS (2026-09-08) — SECURITY: taint laundering via a leading builtin ════════

**On `hand-parser`.** Found while costing DG's NUL/`tr` design proposal — an
`echo` prefix laundered a pipeline completely:

```sh
/bin/cat secret.txt | read x y            # SB5020 — correctly rejected
echo ig | /bin/cat secret.txt | read x y  # CLEAN. x="SECRET" y="DATA"
```

The value then flowed into external argv and `[[ ]]` operands unchecked.
`read`, `read x y`, `read -a` and `mapfile` were all affected (only `while
read` was safe). **Validator bypass — the class the adversarial audit names its
highest priority.**

**Both layers failed, and not independently.** The front end
(`validate.rs:3551`) asked whether the pipeline's *source* was external —
`p.commands.first()` — but taint is a property of the data's **path**, not its
origin. The SSA backstop, which exists for exactly this, was **inert**:
`handle_lastpipe_read` hardcoded `clean_pipe: true` for every lastpipe read,
while the field's own doc said *"True when read is in a lastpipe position with
clean producer stages."* The comment described a check nobody wrote. One layer
had a wrong rule; the other had a hardcoded answer to the same question.

**Fixed:** the front end now accumulates `upstream_external` across stages; the
backstop computes `clean_pipe` from the producers. And the two layers now call
**one shared `is_external_source`** — my first draft added a second classifier
in the transform with subtly different rules, which is the very shape that
caused the bug. One function, no drift.

**The test that was missing** is the one that matters: `taint_switch.sh` sw.9
runs the laundering shape under `--no-taint-cst`, i.e. **exercises the backstop
with the front end switched off**. Nothing did that before, which is why
`clean_pipe: true` sat there unnoticed.

**Specs:** `taint.sh` tl.1–tl.8, `taint_switch.sh` sw.9. Sweep **4302 passing /
39 failing / 401 skipped, 0 regressions** (+24), no new failures.

**Process note:** my first clean-control (`echo | echo | read`) was **flaky** —
1 run in 12, a SIGPIPE race on the middle stage. A flaky spec in a security fix
trains you to re-run until green; replaced with deterministic ones. The race
itself is pre-existing and unrelated.

**Also filed:** `janet-read-a-element-count` (P2) — janet's `read -a` counts one
element too many, surfaced by the new clean control.

**Still open:** DG's design question (NUL-only splitting with an upstream `tr`
stage) — this was the detour, not the answer.

## ════════ STATUS (2026-09-08) — DD#98: `_` is the don't-care binder ════════

**On `hand-parser`.** DG asked how discarding unwanted `read` fields was
addressed — *"was it only the `: $x` mechanism? I'm thinking a 'don't care'
variable like `_` might have value."*

**Answer: it was worse than `: $x`.** Discarding required satisfying two rules
that collide — DD#86's dead-variable gate (SB5050: every variable must be read)
and DD#24 taint (a `read` target is tainted, so any *use* needs attest). `: $v`
is itself a use, so on a **standalone** `read` it hit the SSA taint backstop.
The only thing that compiled was `attest a p?` — **asserting a property of a
value you are explicitly throwing away**. `read` also has no discard of its own:
the *last* target absorbs the remainder.

**DG's call: add `_` everywhere a binder appears — one uniform rule.** It is
**write-only**: exempt from SB5050, taint-safe because unobservable, repeatable,
still binds (so `read _ b` gives `b` field 2), and **every read is SB5056** —
`$_`, `${_}`, `${_[0]}`, `@_`, and a bare `_` in `(( … ))`.

```sh
/usr/bin/printf "xx yy\n" | read _ b
attest b p?; echo "b=$b"        # b=yy — no acknowledgment, no attest

attest _ v in m [[ -n $v ]]     # discard the key; DD#61 normally demands both
```

**Write-only is not negotiable, not a nicety.** If `_` were taint-exempt *and*
readable, `read _ x; y=$_` would launder a tainted value into a clean one —
a validator bypass by punctuation. Rejecting every read closes it and costs
nothing.

**No lexer or parser change was needed.** `is_plain_identifier` already accepted
`_`, and a repeated `read _ b _` was already tolerated. The feature is three
checks: an exemption in `report_dead`, an exemption in the attest-all
both-binders rule, and the SB5056 reject. `_`-prefixed names (`_n`) are
untouched; nothing in spec/ or contrib/ used bare `_`.

**Adversarial probing found one hole in my own first cut:** a bare `_` in
`(( … ))` is an `ArithExpr::Var`, not an expansion, so no expression walker
surfaced it — `(( y = _ ))` fell through to the taint backstop. Correct outcome,
useless diagnostic. Closed with `walk_arith_var_reads`, the exact complement.
**Third instance this week** of a check stopping at the `ArithExpr` boundary
(cf. DD#97's subscript gap).

**Specs:** `scope.sh` dc.1–dc.15, all 3 modes. Sweep **4270 passing / 39 failing
/ 394 skipped, 0 regressions** (+45), no new failures.

**Filed:** `read-taint-backstop-no-sb-code` (P2) — standalone and pipeline
`read` disagree about `: $v`, and the standalone rejection carries **no SB
code** at all (it comes from the SSA backstop, not the front-end pass).

**Next up:** `janet-compound-if-condition-ignored` (P1),
`janet-command-substitution-capture-broken` (P1).

## ════════ STATUS (2026-09-08) — the ICE batch: 6 panics + 2 silent wrong answers ════════

**On `hand-parser`.** DG: *"let's fix the other ICEs still pending."* One task
was filed; enumerating the 13 `panic!("ICE: …")` arms in `codegen/` and probing
each found **four more**, all reachable from input a person would plausibly
write:

| trigger | site | was filed? |
|---|---|---|
| `[[ $(f) = x ]]` (string fn) | `values.rs:81` | yes (P1) |
| `[[ $(f) = 1 ]]` (`-> int` fn) | `values.rs:85` | yes |
| `[[ $(echo a; echo b) = x ]]` | `values.rs:90` | **no** |
| `(( y = ${x:~} ))` | `values.rs:103` | **no** |
| `int n=512; echo ${n:1:2}` | `types.rs:29` | **no** |
| `int n=5; echo hi > $n` | `types.rs:29` | **no** |

**Two cases were worse than a panic.** `[[ @a = x ]]` compiled and was *always
false* — even when `a`'s first element was `x`; `echo hi > @a` wrote to neither
file. `v=@a` is correctly SB5025; the assignment path was the only one checking.
Same root cause as the ICEs (a value kind reaching a position whose lowering
can't handle it), opposite failure mode — a crash is a bug report, a false
comparison is a wrong result someone builds on. Fixed in the same batch.

**Only one genuine decision.** The rest were settled by rules sash already had
(elide→SB5046, splat→SB5025, string-op-on-int→SB4030) — applying an existing
rule to a missed position is finishing a decision, not making one. The capture
was open, and the filed task said so ("compiles **or** is rejected — never
panics"). **DG's call: make it work**, consistent with `v=$(f)`. Nearly free:
`lift_int_func_calls` already handled all three value kinds, so the new
`lift_test_captures` is 30 lines of recursion over `SsaTest` and no new
machinery.

**Two comments asserted what wasn't true.** `types.rs:19` — "the Int arm is …
not user-reachable input"; `values.rs:100` — "reaching a scalar `lower_value`
means validate's SB5046 guard was bypassed". Both were claims about a *global*
property, and each was true only of the positions its author had in mind.
Journal 252's lesson, third session running.

**A gap under the gap:** `walk_arith_expansions` — the shared walker
`walk_expr_tree`/`walk_all_exprs` use to descend into `$(( … ))` — had no arms
for `ArithExpr::Subscript`/`SubscriptAssign`, so *every* expr-level check
silently skipped anything inside `a[…]`. That walker was itself a fix for this
same class. Closed and spec'd (ice.10).

**Specs:** `functions.sh` ice.1–6, `var_modifiers.sh` ice.7–10, `types.sh`
ice.11–13, `glob_splat.sh` ice.14–16. Each new diagnostic's suggested
workaround was **run**, not assumed. Sweep **4225 passing / 39 failing / 394
skipped, 0 regressions** (+43).

**5 new failures, all janet, all pre-existing:** janet renders a user-function
capture as `(os/spawn ["f"] …)` and crashes — in the *assignment* form too, so
unrelated to this work — and truncates a multi-statement `$(…)` to its first
statement. Filed `janet-command-substitution-capture-broken`, left failing not
skipped.

**Closed stale:** `cmdsub-followups` gate 1 (unblessed-external ICE) no longer
reproduces — clean SB5020 now, fixed by `tighten-cmdsub-taint-execve`.

**Next up:** `janet-compound-if-condition-ignored` (P1) and the janet capture
bug above; `eliminate-ice-arms-via-types` remains the structural follow-up that
would retire this whole class by construction.

## ════════ STATUS (2026-09-08) — DD#97: the `array`/`map` keyword introduces a collection ════════

**On `hand-parser`.** DG: *"when I do `f=(*)` I see `$(len f)` return 0 despite
the array having elements."* One symptom, **two** independent defects, meeting
at the transform's `state.collections` registry — the thing that decides
whether a length is `arrlen` (elements) or `strlen` (bytes of the slot). An
unregistered collection gets `strlen` on a table, which reads 0.

**1. A glob array was never registered — even with the keyword.**
`track_collection_kind` inferred collection-ness from the *lowered value* and
matched only `IndexedArray`/`AssocArray`; a glob lowers to
`ParamExpansion{operator:"glob"}` and fell through. So `array f=(*.txt)` had
`join` listing all three paths while `len` said 0 — two intrinsics disagreeing
about one variable. Now registered from the **declaration keyword**, which is
what the user wrote and can't be defeated by how the value happens to lower.

**2. A bare `f=(…)` registered nothing at all**, and half-worked: `${f[1]}` and
`for x in @f` were fine, but `len`→0, `join`/`unset`→SB4030, and `f+=(d)` on an
undeclared name was a **codegen ICE**.

**DG's call on the second:** reject it. **A collection is introduced by the
`array`/`map` keyword** — `f=(a b c)`, `f=(*)`, `f=()`, `f+=(d)` are all
**SB5055**. Re-assigning a *declared* collection stays legal (`array f=(a b c);
f=(x y)`). Grounds: DD#41 already made `array arr=(…)` *the* declaration form,
DD#75 says the keyword form is canonical, and `f=(a b c)` reads as a scalar
assignment while silently producing a table. Migration cost was zero — nothing
in spec/ or contrib/ used the bare form. **The ICE is gone as a side effect.**

**Also fixed a doc error of mine:** DD#96 claimed comma is "already sash's
sequencing separator — array literals, argument lists, and `(( ))`". Only
`(( ))` is true; array/map literals are space-separated and calls are
shell-style. The decision stands (the `(( ))` precedent is the one the
line-break rule borrows), but the justification was inflated. Corrected in
DD96, journal 255, and the entry below.

**Specs:** `undeclared.sh` ud.22–ud.29, `glob.sh` g.7–g.9. Sweep **4182 passing
/ 34 failing / 394 skipped, 0 regressions** (+29 passing).

**One new failure, filed not hidden:** `janet:ud.29` — janet emits array append
as *string* concatenation (`(set f (string f @["d"]))` → prints `44`).
Pre-existing and independent (janet renders from the AST, not the SSA), task
`janet-array-append-string-concat`. Left failing rather than `skip_chk`'d so it
stays visible.

**Next up:** the P1 follow-ups — `bug-fn-capture-in-test-operand-ice` (`$(f)` as
a `[[ ]]` operand ICEs) and `janet-compound-if-condition-ignored`.

## ════════ STATUS (2026-09-08) — DD#96: sed edits chain with `,` ════════

**On `hand-parser`.** DG asked to chain the sed-like edits — `$foo =~ s/a/b/
s/c/d/` "or with some other syntax" — and asked what would be *consistent and
logical*. His call after the survey: **comma, both forms, only `s///`.**

```sh
v =~ s/a/1/, s/b/2/          # statement form
w = ${v:s/a/1/,s/b/2/}       # expansion form
v =~ s/one/1/,               # and across a line break
     s/two/2/
```

Comma won because sash **already** sequences with `,` inside `(( ))` (DD#94).
*(Correction: the DD#96 entry first claimed array literals and argument lists
did too — they don't; those are space-separated. Fixed in DD96/journal 255.)*
Juxtaposition was the
tempting alternative and the worst one: no visible separator means a typo'd `/`
silently fuses two edits into one. The multi-line behaviour was **not a second
decision** — DD#94's rule ("a separator continues while an operand is owed; a
complete item ends at the newline") already settled it. The second feature cost
less than the first.

**It fixed a silent-drop bug underneath.** `split_operator` swept everything
after an edit's closing `/` into the *flags* slot unvalidated, so
`${v:s/a/1/,s/b/2/g}` dropped the second edit **and** turned the first into a
gsub (the dropped text contained a `g`). Wrong output, no diagnostic. The chain
work and the fix are the same change: `split_sed_edit` now returns the
unconsumed rest explicitly, the folder consumes it, and a rest that isn't a
`,`-led edit is SB4032.

That is the **third** instance this week of one shape — journal 252's ICEs,
DD#95's mixed chains, this — all paths that *accepted* what they should have
rejected, all invisible because no spec pinned the rejection.

**Only `s///` chains**, on DG's practical grounds: substitution is the edit
people repeat, and a general rule would first have to settle what
`${x:1:3,:-y}` means. Left open, not foreclosed.

**Specs:** `spec/var_modifiers.sh` 96.1–96.11. Sweep **4153 passing / 33 failing
/ 391 skipped, 0 regressions** (+33 = 11 × 3 modes); all 33 failures remain
janet-only and pre-existing.

**Next up:** the P1 follow-ups — `bug-fn-capture-in-test-operand-ice` (`$(f)` as
a `[[ ]]` operand ICEs) and `janet-compound-if-condition-ignored` (janet emits
`(if true …)` for any compound `if` condition).

## ════════ STATUS (2026-09-08) — DD#95: mixing `&&` and `||` is rejected ════════

**On `hand-parser`.** DG asked whether `!`/`&&`/`||` behave identically inside
and outside `[[ ]]`. Almost: `!`, pure `&&` chains, pure `||` chains and the
statement-level consume rules all match — but a **mixed** chain groups
differently (inside `&&` binds tighter; outside they are equal-precedence
left-assoc), so the truth tables diverge in 2 of 8 rows. The same text meant two
different things depending on where it sat.

**The cross-language survey DG asked for reframed it** (verified against
sources): every mainstream expression language binds `&&` tighter — so sash's
*inside* was already right; **shell is the outlier** and the outside inherited
it; **C's own `-Wall` warns on exactly this shape** (so "we defined a
precedence" is not enough for readers); **Ada makes it a syntax error**. The
deepest lesson was Perl/Ruby's: they have sash's exact two tiers and
distinguish them by *spelling* (`&&`/`||` vs `and`/`or`), not by one spelling
with two precedences — the precedence gap was the symptom, identical spelling
for two tiers was the cause.

**DG's call:** reject the mixture in **both** contexts with **different advice**
— parentheses inside `[[ ]]`, `if`/dispatch outside. One code (SB4053), two
messages (the SB4020 precedent). Pure chains are unaffected, which is the
minimal rule (they group the same either way); `!` and the `cmd || die`
error-handling idiom are untouched.

**What it buys:** the inconsistency is *removed*, not papered over — no text now
means two things depending on context. It also settles the missing-grouping
question outside `[[ ]]` (`( )` is SB4014, `{ }` SB4027): that is not a gap to
fill but the point, since a list needing grouping is asking to be an `if` or a
dispatch.

**Migration measured first:** 21 sites corpus-wide, 20 of them the one idiom
`X && echo Y || echo N`, zero in `contrib/*.sash`. All rewritten to
`if … { } else { }`. **Three were superseded rather than migrated** —
wordsplit.sh 62.7/62.9/62.10 existed to pin the shell left-associativity this DD
removes; they now assert SB4053 with the old claim recorded. This DD
deliberately deletes documented, tested behavior.

specs errors.sh 95.1–95.12, written first. **3-mode sweep: 0 regressions, +36
passing** (4084 → 4120; failing unchanged at 33). DD#95; journal 254. SB4053 new.

## ════════ STATUS (2026-09-07) — DD#74 addendum: an `&&`/`||` tail is not a predicate ════════

**On `hand-parser`.** DG: *"sash supports `[[ ]] || cmd` but if the last clause
is a user function, it throws SB4037."* Broader than reported — **any** user
function on the right of `&&`/`||` tripped it (`false || f` too); externals and
builtins were fine, which made it look like a `[[ ]]` interaction.

`collect_predicate_uses_stmt` implemented DD#74's "operand of `&&`/`||`" as
**both** operands. But a function is a predicate when something *branches on its
status*, and in a bare `A || B` the final clause's status is just the
statement's status — so it was no more a predicate than a bare `f`, which was
already accepted. The rule was inconsistent with itself.

**DG drew the line** (I asked, since DD#74 might have meant it): *"an
`if a && b && c { }` should probably still require `c` to be a predicate."*
So — **statement** `a && b && c`: the tail is not a predicate. **Condition**
`if a && b && c`: the whole list is consumed as a boolean, so every clause
including the last decides the branch. Still predicates in the statement form:
the **left** operand, and a **middle** clause (`&&`/`||` are left-associative,
so `b`'s status decides whether `c` runs).

**The fix is one deleted line** — the rest was already right: a nested list on
the left goes through `mark_predicate_operand`, which recurses into both its
operands (middle clause still marked), and conditions go through
`mark_condition_predicate` → `mark_predicate_operand` (last clause still
marked). specs functions.sh 74.30–74.37, written first, four of them guards that
passed *before* the fix and must keep passing. **3-mode sweep: 0 regressions,
+24 passing** (4060 → 4084; failing unchanged at 33). DD#74 addendum;
journal 253.

## ════════ STATUS (2026-09-07) — DD#93 Part D addendum: wider arith RHS + predicate taint ════════

**On `hand-parser`.** DG: *"this gives SB5031: `x=$(printf 123); attest x
((y=x*2)); echo $y`"*. The confusing message hid worse — with `y` used, the same
line **ICE'd** (`unreachable!` in expr_handlers), a user-reachable panic I
introduced in Part D that morning. My SB4020 exemption keyed on "attest with an
arith last arg" without checking it *decoded* as the form, so a near-miss was
exempted, failed to classify, fell through to the generic-command path and hit
the value lowering as a mutation. Where it didn't ICE it was a silent no-op —
neither clearing taint nor binding — hence SB5031/SB5020 about innocent names.
My own comment claimed "validate reports the near-misses"; I never wrote it.

**DG turned the bug report into two design calls.**

**(1) The RHS may be any expression that REFERENCES the attested name.**
Computing `x*2` still requires converting `x`, so the `i?` gate proves exactly
what it proves for `(( y = x ))` — `attest n (( bytes = n * 1024 ))` now works.
An RHS that never mentions the attested name proves nothing → **SB5054** (also
for a non-assignment `(( … ))`), which is the near-miss diagnostic that should
have existed.

**(2) A predicate may not read another TAINTED name.** I flagged that a wider
RHS can read other variables and that attest predicates were never taint-checked
at all (`taint_command` had `return; // Don't check args of attest`). DG asked
for the wrinkle to be explained before choosing — rightly, because the forms
differ: a `[[ ]]` predicate yields a **boolean** (bounded influence over the
gate) while an arith predicate yields a **value** bound to the clean target, so
`attest x (( y = 0*x + z ))` would launder tainted `z` into clean `y`. DG took
the stricter line for **both**: even one bit of influence over a
declassification shouldn't be free. The attested name stays exempt — it is
tainted by definition. Blast radius checked first: every attest predicate in the
suite reads only its attested name; the one exception (24.16) is a negative test.

`AttestPredicate::ArithConvert` carries the RHS now; `walk_arith_var_reads`
moved `type_check` → `ir` (the reference rule, the taint rule and dead-var all
want it); `check_attest_predicate_reads` + `walk_test_operands` in validate; the
SB4020/SB4030 exemption now requires the predicate to classify (the ICE fix);
dead-var learned the RHS's other reads (spec 93.44 caught `k` looking stray);
janet emits via `arith_expr_to_janet`.

specs taint.sh 93.40–93.48 — DG's report verbatim as 93.48, the laundering case
93.45, the `[[ ]]` tightening 93.46 + clean-operand counterpart 93.47. **3-mode
sweep: 0 regressions, +26 passing** (4034 → 4060; failing unchanged at 33).
fmt+clippy clean. DD#93 addendum; journal 252. SB5054 new.

## ════════ STATUS (2026-09-07) — spec.db pruned of stale rows (totals rebased) ════════

**Housekeeping, on DG's instruction.** Answering "what are the 34 failures?"
turned up that `test_status` keeps the last recorded status of a test **forever**,
including tests that no longer run — so the totals were inflated and one
"failure" was a ghost.

Pruned **297 test ids / 28,194 run rows**:
- **291 legacy bare ids** (`0.1`, `12.1`, …) recorded 2026-06-08, *before* the
  harness added the `MODE:id` prefix — every one superseded by its
  `native:`/`c:`/`janet:` counterpart.
- **6 mode-prefixed rows for two removed tests** — `48.2` (`shift`, dropped by
  DD#75 §5 on 2026-08-21; its row last ran 08-20) and `vm.dr3`. Verified absent
  from `spec/*.sh` before deleting.

`janet:48.2` was the ghost: it accounted for 1 of the "34 failures", so the live
count was always 33.

**Totals rebased: 4330/4754 → 4034/4457 passing, 34 → 33 failing.** Nothing
about the compiler changed — earlier entries' numbers were accurate against the
unpruned DB; compare baselines across this line with that in mind.

Verified: backup taken first, cross-checks before deleting, `integrity_check` ok,
and a full 3-mode sweep afterwards reproduces exactly 4034/33/390 with no row
resurrected (`run.sh` auto-registers unknown test ids, so a re-added test simply
re-registers).

**Then DG said "fix both" — and the first one turned out to be a real bug that
made the second a trap.** The `clauses` table holds **DD requirement text**
("Strings SHALL carry both a pointer and a length", dd 12) keyed by the *bare*
id scheme. When the harness added the `MODE:` prefix, `run.sh` kept deriving
`clause_id` from the full prefixed id, so every test pointed at a clause that
could not exist — DD traceability had been silently broken for all three
backends ever since. Fixed by deriving through the prefix
(`native:56.12a` → `56.12`), in `run.sh` and as a backfill: **231 of 237 clauses
relink, 754 tests regain their DD text**.

That is why pruning the "237 orphans" as asked would have been wrong — it would
have deleted 231 valid DD clauses. Only **6** are genuinely orphaned once the
link is repaired, and they are exactly the removed/renamed tests (`48.2`'s
"Compiler SHALL track cumulative shift count", the `57.x` convert clauses, …);
those 6 are gone, leaving **0 orphans**.

**DG on the end goal (2026-09-07):** *"there will ultimately be a specification
document, presumably with a logical ordering and id scheme, built from the DD
and spec work."* Filed as `specification-document` — not to start it, but so
nearby work stops optimising for the wrong shape. It is the missing consumer the
clause text points at, and it means the current clause **keying is provisional**
(spec ids are DD-numbered by *chronology*, not logic, so expect a re-keying).
The load-bearing constraint is `SUBSET.md`'s failure: a normative document not
mechanically tied to the DDs and specs drifts, and a drifted spec is worse than
none — DG deleted SUBSET.md as "worse than useless" for exactly that.

Residual, filed as `spec-db-clause-text` (P3): **1255 clause ids / 3703 tests**
have no requirement text. Not simply "DD#59 onward" — 21 of the 38 *seeded* DDs
are only partially covered, and the named suites (`cov.*` 92 ids, `vm.*` 67,
`dp.*` 44, …) are the larger half. Not an integrity fault; FKs are unenforced.
The task gates on a prior question — **nothing reads `clauses` today**, so the
text would buy nothing without a consumer (the valuable direction being "which
clauses have no test?"). It also flags a fragility worth fixing regardless: the
231 existing clause texts exist **only inside the 68 MB binary spec.db**, with
no seed file or insert script anywhere in the repo, so a rebuild would lose
them — `spec/clauses.sql` + an idempotent `INSERT OR IGNORE` would make them
diffable and restorable.

## ════════ STATUS (2026-09-07) — DD#94: newlines/`;`/comments are trivia inside `(( ))` and `[[ ]]` ════════

**On `hand-parser`.** Took the deferred task
`arith-block-separators-and-comments.md` plus DG's addition (line breaks and
comments in a **dispatch block**). It was a decision task, so I surveyed the
current behavior, put the options to DG with worked examples, and he chose the
full permissive package: **1b** newline ≡ comma, **2b** `;` ≡ `,`, **3b**
`#…EOL` comments, + dispatch arms and multi-line `[[ ]]` tests. → **DD#94**.

**The framing that made it easy.** sash already treated newlines/comments as
trivia in array literals, bodies, and after a pending `|`/`&&`/operator — this
finishes a rule that was most of the way there. The survey also turned up two
inconsistencies it removes: a comment worked inside a `[[ ]]` *test* but not
between *dispatch* arms (both `[[ … ]]`), and an array literal could span lines
while an arm list couldn't.

**Arithmetic.** A standalone `(( ))` is a sequence block (`,`/`;`/newline,
value = last). The continuation rule fell out of one placement: the *operand*
position skips separators, so a pending operator swallows them (`(( 1 + ⏎ 2 ))`
→ 3, unchanged) while a complete expression ends at one. A separator is
*required*, so `(( 1 2 ))` stays an error. The C-style for-header is untouched
(`;` still delimits its 3 slots — the for-parser splits before the arith parser
runs). Comments: `#` is a comment when whitespace-preceded, still the DD#72
base-literal reject when alnum-adjacent, so **`16#ff` stays a loud SB4035** —
that adjacency test is the whole safety argument.

**`[[ ]]`.** Arms one per line with comments between them; tests span lines
around `&&`/`||`/operands. Skipping is at the parser's structural points, NOT
the scanner — an arm body is an ordinary statement list where newlines still
separate. A newline ends one arm's pattern list, which is what keeps a
multi-line arm list unambiguous.

**A silent truncation, found and fixed.** Spec `nl.r10` went red: with ` # 3))`
now a comment the closing `))` is eaten, and `collect_arith_content` **returned
Ok at EOF with parens still open** — so `echo $((5 # 3))` compiled and printed
`5`. That was already true for any unterminated `(( ))`; comments merely made it
reachable. Now a hard error naming the cause. Same principle as DD#72, one level
up (delimiter, not literal).

specs arithmetic.sh 94.1–94.10, dispatch.sh 94.20–94.28 — written first, and
deliberately including the guards that must NOT change (continuation, `16#ff`,
for-header, one-line pattern lists, arm bodies). **3-mode sweep: 0 regressions,
+56 passing** (4274 → 4330; failing back at the 34 baseline). fmt+clippy clean.
Two touched functions tripped the complexity gate and were
refactored under it, not bypassed (`parse_dispatch` cc 13→6 via
`parse_dispatch_arms`; `tokenize_arith` cog 7→1 via `push_arith_token`); sweep
re-run after. DD#94 + journal 251; task closed as done. Filed `janet-for-init-comma-sequence`
(P3, pre-existing — verified on a stashed clean tree; 94.10 gated native,c).

## ════════ STATUS (2026-09-07) — DD#93 Stage 4: `attest n (( c = n ))` — bundle complete ════════

**On `hand-parser`.** DD#93 Part D, the last stage: an int-assignment IS the
predicate. The conversion can only succeed on a valid, representable i64, so
**performing it proves the property** — and it declassifies BOTH names (the
source string `n` and the int `c` it binds). Failure aborts with the ordinary
attest-fail message. This closes the DD#77 split that opened the thread: `i?`
validates-and-branches, a disposition converts inline, and this form
validates-converts-declassifies in one accountable step.

**Shape.** `classify_attest` now returns an `AttestPredicate`
(`Test(&TestExpr)` | `ArithConvert { target }`) beside the form — and the form
stays `Scalar { var: n }`, so taint clearing, form/kind checks and scope rules
keep treating it as a scalar attest; only the *justification* differs. Accepted
only as exactly `(( TARGET = SOURCE ))` with SOURCE the attested name (a wider
expression proves nothing about the source); same-name is **SB5053** (DD#93
defers in-place retype). **`if` form supported** (DG): branches instead of
aborting, target declared-but-unassigned on the else path — given sash's
existing meaning, so it reads `0` like any declared `int x`.

**Lowered onto existing SSA** — an `i?`-gated `Attest` (the abort + the clean
source) then an ordinary int assign — so **no backend learned a new form**. The
`i?` gate is what makes the assign safe: `sash_atoi` can't overflow on an
already range-checked value.

**Three rvalue rules had to learn the form**, each exempted narrowly (pointer
identity against that predicate expr, collected recursively so `if attest …`
is covered too): SB4020 (it IS an assignment, by design), **SB4030** (DD#93's
stated exemption — the `attest` supplies the accountability), and SB5031 (the
attest introduces the target). spec 93.36 pins that the same expression
*outside* an attest is still rejected, so the exemption is scoped to the form,
not a hole in DD#77.

**janet** emits this form in full (gate + abort message + binding) rather than
eliding it as it does `[[ ]]` predicates — eliding left the target undefined, a
compile error. Separately found that **janet doesn't implement `if attest` at
all**, in any form (it emits `(os/execute ["attest" …])`; the existing `[[ ]]`
form fails there too) → filed `janet-if-attest-emitted-as-command`, specs
93.31b/93.33/93.34 gated `native,c`.

specs taint.sh 93.30–93.37. **3-mode sweep: 0 regressions, 24 new passing**
(4250 → 4274; failing unchanged at 34). 4 new parser unit tests, written first.
fmt+clippy clean. Six touched functions tripped the complexity gate and were
refactored under it rather than bypassed (`classify_attest` cc 17→6, janet
`emit_command` 15→2 split control vs value builtins, `lower_if_attest` 13/7→7/6,
`validate` cog 11→5, `check_sexpr_string_producers` cog 10→5,
`emit_janet_attest` cog 8→2); sweep re-run after, still 0 regressions.
DD#93 + tracker + journal 250. SB5053 new.

**DD#93 is complete** (S1 rename 247, S2 shape tests 248, S3 shorthand 249,
S4 250). Open follow-ups from the DD: UTF-8-aware shape test, the remaining
POSIX classes (`a?`/`d?`/…), the array analogue `attest e in a (( … ))`, and
same-name in-place retype.

## ════════ STATUS (2026-09-07) — DD#93 Stage 3: the `attest VAR OP` shorthand ════════

**On `hand-parser`.** DD#93 Part B: for any unary test operator, the bare form
stands for the predicate applied to the attested name — `attest x i?` ≡
`attest x [[ i? $x ]]`, `attest e in a i?` ≡ `attest e in a [[ i? $e ]]`. The
value is safety, not brevity: the operator binds the attested name **by
construction**, so the wrong-var predicate SB5020 exists to catch is
unwriteable in this form, and `attest` still greps.

**The map form (DG's call).** DD#93 covered scalar + array but not the
two-binder map form, so I asked rather than picked. DG: apply the operator to
**both** — `attest k v in m i?` ≡ `[[ i? $k && i? $v ]]`. It follows from an
invariant already in the code (`taint_attest` requires a map predicate to
reference both key and value, SB5020), so binding one would be SB5020 on
arrival. Recorded in DD#93 itself.

**Pure parser sugar** in `parse_attest` (+ `peek_unary_test_op`,
`attest_shorthand_test`): the lead now also ends on a bare operator word, and
the synthesized predicate is built from the args already collected — the same
shapes `classify_attest` decodes. Nothing downstream changed; `taint_attest`,
`lower_if_attest` and `handle_attest_stmt` never learn the shorthand exists,
and the parser tests assert the shorthand and full form decode identically.

**Fixed a user-reachable ICE.** `attest x -v` panicked in codegen
("ICE: unimplemented unary test operator") instead of SB5028 — and so did the
full form `attest x [[ -v $x ]]` on the committed tree, so pre-existing.
`check_test_constructs` early-returned unless the command was `[[`, so attest
predicates skipped **both** SB5028 and the DD#39/#63 regex checks; the parser
accepts `-v`/`-o`/`-R`, validate was the only rejecter, and codegen has no
lowering. `attest` now goes through the same gate. DD#93's "What does NOT
change" bullet asserted this already held — amended to record that it didn't.

specs taint.sh 93.20–93.26 (incl. 93.22b: one non-integer element aborts the
attest-all, and 93.25/93.26 pinning SB5028 in both forms). **3-mode sweep: 0
regressions, 18 new passing** (4232 → 4250; failing unchanged at 34). 6 new
parser unit tests, written first. fmt+clippy clean. DD#93 + tracker + journal
249.

**Next:** S4 — `attest numstr (( myint = numstr ))`, the last stage.

## ════════ STATUS (2026-09-07) — DD#93 Stage 2: content-shape tests `i?`/`p?`/`g?` ════════

**On `hand-parser`.** Stage 2 of the DD#93 bundle: three sash-native unary
tests spelled with a `?` suffix (out of bash's saturated `-X` namespace) —
**`i?`** strict i64-representable integer (**hard rename of `-i`**, no alias),
**`p?`** non-empty + all bytes ASCII printable (0x20–0x7E), **`g?`** non-empty +
all bytes ASCII graph (0x21–0x7E). **General operators**, not attest-only.
Test-first per layer, mirroring the `-i`/`is_int` path: runtime
`is_print_inner`/`is_graph_inner` over a shared `all_bytes_match(bytes, pred)`
(so a later `a?`/`d?`/… is one line) + `sash_is_print`/`sash_is_graph`; parser
`is_unary_test_op` + the `try_parse_leading_unary` gate broadened past
`starts_with('-')` (the lexer needed nothing — `?` is not a word-break char);
dispatch ×3 backends incl. the two C `extern` prototypes. specs types.sh 77.x
migrated to `i?`, new 93.10–93.19. **3-mode sweep: 0 regressions, 30 new
passing** (4202 → 4232 passing, failing unchanged at 34). fmt clean. DD#93 tracker + journal 248.

**Two bugs found.** (1) `sash-rt-tests/build.rs` had no
`cargo:rerun-if-changed`, so the runtime tests linked a **stale**
`libsash_rt_test.a` — new symbols "undefined", and worse, changed behavior would
test green against old code. Fixed. (2) **janet emits `(if true …)` for any
compound `if` condition** — `if [[ -z x ]] || [[ -z y ]]`, `&&`, even
`if false || false` take the then-branch in janet but the else-branch in
native/c. Pre-existing and unrelated to DD#93; a silent wrong answer. Filed P1
`janet-compound-if-condition-ignored`. It also means janet specs with a compound
`if` condition expecting the then-branch pass **vacuously** today (77.5 is one),
so fixing it may surface masked janet failures.

**Environment (this tree was rsynced) — repaired.** A clean tree on this host
initially disagreed with the rsynced `spec.db` by **61 tests** (59 janet, c/native
54.1); verified environmental by stashing and re-running. Cause: **stale binaries
carried over by the rsync**, built against a newer glibc (`GLIBC_2.34 not found`)
or simply never built here — `target/release/sash-regex` (the 59 janet regex
failures), `sash-getopt`, `target/debug/sash` (every cargo test that shells out),
and `sash-expand`, which was missing entirely (54.1's `$(which sash-expand)`).
After rebuilding all four the host **matches the source baseline exactly**:
34 failing before, the same 34 after — and **4202 → 4232 passing** with Stage 2's
+30. Still open here: `contrib/sash-grep.sash` no longer compiles (SB5025
`$(argv)` splat), so `sash-grep-test` halts `mk test` before the cargo suites;
and `mk lint` fails on two untouched files (`transform/functions.rs:598`,
`validate.rs:864`, `collapsible_if`) under this host's newer clippy 0.1.98. The
cargo suites carry a large failing baseline here (84 codegen / 48 e2e / 25
taint_spec …) — verified identical on a clean tree, so not from Stage 2, but
worth a look since `mk test` is a stated gate.

**Tooling.** Fixed the indexnudge `sqlite3` shim: `freshen` renames the DB out
of the way (rename-as-lock), so a concurrent reader found no file, silently
created an **empty** DB there, got `no such table: lines`, and the next freshen
renamed that empty file over the real `.db.bak` — destroying it and forcing a
full reindex. Now serialized with flock (shared for readers, exclusive for
freshen), and the shim's `-f` existence test moved *under* the lock (testing it
before locking was the actual hole). 0 failures in ~200 stress queries; index
verified pristine. Residual: a **direct** `indexnudge --freshen` still bypasses
the shim lock.

**Next:** S3 (`attest VAR OP` shorthand), then S4 (arith-assign attest form).

## ════════ STATUS (2026-09-06) — DD#93 attest ergonomics: Stage 1, bless → attest rename ════════

**On `hand-parser`.** DG opened on bless ergonomics ("assigning an int var
rather than using a `-i` test could save a step, e.g. `bless numstr (( myint =
numstr ))`"). The thread produced **DD#93**, a four-part bundle. Pivotal: DG
rejected my "make every arith result clean" (Option A) — "makes the audit trail
harder to follow"; declassification must stay explicit/named/greppable. His own
spelling keeps the keyword, so it saves the step without erasing the trail.
Parts: **(A)** rename `bless`→**`attest`** (a reviewer balked at the *word*;
"attest" = certify a checkable property, and leaves "trust" free for the
*unchecked* bypasses `:!`/`--no-taint`); **(B)** unary shorthand `attest VAR OP`
≡ `attest VAR [[ OP $VAR ]]` (+ attest-all); **(C)** content-shape tests
`i?`/`p?`/`g?` (rename `-i`→`i?`; general operators; non-empty; ASCII-only);
**(D)** `attest numstr (( myint = numstr ))` — int-assign IS the predicate,
cleans both, distinct names, scalar only.

**Stage 1 (this commit) — the rename.** Hard, no alias: `bless` is no longer a
keyword (degrades to unknown command → SB5011). Renamed the live surface —
code (incl. `sash_attest_fail` runtime symbol + janet prelude), converter,
specs, Rust tests, current-state docs (README/GUIDE/REVIEW + one AGENTS line);
**left historical records** (journal, prior DDs, TIMELINE, audit,
run-compiler-tasks task-name refs). specs taint.sh 93.1–93.3 (93.2 native,c).
**3-mode sweep: 0 regressions** (native/c/janet); native+c taint 132/132.
fmt+clippy clean. DD#93 + journal 247 + task tracker.

**Next:** Stage 2 (`i?`/`p?`/`g?`), then S3 shorthand, S4 arith-assign form.

## ════════ STATUS (2026-09-05) — the `args` array is immutable + always clean (DD#92 addendum; SB5052) ════════

**On `hand-parser`.** DG: "could I ever pass a tainted arg to a function — I don't
think so; passing the arg counts as a read." Confirmed: passing a value is a
call-site read, so a tainted one is SB5020 unless blessed (even a tainted default
`${1:-$(getenv HOME)}` is caught) → received args are unconditionally clean, and
the SSA backstop already relies on it (seeds `$1..$9`/`$@`/`$*` Clean by DD#24).
The one nuance (args could be *locally* re-filled tainted via `argv 1 args` /
tainted `read -a args`) → DG chose to make **`args` immutable**: every write is
**SB5052** — `args=…`/`args[i]=…`, `read -a`/`argv`/`mapfile` into args,
`array args=…`, and `bless … in args` (now a clean rejection, was the
`bug-bless-all-args-array-ice` codegen ICE — meaningless anyway since args can't
be tainted). Reading args (`@args`, `${args[i]}`, `$1`, `$#`, `len args`) is
unaffected; use a differently-named local array. validate `check_args_immutable_stmt`
(+ `command_writes_args`/`argv_target_name`/`mapfile_target_name`/`bless_array_target`).
specs optional_args.sh 84.25–30 (84.30 native,c — janet read-array gap). 3-mode
sweep: 0 regressions, 18 new. DD#92 addendum; journal 246; ICE task resolved. SB5052 new.

## ════════ STATUS (2026-09-05) — bare `@args` external splat implies min arity ≥1 (DD#92) ════════

**On `hand-parser`.** DG: "forego the SB5032 or `@args:?` modifier for args in a
command within a function when the arity check knows there's always ≥1 argument …
treat `fn f { tool @args }` as if `fn f { : $1; tool @args }`." A bare `@args`
splat to an EXTERNAL command now records a **required index 0** in the arity
(→ variadic `floor ≥ 1`) and is **exempt from SB5032**. Soundness is DG's insight:
raising the arity (not just waiving the check) makes the splat provably non-empty
and a 0-arg call SB5042 — merely waiving would let the external run with 0 args.
Scope: `@args` only (general `@a` keeps SB5032, DD#69); external only (for-in /
builtin `@args` impose no floor); dispositions (`@{args:-}`/`@{args:?}`) still
mean "accept 0 args". validate `check_external_splat` (+ `bare_arrvals_splat_name`)
skips SB5032 for `@args`; transform `sig_scan_stmt` (+ `is_external_head` /
`is_bare_args_splat`) inserts required 0 — both key on the same resolved-head
predicate. specs glob_splat.sh gs.31–35 (gs.31/34/35 native,c — janet `@args`
splat unimplemented, pre-existing). 3-mode sweep: 0 regressions, 15 new. DD#92;
journal 245. Filed `bug-bless-all-args-array-ice` (pre-existing `bless x in args`
ICE, separate).

## ════════ STATUS (2026-09-04) — bugfixes: builtin-echo elide ICE + fresh-array lastpipe read segfault ════════

**On `hand-parser`.** DG hit `echo -n $ckd:! | read -d '' -a norm` → ICE
(`ElideEmpty must be expanded by a list builder`). Two pre-existing bugs (both
reproduce with `:~` and even with no elide — the `:!` line just surfaced them):
**(1)** builtin `echo` didn't handle an elide (`:~`/`:!`) arg — native ICE'd,
c/janet double-spaced (`a  b`). Fixed: builtin echo writes an elide arg's
separator+value only when non-empty (native `echo_write_elide_arg` via `brif`; c
`emit_echo_elide_arg`; janet gap filed `janet-echo-flags-and-elide`). **(2)** a
FRESH `read -a`/`mapfile` target inside a *called* function segfaulted (null
array table): the function body is lowered twice (dead `fn_` template + inline),
and the template's `autodeclare_bulk_target` registration leaked into the inline
via `state.collections`, so the inline skipped the table `Assign` → null table.
Same class as DD#86's `readonly_vars` scoping — fixed by also snapshotting
`collections`/`assoc_arrays` in `enter/exit_fn_body_scope`. Pre-declaring the
array already worked. specs wordsplit.sh 22.28–22.31 (native,c; janet gated for
read-array/echo gaps). 3-mode sweep: 0 regressions, 12 new. Rust e2e/taint_spec
at baseline. journal 244. Filed `janet-echo-flags-and-elide`.

## ════════ STATUS (2026-09-04) — `:!` taint-off operator + `$( )` brace-less operands (DD#91) ════════

**On `hand-parser`.** DG: "yes, `$( )` too [in brace-less operands]; and a new
operand `$foo:!` = `:~` but also disables the taint guard — a compromise between
bless and `--no-taint`." Two parts: (1) the DD#90 brace-less operand scanner now
handles `$( )`/`$(( ))`/backtick operands (balanced-depth scan, quotes skipped)
— `$1:-$(getpid)` works, external captures still SB5020. (2) **`:!`** — a
per-use bless-without-predicate: `${x:!}`/`$x:!`/`@a:!`/`@{a:!}` use a tainted
value without a bless predicate (clean + propagating, elidable-position-only
like `:~`, wordless → SB4051/SB5033, SB5046 outside a list). Scalar +array,
braced +brace-less, all 3 backends. Design settled with DG first
(security-sensitive validator bypass). Dual-pass: `ElideEmpty{taint_off}` +
`ArraySplatDisposition.taint_off` (op `arrvals_trust`), honored by the CST gate
(authoritative) and the SSA backstop (forward-compatible; backstop doesn't check
these positions yet). Verified `--no-taint-cst` no-ICE. specs taint.sh 91.1–91.11
(getenv ones gated native,c — janet lacks getenv), var_modifiers.sh vm.bl13–15.
3-mode sweep: 0 regressions, 42 new. DD#91; journal 243. Filed
`audit-taint-bypass-report` (DG: a broader bless/taint audit report). No new SB.

## ════════ STATUS (2026-09-04) — brace-less dispositions `$x:-foo` / `@a:~` (DD#90) ════════

**On `hand-parser`.** DG: "the `{ }` in `${x:-foo}`, `@{a:~}`, etc are
unnecessary — only needed for trailing concat, unquoted whitespace in the
default/error, and perhaps interpolation." Settled the design first (decision
authority): **disposition family only** (`:-`/`:?`/`:~`), scalar + array;
disambiguation = only those three open a disposition (every other `:X` stays
literal concat, so `$x:/bin`→`hi:/bin`, `$x:3`→`hi:3` preserved); braces still
required for trailing concat, whitespace, embedded `}`, `$(…)`/backtick operands,
and **inside `"…"`** (DG: clarity). Substring `:off:len`/regex `:s/` stay braced.
Pure lexer desugar: `$x:-foo`→`DollarBrace("x:-foo")`, `@a:~`→`Word("@{a:~}")` —
so validate/transform/codegen/all 3 backends and every rule (SB5020 taint,
SB5046 elide-in-concat, SB5042/SB5048 arity, `@{a:-}`=one-empty per DD#69/#84)
are byte-identical to braced. `$x:-foo` reinterprets from literal `hi:-foo` to
the default (accepted narrow change; `${x}:-foo`/quoting restores literal).
Surfaced (not caused) a pre-existing janet positional-default bug (braced
`${1:-DEF}` w/ arg also returns DEF) → gated vm.bl4 native,c, filed
`janet-positional-default-present`. specs var_modifiers.sh vm.bl1–12 (plan 57→69),
glob_splat.sh gs.25–30 (24→30). 3-mode sweep: 0 regressions, 54 new. DD#90;
journal 242. No new SB code.

## ════════ STATUS (2026-09-04) — array `@{a:-}` = one empty element; `@{a:~}` = elide (DD#69/#84) ════════

**On `hand-parser`.** DG: "I thought `@{a:-}` produced an empty element in a
command." He was right — a latent inconsistency: scalar `${x:-}` empty → one
empty arg (bash parity), but array `@{a:-}` empty → **zero args** (diverged from
bash `"${a[@]:-}"` and from its own scalar `:-`). This also made the just-added
`@{a:~}` redundant with `@{a:-}`. DG chose (A): **`@{a:-}` → one empty element**,
**`@{a:~}` → the sole elide (zero args)** — uniform with `${x:-}`/`${x:~}` and
bash, and resolving DD#84's internal "`:-` = exactly one token" contradiction.
Parser-only (`parse_splat_op`): `:-` now always carries an operand (empty word →
`Some("")` → one empty element via has_word=1), `:~` carries none (zero args);
transform/runtime/janet already mapped Some→one-element / None→zero, so nothing
downstream changed. Verified identical in all 3 backends. Drop-if-empty idiom
migrates `@{a:-}`→`@{a:~}` (wordsplit 22.18: `cat @{e:~}`). specs gs.4 (one empty
arg now, disambiguated), gs.20 (zero args). 3-mode sweep: 0 regressions. DD#69/#84
corrected; journal 241. No new SB code.

## ════════ STATUS (2026-09-04) — `@{a:~}` elide disposition for array splats (DD#69/#84) ════════

**On `hand-parser`.** DG: "for array splat to command, we allow `:-` and `:?`; I
think `:~` should work too." DD#84's `:~` (elide) worked for scalars (`${x:~}`)
but the array-splat parser rejected `@{a:~}` (SB5033) — the exact array form the
scalar operator was modeled on. Now accepted: `@{a:~}` elides. **(Superseded
same day** — the follow-up entry above corrects `@{a:-}` to one empty element and
makes `@{a:~}` the *sole* elide, so `:~` is no longer "identical to `@{a:-}`".**)**
specs glob_splat.sh gs.20–24 (plan 19→24). journal 241.

## ════════ STATUS (2026-09-04) — `close N` builtin: fd close (DD#14; SB4052) ════════

**On `hand-parser`.** DG: "this gives SB4048: exec 9> foo; exec 9>&-". Not a bug
— `exec` fd-manipulation supports open (`9> foo`) and dup (`N>&M`) but the close
form `N>&-` was deliberately rejected (2026-08-29 reject-only fix), leaving no
way to close an fd (pending `fd-manipulation-builtins`). DG chose a **`close N`
builtin** (close-only; dup stays on `N>&M`). Void; `$?`=0/1; per DD#55 a bare
failed close aborts (`close: <fd>: <reason>`), caught/`-`-suppressed doesn't. Arg
must be a literal fd number (else **SB4052**). Modeled on `cd`: `BuiltinCmd::Close`,
`check_close_arg`, `handle_close` (`lower_value_as_int` → `sash_close` →
`emit_status_abort_check`). Fixed `sash_close` to record the errno
(`set_last_errno_raw`) so the abort reason renders (was "error 0"). SB4048
message now points at `close`. Native-only (like `exec`; c/janet ride
`c-janet-exec-support`). specs exec.sh cl.1–cl.8 (plan 11→19). 3-mode sweep: 0
regressions, 8 new. DD#14 amended; `fd-manipulation-builtins` done; journal 240.

## ════════ STATUS (2026-09-03) — relative external paths rejected (DD#23; SB5051) ════════

**On `hand-parser`.** DG: "look at _tmp/17, I'm getting 2 SSA taint backstop
warnings" (a `sash-getopt` arg parser run as `PATH=.:$PATH ./sash 17`). Root
cause: with `.` in PATH, `sash-getopt` resolved to the **relative** path
`./sash-getopt`, and the CST taint check (`command_name_is_taint_source`) used
`starts_with('/')` (absolute-only), so the capture wasn't tainted — only the
fail-closed SSA backstop caught it (a reject where a clean SB5020 should fire).
A relative external path violates DD#23 ("absolute path only / no search
ambiguity"). DG chose **reject** (not absolutize) + CST-taint hardening. Fixes:
`resolve_command_path` skips non-absolute PATH dirs; a source-written relative
head (`./x`, `foo/bar`) → **SB5051** uniformly (top-level, `$( )`, pipeline
stage); `is_valid_command` defers relative heads so `resolve` owns the
diagnostic; `command_name_is_taint_source` → `contains('/')` (defense-in-depth).
Now `PATH=$PWD:$PATH` (absolute) gives the correct **SB5020** on the unblessed
capture. specs errors.sh relpath.1–6 (plan 98→104). 3-mode sweep: 0 regressions,
18 new passing. SB5051 new. DD#23 addendum; journal 239.

## ════════ STATUS (2026-09-03) — `exit` accepts any int (DD#89) ════════

**On `hand-parser`.** DG: "bless … any int value/variable/return should work as
argument to exit, and a DD/specs … but I don't intend to dictate [tagging] vs. a
global — it's up to them." `exit`'s argument is now an ordinary int sink — a
literal, variable, `$((…))`, or `-> int` function return — no compile-time range
limit. Two gaps closed: native `emit_exit` only handled a literal (non-literal →
0; c already used `lower_value_as_int`); type_check rejected `exit $intvar`
(removed). Full i64 internally (readable in END via `$?`), truncated to 8 bits
only at `exit_group` (`exit 300` → END sees 300, process exits 44). DD#89 blesses
the mechanism, NOT an idiom (out-of-range-tag vs plain global = programmer's
choice). `emit_exit` routing flattened (cog 8→2). specs exit_arg.sh 89.1–89.8
(89.8 native — the c exit-skips-END gap). journal 238. 3-mode sweep: 0
regressions.

## ════════ STATUS (2026-09-03) — `$?` readable in the END block (DD#56) ════════

**On `hand-parser`.** DG: reading `$?` as the first action of `END { }` gave
SB5012, but an abort/`exit N` jumps to END with a triggering status — so it
should be readable. DD#56 already designed this ("In END: the status that
triggered END"); the runtime already carries it (`__end_status = __exit_status`
at END entry, `__exit_status` not reset). Validator-only fix: `check_dq_function`
starts the `__sash_end__` body (the END block) in the `Available` `$?` state —
first-action readable like a handling branch; other functions stay `Undefined`
(no leak). `$?` is a full i64 — works in `(( $? > 255 ))`, `int code=$?`.
Behavior: abort → `$?`=failing code; `exit N` → N; normal → last status.

specs signals.sh 56.30–56.33 (plan 19→23; 56.33 guards no function leak). Found
+ filed a pre-existing **c bug** (`emit_c::emit_exit` `goto __exit` skips END for
a main-body `exit N`; native routes through it) — `bug-emit-c-exit-skips-end`;
56.31 gated `native`. janet END is a separate pre-existing gap. 3-mode sweep: 0
regressions. DD#56 amended; journal 237.

## ════════ STATUS (2026-09-02) — capture-condition `if x=$(cmd)` shipped (DD#88) ════════

**On `hand-parser`.** DG: "what would it take to enable `if ! x=$(cmd) {`" +
"the actual exit status of the cmd is wanted, and the stdout need only survive
if the cmd succeeded." That narrowing made a shippable subset of the
`multi-assignment-and-capture-trio` decision — DG chose to implement now.

**DD#88 capture-condition:** in a boolean context, an assignment whose RHS is a
single **external** `$(cmd)` (or pipeline `$(a|b)`) branches on the command's
**actual exit status** (no abort) and captures its stdout into the target
(tainted, bless before use). `$?` is available as the first branch action
(DD#55), so `if x=$(cmd) { use $x } else { echo "failed $?" }` reads the real
code. `!`/`else`/`$?` are uniform with a plain command condition (negated form
loses `$?` magnitude, like `if ! cmd`). Untyped only; `while` included; a
truly-unused captured target is SB5050 (use the non-capturing `if ! cmd`).

Mostly wiring (union of `x=$(- cmd)` + `while read x`): new sigil
`SetTestFromStatus`; `try_capture_condition` in transform; pipeline `checked`
honors `state.suppressed`; validate SB4025 carve-out via the structural
external-path signal + `$?`-provider registration. native + c; **janet gated**
(emits from IR not the sigil; `$(cmd)` capture is a stub — filed
`janet-capture-condition`). Rejects backend-independent. `spec/capture_cond.sh`
88.1–88.14. 3-mode sweep vs HEAD: native 0 regressions, c 0, janet 0
(+14/+14/+6). DD#88; journal 236. The general multi-value return / trio stays
deferred (`multi-assignment-and-capture-trio`).

## ════════ STATUS (2026-09-02) — `if ! out=$(cmd)`: decision filed + 2 bug fixes ════════

**On `hand-parser`.** DG asked whether SB4025's "sequence it" advice works for
`if ! out=$(cmd) { echo bang }`. It doesn't — capture stdout AND branch on a
command's status has no working sash form. DG: "capture this as a decision
task; then address only the existing bugs, but not new features" + "I consider
the bad sequencing advice a bug too."
- **Decision (feature, deferred):** filed `multi-assignment-and-capture-trio`
  — general multi-value `return` + external capture as `(stdout, status,
  stderr)`; crux rule = *binding the status toggles catch vs. abort*; owns the
  SB4025↔DD#55 conflict resolution. No impl until DG settles the axes.
- **Bug 1 (fixed):** SB4025's "sequence it" advice was wrong for a `$(…)`
  capture (a failed capture aborts before any follow-up check). Made
  `nonstatus_in_condition_diag` RHS-aware (`value_is_capture`): capture → honest
  advice (`if ! cmd { }`; capture+branch unsupported); literal → keeps "sequence
  it". specs e.br8/e.br9.
- **Bug 2 (fixed):** `x=$(- /bin/false)` aborted, but DD#55 says `-` discards
  the status (no abort) — spec 55.9 only tested a succeeding `$(- /bin/echo)`.
  The single-command capture fast-path dropped the `-` suppression; `CmdSubst`
  gains `suppressed`, detected in `handle_command_substitution`, gating the
  abort in native + c. spec 55.9b.

3-mode sweep vs HEAD: native 0 regressions, c 0, janet 0 (+1/+1 55.9b;
e.br8/e.br9 all modes). journal 235.

## ════════ STATUS (2026-09-02) — bugfix: positional `${N:OP}` == `${args[N-1]:OP}` ════════

**On `hand-parser`.** DG hit a spurious **SB5042** on a function using `${2:~}`
called with one arg. Root cause = two coupled bugs (the positional `${N:OP}`
disposition form was never tested — all DD#84 specs used `${args[k]:OP}`):
1. **Arity** (`sig_scan_pexp`): a positional `${N:-}`/`${N:~}` was marked
   REQUIRED (fell through to `sig_record_name`, ignoring the operator), so
   `${2:~}` demanded 2 args → SB5042. Now classified optional like the
   `args[k]` path.
2. **Value** (`pe_source_value`): a no-index `${N:OP}` read `Var(lookup("N"))`
   — a scalar var literally named "N" that never exists — so `:-` always
   defaulted and `:~` always elided even when `$N` was present. Now routed
   through `positional_to_value` → `args[N-1]`.

`${N:?}` stays required (compile-time SB5042, like `${args[N-1]:?}`); SB5047
mixing / SB5048 trailing now fire for positionals too. specs
`optional_args.sh` 84.19–84.24 (plan 18→24). 3-mode sweep vs HEAD: native 0
regressions, c 0, janet 0 (+6/+6/0). DD#84 addendum; journal 234.

## ════════ STATUS (2026-09-02) — `read -r` no-op; `read -d` = terminator ════════

**On `hand-parser`.** DG: "we add -r as a no-op and we change -d to be
terminator not separator" (the two top recommendations from the
`read-behaviors-not-wired` survey). Both shipped:
- **`-r`** — accepted as an ignored no-op in `ir::classify_read_flags` (sash
  reads are always raw; was SB5015). Fixes the hand-written `read -r x` tax.
  Bundled `-rd ''` still unsupported (sash bundles no flags).
- **`-d`** — now **terminator** semantics: an explicit `read -d` drops the
  trailing empty from a final delimiter (bash `mapfile -d ''` / `find
  -print0`). Threaded a `terminator` arg to `sash_split_into_array`, set iff
  `delim.is_some()`. Interior empties (`a::b`) kept; the **default** space
  split (no `-d`) keeps separator semantics (DG scoped to `-d`). The getopt
  idiom is now clean — `sash-getopt … | read -a opts -d ''` has no trailing
  `[]`, and `bless e in opts [[ -n $e ]]` passes.

specs wordsplit.sh 22.25–22.27 (plan 77→80). 3-mode sweep vs `0204fe9`:
native 0 regressions, c 0, janet 0 (+3/+3/+1). journal 233. Survey task marked
done; remaining `-n`/`-N`/`-u`/interactive flags stay unimplemented.

## ════════ STATUS (2026-09-02) — read-flag parsers unified; `read -d ''` = NUL ════════

**On `hand-parser`.** DG: "unify them, support -d '', call out other read
behaviors." Replaced **four** independent read-arg parsers (transform ×2,
validate `collect_read_targets`, type_check `register_read_array_target`) with
one shared `ir::classify_read_flags`. Kills the reject-vs-ignore inconsistency
(unknown flags now → SB5015 in BOTH standalone + lastpipe), the lastpipe `-d`
split-form bug, and a dead-var (SB5050) false positive where a delim value
(`read -d : x` → `:`) was mistaken for a target. `read -d ''` now = NUL (root
cause was `expr_to_string` re-quoting; `read_arg_value` unwraps quotes, empty
→ NUL) — the sash-getopt idiom `sash-getopt … | read -a arr -d ''` works
end-to-end (both pipe + redirect; space-bearing fields stay one element).
`read -z` still rejected but consistently. specs wordsplit.sh 22.21–22.24.
3-mode sweep vs `39705d8`: 0 regressions. journal 232.

**Called out (filed `read-behaviors-not-wired`):** (1) `-r` — rejected but
sash is always raw, and bash uses it everywhere → recommend accept-as-no-op;
(2) NUL-terminator trailing-empty (getopt's final NUL leaves a trailing `[]`;
bash mapfile -d '' drops it) → recommend terminator semantics for `-d`;
(3) `-n`/`-N`/`-u`/interactive flags — leave unless needed.

## ════════ STATUS (2026-09-02) — `sash-getopt` vendored (NUL/unquoted) ════════

**On `hand-parser`.** Second half of the readc-drop directive: vendored
`sash-getopt` from `~/contrib/util-linux/misc-utils/getopt.c` (Frodo Looijaard,
GPL-2+; header preserved). Stripped the util-linux deps (nls/xalloc/strutils/
closestream → small inline helpers), kept the getopt(3)/getopt_long core +
`-o`/`-l`/`-n`/`-a`/`-q`/`-Q`/`-T`/`-U`. **Patched the output to NUL-separated,
unquoted fields, no trailing newline** — `emit(s)` writes `s\0`; `print_normalized`
(shell-quoting) removed; `-s`/`-u` accepted-and-ignored. `sash-getopt -o ab: --
-a -b val 'has space'` → `-a\0-b\0val\0--\0has space\0` (space-bearing arg is one
field). Build: `target/release/sash-getopt` (`cc -O2`) in mkfile `build`;
`sash-getopt-test` in `test`; `contrib/test-getopt.sh` (5 checks, all pass).
journal 231.

**OPEN — the NUL-read consumer.** The runtime split is already NUL-safe
(`sash_split_into_array` length-based; `read -a` splits on `flags.delim`); only
the *surface* for a NUL delimiter is undecided (`read -z` is rejected today,
SB5015/cov.20). Flagged in `switch-free-read-family` for a DG decision.

## ════════ STATUS (2026-09-02) — `readc` DROPPED (getopt-NUL route) ════════

**On `hand-parser`.** DG dropped `readc` (cooked read, shipped `688616a`) after
an ICE on `… | readc -a out`. Cooked-read parsing of getopt's shell-quoting was
judged too fragile; the sanctioned getopt idiom is **NUL-splitting** — a patched
`sash-getopt` (NUL field separation, no quoting) piped into a NUL-delimited
`read`. Removed all readc + cooked machinery (runtime `sash_split_first_word_
cooked`/`sash_deescape`; the `cooked` SSA field + all plumbing; native/c codegen
cooked paths; readc recognition in BUILTINS/taint/type_check/lastpipe/janet).
Kept the independent `insert_read_target_types` c-fix. `readc` → SB5011; the ICE
command is now a clean error. 3-mode sweep vs `ddc237a`: 0 regressions. journal
230. **Next: vendor `sash-getopt`.**

## ════════ STATUS (2026-09-02) — DD#86 Phase 5: converter drops `local`; docs. DD#86 COMPLETE ════════

**On `hand-parser`.** DD#86 Phase 5 (migration/cleanup) — the converter no
longer emits `local` (it produced uncompilable sash): bare `local x=5` → `x=5`
(auto-local), `local -i x=5` → `int x=5`, `local -r` → `const`. Round-trip
verified; 18 sash-convert tests pass (`coerce_local_i_to_int` updated). Docs:
GUIDE dropped the `local int x` example; DD#35 amended (lifetime axis retired,
`str` added). Shadow-rename refinement filed (`converter-shadowing-local-
rename`). journal 229.

**DD#86 is COMPLETE (phases 1–5):** 1 `str` keyword; 2 drop `local`/`local fn` +
structural scope; 3 no-shadow (SB5049); 4 dead-variable rejection (SB5050);
5 converter + docs. No `local`, no shadowing, structural scope, use-before-and-
after (SB5031 + SB5050).

## ════════ STATUS (2026-09-02) — DD#86 Phase 4: dead-variable rejection (SB5050) ════════

**On `hand-parser`.** DD#86 Phase 4 — the dual of DD#62 SB5031: a variable
assigned but never read is a dead variable (**SB5050**). A global is dead if
read nowhere in the program; a function-local if read nowhere in its function.

**DG rulings (2026-09-02):** (1) more-specific error wins — dead-var runs LAST,
after taint/type/transform (moved out of `validate()` to end of the front-end;
`enforce_transform_diags` now runs for every backend); (2) `read`/`mapfile` targets and `for` variables ARE checked (for-var no longer exempt);
(3) `: $v` is the acknowledgment escape hatch (a real read per DD#81 arity).

**Reuses the read-enumerator (DG):** inverted the SB5031 read-traversal via a
shared `ReadSink` (Flag = SB5031, Collect = dead-var) — one read-recognition,
so they can't diverge. SB5031-specific opt-outs (`${x:-…}`/`@a`, DD#58 sexpr
names) live in the sink so Collect still counts them. Added value-fn bare
operands (`len`/`join`/`keys`) + fixed two enumerator gaps (ArithTest `(( x ))`
reads, bless array/map operand) that benefit SB5031 too.

**Read-completeness proven:** 65 native breaks (hand-rolled, with FPs) → after
the enumerator reuse, **every break was genuinely dead — 0 false positives**.
Rewrote 14 genuinely-dead specs (`: $v`/`: @a`/`bless` to read the var).
specs scope.sh 86.13–86.20. Full 3-mode sweep vs `891bcf9`: **native 0
regressions +8, c 0 +8, janet 0 +8**. fmt/clippy clean; lib tests pass;
complexity gate cleared. journal 228. Filed
`bare-collection-name-operand-asymmetry` (DG asked).

**Next:** Phase 5 (converter `local`→drop; GUIDE/SUBSET; DD#35 amend).

## ════════ STATUS (2026-09-01, cont) — DD#86 Phase 3: no shadowing (SB5049) ════════

**On `hand-parser`.** DD#86 Phase 3 — a typed re-declaration
(`str`/`int`/`array`/`map`/`const`) inside a function of a top-level global name
is a redeclaration error (**SB5049**); a bare assignment writes the global.
`check_no_shadow` in validate.rs collects top-level decls (globals, not
descending into fn bodies) then scans each fn body for typed decls colliding
with a global. specs scope.sh 86.7–86.12 (native,c,janet). Full 3-mode sweep vs
`dcf2c97`: native 0 regressions +6, c 0 regressions +6, janet +8. journal 227.

**Next:** Phase 4 (dead-var), Phase 5 (converter + docs).

## ════════ STATUS (2026-09-01, cont) — DD#86 Phase 2: drop `local` ════════

**On `hand-parser`.** DG: "let's finish the local-related work." DD#86 Phase 2 —
removed the `local` keyword and **all** its machinery (lexer/parser `Tok::Local`
+ `parse_local` + `is_local`; IR `DeclKeyword::Local` + `Function::is_local`;
transform `collect_local_vars`/`save_caller_locals`/`restore_caller_state`/
`local_fn_scope`/`__local__`; validate/resolve/janet arms). DG confirmed **drop
`local fn` (DD#18) entirely** — folded into Phase 2 since it shared every piece
of machinery. `local x` / `local fn` now → SB5011 (converter drops it, Phase 5);
a nested `fn` is an ordinary top-level function.

Structural scope was already mostly in place (bare `x=v` in a fn writes the
global; a new fn-only name is invisible at top level via SSA versioning +
SB5031). `local` only added a *shadow* — pure deletion.

**Latent bug surfaced + fixed:** a function-local `const int x=5` tripped SB4044
because the body is lowered twice (template + inline) and `readonly_vars` leaked;
`local const` masked it (keyword was `Local`, never populated `readonly_vars`).
Fixed with `enter/exit_fn_body_scope` snapshot/restore of `readonly_vars` around
every fn-body lowering — fn-local const is now isolated while global-const and
within-fn reassignment enforcement are preserved.

Specs: rewrote the 7 `local` specs (functions 18.1–18.4, types 35.1, arithmetic
ivc.8, errors 55.7). Full 3-mode sweep vs `688616a`: **native 0 regressions,
c 0 regressions**, janet +2. fmt/clippy clean; lib unit tests pass. journal 226.

**Next:** Phase 3 (no-shadow), Phase 4 (dead-var), Phase 5 (converter + docs).

## ════════ STATUS (2026-09-01, cont) — `readc` cooked read (switch-free step 1) ════════

**On `hand-parser`.** DG: "capture the larger question, plus a survey of all
switch use into a decision task; implement readc now." The decision task
(`switch-free-read-family`, filed earlier) holds the larger direction
(`read`→`readr`/`readc`, drop `-a`/`-f`/`-d`/`-z`). This session lands the first
concrete step: **`readc`**, a cooked read (honors `\`-escapes; `\<ws>` doesn't
split, field de-escaped) — the bash-default `read`, where sash `read` stays raw
(bash `read -r`). Unblocks the eval-free getopt-csh idiom.

Runtime `sash_split_first_word_cooked` + `sash_deescape` (NUL-term per DD#87);
`cooked: bool` on `ReadLine`/`ReadLineMulti`; `readc` recognized everywhere
`read` is (BUILTINS/SB5011, taint, type_check, lastpipe, janet while-read).
Native + c codegen wired (single-var de-escapes the line; multi-var:
cooked-split middle fields, de-escape the remainder). janet recognizes readc
but treats it raw (cooked deferred → `janet-readc-cooked`).

**Pre-existing c bug fixed:** `emit_c::collect_inst_var_types` never declared a
read target that's assigned-but-never-read (native always did) → C compile error
on `… | read a b; echo $b`. Reproduces at HEAD with raw `read`. Fixed via
`insert_read_target_types`.

**Not a bug:** an early spec exposed sash double-quote `\\` keeping two
backslashes vs bash's one — that's DD#64 (no-shell-backslash), ratified; spec
dropped.

specs `spec/wordsplit.sh` 22.20–22.24 (native,c; janet skip). Full 3-mode
sweep vs HEAD `8e01b0a`: **native 0 regressions +5, c 0 regressions +5**,
janet 0/0. fmt+clippy clean; complexity gate cleared (extracted
`emit_read_line_call`/`emit_read_last_var`/`emit_read_field_split` native,
`emit_readline_last`/`insert_read_target_types` c). journal 225.

## ════════ STATUS (2026-09-01, cont) — bless-fallback via `||` ════════

**On `hand-parser`.** DG wanted "use DEFAULT when a bless fails" (the unbuilt
journal-036 `${port:~8080}` idea). Instead of a new operator, DG asked whether
`||` solves it — and it should, but `bless v [[t]] || v=dflt` **mis-compiled**
(aborted on fail; ran the fallback even on valid input). Root cause: a bless as
the left of `&&`/`||` lowered to the *abort* form, not the catchable if-bless
form. Fixed `lower_list_stmt` to rewrite `bless … || X` → `if bless {} else {X}`
(and `&& X` → `if bless {X}`), reusing `lower_if_bless`. Now `bless v [[t]] ||
v=dflt` = validated-or-default, clean either way; fallback trusted, not
re-validated (caveat programmer). Bare bless still aborts. specs taint.sh
bfb.1–6; 0 regressions, +12 passing. DD#24 amended; `:~` stays DD#84 elide (the
`colon-tilde-elide-vs-bless-fallback` task is closed, no `:!` operator). journal
224.

## ════════ STATUS (2026-09-01) — DD#86 scope model (Phase 1) + DD#87 runtime NUL fix ════════

**On `hand-parser`.** Two things this session, both pushed:

**DD#87 (pushed `29e6426`) — critical runtime bug.** Found while testing DD#86:
interpolating an int var + a string var into one string and passing it to an
**external** command produced trailing garbage (`/bin/echo "n=$n s=$s"`), while
the builtin echo was clean. Root cause: sash-rt string builders allocated exactly
`len` bytes with no NUL; execve reads argv as C strings → read past the end.
Masked because specs use the builtin (length-based) echo. DG chose approach **B**
(whole-runtime invariant): every sash string NUL-terminated at `[len]` via a new
`alloc_cstr` helper; `sash_substring` now copies; `sash_itod`/`sash_regex_replace`
re-terminate. specs `spec/extern_args.sh` xa.1–8; 0 regressions, +16 passing.
DD#87, journal 222.

**DD#86 Phase 1 (this commit) — the `str` keyword.** DD#86 (variable scope: no
`local`, no shadowing, use-before-and-after) is fully ratified with DG and is a
5-phase change. Phase 1: `str` declares a string with meaningful-zero `""`
(parallel to `int`), so a string global can be declared at top level without
`=""`. DG picked `str` over `var` (names the type; 0 spec collisions vs 50).
Also fixed a pre-existing janet bug (`int n` no-init → malformed `(var n )`;
now `0`). specs `spec/scope.sh` 86.1–6 (all modes); 0 regressions, +18 passing.
DD#86, journal 223.

**Phases 2–5 remain** (filed `notes/tasks/dd86-scope-phases-2-5.md`): drop
`local` + structural scope (top-level=global, new-in-fn=local, frictionless
global R/W); no-shadowing/redeclaration error; dead-variable rejection (dual of
DD#62, no warnings); abandon `local fn` (DD#18); converter + all `local` specs +
DD#35 amend. Each phase lands with its spec updates to keep the gate green.

## ════════ STATUS (2026-08-31, cont.5) — comments are lexical trivia (DD#85) ════════

**On `hand-parser`.** DG: "trailing comments seem to be broken." They were —
`echo hi # c`, `int foo=1 # c`, `for x in a b # c`, `if true # c`,
`echo hi | cat # c`, `return 0 # c` all → SB4014. Root cause: value-collecting
parse loops recognized only `Newline|Semi|Eof` as terminators, so a trailing
`Tok::Comment` hit the value parser.

DG chose **B — comments as lexical trivia** (over patching every parse
position): the Scanner now drops `Tok::Comment` (`next_token_skip_comments`),
and `StmtKind::Comment` + `parse_comment_stmt` are removed. Safe because
`.sash_source` embeds source **verbatim** (round-trip unaffected), comments were
already no-ops in native/c, and `--emit-source`/`--annotate` are already
rejected. Only loss: non-gate janet stops rendering comments (DG accepted).
DD#85; journal 221.

`next_token` left byte-identical (grandfathered); the filter is a tiny wrapper.
Behavior matches bash (`# ; x` runs nothing after `#`; same-line `# c }` eats
the `}`; shebang dropped; `#` in `$(( ))` stays a Word per DD#72). specs
`spec/comments.sh` cmt.1–11 (all modes; cmt.11 native,c). Full 3-mode sweep vs
`da02f3a`: **0 regressions, +32 passing**. Lib tests green.

**Open follow-up (filed, needs a DG decision):**
`notes/tasks/arith-block-separators-and-comments.md` — DG observed that
multi-line `(( ))` disallows end-of-line comments, and that the `;`/`,`/newline
separator story inside `(( ))` is inconsistent (for-header allows `;`, standalone
rejects it; `,` works). Three linked decisions: (1) is multi-line `(( ))` one
expression or a sequence block (newline ≡ comma)? (2) `;` ≡ `,` in a standalone
`(( ))`? (3) allow `#` comments inside arith (DD#72 freed `#`)? All are
deliberate-divergence-from-bash calls; today sash matches bash on each.

## ════════ STATUS (2026-08-31, cont.4) — `!` on the terminal pipeline segment ════════

**On `hand-parser`.** DG's `… | ! grep -q z` (journal 219's line) uses a `!` on
a non-first pipeline stage. Designed + implemented **terminal-segment
negation**: `a | ! b` negates the FINAL stage's status (0↔nonzero) before the
DD#55 worst-of; a non-terminal `!` (`! a | b`, `a | ! b | c`) stays SB4040;
`-`/`!` don't combine on one segment (SB4019 both directions); `return ! X | Y`
→ SB4040 (was SB4014).

Bug fixed: `append_pipeline_stage` flattened the parsed `! b` stage and **dropped
the negation**. New `StmtKind::Negated` per-segment wrapper (symmetric with
`Suppressed`) → `negated: Vec<bool>` mask on SSA `Pipeline`; native/c/janet all
invert the negated stage's status in the pipeline wait. Standalone `! cmd`
(`Pipeline.negated: bool`) untouched. `parse_return` cc 14→7 (extracted producer
head).

DD#70 addendum + DD#55 pointer. specs `spec/errors.sh` bng.7–12 (plan 91→97), all
three modes. Full 3-mode sweep vs `e38ee4f`: **0 regressions, +18 passing**. Rust
suites: failure set identical at baseline (stash-verified; pre-existing rot).
journal 220. The `X | ! Y` open item from cont.3 is now **resolved**.

## ════════ STATUS (2026-08-31, cont.3) — `return <composite producer>` (SB4037 fix) ════════

**On `hand-parser`.** DG (demo): `fn f { return echo abc | ! grep -q z }`
reported **SB4037** but is clean. Root cause: `parse_return` parsed only the
first producer unit, so `return A | B` folded to `Pipeline([return A, B])` — the
`return` trapped in stage 0, `B` uncaught (SB4037 in a predicate; a silent abort
as a statement). Whole class `return A|B` / `A&&B` / `A||B` / `A>f` broken.

Fix: `parse_return` now folds the producer through `maybe_wrap_redirects` (the
same composite-folding a statement gets) before `ReturnProducer`. Lowering
already handled `ReturnProducer(Pipeline/List)` (condition-context lower +
un-check last). One-line-ish parser change; cc 12 / cog 4 clears the gate.
Verified vs bash (pipeline match/no-match, `&&` both-true/second-false, `||`).

specs `spec/functions.sh` 74.19–74.23 (plan 63→68): pipelines 74.19/20 all
modes; `&&`/`||` lists 74.21–23 native,c only. **0 regressions** (3-mode diff vs
HEAD `071fbb7`); +12 passing (5 native + 5 c + 2 janet), 0 new failing. DD#74
producer table amended; journal 219.

janet gated: its AST return-producer emit **collapses** an `&&`/`||` list to one
operand (emits `(if true 0 1)` regardless) — filed `janet-return-composite-
producer`. Pipelines are genuine in janet.

**Open (raised separately, needs DG):** `X | ! Y` — a negated **non-first**
pipeline stage silently drops the `!` (inverted result) + aborts as a statement.
Pre-existing, inconsistent with the SB4040 rejection of `! X | Y`. DG's line uses
it. Fork: reject (SB4040/DD#70-consistent) vs support (negated final stage
governs status).

## ════════ STATUS (2026-08-31) — DD#84 optional trailing args + `:~` elide (native+c) ════════

**On `hand-parser`.** DG-designed feature (discussed + ratified this session):
make a trailing argument optional and let a scalar token *disappear* from a
list. Two capabilities: **(A)** a trailing `${args[i]}` read only via `:-`/`:~`
is optional (arity `[min,max]`; mixing bare+disposition → SB5047, required-after-
optional → SB5048); **(B)** `${x:~}` / `${args[i]:~}` *elides* — zero tokens when
empty, one otherwise — in an elidable position (command arg / array element),
rejected elsewhere (SB5046) and never takes a word (SB4051). DD#84; journal 218.

Lowering: `:~` → `SsaValue::ElideEmpty`; native + c list builders do a runtime
0-or-1 on emptiness (argv + array literal, compacting). specs
`spec/optional_args.sh` (84.1–18). Full 3-mode sweep vs `aea691e`: **0
regressions; +18 native/c, +4 janet** (arity refactor touched every fn def —
clean). janet deferred (no `args` model; `janet-optional-args-elision`); int-cell
identity deferred (elides on empty string).

## ════════ STATUS (2026-08-31, cont.2) — dispatch compact-op dead branches removed ════════

**On `hand-parser`.** The last two grind never-runs (`extract_from_literal`,
`extract_leading_operator`) resolved to shadowed-dead: an eprintln probe over a
full `[[ ]]` battery showed neither ever fires — the lexer always splits `==`/`=~`
into its own token, so a compact operand always arrives as `Concat([subj, "==",
pat])` (middle-part branch), never a glued `Literal` or glued last part. Removed
both fns + the two dead call-site branches; remove-then-run 0 regressions (3-mode
sweep vs `790711b`). Task done.

**Grind endpoint reached** — spec-only native-core never-run is now only the two
unit-tested intentional ICEs + `type_check::fmt` (documented artifact).

## ════════ STATUS (2026-08-31, cont.) — grind tail resolved (CLI + ICEs + queue) ════════

**On `hand-parser`.** Closed out the coverage grind tail per DG:
- **CLI switches → `spec/cli.sh`** (new; cli.1–6, all modes): `--manifest`
  (emit_manifest), `--extract` (dump_sash_source), `--sysroot=` (set_sysroot +
  sysroot_file_exists). All four lib.rs handlers flipped 0→>0.
- **Intentional ICEs → unit tests**: `panic_payload_message` (3 cases);
  `redirect_expr_zero` **removed** → the three silent-empty `(0,0)` redirect-target
  fallbacks are now a **loud ICE** `redirect_target_ice`, with a unit test that
  builds a FunctionBuilder and asserts the panic. Full sweep after making it loud:
  0 regressions (nothing real reached the silent-empty path).
- **Queued**: `extract_from_literal`/`extract_leading_operator` (dispatch
  compact-operator Literal/leading-op branches) → `grind-dispatch-compact-operator-branches`.
- **Explained**: `type_check::fmt` (uncalled `Display for TypeError` — errors
  surface via `.message`/`.span`, not `{}`).

Full 3-mode sweep vs `0500841`: **0 regressions, +6 cli each**. 16 lib unit tests
pass. Spec-only never-run endpoint = the two unit-tested ICEs (can't be
spec-driven) + the two queued dispatch branches; all else covered or documented.

## ════════ STATUS (2026-08-31) — pipeline-stage redirect targets fixed (grind) ════════

**On `hand-parser`.** Resumed the coverage grind; probing never-run redirect fns
found a bug (DG ruled "fix to support"). Pipeline-stage redirects silently
dropped every non-literal target (`echo hi > $f | cat` etc. leaked to the pipe
instead of the file) — the stage path had its own `ir_expr_to_ssa_value` that
hardcoded SSA version 0 and dropped `${x:-f}`/`$a[i]`. Fixed: route
`emit_stage_redirects` through the top-level `lower_ir_redirect_expr`; spill
stage targets in `push_command_stage` (threaded `block` through the pipeline
builders); deleted `ir_expr_to_ssa_value`. Journal 217.

specs commands.sh 14.26–31 (stage `$f`/`${x:-f}`/`$a[i]`; plus never-run
`&>`/`> '…'`/`> "$x.txt"`; plan 40→46, native,c). Full 3-mode sweep vs
`cc5b8aa`: **native/c 0 regressions, +6 each; janet 0**. Coverage re-measured:
native-core never-run **25 → 21** (4 flipped/removed).

**Grind continues** (21 left, triaged in `coverage-accounting.md`): bucket-A
diagnostic specs (`find_dollar_question`/`expr_invokes_random`/
`check_arg_default_taint`), bucket-B `strip_matching_quotes` (remove), defensive
`redirect_expr_zero`/`panic_payload_message`, CLI-only fns.

## ════════ STATUS (2026-08-30) — here-strings `<<<` dropped (DD#83, SB4050) ════════

**On `hand-parser`.** DG ruled `<<<` out of the language. It's exact sugar over
`producer | consumer` in sash — no subshell (so `echo $x | read v` already
persists `v`), provenance-based taint (clean/tainted is the source, not the
channel), no word-splitting → `<<< X` ≡ `echo-of-X | consumer` for every
operand, byte-for-byte. Pure sugar, so removed. DD#83, journal 216.

`<<<` → **SB4050** at parse (message names the rewrite). Removed
`RedirectOp::HereString` + all downstream (native/c/janet emitters, the
`sash_herestring_redirect` runtime symbol, the validate taint exception, the
`ssa_taint` clean-literal source, the transform special-case) — 9 sites, driven
by exhaustiveness. specs commands.sh 14.25 (SB4050 all modes; plan 39→40);
taint.sh 24.6 rewritten to `echo hello | read x`. Full 3-mode sweep vs
`b8d2a13`: **0 regressions, +1 each**. Supersedes
`bug-herestring-nonscalar-data-empty`; filed `converter-herestring-to-pipe` (P3).

**Next:** resume the never-run grind (`grind-continue`) — native gate clean.

## ════════ STATUS (2026-08-30) — redirect target must be a scalar (SB4049 + spill) ════════

**On `hand-parser`.** DG's ruling on the redirect-target silent-empty bug
(`bug-redirect-nonsimple-target-empty`, surfaced by the grind): **support
scalars, reject complex.** Journal 215; bug task marked done.

`> ${x:-f}` / `> $a[i]` silently redirected to an empty filename; `> $(cmd)` /
`> $((x))` too. Fix in two parts: (1) **parse reject SB4049** —
`reject_nonscalar_redirect_target` on every file-redirect target
(`>`/`>>`/`<`/`<>`/`&>`; not `<<<`, not `>&`/`<&` fd-dups); allowlist =
literal/quoted/`$x`/`${…}`/`$a[i]`/concat-of-those. (2) **transform spill** —
`lower_redirected_stmt` lowers a ParameterExpansion/Subscript target via the
normal `expr_to_value` into a synthetic `__redir_target_N` scalar and rewrites
the redirect to read it (codegen already handles `$var`); no new SSA type,
native+c inherit it. janet unchanged (no file redirects; reject still applies at
parse). specs commands.sh 14.19–24 (plan 33→39); taint.sh 60.41/60.52 rerouted
through a scalar (direct `$(join …)` target now SB4049 before SB5020). Full
3-mode sweep vs `bbeaf24`: **native/c 0 regressions, +6 each; janet 0, +2**.
fmt+clippy clean, complexity gate clear.

**Filed:** `bug-herestring-nonscalar-data-empty` (P2) — the `<<<` DATA path has
the same silent-empty class + a likely missing-newline bug; left for a DG ruling
(here-strings feed stdin data, so "reject complex" doesn't transfer 1:1).

**Next:** resume the never-run grind (`grind-continue`) — native gate clean.

## ════════ STATUS (2026-08-29) — `$(( args[i] ))` in a function fixed (arith args resolve) ════════

**On `hand-parser`.** DG "fix now" — the latent native bug flagged while
answering "were there more bugs". Journal 213.

`$(( args[0] ))` in a function was an SB5031 false positive masking a codegen
ICE: the arith-subscript lowering never resolved `args`→`__argsN` (the word path
does). Fix: (1) `subscript_read_to_ssa` calls `resolve_args_name` (mirror
`subscript_to_value`); (2) `check_function_scope` adds `args` to `fn_known`
(DD#75 §5). The defensive `find_array_slot` ICE stays (it's a guard, not dead
code). `f(){ echo $(( args[0]+1 )); }; f 5` → 6. Unmasking routes arith args
through the pre-existing positional rules (non-contiguous → SB5044, over-arity →
SB5043; parity with `${args[i]}`); top-level `args` still SB5031. specs
arithmetic.sh bsx.8/9/10. Full 3-mode sweep vs `beee24e`: **0 regressions**; +3
native/c, +1 janet. janet arith subscript still gapped (`janet-arith-subscript-
read`).

**Native gate has no known latent bugs now — clear to resume `grind-continue`.**

## ════════ STATUS (2026-08-29) — redirect fd-dup target validated (SB4048) ════════

**On `hand-parser`.** Answering DG's "were there more bugs before resuming the
never-run grind?", I probed the sibling operator-dispatch parsers for the
silent-degrade class. arith/`[[ ]]`/subscript are clean (reject); **redirects
had the bug**. Journal 212.

`>&`/`<&` fd-dup targets weren't validated — `emit_dup_output` did
`s.parse().unwrap_or(2)` / `_ => 2`, so `>&foo`, `>&999x`, `2>&1x`, `>&-` all
silently dup'd to fd 2. Fix (DG: **reject-only**): `reject_nonfd_dup_target` in
`parse_redirect` → **SB4048** for any non-numeric/malformed target; `>&N` /
`N>&M` / `<&N` still work. `RedirectOp` gained `Copy`. specs coverage.sh
cov.86-88 (all modes). Full 3-mode sweep vs `d46d051`: **0 regressions**, +3
each. Follow-up filed: fd dup/close as **builtins** (`fd-manipulation-builtins`).

**Next:** resume the never-run grind (`grind-continue`) — the native gate is
clean (only latent native bug left is `arith-args-subscript-codegen`, masked by
SB5031; the rest are c/janet baselines).

## ════════ STATUS (2026-08-29) — no-ambiguity param fallthrough (SB4047) ════════

**On `hand-parser`.** DG: "need to prevent ambiguity — if the compiler doesn't
know what something does, that's very bad." (Journal 211, follows 210.)

`parse_parameter_expansion` used to fall through to a bare `${name}` for any
unrecognised trailing content, **silently dropping** it (`${x&y}`, `${x:foo}`,
`${x.y}`, `${x:}`, `${x y}` → `$x`). Now the fallthrough carries the raw
remainder on a sentinel `unrecognized` operator and rejects → **SB4047**.
Two-layer model: SB4034 = known-dropped op (specific guidance); SB4047 =
genuinely unknown content. Everything sash understands (`:-`/`:?`/`:off:len`/
`:s/`, subscripts, `@`→SB4015, `!`→SB4003, strlen) is recognised earlier, so
only garbage reaches SB4047. specs var_modifiers.sh vm.un1-4. Full 3-mode sweep
vs `e4269d1`: **0 regressions, 0 gone** — nothing relied on the silent-accept.

## ════════ STATUS (2026-08-29) — non-colon param family rejected + SUBSET.md deleted ════════

**On `hand-parser`.** DG-directed (journal 210). "Isn't `${x-4}` invalid sash?"
— yes, and it was a silent-degrade gap.

- **Bug:** `split_operator` didn't recognise the non-colon default family
  (`-` `+` `=` `?`), so `${x-4}` parsed as bare `${x}` with `-4` **silently
  discarded**; `${x+4}` even returned `$x` (5) not the alternate (bash 4) — a
  silently wrong answer.
- **DG:** "clarity favors a single spelling. DD65 gives the answer." → reject all
  non-`:-`/`:?` forms **SB4034**, no compiler coercion (conversion `-`→`:-` is
  sash-convert's separate job). Kept spelling = the colon forms `:-`/`:?`.
- **Fix:** add `-`/`+`/`=`/`?` to `split_operator` one-char ops +
  `is_dropped_param_operator`. Array path self-guarded; converter (tree-sitter)
  unaffected. specs var_modifiers.sh vm.dr19-22. DD#65 non-colon addendum.
- **SUBSET.md deleted** (DG: "worse than useless" — claimed non-colon coercion,
  listed dropped `:=`/`:+` as supported). Repointed README/AGENTS/GUIDE/REVIEW at
  `notes/decisions/` as the subset authority. Corrected the journal-209/DD#65
  bare-`-` note (was wrong).

**Verification:** full 3-mode sweep vs `8979f1f` → **0 regressions**; +4 each
(vm.dr19-22). fmt+clippy clean.

## ════════ STATUS (2026-08-29) — int `:-` identity-element default (all 3 backends) ════════

**On `hand-parser`.** DG-directed, closing the open bug
`bug-cjanet-default-on-int-var`. Journal 209.

- **Semantic (DG):** `:-` triggers the default on the **identity element** of the
  subject's type — `""` for strings, **`0` for ints**. `${int_x:-D}` ≡
  `(x == 0 ? D : x)`. DD#62 meaningful-zero preserved; `:-` just picks 0 as its
  trigger. Was a **native behavior change** too (native returned `0` for
  `int x=0; ${x:-99}`) — all 3 backends had wrongly string-emptiness-tested an int.
- **Literal-only default (DG):** an int `:-` default `D` must be an **int
  literal** (decimal/`0x`/`0o`/`0b`) → else **SB4030** at type_check. Unifies with
  the arith-context rule (DD#77 D4).
- **Impl:** transform lowers int `:-` to `Ternary{x==0 ? D : x}` (native+c via
  SSA); janet `emit_pe_default` int branch `(if (= name 0) …)`; type_check fourth
  pass `check_int_value_defaults` (treats `$(( ))` opaque — no dup with the arith
  default check). DD#65 addendum; new task `janet-int-array-and-cell-default` (P3,
  janet int arrays broken independent of `:-`).

**Verification:** full 3-mode sweep vs HEAD `91dddfb` → **0 regressions** native/
c/janet; +6 native, +6 c, +5 janet new passing, c:cov.78 skip→pass. coverage.sh
cov.78/80/81 (positives), 82/83/84 (rejects), 85 (int-cell, native+c). fmt+clippy
clean; new fns pass the cog/cc gate.

## ════════ STATUS (2026) — 2 more grind bugs fixed (#3 arith, #4 redirect) ════════

**On `hand-parser`.** Two more grind-surfaced bugs closed, DG-directed ("fix #3;
reject and delete"). Journal 206.

- **Bug #3 — nested/embedded arith assignment dropped → FIXED (fork A).**
  `(( (a=2) ))`, `(( 1+(a=2) ))`, `(( a=2,(b=3) ))` (statements) used to silently
  leave the var unchanged. `lower_arith_test_stmt` now **hoists** an embedded
  assignment in a `(( … ))` **statement** into a real store
  (`hoist_arith_mutations`), matching bash. Consistent subset rule (SUBSET.md): a
  mutation belongs in a `(( … ))` statement only — a mutation in a `$(( … ))`
  **value** → SB4020, in an `if`/`while` **condition** → SB5007
  (`validate_arith_in_condition` now checks `arith_has_mutation`, so
  `if (( (a=2) ))` joins `if (( x=5 ))`), and an embedded `++`/`--`, `a[i]`, or
  mutation under `&&`/`||`/`?:` in a statement → SB4020 (never silent). The
  half-wired value arms (`incdec_to_ssa`/`compound_assign_to_ssa` + the mutation
  arms of `arith_to_ssa`) were **deleted** — documented `unreachable!`, proven by
  a 0-new-failure sweep. arithmetic.sh 31.25-36.

- **Bug #4 — redirect on a non-command → FIXED (reject + delete).** A redirect on
  a function-def / declaration / assignment is one clean spanned **SB4046** (was:
  misleading SB5011 / SB4014, and **silently dropped** for `x=5 >f`). Rejected at
  `wrap_pipeline_part`; dead `lower_redirected_function` + `function_redirects`
  machinery deleted. commands.sh 14.13-16.

New codes: SB4020 reuse (arith mutation placement), **SB4046** (redirect on
non-command). No new DD. **Verification:** two fresh HEAD-vs-change baselines
(native + c) → **0 regressions** each; 16 new tests pass native+c; `test_status`
0 native/0 c failing. fmt+clippy clean.

**Taint follow-up (2026-08-29):** the P1 I filed
(`bug-nested-bare-cmdsub-bless-taint.md`) was **mischaracterized**. DG: nested
tainted `$( … $() … )` is **correctly** rejected SB5020 — the inner tainted
result is read as an argument to the outer command, and `bless x` covers only
the outer assignment. No bypass / nondeterminism (660/660 deterministic reject);
the "flip-flop" was a **chk-id collision** (commands.sh 14.7/14.8 masked by
wordsplit.sh 14.7/14.8). Fixed: commands.sh 14.7/14.8 now expect SB5020,
renumbered to 14.17/14.18; broader collision filed
(`bug-commands-wordsplit-14x-chk-collision.md`, P2).

**Spec chk-id de-collision (2026-08-29):** audited the whole suite for the same
masking hazard and fixed **every** collision (`decollide-spec-chk-ids.md`, done):
4 cross-file pairs (arithmetic↔types `58.10-15`; convert↔types `30.7`;
misc↔wordsplit `20.3-6`; types↔wordsplit `62.11`) + within-file duplicates
(types.sh had 2-3 blocks reusing `30.8-11`/`33.11-18`; errors.sh `55.30`). Only
ids moved (tests unchanged, plan counts preserved); native+c 0 failing, and no
de-collided pair unmasked a real failure. The task carries a re-runnable audit
(cross-file collisions AND within-file duplicate `chk` must both be empty).

**Coverage grind round (2026-08-29, journal 208):** re-measured per-function
count. bug#3/#4 already removed 3 never-run fns (95.03%→95.30%); this round
flipped 3 more via `coverage.sh` cov.76-78 (`skip_balanced_squote/dquote`,
`parameter_expansion_type`) → **95.50% (67 never-run)**. cov.78 surfaced a real
**c/janet bug**: `${int_var:-default}` miscompiles (c `x_ptr` undeclared; janet
`(length int)`) — filed `bug-cjanet-default-on-int-var.md` (P2), cov.78 guarded
native. Reclassified (accounting): `collection_reader_taint`/`bare_name_tainted`
shadowed-dead (join/keys→intrinsics, removable); `extract_from_literal`/
`extract_leading_operator` need the dispatch-glued form (P3); `scan_base_hash_span`
reachability discrepancy (needs arith-tokenizer trace).

**Next (DG mandate — keep draining the A-bucket via specs):** remaining named
never-run functions (sig_scan_case/sig_record_arith_index, c-backend
emit_dup_output/latest_var_as_str/emit_arr_keys_joined, taint
collection_reader_taint, redirect/arith scanner sub-paths, fully-glued
extract_from_literal). B-candidates to confirm-then-remove: try_keys_intrinsic.
C: sysroot CLI. See `notes/coverage-accounting.md` for the full classification.

## ════════ (prior) coverage accounting reframed; 2 P1 bugs fixed ════════

**Coverage:** lines 87.69% → **92.28%**, functions → **95.03%** (native+c).
Source-based LLVM coverage is **optimization-independent** — the per-function
`count` from `cargo llvm-cov report --json` is **authoritative**. A passing
feature probe does NOT prove a specific fn ran. Two earlier P1 bugs fixed:
- **const-reassign panic** → **SB4044** (journal 204). Fork C: validate
  `check_const_reassignment` (spanned) + `define()` spanless backstop.
- **intrinsic-arity fallthrough** → **SB4045** (journal 205). Fork C: validate
  `check_cmdsub_arity` (spanned) + transform backstop.

## ════════ (prior) trap purged + exec supported ════════

**Pushed @ `8acce46`.** Two DG dispositions of the "accepted-in-source bash
forms" the coverage grind surfaced:



**On `hand-parser`, pushed @ `a044bb9`.** Coverage grind (DG: "keep going").
Release line coverage **87.69% → 90.80%** across the session (compiler,
native+c). Latest commits (all 0 native/c regressions, verified by full
3-mode sweep):
- `be426a6` removed the dead positional-init cluster
  (`init_numeric_positional`/`init_at_var`, ~90 lines) — top-level positionals
  are validation-rejected and function positionals use the args-array (DD#60),
  so `init_one_positional` only ever hits its DD#53 empty-default. **Verified
  DG's way: remove + full-suite run confirms safety, git checkout reverts.**
- `67affbc` removed the dead `command` builtin codegen (DD#23/SB4001-shadowed).
- `d5b5cf5` `spec/trace.sh` (DD#32 `--trace`/SASH_TRACE tracing).
- `12fb51e`/`3ac9c96`/`a044bb9` cov.59-61 (`$(< file)` slurp, `trap … EXIT`,
  for-in-cmdsubst pipe-while-read).

**Bugs filed this stretch:** `c-pipe-while-read-eof-extra-iter` (P3 — c does an
extra empty EOF iteration in `for x in $(cmd)`; native correct).

**Remaining uncovered = mostly not-spec-reachable or ambiguous:** `ssa::display`
(no CLI trigger — only Rust tests / debug); `extract_from_literal` (compact
`[[a==b]]`, partly covered, attribution unclear); lexer `scan_dq_backtick`
(backtick-in-dquotes literalized — diverges from bash, possibly a gap not a
feature) / `scan_ansi_c_octal`; exec paths (`build_argv_on_stack`,
`emit_exec_redirects_only`). The clear, safe, gate-verifiable targets are
largely exhausted; further progress needs DG input on the ambiguous forms.



**On `hand-parser`, pushed @ `12fb51e`.** Coverage grind continued (DG: "keep
going"). Release line coverage **87.69% → 90.36%** across the session (compiler,
native+c; 610 fewer missed lines). Commits after the ternary work (all 0
native/c regressions):
- `67affbc` removed the dead `command` builtin codegen (`emit_command_builtin`
  + `BuiltinCmd::CommandBuiltin` chain, ~79 lines) — DD#23 rejects `command`
  via SB4001 (`check_dynexec`); the codegen was shadowed vestige. cov.58 pins
  it; shrinks the command-exec attack surface.
- `d5b5cf5` `spec/trace.sh` (NEW, native-only) covers DD#32 tracing
  (`--trace` + `SASH_TRACE=1` → `emit_branch_outcome_trace` /
  `intern_trace_terminator`, ~68 lines). tr.1-3 (if.then/if.else/while).
- `12fb51e` cov.59 covers `$(< file)` slurp parse (parse_slurp_form).

**Candidate-dead (needs careful verification, NOT removed):**
`codegen/setup.rs::init_numeric_positional` + `init_at_var` (+ their only caller
`init_one_positional`). Top-level `$1`/`$@`/`$#` are all validation-rejected
(SB5024/SB5045) and function positionals inline via the args-array (DD#60), so
`collect_positional_refs` only ever surfaces `$?`/`$$` (which
`init_positional_params` skips) — the init cluster (~90 lines) appears dead but
removing positional codegen is delicate; verify `scan_value_for_vars` can never
surface a numeric/`@` ref before deleting.

NEXT (still a path forward): the candidate-dead positional cluster; remaining
reachable-untested from the scan (handle_trap, emit_cmdsubst_pipe_while_read,
lexer scan_dq_backtick/scan_ansi_c_octal). Re-measure: `/tmp/rcov.sh`.

## ════════ STATUS (2026-08-27) — ternary fixes + coverage grind (89.66%) ════════

**On `hand-parser`, pushed @ `3cbcd40`.** Coverage grind continued (DG: "no
rest, continue while a path forward exists"). Release line coverage **87.69%
→ 89.66%** this session (compiler, native+c, no janet; ~447 fewer missed
lines). Commits this run (all 0 native/c regressions):
- `93a85bf` removed dead `$@` transform machinery (post-DD#81, ~82 lines).
- `2b49dbe` arith-operator coverage cov.29-48 (unary/inc-dec/compound-assign/
  comma).
- `d9e68e0` + `03eabb1` **retired `${#…}` entirely** (DG Policy C) — see block
  below.
- `70c34ed` **c-backend ternary was silently empty** (no lower_value_as_str
  arm) → fixed via lower_value_as_int + emit_ternary_as_int_c (if/else temp,
  lazy). cov.49-56.
- `3cbcd40` **native ternary was eager** (evaluated both branches → leaked a
  side-effecting untaken branch's div-by-zero/overflow) → both sites
  (lower_ternary_value + new lower_ternary_int) rewritten to brif+phi merge.
  Now native==c==bash (lazy). cov.57 pins it. Closed native-ternary-eager-eval.

3 bugs found+fixed (${#a[N]} silent-0, c-ternary empty, native-ternary eager),
1 filed (janet-unset-len-nil P3). NEXT (coverage grind, still a path forward):
more reachable-untested fns from the "fully-uncovered function" scan
(emit_cmdsubst_pipe_while_read, parse_slurp_form, extract_from_literal,
init_at_var, ssa::display/emit_branch_outcome_trace behind flags). Re-measure:
`/tmp/rcov.sh`; map: `/tmp/rcov_missing.txt`.



**On `hand-parser` (committing).** Coverage grind surfaced dead
`emit_strlen_elem`; probing it found a bug: `${#a[N]}` (element length)
silently returned 0 (element-length was half-wired, no parser producer). DG
chose **Policy C**: drop the `${#…}` length sugar *entirely* — `$(len x)`
(value) / `(len x)` (arith) is the sole length spelling; keep `len`; converter
rewrites bash `${#x}`→`$(len x)`. Impl: parser `${#…}`→SB4038 everywhere
(`len_retired`), removed arith `ATok::LenOf`, deleted dead `emit_strlen_elem`.
`$(len …)` untouched (converges on the same live `strlen`/`arrlen` operators).
Migrated ~10 spec sites (types/arithmetic/var_modifiers); repurposed 75.4-75.6
to assert the retirement. DD#75 §2/§4/§6 amended; SUBSET.md; journal 191. Native
+ c gate 0 regressions; guarded types.sh 30.11 native,c (pre-existing janet
unset+len nil crash → task `janet-unset-len-nil`). Converter rewrite DONE
(Commit 2: `${#x}`→`$(len x)`, spec convert.sh 57.13c).

## ════════ STATUS (2026-08-27) — spec coverage grind (in progress) ════════

**On `hand-parser`.** DG asked to understand spec-suite coverage, delete dead
code, and add specs toward every spec-reachable line (ignoring janet). Reliable
release baseline (native+c, compiler only): **87.69% lines** (2750 missed).
Release coverage IS reliable (a debug-mode detour + "unreliable" claim were
retracted — every verified "missing" line was genuinely dead-or-untested;
the 91%↔48% debug swing is just line-counting granularity). Harness: needs
`cargo llvm-cov clean --workspace` to force instrumentation; rebuild whole
workspace (convert/expand) before those specs; force-rebuild uninstrumented
after. Batches pushed (0 regressions, native 0): `56024da` 4 dead methods;
`511cddf` parser-shadowed check_export_or_declare + coverage.sh cov.1-3;
`2885993` coverage.sh cov.4-10 (7 reachable-untested diagnostics);
cov.11-28 (more reachable diagnostics + redirect forms); `0c5c4aa` dead
`$@`-exec codegen path; **dead `$@`-expansion transform machinery**
(`expand_dollar_at_*`, ~82 lines — all no-ops post-DD#81 `$@`-rejection).
High-yield pattern: recent feature *removals* ($@, case, var-modifiers)
leave whole dead functions still line-counted — delete + verify
behaviour-preserved. Method: per cluster — shadowed-dead → remove+verify;
reachable → spec. Multi-session grind; journals 188-190.

## ════════ STATUS (2026-08-26, cont.) — taint bypass switches (DD#82) ════════

**On `hand-parser` (committing).** DG: a public fork could rip out taint and
overshadow the original — so provide a *supported* bypass (removes the fork
incentive), and use the two-layer split to test the SSA backstop in isolation.
Added `--no-taint-cst` (CST front-end off, SSA backstop on — native) and
`--no-taint` (all taint off). Taint-only: `--no-taint` + `$@` is still SB5045.
`lib.rs` gates `check_taint`/`verify_ssa_taint`; `SASH_CFLAGS` (common.sh)
injects compile flags for tests. specs taint_switch.sh sw.1–sw.7. 0
regressions native/c/janet; native gate 0; 15 new passing. DD#82, journal 186.
NEXT: SSA hardening (cmd_subst_taint operand propagation + argv/mapfile array
classification + Test/Redirect use-checks) so `--no-taint-cst` catches the full
laundering class (task `ssa-taint-cmdsub-operand-propagation`).

## ════════ STATUS (2026-08-26, cont.) — SSA operand propagation (partial) ════════

**On `hand-parser` (committing).** With `--no-taint-cst` able to exercise the
SSA pass, implemented `cmd_subst_taint` operand propagation (echo/dtoi/itod +
join/keys via `collection_reader_taint`) — matches every other transform value;
catches `x=$(dtoi $tainted)` under `--no-taint-cst` (sw.8). SSA messages
neutralised. **Found the SSA backstop is far from a complete gate**: capture-form
builtins (`$(join @a)`) bypass `cmd_subst_taint` (they lower via
`CaptureEnd{tainted}`, a transform-time bool); array taint isn't classified at
SSA (versioning); inline args/`[[ ]]`/redirect positions aren't in
`check_inst_uses`. Shipped the principled propagation; scoped the rest as
`ssa-taint-full-backstop` (P2, supersedes `-cmdsub-operand-propagation`). CST
stays the authoritative gate. 0 regressions; native gate 0. Journal 187.

## ════════ STATUS (2026-08-26, cont.) — taint laundering bypass CLOSED ════════

**On `hand-parser` (committing).** DG demo (`argv 1 a; echo $(join a)`) surfaced
a **taint-laundering bypass class**: a `$( )` that READS already-tainted data
was only checked in argument position; scalar/subscript-assign RHS, array
literals, `[[ ]]` operands, redirect targets, and nested cmdsubs silently
compiled and leaked at runtime. Also found: bare-name `join a` wasn't a reader
use-site (only `keys` was), and `[[ ]]` operands + redirect targets weren't
taint-checked at all (direct tainted vars leaked too). DG's model: every `$( )`
is internally `_tmp=$(…)` reusing the one read-checking machinery. Fixed CST
(authoritative): extracted `check_cmdsub_inner_reads`, applied it position-
uniformly; added a `Test` arm to `check_expr_taint`, redirect-target checks,
and `join` to `check_collection_reader_taint`. SSA defense-in-depth
(`cmd_subst_taint` operand propagation) filed P2 (`ssa-taint-cmdsub-operand-
propagation`) — can't fire on CST-cleared programs, backstop-only. specs
taint.sh 60.37–60.46. 0 regressions native/c/janet; native gate 0; 29 new
passing. Journal 185. DD#24 amended (reads are use-sites in every position).
Gap-fill (DG "protect against regressions"): taint.sh 60.47–60.55 extend the
matrix (echo-scalar, mapfile source, keys-in-array-literal, subscript-in-test,
bare-name in test/redirect, + 2 clean-value positive controls). All pass 3
modes; 0 regressions; native gate 0.

## ════════ STATUS (2026-08-26) — `$@`/`$*` universally rejected (Option A) ════════

**On `hand-parser` (committing).** DG: "haven't we universally rejected `$*`
and `$@` now?" — no. The `$@` drop left three gaps: in-function `$*` **silently
compiled** (SB5045 matched only `@`); top-level `$@`/`$*` were SB5024 but
mislabeled "function-local" (they're rejected everywhere); braced `${@}`/`${*}`
(and `${@:-x}`) never fired SB5045 at all (`scan_param_name` drops the `@`/`*`
sigil → empty-name ParameterExpansion). **Option A** (DG): `$@`/`$*` are *not in
the language* — one identity (SB5045 "sash has no `$@`/`$*`") fires **everywhere**
with a context-sensitive hint (in-fn → `@args`/`$(join args)`; top-level →
`$(argv 1)`/`$(arg N)`); only `$1`/`$#` stay function-local (SB5024). Parser:
`${@…}`/`${*…}` → `SimpleExpansion` (array `[@]`/`[*]` untouched, still SB4038).
`check_dollar_at` now walks everywhere + arith. specs functions.sh 81.15/81.16,
errors.sh e.dd46.t6/t7 (+t1/t3 assertions SB5024→SB5045). 0 regressions
native/c/janet; native gate 0; 12 new passing. Journal 184. DD#46/#75/#81 amended.

## ════════ STATUS (2026-08-25, cont.) — top-level `$@`/`$*` message → `$(argv 1)` ════════

**On `hand-parser` (uncommitted → committing).** Follow-up to the `$@` drop:
the top-level SB5024 diagnostic for `$@`/`$*` directed to `$(argv)`, but that
includes the program name at index 0 (spec/argv.sh av.s1). The function-local
`$@`/`$*` are the **user args** = `$(argv 1)` (av.s2), matching DD#60.
`flag_positional` (validate.rs) now says "use $(argv 1) for the user-arg slice
(or $(arg N) for one)". Rejection itself unchanged (still SB5024); only the
suggested replacement corrected. `$1`→`$(arg N)`, `$#`→`$(argc)` untouched.
DD46-positional-local.md updated; specs errors.sh e.dd46.t1/t3. 0 regressions
native/c/janet; native gate 0. Journal 183.

## ════════ STATUS (2026-08-25, cont.) — drop `$@` (SB5045) ════════

**Pushed on `hand-parser`.** DG caught that `$@` breaks DD#75 §4's "`@` is the
sole splat" (a `$`-sigil that splats). `validate::check_dollar_at` rejects
in-function `$@`/`${@}` → SB5045 (use `@args`, or `$(join args)` joined);
top-level stays SB5024; `$1`/`$#` untouched (scalar). DD#75 §5/line 172 + DD#81
note corrected ($@ removed, not sugar). `"@foo"` stays a literal (no
false-positives on `user@host`); the converter warns — filed
`converter-dollar-at-migration`. specs functions.sh 81.14, control_flow.sh
cf.f11 (→`$(join args)`, native,c)/cf.f12 (→`@args`). 0 regressions; native 0.
Journal 182.

## ════════ STATUS (2026-08-25, cont.) — DD#81 COMPLETE (Parts A + B) ════════

**Pushed on `hand-parser`.** DD#81 "function-call discipline" fully implemented.
Part B (exact arity) landed: `transform/functions.rs::analyze_arity` infers a
per-function `Arity` (Fixed/Variadic) from `args` usage — `$N`/`${args[k]}`
literal → fixed index; `@args`/`$(len args)`/`${#args}`/`$#`/`$@`/dynamic-index →
variadic; **arith scanned** (`$#` inside `$(( ))`). Checked exactly on all three
call paths (plain/pipeline/`$( )` capture): SB5042 too-few, SB5043 too-many,
SB5044 hole (rule (a)). DG's no-op idiom (`: $1` to acknowledge an unused param,
`@args` for variadic/high-index) resolved the rule-(a) tension; specs 67.2/75.20/
75.22 updated. Old min-arity scanner (~130 lines) removed. specs functions.sh
81.9–81.13; 0 regressions native/c/janet; native gate 0. Journal 181.

## ════════ STATUS (2026-08-25, cont.) — DD#81 structural stages COMPLETE; Part B (arity) filed ════════

**Pushed on `hand-parser` (d73c69e, 56e4c17, 50474e9) + DD 3081205.** DD#81
("function-call discipline") structural work done — three reachable ICEs fixed
and the "one name, one meaning" principle implemented:

- **Part A** (d73c69e) — reject command/function shadowing + forward refs
  (SB5041); unified the `$(f)` gate onto the plain-call `function_bodies` gate.
- **func_constants removal** (56e4c17) — the forward-`$(f)` machinery, now
  provably dead, deleted (−216 lines, 8 files).
- **`$(f arg)`** (50474e9) — the capture path now carries + sets up the callee's
  `args` (was an `undefined array` ICE); unified onto the args-array model.
- **message cleanup** — MOOT: Part A + SB5011 catch every unresolved/forward
  bareword at compile, so the sash-rt `bug: bare command` backstop is now an
  unreachable internal invariant (correctly signals a compiler bug if ever hit).

**Remaining: DD#81 Part B — uniform exact arity from `args` usage**
(`notes/tasks/dd81-part-b-arity.md`, ready, P2). A feature-sized new
signature-inference analysis (fixed vs variadic from `@args`/`${args[i]}`/`$(len
args)`/dynamic-index; holes SB5044; exact checking SB5042/43 on all call paths).
The current `$N`-only min-arity check (plain+pipeline) is the starting point.

0 regressions native/c/janet throughout; native gate 0. Journals 178–180.

## ════════ STATUS (2026-08-25, cont.) — DD#81: `$(f arg)` passes args (was ICE) ════════

**Committed on `hand-parser`.** Third DD#81 stage, a reachable ICE fix. `s=$(f
hello)` / `echo $(f x)` on a defined function reading its args crashed
(`undefined array`): the capture path (`capture_func_into`) used the old
positional model (`setup_func_args`, `$1..$9`) and was passed `&[]`, so the
body's `$1`≡`${args[0]}` (DD#75) found no `args` array. Fix: `SsaValue::FuncCall`
now carries `args`, and `capture_func_into` uses the same args-array model as
`inline_function_call` (`save_caller_args`/`setup_callee_args`). Dead positional
helpers removed. specs functions.sh 81.7/81.8 (native,c); 0 regressions, native
gate 0. Journal 180.

**DD#81 remaining:** Part B uniform arity (SB5042/43/44 — capture path now
carries the arg count); `bug: bare command` → `command not found`.

## ════════ STATUS (2026-08-25, cont.) — DD#81: remove dead func_constants machinery ════════

**Committed on `hand-parser`.** Second DD#81 stage. With forward `$(f)` rejected
(Part A), a bare `SsaValue::FuncCall` can never reach codegen (defined `$(f)`
always lifts to a captured temp; undefined resolves external). Confirmed dead
(instrumented panic → 0 native regressions), then removed `fold_function_outputs`
+ `try_fold_block`, `lower_func_call_value`, the setup intern loop, and the
`func_constants` parameter threaded through the whole value-lowering tree (139
lines / 8 files + the InstCtx field). The `FuncCall` arm now panics
"should-have-been-lifted" (joins IntFuncCall/CmdSubstBody). Net −216 lines; 0
regressions native/c/janet, 0 new (behavior-preserving). Journal 179.

**DD#81 remaining:** `$(f arg)` arg passing; Part B uniform arity
(SB5042/43/44); `bug: bare command` → `command not found`.

## ════════ STATUS (2026-08-25) — DD#81 Part A: reject shadowing (name resolution) ════════

**Committed on `hand-parser`.** First DD#81 stage. Unified the `$(f)` capture
gate onto the plain-call `function_bodies` gate (defined-so-far / execution
order): a not-yet-defined `$(f)` is now an external command substitution, not the
broken `known_functions`→`func_constants` path. Added **SB5041**: a bareword used
as an external command, then defined as `fn NAME`, is a shadowing collision
(`externally_used` set, gated off during dead `fn_` template lowering via
`lowering_fn_template`). Rejects dynamic shadowing + forward refs (both `$(f)`
and plain), keeps helper-below-caller / static-override / normal calls. specs
functions.sh 81.1–81.6; 0 regressions native/c/janet; native gate 0. Journal 178.

**DD#81 remaining (next stages):** remove now-dead `func_constants` /
`lower_func_call_value` / `fold_function_outputs`; fix `$(f arg)` arg passing;
Part B uniform arity (SB5042/43/44 from `args` usage); `bug: bare command` →
`command not found`. Tracked in the session todos.

## ════════ STATUS (2026-08-24, cont.) — bug fix: `[[ -o ]]`/`[[ -R ]]` codegen ICE ════════

**Committed on `hand-parser`.** Reachability audit for the ICE-arms keep+cite
pass surfaced a real bug: the parser's `is_unary_test_op` accepts `-v`/`-o`/`-R`,
but codegen (tests.rs) implements only `-n`/`-z`/`-i`/`-t` + file tests, and
validate rejected only `-v` (SB5028). So `[[ -o errexit ]]` and `[[ -R x ]]`
crashed the compiler (`ICE: unimplemented unary test operator`). Fix follows the
`-v` precedent: `unsupported_unary_test_hint(op)` rejects all three at validate
(SB5028); the now-unreachable tests.rs panic gets a keep+cite `// INVARIANT:`
comment. specs wordsplit.sh 62.20/62.21 (avoided a pre-existing `chk 62.11`
collision). 0 regressions native/c/janet; 6 new passing; native gate 0.
Journal 177. Filed `unary-test-ops-k-g-u` (reverse gap: parser under-accepts
-k/-g/-u that codegen already implements).

## ════════ STATUS (2026-08-24, cont.) — eliminate-ice-arms P1 Pattern-A (VarSlot match) ════════

**Committed on `hand-parser`.** P1 Pattern-A: the five codegen sites that
dispatched on `VarSlot::is_int()` then reached through a panicking accessor
(`as_int`/`str_ptr`/`str_len`) now match the enum directly
(`match *sv { VarSlot::Int(v) => .., VarSlot::Str { ptr, len } => .. }`) — total
by construction. Sites: lower_var_value, index_var_to_int, lower_var_as_int
(values.rs), handle_regular_assign (inst_handlers.rs), emit_capture_end
(dispatch.rs). Consequence: `as_int`/`is_int`/`as_str` fell to zero callers and
were **deleted** — removing 2 more ICE panic arms (`as_int`/`as_str`) outright.
`str_ptr`/`str_len` kept for the ~40 Pattern-B callers (known-Str slots) with an
`// INVARIANT:` keep+cite comment. Behavior-preserving (`match` ≡ the 2-arm
if/else) → no new specs. 0 regressions native/c/janet; native gate 0. Journal 176.

Review remaining: keep+cite for P3 (undefined var/array/index → validate SB5031)
and P7 (should-have-been-lifted); reachability check on the
"unimplemented/unsupported operator" panics (tests.rs test ops,
value_handlers.rs param-expansion op) before cite-vs-fix.

## ════════ STATUS (2026-08-24, cont.) — eliminate-ice-arms P2 (lazy literal interner) ════════

**Committed on `hand-parser`.** P2 of the per-pattern ICE-arms review: the five
codegen sites that `panic!`'d on a `literals`-map miss (`"ICE: missing literal"`
/ `"ICE: assoc array key not interned"`) now go through
`values::intern_or_declare` — read the map, or declare+define fresh anonymous
data on a miss. Total by construction (no ICE). No signature churn: `module` is
already `&mut` everywhere, and `declare_anonymous_data` needs no unique name.
Sites: glob dir/operand (2), join separator, const-folded string, assoc/indexed
key. Behavior-preserving (the panics were unreachable drift backstops) → no new
specs. 0 regressions native/c/janet; native gate 0. Journal 175.

Review remaining: P1 Pattern-A `match` conversions (types.rs flag-checked
`is_int` sites); keep+cite for the table / P3 / P5 / P7 sites.

## ════════ STATUS (2026-08-24, cont.) — eliminate-ice-arms cohort A (join/CheckedArithOp/case) ════════

**Committed on `hand-parser`.** Three ICE arms from the per-pattern review
(reviewed interactively with DG) retired:
- **P8** (`3cf1519`) — `$(join)` 0/>2-arg was a **reachable compiler panic**; now
  `validate::check_join_arity` → SB5040. spec types.sh 33.23-24.
- **P4** — `ssa::CheckedArithOp{Add,Sub,Mul}` makes `emit_checked_arith(_c)` total
  (2 arms gone). Dispatch maps each ArithOp variant directly.
- **P6** — case conversion is now a `bool` (whole-string toupper/tolower), not a
  stringly-typed `"^^"/",,"` match (2 arms gone; the first-char `^`/`,` family
  had no producer since DD#65). Removed dead `toupper_first`/`tolower_first`
  front-end refs; filed `remove-dead-case-first-runtime` (sash-rt twin).

0 regressions native/c/janet; native gate 0. Journal 174. Review remaining:
P2 (lazy interner ~5 arms), P1 Pattern-A `match` conversions, keep+cite for the
table/P3/P7 sites.

## ════════ STATUS (2026-08-24) — index-var is_int-total + $@[i] ICE + atoi audit ════════

**Committed on `hand-parser`.** Follow-through on the P1/ICE-arms discussion (DG:
"arrays/index-vars are always Str-slots" is a convention tests can't prove).
- `codegen/values.rs`: merged the argv/table index-var reads into
  `index_var_to_int`, `is_int`-total (int slot read directly, string slot
  `atoi`'d) — removes the `str_ptr`-on-int panic risk, mirrors `lower_var_as_int`.
- `validate.rs`: `$@[i]` / `$*[i]` (subscripting the positional splat) was a
  codegen ICE (`undefined array: @`); now SB5031 via `check_subscript_index`.
  spec subscript.sh 80.20-21.
- **atoi audit** (DG follow-up): every atoi is a string→int coercion; no atoi on
  an already-int value. Native clean; filed `c-argv-index-isint-parity` (the C
  argv-index atoi's the string `__fi` unguarded — parity gap, not a live bug).

0 regressions native/c/janet; native gate 0. Journal 173. The eliminate-ice-arms
review continues — P1 index-var/Pattern-B done; table sites keep+cite; P2 (lazy
interner), P4 (CheckedArithOp), P6/P8 still open per the per-pattern review.

## ════════ STATUS (2026-08-23) — undeclared collection read ICE → SB5031 ════════

**Committed on `hand-parser`.** `echo ${z[0]}` (undeclared array/map read)
panicked at codegen (`ICE: undefined array`) instead of a clean error — found
during DD#80. `validate::check_expr_reads`'s `Subscript` arm now flags an
undeclared collection *name* (SB5031), mirroring the arith path (which already
did); `args` skipped (top-level → SB5024, in-function → in scope). spec
undeclared.sh ud.19-21 (×3). Filed `arith-args-subscript-codegen` (a masked
codegen gap: bare `$(( args[0] ))` never resolves `args`→`__argsN`; the SB5031
false-positive was protective). 0 regressions; native gate 0. Journal 172.

## ════════ STATUS (2026-08-22) — DD#80 braced subscript key/index discipline ════════

**Committed on `hand-parser`.** DD#80 (companion to DD#79): `${a[K]}` / `${m[K]}`
take a **direct, unambiguous** key — no *implicit* arithmetic; computation uses
an explicit nested context. `bare in (( )) = arithmetic; ${ } = direct key.`

- Parse: `try_subscript` now captures the index balanced/quote-aware
  (`capture_balanced_subscript`) and re-parses it via `parse_expr`, so nested
  `${…}`/`$(( … ))`/quoted keys work and lower to SSA temporaries for free
  (`${a[${a[0]}]}` was SB4035 → now works). A bare dollar-subscript `$b[0]` is
  kept ambiguous (use `${b[0]}`).
- Reject: **SB4041** (new, `validate::check_subscript_index`) for an
  ambiguous/implicit bare index (`${a[a[0]]}`, `${a[i+1]}`, `${a[$b[0]]}`);
  **SB4030** (reused type-check string→int) for a string key on an int array
  (`${a[foo]}`, bare `${a[i]}`, `${a["5"]}`). Catches word AND arith contexts.
- Fixes `bug-braced-nested-subscript-index` (the silent inner-cell read) +
  the wider silent-`0` footgun. Aligned with DD#33 (bare integer on a map is
  quoted). Filed `converter-braced-subscript-index` (follow-up).

spec/subscript.sh 80.1–80.19 (57 = 19×3). 0 regressions native/c/janet; 51 new
passing; native gate 0. DD#80, journal 171. This resolved the design thread that
began with the `arith-bare-subscript-followups` edge-2 finding.

## ════════ STATUS (2026-08-21, cont.) — arith-bare-subscript-followups DONE (all 4 edges) ════════

**Pushed on `hand-parser`.** Task `arith-bare-subscript-followups` complete.

- **Edge 1** (`$(( $a[i] ))`): DG chose the diagnostic route. A read-the-cell fix
  silently mis-read variable indices (`$a[$i]` rebuilt as `"$a[ $i ]"` via the
  arith token→string→re-lex path → read a[0]); reverted. `arith_bad_literal_error`
  now special-cases a stray `[`/`]` → "use `a[i]` or `${a[i]}`". Specs dsx.1-2.
- **Edge 2** (`$(( a[a[0]] ))`): the core word lexer broke words on doubled `]]`
  (for `[[ ]]`), corrupting a bare nested cell read (ends in `]]`).
  `scan_word_chars` now tracks subscript depth: `]]` closes two subscripts at
  depth>0, delimits `[[ ]]` only at depth 0. Specs nsx.1-3.
- **Edge 4** (substring/sed on a cell): parity with scalars. Sed desugars to
  `$(sub|gsub "${a[i]}" …)` (3 backends free); substring lowers via a new
  source-carrying `SsaValue::Substring` (native/c/janet). Specs scx.16-22.
- Filed `bug-braced-nested-subscript-index` (pre-existing, separate path:
  `${a[a[0]]}` returns the inner cell).

Commits `4b4935c` (edges 1+2), edge 4 (this). 0 regressions native/c/janet;
journals 169, 170. Remaining native backlog: `eliminate-ice-arms-via-types`,
`audit-string-keyed-builtin-dispatch`, doc task.

## ════════ STATUS (2026-08-21, cleanup) — retire legacy sed/regex_replace machinery ════════

**Committed on `hand-parser`.** Task `retire-legacy-sed-regex-replace-machinery`
done. Removed the unreachable legacy `:s`/`=~` `regex_replace` path
(`try_regex_substitution` in transform; the `regex_replace` + `/`|`//` codegen
arms and `emit_pattern_replace`/`emit_regex_replace_op`/`intern_literal_value`;
the dead `intern_param_expansion` guards). The LIVE `$(sub)`/`$(gsub)`
(`SsaValue::RegexReplace` → `lower_regex_replace_value` → `sash_regex_replace`)
path is untouched.

Two bugs fixed in passing (both were masked by the machinery):
- `try_regex_substitution` hijacked any command whose 1st arg looked like
  `s/…/…/` → `/bin/echo s/a/b/` printed nothing. Now runs literally.
- **`is_regex_mutation` SB5011 bypass** (`resolve.rs` + `validate.rs`): since
  `=~` desugars at parse to an Assignment (not a Command), those guards only
  ever matched *real* commands, exempting them from DD#23 resolution → an
  unresolvable single-`s/`-arg command (`nosuchcmd s/a/b/`) compiled then hit a
  runtime `bug: bare command` abort. Removed both guards+fns → proper SB5011.

Specs regex.sh §H (63.h1–h3, ×3 modes = 9 new passing); 0 regressions native/c/
janet. Journal 168. Remaining native backlog: `arith-bare-subscript-followups`,
`eliminate-ice-arms-via-types`, `audit-string-keyed-builtin-dispatch`, doc task.

## ════════ STATUS (2026-08-21, backlog sweep cont².) — args-in-len + len-undefined ════════

Combined effort from the `len-undefined` discussion (DG approved). Three parts,
all 0-regression across native/c/janet:

- **`2834536` args in len/join/keys + top-level reject** — `$(len args)`/
  `$(join args)`/`$(keys args)` now resolve the implicit array (was a silent 0);
  `resolve_args_name` at the intrinsic sites + type_check `check_function`
  registers `args` as a function-scope collection. Top-level `@args`/`${args[i]}`
  → SB5024 (function-local). Specs 75.23-75.26.
- **`27201d2` `len <undefined>` → SB5039** — a dedicated `validate.rs`
  `check_len_operands` pass, NOT the type_check route the task sketched. Key
  insight (DG): the loop-var gap is only in *type_check*'s env; validate's SB5031
  `collect_scope_names` already tracks `for` vars / decls / read targets, so
  `for e in @a; do $(len e); done` is accepted for free. Also taught
  collect_scope_names about `argv [N] ARR` targets. Specs 75.27-75.30.

Backlog sweep total this session: arg-negative-index (SB5038); bare-nested-cmdsub
(lexer `))` split); arith-disposition 0x/0o/0b; args-in-len + len-undefined.
`const-fold-command-name` discarded (DG: "gpp territory"). Remaining native:
`arith-bare-subscript-followups`, `eliminate-ice-arms-via-types`,
`audit-string-keyed-builtin-dispatch`, cleanups (retire regex_replace, strip
runtime), + the doc task.

## ════════ STATUS (2026-08-21, backlog sweep cont.) — 3rd fix: arith 0x/0o/0b disposition defaults ════════

**Pushed `2f435a6`.** `arith-disposition-hex-octal-default`: the arith `:-`
default now accepts DD#72 `0x`/`0o`/`0b` literals (was decimal-only, SB4030).
New shared `parse::parse_dd72_literal`; type_check D4 widened; native/c convert
to decimal at SSA level (`canonicalize_arith_default`, arith-scoped so string
`${s:-0xff}` keeps "0xff"); janet got its missing `ArithExpr::Expansion` arm
(emits `(_num …)`, was an empty operand → advances `janet-arith-subscript-read`).
Specs d4.2/d4.4-d4.7b. 0 regressions all 3 modes.

**Backlog sweep so far (all 0-regression, pushed):** arg-negative-index (SB5038)
`fc7262e`; bare-nested-cmdsub (lexer `))` split) `07ee969`; arith-disposition
`2f435a6`. Remaining native: `len-undefined-reject` (needs loop-var/param
tracking), `const-fold-command-name` (feature), `arith-bare-subscript-followups`,
+ args-model polish (`$(len/join/keys args)`, `$*`→`$(join args)`) + the doc task
(`docs-tail-recursion-args-model`). janet residuals tracked in
`janet-arith-subscript-read`.

## ════════ STATUS (2026-08-21, backlog sweep) — native backlog: 2 parser/validator bugs fixed ════════

Working the native backlog (DG: "keep after the backlog"). Two bugs fixed, each
0-regression across native/c/janet:

- **`arg-negative-index-compile-reject`** (`fc7262e`): `$(arg -1)` compiled
  (only aborting at runtime) despite DD#60's compile-time guarantee. Now
  `arg_index_diag` rejects a negative integer literal index with **SB5038**
  (both `$(arg -N)` and `$(arg -N DEFAULT)`). Specs argv.sh av.t7 (strengthened
  to assert SB5038) + av.t7b.
- **`bare-nested-cmdsub-arg`** (`07ee969`): a bare nested `$(f $(g))` was a parse
  error (SB4014). Root cause: logos lexes `))` as one `DblParenClose`, but that
  token is arith-only; in `$(f $(g))` the `))` are two cmdsub closes.
  `split_dbl_paren_close` splits `DblParenClose`→`RParen` when `arith_depth==0`.
  Specs commands.sh 14.7/14.8.

Process note: a complexity-probe `git commit -F /tmp/j.txt` accidentally landed a
junk `x` commit when the gate passed; squashed it out (soft-reset, not pushed)
and restored `notes/misses.md` (never stage it). Don't probe the gate with a real
`git commit` — the FAIL output already names the function on a real attempt.

Remaining native backlog: `len-undefined-reject`, `const-fold-command-name`,
`arith-bare-subscript-followups`, `arith-disposition-hex-octal-default`, + the
args-model polish (`$(len/join/keys args)`, `$*`→`$(join args)`) and the doc task.

## ════════ STATUS (2026-08-21, later²) — FIX: nested-paren sexpr arg (arith collector) ════════

**Pushed `665151a`.** Fixed the P2 bug found during tail-recursion work: a DD#58
arith sexpr call with a **parenthesised arg** — `(dbl (2 + 1))` — silently gave
0 (and `(dbl (2))` was an SB4020 parse error).

Root cause: the `$(( ))`/`(( ))` content collector tracked paren depth
asymmetrically — `((`/`$((`/`))` counted as ±1 units, but a single `(` pushed
text WITHOUT incrementing depth. So `(dbl (2))`'s two single `(` never raised
depth, and the trailing `))` (one `DblParenClose` token) closed the content
early, truncating `(dbl (2))` → `(dbl (2`.

Fix: count **individual** parens — depth starts at 2 (the consumed `((`/`$((`),
each `(` is +1 / `)` is −1, `((`/`))` are +2/−2 shortcuts. DG's steer: inside
`(( ))` you're already in arithmetic, so the natural form is a plain
parenthetical `(dbl (2 + 1))`, not a redundant nested `$(( ))` — and that plain
form was exactly what didn't parse.

Verified 6 = dbl(3) on native+c+janet; 0 spec regressions all 3 modes. Specs
types.sh 58.14/58.15. (This also clears the accumulator-ergonomics caveat noted
for tail recursion — a nested arith arg now works directly.)

## ════════ STATUS (2026-08-21, later) — RECURSION: reject non-tail, tail→loop ════════

**Pushed `46ff63a`.** Recursion crashed the compiler (unbounded inlining → stack
overflow). Now, per DD#27 (amended):

- **Non-tail + all mutual recursion → SB5037** (clean reject, no crash). An
  inline-stack (`VersionState.inlining`) at all three inline sites catches a
  call to a function already being inlined.
- **Direct self tail-recursion → a LOOP.** DG's cut: "support tail recursion,
  reject the rest." A self-call that is a `return` producer is tail by
  definition (nothing runs after a return) — so no tail-position analysis. The
  first inline makes the body entry the loop header; `return F newargs` rebinds
  F's implicit `args` array (DD#75 §5) and branches back (`emit_tail_call`).
  Loop-carried args needs **no manual phi** — all SSA versions of a name share
  one Cranelift Variable (auto-merge at the join, `codegen/setup.rs`).
  - Mechanism (not a max-depth counter — DG's steer): an inline stack is
    precise (catches direct + mutual, no arbitrary limit, identifies the edge).
  - `return F …` is the only tail form (required for `-> int`/predicate per
    DD#74, uniform for void too). Accumulator style: bind locals then
    `return $(( (go n1 a1) ))`.
  - native+c via the SSA loop; janet keeps its own recursive emit (same
    results). All 3 modes reject non-tail/mutual. Specs `functions.sh` 27.1–27.5.

Also: transform diagnostics are now exact-deduped (a body is lowered as a
template AND at each inline site, so a body-level diag repeated).

**Found + filed a pre-existing bug** (`bug-arith-sexpr-nested-cmdsub-arg`, P2): a
nested `$(( ))` arg *directly* to an arith-sexpr call (`(dbl $(( 2+1 )))`)
silently lowers to 0. Independent of the args model / tail recursion (a plain
`(dbl 3)` and a bound-local arg both work). Accumulator style sidesteps it.

0 spec regressions across native/c/janet.

## ════════ STATUS (2026-08-21) — DD#75 §5 args model SHIPPED (0-indexed); `shift` dropped ════════

**Pushed `03e37e8`.** Function positionals (`$1..$9`/`$@`/`$#`) are now backed
by a real **0-indexed** `args` array, not scalar `"1".."9"/"#"` vars. DG's key
insight: `args` is a compiler-managed abstraction, so each inline mints a
**fresh unique array name** (`__args0`, `__args1`, …) materialized once from the
call args — nested/recursive calls never collide, so **no SSA save/restore**
(only the current name, a cheap string).

**Fixed a pre-existing bug:** a function reading a positional *after calling
another function* got the callee's value (`fn f { g Z; echo $1; }` → `Z`).
Reproduced identically at HEAD (arrays are name-slotted in codegen, not
SSA-versioned). Now correct in native+c+janet.

- `$1..$9` → `${args[0]}..${args[8]}` (0-indexed — DG, amended DD#75 §5 from
  1-indexed after checking prior art: matches sash's own arrays + Elvish/Oil/
  Python/Ruby/…; the 1-indexed-args langs, fish/zsh/Lua, are 1-indexed
  *everywhere*).
- `$#` → `${#args}`; expression + arithmetic share `positional_to_value`.
- user `${args[i]}`, `@args`, `${#args}`, `for x in @args` work (native+c;
  janet keeps its `_args` model). `resolve_args_name` maps `args`→`__argsN`.
- `>9` args reachable via `${args[N]}` (old scalar model capped at `$9`).
- **`shift` dropped** — a degenerate slice (DG). A general array **slice** is a
  post-beta research topic (`research-array-slice`, incl. value-vs-view +
  **copy-on-write**).

0 spec regressions native/c/janet (1037 native). Specs `types.sh` 75.17–75.22.
**Deferred polish** (`dd75-args-model-polish`): `$(len/join/keys args)` (intrinsic
recognition still gates on `collections.contains("args")`); `$*` removal;
`$@`→`@args` splat for >9; recursion guard (recursive inlining overflows the
compiler — pre-existing); friendlier `shift`-removed diagnostic.

Native backlog remaining (DG: "any/all remaining native implementation work"):
`bare-nested-cmdsub-arg`, `arg-negative-index-compile-reject`,
`len-undefined-reject`, `const-fold-command-name`, `arith-bare-subscript-
followups`, `arith-disposition-hex-octal-default`, + the args-model polish.

## ════════ STATUS (2026-08-20, later still) — janet STRUCTURAL MAP: Stage 2 SHIPPED ════════

**Stage 2 shipped `aac76b6` (pushed)** — the function surface + typed bindings.
Verified byte-identical to native on 7 programs (functions, positionals, `$@`
splice, return, function-in-condition, elif-in-function, negative int).

- `int n=5` → `(set n 5)` (janet **number**); untyped `x=5` → `(set x "5")`
  (**string**). Type shows through the *value representation*, not an
  annotation (DG). `int h=0xff` is front-end-rejected (SB4030) — prefixed ints
  arrive via `$((…))` (S4).
- `f(){…}` → `(sash/defn …)`; a user-function CALL renders distinct from an
  external: `(f args…)` / `(sash/succeeded (f …))` in a condition, vs
  `sash/run`/`sash/ok?`. `Env.fns` collected up front. Bare nonzero-return does
  NOT abort (matches native).
- positionals `$N`/`$#`/`$@` → `sash/arg`/`argc`/`args` (function-local; SB5024
  bars them at top level). `return N` → `(sash/return N)`.
- `(sash/TODO …)` is now a preamble macro: a partial program compiles and runs
  up to the first unmapped construct, then aborts naming it (`expr:arithmetic`
  etc.) — graceful degradation for DG's experimentation.

**Fix folded in first (`f6db842`):** removed the numeric `[[ ]]` forms
(`sash/-eq…-ge`) — a bashism; sash `[[ ]]` is string/file only, numeric compare
is `(( ))` (S4). Readable TODO labels (`stmt:*`/`expr:*`).

Known S2 gap: flat scope (local-shadowing unmodelled, same as existing
emit_janet) — proper per-scope var/set deferred to S3/S5.

Next: **S3 pipelines / `&` / `$!` / redirects** (+ consider real local scope).

## ════════ STATUS (2026-08-20, later) — janet STRUCTURAL MAP: Stage 1 SHIPPED ════════

DG re-aimed janet: "as near as possible to a 1-to-1 map of how the compiler
interprets the sash source … side-by-side … 'oh, I see that didn't parse the
way I expected'." Structural legibility (the parse shape) over semantic/
idiomatic legibility. Build in **stages**; get an early read after stage 1.

**Stage 1 shipped `6bc0130` (pushed).** New `--janet-map` backend, additive
(old `--janet` untouched):
- `sash/src/emit_janet_map.rs` — structural emitter: Program → `sash/*` map.
- `sash/janet/sash-rt.janet` — primitive library, prepended as preamble.
- `--janet-map` flag in lib.rs; runs the real front-end (validation-gated).
- DD#15 amended; task `janet-structural-map` (S1 done, S2–S6 outlined); j.167.

Design (DD#15 amendment, DG-ratified): sash-hugging vocabulary; semantics in
sash-rt.janet; control flow via THIN macros (`sash/if` ≈ janet `if`) so emit
keeps supplying structure (IR-stage oracle independence preserved); `bless`
present+functional (`sash/bless-checked` aborts / `sash/bless` branches) but
NO taint flag; purely structural (no taint/type annotations); position-based
reads VISIBLE (`sash/run` bare-checked vs `sash/ok?` condition vs `sash/dash`
for `- cmd`); `($ x)` reads nil-coalesce to "".

Stage 1 surface: assign, echo/`:`, external (run vs ok?), if/else/elif,
while/until, `[[ ]]` string+numeric tests, `$()` single-external capture,
bless, `- cmd`, `$?`. **Awaiting DG's early read + experimentation before S2.**

Next stages (staged, per DG's read): S2 functions/return/positionals; S3
pipelines/`&`/`$!`/redirects; S4 `${…}` modifiers/arith/arrays/splats; S5
abort-channel/signals/case/multi-stmt-capture/regex; S6 oracle re-green +
retire old emit_janet, point `--janet` at the new one.

## ════════ STATUS (2026-08-20, cont.) — c PARITY SWEEP COMPLETE (janet deferred) ════════

DG: "tackle all the c parity work, and wait on janet" → then approved all
remaining (descriptive-abort incl. harness change, bgpid, ivc.8).

**RESULT: the c backend has ZERO failing tests and ZERO actionable skips.**
Started at 13 failing + 63 native-guarded; now 0 failing, 7 skipped — and all
7 are genuinely NOT c gaps: janet tests (15.1/15.2, `run_sash_janet`),
native-ELF-only (32.1/32.2, `.sash_source`/`--extract`), and mode-independent
CLI flags (57.46a-c). ~13 commits `cde633e`..`01884df`, all 0 regressions.

Batches (journal 158-166):
- bless-fail abort, signum, DD#56 signals/END (+9)
- external `$(cmd)` capture + DD#55 abort (+3); `sash_capture_status` runtime
- bless-all + mapfile-from-file + RedirectPush/Pop + array append (+19)
- function/multi-statement `$()` capture (CaptureStart/End) (+4)
- un-guarded ~28 already-working / newly-working tests
- **cd** (was a no-op — real bug) + fixed `bug-emit-c-splat-vla-jump` (P2)
- **ivc.8** `local int array` (declare_vars "String wins" + table-slot override)
- **descriptive-abort** FULLY: cd + echo-write(2) + redirect-open(2) aborts +
  a harness change (`_run_sash_c` captures `$rerr`) → un-guarded 55.20-30 +
  24.20/25.8/gs.6/e.a70.regcomp
- **bgpid** `&`/`$!`/`wait` (SubshellStart/StoreBgPid/Wait, incl. no-arg wait)

New emit_c machinery: sash_capture_status/sash_chdir/sash_emsg/sash_alloc/
sash_trap/sash_signal_flag/sash_capture_start-end externs; `Emit.{
has_exiting_signals, cur_block, redirect_stack, in_condition}`; sub-dispatchers
`emit_read_inst/emit_bless_inst/emit_capture_inst/emit_builtin_extra/
emit_bg_builtin_c` (complexity gate).

**Remaining is janet parity (deferred by DG) + the c/janet parity tier tasks
that are janet-only.** The c half of every c-janet-* task is done.

## ════════ STATUS (2026-08-20, cont.) — c PARITY SWEEP (janet deferred) ════════

DG: "tackle all the c parity work, and wait on janet." Started at **13 failing
+ 63 native-guarded** c tests; now **2 failing + ~23 skipped**, most of the
skipped being not-c-gaps. 7 batches, all 0 regressions, all pushed on
`hand-parser`:

- **158** `cde633e` bless-fail abort, signum, DD#56 signals/END (+9:
  24.12, sx.7, 56.4-56.11). Signal runtime prims already in sash-rt.
- **159** `4489c63` external `$(cmd)` capture + DD#55 abort (+3: 55.7/8/9).
  New `sash_capture_status` runtime helper.
- **160** `68d18f2` bless-all + mapfile-from-file + array append (+19:
  60.9-60.36, ad.1-3). RedirectPush/Pop stack, BlessAll loop (shadowing,
  if-form, map), BlessRetag, ArrayAppend.
- **161** `6e83b5f` un-guard 12 already-working (e.b70.tbare/tdash, 63.2e/f,
  63.cap6/7, bng.1-6, ud.16, e.a70.silent).
- **162** `92a8aa7` function/multi-statement stdout capture (+4: 17.3/4,
  mcs.3/4). CaptureStart/End via sash_capture_start/end.
- **163(6)** `e3e867d` un-guard 14.2c/14.3a/b/d (cmdsub, work via 159/162).
- **163(7)** `08b5253` implement `cd` (was a no-op — real bug); fix
  `bug-emit-c-splat-vla-jump` P2 (heap-alloc splat argv, not a stack VLA, so
  cd's abort goto can cross it). 0 un-guarded but 2 real fixes.

**Still failing (2):** 38.5, 55.19 — `$!` background pid (`&` →
`__subshell`/`__store_bg_pid`, TODO in c). → bgpid task.

**Remaining skipped — NOT c gaps (leave native-only):** 15.1/15.2 (janet
tests, `run_sash_janet`), 32.1/32.2 (native-ELF `.sash_source`/`--extract`),
57.46a-c (mode-independent CLI flags), 24.20/25.8/gs.6/e.a70.regcomp (assert
`$rerr`, which the c harness discards).

**Remaining c gaps needing work (3 areas):**
1. **descriptive-abort** (55.20-30, ~11 tests) — cd done; still need echo
   write(2)-error abort (emit_echo ignores sash_write results, 2 call sites) +
   redirect open(2)-error abort + a harness change so `_run_sash_c` captures
   runtime stderr into `$rerr` (currently `2>/dev/null`). `test_bin:` is a
   substring of c's `./test_bin:` so messages would match once captured. The
   harness change touches ALL c tests → needs a full-sweep re-verify.
2. **bgpid** (38.5, 55.19) — `&` background / subshell exec unimplemented in c
   (`__subshell`/`__store_bg_pid` TODO). A new exec model.
3. **ivc.8** — `local int array` function-local: the SSA restore mistypes the
   int-collection as an int scalar (`sash_atoi`) instead of the table slot;
   declare_vars "Int wins" picks the wrong slot. A transform/type fix.

## ════════ STATUS (2026-08-20, cont.) — P3 partials finished (tilde, dd67) ════════

DG: "finish the p3 partials." Both closed.

- **tilde-vs-dd53 → DONE**: the converter half is now implemented. `sash-convert`
  rewrites bash `~`/`~/path` → hoisted `home=$(getenv HOME); bless home [[ … ]]`
  + `$home`/`$home/path` (mirrors the `$HOME` env-var hoist). Unresolvable forms
  get a MANUAL note so the compiler's SB4036 forces review: `~user` (needs a
  passwd lookup), multi-tilde words `~/bin:~/lib` (no partial rewrite — would
  drop the 2nd `~`), command-position `~/bin/prog` (DD#23 rejects a dynamic
  command path). `~/$x` composes via the CST `concatenation(word "~/", $x)` →
  `$home/$x`. `convert_tilde_word` in `sash-convert/src/convert.rs`. Verified
  end-to-end: convert→compile→run is byte-identical to bash for `echo ~`,
  `echo ~/foo`, `x=~/p`. spec/convert.sh 57.47-57.55 (9 new, native-only; the
  compiler half is taint.sh 53.x). Rust convert_tests.rs tilde_* (4). NOTE:
  first drafted as 53.x → **collided** with taint.sh's 53.x (test_ids are global
  in spec.db, `native:53.1` shared); renumbered into convert.sh's DD#57 namespace
  (global max was 57.46).
- **dd67-bash-sugar-sweep → DONE (compiler scope)**: §1/§2/§4B + the keep/drop
  table were already done. §3 (sash-convert canonicalize mode) was blocked on
  dd68's brace forms; **dd68 is now done**, so §3 is unblocked but distinct
  Tier-6 converter work — **split out** to `sash-convert-canonicalize-mode`
  (status ready) rather than left lingering. dd67 itself is done.

Native sweep: 0 regressions, +9 (57.47-57.55). Change is isolated to
sash-convert + a spec file; the compiler is untouched.

## ════════ STATUS (2026-08-20) — finishing partials ════════

DG: "finish partials." Also fixed a bless-all bug DG surfaced.

- **bug-mapfile-undeclared-len-ice — front-end completion** (6b12c73): the
  transform auto-declare made `$(len a)` work but the FRONT-END didn't register
  an auto-declared mapfile/read array as a collection, so `bless e in a` /
  `${a[0]}` gave SB4030 ("scalar") — while `argv` and explicit `array a=()`
  worked. `type_check::check_command` now registers mapfile/readarray/read -a
  targets (match-dispatched, cog 15→2). Taint gate preserved. taint.sh ad.3/ad.4.
- **intrinsics-lengths-in-arithmetic → DONE**: all gates verified — `(len a)`
  sexpr evaluates, `$(( ${#a}+1 ))` works, a string intrinsic in arith
  (`(toupper s)`) is a clean SB4030. No code needed (already shipped).
- **int-valued-collections (DD#76) → DONE**: Stage C (bare `a[i]` arith
  rvalue+lvalue) shipped via DD#79 this session; A/B earlier. Backend gaps
  (janet-int-valued-collections, c-local-int-collection-decl) filed separately.
- **dd75-len-polymorphic → DONE (core)**: len type-dispatch + `${#x}` Int sugar
  done. The undefined-`len` hard reject is SAFE as-is (0, no garbage) and split
  to `len-undefined-reject` (P3) — confirmed `check_for` doesn't register the
  loop var, so a naive reject would false-positive on `len <loopvar>`.

Remaining partials: c-janet-dd55-abort-channel (P2, c/janet backend),
dd67-bash-sugar-sweep (P3), tilde-vs-dd53 (P3).



P1s: all done/superseded (the lone `fixed` one — arith `${var:OP}` silent-0 —
normalized to done; native/c gate resolved, only the janet arith-value residual
remains, tracked as janet-arith-subscript-read P3).

Working P2 bugs (bugs-first tier order):
- **bug-braced-rejected-var-bypass DONE**: `${RANDOM}`/`${HOME}`/`${HOME:-x}`/
  `${#HOME}` were silently compiling to empty (validate_expr only checked
  `$name`). Now `validate_expr` validates `${name…}` (any operator/index) via
  `validate_var_name` → SB5017/SB5018; normal `${x}`/`${x:-9}`/`${a[i]:-N}`
  stay clean. spec var_modifiers.sh bvb.1-5; +15, 0 regr. Journal 152.

Remaining P2 bugs (next): bug-mapfile-undeclared-len-ice (ICE),
bang-precedence-and-suppress; then c/janet parity + partials.

- **bug-mapfile-undeclared-len-ice DONE**: `mapfile -t a < f; echo $(len a)`
  with no prior `array a=()` panicked (ICE: undefined array). Fixed by a shared
  `autodeclare_bulk_target` (mirrors argv/DD#60) wired into argv/mapfile/read;
  auto-declared target behaves identically to a declared one (len works,
  element reads correctly tainted). native (c/janet mapfile gaps); taint.sh
  ad.1/ad.2; +2, 0 regr. Journal 155.
- **bang-precedence-and-suppress DONE** (DG reviewability call): `!` binds a
  single reviewer-atomic unit, so `! X && Y` = `(!X) && Y` (was wrongly SB4018);
  `!` on a pipeline → **new SB4040** (escape hatch: wrap in a predicate fn);
  `- ! X` → SB4019 at parse time (all modes). Design principle recorded in DD#55
  (reframed "ambiguous" → **sash must be easier to review than bash**) + DD#70
  addendum. spec errors.sh bng.1-6; 0 regr. Journal 156. (c/janet `!` runtime
  parity still separate.)



DG: "let's tackle the decisions." All three implemented + pushed:

- **A — multi-statement/pipeline `$(…)`: IMPLEMENTED** (this commit). `$(x=1;
  echo $x)` / `$(a | b)` were a silent `$(...)` placeholder; now capture
  correctly. New `SsaValue::CmdSubstBody` lifted via `lift_int_func_calls` →
  `capture_body_into` (reuses `CaptureStart/End` — no new codegen); subshell
  isolation via save/restore of assigned vars. **Closed a taint hole**: the
  taint-source scans inspected only `stmts.first()`, so pipeline/multi-stmt
  external output (and `$(- external)`) flowed as clean, bypassing bless —
  now recursive over all stmts/stages + a matching `CaptureEnd{tainted}` flag.
  native (c/janet capture is a pre-existing gap); taint rejects all modes.
  spec commands.sh 14.3*, taint.sh mcs.*, errors.sh 55.9. +17, 0 regr. Jrnl 154.
- **B — compound-condition catch** (b32e038): see below.
- **C — $LINENO reject** (e671d6a): see below.



DG: "re: string-array cell; I would expect the same behavior as from a string
variable." Then "implement it properly." A cell `${a[i]:OP}` now behaves exactly
like a scalar `${s:OP}`, word + arith.

- **`${a[i]:-N}` / `${a[i]:?msg}` work** (array/map, literal or `$i` index) —
  DD#77 disposition on the cell's value. Closed a **silent-miscompile bug**:
  `try_subscript` dropped the operator after `]`, so `${a[i]:-N}` silently atoi'd
  and `${a[i]:?msg}` never aborted.
- **Bare string cell `$(( a[i] ))` → SB4030** (D1 extends to cells); bare string
  index `a[s]` too. Int cells (DD#76) unaffected.
- Operator parity: `#`/`/`/`@X` → SB4034/SB4015; substring/sed on a cell →
  explicit **SB4039** (staged, follow-up — never a silent drop). DD#78 default
  discipline (taint SB5020 / non-random SB5035) rides for free.
- Representation: `ParameterExpansion.index` + SSA disposition nodes generalized
  `var: VarRef` → `source: Box<SsaValue>` (Var | Subscript). janet word-context
  works; janet **arith** cell disposition = pre-existing `janet-arith-subscript-
  read` gap.

native+c full parity. spec arithmetic.sh scx.1-16 (plan 133→143). 3-mode sweep
**0 regressions, +42 passing**. fmt+clippy clean; complexity gate clear.
Follow-ups: `arith-bare-subscript-followups` gate 4 (substring/sed cell parity);
`janet-arith-subscript-read` (widen scx.12-14). DD#77 "Implemented — string-
array/map cells"; journal 151. **NOT committed at write time; commit + push next.**

## ════════ STATUS (2026-08-18, cont.) — arith subscripts model B Stage 2 + B2 (DD#79) ════════

DG: `(( a[i]++ ))` — shouldn't this work? It's the ratified-but-unbuilt **Stage
2** of arith-subscript-values (model B). Implemented + a design seam DG
resolved. **DD#79** records both parts.

- **Stage 2**: bare `a[i]` in arith now reads + mutates (`$(( a[i]+1 ))`,
  `(( a[i]++ ))`, `(( a[i] op= v ))`, `(( a[i] = v ))`). `BareSubscript` lexer
  token → `ArithExpr::Subscript`/`SubscriptAssign`; lvalue reuses the statement
  subscript-assign write path; rvalue → `SsaValue::Subscript` (arith int key).
- **Bare `m[k]` → SB4030** (was a silent-wrong bug: int-key miss on a
  string-keyed map).
- **B2** (DG): SB5031 extended into arithmetic — a bare name *read* assigned
  nowhere is a typo (`$(( y+1 ))`, an undeclared index). Anything you assign
  (`(( f++ ))`, `for (( i=0;… ))`) is in scope. Function names exempt (DD#58).

native+c; janet arith-value subscript is the pre-existing filed gap
(`janet-arith-subscript-read`); `janet-undeclared-arith-counter` filed. spec
arithmetic.sh bsx.1-7, b2a.1-3; plan 117→127. 3-mode sweep 0 regressions, +24.
Follow-ups filed: `arith-bare-subscript-followups` (`$a[i]` form, nested index,
string-array-cell DD#77 question). DG's line works with `int i=0`.

## ════════ STATUS (2026-08-18) — DD#24 taint bug fixed: scalar assignment no longer propagates ════════

DG, reviewing the DD#78 summary, caught that I'd called `z=$y` a legal "source
assignment" — it's a **read** of `$y`. Investigation confirmed a pre-existing
bug: `taint_assignment` uniquely *propagated* taint (`z=$y` compiled, taint
caught only at a later sink) instead of rejecting the read, contradicting DD#24
§Quarantine ("No propagation", `b=$a` rejected) + journal 036 (`x=$1 # compile
error`). Every other read site rejected; scalar assignment was the lone
offender. It was the root cause of the DD#78 sink-hole class.

**Fixed (this commit):** scalar assignment now rejects a tainted read in the RHS
(SB5020) — `z=$y`, `z="$y"`, `z=${y}`, `z=${x:-$y}`, `z+=$y` — while still
allowing assignment *from a source* (`z=$(getenv X)`). Behavior change:
bless-at-source, not on a copy. Also closed a residual DD#78 hole
(`z=${x:-$(getenv)}`). spec/taint.sh np.1-7; plan corrected 75→85; 3-mode sweep
0 passing→failing, 0 failing→passing (nothing relied on propagation). Journal
149; DD#24 enforcement note.

## ════════ STATUS (2026-08-17, cont.²) — DD#78 disposition-default discipline + DD#60 arg-default ════════

DG proposed `$(arg N)` take an optional default instead of aborting on OOB.
Design ratified **DD#78** (disposition defaults must be clean + non-`random`)
and a **DD#60 addendum** (`$(arg N DEFAULT)`). **Both complete + pushed.**

- **DD#78 Stage 1 (14a3b70):** closed a real taint hole — a `:-` default's
  operand was never taint-checked (`echo ${x:-$(getenv)}` / `${x:-$y}` compiled;
  `z=${x:-$(getenv)}` left z clean). Both taint primitives now recurse into the
  `:-` operand; `check_disposition_defaults` rejects `$(random)` defaults
  (SB5035). Clean-when-read (use rejects; store propagates). taint.sh dd78.1-7.
- **DD#60 arg-default (this commit):** `$(arg N DEFAULT)` returns the default on
  OOB (bare `arg N` still aborts). Runtime `sash_argv_get_or`, 3-backend
  lowering, arity SB5036, DD#78 clean/non-random default (SB5020/SB5035),
  tainted result. argv.sh av.d1-d7, plan 37→44. 3-mode sweep 0-regression +20.

Deferred (filed): `arg-negative-index-compile-reject` (DD#60 negative-index not
enforced), `bare-nested-cmdsub-arg` (general parse limit — quote the nested sub),
`janet-getpid-class-intrinsics` (av.d4 janet guard). Pre-existing `mk test` rot:
`audit-rust-test-suites`.

**NEXT:** back to the DG tier backlog. DD#60/DD#78 need nothing further.

## ════════ STATUS (2026-08-17, cont.) — DD#77 COMPLETE (Stage C1/C2/C3 shipped) ════════

DD#77 (arith string→int conversion discipline) is **complete**. Stage C shipped
this session on top of the mechanism + `-i` work below:

- **ca4f75f** C1 — un-prune `:?` everywhere (word ctx aborts on unset/empty;
  arith on non-int); `RequireValue` SSA node + `sash_require_value` runtime +
  3-backend codegen.
- **bd296c3** C2 — arith strict-disposition semantics for `:-`/`:?` via
  `sash_atoi_or`/`sash_atoi_or_abort` (sash_is_int gate); native+c.
- **(this commit)** C3 — **D1** bare string operand in arith → SB4030 (directs to
  `${s:-N}`/`${s:?msg}`); **D4** the `:-` default must be a decimal integer
  literal (was a silent atoi→0 footgun for `${s:-hello}`/`${s:-0xff}`); migrated
  the DD#72 bare-coercion tests (`nl.s*`, `30.2`, `pex.7`) to dispositions;
  amended DD#72. type_check-only (front-end, all modes reject).

Out-of-range *input* is now a disposition concern (`${s:?}` abort / `${s:-N}`
default via `parse_strict_i64`'s range check), NOT a DD#25 arithmetic overflow
(that's for operations like `INT_MAX+1`, still covered by 25.2–25.12).

Guarded gaps (filed, not blocking): janet arith-value `${…}` disposition
(`janet-arith-subscript-read`); full DD#72 grammar (`0x`/`0o`/`0b`) as a `:-`
default (`arith-disposition-hex-octal-default` — D4 currently decimal-only, which
converts the silent-0 into a clean reject). Full 3-mode sweep: 0 passing→failing,
0 gone, 11 new passing; native 0-fail; clippy clean.

**NEXT:** back to the DG tier order (notes/status.md lower blocks) — the T4
c/janet parity / remaining backlog. DD#77 needs nothing further.

## ════════ STATUS (2026-08-17) — DD#77 arith string→int discipline (mechanism + `-i` done; Stage C next) ════════

Design conversation ratified DD#77 (string→int conversion discipline in arith)
+ DD#30 point-1 refinement. Shipped this session (all pushed hand-parser, 0 real
regressions, native 0-fail):

- **54b57a7** point-1 — bare numeric literal to a known-int LHS (`int x=0; x=5`,
  `a[1]=99`); DD#30 amended. Fixed a pre-existing janet scalar-`+=` silent
  miscompile inline.
- **fe5af21** DD#77 A — route arith `${…}` through the real param parser; fixes
  the P1 silent-0 bug (`${s:-0}` → bogus var → 0).
- **ca44731** DD#77 B — `sash_is_int` + `[[ -i EXPR ]]` test, 3 backends.
- **87d60d7** fix — Stage A opened a pruned-operator ICE hole (`:?`/`:=`/`##`… in
  arith bypassed SB4034 → ICE); walk now descends arith, rejects cleanly.
- **a8d2c22** DG follow-up — `-i` range-checked (valid AND representable);
  runtime atoi overflow now a clean `sash_arith_overflow` trap (was rc-99 panic).

DD#77 recorded (+ resolved sub-decisions: `:?` un-prune everywhere mirroring
`:-`; D1 as approved, DD#72 bare-coercion superseded; `-i` range-checked).

**NEXT — DD#77 Stage C (unblocked, large):**
- C1: un-prune `:?` (word ctx aborts on unset/empty; arith on non-int) — abort
  node + 3-backend codegen.
- C2: arith strict-disposition semantics for `:-`/`:?` (sash_is_int gate).
- C3: D1 bare-string-in-arith → error + D4 numeric-literal default + migrate
  DD#72 `arith_literals.sh` bare-coercion to dispositions + amend DD#72.
See notes/tasks/dd77-arith-string-conversion.md.

## ════════ STATUS (2026-08-16, cont.) — DD#76 int-valued collections A+B DONE ════════

DG: fold int-valued collections into the arith-subscript effort. **DD#76**
(int-valued collections, `val_len` double-duty) recorded; Stages A+B shipped
native + c.

- **int array a=(10 20 30) / int map m=([k]=5)** now declare (top-level parser
  stacks `int`+structure; `local int array` already stacked via parse_local).
- **Storage (encoding b, DG-chosen):** int cell = `{val_ptr:null, val_len:i64}`.
  Runtime unchanged. Two SSA additions: `SsaValue::IntCell` (write → `(null,i64)`,
  wraps literal elems + assign RHS) and `is_integer_value` on `Subscript` (read:
  string ctx → itoa, int ctx → raw val_len).
- **Slot fix:** a collection var holds a table pointer even for int elements
  (value-shape keys slot allocation) — native + emit_c.
- **Consistency:** bare `a[1]=99` → SB4030 (uniform with `x=5`); use
  `a[1]=$(( 99 ))` (no DD#30 quotes for an int cell). `int x=${a[0]}` works.

specs arithmetic.sh ivc.1-11 (plan 90→101). Full 3-mode sweep: 0 passing→failing,
0 gone, 23 new passing. native 0-fail. clippy clean. Rust == benched baseline.

**Remaining:** Stage C (bare `a[i]` in `(( ))` — arith lvalue/rvalue, the
arith-subscript-values stage 2). Backend gaps filed: janet-int-valued-collections
(janet is_int conflation), c-local-int-collection-decl (function-local table var
undeclared; ivc.8 native-only). Open DG question: const+collection stacking
(DD#35 orthogonality).

## ════════ STATUS (2026-08-16) — arith-subscript reads (BUG 1) fixed; janet oracle rethink ════════

DG reported 2 suspected bugs in array-values-in-`(( ))`:
- **BUG 1 (real, FIXED stage 1):** `${a[i]}`/`${m[k]}` evaluated to 0 in arith
  (parser read `${a[0]}` as a var named "a[0]"). New `ArithExpr::Expansion(Expr)`
  imports the normal `Subscript` Expr via shared `expr_to_value`. native+c
  (janet value-path gap filed: janet-arith-subscript-read). specs asx.1-6.
- **BUG 2 (not a bug):** OOB/missing key → zero-value (sparse table-backed,
  DD#53); DG: keep it.
- Design (arith-subscript-values): `${…}` is an rvalue value-import ONLY (never
  an lvalue — preserves `${…}`="the value of"); **stage 2 remaining**: bare
  `a[i]` int-array arith lvalue+rvalue (`(( a[i]++ ))`), bare `m[k]` → error.

Also this session: **janet oracle rethink** (janet-rt-primitive-library) — build
a `sash-rt.janet` primitive library + thin leaf lowering, keep IR control flow.

native 904/0-fail; c 13 / janet 36 == baseline; 0 regressions.

## ════════ STATUS (2026-08-15) — DD#58/Option C sexpr int intrinsics DONE ════════

bug-arith-sexpr-intrinsics-zero complete (both parts, pushed on hand-parser):
- **Part 1 (d42cac5)** — category A int intrinsics compute in a sexpr (were
  silent 0); `$(( (x) ))` grouping bug (→0) fixed; Q3 `random` int-typed.
- **Part 2** — category B + Q2: a string producer in a sexpr → SB4030 type
  error (type_check third pass). Closes the getenv/arg validator-bypass +
  user-string-fn side-effect run. dtoi/itod rejected in a sexpr.

Governed by Option C (`$()` return-type-polymorphic; int producers work in both
`$(f)` and `$(( (f) ))`). specs arithmetic.sh sx.1–17 (plan 62→84). native
898/0-fail; c 13 / janet 36 == baseline; 0 regressions; Rust suites == HEAD.
Deferred/filed: getcpid-in-sexpr, c/janet signum intrinsic.

## ════════ STATUS (2026-08-15) — DD#58/Option C sexpr int intrinsics (part 1) ════════

Implementing bug-arith-sexpr-intrinsics-zero (Option C: `$()` return-type-
polymorphic; int producers work in both `$(f)` and `$(( (f) ))`). Part 1 done:
- **Category A** — int intrinsics compute in a sexpr (were silent 0):
  `(getpid)`→`$$`, `(status)`→`$?`, `(getppid)/(argc)/(random)/(signum)/
  (epochseconds)`→ CmdSubst; native direct-i64 via shared `int_intrinsic_i64`,
  c via atoi fallback. native+c (signum native-only — c/janet TODO stub, filed
  c-janet-signum-intrinsic).
- **Grouping bug** — `$(( (x) ))` on a plain variable returned 0; now returns
  the variable (arith parser defers the `(name)` call/group decision here).
- **Q3** — `random` int-typed (`int n=$(random)` was wrongly SB4030).

spec arithmetic.sh sx.1–11 (plan 67→78). native 892/0-fail; c 13 / janet 36 ==
baseline; 0 regressions; codegen 179/77 + e2e 89/41 == HEAD.

**Part 2 (next):** category B (string producer in a sexpr → type error; closes
the getenv/arg validator-bypass) + Q2 (dtoi/itod rejected). Needs a type_check
arith-walk. getcpid-in-sexpr deferred.

## ════════ STATUS (2026-08-14 unattended, cont.) — DD#75 §1 special-var sugar DONE ════════

DD#75 §1 (special vars are sugar; keyword forms canonical) — compiler core done.
Both spellings already lowered to one node; pinned the equivalence + reframed
the surface: `$$`≡`$(getpid)`, `$?`≡`$(status)`, `$!`≡`$(getcpid)` (specs
misc.sh spv.1–4; spv.1–3 `native,c` — janet execs getpid + mis-tracks arith
status, filed). SB5012 leads with `$(status)`. DD#75 §1 + DD#24 docs updated.
Convert + public GUIDE/SUBSET prose → converter-special-var-sugar (DG voice).
native 881 / 0 fail; c 13 / janet 36 == baseline; 0 regressions.

**Only DD#75 §5 (args-model) remains** — the invasive one (function args →
implicit 1-indexed `args` array; `$1`→`${args[1]}`, `$@`→`@args`, `$#`→`${#args}`,
`$*` removed). Now **scoped** in its task file (current model = scalar SSA vars
"1".."9"/"#", not an array; files: parse/transform/validate/type_check/codegen).
Has an implementation-approach fork needing DG's nod before coding: (a) a real
`args` table (true DD#75 uniformity, larger/riskier) vs (b) surface sugar over
the existing scalar vars (smaller, only partial). Parked before a rushed
invasive edit (same call as the retire-bracket pause).

## ════════ STATUS (2026-08-14 unattended, cont.) — bug fix: special-var arith silent-0 ════════

Stop-everything bug found while assessing dd75-special-var-sugar (§1): `$$`/`$?`
evaluate to **0** as an operand in a compound `$(( ))` expression (standalone +
word contexts were fine; `$#` unaffected — real slot). Pre-existing; violates
DD#24/DD#31 (both list these as clean arith operands). Root cause: the int
codegen path (`lower_value_as_int`) lacked the computed-special handling the
string path has (`$`→getpid, `?`→exit-status). Fix: `special_var_as_int` helper
+ threaded `exit_status_var` through `lower_value_as_int` (native). c already
correct; janet errors (filed). Test-first arithmetic.sh 31.20–24 (`native,c`;
plan 62→67). native 0 failing; c 13 / janet 36 == baseline; codegen 179/77 +
e2e 89/41 == HEAD. Filed bug-arith-keyword-pid-intrinsic-zero (sexpr
`(getpid)`→0) + bug-janet-special-var-arith.

## ════════ STATUS (2026-08-14 unattended, cont.) — dd75-retire-bracket DONE ════════

Retired the `[@]`/`[*]` bracket-subscript collection forms (DD#75 §4). `@a` is
the sole splat; `${#a}`/`$(len a)` length; `$(keys m)` keys (maps only).
**Parse-level reject (SB4038)** — `${a[@]}` and `@a` share the `arrvals` repr,
so they can only be distinguished at parse. Tagged 5 bracket sites in
`parse/expr.rs` (`*_retired` ops); `parser.rs` `retired_bracket_operator_diag`
emits SB4038 with the canonical replacement per form. Canonical `@a` untouched.

specs types.sh 75.10–16 (plan 86→93) + ws.q4/q5 flipped to SB4038; Rust
migrated to canonical forms (brush/c/codegen/e2e, all pass). native 0 failing;
c 13 / janet 36 == baseline (0 regressions). Converter half filed as
`converter-retire-bracket-splat`.

**Remaining DD#75 (2 invasive P3, not started):**
- dd75-args-model — function args → implicit `args` array; `$1`→`${args[1]}`,
  `$@`→`@args`, `$#`→`${#args}`, `$*` removed. Most invasive.
- dd75-special-var-sugar — `$?`/`$!`/`$$` → sugar for `$(status)`/`$(getcpid)`/
  `$(getpid)`; mostly converter/docs/taint framing (compiler already accepts).

## ════════ STATUS (2026-08-14 unattended, cont.) — DD#75: 5 tasks landed ════════

Continued the unattended DD#75 run (test-first, pushed on hand-parser):
- 7193ba6 bug-subscript-assign-bare-key (P2)
- a17909f dd75-keys-map-only (P3) — + added the missing For-arm to type_check
- 51c3754 + dcadfd9 intrinsics-lengths-in-arithmetic gates 1+2 (P2, DEMO):
  `$(( (len a)+1 ))` and `$(( ${#a}+1 ))` evaluate (sexpr + ATok::LenOf)
- 418455f string-func-capture-in-arg-ice (P2) — DG: lift-and-print; shared
  capture_func_into; `echo $(greet)` works incl. runtime capture (no ICE)

All native 0 failing; c 13 / janet 36 == baseline. Backend gaps filed:
janet-join-intrinsic-gaps (join/keys/len-in-arith), cjanet-function-stdout-capture.

DG unblocked the invasive DD#75 tasks (unreleased → no migration risk; convert
Qs as follow-ups). Remaining, scoped in their task files:
- dd75-retire-bracket-subscripts — PARSE-level reject (${a[@]} shares the arrvals
  repr with @a) + flips passing wordsplit/codegen/brush tests.
- dd75-args-model — implicit `args` array (most invasive).
- dd75-special-var-sugar — mostly converter/docs.

## ════════ (prior status below) ════════
## ════════ STATUS (2026-08-14 unattended) — DD#75 continuation: 3 more tasks ════════

Unattended session (SOP: ready backlog, dependency order, test-first, skip
decision-gated). Landed on hand-parser:
- 7193ba6 bug-subscript-assign-bare-key (P2) — indexed `a[i]=` bare-key reject.
- a17909f dd75-keys-map-only (P3) — keys map-only; added the missing For-arm
  to type_check (for-in headers now type-checked).
- 51c3754 + dcadfd9 intrinsics-lengths-in-arithmetic (gates 1+2) — DG demo:
  `$(( (len a)+1 ))` and `$(( ${#a}+1 ))` now evaluate (arith sexpr + braced
  `${#x}` via ATok::LenOf → the len FuncCall shape). Filed sub-task
  arith-braced-length (done). string-intrinsic-in-arith reject deferred.

All: native 0 failing; c 13 / janet 36 == baseline. janet len/keys/join-in-arith
gaps filed (janet-join-intrinsic-gaps).

**Remaining DD#75 (3 invasive P3 language changes — not started):**
dd75-special-var-sugar ($?/$!/$$ → sugar; converter+docs+taint framing),
dd75-args-model (implicit `args` array; invasive), dd75-retire-bracket-subscripts
(retire [@]/[*]; migration). Better as focused efforts. string-func-capture-in-arg-ice
is decision-gated (works vs reject) — for DG.

## ════════ (prior status below) ════════
## ════════ STATUS (2026-08-11, late) — DD#75 implementation: 3 parts done ════════

Implementing DD#75 in DG's recommended order, **test-first** with spec tests.
Pushed on `hand-parser`.

**DONE (committed + pushed):**
1. `keys-splat-context-bypass` **P1** (`9685412`) — `$(keys m)` bypassed the
   SB5025 splat guard → silent join + garbage bytes (memory-unsafe). Added
   `is_keys_splat_expr`; SB5025 outside for-in. Also fixed a stray `dp.co6`.
2. `bug-join-explicit-separator-segfault` **P1** (`3d1f8f5`) — DG chose
   `join arr SEP` (array-first); swapped arg order; `check_join_operand` rejects
   a non-array operand (SB4030) — no more segfault. janet join gaps guarded +
   filed (`janet-join-intrinsic-gaps`).
3. `dd75-len-polymorphic` **P2 core** (`1b35dc2`) — `len` dispatches on TYPE
   (collection→count, string→bytes); `${#x}` is Int sugar (`int n=${#a}`→3,
   `int n=${#s}`→5). Demo works. Partial: arith + undefined-reject deferred.

All three: native 0 failing; c 13 / janet 36 == clean baseline.

**REMAINING DD#75 (larger, P3 language changes — not started):**
- `dd75-keys-map-only` (P3) — reject `keys` on indexed arrays.
- `dd75-special-var-sugar` (P3) — `$?`/`$!`/`$$` → sugar; touches DD#24, converter, docs.
- `dd75-args-model` (P3) — function args → implicit `args` array; invasive.
- `dd75-retire-bracket-subscripts` (P3) — retire `[@]`/`[*]`; migration.
- Follow-ons: `intrinsics-lengths-in-arithmetic` (arith len), `bug-subscript-assign-bare-key`.

## ════════ (prior status below) ════════
## ════════ STATUS (2026-08-11) — DD#75 drafted; collections/positionals redesign ════════

**DG design session → DD#75 "Named/keyword forms canonical; `$`-sigils sugar;
collections uniform"** (`notes/decisions/DD75-…md`, status accepted). Captures a
project-wide **pivot**: bash-compat → unambiguous/self-consistent/symmetrical/
transparent. Codifies five rules:
1. `$?`/`$!`/`$$` → sugar for `$(status)`/`$(getcpid)`/`$(getpid)` (inverts
   journal-050; keyword canonical). DG confirmed incl. `$?`.
2. `len` type-polymorphic (array/map→count, string→bytes); `${#x}` Int sugar.
3. `keys` map-only (indexed → reject).
4. `@a` sole splat; retire `${a[@]}`/`${a[*]}`/`${#a[@]}`/`${!a[@]}`.
5. Function args = implicit 1-indexed `args` array; `$1`→`${args[1]}`,
   `$@`→`@args`, `$#`→`${#args}`, `$*` removed→`$(join args)`.

**Bugs surfaced while designing (filed):**
- `keys-splat-context-bypass` **P1** — `$(keys m)` in cmd-arg position bypasses
  the SB5025 guard → silent join + **garbage bytes** (memory-unsafe). Root:
  `check_argv_splat` hard-coded to `$(argv)`.
- `bug-join-explicit-separator-segfault` **P1** — `$(join a SEP)` segfaults.
- `bug-subscript-assign-bare-key` P2 — `a[i]=` bare key accepted, read rejected.

**Task breakdown:** `dd75-special-var-sugar`, `dd75-len-polymorphic` (P2, demo),
`dd75-keys-map-only`, `dd75-args-model`, `dd75-retire-bracket-subscripts`.
Superseded: `length-operator-int-typing`, `len-nonarray-operand-reject`.

**Implementation NOT started** — DD lands first. Next: fix the two P1
memory-safety bugs (independent of the language change), then §2 (len) for the demo.

## ════════ (prior status below) ════════
## ════════ STATUS (2026-08-10, late) — dispatch operator parity COMPLETE ════════

**Pushed `3bca6b1` on `hand-parser`.** Achieved dispatch-boolean parity for compact
operators (==, =~). Issue: `[[ $x==a { … } ]]` required spaces; `[[ $x==a ]]`
worked fine. Root cause: parser's `parse_test_binary_tail` returned a Unary
(implicit `-n`) when no token-level binary operator followed. Fix: added
`extract_compact_operator` group (4 helpers) to detect and split compact
operators in Literal and Concat expressions, recreating proper Expr structures
and preserving variable references. Dispatch form now works identically to
boolean test: compact, spaced, with variables, with multiple patterns, with `=~`.

Spec: added tests I1–I8 (dp.co1–dp.co8) covering compact parity. Verification:
spec tests PASSED (1696 native passing, net new ~20 from dispatch tests).
Complexity: initially flagged high cog, refactored to flatten nesting
(extract_from_literal, extract_from_concat, match_operator, extract_leading_operator).

Closed task: dispatch-compact-operator-spacing (P3).

## ════════ (prior status below) ════════
---
title: Status
modified: 2026-06-07
---
title: Status
modified: 2026-06-07
---

<!-- BEGIN: mk-status (regenerated by contrib/status-update.sh) -->
## Current branch: hand-parser

## Spec status: 6740 passing / 0 failing of 6753 (spec.db @ fffc23f)

_Regenerated 2026-09-28 by `mk status`. Edits inside this block will be
overwritten; edit outside the BEGIN/END markers._
<!-- END: mk-status -->

### Session 2026-08-10 — `[[ ]]` whitespace-optional, like `(( ))`

DG (demo) hit an asymmetry: `return((1==1))` parsed but `return[[a=a]]`
didn't. Root cause: `[`/`]` are word chars (subscripts), so `[[`/`]]` only
tokenized when whitespace-delimited, while `(`/`)` are word-breaks. Fix
(`scan_word_chars`): break a word on a **doubled** bracket (`[[`/`]]`), so
`[[ … ]]` is whitespace-optional everywhere (`if[[…]]`, `while[[…]]`,
`![[…]]`, compact `[[a==a]]`, DD#74 `return[[…]]`); a single `[`/`]` still
absorbs as a subscript char. Safe because sash subscripts need a `$` sigil
(`a[${b[0]}]` ends in `}]`, never `]]`); a repo-wide search found 0 glued
brackets in sash source. Also guard-converted the arm's nested `if`s,
dropping `scan_word_chars` cognitive 14→3 (grandfathered gate violator).
Specs `spec/dispatch.sh` H1–H9; **0 regressions, 27 new passing** (9×3
native/c/janet); Rust failure set identical to HEAD. DD#59 addendum. Dispatch
`==` spacing inconsistency (SB4028) filed separately
(`dispatch-compact-operator-spacing`).

### Session 2026-08-09 — DD#74 predicate functions (phases 1-3)

DG established DD#74: a function used in a *boolean context* (an `if`/`elif`/
`while`/`until` condition, `! f`, or a `&&`/`||` operand) is a **predicate**;
its status comes only from an explicit `return` — no implicit trailing-command
status. Shipped native/c/janet, 0 native regressions:

- **Phase 1** (`62fa5c1`) — `return N` drives the enclosing condition (it now
  syncs `test_result_var`, not just `$?`). Fixed c's missing `!` sigil and
  janet's `if <userfunc>`.
- **Phase 2** (`679262a`) — `return <producer>`: `return CMD` / `return [[ ]]`
  / `return (( ))`. New `StmtKind::ReturnProducer`; the command form is
  consumed (never aborts).
- **Phase 3** — definite-return in a boolean context (**SB4037**):
  `check_predicate_definite_return` in validate.rs rejects a predicate that can
  fall off its end. A bare-only void call (a procedure) is unaffected. specs
  74.13–74.18. Refactored `validate()` (cog 8→4) to clear the complexity gate
  after touching it.

Prior focus below.

### Session 2026-07-06/07 — backlog prioritization (DG rule)

DG set the working order for all remaining tasks: **(1) finish partially
completed work → (2) fix compiler bugs → (3) implement remaining compiler
features → (4) c/janet backend parity → (5) convert tasks.** "Convert is
secondary to anything compiler-related." Dependencies may pull an item
earlier, but nearly all `depends:` are satisfied (DD docs or done tasks;
`advance-dd23-resolution` is now done, unblocking
`tighten-cmdsub-taint-execve`), so the tiers apply cleanly. **⚠ = blocked
on a DG decision before it can complete.**

**Tier 1 — finish partials (native)**
1. ~~`abort-diagnostics-remaining`~~ — DONE 2026-07-07 (native abort
   diagnostics for external/cmdsub/pipeline/regerror9; bg-exec uses the
   status form, errno-precision filed as `bg-exec-errno-message`).
2. ~~`implement-getpid-builtins`~~ — DONE 2026-07-07 (getpid + random landed
   native+c; `$(random)` = non-negative i64 via getrandom(2). janet class
   gap filed as `janet-getpid-class-intrinsics`, Tier 4). **Tier 1 COMPLETE.**

_Correction 2026-07-07 (autopilot): `dd55-abort-channel-remaining` moved to
Tier 4 — it is entirely c/janet backend work (native landed in `bc65140`),
so it follows the native tiers per the native-first rule. `dd67-bash-sugar-
sweep` compiler part is DONE (§1/§2/§4B shipped); its only remainder is §3,
a sash-convert canonicalize mode → moved to Tier 6._

**Tier 2 — compiler bugs (native)**
5. ~~`bug-echo-builtin-array-splat-ice`~~ — DONE 2026-07-07 (SB5030 rejects
   `echo @a`; ICE backstop unreachable)
6. `tighten-cmdsub-taint-execve` — DONE 2026-07-07 (printf/pwd made
   external — removed the Printf/Pwd builtins; execve capture tainted)
7. `bug-test-operand-concat-splice` — DONE 2026-07-07 (NOT A BUG: repro was
   arithmetically wrong, `run`+`ing`=`runing`; bash agrees; guarded ow.11-13)
8. `tilde-vs-dd53` — compiler half DONE 2026-07-07 (SB4036 focused reject);
   converter rewrite half → Tier 6.

**Tier 2 complete** (native compiler bugs); Tier 3 next.

**Tier 3 — remaining compiler features (native)**
9. `reject-undeclared-scalar-read` (P2) — DONE 2026-07-08 (DD#62 Model B:
   meaningful zero value + a scalar typo check. SB5031 rejects a scalar
   read of a name introduced nowhere in scope; `${x:-…}` is the opt-out.
   Single-pass scope-membership, not definite-assignment. Recurses into
   command-substitution bodies with subshell scope. `spec/undeclared.sh`
   ud.1-16 all 3 modes; 0 regressions. Filed janet-scalar-zero-value and
   bug-multistatement-cmdsub-placeholder surfaced by the specs.)
10. `regex-interpolation-delimiters` (P2) — DONE 2026-07-08 (DD#63 D/E/G:
    both `${var:s/…}` and `var =~ s/…` re-routed at parse to interpolating
    `$(sub|gsub "$var" RE SUB)` — `$x` in a field now expands (D7/D8).
    `\xnn` hex escape in the vendored engine (E1/E2). Lexer Path B
    (`scan_braced_content`) scans the `:s/` fields by slash count so a `{`/`}`
    byte doesn't truncate `${…}` (G1/G2/G3), malformed payload → SB4032
    (G5/G8). spec/regex.sh 63.d7/d8/e1/e2/g1-g8; 0 regressions. Filed:
    decide-regex-bare-brace (bare `{` literal vs `\{`), and
    retire-legacy-sed-regex-replace-machinery (the old `:s`/regex_replace
    transform+codegen is now unreachable, remove it).)
11. `dd70-error-model-contract-spec` (P2) — DONE 2026-07-08 (filled the
    gaps: bare `[[ … ]]` is now a must-consume (B) boolean —
    check_boolean_must_consume generalized from match to also cover
    BuiltinCmd::Test → SB4018 bare / SB4019 `- [[ ]]`; consumed (if/&&/||/`|| :`)
    and dispatch `[[ … { } ]]` forms unaffected. Contract tests added to
    spec/errors.sh: e.b70.tbare/tdash/tdisc/tand/tor/fndisc, e.a70.silent/
    regcomp. rerr-asserting + `-> int`-discard cases are native-only (the
    c/janet harnesses discard runtime stderr; c/janet also can't yet compile
    a `-> int` fn in `|| :` — filed c-janet-fn-int-discard). 0 regressions.)
12. `glob-as-array-splat-disposition` (P2) — DONE 2026-07-08 (Part 1 was
    already done; Part 2 == #13, now landed → whole task done).
13. `glob-splat-disposition-word` (P2) — DONE 2026-07-08 (DD#69 Part 2:
    external-splat empty-disposition. Bare `@a` into an external command
    (`/`-prefixed post-resolve head) → SB5032; `@{a:-WORD}` (empty → single
    arg WORD; `@{a:-}` → 0 args) / `@{a:?WORD}` (empty → abort exit 1
    `<progname>: WORD`); only `:-`/`:?` (else SB5033). New `@{…}` lexer token
    (scan_at_brace) → parser (arrvals_default/arrvals_abort) → SsaValue::
    ArraySplat.disposition → runtime sash_splat_disposition, applied by all
    three argv builders. spec/glob_splat.sh gs.1-13; wordsplit 22.14-19
    migrated; 0 regressions. **Tier 3 fully COMPLETE.**)
14. `argv-snapshot-to-outparam` (P2) — DONE 2026-07-08 (DD#60 revision:
    `array a=$(argv N)` → the out-param builtin `argv [N] a`. New
    BuiltinCmd::Argv + SsaInst::ArgvFill (mirrors Mapfile/MatchInto);
    removed SsaValue::ArgvSnapshot. Auto-declares the array (transform emits
    an empty-array Assign to reserve the versioned `a_0` slot, then fills —
    also fixes the mapfile-style ICE on `len` of an undeclared fill target)
    + registers it in the type checker. Fills a tainted array (DD#61):
    taint_read_target extended to argv. Old `array a=$(argv …)` → SB5025.
    native/c/janet all 37 argv specs; 0 regressions. **Tier 3 COMPLETE.**

**Tier 4 — c/janet backend parity** (native is the gate; after tiers 1-3)
15. `c-janet-dd55-abort-channel` (partial — do first) ⧉ reconcile overlap
    with `dd55-abort-channel-remaining` (both are DD#55 abort in c/janet)
16. `dd55-abort-channel-remaining` (P2) — pipefail/c-cmdsub/descriptive
    msgs/janet pipeline (moved from Tier 1: all backend)
17. `c-janet-compound-pipeline-stage`
18. `cf-backend-gaps-brace-surfaced`
19. ~~`echo-read-inline-pipe-cjanet`~~ — DONE (2f183c2): DD#20 lastpipe
    read/mapfile for c+janet + bare `array`/`map` decl fix (janet). Extended
    beyond the single-read gate to 22.4-10/22.12. c 47→38, janet 70→61.
20. ~~`janet-arith-string-coercion`~~ — DONE (e91a9ac): `_num` coercion at
    arith use sites; DD#72 0x/0o/0b; fixes 58.2/3, cf.f5/f8, 16.2, nl.s*.

**Tier 4 extra (not a pre-filed task) — DONE this session (2026-07-14):**
- DD#58 int-func-in-arith c+janet (793c8f3): c 58.1-8 (fixed the `$?`-assign
  → __exit_status mis-wire); janet 58.1/4/5/8 (nullary `(f)` call + `_ret`
label numeric return). janet 58.2/3 → DONE via janet-arith-string-coercion.
- dtoi/itod fixed-point intrinsics c+janet (c655bc5): 36.1-4 both modes.
- janet arith string→int coercion `_num` (e91a9ac): 58.2/3, cf.f5/f8, 16.2,
un-guarded nl.s1-10.
- c_var_name distinct special-var names + enforce transform diagnostics for
  c/janet/manifest (6d0a619): fixed dup-`__ptr` compile error + made the
  oracles reject SB4016-class programs like native. Fixed 0.6/46.4/48.1/
  55.12/13/14/17c/16.2/0.5.
- dtoi/itod (c655bc5), janet arith coercion (e91a9ac), unset collection
  (ccb643c: 30.11), read-herestring + multi-part DoubleQuoted (076d3f3: 24.6).
- **DD#55 pipeline semantics pass** (027753a, DG-ratified): context-independent
  aggregate (pipefail over non-suppressed segments); position decides only
  abort-vs-branch. Fixed pipefail-in-conditions, leading-`-` later-stage
  abort, non-first-`-` stage-drop; `pipeline &&/|| C` → SB4023. Per-segment
  `suppressed` mask on SSA Pipeline. specs 55.32-40. native 792→801.
- Session tally: **c 47→13, janet 70→37, native 0** (gate green throughout;
  10 code commits; 0 regressions). Remaining Tier 4 is feature-sized: 56.x
  signals/END, DD#55-abort remainder (c external cmdsub 55.7/8/9, `$!`
  38.5/55.19, descriptive msgs), janet control-flow singletons (cf.f9/f10/
  f12), scattered singletons. See journal 132.
21. `cmdsub-followups-taint-intrinsics-c`
22. `oracle-blessall-mapfile`
23. `janet-getpid-class-intrinsics` — janet execs getpid-class names as
    fake commands; add os/getpid/os/getenv/os/cryptorand/… dispatch

**Tier 5 — hardening / infra / cleanup** (compiler-related → before convert)
22. `eliminate-ice-arms-via-types` (bug-surface; may pull up near tier 2)
23. ~~`excise-dormant-coercion-machinery`~~ — DONE (already excised in
    `3a352ef`; verified 2026-07-08, no Coercion refs remain)
24. `audit-string-keyed-builtin-dispatch`
25. `fuzz-validator`
26. `brush-rerun`
27. `r47-validate-rs-per-rule-tests` (P7)

**Tier 6 — convert (last among work)**
28. `converter-positional-use-needs-bless` (P2) — RESOLVED (DG 2026-07-08:
    option 1, hoist + placeholder bless + MANUAL marker, $HOME-style)
29. `converter-case-to-dispatch` (P3, policy resolved)
30. `converter-var-modifier-migration` (P3)
31. `converter-compound-postfix`
32. `dd67-bash-sugar-sweep` §3 only — sash-convert canonicalize mode
    (compiler §1/§2/§4B already done; §3 couples to dd68 brace forms)
33. `tilde-vs-dd53` converter half — rewrite bash `~`/`~/path` →
    `$(getenv HOME)` (+ bless); compiler half done (SB4036)

**Deferred to the very end:** `audit-rust-test-suites` (+ superseded
`rust-test-suite-rot`, `stale-rust-test-suites`) — revisit its specific
actions with DG at pickup.

**Out-of-band (not in the linear sequence):** `cohort-complexity-cog-cc`
(perpetual ratchet, pre-commit-gated), `r47-promote-monitoring-rules`
(process).

### Session 2026-07-05 (cont.) — DD#72 numeric literals in arithmetic

DG questioned arbitrary bases ("we have 0xff/0377 — why base#?"). Became a
full numeric-literal model + fixed **4 silent miscompiles** (`16#ff`→16,
`2#1010`→2, `0o10`/`0b1010`→0) plus 2 silent-error sites (unknown-byte skip,
no leftover-token check).

- **Model (DD#72):** constants in `$(( ))` = decimal + `0x`/`0o`/`0b`;
  reject leading-zero (`010`/`0377`) + `base#` + bad-prefix-digit cleanly
  (SB4035). Runtime string→int (`sash_atoi`) = honor `0x`/`0o`/`0b`, else
  decimal with leading zeros ignored (`x=010`→10, the anti-footgun; makes
  bash `10#$var` unnecessary). DG: "honor prefixes but ignore leading
  zeroes without the x/o/b".
- `arith.rs` (BadLit→SB4035, radix scanner, trailing-token reject),
  `lexer.rs` (`#` not a comment in arith), `sash-rt/inner.rs` (atoi radix
  prefixes). `spec/arith_literals.sh` new (native/c 31/31; janet skips the 9
  string-coercion tests — filed `janet-arith-string-coercion`). Migrated
  arithmetic.sh 26.7 (`017` now rejects) +26.7b (`0o17`). **0 regressions.**

### Session 2026-07-05 (cont.) — P1: functions as pipeline stages

DG picked `bug-function-in-pipeline` (P1, confirmed live: `fn f { echo hi
}; f | cat` crashed with `sash: bug: bare command passed to sash_exec`,
breaking the SB4021-4024 "wrap in a function" escape hatch). **Native
fixed, 0 regressions.**

- A known-function `Command` pipeline stage now inlines as a forked
  `__compound_` child (args bound as $1..$N via `setup_callee_positionals`,
  `$@` expanded, `return` → ret_block, subshell semantics via
  `state.snapshot()`/`restore()`), instead of exec'ing the bare name.
  Fixed both `handle_regular_pipeline` and `build_producer_stages`
  (pipe-while-read/lastpipe); shared `push_command_stage` helper.
- **c/janet**: pre-existing gap — they have no `__compound_` support at all
  (even `{ }`/for/if stages fail: c `bare command`, janet `unknown symbol
  p1`). DG-decided to split it out → `c-janet-compound-pipeline-stage` (P2).
  `spec/pipe_functions.sh` (pf.1-11) is native-only guarded (like brace.sh).

### Session 2026-07-04 (cont.) — DD#65 var modifiers: `:-` word (`:- ≡ "…"`)

DG-directed, spec-first, **0 regressions native/c/janet** (native 648→663
before this entry's tests; c 52 / janet 74 unchanged). DD#65 build-up:
- **toupper/tolower** builtins (896a424), **basename/dirname** sugar
  (bdcac8f) — earlier this session.
- **`${var:-DEFAULT}` full interpolation** — DG settled the shape as
  **`:- ≡ "…"`**: the default is *exactly* a double-quoted-string body.
  Quotes (`'`/`"`) are ordinary literal bytes (no strip/group), whitespace
  verbatim, `$…`/`` `…` `` interpolate; the **C-string is the any-byte
  hatch** (`${x:-$'a\x7db'}` → `a}b`). For full consistency the `${…}`
  brace matchers (`scan_brace_expansion` + `consume_dq_brace_expansion`)
  dropped their quote/`\`-skip, so the **first unnested `}` closes**
  (`${n:-'a}b'}` = `${n:-'a}` + `b'}`). Bare env var in a default now
  rejects SB5018 (real interpolation context).
- Lowering: operand → `ExprKind::DoubleQuoted` → `SsaValue::DefaultValue
  { var, default }` (native + c select var-or-default; janet interpolates
  at IR level). spec/var_modifiers.sh vm.dv1-11 (+ tu/bn) = 24, all modes.
- DD#65 doc updated to the `:- ≡ "…"` model (superseded the earlier
  "interpolating word context" framing). **drops** now shipped too:
  every `${var:OP}` op except `:-` rejects **SB4034** (one uniform code,
  generic-builtins message; sash-convert owns migration —
  `converter-var-modifier-migration`). The kept `:s/` sed op was
  disambiguated from the dropped `/`-glob by remapping the glob path to an
  internal `glob_replace` operator. 0 regressions native/janet; c +1
  (c:39.4). Remaining: remove now-dead backend dispatch (follow-up).
  **Also dropped the per-element array forms** `${arr[@]#p}`/`[@]/p/r`
  (were an asymmetry vs the scalar drop; DG chose consistency) → reject
  SB4034, migrated `types.sh` 33.16-33.18 to `for x in @a`+sub/gsub, which
  *fixed* them in c/janet (c 52→48, janet 74→71; native still 0). Runtime
  strip-fn cleanup filed (`remove-dead-strip-runtime`).

### Session 2026-07-04 (cont.) — language sweep DD#65-68 part 1

DG: "do the language sweep." Shipped (0 regressions, native clean):
- **dd67** verified already-done (name()/function/backticks/[literal).
- **dd68 brace control flow** (217b25d) — if/while/until/for `{ }`;
  parser-only, same IR; spec/brace.sh 25 native-only.
- **dd66 dispatch form** additive (292a046) —
  `[[ SUBJECT ==|=~ PAT { grp } … || { grp } ]]`; == exact / =~ regex;
  statement-only (SB4028); ir::Case gained `op`; native+c+janet;
  spec/dispatch.sh 22.

Remaining (filed): **dd65 var modifiers** (atomic drop+rebuild).
See journal 126.

### Session 2026-07-04 (cont.) — DD#66 remove `case` (sweep part 2)

DG: "continue — remove case first." Destructive half of DD#66 shipped
(05d2df0) spec-first, **0 regressions native/c/janet**:
- `case`/`esac` → reserved-word reject **SB4029**; `;&`/`;;&` tokens gone;
  `CaseItem.fallthrough`/`resume`, `=glob`, `sash_fnmatch`/`fnmatch_impl`
  all deleted (filename `sash_glob` kept). `case.merge`→`dispatch.merge`.
- control_flow.sh cf.c1-c12 migrated to dispatch (+cf.c13/c14 reject);
  janet cf.c6 flipped green → closes `bug-janet-case-glob` (dissolved).
- Converter: MANUAL dispatch annotation for `case` (subshell convention);
  full auto-rewrite is a DG decision → filed `converter-case-to-dispatch`.

Remaining sweep: **dd65 var modifiers**. See journal 127.

### Session 2026-07-04 (cont.) — DD#71 operand-words design (from dispatch follow-up)

Design session (no code yet). DG steered "how does a `=~` regex include a
space?" into **DD#71** (`notes/decisions/DD71-operand-words-value-bindings.md`,
accepted):
- **`=~` is sugar for `match()`; its RHS has no unique handling** — an
  ordinary *operand word* (absorb metachars to whitespace, quote/`$`-aware),
  same routine as `==`. Delete `parse_regex_rhs` + the `{` gap heuristic.
- **Absorb-mode scope** = value positions that aren't connectable commands:
  `[[ ]]` operands/patterns + assignment/declaration RHS. Command args stay
  operator-mode. **Assignments are value bindings, not commands** (no status,
  not `&&`/pipe/bg-able).
- Falls out: **multi-exact dispatch arms** (`== a b c`, parser-only —
  backends already do multi-pattern); both `=~` bugs fixed by construction.
- Dynamic regex **supported** (validate-static / runtime-abort-dynamic).
- Filed: `operand-word-absorb-mode` (impl), `reject-nonstatus-in-boolean-
  context` (the DD#70 dual — `x=hi && …` becomes a clean reject), plus two
  DG-caught bugs `bug-dispatch-literal-star-wildcard` (P1: `*` wrongly a
  wildcard) and `bug-echo-builtin-array-splat-ice` (P2: `echo @a` ICE).
  `bug-regex-rhs-space-quote` superseded by DD#71.

### Session 2026-07-04 (cont.) — DD#71 Cluster A implemented (spec-first)

DG: "Cluster A — DD#71 operand-words, spec-first." Shipped all five coupled
components, one commit each, **0 regressions native/c/janet** every time.
Native passing 595 → 635. See journal 128.
- **star-wildcard** (9424d66) — dispatch `*` is a literal; `is_default` flag
  replaces the `*` sentinel.
- **operand-word** (60c667b) — `=~` RHS is an operand word
  (parse_operand_word); deleted parse_regex_rhs. Fixes quoted spaces, the
  silent-`foobar` concat, and dynamic `=~ $pat`; comptime regex validation
  extended to any static operand shape.
- **multi-exact** (e9fc520) — `[[ $x == a b c { … } ]]` (parser-only).
- **boolean-reject** (1c2e74f) — SB4025: a value binding in a boolean context
  (if/while cond, left of &&/||, `!`) is a compile error. Fixes silent
  `x=hi && echo $x`.
- **assign-absorb** (000dd5a) — assignment RHS absorbs `& && || |` as
  literals (`a=foo|cat` → literal). AbsorbMode enum; `=(…)` unaffected.

Filed `bug-test-operand-concat-splice` (P3, pre-existing `run$suf`). Tasks
operand-word-absorb-mode / reject-nonstatus-in-boolean-context /
bug-dispatch-literal-star-wildcard → done. Remaining sweep: **dd65 var
modifiers** (vet-var-modifiers).



DG: "tackle the whole batch." Shipped the c/janet parity cluster spec-first,
one commit per change, full-suite diff each time. **Native 0-failing
throughout; zero regressions.** Pre-existing baselines: **c 58→52, janet
106→75.**
- janet pipeline EAGAIN flakiness → blocking `(os/pipe :WR)` (567b8fe).
- janet multi-pattern `case a|b` → match guard alternation (4b0e0f6, P1).
- janet argv trio DD#60 via `(dyn :args)` + fixed `for x in @a` splat +
  `join` (0f36cdf, −24 janet).
- `&&`/`||` branch on exit status (not value) in c+janet; every command
  sets the status var (fa9da7c).
- DD#55 (A) external-command abort in c+janet (bc65140, partial — external
  done, END-routing via `Emit.end_block`; remainder in
  `dd55-abort-channel-remaining`).
- `[[ =~ ]]` test operator implemented in both oracle backends (2b06810);
  `echo|read` guarded to native + filed (381512a).

Followups: `bug-janet-case-glob`, `dd55-abort-channel-remaining`,
`echo-read-inline-pipe-cjanet`. See journal 125.



DG ratified the disposition of out-of-range integer literals. Probing
found the bug was broader than the task: **six** silent `0` miscompiles.
Policy A (bash-faithful): a **decimal** literal whose magnitude exceeds
i64 → compile error (SB4026); **hex/octal** are 64-bit bit-patterns
(`u64`→`i64` reinterpret at scan time, so `0xFFFFFFFFFFFFFFFF` = -1);
**i64::MIN** is expressible in arith via a unary-neg peephole
(`-9223372036854775808`). Non-arith `int x=NNN` out-of-range also → SB4026
(validator). New `ATok::BadNum`; `arith_range_error`. spec/arithmetic.sh
section 26 (10 cases) native+c green, janet guards the i64 boundaries.
Native 0-failing (545); spec.db diff = zero regressions. Also resolved
two DD#25 follow-ups: left-shift = no trap (bits ≠ arithmetic, already
correct); const-fold overflow = runtime trap stands (compile-error not
"essentially free" — detection is discarded into a runtime op). Found +
filed a new bug: based literals (`16#ff`→16) drop their digits because
`#` is eaten as a comment (bug-arith-based-literal-hash-comment). See
journal 124.



DG-directed cleanup. Heredocs (`<<`/`<<-`) are dropped (lexer emits a
`__HEREDOC__` sentinel → validator SB5002 rejects, pointing at `<<<`). But
all the `*heredoc*`-named codegen/runtime/emit functions actually
implement **herestrings** (`<<<`, `RedirectOp::HereString`) — a misleading
holdover. Renamed across the 6 herestring files (incl. the C ABI symbol
`sash_heredoc_redirect` → `sash_herestring_redirect`, consistent across
sash-rt + native setup + emit_c). Left the genuine heredoc-rejection
machinery (`scan_heredoc_sentinel`, `check_heredoc`, `heredoc_diag`,
`__HEREDOC__`, SB5002) untouched. Pure rename: native 0-failing (535);
c=58/janet=106 byte-identical; herestrings verified on native + C,
heredoc still rejected.

### Session 2026-06-26 (unattended, round 3) — fix: `<<`/`<<=` mis-lexed as heredoc

Closed `bug-arith-left-shift-heredoc` (last arith *lexing* gap). `scan_lt`
tokenized `<<` as a heredoc sentinel regardless of context, so
`(( x << 2 ))` / `$(( … ))` / `<<=` dropped the operator (`>>` was fine).
Fix: `Scanner.arith_depth` (set by `collect_arith_content`); `scan_lt`
emits `Word("<<")` in arith mode → arith re-lexer recombines to Shl, and
the `<op>=` separator rule handles `<<=`. spec 31.1j-m; native 0-failing
(535); zero regressions; heredoc rejection / `>>` / `<=` intact. Whether
left-shift *overflow* should trap is left as a DG decision. Journal 123.

### Session 2026-06-26 (unattended, round 2) — DD#25 overflow sweep: Pow + Div

Continued the DD#25 overflow work. Probed the remaining overflow-capable
ops: **Pow** saturated silently (→ now traps), **Div** `MIN/-1` SIGFPE
core-dumped (→ now clean trap); **Neg** already traps (`0-x`→ssub_overflow)
and **Mod** `MIN%-1`=0 (srem, correct) — no fix needed. Fixes: `sash_pow`
calls `sash_arith_overflow`; native+C div guard `MIN/-1`. spec 25.9–25.12;
native 0-failing (531); c=58/janet=106 unchanged. Shl still blocked on the
`<<` heredoc bug. Filed `bug-arith-literal-out-of-range` (out-of-range
literal → 0). Journal 122.

### Session 2026-06-26 (unattended) — P0 bug: compound-assigns other than += were no-ops

Found while verifying the overflow fix. `(( x OP= N ))` for OP ∈
{−,*,/,%,&,|,^} was a **no-op** (only `+=` worked) — pre-existing,
spec-uncovered. Root cause: the shell lexer splits `-=`/etc. into `- =`
(no arith context; `+=` is a single `PlusAssign` token), and
`needs_separator_space` inserted a space that the arith byte-lexer then
mis-read as `x - 0` (a comparison, no write-back). The `<=`/`>=`
special-case existed but didn't cover the compound-assign operator chars.
Fix: extend it to `- * / % & | ^` (parse/arith.rs). spec 31.1d–31.1i;
native 0-failing; zero regressions. `<<=`/`<<` left-shift still broken
(heredoc-vs-shift, `bug-arith-left-shift-heredoc`). Journal 121.

### Session 2026-06-26 (unattended) — P0 bug: runtime overflow silently wrapped

Started `abort-diagnostics-remaining`; its DD#25 overflow gate exposed a
**stop-everything P0**: runtime integer overflow (+, -, *) **silently
wrapped** (cranelift plain `iadd`/`imul`; C plain ops; const-fold
`wrapping_*`), violating DD#25 ("traps at runtime"). Spec 25.2/25.3 only
"passed" by accident — overflow → exactly i64::MIN → `itoa` panicked on
`checked_neg`.

Fixed (spec-first, native + C; janet bignum out of scope):
- `sash_arith_overflow()` → `<argv0>: arithmetic overflow`, exit 2.
- native `emit_checked_arith` (sadd/ssub/smul_overflow); C
  `__builtin_*_overflow`; const-fold `fold_arith_op` → `Option` (defers
  overflow to the runtime trap, decision-free).
- `itoa_inner`/`write_i64`/`itoa_buf` use `unsigned_abs()` — i64::MIN
  echoes correctly.
- spec/arithmetic.sh 25.4-25.8. native **0 failing**; c=58/janet=106
  unchanged (zero regressions). DD#25 note corrected; journal 120.

Follow-up (filed in `bug-runtime-arith-overflow-wraps`): Pow saturates,
Shl/Neg/Div overflow-capable; const-fold trap-vs-compile-error is a DG
sub-decision. The other 5 `abort-diagnostics-remaining` gates remain open.

### Session 2026-06-25 (cont.) — DD#63 captures (out-array + backrefs)

Both capture surfaces, all three backends, after DG resolved D1–D6.

- **`match S P ARR`** out-array: bash-parity length (engine `Reprog.nsub`),
  non-participating group → `""`, clear-on-entry, still (B) must-consume.
  New `SsaInst::MatchInto` + `sash_regex_match_into` + `sash_table_clear`.
- **`\0`–`\9` backrefs** in `sub`/`gsub` (`sash_regex_replace` template
  pass); invalid `\`-escape → **SB4033** (validate, all-modes).
- **Taint airtight (D4):** tainted subject rejected at the read site
  (SB5020) — captures always clean, no DD#61 change.
- **Engine:** `Reprog.nsub` exposes the group count. `build.rs` now tracks
  csrc/headers via `rerun-if-changed` (stale-object gotcha fixed).
- **Backends:** native (codegen) + C (`emit_match_into_c`, shared runtime)
  + janet (`sash-regex captures` CLI op + `string/split`; fixed a
  pre-existing `$(len ARR)` janet gap, +1 janet_test).
- Spec: regex.sh 63.cap1–12. Native **28/28, 0 failing** (gate); c & janet
  24 + 4 native-only skips; zero regressions (c=58/janet=106 baseline).
- **Finding:** the `sash-rt-tests` harness is pre-existing broken
  (references removed `sash_streq`/`sash_arith_eval`; not on `mk test`).
  Cargo core suites have ~122 pre-existing stale-test failures (old
  `declare`/`${#arr[@]}` syntax) — not regressions.

### Session 2026-06-25 (cont.) — DD#70 (A)/(B) write-up + match boolean

- **DD#70 written** (`notes/decisions/DD70-error-taxonomy-A-B.md`):
  (A) runtime abort-if-unchecked vs (B) compile-time must-consume,
  refining DD#55. DG resolved: booleans must be consumed (compiler-
  enforced); ignored-result codes adjacent (SB4018 discarded boolean,
  SB4019 `-` on a (B) construct, next to SB4016/SB4017); verbatim argv[0].
  Contract-spec task `dd70-error-model-contract-spec.md`.
- **`match` boolean command (DD#63 1b-bool)** shipped on all three
  backends (commit c03b933): `match S P` → exit 0/1, sets the if/&&/||
  boolean. native (sash_regex_match), C (sash_regex_match), janet
  (sash-regex `matchq` — quiet, no stdout leak). Bare `match` → SB4018,
  `- match` → SB4019 (native; via a `suppressed` counter +
  check_match_must_consume mirroring SB4016). Fixed a janet `:` (Noop)
  gap. regex.sh 63.1c-2f.
- **Finding:** transform diagnostics (the whole SB40xx must-consume
  family incl. the existing SB4016) are native-only — `--emit-c`/`--janet`
  don't surface them. Must-consume compile-error specs are `in_modes
  native` until that's fixed.

### Session 2026-06-25 — DD#55 W3: syscall aborts + stderr diagnostics

- **cd / echo / redirect-open now participate in the DD#55 (A)-channel**
  (native; commits `c232b75`, `313843a`). Three bugs fixed: `cd` never
  set test_result/aborted (`if cd /tmp` took else; failed cd continued);
  `echo` ignored `write(2)` failures ("echo always succeeds"); a failed
  redirect `open(2)` **silently misrouted** output to fd 1.
- **Redirect model (DG):** `> file` is the implicit `exec N>file` that
  runs FIRST; a failed open makes the command fail without writing (no
  misroute), aborting unless in a condition (if/||). `-` binds to the
  command, not the open, so `- echo > /bad` aborts. `in_condition` derived
  at the emit site from the Branch terminator — **no new IR bit**. Fixed
  for echo and external commands.
- **Descriptive diagnostics (DG: all abort cases):** every abort prints
  `<argv0>: <context>: <reason>` to stderr, prefixed with the program name
  (argv[0]). `<reason>` = OS strerror via a hand-rolled no_std errno table
  (`sash_emsg`, `sash_last_errno`, `errno_str`). Fires only on the actual
  abort; caught/`-`-suppressed stay silent. This pass = cd/redirect/echo
  (errno cases).
- **Error model ratified:** (A) runtime abort-if-unchecked for real
  failures (`-` suppresses the verdict); (B) compile-time must-consume for
  pure logical results (`match`/`[[ ]]`/`-> int`; SB4016 already enforces).
- **Harness:** native runner now captures runtime stderr into `rerr`.
  Spec `errors.sh` 55.20-30 (native only). Native 0 failing; c=58/janet=106.
- **Deferred (filed):** `abort-diagnostics-remaining.md` (external/pipeline/
  cmdsub/bg/overflow messages), `c-janet-dd55-abort-channel.md` (C/janet
  lack the whole (A)-channel — un-guard 55.20-30 cross-mode after).

### Session 2026-06-21 (cont.) — dd67 sugar, DD#23 resolution, native green

- **Native spec suite is GREEN (0 failing).** cf.c7/cf.c8 rewritten to the
  **star-only** glob contract (DG, DD#39): `?` and `[class]` are literal
  characters in `case` patterns, not metacharacters — only `*` globs.
- **DD#23 resolution hoisted ahead of the front-end** (`d4fdc63`): new
  `sash/src/resolve.rs` rewrites external command heads (incl. cmdsub heads)
  to absolute paths before taint/type checks, so the CST taint pass sees the
  real exec form. Unresolved = fatal (SB5011, now incl. cmdsub heads).
  `--sysroot` dir + `.squashfs` honored. Structural rejections (heredoc,
  export/declare) now run before resolution.
- **dd67 sugar sweep**: `name() { }` (`ee12efd`), flat backticks (`feb244e`),
  single-bracket `[` is an ordinary literal (`45648a9`).
- **Rust suites (sash/tests/) have significant bit-rot** (DG) — they are NO
  LONGER a regression gate. **spec.db is the source of truth.** Filed
  `audit-rust-test-suites`. Their nondeterminism: codegen tests call
  `compile_to_object` in-process, sharing global compiler state.
- **Heredoc removal is INCOMPLETE**: lexer rejects `<<` (→ `__HEREDOC__` →
  SB5002), but `RedirectOp::HereDoc` + its codegen/emit_c/emit_janet/validate
  handling remain as dead code. Filed `finish-heredoc-removal`.


### Session 2026-06-21 (cont.) — DD decision review

Walked every candidate DD with DG; **all now resolved** (DD#57 implemented;
DD#62–69 accepted). Commits `b7ed1a4`…`776aec4`. Highlights:

- **DD#64 addendum** (new): `$'…'` is recognized **inside `"…"`** as an
  escape-aware, compile-time-constant expansion — every `$`-form now
  expands in `"…"`. A C-string is a universal literal-byte encoder, giving
  full literal control inside an interpolated string (even a literal `"`),
  no concatenation. Resolves the DD#63/#65 default-vs-replacement context
  uniformly. Task `cstring-in-double-quotes`.
- **DD#62**: undeclared scalar read → compile error under **strict
  definite-assignment** (`${x:-}` is the opt-out). Task
  `reject-undeclared-scalar-read`.
- **DD#63**: `\`+non-digit in a `sub`/`gsub` replacement is an error
  (`Resub` stride = 16B); replacement/`:-default` are one word context
  (replacement adds only the `\0`–`\9` pass); `$(cmd)` allowed (only
  external captures taint).
- **DD#65**: prune accepted; `$(cmd)` allowed in `:-default`; `:?` is
  scalar-only (`@{a:?W}` splat is DD#69); `${#var}` kept, `@X` rejected.
- **DD#66**: `[[ ]]` two-form + `case` removal; **numeric dispatch dropped**
  (==/=~ only). **DD#67**: re-add `name()`/`function name`; flat backticks
  kept (nested rejected); canonicalize switch in `sash-convert`; sweep →
  keep/drop table DG ratifies first. **DD#68**: brace control flow,
  converter default preserves `then/do`. **DD#69**: ratified.

Doc fixes (`199b098`, `2373317`): corrected stale tree-sitter/CST docs
(hand-parser is AST-based; `emit_source` coercion path is dormant →
`.sash_source` is verbatim). Tasks filed:
`excise-dormant-coercion-machinery`, `cstring-in-double-quotes`,
`reject-undeclared-scalar-read`.

**Still-pending non-DD decisions:** array-splat native runtime-argv ABI
(B1/B2), `bug-janet-int-overflow` approach, `tilde-vs-dd53`.

### Session 2026-06-21 — autonomous backlog sweep (cont.)

Five tasks landed (journal 115), all multi-backend verified with zero net
regression (native cf.c7/c8 only; c/janet diffed per-mode; complexity
gates green):

- **bug-dollar-hash-in-arith** (`55a2d46`): `$#`/`$?`/positionals now read
  inside `(( ))` (lexer `scan_arith_dollar` scans the special single-char
  names; DD#46 enforced in arith via `check_positionals`). Fixed a
  pre-existing janet double-paren arith-value crash (repaired 6 specs).
- **bug-janet-arith-condition** (`412db73`): janet `(( ))` conditions were
  constant-`true`. Now `emit_condition_inline` handles `ArithTest`;
  comparisons/logicals yield bash 1/0 ints; int counters stay numeric;
  string `[[ ]]` tests stringify int operands. janet −8 (cf.w1–w7, cf.i3).
- **nonbrace-dynamic-subscript** (`b0c189d`): `$a[$i]` reads `${a[$i]}`
  (decision B) via `try_split_subscript_arm` (peek_n lookahead).
- **bug-c-janet-concat-in-value** (`13f03af`): c `lower_value_as_str`
  gained a `Concat` arm (`emit_concat_value` via runtime `sash_concat`).
- **bug-janet-indexed-assignment** (`51d5ee4`): janet `a[i]=v`/`a[i]+=v`
  now `(put …)` with collection-typed index (`EmitCtx::map_vars`).
  types.sh 33.11–33.13 fully un-guarded (all modes).

Filed for review: **array-splat-command-arg** — investigated (feasible
via `sash_alloc`+table iteration); held for DG on the native runtime-argv
ABI choice (B1 codegen-side vs B2 runtime-side). Filed bugs:
`bug-janet-int-overflow`, `bug-janet-array-dynamic-subscript-read`.

### Session 2026-06-21 (cont.) — DD implementation phase (journal 116)

Implemented the resolved DDs in dependency order; all multi-backend
verified, native baseline held (cf.c7/cf.c8 only), rust 184=184 (oracle
order-flakiness only). Commits:

- **cstring-in-double-quotes** (`e4efef0`): DD#64 addendum — `$'…'`
  recognized inside `"…"` (lexer `scan_dq_cstring`). Specs wordsplit
  64.4–64.8 (all backends).
- **array-splat-command-arg** (`8245a14`): `@a` expands to one argv entry
  per element (DD#20/#22), all backends. A1 `SsaValue::ArraySplat` (taint
  inherits from the array); B1 native `build_argv_values` (unifies exec/
  pipeline/cmdsub; runtime `sash_alloc` + Cranelift fill-loops); C VLA;
  janet `;`-splice. Specs wordsplit 22.14–22.19. **Folded in a spec-
  hygiene fix**: wordsplit had a misplaced mid-file `tally` (its
  "Additional coverage" ran only in native) → moved to EOF; surfaced two
  pre-existing c/janet gaps (39.10 `\w`, 22.12 echo|read, filed).
- **glob-splat Part 1** (`076e8a2`, `c997193`): DD#69 globs-are-arrays.
  1.2 — inline `*` in command-arg position is SB5029 (use `array a=(*)`).
  1.1 — `for f in *` routes through glob→array→per-element
  (`try_dispatch_glob`); **fixed a C-backend glob SIGSEGV** (`emit_glob_c`;
  emit_c had a glob TODO → null table). Specs wordsplit 20.1–20.6
  (for-in glob native/c; janet doesn't glob).

**Decisions now needed from DG (filed):**
- `DECISION-glob-splat-disposition-word` — glob-splat **Part 2**
  (`@{a:-WORD}`/`@{a:?WORD}` disposition + bare-`@a`-to-external reject):
  WORD-interpolation semantics, `:?` abort format, taint compose. Blocks
  the rest of glob-splat.
- Remaining ready tasks are each design-gated per their files:
  `reject-undeclared` (typed-zero-init set + pre-SSA-vs-SSA placement),
  `vet-var-modifiers` (basename/dirname builtin-vs-external; couples to
  regex-builtins for sub/gsub), `dd67-sugar-sweep` (keep/drop table needs
  DG ratification *before* sugar), `regex-builtins`/`dd66`/`dd68`.

Other tasks filed this session: `bug-cmdsubst-builtin-not-resolved`
(`$(printf …)` passes a bare name to sash_exec — pre-existing),
`janet-pipeline-eagain-flaky`, `cjanet-wordsplit-39-10-22-12`,
`spec-wordsplit-count-desync` (done).



Worked the ready backlog in dependency order (journal 115). Landed:

- **for-loop-arith-condition** (`f06866f`): for-in bound checks moved off
  the synthesized `[[ -lt ]]` test onto the `(( ))` arith path
  (`emit_for_bound`: `ArithBinOp` + `LetExit`); `emit_let_exit`
  generalized to any `SsaValue`; the `[[ ]]` numeric arm deleted in both
  backends (`[[ ]]` now carries zero integer machinery); direct `arrlen`
  → `sash_table_len` arm. Fixed a latent catching-construct bug
  (`for_arr.exit`/`for_at.exit` added to `is_catching_construct_exit`).
- **Real bug fixed:** `(( <= ))`/`(( >= ))` were silently wrong (the bash
  lexer splits `<=` into `<`+`=`; `collect_arith_content` inserted a
  separator space). Fixed in `arith.rs::needs_separator_space`. Spec
  arithmetic.sh 31.15–31.18.
- **subscript-append-broken** (`da9b17b`): `a[i]+=v` now read-modify-write
  (`Concat([${a[i]}, rhs])`). native; spec types.sh 33.11–33.13.
- **bug-until-arith-condition** (`081d122`): `until` desugar appends the
  `NegateTestResult` sigil instead of patching `SsaInst::Test`, so
  `until (( ))` / `until <cmd>` are negated (and compound conditions
  negate once). 3 until codegen_tests re-enabled.

Filed bugs (pre-existing, surfaced here): `bug-dollar-hash-in-arith`
(`$#` not read in `(( ))`), `bug-c-janet-concat-in-value`,
`bug-janet-arith-condition` (janet `(( ))` cond always-true). Several
ignored c/janet cases are guarded `in_modes native`.

All commits: native baseline (cf.c7/c8), c/janet no new failures, rust
no net change, gates green. **Next ready (large/design):**
`array-splat-command-arg` (runtime-variadic argv → unblocks DD#69 glob
work), the c/janet concat/janet-arith backend gaps, and the `$#`/DD#46
and non-brace-subscript design questions.


### Session 2026-06-20 — DD#62 section A: `[[ ]]` operands → `SsaValue`

The operand→`SsaValue` refactor (the last piece of DD#62). `SsaInst::Test`
/ `Bless` / `BlessAll` now carry `SsaTest`/`SsaTestRhs` whose value
operands are pre-lowered `SsaValue` (only `=~`/`=glob` patterns stay raw
text). `test_to_ssa` does the lowering in transform; codegen `tests.rs`
+ `emit_c.rs` consume it. This routes `[[ ]]` subscripts through the SSA
value path (`subscript_to_value` → `lower_subscript_value`, which knows
`assoc_arrays`), **fixing `[[ ${m[$k]} ]]`** (map dynamic key did an int
lookup) in native **and** C. Deleted the duplicate IR-`Expr` test-lowering
subtree (`lower_subscript` & ~9 siblings, `find_var_slot`/`find_var_key`,
`next_slot_version`, emit_c's `ir_expr_as_int`/`latest_var_as_int`) —
~330 LOC. Closes `dynamic-map-subscript-in-test`. Specs `types.sh`
62.11–62.15. See [journal 114](journal/114-dd62-section-a.md).

Three refactor-introduced bugs caught + fixed before commit (none
shipped): (1) synthetic for-in `[[ __ai -lt ${arr arrlen} ]]` bound
checks ICE'd after I dropped codegen's numeric arm (SB5019 only rejects
*user* numeric ops) — restored it both backends; (2) scalar
`bless x [[ … $x … ]]` resolved `$x` to the not-yet-populated target —
`test_to_ssa` now runs before `state.define`; (3) bless-all binders
mis-versioned (`x_0` vs codegen's `x_4`) — `BlessAll.binders` is now
`Vec<VarRef>` with transform-assigned unique versions.

Zero net regression: native back to baseline (cf.c7/c8 only); c/janet no
new failures (both improved); rust 186 (flaky `oracle_pipeline_grep`
only). Next in the DD#62→68 implementation order: **DD#63** (regex
builtins; absorbs DD#62 C/D).



### Session 2026-06-19 (cont.) — &&/|| left-associativity fix (DD#62 F)

`A && B || C` parsed **right-associative** (`A && (B || C)`), so
`false && echo A || echo B` ran neither branch. Root cause:
`parser.rs::binary_list_op` parsed the RHS via the full `parse_statement`
(recursing into more list ops). Fixed: `parse_trailing_list_op` is now a
left-associative loop over single operands (`parse_list_operand` =
command + pipeline/redirects, no trailing list op); `binary_list_op`
removed. Compound operands still rejected (SB4023). Native verified across
mixed/deep chains. Filed `bug-andor-right-associative` for the analysis.

Surfaced (and filed) a **pre-existing** janet/c backend bug
(`bug-andor-backend-chain-status`): those backends evaluate chains by
language value-truthiness, not exit status (e.g. `print`→nil makes `||`
mis-fire), and don't set the status after every command. Native (SSA
`lower_list_stmt`) is correct. The nested-chain spec tests (wordsplit
62.8–62.10) are guarded `in_modes native`; 62.7 runs all modes.

Zero net regression: native 417/2 (cf.c7/c8), janet 125, c 62, rust 186
(flaky oracle only).

### Session 2026-06-19 (cont.) — DD#62 sub-unit 1 + janet printf fix

**DD#62 sub-unit 1** (of several): `==`/`!=` are now **exact-only** in
native codegen (`emit_eq`/`emit_ne` drop the `fnmatch` glob branch — the c
backend was already exact via `sash_strcmp`, so this aligns native↔c);
`emit_regex` is always regex (dead quoted-literal branch removed); and
`right_is_quoted` is **deleted** (zero callers). Spec `wordsplit.sh`
62.1–62.4. `case`'s `=glob`/`fnmatch` is untouched (its own op). Remaining
DD#62: operand→`SsaValue` + delete `lower_subscript`, remove `-v`, `=~`
quoting/spaces, the `&&/||` chain.

Also fixed a **janet printf regression** the DD#64 escape fix introduced
(`printf '%s\n'` got a literal `\n` once literals were escaped): the
format string now has printf escapes interpreted before Janet-escaping
(`interpret_printf_escapes`). Caught by `oracle_general_command`.

Verified zero net regression: native 417/2 (cf.c7/c8), janet 125, c 62,
rust 186 (only flaky `oracle_*`). Next DD#62 sub-units, then DD#63.

### Session 2026-06-19 (cont.) — DD#64 implemented (no shell backslash in "…")

First implementation phase of the DD#62–68 arc. **DD#64 done.** The only
real change was `lexer.rs::scan_dq_escape`: inside `"…"`, `\` is now a
verbatim byte (only `\<newline>` continuation survives) — `"a\\b"`→`a\\b`,
`"v\$x"`→`v\`+interp, `"a\"…`→`a\`+close. Bare words and regex-in-a-word
were already verbatim (no change). `\}`/`\/` stay deferred to DD#63/65.

Exposed + fixed a **janet backend bug**: `emit_expr_inline` emitted
`Literal`/`SingleQuoted` raw, so backslashes/quotes weren't escaped for
Janet string syntax (`"a\\b"` → 1 backslash at runtime). Now wrapped in
`escape_for_janet_string`.

Spec: `spec/wordsplit.sh` 64.1–64.3 (moved before the mid-file `tally`;
note the file has uncounted "bonus" tests after `tally`). **Zero net
regression**, verified against a clean stash-baseline: native 417/2 (only
cf.c7/c8), janet 125 (baseline), c 62 (baseline), rust 186 (only flaky
`oracle_pipeline`). Next: DD#62.

### Session 2026-06-19 — DD#68 brace-block control flow + sweep task

DG green-lit brace `{ }` bodies for `if`/`while`/`until`/`for` (matching
`fn name { }` and DD#66 dispatch), with `then/fi`/`do/done` kept as DD#67
1-to-1 sugar. **Parser-only** — both forms build the existing
`If`/`While`/`For` IR, so `$?`-catching, `break`/`continue`, redirects are
unchanged.

The `if`-condition ambiguity was resolved: **the condition is a command
list ending at the first top-level `{` (the body); a `{ }` group may not be
a condition.** Unambiguous because sash already (1) reserves `{` (`echo {`,
`echo a{` are SB4014; literal is `'{'`), (2) has no `{a,b}` brace-
expansion, (3) bans dispatch-`[[ ]]` as a condition (DD#66 b). Group
conditions cost nothing to drop — `if { a; b; }; then c` ≡ `if a; b { c }`.
`else if` accepted (= `else` + nested `if`); no mixing `then`↔`{` in one
construct.

Captured: **DD#68** (`brace-control-flow`, candidate) + spec-first task
`dd68-brace-control-flow`. Also filed `dd67-bash-sugar-sweep` (DG-approved):
re-add `name() { }` as 1-to-1 sugar for `fn name { }`, add the convert/
canonicalize switch (default preserves, `--native` normalizes), and sweep
the rejected bash forms (backticks, `[ ]`, `let`, …) against the DD#67
test. No compiler code yet.

### Session 2026-06-19 (cont.) — DD#67 native-syntax / 1-to-1 sugar policy

DG set an adoption-hedging **syntax policy**: a bash form that is an
**unambiguous 1-to-1 thin sugar** over a native sash form is **kept** (both
accepted); the **default preserves** the user's familiar form, and a
**convert switch** normalizes to native-only. Forms that can't map 1-to-1
(need semantic translation or are ambiguous) are **dropped**, and the
converter rewrites them.

The 1-to-1 test resolves the open `then/fi/do/done` question (**keep** —
pure bracket alias over `{ }`, DD#68) and confirms the `case…esac` /
`;&`/`;;&` **drops** (DD#66 — glob≠exact/regex isn't 1-to-1). It also
surfaces re-add candidates the policy invites reconsidering: `name() { }`
(currently only `fn name { }` is accepted) and backticks `` `…` `` for
`$(…)`. Semantic breaks (DD#64 no-backslash, DD#62 `==` exact) are NOT
sugar candidates — they change meaning, not spelling.

Captured as **DD#67** (`native-syntax-sugar-policy`, candidate). Default =
preserve (sugar round-trips through `--emit-source`); convert switch =
native-only (build on `--emit-source`/`sash-convert`). Next: DD#68
brace-block control flow applies it (`if/while/until/for` brace bodies,
`then/fi/do/done` as sugar). Open: `name()` re-add, backticks, flag name,
a rejection sweep. No compiler code yet.

### Session 2026-06-19 — DD#66: `[[ ]]` dispatch form replaces `case`

The `case` redesign converged. Instead of a new `switch` keyword (brace-
overload) or regularized `case`, **`[[ ]]` becomes sash's single match
construct in two forms** (DG's idea):
- **boolean test** `[[ $x == foo ]]` — yields `$?` (unchanged).
- **dispatch statement** `[[ $x == foo { … } bar { … } || { … } ]]` —
  runs one matching group. `{` after the first pattern selects dispatch.

Decisions (DD#66): `==` is **strict exact equality** in both forms (no
alternation — that would make the boolean form a degenerate glob); `=~`
is **unanchored** regex (alternation = `^(a|b)$`, not `^a|b$`); first-
match-wins, **no fall-through**; `|| { }` is the optional default (`||`
can't collide with a pattern; literal is `'||'`). The dispatch form is a
**catching construct** — `$?` resets to 0 after (the existing
`is_catching_construct_exit` machinery, `codegen/entry.rs:263`), so there
is no ambiguity over "which group set `$?`"; the boolean form yields the
test result. This is DG's clean statement-vs-test separation.

**Removed:** `case`/`esac` (keyword → reserved-word diagnostic), `;&`/`;;&`
(`SemiAmp`/`DblSemiAmp`, `CaseItem.fallthrough/resume`), the `=glob` op +
`emit_glob` + **`fnmatch_impl`** (case's sole user). **Kept:** filename
glob `sash_glob` (separate fn — glob stays for *filenames*, its intent).
Lowering reuses `ir::Case`/`build_dispatch_chain` with the op carried from
the `[[ ]]` operator (`==`/`=~` instead of `=glob`). Converter rewrites
bash `case` → dispatch.

Captured in **DD#66** (`cond-form-dispatch`, candidate) + task
`dd66-cond-form-dispatch` (spec-first). Supersedes the case-as-regex idea,
the fix-(a) glob note, and DD#63's open case-desugar; subsumes the Janet
multi-pattern bug. No compiler code yet.

### Session 2026-06-18 (cont.) — `case` redesign direction + 2 Janet case bugs

Design discussion (DG-driven) on `case`. Confirmed `case` is **literally
sugar for an if-chain**: `build_dispatch_chain` emits, per arm, a
`TestExpr value =glob pattern` → `Branch{body, next}`; `*` = uncond goto;
`a|b` = OR-chain; `;&`/`;;&` add fall-through/resume edges. `=glob` →
`sash_fnmatch` (whole-string, `*`-only) is **case's sole user** of
`fnmatch_impl` (after DD#62 removes `[[ ]]` glob). Filename globbing is a
*separate* fn (`sash_glob`, getdents) used by `arr=(*.sh)` — it survives.

**Direction (DG):**
- `case` becomes **exact-match** dispatch — not glob, not regex. Glob was
  only ever meant for filenames (`*`/`*.ext`). This dissolves the P0
  glob-pattern bug (`bug-case-pattern-glob-not-validated`) and supersedes
  the fix-(a) glob-rejection note + the case-as-regex idea.
- **Abandon `;&` and `;;&`** (fall-through/resume): untested, and the
  Janet/C backends never honoured them. Removing them simplifies
  `lower_case` (drops `resume` check-blocks, fall-through wiring) and the
  `SemiAmp`/`DblSemiAmp` tokens + `CaseItem.fallthrough/resume`.
- Envisioned core: a clean **(key → command-group) pairs** primitive
  (exact, first-match, no fall-through), with bash `case…esac` (literal +
  `|` + `*` + `;;` subset) as **sugar** over it. Glob/regex patterns and
  `;&`/`;;&` are dropped, not sugar.

**Open (DG to settle):** the clean-core surface syntax (new keyword
`match`/`switch`/`when` + brace groups vs. regularized `case…esac`);
keep bash `case` as sugar vs. clean break (converter rewrites); default
token (`*`/`else`/`default`); multi-key spelling; literal-only vs
interpolated keys. **No DD yet** — write once syntax is locked.

**Bugs found:** (1) Janet miscompiles multi-pattern arms `a|b)` → `_`
catch-all → filed `bug-janet-case-multipattern` (P1; affects arms we're
keeping). (2) Janet/C ignore `;&`/`;;&` — mooted by the abandonment.

### Session 2026-06-18 — Path B (`/`-quote after `:s`) + `${var:-}` is a word

Empirically re-checked the brace-`}` cases. Two findings: (1) the
`${var:OP}` lexer **does** look inside `${}` — `scan_brace_expansion`
brace-counts and skips `'…'`/`"…"`/`\…`, which is why `${foo:-'a}b'}`
finds the right close but `${var:s/a}b/z/}` silently miscompiles to
`a}bb/z/}` (the bare regex `}` ends the count). (2) `:-` defaults are
taken **verbatim** today — quotes leak (`${foo:-'abc'}` → `'abc'`), no
interpolation (`${foo:-$bar}` → `$bar`).

**DG decisions captured:**
- **Path B** (DD#63): the lexer treats `/` as a quote-delimiter after
  `:s`, so a bare `{`/`}` inside RE/SUB is literal — `${var:s/a}b/z/}`
  becomes a *valid* substitution. A malformed `:s` payload is a **compile
  error**; literal text after a valid `}` still concatenates. This +
  DD#65 dropping the `/`-glob op makes **both** in-operand backslash
  handlers removable (`\}` lexer.rs:660, `\/` find_unescaped_slash) —
  recorded in DD#64 (deferred → resolved).
- **`${var:-default}` is a word** (DD#65): interpolation + concat +
  quoting; unquoted `}` closes (single-quote to include), command-subs
  rejected, arith allowed.

Spec tests specified (TDD, spec-first): `regex-interpolation-delimiters`
§G (G1–G7: brace handling + malformed→error) and `vet-var-modifiers`
(V1–V11: `:-`-as-word incl. the `${foo:-'a}b'}` regression, + drop
diagnostics). Open for DG: malformed-`:s` diagnostic code; whether the
trailing `/` in `s/RE/SUB/` is mandatory. Notes-only; no code yet.

### Session 2026-06-17 — [[ ]] design discussion → DD#62 + refactor task

Design discussion (no code) auditing `[[ ]]` operator semantics and the
duplicate test-path lowering. Captured as **DD#62**
(`test-expr-simplification`, candidate) + task `dd62-test-expr-ssavalue`.

**Decisions (DD#62):** (1) `==`/`!=`/`=` are exact-only — drop the
unquoted-RHS glob holdover; pattern-match via `=~`. (2) `=~` quoting is
grouping, not literalization (the bash quoted-regex-is-literal branch is
already dead/broken — quoted regex doesn't even parse, SB4014). (3)
Remove `-v` — sash has no unset state (undeclared scalar = zero value,
`unset` scalar rejected), so it's a misnamed alias for `-n` and broken
for arrays.

**Architectural payoff:** removing those eliminates `right_is_quoted`,
the only syntactic property of a *value* operand `SsaValue` can't carry.
Then `[[ ]]` value operands lower to `SsaValue`, subscripts use the SSA
path, and the duplicate `lower_subscript` is deleted — closing
`dynamic-map-subscript-in-test` (now superseded) by construction.

**Misses logged for the task's spec-first plan:** quoted `=~` is SB4014;
no way to put a space in a regex (quoted/unquoted/`[[:space:]]`/`\ ` all
broken); `a[[:space:]]b` mis-terminates at the internal `]]`;
`${var:s/re/repl/}` corrupts on a `/` in the regex/replacement (naive
`find`/`rfind`, no `\/`); `[[ 1 = 2 ]] && X || Y` drops `|| Y`.

**Open (DD#62):** the `\` meaning (engine-escape vs literalizer), `/`
delimiter escaping in `:s/`, and whether undeclared scalar reads should
be a compile error.

### Session 2026-06-17 (cont.) — DD#63 regex quoting/interpolation

Walking the regex-quoting design surfaced that **field interpolation is
unimplemented on every regex surface** — `$var` in a pattern/replacement
is taken literally (`[[ $x =~ $pat ]]` doesn't interpolate; `${pat}` →
SB4014; `${v:s/$x/Y/}` no-ops; mutation `v =~ s/$x/Y/` → `sash: bug:`).
DD#39's "dynamic patterns compiled at runtime" was aspirational; DD#39 is
also stale on `{n,m}` (counted repetition now works).

**DD#63 reframed to a builtin family** (`match`/`sub`/`gsub`) with the
three syntaxes (`[[ =~ ]]`, `s/re/sub/`, `${var:s/…}`) as **thin sugar**
that desugars to them. Because args are ordinary strings, quoting/
interpolation = normal argument evaluation — the bespoke regex-field
parser (and its open `\`/`/` questions) disappears. Decisions:
- `match SUBJ PAT` (boolean exit); `$(match …)` = whole match (`resub[0]`);
  `match ARR SUBJ PAT` fills `ARR=[whole,g1..g9]` (mapfile-style out-array).
- `sub`/`gsub` (awk) — `g` = the function choice, no flag arg. Engine has
  **no flags at all** (regcomp9 takes only a pattern; no ICASE) — case-
  insensitive unsupported, punted.
- Replacement backrefs `\0` (whole) / `\1..\9` (groups) — template subst
  from `resub`, *not* engine pattern-backrefs (Plan 9 still lacks those).
- `\xnn` (2 hex) in regcomp; `\x`+non-hex = error.
- Thin `s///` sugar: `/`-delimited, always interpolating; anything else →
  `$(sub|gsub …)`. `=~` RHS is just an argument (no special parsing).
- Literal containment → `fgrep`. The engine already captures submatches
  (10-slot `Resub`); `sash_regex_match` discards them today.

**Synergy:** `=~`-as-`match`-command means `=~` leaves the `[[ ]]`
TestExpr, simplifying the DD#62 value-operand→SsaValue refactor. Captured
in DD#63 + task `regex-interpolation-delimiters` (rewritten).

**DD#60 #1 (argv snapshot → out-param):** surfacing the array-last
out-param pattern (mapfile/read -a/`match S P arr`) prompted reworking the
argv snapshot from `array a=$(argv N)` (the lone command-sub→array case)
to `argv [N] a` (array last). Deletes the `ArgvSnapshot` command-sub→array
machinery; splat `$(argv)` for-in and scalars `$(arg N)`/`$(argc)`
unchanged. DD#60 revision note + task `argv-snapshot-to-outparam`.

**DD#64 (no shell backslash):** the backslash thread resolved to "shell
reads `\` only in `$'…'` and at EOL; verbatim everywhere else; escape via
quoting + concatenation (`'"'`, `'$'`); `\xNN` for literal regex bytes."
Removes the `\\`→`\` collapse / `"\\\\"` wart, `a\ b`≠`'a b'`, `\/`-broken,
dropped-standalone-`\`. Backslash *inside* `${var:OP}` operands deferred to
the var-modifier vet.

**DD#65 (prune `${var:OP}`):** vetted the family. Keep `:-`, `:off:len`,
`:s/`. Drop `:=` `:+` `:?` (broken/redundant), `#`/`##`/`%`/`%%` strip
(use `basename`/`dirname` — the community idiom), `/`-family glob-replace
(redundant with `sub`/`gsub`, glob holdover, only var-op using `\`), and
`^`/`,` first-char case. Move `^^`/`,,` → **`toupper`/`tolower` builtins**
(awk names, joining `match`/`sub`/`gsub`; gives case-insensitive matching
the engine's missing `i` flag can't). Open: `basename`/`dirname` builtin
vs external. Task `vet-var-modifiers` → DD#65.

### Session 2026-06-17 — map iteration: dynamic subscript + flat-literal + keys taint

DG: "fix the map iteration bugs." The filed `map-iteration-builtins-broken`
diagnosis (keys garbage / values ICE / keys escape) was **wrong**;
discussion with DG corrected it before coding. See
[journal 113](journal/113-map-iteration.md).

**Three real bugs, all fixed:**
- **(A) Flat map literal `map m=(a 1 b 2)`** was silently stored as a
  4-element indexed array. DG chose **reject** → new **SB4031** (DD#33),
  `validate.rs::reject_flat_map_literal`. (`[k]=v` and empty `()` OK.)
- **(B) Dynamic subscript was unimplemented.** The parser literalised the
  bracket index, so `${m[$k]}` was empty, `${a[$i]}` SB4030-false-rejected,
  and `a[$i]=v`/`m[$k]=v` writes mis-parsed as a command (`sash: bug:`).
  Read fix: `expr.rs::subscript_index_expr` expands `$var`/`${var}` at the
  three subscript sites (downstream already handled `SimpleExpansion`).
  Write fix: `parser.rs` recognises the split-token `name[$var]=` LHS.
- **(C) `keys` of a tainted unblessed map escaped taint** (bare-name arg).
  `validate.rs::check_collection_reader_taint` + `taint_for` cmdsub check
  → SB5020; `len` stays exempt.

The map-iteration idiom now works:
`for k in $(keys m); do echo "$k=${m[$k]}"; done`.

**Spec:** native **414 passing / 2 failing** of 416 (only pre-existing cf.c7/c8).
8 new tests (types.sh 33.5–33.10, taint.sh 60.35/60.36). Rust suite:
identical 167-failing set vs baseline — zero net regression.

**Filed (pre-existing, found here):** `subscript-append-broken` (P2,
`a[i]+=v` discards the element), `nonbrace-dynamic-subscript` (P3, `$a[$i]`
non-brace read — brace form works). `map-iteration-builtins-broken` marked
done with corrected diagnosis.

### Session 2026-06-16 — argv/arg/argc trio (DD#60) + two latent bugs

Implemented the full argv-trio chain (argv-trio-*, all 7 tasks done),
closing `bug-for-in-dollar-at-taint-ice` (top-level `for x in "$@"` ICE).
See [journal 112](journal/112-argv-trio.md).

**DD#60 locked + implemented** (supersedes the argv half of DD#42).
`$(argc)` = C argc (clean); `$(arg N)` = scalar 0-based tainted;
`$(argv)`/`$(argv N)` = tainted array splat (for-in / snapshot only);
`array a=$(argv N)` = tainted snapshot. Native + c backends both pass
`spec/argv.sh`. Validator gained SB5024 (top-level positional, DD#46),
SB5025 (splat misuse), SB5026 (quoted splat, DD#22), SB5027 (mixed
for-in), plus a bare-tainted-cmdsub use-site reject that closed a
**pre-existing getenv leak** (`echo $(getenv HOME)` printed `$HOME`).

**Two latent bugs fixed beyond the documented ICE:** inside-function
`$@` was splicing program argv (not the caller's args) — fixed by
expanding `$@`→`$1..$N` at inline time; and the getenv use-site leak.

**Spec:** native 402 passing / 2 failing (only pre-existing `cf.c7`/`cf.c8`
case-glob). Zero regressions in any mode; c improved 66→62 failing.

**Filed:** `emit-janet-argv-parity` (janet has no argv backend),
`rust-test-suite-rot` (mk test ~158-red at HEAD, pre-existing — argv-trio
added zero net rust regressions), `converter-positional-use-needs-bless`
(every top-level positional read needs a bless the converter can't author).

### Session 2026-06-17 — bless-shape negative-test audit

Per DG's rule (*every blessed spec shape needs a no-bless negative*),
audited all `bless` usages and closed **one more taint hole** plus filed
another. See [journal 112](journal/112-argv-trio.md) follow-up.

**Fixed:** `$(join @a)` / `$(join @arr)` over a tainted-but-unblessed
array escaped the CST taint pass (silent compile for stored arrays, ICE
for argv snapshots). `check_expr_taint` now recurses into a command
substitution's inner command args (`check_cmdsub_use_taint`), so any
tainted read inside `$(… @x …)` rejects SB5020 cleanly. `$(len a)` stays
exempt; blessed/clean joins still compile.

**Negatives added** (5): av.n6 (snapshot join no-bless), av.n7 (snapshot
subscript no-bless), 60.32 (mapfile-array iter no-bless), 60.33
(mapfile-array join no-bless), 60.34 (mapfile-map subscript no-bless).
Every blessed collection shape now paired.

**Filed:** `map-iteration-builtins-broken` (P1) — `keys` emits garbage on
a clean map, `values` ICEs on a clean map, and both leak a tainted map's
data; not spec-covered, needs a key-vs-value taint DD + a codegen fix.
_(Diagnosis later corrected — see 2026-06-17 map-iteration session below.)_

**Spec:** native 409 passing / 2 failing (only pre-existing cf.c7/cf.c8).
Zero regressions.

**Follow-up (same day):** a converter review surfaced a missed ICE —
`for x in $(argv); do echo "$x"; done` (unblessed) and `for x in @a` (a
tainted) ICE'd (the `av.s*`/`av.n*` tests masked it with body blesses).
Fixed in `validate.rs` `taint_for`: splat sources seed the loop var
tainted (body-bless valid, DD#60 Q5), tainted-array iteration is rejected
(DD#61 bless-all). Regression tests av.s5/av.n5.

### Session 2026-06-15 — array taint construction restriction + bless-all (DD#61)

DG reported `array a=( $(getenv HOME) $(getenv PATH) ); bless a [[ … ]];
echo $a[1]` compiling when it shouldn't. Two blind spots: the IR taint
pass didn't recurse into array/map/concat literals, and the SSA backstop
didn't check the array name on subscript reads. Both closed.

**DD#61 locked + implemented.** Tainted values may not enter an array via
literal / `+=` / `a[i]=` (SB5021) — tainted arrays come only from bulk
reads (read -a, mapfile). Whole-array taint is now exact. New bless-all
syntax: `bless x in a [[ $x … ]]` (indexed), `bless k v in m [[ … ]]`
(map, both validated). Scalar bless on a collection is a type error.
Full pipeline: parser → classify_bless → BlessAll/BlessRetag SSA →
Cranelift loop codegen. 31 new spec tests (taint.sh 60.*), all green
native; compile-time checks cross-mode, runtime guarded to native.

**Two coupled pre-existing bugs fixed:** `mapfile`/`read -a < file`
silently dropped the redirect (now routed through __redirect_push/pop);
`map m=()` built an indexed table (now respects the declared kind).

**type_check now runs for all backends** (moved ahead of the janet/c
emit branch) so bless-form errors fire everywhere. Migrated existing
scalar-bless-on-collection tests (54.1, 22.6/8/9/10) to bless-all.

Filed `oracle-blessall-mapfile` (P2): the c/janet oracles don't yet
cover bless-all + mapfile (pre-existing gap). DD#60 stays reserved for
the argv trio; vetting those tasks (join / `@a` spelling issues) next.

### Session 2026-06-07 — task hygiene sweep + cohort wave + DD#53 cruft

Swept 14 non-done tasks; 6 closed (5 done, 1 done-via-supersede), 1
redirected (monitoring-rules → skill-not-paragraph). Filed
cohort-complexity-cog-cc as the standing home for the ongoing
refactor work.

**Cohort wave: 28 commits, 28 violators cleared.** Live cog ceiling
fell from 59 (`emit_stmt`) to 16. Mean cog 3.82 → 2.94, so the
3× threshold ratcheted automatically twice (11.46 → 9.84 → 8.83).
The cohort surfaced 96 functions because the tightening keeps
pulling in moderate violators as the population improves.

**Pre-DD#53 cruft discovered in handle_cd.** While refactoring,
DG flagged that `cd -`, `cd HOME`, and `PWD`/`OLDPWD` autoupdate
violate DD#53 (env isolation). All three were silently active dead
code. handle_cd reduced to chdir + \$? only. See journal 109. A
companion task (tilde-vs-dd53) tracks the same problem in tilde
expansion, which the parser lowers to bare \$HOME and the validator
rejects with a misleading SB5018.

Open task count: 10 (1 active, 4 ready, 3 pending, 1 deferred, 1 todo).

### Recently landed (last ~2 weeks)

**Tree-sitter fully gone.** No `tree-sitter*` in `sash/Cargo.toml`,
no `tree_sitter` anywhere in `sash/src/`. The old `rule.rs`,
`rules/`, `taint.rs`, `validator.rs` files have been removed from
the tree, not just left dormant. Single-parser architecture via
logos (DD#59) + hand-written recursive descent in `sash/src/parse/`.
Validator and taint reimplemented as IR walkers in `validate.rs`
(~650 lines, formerly ~2575 lines across rules + taint).

**C oracle complete (6 phases, 67 tests).** `sash/src/emit_c.rs` +
`sash/tests/c_tests.rs` give a differential oracle against the
Cranelift backend. Both consume SSA. gcc-compiled C output is diffed
against bash. 25/30 janet_tests cases also reach parity via the C
path; the 5 remainders are legitimate divergence (3 sash-rejected
bash syntax, 2 sash extensions bash misinterprets). 38 sash-rt
runtime functions wired. Mirror-codegen architecture (emit_c
mimics codegen call-by-call) caught 6 bugs before runtime.

**Janet oracle stays on IR.** It's the IR-stage oracle; C is the
SSA-stage oracle. emit_janet joined the complexity-gate cohort
(the exemption was removed in the same change set that filed
add-c-oracle).

**Complexity gate enforced.** Commit `323eeb4` introduced the
3× mean cyclomatic gate (currently cc > 16.8) with
grandfather-on-modify mechanic. The cog-vs-cc rubric in
[journal 108](journal/108-cog-cc-rubric.md) replaced the cyclo-allow
exemption table with a delta-aware framework. Phase 11 + 12
landed 16 cohort refactors using the rubric; live ceiling moved
from 49 → 26.

### Architecture (current)

- `sash/src/parse/` — logos lexer + hand-written recursive descent,
  produces IR directly
- `sash/src/lower/` — retired (was CST→IR)
- `sash/src/validate.rs` — IR walker, ~650 lines
- `sash/src/taint.rs` — removed (rolled into validate.rs)
- `sash/src/transform/` — IR → SSA
- `sash/src/codegen/` — SSA → Cranelift → static ELF
- `sash/src/emit_janet.rs` — IR oracle (1331 LOC, in cohort)
- `sash/src/emit_c.rs` — SSA oracle (mirrors codegen)
- `sash-rt/` — pure Rust `#![no_std]` runtime, rustix, no libc

### Remaining open tasks (9)

Active: `cohort-complexity-cog-cc` (standing — 15 grandfathered violators).
Ready: `brush-rerun`, `fuzz-validator`, `r47-validate-rs-per-rule-tests`.
Pending: `audit-string-keyed-builtin-dispatch` (36 sites),
`extract-instctx-setup` (15 sites in dispatch.rs),
`r47-promote-monitoring-rules` (redirected to skill design).
Deferred: `audit-run3` (schedule after next big feature wave).
Todo: `converter-compound-postfix` (SB402x in sash-convert),
`bang-precedence-and-suppress` (`! X && Y` should parse `(!X)&&Y`;
`- ! X` → SB4019; c/janet `!` parity — parser-precedence tail of DD#70's
`!`-must-consume work, whose native core shipped 2026-07-29),
`bug-emit-c-splat-vla-jump` (c-backend: external `@{a:?}` splat after a
preceding statement emits non-compiling C — goto into VLA scope; native ok).

