title: "DD#86 — Variable scope: no `local`, no shadowing, use-before-and-after"
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
- **Every body entry starts a fresh private lifetime.** Strings and integers
  begin at `""` and `0`; arrays and maps begin empty (DD#62). This includes
  repeated calls, self-tail iterations and anonymous capture entries.
  Captures preserve bindings owned by an enclosing live body; globals,
  arguments and collection slots are not new private locals.
- **`local fn` (DD#18) is abandoned** — it was the only other user of `local`,
  and "designed, not built."

## Provisional immutable scratch bindings (DG, 2026-11-09)

DD131's separate-branch trial now gives scalar and collection scratch
constructions fresh **immutable instances**. The same source site may
execute again, beginning a new lifetime without the preceding value or
proof; arbitrary writes and second declaration sites remain forbidden.
The leading-underscore notation is provisional, with the exact scalar
declaration spelling still to be finalized.

Positional visibility and no shadowing remain. Freshness at construction
does not create block scope, reconstruct a skipped site, or imply
destruction at loop exit. Readers, for binders, conversion targets and
array/map constructors follow the same model. Mutability restrictions
cover collection contents and A-E aliases, not just the source name.
This is not general permission for ordinary const declarations in loops
or for ordinary writes to remove unresolved taint.

DD132 adds regex proof and the caught-scratch rule: ordinary reads only
in the attest-success arm; outside it, only explicit trusted data reads,
never trusted pattern operands. Failed scratch may remain unresolved
without being replaced or abandoned. This exception is not granted to
ordinary consts. `q-glob-semantics` owns the integrated trial and DD revision
inventory; neither model is implemented by this recording.

## Function names: one definition each (DG, 2026-11-06)

A function name has exactly one source definition in a program. A second
definition is a compile-time error, even if its body and signature are
identical, its return kind is unchanged, or neither definition is called.
There is no function replacement or overloading by return kind, scalar
arity or collection signature.

Function names remain global and hoisted; definitions remain legal only
in the program's main statement list (SB5071). Calls before or after the
single definition refer to that same function. The rule does not change
variable scope, anonymous capture bodies, generated implementation
functions or DD#27's permitted self-tail calls.

DG chose rejection over same-signature replacement or changing definitions.
One name therefore denotes one body, one result kind and one inferred
signature, without a textual "definition in force" choice.

**Enforcement is pending** in `reject-duplicate-function-definitions`.
That task also removes the accepted duplicate-definition programs behind
`bug-int-call-by-name-redefinition`; neither task is complete merely because
the policy is settled. DD#55's former definition-in-force contract is
replaced by this rule. Journal 582 records the decision.

Function-name spelling is separately settled by DD#125 (2026-11-09):
the ASCII variable-identifier grammar, with existing reserved-name rules.
Invalid definitions must reject even if unused; no path/numeric-name
lookup exception remains. Enforcement is queued in
`bug-function-name-shadows-command-path`, independently of duplicate
definition rejection.

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

## Const freezes the value, not validation (DG, 2026-11-09)

> yes, separate immutability from validation

A `const` may be initialized from an otherwise legal taint source.
Immutability does not make that input clean. An otherwise legal `attest`
may validate the const and clear its taint without changing its value:

```sash
const home=$(getenv HOME)
attest home -n
echo "$home"
```

The rule is uniform across full predicates, unary shorthands, standalone
attests and legal caught forms. It also permits a const as the **source**
of DD#93's arithmetic conversion: the source's bytes and type are
unchanged, while a distinct target receives the computed integer.
Creating a new internal SSA version to carry the successful verdict is
not a source-level reassignment.

Actual writes remain forbidden. Assignments, mutations, command out-params
and conversion **targets** still obey SB4044; a failed attest does not
permit replacing its const subject with a default. Same-name conversion,
collection consts, redeclaration and const-in-loop restrictions are not
relaxed.

DD#121/DD#127/DD#128 still govern adjacency, ownership and resolution of
fall-through paths. Being an ordinary const grants no late or
inherited-input attest, no batch resolution, and no new clean-read
permission on failure. `:!` remains an explicit data-read override,
not validation of the binding or a regex
proof (DD132). The provisional scratch exception above is separate.
Signal deferral and EXIT/ERR entry requirements continue to follow the
actual legal resolution.

**Enforcement is pending** in `q-attest-retag-of-const`. The current
compiler still rejects the retag with SB4044. This supersedes F13's
retained retag rejection, not its real-write checks. No new declaration
syntax, diagnostic code or lowering architecture is selected. Journal 602
records the ruling and the implementation boundary.

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
   `const`. Main, EXIT and ERR share the global scope (DD#128). SB4044 no
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
6. Fresh per-entry private values. **— completed in journal 568 (B14).**
   Lowering uses the canonical lexical scope inventory and the type checker's
   actual scalar/element types and collection kinds. One body-entry hook
   emits real zero-value assignments for newly owned locals and starts their
   fresh taint lifetime. Named bodies, templates and anonymous captures share
   it; self-tail back edges execute it again. Active enclosing locals, globals
   and slots are preserved. `flife.1`-`.8` cover values and ownership;
   `adjssa.16` pins inherited private taint independently of the source gate.

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

## Evaluation order (DG, 2026-11-06)

DG chose **left-to-right**, applying the principle of least surprise
(journal 580). Words and their component expressions are evaluated in
source order, not in a separate captures-first pass. This applies across
an argument list and within one interpolated word, test operand or reached
dispatch pattern alike.

An earlier read takes its value before a later capture runs. The capture's
effects are visible to later reads, but cannot retroactively change the
value already read:

```sh
str g=old
fn f { g=new; echo X }
echo "$g" "$(f)" "$g"     # required result: old X new
```

Putting those expansions into one quoted word does not change their order.
The same principle applies to value-producing function calls, not just
stdout captures.

Existing control-flow and body boundaries remain: an unreached lazy operand
or dispatch pattern does not run; a stage's operand evaluation stays in its
child; captures do not acquire subshell isolation. This changes neither
collection-identity passing nor the rule that same-process global writes
survive. DD55's first-expression rule for `$?` follows the same order.

**Implementation is pending** in `q-capture-hoisting-order`, now a bug task.
The current compiler still prints `new X new` for the example. Removing
capture hoisting must preserve earlier values, not merely visit their
expressions earlier while leaving their reads deferred.

### Arguments before attached redirects (DG, 2026-11-07)

DG chose **arguments first**, uniformly for standalone commands and
pipeline stages (journal 585):

1. Evaluate and retain the command's arguments, following the left-to-right
   word/component rule above.
2. Process its attached redirects from left to right. Evaluate each target
   at that redirect's turn, then apply the redirect; stop at the first
   failure.
3. Run the command or enter the called function body only after its
   redirects succeed.

A redirect target therefore sees effects from argument evaluation, while
an already-evaluated argument cannot change because of later work. DD119's
target restrictions remain: this adds no command substitution or intrinsic
in a filename target.

The command's own attached redirects do not affect its argument captures.
Redirects already active in an enclosing scope still apply, and a capture
retains its own capture routing and inner redirect scopes. For a forked
stage, arguments still run in the stage child, after pipeline plumbing but
before that command's attached redirects. Do not move them to the parent.
DD122's parent-process readers and lexical pipe ownership are unchanged.

Failure during argument evaluation follows its own checks and no-reach
rules, before any attached redirect runs. A later redirect failure does
not undo completed argument effects; DD130 still governs recovery and
ordered partial setup.

**Implementation is pending** in `q-stage-capture-redirect-order`, now a
bug task. The current compiler installs a stage's attached redirects before
its argument captures, unlike standalone commands. This phase-order repair
is distinct from the within-argument repair in `q-capture-hoisting-order`;
neither task may weaken the other's contract.

DD55's 2026-11-09 single-write rule applies these phases to `echo` and
`write`: prepare stable argument values before publishing the complete
output request. A failing later argument must not leave the builtin's own
prefix on stdout or in its redirect target. This does not roll back an
argument's independent effects. `bug-echo-partial-line-before-abort` owns
that output repair; assembling one request alone does not repair
captures-first evaluation or unstable earlier reads.

## Addendum (2026-09-29) — the body is validated as a function body

"A `$( … )` body is an anonymous function" was settled above for *scope*. It
has a second consequence, and leaving it unstated cost three nested bugs
(journal 444): **ordinary function-body statements are available in a `$( )`
body and get the same validation, subject to the capture's result and
process-replacement restrictions.** DG's restrictions on explicit status
returns (2026-11-06) and process-replacing `exec` (2026-11-08) are stated
below. Status returns remain owned by their lexical body; the `exec`
restriction also follows calls into other functions.

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

### A capture body does not return a status (DG, 2026-11-06)

An explicit status-producing return from an anonymous capture body is
an error (DD#55), including zero, status forwarding and redirected
producers. Bare `return`, fall-through and compatible void forwarding
remain legal. The rule follows the return's owning body: it does not
forbid status returns inside a named function called by that body.

Use a named status function and its existing explicit handling/discard
forms when that result is needed. This adds no capture-level status
consumer and changes neither lexical scope nor enclosing-binding access.
Enforcement is pending in `reject-anonymous-capture-status-returns`;
journal 584 records the decision.

### No process-replacing exec in a capture (DG, 2026-11-08)

DG chose **"reject exec within capture"**. A source-level
`exec PROGRAM ...` is a compile-time error inside a **stdout-capture
body**, directly or through one or more called functions. This includes
anonymous bodies and captures of named functions. A helper's return kind
does not exempt an `exec` reached while evaluating such a body.

The restriction follows the body, not the capture's placement in an
assignment, argument or interpolation. Conditional paths, nested captures,
redirects, suppression and forked child bodies do not provide exceptions.
It is a structural rejection, independent of taint checking, not a claim
that every particular process arrangement would lose the captured output.

The distinction is the **source command**, not the `execve` syscall.
Ordinary external commands inside captures remain legal: their execution
forks a child and replaces that child with the external program. The
capturing process remains to collect the output and check completion.
The fd-only form, such as `exec >file`, does not replace a process and
retains its existing descriptor semantics.

This does not turn a capture body into a subshell or introduce an automatic
fork to accommodate `exec`. Same-process global effects keep this DD's
existing rules. Nor is it a blanket ban on non-returning paths: DD128's
`exit` and `fail` contracts remain unchanged.

Apply the semantic body/value distinction, not a test for `$()` punctuation.
A direct DD58 integer-value call is not a stdout-capture body merely
because it is written `$(f)` rather than as an arithmetic call; redirects
do not change its result kind. An integer helper called *inside* a stdout
capture is still subject to the body's `exec` restriction.

**Accepted, implementation pending** in `q-exec-in-capture`, retained as
an open P3 implementation bug. Journal 587 records the decision. Existing
acceptance of the process-replacing forms is a gap, not an exception.

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

### Status entry is fresh too (DG, 2026-11-07)

DG chose **no inheritance** of the caller's status by an anonymous capture
body (DD55, journal 586). The body starts without permission to read `$?`;
its own handling flow can establish that permission. Each nested capture
body has the same fresh boundary, regardless of the outer expression's
position, redirects or marks.

This is not isolation of ordinary bindings. A status saved into an
enclosing binding at a legal handling position remains accessible under
this DD's lexical rules. Global writes still survive in the same process.
DD126's fresh-function latch context applies as well: `$!`/`getcpid` and
`waitstatus` need a producer in the body's own flow.

Distinguish executable bodies from direct intrinsic value expressions.
`$(status)` is the same current-context read as `$?`, not a new body;
the same distinction preserves direct `$(getcpid)` and `$(waitstatus)`.
A getter inside an executable capture reads that body's context instead.
Neither evaluating captures earlier nor inventing an initial zero is a
substitute for the required refusal.

Enforcement remains pending in `bug-sb5012-sb5094-nested-body-flow`.
The separate return-kind, left-to-right and redirect-timing contracts are
unchanged.

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
actual write channel needs checking. A value-preserving attest retag is
not such a write under the 2026-11-09 ruling above; F13 retained its old
rejection while that question was deferred. Before F13, validate ran
before binding resolution, over flat names, in textual order, and enumerated only some
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
the then-enumerated binding channels, and the backstop becomes an invariant.

**The rule, with the 2026-11-09 validation distinction.** SB4044 rejects
every actual write to a const other than its initialization: an assignment,
a `for` variable, a `(( ))` write, a command out-param (`readline`, `read`,
`argv`, `mapfile`, a `keys` or `match` fill), and DD#93's conversion target.
An attest's value-preserving retag of its subject is allowed; removing the
current rejection is queued in `q-attest-retag-of-const`. Every real write is
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

**The current backstop is an invariant.** `VersionState::define` panics if
lowering binds a const twice, and a unit test pins it
(`defining_a_const_twice_is_an_invariant_failure`). It is orthogonal to
validate: it sees lowering's own bindings, so it also notices lowering binding
a name twice by itself. That needed every lowering path that did so fixed
first. Both merge sites, a two-way branch's phis and a dispatch's, use
`define_merge`; a function's template runs its self tail call as a loop; a
captured function owns its tail loop (batch-531 B58); and the suppressed tail
specialization restarts from the readonly set its body was entered with.
The queued retag change must distinguish validation versions without
weakening this guard against real writes or making the binding mutable.

**Decide-and-report.**

- A fill into a const was the type mismatch SB4030, because a const is a
  scalar. It is now SB4044, the more specific error (`constw.6`).
- SB4044 moved from the structural batch to the semantic one, which changes
  only which diagnostics accompany it. A program that also breaks a
  structural rule now reports that rule alone (SB5007, SB4025, SB4064), and
  one that reached lowering loses its lowering diagnostics (SB4018). SB4044
  now accompanies semantic-batch codes instead, such as SB5020 and SB5068.
  At F13's landing, every one of these programs was rejected before and
  after; the later const-validation allowance is separate.

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
