# 228 — DD#86 Phase 4: dead-variable rejection (SB5050)

The dual of DD#62's SB5031 (read with no reaching assignment). DD#86: every
variable must appear in **both** a write and a read; a variable **assigned
but never read** is a dead variable → **SB5050**. This is the safety net for
a misspelled write target (the `coutn = count` typo from the DD), caught from
whichever side it is stranded on.

## Scope model

- A **global** (a top-level introduction) is dead iff it is read *nowhere in
  the program* — a global may be read from any function (the method pattern).
- A **function-local** (a name written in a function that is not a top-level
  global) is dead iff it is read *nowhere within that function*.

## DG rulings (2026-09-02)

Three forks DG settled while reviewing:

1. **More-specific error wins.** A variable can be both dead AND involved in a
   taint (SB5020), type (SB4030), or transform (SB4044 const-reassign,
   SB5015/16 read-flags, SB5041 collision) error. DG: the more-specific error
   should surface. So the dead-var gate runs **last** — after taint, type, AND
   the transform diagnostics. Implementation: `check_dead_variables` moved out
   of `validate()` (structural) to the very end of the front-end in `lib.rs`,
   after `enforce_transform_diags` (which is now run for every backend, not
   just the alt ones). A pure dead var with no other error still reports
   SB5050.
2. **read targets and for-vars ARE checked.** A discarded `read`/`readc`/
   `mapfile` target and an unused `for` loop variable are ordinary writes —
   dead if unread. (`for` is no longer exempt.)
3. **`: $v` is the acknowledgment escape hatch.** Established in DD#81 arity
   (`: ${args[1]}` acknowledges a skipped positional; `: @args` marks a
   function variadic), so `: $v` is a real read. A deliberately-unused write is
   kept alive with `: $v` (or `: @a` for a collection). For a *tainted* unread
   value the acknowledgment must be blessed (any tainted read is).

## Implementation — reuse the read-enumerator (DG)

DG: "reuse the existing complete read-enumerator" (not a parallel hand-rolled
one). The SB5031 read-traversal (`check_reads_in_scope` and friends) was
inverted via a shared `ReadSink`:

- `ReadSink::Flag(&mut diags)` — the SB5031 behavior (flag a read whose name is
  not in the current scope's `known`).
- `ReadSink::Collect(&mut set)` — dead-var (collect every read name; scope-
  agnostic superset).

The traversal now drives one `record` per read; the sink decides. So SB5031
and dead-var can **never disagree** on what a read is. Two SB5031-specific
opt-outs are expressed in the sink, not the traversal, so Collect sees them as
reads:

- `${x:-…}` / `@a` / `${x:s/…/…/}` (operator-bearing param expansions) — SB5031
  treats these as the "intentional default / transform" opt-out; dead-var
  counts them (`record_param(opt_out)`).
- a DD#58 arith sexpr name `(x)` / `(random)` / `(len m)` — collected for
  dead-var (`collect_only`, might be a variable) but never flagged by SB5031
  (intrinsic/function names are not undeclared reads).

Also surfaced + fixed two enumerator gaps (benefit SB5031 too): a `(( x ))`
statement/condition (ArithTest) and the bless **array/map operand**
(`bless x in a` reads `a`) were not visited.

**The one asymmetric case** the generic expansion enumerator does not cover:
the bare *collection-name* operand of a value builtin (`len a` / `join a` /
`keys m` read `a`/`m` by bare name — see below). `collect_value_fn_operands`
adds them, reusing `len_operand_name` / `join_operand_name`.

## The bare-collection-name asymmetry (filed)

`len a` / `join a` / `keys m` read the collection by **bare name** (deliberate:
`join_operand_name` maps bare `Literal("a")` → collection `a`, and SB4030 says
"…or splat with `@a`"). A **user function** does the opposite — `f a` passes
the literal string `"a"`, and you splat with `@a`. Same call surface, opposite
binding. Filed `notes/tasks/bare-collection-name-operand-asymmetry.md` with
four options for DG (keep / unify-to-splat / unify-to-byref / explicit list).
(DG's recollection that `join` "needed the splat" is exactly this asymmetry —
the bare-name form is current behavior.)

## Verification

The read-completeness was validated empirically: strict dead-var started at 65
native breaks with the hand-rolled collector (with false positives on
`len a`/`join a`/`@a`/`${x:-…}`); after inverting to the shared enumerator +
the value-fn operands + the ArithTest/sexpr/bless gaps, **every remaining break
was a genuinely-dead variable — zero false positives**. The reorder (dead-var
last) + spec rewrites cleared them.

Rewrote 14 genuinely-dead specs to keep their intent while reading the variable
(`: $v` / `: @a` acknowledgment; `bless x [[ -n $x ]]` for tainted values;
dropped truly-unused scaffolding). New specs `spec/scope.sh` 86.13–86.20
(dead global, the `coutn` typo, read-makes-live, `: $v` escape hatch, for-var
dead/live, method-pattern cross-function read, taint-precedence). Full 3-mode
sweep vs HEAD `30be5f6`: **native 0 regressions +8, c 0 regressions +8, janet
0 regressions +8**. `cargo fmt`/`clippy` clean; lib unit tests pass; all
touched functions cleared the complexity gate (extracted `check_dead_globals`/
`check_dead_locals`/`report_dead`/`push_*_writes`/`record_cmdsub_value_fn_
operands`/`check_arith_compound`/`check_arith_each`).

## DD#86 status

Phases 1–4 shipped. Phase 5 (converter `local`→drop; GUIDE/SUBSET; DD#35
lifetime-axis amend) remains.
