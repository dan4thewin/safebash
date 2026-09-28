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
declaration or a binding. The three ways to bind without declaring — a bare
assignment, an arithmetic assignment, and a `for` header — all fix the type,
and all are now treated alike.

| form | introduces | why |
|------|-----------|-----|
| `x=5` | **string** holding `"5"` | a word is a word; `$(( x + 1 ))` on it is SB4030 |
| `(( x = 7 ))` | **int** | the arithmetic context means a number |
| `for x in a b` | **string** | a word list binds words |
| `for (( i = 0; … ))` | **int** | the arithmetic header means a number |

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
