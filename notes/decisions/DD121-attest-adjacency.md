title: DD#121 — attest adjacency, and caught-attest resolution
status: accepted

Settled with DG (2026-09-30), out of a session that started as an explanation
of taint scoping and turned up three P1 bugs.

## The thesis

DG: *"requiring attest to immediately and unconditionally follow the operation
that taints a variable eliminates all order-dependent taint-bypass. Absent an
attest, the tainted variable can only be read with `:!` modifier."*

## Half of it was already law

The second clause ships today and needed no change:

- **SB5020 already rejects every read of a tainted name** — copy
  (`str y=$t`), comparison (`[[ $t == foo ]]`), command argument
  (`/bin/echo "$t"`). Arithmetic is SB4030.
- **`:!` (DD#91) is already the sanctioned per-use bypass**, and since that
  DD's 2026-09-09 split it is valid in every position a bare expansion is.
- **An anonymous taint source is already forced to bind a name**:
  `/bin/echo $(getenv HOME)` is *"tainted command substitution used without
  attest — assign it to a variable and attest before use"*. So "the operation
  that taints a variable" has no gaps to cover.
- SB5020's own message has advised the discipline all along: *"`attest t
  [[ … ]]` **at the source**, then read it."*

What was missing was enforcement of the first clause.

## The rules

**R1 — Adjacency.** An `attest` clears taint only when it is the
**immediately following statement, in the same block**, of the statement that
tainted its subject. A non-adjacent attest is **SB5088**.

**R2 — Caught resolution.** A *caught* attest (`if attest …`, `attest … &&
CMD`, `attest … || CMD`) must leave its subject **clean on every path that
falls through** to the merge. A path that does not fall through — `exit`,
`return`, `break`, `continue` — is exempt. The **no-else form is rejected**
(**SB5089**): its implicit fall-through resolves nothing.

The two legal ways to satisfy R2:

| | |
|---|---|
| resolve | `attest v [[ … ]] \|\| v=8080` · `if attest v …; then …; else v=0; fi` |
| don't fall through | `if attest v …; then …; else exit 1; fi` |

`else :` is **not** one of them — it falls through and leaves the subject
tainted.

**R3 — unchanged.** Absent an attest, a tainted variable is readable only
through `:!` (DD#91) or a `bless` predicate.

## Why: removal is the defect

Before this DD, `attest` was *a statement that removed a name from a mutable
set*, at any point in the program. That single property caused everything we
found:

- **It breaks joins.** `taint_child_bodies` walks every sibling body with one
  `&mut` set, so a removal inside a branch is permanent for everything walked
  afterwards. Measured: `if`/`then`-attest read from `else`; a never-true
  `while`; and a **zero-trip `for`** all printed unattested argv.
- **It breaks the call-graph fixpoint.** A fixpoint converges on a grow-only
  lattice; a set with removals can oscillate. SB5067 — which had **no DD**
  until this one's 2026-09-30 addendum — exists only to dodge that.
- **It creates "at which program point do I read the set?"** —
  `tainted_slots_on_entry` reads the *completed* top-level set, so a trailing
  attest retroactively cleans a slot that was tainted at the call.

After this DD the attest is **folded into the definition**: a removal can only
happen one statement below the insert, in the same block, so no removal
survives a block boundary and nothing can observe the taint in between. Taint
becomes ordinary **reaching definitions over a join-semilattice** — tainted if
*any* reaching definition is tainted — so branches, loops, back-edges and the
call graph are monotone and standard.

## What "adjacent" means, precisely

R1 is a **locality** rule; the soundness comes from the per-path merge below
it. So "the statement above tainted it" is the whole test, and it is measured
against an *empty* incoming set — a second `argv a` is still the operation
that taints `a`, even though it changes nothing.

Four structural positions count as "the statement above", because in each the
tainting operation is the syntactic head of the block it precedes:

| position | example |
|---|---|
| a `while` condition → its body | `while readline v { attest v … }` |
| an `if` condition → its branches | `if x=$(cmd) { attest x … }` (DD#88) |
| a `for` in-source → its body | `for x in $(argv 1) { attest x … }` |
| DD#100 slot arrival → the body | `fn f { attest v in A … }` — Part E |

Everything else is plain statement order within one block. A compound
statement is measured like any other: `h=$(getenv X) \|\| h=DEF` taints `h`,
so the attest below it is adjacent.

## The assignment kill

R2 has to be satisfiable, so DD#121 adds the missing half of
"definition-based": **a whole-variable assignment from a clean RHS kills the
taint.** `v=8080` and `a=(dflt)` replace the entire value with something that
came from nowhere external, so the name is clean afterwards.

Scoped tightly — `+=` appends to what is already there, and `a[i]=v` replaces
one cell of a container. Neither kills.

The old flat walk got the same answer by accident, because `attest` removed
the name unconditionally and nothing ever put it back. With per-path merges
that accident is gone, and DD#24's sanctioned `attest v [[ … ]] || v=8080`
only works because the assignment itself resolves the subject.

This is what AGENTS.md has claimed all along and the implementation did not
honour: *"Taint is definition-based, not name-based."*

## What it fixes

| filing | outcome |
|---|---|
| `bug-attest-in-branch-leaks-past-merge` | **fixed** — verified rejected; pinned by `bfb.15`/`bfb.16` |
| `bug-slot-inward-taint-reads-final-set` | **fixed** — the trailing attest is now SB5088; verified rejected |
| SB5067 | **re-decided 2026-09-30** — relaxed to a net-effect rule; see the addendum below |
| `bug-caught-attest-all-fail-path-undefined-handle` | **fixed separately** (journal 449) — a native-codegen defect, not a taint one; DD#121 made it unreachable from safe source, `--no-taint` kept it reachable |

## Measured cost

3261 programs harvested from a green native sweep; 221 compile and use attest,
giving 136 taint-source sites:

- **126 already adjacent** (93%)
- **5 caught**
- **3 non-adjacent**, all trivially reorderable (two are `a=$(p); b=$(q);
  attest a; attest b`; one is a dead double `argv x`)
- 2 never attested (read through `:!`)

Of 26 caught-attest sites in the suite, **20 needed rewriting and the rest
survive** (including `bfb.1`–`4`, the `|| v=8080` idiom). The 20: 14
`coverage.sh` arith-convert sites, `24.13`, `60.18`, `bfb.5`, `bfb.6`,
`taint_switch.sh` `bkstp.3` (no else) and `errors.sh` `csc.37` (`|| :`). Every
rewrite is one added `else`. Ten new specs (`bfb.7`–`bfb.16`) pin R1, R2, R3
and the two branch leaks; the sweep went 3614/3396 → **3624/3406, green**.

## Rejected alternatives

- **Join only** (compute the union at each merge, reject at the later read).
  Sound, costs zero specs, and it is still implemented underneath — but it
  leaves a caught attest uncheckable in isolation, which is the locality the
  whole DD is buying.
- **Ban caught attest.** Loses `|| v=8080`, which DD#24's 2026-09-01 amendment
  added deliberately.
- **Abort on the unwritten else** (DG's first sketch). Cheaper — 3 specs — but
  it makes `attest X && CMD` *exactly* equivalent to `attest X; CMD` and
  `if attest X; then A; fi` to `attest X; A`. Both spellings would survive
  while no longer catching anything, so a reader sees `&&` and expects a gate
  that is not there. Compile-time rejection is the house style.

## A consequence accepted on purpose

```sash
if attest v [[ … ]]; then process "$v"; else /bin/echo bad; fi
# v never read again
```

is **sound and rejected**. The author writes `else exit 1` or `else v=""`.
The payoff is that a caught attest is checkable by looking at the caught attest
— no downstream analysis, the same locality R1 buys at the definition.

## Supersedes

DD#24's 2026-09-01 amendment table row:

> | `bless VAR [[ t ]] && CMD` | short-circuit; CMD skipped; execution continues |

The no-else form is now rejected outright (SB5089), so there is no
continue-on-failure path left to specify. The `|| DEFAULT` row stands unchanged
— it resolves the subject, which is exactly what R2 asks for.

DD#24's *"`if bless VAR [[ test ]]` → False branch taken, VAR remains
tainted"* stands for the **written** else: the subject is tainted inside that
branch, and R2 requires the branch to resolve it before falling through.

## Addendum 2026-09-30 — SB5067 re-decided: a net-effect rule

The table above left SB5067 "in place, re-decided separately with DG". DG chose
**(a) relax it to a net-effect body-exit rule**, paired with **(ii) close the
SSA backstop's collection gap** (a separate piece of work, below).

**The rule is now: a function body may not *leave* a global tainted that it did
not inherit.** The old rule was syntactic — *any* taint of a global anywhere in
a body was rejected, regardless of what happened next.

The predicate is deliberately **not** "the global is clean at body exit". That
is unsatisfiable: a body inherits the caller's tainted set, and R1 forbids
cleaning an inherited taint (the attest would not be adjacent to anything), so
the moment any global is tainted at top level *every* function in the program
would be in permanent violation — including ones that never mention it.
Measured before choosing the wording.

So the predicate is **no net insert**: the exit set may not contain a global
absent from the entry set. Paired with R1 — which cannot remove an inherited
taint — a body's net effect on the caller's set is exactly nil, which is what
preserves `check_taint`'s fixpoint-free two-phase design. That design's own doc
comments named SB5067 as the soundness premise; they now name this rule.

**R1 is what makes the relaxation safe, and it is why the ordering objection
does not apply.** The worry about relaxing SB5067 was always that intervening
code between the taint and its repair reintroduces order-dependence. R1 forbids
intervening code outright — the attest must be the statement immediately below —
and it is **name-agnostic**: measured firing identically for a local and for a
global. There is no order to depend on.

Consequences, each pinned by a spec:

| shape | verdict | spec |
|---|---|---|
| taint a global, attest adjacently | **now compiles**; caller reads it clean | `bfb.17` |
| the same body without the attest | SB5067 | `bfb.18` |
| the in-body attest | a real runtime gate, not a compile-time waiver | `bfb.19` |
| one branch attests, another does not | SB5067, reported at the *leaking* branch | `bfb.20` |
| re-taint a global the top level attested clean | SB5067 — it is a net insert | `bfb.21` |
| a body that never touches an already-tainted global | not blamed | `bfb.22` |

`bfb.21` is the case that matters most: if the top level attests a global clean
and a callee re-taints it, the entry set no longer contains it, so the re-taint
*is* a net insert and is rejected. The caller's clean view cannot be silently
falsified.

**Diagnosis keeps its span.** Rather than report at body exit — which would
blame the function and lose the location — the pass gathers syntactic taint
sites with their spans *before* the walk and passes verdict *after*, reporting
only sites whose name is still dirty at exit, deduped to the last site per name
(the one actually responsible). `bfb.20` demonstrates it: the blame lands on the
`else` branch, not the `if`.

**The number SB5067 is kept** — my call at the time, ratified by DG
2026-09-30 ("we already reused SB5067? if so, let it stand"). The diagnostic is
the same rule loosened, not a new one, and reuse keeps every existing spec
assertion structurally valid.

**Scope change worth knowing:** site collection moved from `walk_stmt_tree` to
`walk_scoped_structural_stmts` filtered on `in_fn.is_empty()`, so a nested
function's sites are no longer attributed to its parent — the nested body gets
its own verdict. The walk does not descend into `$( )` bodies either, and for
the same reason: as of the extension below, each body gathers and reports its
own sites, so descending here would report one leak twice.

### Why (a) alone, and what (ii) is for

(ii) — giving `ArgvFill`/`ReadArray`/`SplitValue`/`MapFromArray` real SSA
versions so the SSA taint backstop can see bulk fills — does **not** make
SB5067 redundant, which is why DG took both. The backstop cannot be the
authority: **SSA is spanless** (the readonly backstop at `ssa.rs:1492` hardcodes
`"1:1:"` for want of a location), it reports at the *use* site rather than the
definition, `taint_of_name` is flow-insensitive in the permissive direction, it
honours `:!`, and `--no-taint` disables it. (ii) upgrades the collection failure
mode from a silent leak to an ICE at the use site; it does not diagnose.

**(ii) landed 2026-09-30.** The four fills now carry `target: VarRef` and call
`state.define()` *after* the autodeclare, giving a monotone layering — clean
empty declaration, tainted fill, clean attest — that `taint_of_name`'s
max-version rule reads correctly with no special-casing. That is also why the
"attest-clears check" the backstop task asked for turned out to be unnecessary:
the attest already defines a strictly higher clean version.

It changed nothing about SB5067, exactly as predicted above. What it did change
is the relationship between the two layers, in the direction this section
argues for: a function that bulk-fills a global is now caught twice — by
SB5067 with a span at the taint site, and by the backstop at the use site — and
only the first of those is a diagnosis. Detail and the two bugs it exposed:
`notes/tasks/ssa-taint-full-backstop.md`, journal 450.

### Extended to `$( … )` bodies — DG, 2026-09-30

The net-effect rule now applies to anonymous bodies as well, closing
`q-cmdsub-body-taints-outer-global`. DD#86 already calls a `$( … )` body an
anonymous function body, and sash **inlines** it rather than forking a
subshell, so a write to an enclosing name persists. Before this, a function
leaving a global tainted was a located SB5067 and the identical `$( )` form was
silent — or, where the global happened to be read afterwards, a spanless SSA
backstop reject that `--no-taint` turns off.

The relaxation is what made this affordable. The old objection to extending
SB5067 was that it "would newly reject programs that taint inside a body and
attest after it"; under the net-effect rule an **adjacent in-body attest is the
repair**, and it works — so the only newly-rejected shape is the genuinely
leaky one the function rule already rejected. Measured: the sweep went
7071 -> 7083, the entire +12 being the new specs. No existing spec changed
verdict.

**DG's `:!` observation is why this is complete rather than partial.** An
earlier draft flagged `:!` on a body-tainted global as an exception the
"anonymous function" model left open. It is not one: SB5067 fires at **body
exit** and never examines the read site, so the bypass is evaluated too late to
matter. That also retires the exception raised during this DD's own discussion
— "the sole exception being a missing attest and a global only accessed by `:!`
(an exception we could disallow...)". The relaxation closed it as a side
effect. `:!` keeps working where the intent is local: a top-level taint, and a
body's own locals.

Clone-and-discard in `taint_cmdsub_bodies` is now sound rather than merely
convenient — the verdict is what makes the discarded set provably equal to the
one the enclosing scope keeps, which is the no-fixpoint invariant
`taint_top_level` relies on.

Specs `bfb.23`-`bfb.28`. Journal 451.

## See also

- DD#24 taint model · DD#91 `:!` · DD#82 `--no-taint` · DD#100 collection
  parameters (the slot machinery whose call-site injection is the pattern this
  generalises)
- Specs: `taint.sh`, `coverage.sh`. journal 448.
