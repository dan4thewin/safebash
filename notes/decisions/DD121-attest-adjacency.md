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
tainted its subject. A non-adjacent attest is **SB5088**. A binding list
does not enlarge this unit: adjacency is measured per binding, as
specified below.

**R2 — Caught resolution.** A *caught* attest (`if attest …`, `attest … &&
CMD`, `attest … || CMD`) must leave its subject **clean on every path that
falls through** to the merge. A path that does not fall through — `exit`,
`fail`, `return`, `break`, `continue` — is exempt. The **no-else form is rejected**
(**SB5089**): its implicit fall-through resolves nothing.
Clean replacement must be written directly on that subject's failed-attest
path; a helper does not inherit this permission (DD#128). Every `exit N`
must also satisfy EXIT's incoming-data requirements; `fail N` selects ERR
instead, independently of its status.

The two legal ways to satisfy R2:

| | |
|---|---|
| resolve | `attest v [[ … ]] \|\| v=8080` · `if attest v …; then …; else v=0; fi` |
| don't fall through | `if attest v …; then …; else fail 1; fi` |

`else :` is **not** one of them — it falls through and leaves the subject
tainted.

**R3 — unchanged.** Absent an attest, a tainted variable is readable only
through `:!` (DD#91) or its eligible attest predicate.

## Why: removal is the defect

Before this DD, `attest` was *a statement that removed a name from a mutable
set*, at any point in the program. That single property caused everything we
found:

- **It breaks joins.** `taint_child_bodies` walks every sibling body with one
  `&mut` set, so a removal inside a branch is permanent for everything walked
  afterwards. Measured: `if`/`then`-attest read from `else`; a never-true
  `while`; and a **zero-trip `for`** all printed unattested argv.
- **It breaks the call-graph fixpoint.** A fixpoint converges on a grow-only
  lattice; a set with removals can oscillate. SB5067 was introduced, with no
  DD, only to dodge that. It now has a reason of its own (DD#127): a function
  with zero, two or more call sites may not leave its own taint behind.
- **It creates "at which program point do I read the set?"** —
  `tainted_slots_on_entry` reads the *completed* top-level set, so a trailing
  attest retroactively cleans a slot that was tainted at the call.

The attest is **folded into the acquisition's resolution**: only a legal
adjacent success or that subject's direct failed-branch replacement may clear
a live binding. Joins retain unresolved states from every reaching path.
DD#128 closes the ordinary-assignment escape, so replacing bytes elsewhere
cannot silently discharge the protocol. Definition dependencies, branches
and loop back edges are solved to a fixed point before uses are checked.

## What "adjacent" means, precisely

R1 is a **locality** rule; the soundness comes from the per-path merge below
it. So "the statement above tainted it" is the whole test, and it is measured
against an *empty* incoming set — a second `argv a` is still the operation
that taints `a`, even though it changes nothing.

Three structural positions count as "the statement above", because in each the
tainting operation is the syntactic head of the block it precedes:

| position | example |
|---|---|
| a `while` condition → its body | `while readline v { attest v … }` |
| an `if` condition → its branches | `if x=$(cmd) { attest x … }` (DD#88) |
| a `for` in-source → its body | `for x in $(argv 1) { attest x … }` |

Everything else is plain statement order within one block. A compound
statement is measured like any other: `h=$(getenv X) \|\| h=DEF` taints `h`,
so the attest below it is adjacent.

**Binding lists (DG, 2026-11-09).** DD71 permits one or more adjacent
bindings before a separator. This is syntactic convenience, not a batch
acquisition/attestation protocol. Plain assignments and typed declarations
follow the same rule: each binding is its own adjacency step. A later
binding intervenes even if its value is clean. The list's final binding
may therefore have an adjacent attest, but an earlier acquisition cannot
borrow that position.

Write acquisition and resolution together:

```sash
a=$(getenv FIRST)
attest a p?
b=$(getenv SECOND)
attest b p?
echo "$a $b"
```

Do not acquire `a` and `b` in one binding list and then attest them as
a batch. Adding `str`, splitting the captures with semicolons, or
reordering the later attests cannot make both acquisitions adjacent.
The same applies when a clean binding intervenes between an acquisition
and its proposed attest. Separate each acquisition/resolution pair
instead; no new combined syntax is needed.

This is not a count of `$(` tokens or a blanket ban on grouped captures.
Clean producers still need no attest, a whole collection may still use
its ordinary attest-all, and R3 still permits explicit `:!` reads of
unresolved input. An earlier binding left unresolved cannot be cleared
by an attest of the final binding. No batch-scoped signal deferral is
introduced: DD128 protects each legal acquisition/resolution unit.
The binding-list enforcement and preservation controls are pending in
`q-statement-form-residuals`; journal 599 records the ruling.

A call to a function with exactly one call site is measured the same way: the
function's body counts as one compound statement of the caller, so an attest
immediately below the call is adjacent to whatever the body tainted (DD#127).
A body is not a position. A call does not produce the taint it passes in, so
an attest inside a body, of taint the body received, is SB5088, reported at
the call. This applies to globals and collection slots alike (DD#127,
journals 557 and 559).

DD#128 applies the same locality rule to EXIT and ERR. An incoming global
does not become newly acquired merely because a handler starts; its first
statement cannot attest main's interrupted acquisition. Fresh input acquired
inside either handler has ordinary adjacency, with ERR's operational failures
explicitly handled.

Signal dispatch into EXIT cannot split a legal acquisition/resolution pair.
The compiler checks pending signals before acquisition, defers dispatch
through the attest and its required failed-branch resolution, then resumes
polls. Receipt still records the signal. This includes single-use calls and
reader completion, not just adjacent instructions in one backend block.
An ordinary failure or explicit transfer takes its own route before deferred
dispatch. A path that bypasses the attest, such as a false reader condition,
must satisfy EXIT's actual incoming-state requirements.

## Direct failed-attest replacement

DD#128 narrows the former general assignment-kill rule. Within a live
binding, **a clean whole-value replacement resolves outstanding taint only
on that subject's failed-attest path**. Thus `attest v [[ … ]] || v=8080`
still works. An unrelated `v=8080`, buffer copy, clean loop binding or
whole-collection unset changes the value but leaves it unresolved.

Permission is subject-specific and does not enter a named helper, anonymous
capture or forked child. It survives ordinary control flow within the failed
branch, so each continuing branch may write its own clean replacement.
Whole-collection unset and a clean value split can supply that replacement.
Scalar unset remains illegal. `+=`, cell writes and cell unsets never
resolve the whole subject.

A genuine new acquisition gets its own adjacency opportunity, even on an
already-tainted target. A helper can acquire and resolve its own new input;
it cannot merely overwrite received taint on its caller's behalf. `:!`
authorizes one read, not self-copy declassification.

This is not a permanent name blacklist. Clean bindings stay clean, legal
resolution restores ordinary use, and a new body entry initializes fresh
private bindings rather than inheriting the previous call's taint. Globals,
collection slots and captured enclosing bindings retain their live state.
Implemented in journal 568 alongside the independent SSA backstop.

## Loop paths

`break` and `continue` preserve the complete current state at their actual
loop target, including DD#127 output ownership. A break contributes to that
loop's exit; a continue contributes to its next head. Neither state may
disappear merely because the diverting branch does not fall through.
Counts use the same default and outward-clamping decoder as lowering.

A loop head joins its entry with body fallthrough and continue states until
both names and origins stabilize. There is no arbitrary iteration cutoff.
Quiet passes preserve the enclosing loop depth but cannot export return,
jump, ownership-obligation or source-log effects.

A while condition belongs to its own loop (DD68). Its conservative exit
joins the head, post-condition and break states: a hypothetical body must
not erase the final condition's effects. A for checks its words at the
stabilized head, then binds the iteration variable only on the entered-body
path. Its exit retains the pre-binding words state, including the zero-trip
path and the final failed head.

W3 implements these transfers on the existing independent summaries
(journal 562; `loopflow.1`-`.21`, with `.10`/`.14`/`.15`/`.20` subsequently
replaced by `adjpolicy.6`-`.9` under DD128). This does not claim that lowering already
isolates every counted jump across body boundaries; the separately
reproduced capture defect is `bug-counted-jump-crosses-capture-boundary`.

## What it fixes

| filing | outcome |
|---|---|
| `bug-attest-in-branch-leaks-past-merge` | **fixed** — verified rejected; pinned by `bfb.15`/`bfb.16` |
| `bug-slot-inward-taint-reads-final-set` | **fixed** — the trailing attest is now SB5088; verified rejected |
| SB5067 | now DD#127's firewall for functions with zero, two or more call sites; see "SB5067 — what a body may leave behind" below |
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

is **sound and rejected**. The author writes `else fail 1` or `else v=""`.
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

## SB5067 — what a body may leave behind

**The rule (DD#127):** a function with zero, two or more call sites may not
leave behind taint produced by its own commands, on a global or on a
collection slot (DD#100). It attests that taint inside itself, adjacently, by
R1. A function with exactly one call site, and an anonymous `$( … )` body, is
part of the caller's statement: whatever it leaves tainted, the caller attests
immediately after the call, or a later non-`:!` read is SB5020. Taint a body
receives is not its own, so it never makes SB5067, and the body cannot attest
it either (see "What "adjacent" means, precisely").

Before W1 landed, the compiler applied the 2026-09-30 rule recorded under
History below, and `bfb.17`-`bfb.28`, `cov.264`-`cov.270` and `cov.341` pinned
that rule.

**Why an in-body attest is enough.** The worry about any rule that lets a body
repair its own taint is that code between the taint and the repair brings back
order-dependence. R1 forbids such code: the attest is the statement
immediately below, and R1 is name-agnostic (measured firing identically for a
local and for a global). There is no order to depend on.

**Diagnosis keeps its span.** Rather than report at body exit — which would
blame the function and lose the location — the pass gathers syntactic taint
sites with their spans *before* the walk and passes verdict *after*, reporting
only sites whose name is still dirty at exit, deduped to the last site per name
(the one actually responsible). This applies to shared named bodies under
DD#127. The former single-use `bfb.20` example no longer pins this diagnostic:
it retains its single call and now asserts SB5020 at the later caller read.

**The number SB5067 is kept** — my call at the time, ratified by DG
2026-09-30 ("we already reused SB5067? if so, let it stand"). The rule has
been loosened and re-scoped, not replaced, and reusing the number keeps every
existing spec assertion structurally valid.

**Each body reports its own sites.** Site collection uses
`walk_scoped_structural_stmts` filtered on `in_fn.is_empty()`, so a nested
function's sites are not attributed to its parent, and the walk does not
descend into `$( )` bodies. Under DD#127, a statement that holds a single-use
call or a `$( )` and leaves own taint behind is itself a source site of the
enclosing body. W1 decides the span it reports.

### The SSA backstop is not the authority

Giving `ArgvFill`/`ReadArray`/`SplitValue`/`MapFromArray` real SSA versions,
so that the SSA taint backstop can see bulk fills, does **not** make SB5067
redundant. The backstop cannot be the authority:

- its messages carry no line or column (`tainted variable $x used as …
  (SSA taint backstop)`);
- it reports at the *use* site, not the definition;
- `taint_of_name` is flow-insensitive in the permissive direction;
- it honours `:!`;
- `--no-taint` disables it.

It upgrades the collection failure mode from a silent leak to an ICE at the
use site; it does not diagnose.

That change landed 2026-09-30. The four fills carry `target: VarRef` and call
`state.define()` *after* the autodeclare. That gives a monotone layering —
clean empty declaration, tainted fill, clean attest — which `taint_of_name`'s
max-version rule reads correctly with no special-casing. A leaked bulk fill is
therefore caught twice: by the validator, with a span at the taint site, and
by the backstop at the use. Only the first is a diagnosis. Detail, and the two
bugs it exposed: `notes/tasks/ssa-taint-full-backstop.md`, journal 450.

### History

- **2026-09-30, the net-effect rule.** This DD first left SB5067 "in place,
  re-decided separately with DG". DG relaxed it from a syntactic rule (any
  taint of a global anywhere in a body) to a net-effect one: a body may not
  leave a global tainted that it did not inherit ("no net insert", measured
  against the body's entry set). The backstop work above was its pair. Specs
  `bfb.17`-`bfb.22`.
- **2026-09-30, extended to `$( … )` bodies**, closing
  `q-cmdsub-body-taints-outer-global`. Sash inlines an anonymous body, so it
  could leave an outer global tainted with no located error. Specs
  `bfb.23`-`bfb.28`, journal 451; the sweep went 7071 → 7083, all of it the
  new specs. The extension is what makes `taint_cmdsub_bodies`'s
  clone-and-discard sound. DG observed that `:!` on a body-tainted global is
  no exception, because SB5067 judges the body at its exit and never examines
  the read. That still holds for shared functions.
- **2026-10-31, DD#127 replaces both.** A shared function keeps a firewall,
  now on its own taint only. A single-use function and an anonymous body pass
  their own taint to the caller, which attests after the call, so the
  extension to `$( )` bodies is reversed: `bfb.23`, `bfb.24` and `bfb.28`
  become accepted. W1 replaces the clone-and-discard with the body's summary.
  The text of both earlier rules is in git at 0149c49b.

## Addendum 2026-10-02 — the negated spelling

`if ! attest C { A }` is accepted, and means what it reads as: divert if the
attest fails. It is **not a fourth caught form**. The parser rewrites it at
construction into the first one —

```
if ! attest C { A } else { B }   ==>   if attest C { B } else { A }
```

— so the three caught forms above are still exactly the three the analysis
knows, and R2 applies to the rewritten shape unchanged. DG's reasoning: a
negated spelling could instead have been supported by carrying a polarity into
R2's resolution check and the SSA merge, but those are two places whose
"resolve" and "divert" arms would have to invert in step, and a silent
disagreement between them is a miscompile rather than a diagnostic.

With an `elif` chain the rewrite nests rather than swaps, since the chain
hangs off the unnegated condition being false. The rewrite is attest-only:
an ordinary `if ! cmd` keeps its polarity, and `--emit-ir` keeps reporting it.

A bare `! attest …`, and `while ! attest …`, remain rejected — see
`notes/tasks/sb5088-wording-for-non-caught-attest-positions.md`, which is
about their wording, not their legality.

## See also

- DD#24 taint model · DD#91 `:!` · DD#82 `--no-taint` · DD#100 collection
  parameters (the slot machinery whose call-site injection is the pattern this
  generalises) · DD#127 taint across function boundaries
- Specs: `taint.sh` (`div.1`-`div.32`), `coverage.sh`. journal 448, 457.
