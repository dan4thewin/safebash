title: DD#86 — Variable scope: no `local`, no shadowing, use-before-and-after
status: accepted

Settled with DG (2026-09-01). Replaces bash's `local`/dynamic-scope model with a
**structural** one. Supersedes the *lifetime* axis of DD#35, abandons DD#18
(`local fn`), and pairs the DD#62 read-side typo check with a new write-side one.

## The problem

bash's default is dynamic scope: a bare `x=v` in a function writes a *global*,
and you must remember `local x` to opt out. Forgetting `local` silently clobbers
a global; using `local` creates a **shadow** (a local hiding a same-named
global) that a reviewer can't trace. Both are footguns.

## The model

- **Scope is positional, not keyworded.**
  - A variable **declared at top level** is a **global**.
  - A name **first assigned inside a function** that is *not* a top-level global
    is a **function-local** (automatic — no keyword).
- **Globals are read *and* written with no friction** from any function. The
  method-like pattern (a set of functions manipulating shared top-level state)
  is first-class — reading and writing a global look identical to a local.
- **No shadowing, and one declaration per name per scope.** A name is a global
  *xor* a local — never a local hiding a global. Consequently a *typed
  re-declaration* inside a function of a name that is already a top-level
  global is an **error** (redeclaration); a *bare* assignment to that name
  simply **writes** the global. The same error covers a second typed
  declaration within **one** scope, whether or not the type matches — so
  `str x` twice, `str x` then `int x`, and `str x` then `const x` are all
  rejected. What stays legal is what lives in a genuinely different scope: two
  sibling functions may each declare `str x`. Control flow has no scope of its
  own (below), so two branches declaring one name are two declarations of one
  global. A `for` variable is not a declaration and is untouched.
- **`local` is removed** (keyword + the save/restore machinery). Lifetime is now
  positional, so the DD#35 *lifetime* axis is retired.
- **`local fn` (DD#18) is abandoned** — it was the only other user of `local`,
  and "designed, not built."

## Declaring globals — the `str` keyword

Globals must be introduced at top level, so a way to declare each type
*without* an initializer is needed. `int n` already declares an int
(meaningful-zero `0`, DD#62). Add the missing scalar:

- **`str name`** — declares a **string** variable, meaningful-zero `""`
  (parallel to `int`). `str name = "x"` also allowed.

Full type-keyword set is now `str` (string) / `int` / `array` / `map`, with
`const` orthogonal (mutability). A bare top-level `x = v` still declares a global
(inferred string), same as today; `str` is for the declare-without-assign case
(and for making a string declaration explicit).

## Use-before-**and**-after: reject unused variables

DD#62 already rejects a **read** with no reaching assignment (SB5031, "assigned
nowhere"). DD#86 adds the dual: **a variable that is assigned but never read is
an error** (dead variable). Every variable name must therefore appear in *both*
a write and a read.

This is the safety net for "no shadowing + frictionless writes": a misspelled
target is caught from whichever side it is stranded on, with **no warning** —
warnings signal ambiguity, and there is none here, so it is a hard error:

```
str count
fn inc { coutn = count + 1 }   # `coutn` local, assigned, never read → ERROR
fn inc { count = coutn + 1 }   # `coutn` read, never assigned         → SB5031
```

Because globals must be top-level-declared and there is no dynamic scope
(a callee cannot see a caller's local), a misspelled name is always a
per-function local — so it is caught by the unread-write rule if the typo is on
the write, or the unassigned-read rule (SB5031) if it is on the read (a local
never crosses to another function). The only uncatchable case is a typo that
lands on *another* declared global — fundamentally undetectable, and rare.

`const` is **not** exempt: an unread `const` is dead → error.

## Explicitly rejected alternatives

- **Warnings** for "function writes a global it never reads" — rejected. No
  compiler warnings at all; ambiguity is a hard error.
- **Loop-variable auto-scoping** — rejected as YAGNI. A `for` / C-style loop
  variable is an ordinary variable (may persist after the loop; use a `_i`
  convention if a same-named global is a concern).
- **Read-only globals from functions** (an earlier "Model A") — rejected;
  globals are freely writable.

## Consequences / migration

- **Converter**: bash `local x` → drop the keyword (the variable is
  auto-local). bash functions that write globals keep working (writes stay
  frictionless). A bash local that *shadowed* a global must be renamed
  (no-shadowing) — the converter flags it for review.
- **DD#35** amended: lifetime axis retired; `str` added to the type axis.
- **DD#18** abandoned.
- Spec suites using `local` are rewritten to the keyword-free form.

## Implementation phases

1. `str` keyword (additive). **— shipped (journal 223).**
2. Drop `local` + make scope structural (top-level global vs function-local),
   frictionless global R/W. **— shipped (journal 226); `local fn` (DD#18)
   retired in the same phase since it shared all the machinery. Surfaced +
   fixed a latent bug: a function-local `const` tripped SB4044 because the
   body is lowered twice and `readonly_vars` leaked (isolated via
   `enter/exit_fn_body_scope`).**
3. No-shadowing / redeclaration error. **— shipped (journal 227): SB5049,
   `check_no_shadow` in validate.rs; a typed re-declaration inside a function
   of a top-level global is rejected; a bare assignment writes the global.**
   **Same-scope half completed (journal 310, DG 2026-09-14): `check_redeclaration`
   rejects a second typed declaration of a name within one scope, regardless of
   type match. Previously only the cross-scope half was enforced, so at top level
   a name could be re-declared, have its type changed, or silently acquire
   `const`. The top level and the DD#56 `END { }` body are one scope. SB4044 no
   longer fires on a declaration — it means only a bare assignment to a const.**
4. Dead-variable rejection (new validate pass, dual of DD#62). **— shipped
   (journal 228): SB5050. Reuses the SB5031 read-enumerator (inverted via a
   shared `ReadSink`: Flag mode = SB5031, Collect mode = dead-var) so the two
   never disagree on what a read is; value-fn bare operands (`len`/`join`/
   `keys`) added on top. DG rulings (2026-09-02): (a) more-specific error wins
   — dead-var runs LAST, after taint/type/transform; (b) `read`/`mapfile` targets and `for` variables ARE checked; (c) `: $v` is the
   acknowledgment escape hatch (a real read, per DD#81 arity). Filed
   `bare-collection-name-operand-asymmetry` (builtin bare-name vs user-fn
   splat).**
5. Retire `local fn`; converter; specs; GUIDE/SUBSET. **— shipped (journal
   229). `local fn` retired in Phase 2; the converter now drops `local`
   (bare `local x=5` → `x=5`, `local -i x=5` → `int x=5`); GUIDE + DD#35
   amended (lifetime axis retired, `str` added). Remaining refinement: the
   converter does not yet detect+rename a bash local that *shadowed* a global
   (flag-for-review) — filed follow-up.**

## Amendment (2026-09-11, DG) — a `fn` must be defined at the top level

> "I think all functions must be top-level — counterargument?"

**Accepted, in the broad form.** A `fn` definition is legal only as a direct
element of the top-level statement list. Anywhere else is **SB5071**: inside
`if`/`while`/`for`/`case`, inside a `{ … }` group, as an operand of `&&`/`||`,
as a pipeline stage, or inside another function's body.

### Why this belongs to DD#86

DD#86 makes scope **positional**, and gives control flow no scope of its own —
loop-variable auto-scoping is one of the rejected alternatives above. So a
function name is global *wherever the definition is written*, and the compiler
hoists it. The hoisting is DD#86-correct. What is wrong is that the placement
then **reads as a scope, or a condition, that does not exist**:

```sh
if [[ 1 = 2 ]] { fn f { echo A } }
f                    # sash printed A;  bash: `f: command not found`
```

The branch is false and never runs. Bash defines functions at *execution* time,
so it has no `f`; sash defines them at compile time, so it does. Same
divergence for `false && fn f {…}`, and for a `fn` used as a pipeline stage —
bash runs the stage in a subshell, so the definition cannot survive into the
parent at all. With one definition per branch of an `if`, sash took the
**lexically last** regardless of which branch ran.

It was also the last **taint bypass**. `check_taint` defers the functions it
finds in the top-level statement list and re-walks them against the completed
tainted set; one hidden inside control flow was not in that list, kept the old
definition-point walk, and could read a global tainted afterwards. It compiled
and leaked raw argv. SB5071 closes that hole **by construction** — the
`taint-fn-in-toplevel-control-flow` fix sketch (thread a phase flag, pair each
deferred function with its scope) is no longer needed.

### The counter-argument, and why the broad form won

The counter-argument is only against the word *all*. Specs 18.3/18.4 pinned
`fn` inside another `fn` as legal, citing this DD: a nested function is an
ordinary global, callable from outside. That form is never conditional, so it
carries none of the divergence and none of the taint hole. Rejecting it costs
two specs.

But that is a **cost** argument, not a principled one — and by this DD's own
logic the nested-in-a-function case is misleading for exactly the same reason:
its placement suggests a scope that function names do not have. Spec 18.3 was
pinning the wart, not a feature. Applying the principle uniformly is the whole
point of having it, so the broad rule wins and 18.3/18.4 are inverted.

Specs `spec/functions.sh` 18.3–18.12 (including the taint case, and a control
proving the top-level spelling is still taint-checked).

## Amendment (2026-09-11, DG) — `const` in a loop body is SB5072

> "const shouldn't be within loop"

Follows directly from the no-block-scope model. A loop body introduces no
scope, so a `const` inside one is a single binding **re-executed** — a const
reassigned.

The construct was already rejected, but badly: only by the spanless backstop in
`VersionState::define`, reported at `1:1` with no source location. In the
`while` case it was rejected only as a *side effect* of `readonly_vars` leaking
across that loop's two-pass lowering — the same leak Phase 2 above had already
fixed for function bodies. SB5072 makes the rejection deliberate and spanned,
so it no longer depends on a bug to happen.

`const` inside a **function** body stays legal (Phase 2's fix): a function body
is entered once per call, not re-executed in place.

Specs `spec/scope.sh` 86.25–86.30.

## Addendum (2026-09-17) — the `for`-variable "exemption": RETRACTED

**This addendum originally claimed the exemption was deliberate and held. It
was wrong on the fact that mattered, and the claim is withdrawn.** What
follows is kept as a record of the error; the live question is in the
follow-up addendum below.

The argument rested on reading the model section's "A `for` variable is not a
declaration and is untouched" as a decision. It is not one. `git log -L`
places it in commit `06912b0` (2026-09-14), an *implementation* commit for
SB5049, whose own message gives the rationale: "A for variable is its own
StmtKind, never a Declaration, so repeating one is untouched." That is an
observation about the AST, written into the model section as though it were a
design call. DG did not make it.

The original audit table follows:

| pattern                                   | verdict |
|-------------------------------------------|---------|
| loop var reuses a top-level global's name | legal — one variable, overwritten; bash-identical |
| loop var reuses a function-local's name   | legal — same |
| inner loop var reuses outer loop var name | legal — same |
| two sequential loops reuse a name         | legal — same |
| loop var = reserved slot letter / `args`  | **SB5080** (DD#100 addendum) |

The reasoning was: the first four are one name with one meaning, and
rejecting them would reject working, bash-identical programs.

**Why that is not good enough.** "Bash-identical" is not DD#86's standard.
`int x=1; int x=2` is bash-identical too, and DD#86 rejects it. The DD's
standard is *one name, one declaration per scope*, which is deliberately
stricter than bash. The audit quietly swapped in bash-compatibility as the
test and got the answer that substitution implies.

Unrelated to scope but found in the same pass: `break`/`continue` with no
enclosing loop compiled to a **silent no-op** (the transform's `loop_stack`
was empty and `handle_break_stmt` discarded the statement). Now **SB5081**,
with a function body starting a fresh loop context.

## Addendum (2026-09-17, later) — `for` introduces a name that outlives the loop

DG, on reading the retraction above: *"I never sanctioned a for variable as
exempt from the no-shadow rule. Rather I recall saying that a for variable
survives the loop, making the for an implicit declaration, rather than a
non-declared special var."*

Measured, in sash and in bash:

```sh
for i in 1 2 3 { echo "$i" }
echo "after=$i"                  # after=3   (both)
```

The name outlives the construct. Whatever else it is, it is not scoped to the
loop — which is the property that makes calling it "not a declaration"
untenable. The current behaviour is the AST accident, not a model.

Three cases where the accident is visible today, all **currently accepted**:

| # | program | today | under "`for` declares" |
|---|---------|-------|------------------------|
| A | `int x=5` … `for x in 1 2 3 { … }` | accepted; `x` becomes 3 | SB5049 (redeclaration) |
| B | `for y in 1 2 {…}` … `for y in 8 9 {…}` | accepted | SB5049? (two declarations of `y`) |
| C | `for z in 1 2 {…}` … `int z=7` | accepted | SB5049 (the loop already introduced `z`) |

C is the sharpest: `int z=1; int z=7` is SB5049, but interposing a loop as the
first introduction makes it legal. A is the one with a live hazard — a
declared, typed variable is silently retyped and overwritten by a loop.

**Resolved by DG (2026-09-17), and the resolution is a single principle:**

> *"I don't think a variable should ever change type."*

Plus the mechanism that makes it liveable:

> *"the following should also create an int by assignment: `((x=7))`"*

### What that settles

A name's type is fixed at its **first introduction**, whether that is a
declaration or a binding. Bare and arithmetic assignments, `for` headers,
and DD#93's arithmetic-attest target all fix the type without a declaration.

| form | introduces | why |
|------|-----------|-----|
| `x=5` | **string** holding `"5"` | a word is a word; `$(( x + 1 ))` on it is SB4030 |
| `(( x = 7 ))` | **int** | the arithmetic context means a number |
| `for x in a b` | **string** | a word list binds words |
| `for (( i = 0; … ))` | **int** | the arithmetic header means a number |
| `attest s (( n = s ))` | **int scalar** | the validated conversion binds a number |

Two rules follow, and both were unenforced:

**SB4030 on a `for` binding.** A `for` header binds its variable exactly as an
assignment does, so it is type-checked exactly as one. `int x=5` then
`for x in a b` bound **0** each iteration in silence, while the equivalent
`x=hello` was already SB4030.

**SB5082, declaration after a binding.** Declarations are collected
program-wide, so a declaration reached *backwards* and retyped every read
above it:

```sh
x=hello
echo "$x"      # printed 0 — a value the program never wrote
int x=7
```

`(( x = 7 )); echo "$x"; str x="a"` was the same bug in the other direction
(`7` then `0`). A typed declaration of a name an earlier binding already
introduced is now rejected, pointing at declaring before first use.

The arithmetic-attest target participates in this rule too: an attest that
introduces `n` followed by `int n=7` or `str n=x` is SB5082. Its target may
already be an int scalar, but not a string or collection (SB4030).
These missing checks were repaired with the unified type map in journal
565 (`typeflow.4`-`.6`, `.19`).

### What stays legal

DG: *"if I declare a variable and then use that in the for loop, it should
work using the existing variable. In the same sense, if another loop uses a
variable from the prior loop, it should work. this seems consistent with the
way a variable can be instantiated by assignment without being declared."*

All three hold. Declaring then looping writes the existing variable; two loops
share one variable; instantiation-by-assignment is untouched. None of them
changes a type, so none is affected.

### The cost, and why it is not a restriction

Typing a word-list loop variable makes `for x in 10 20 30 { (( sum += x )) }`
SB4030. Measured: **two** sites in sash relied on it — `cf.f5` and one spec
written the same afternoon. (Eighteen other hits are in `suites/`, which is
bash's own test corpus, oils, and shellcheck's source — reference material,
not sash code.)

Both routes out already existed:

```sh
for x in 10 20 30 { (( sum += ${x:-0} )) }      # the disposition SB4030 names
for (( i = 10; i < 40; i += 10 )) { … }         # DG: the form that means this
```

`cf.f5` was rewritten to the second. The word-list header is for words; the
arithmetic header is for numbers.

Specs `spec/scope.sh` 86.39–86.58; journal 357.

## Addendum (2026-09-28) — a `$( )` body is an anonymous function

DG: *"sash doesn't have subshells - so changes to globals, for example,
survive unless a fork is required. […] no shadowing and no rebinding. […] The
variable snapshot/restore seems unnecessary - a `$( )` capture could be
modeled as an anonymous function; all new variables would be local."*

Two rules, both of them this DD's existing rules applied to a context that had
quietly exempted itself.

### 1. A capture body writes globals like any other code

The compiler used to synthesise an explicit save/restore around a capture body
— `collect_assigned_vars` / `snapshot_assigned` / `restore_snapshot` in
`transform/stmt_handlers.rs` — to emulate bash's subshell isolation. It is
gone. A `$( )` body is lowered inline into the enclosing SSA function (it
always was; the isolation was a separate, deliberate copy), and a write to an
existing global persists.

Three independent reasons, any one of which is sufficient:

- **It contradicted this DD.** A global is read *and written* with no friction
  from any scope. A capture body is not a scope boundary; nothing in the model
  said it was.
- **It contradicted itself.** Measured before the change:

  ```sh
  str x=one; fn f { x=two; echo hi }; str s=$(f)   # x is two — survives
  str x=one; str s=$(x=two; echo hi)               # x is one — restored
  ```

  The same effect persisted or vanished according to spelling. The function
  path was the one consistent with this DD, so the inline path was the bug.
- **The save set was not total.** `collect_assigned_into` handled only
  `Assignment` and `Declaration`, so `a[0]=v` and a `read` inside a body
  escaped the restore regardless. An isolation you cannot rely on is worse
  than none.

### 2. A name first introduced in the body is local to it

That is the other half of "anonymous function", and it is this DD's existing
rule for function bodies verbatim: a name first assigned inside a function
that is not a top-level global is automatically local. `str s=$(int y=5; echo
hi); echo $y` is SB5031.

### A deliberate bash divergence

This is one of the few places sash does not match bash. Bash forks a subshell
for `$( )` and the write is discarded; sash has no subshells, so it does not.
Two specs asserted the bash behaviour and were rewritten, not silenced:
`commands.sh` 14.3b and `errors.sh` csc.7.

### No rebinding, including by the bulk binders

"A name's type is fixed at its first introduction" binds `read`, `argv` and the
3-argument `match`/`matchall` as much as anything else: each *introduces a
collection*, so aiming one at a name already bound as a scalar is a rebinding
and is now SB4030. It used to be accepted, and accepting it is how a
`SashTable*` reached a scalar pointer slot — see
`notes/tasks/bug-autodeclared-collection-scalar-read-leaks-pointer.md`. Over an
`int` target it was a compiler ICE.

Specs `spec/scope.sh` 86.59–86.70, `spec/types.sh` 75.101–75.107; journal 434.

## Addendum (2026-09-29) — the body is validated as a function body

"A `$( … )` body is an anonymous function" was settled above for *scope*. It
has a second consequence, and leaving it unstated cost three nested bugs
(journal 444): **every statement legal in a function body is legal in a `$( )`
body, and gets the same validation.**

Both halves were false in the implementation.

**Legal.** Statement builtins with no value lowering — `:`, `write`, `cd`,
`wait`, `close`, `attest`, `unset`, DD#60's `argv N a` fill — resolved to
nothing as the sole body, and the backend built an exec argv whose head was a
bare builtin name. The program printed `sash: bug: bare command passed to
sash_exec` into its own stdout and exited 0.

**Validated.** Forty-two of the forty-five statement walkers in `validate.rs`
never entered a body, so `$(signal INT ignore)` panicked the compiler,
`$(mapfile -m q)` ICEd at runtime, and `$(break)`, `$(read -a "q")` and
`$(for args in …)` were accepted in silence.

### The IR contract this establishes

`Statement::child_bodies()` yields `structural_bodies()` ++ `cmdsub_bodies()`,
and the expression walker `walk_expr_tree` **stops** at `CommandSubstitution`.
A body belongs to exactly one side of the recursion, so a pass cannot visit it
twice and double its diagnostics.

For a new pass:

- no `ExprKind::CommandSubstitution` handling of its own → use
  `child_bodies()` and get the body for free;
- its own handling already → use `structural_bodies()` and keep it.

A walk that **mutates** scope state is the exception and must say
`structural_bodies()` regardless, until it opens a child scope for the body the
way `check_function` does — otherwise the body's local names leak outward,
which is precisely the rule this DD establishes. `type_check.rs` is pinned for
that reason, and `$(unset zz)` of a whole variable remains silently accepted
because of it.

### Condition context does not cross the boundary

`in_condition` resets to `false` entering a body. A body is a function body;
DD#70/#71's boolean contexts stop at it, exactly as they stop at `fn`.

Specs `spec/errors.sh` csc.11–csc.34; journal 444.

### Addendum (2026-09-29, later) — the type pass descends too

The walker-classification rule above is stated for passes in general, but the
pin it describes ("a walk that **mutates** scope state is the exception") was
read too broadly for `type_check.rs`. Two corrections:

- **`check_function` does not open a child scope.** It saves and restores the
  implicit `args` collection and the DD#100 slot kinds, and nothing else.
  Ordinary names declared in a function body are written into the shared
  `TypeEnv`. A `$( )` body is treated the same way, which is what "an anonymous
  function body" already implied — giving bodies their own name scope here
  would make them *stricter* than the construct they are defined in terms of.

- **The pin was never what blocked the descent.** `check_stmt` and
  `infer_stmt` now take `cmdsub_bodies()` unconditionally, after their `match`,
  with the `_` arm still on `structural_bodies()`. The arms that carry a
  substitution — `Command`, `Declaration`, `Assignment` — are leaves, so a pass
  that only widens its default arm still never enters a body. Any pass added
  here must descend unconditionally or it will have the same hole.

DD#30's `unset`-on-a-scalar is the witness: rejected at top level, in a
function body and in an `if` body, silently accepted inside `$( )` until this
change. Specs `csc.35`-`csc.40`.

## Addendum (2026-10-18, DG) — resolve lexical identities once

Sibling functions and sibling captures may reuse a local spelling, including
with different types. The shared, unqualified type map did not preserve that
rule: a string-array `items` in one function and an int-array `items` in
another printed **`4` then `7`**, instead of `word` then `7`, in both backends.
Cloning type environments alone would leave SSA and collection lookup with
the same ambiguous names.

DG chose centralized binding resolution and suggested function-name prefixes:

> "do 1; could prepend function name to locals..."

`bindings.rs` now gives each lexical local one internal identity, once, before
semantic checks, typing and SSA lowering. Named functions inherit globals,
not caller locals. Captures inherit their enclosing bindings and introduce
their own locals; structural control-flow bodies do not introduce scopes.
Attest-all binders belong only to their predicate. Generated identities
reserve source identifiers and disambiguate collisions.

All downstream passes share that resolved AST. The earlier addendum's claim
that ordinary names share the `TypeEnv` describes the old representation,
not permission to conflate lexical bindings. Separate environments are no
longer needed for uniquely resolved ordinary names; implicit `args` and
collection slots still receive their own function environments.

Resolution distinguishes variable/name operands from literal values and map
keys. Diagnostics and runtime attest labels recover source spellings;
`--emit-ir`, embedded IR and embedded source retain the original AST/source.
C output and manifests reuse the checked SSA rather than rebuilding it from
the original, unqualified AST.

Redeclaration tracking also opens a child scope for each capture: inherited
names still cannot be shadowed, but one capture's new locals do not escape
into its siblings.

SSA diagnostics restore the structured binding name **before** appending its
version. A formatted `qualified_name_1` can equal another binding's qualified
base name; replacing identifier tokens after formatting can therefore name the
wrong source variable, not merely leak an internal prefix. The backstop
receives the same source-name table as the other diagnostic paths.

Specs `spec/collparam.sh` `bindscope.1`-`bindscope.39`;
journal 525.

## Addendum (2026-10-24, batch-531 port step F13) — SB4044 covers every write

Phase 3 above says SB4044 "means only a bare assignment to a const". That
described the enforcement, not the rule: a const is immutable, so every
channel that binds the name writes it. Validate's check ran before binding
resolution, over flat names, in textual order, and enumerated only some
channels. `VersionState::define` caught some of the rest during lowering and
reported them at `1:1`. Measured on `1309385`:

- **Writes it missed or misplaced.** A `readline` stage and a `for` variable
  reached the lowering backstop, at `1:1`. A write in a function defined
  before the const (`fn f { x=2 }; const x=1`) compiled.
- **Writes that were not writes.** The backstop also fired on legal programs:
  a const bound on one path of an `if` (the merge's phi) or in a dispatch's
  default arm (the dispatch's merge keeps that arm's versions), in a function
  whose output is redirected, and in a self-tail-calling function, whose
  lowered template inlined the tail call instead of looping. The flat names
  did the same for a function's `const x` against another function's local
  `x`, or a sibling capture's.

DG approved the refinement with the rest of F13: validate's SB4044 covers
every binding channel, and the backstop becomes an invariant.

**The rule.** SB4044 rejects every statement that binds a const other than its
declaration: an assignment, a `for` variable, a `(( ))` write, a command
out-param (`readline`, `read`, `argv`, `mapfile`, a `keys` or `match` fill), an
attest's re-tag of its subject, and DD#93's conversion target. Every write is
reported, each at the innermost statement that makes it, whether that comes
before or after the declaration in the text (a binding does not depend on
textual order, above). Two channels stay with
type_check: `unset`, and the collection an `attest … in a` re-tags. Every
const is a scalar, and both are SB4030 on a scalar in either order. A second
declaration is SB5049, and a `const` in a loop body SB5072, as before.

**Where it runs.** `check_const_writes` runs first in the semantic batch, on
the program `bindings::resolve` returns, so a single program-wide set of
qualified names is exact. It sits outside the taint switch: `--no-taint` does
not carry a const write into lowering.

**The backstop is an invariant.** `VersionState::define` now panics if lowering
binds a const twice, and a unit test pins it
(`defining_a_const_twice_is_an_invariant_failure`). It is orthogonal to
validate: it sees lowering's own bindings, so it also notices lowering binding
a name twice by itself. That needed every lowering path that did so fixed
first. Both merge sites, a two-way branch's phis and a dispatch's, use
`define_merge`; a function's template runs its self tail call as a loop; a
captured function owns its tail loop (batch-531 B58); and the suppressed tail
specialization restarts from the readonly set its body was entered with.

**Decide-and-report.**

- A fill into a const was the type mismatch SB4030, because a const is a
  scalar. It is now SB4044, the more specific error (`constw.6`).
- SB4044 moved from the structural batch to the semantic one, which changes
  only which diagnostics accompany it. A program that also breaks a
  structural rule now reports that rule alone (SB5007, SB4025, SB4064), and
  one that reached lowering loses its lowering diagnostics (SB4018). SB4044
  now accompanies semantic-batch codes instead, such as SB5020 and SB5068.
  Every one of these programs was rejected before and still is.

Specs `spec/types.sh` 35.3-35.9 and `constw.1`-`constw.21`, `spec/glob.sh`
`forglob.3`; journal 532.

## Addendum (2026-10-29, batch-531 port F5): a `$( )` body starts a fresh loop context

"A body is a function body" (2026-09-29) now also covers the loop context.
A function body starts a fresh loop context (SB5081, 2026-09-17), and so
does a `$( … )` body. `for x in a b { str s=$(echo $x; break) }` is SB5081.
It used to compile, and the `break` branched out of the open capture, so
every later write was lost (B60). A loop wholly inside the body still
encloses its own `break` (cf.n21).

A body also no longer inherits the enclosing function's tail loop (DD27's
2026-10-29 addendum). The rule above, "condition context does not cross the
boundary", now holds where the transform lowers a body too (DD55's
2026-10-29 addendum).

Specs `control_flow.sh` cf.n19-cf.n21; journal 542.
