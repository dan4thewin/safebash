# 444 — Forty-two of forty-five

Three bugs, nested one inside the next. The first was reported as a dropped
statement in a `for`-in header. The third turned out to be a structural gap in
how the compiler recurses, and closing it moved `$( … )` bodies from the
expression side of the IR to the statement side.

## The first bug: `$( )` classified by `stmts.first()`

`validate.rs::cmdsub_command_is` decided what a substitution *was* by looking
at one statement and one command name:

```rust
let inner = match stmts.first().map(|s| &s.kind) { … };
matches!(inner, StmtKind::Command(c) if expr_literal_str(&c.name) == name)
```

`stmts.first()`, not "the only statement". So `for x in $(argv;
/usr/bin/touch /tmp/SIDE_EFFECT)` compiled clean, looped correctly, and never
created the file. And because the test was on the *name*, it could not tell
DD#60's two `argv` shapes apart — the splat expression `$(argv N)` from the
out-param fill statement `argv N a` — so `$(argv 0 b; /bin/echo @{b:-none})`
was rejected with SB5025, a diagnostic whose advice is to write what the
author had already written.

DG chose (a): the fill statement is legal inside a body. It is a statement,
DD#86 makes a body a function body, and a statement is legal in a function
body. The splat now requires a sole statement; the fill is discriminated by
its trailing bare name.

## The second bug: `None` meant "external"

Fixing the first exposed the next. `resolve_single_cmdsubst` returned
`Option`, and `handle_command_substitution` read `None` as "not a builtin, so
exec it". About sixteen builtin forms have no *value* lowering — `:`, `write`,
`cd`, `wait`, `close`, `attest`, `unset`, the `argv` fill — so each one, as
the sole body, built an exec argv whose head was a bare builtin name. The
program printed

```
sash: bug: bare command passed to sash_exec
```

into its own stdout and exited 0. A compiler that announces its own bug in the
user's output stream and then reports success is worse than one that crashes.

Fixed by asking first: `is_statement_builtin` (total over `BuiltinCmd`, no
wildcard) routes the whole cohort down the body path before resolution is
consulted at all.

## The third bug, and the number that decided the shape

`$(signal INT ignore)` still panicked the compiler. `$(mapfile -m q)` still
ICEd at runtime. `$(break)`, `$(read -a "q")`, `$(for args in …)` were still
silently accepted. Each traced to a *different* pass, which is the tell that
patching them one at a time would be patching symptoms.

So I asked the index how general it was:

> **42 of the 45 `child_bodies()` walkers in `validate.rs` had no cmdsub
> descent.**

Body *expressions* were reachable — `walk_expr_tree`'s `CommandSubstitution`
arm went through `walk_each_stmt` into `walk_all_exprs`. Body *statements*
were not, because `child_bodies()` did not yield them. That asymmetry is the
whole bug class, and it also constrains the fix: any patch that adds statement
coverage without removing the expression route emits every expression
diagnostic twice, which is journal 308's failure mode.

I put three shapes to DG — patch the five demonstrated gaps, add a second
`$( )`-body pass, or move bodies to the statement side — with the 42/45 number
and the duplication constraint. He chose **(c), the structural move**.

## What (c) is

`Statement::child_bodies()` is now `structural_bodies()` ++ `cmdsub_bodies()`,
and `walk_expr_tree` **stops** at `CommandSubstitution`. Bodies belong to
exactly one side of the recursion. A walker that says `child_bodies()` gets
body coverage for free; one that already handles `$( )` itself says
`structural_bodies()` and keeps its own handling.

`cmdsub_bodies()` collects through `walk_own_exprs`, the one total expression
walker, rather than a hand-written match. That matters: the hand-written
`cmdsub_expr_walk` it replaced had arms for Concat, DoubleQuoted, Array and
Subscript and nothing else — so a substitution inside a `[[ … ]]` operand, a
`${…}` default, an arithmetic expansion or a `SlurpFile` was never visited at
all. csc.28 pins the test-operand position.

## The audit, stated as a rule

Every caller of `child_bodies()` had to be classified, because the widening is
silent where it is wrong:

> **A pass that already has its own `ExprKind::CommandSubstitution` arm keeps
> it and takes `structural_bodies()`. A pass with no cmdsub handling takes
> `child_bodies()` and gets the body for free.**

Four passes kept their own handling and were pinned, each for a reason worth
recording:

- `check_stmt_exprs_and_bodies` — `check_cmdsub_reads` opens a child scope.
  Double-visiting used the *enclosing* scope and reported
  `s=$(argv 0 b; echo @b)`'s `b` as introduced nowhere.
- `taint_child_bodies` / `stmt_runs_taint_source` — `check_expr_taint` already
  reaches a body via `check_cmdsub_use_taint`.
- `check_stmt_splat_operand` — `check_cmdsub_splat_operand` passes
  `in_cmdsub = true`, and a scalar-operand intrinsic is legal there and
  nowhere else.

All eleven non-validate callers (`resolve.rs`, `transform/capture_purity.rs`,
`transform/functions.rs`, seven in `type_check.rs`) were pinned to
`structural_bodies()` so the blast radius stays inside `validate`.

## Two things the sweep caught that reasoning had not

**The argv collision.** `check_fill_target_in` began entering bodies and read
the `1` of `$(argv 1)` as a fill target — SB5063 on the canonical
`for x in $(argv 1)`, sixteen failures across seven specs, all one cause. DD#60
gives `argv` two shapes that collide at exactly one argument, and now two
passes needed to discriminate them, so the predicate is shared:
`cmdsub_is_argv_splat`. Any future pass that walks into a body consults it
rather than re-deciding.

**Spec 86.70 was a tautology.** `str s=$(if [[ 1 == 1 ]] { x=two }; echo hi)`
had compiled only because nothing walked into the body; SB5086 rejects a
comparison the source text already settles. The spec was testing that a nested
write persists, so the condition became `[[ $g == 1 ]]` and the point survives.
A spec that stops compiling because a rule finally reaches it is the rule
working.

## The regression I introduced, and how it was found

`validate_stmt_ctx`'s leaf arms — `Command`, `Declaration`, `Assignment`,
`ArithTest` — never recurse at all. So `$(signal INT ignore)` under a
declaration was *still* unreached after (c), and worse: pre-(c),
`walk_own_exprs_in_stmt` had reached body expressions by descending through
the substitution node, and (c) took that away. `$RANDOM`, `$HOME` and `**`
inside a body compiled clean for one build.

**The full spec suite did not notice.** 6913 green rows and not one of them
put a rejected variable inside a `$( )`. Only writing csc.19 — a spec for a
bug I believed was already fixed — surfaced it, because the compiler still
panicked.

The fix is the same structural move applied one level up: `validate_stmt_ctx`
descends into `cmdsub_bodies()` unconditionally, for every statement kind, and
`validate_nested` drops to `structural_bodies()` so wrappers are not visited
twice. `in_condition` resets to `false` crossing into a body — it is a function
body, and the condition context does not cross that boundary.

csc.34 is the guard: an `if` inside a `while` inside a body, asserting SB5017
appears exactly **once**. Journal 308's failure mode multiplies with depth, so
a count of one at depth three is worth more than an eyeball at depth one.

## What it cost, and the gate

The complexity gate rejected six functions. Two I had materially changed
(`check_fill_target_in` at cog 10, `check_argv_form_cmdsub` at cog 6); four
were pre-existing violators whose only edit was the one-token
`child_bodies` → `structural_bodies` substitution — grandfather-on-modify
doing exactly what it is for.

All six were the same shape: a nested double loop inside a match arm, where
the cost is the nesting and not the logic. Flattening with
`.into_iter().flatten()` or extracting the body half into a named helper
cleared every one. Four of those helpers now carry the *reason* they take
`structural_bodies` — `check_stmt_bodies` and `infer_stmt_bodies` mutate
`env`, so a body descent would leak body-local names into the enclosing
scope; `collect_returns_bodies` must not attribute a body's `return` to the
enclosing function.

## Lessons

- **Compiling is not verifying.** Every bug in this entry produced a
  successful compile. Run the binary.
- **A number decides a shape.** "Five gaps" and "42 of 45" call for different
  fixes, and the only way to tell which you have is to ask the index.
- **A green suite is evidence about what it covers, not about what you
  changed.** 6913 rows missed a silently-accepted `$RANDOM`. The spec that
  found it was written for a different bug.
- **Predict the total first.** 6913 + 24 specs × 2 modes = 6961, and 6961 is
  what came back. A number you did not predict is a finding.

## State

`mk test` green. Full sweep green at **6961** — 3583 native, 3365 c, 13
skipped — exactly the predicted total, no retirements. `cargo fmt`, clippy,
the totality gate and the complexity gate all clean.

## Filed, not fixed

- `$(unset zz)` of a whole variable is still silently accepted.
  `check_unset_args` lives in `type_check.rs`, whose walk mutates `env`; a
  descent there needs a child scope the way `check_function` has one.
- `match S P ARR` (3-arg) inside `$( )` is SB4045, though DD#86's principle
  says it should be legal.
- `arity_too_many_msg` prints `min` where it means `max`.
- `walk_scoped_stmts` now enters bodies, affecting thirteen callers. Green,
  but not individually audited.

---

# 444b — clearing the filings, and the one that came back a regression

Of the four items filed at the end of 444, three needed no decision from DG and
one turned out to be a bug I had introduced the same day.

## `arity_too_many_msg` — a diagnostic that stated a false number

`fixed_arity_msg` passed `min` into a message that renders it as "takes N
arguments". Invisible while min == max, which is why it survived: it is wrong
only for a DD#84 callee with trailing optionals.

```sash
fn f { /bin/echo ${args[0]} ${args[1]:-d} }
f a b c
→ SB5043 function 'f' takes 1 argument but was called with 3
```

`f` takes one argument **or two**. The message denies that `f a b` is legal.
It now reports the range through `arity_range`, which already existed for
SB5087's dynamic-count message: "takes 1 to 2 arguments", and "takes exactly 1
argument" when they coincide. Specs 81.30, 81.31.

## The `match` filing was wrong, and I had generalised from one probe

Filed as "the 3-argument statement form is legal everywhere but inside
`$( )`". Measured:

| position | `match S P m` |
|---|---|
| top level, status unconsumed | SB4018 |
| top level, `\|\| :` | accepted |
| function body, `\|\| :` | accepted |
| `if match S P m; then …` | accepted |
| `$(match S P m \|\| :)` | **accepted** |
| `$(if match S P m; then :; fi)` | **accepted** |
| `$(match S P m)` bare | SB4045 |

There is no legality gap. The statement form works in a body exactly as DD#86
says. `matchall` was never affected — it reports SB4018 in every position.

I had probed the bare form, seen SB4045, and written a task asserting a
position-dependent rejection without checking the consumed forms. That is
journal 269's finding — nine of nineteen task filings materially wrong —
reproduced inside a single day. The task is rewritten as what remains: the
bare form's *wording*, since `check_cmdsub_arity` looks up a single
`value_intrinsic_arity` and says "a value intrinsic must be called with its
exact arity", a sentence that denies the statement form exists.

## The `walk_scoped_stmts` audit found a real regression

Thirteen callers, of which four consume `in_fn` directly. The audit had to
answer two different questions about them, and conflating the two is what made
the bug hard to see:

**Is `in_fn` the right *name*?** Yes, and measurably so — a `$( … )` body
shares the enclosing function's arguments:

```sash
fn g { str s=$(/bin/echo ${args[0]}); … }
g hello    → hello
```

So attributing a slot or a call inside a body to `g` is attributing it truly.
`collect_slot_calls`, `collect_slot_fills` and `flag_nested_fns` are all
correct under the widening — a `fn` written inside `$( )` genuinely is not at
the top level, and SB5071 applies exactly as it does inside an `if`.

**Is `in_fn.is_empty()` the right test for "top level"?** Not always, and this
is where it broke. `check_collection_slots` survives it — a `=A` slot read in
a top-level body is outside every slot scope, so flagging it is right. But
`collect_global_names` builds a *set of names in scope*, and a name first
introduced inside a body is local to that body (DD#86). Entering the body put
it in the global set, and then:

```sash
str s=$(str y=v; /bin/echo "$y")   # y is local to this body
fn f { str y=$(getenv HOME); attest y [[ -n $y ]] }
→ SB5067 `f` taints the global `y`
```

There is no global `y`. The identical shape with `fn h` instead of `$( )` is
clean, which is the tell. **A regression introduced by this session's own
change**, caught by auditing it rather than by the suite.

Fixed with `walk_scoped_structural_stmts` — the same walk, over
`structural_bodies()`. The rule it encodes is narrower than "readers may
enter, mutators may not": **a visitor that builds a set of names in scope must
not enter a body, whether or not it mutates anything.** Specs cov.340-343,
81.32.

## Lessons, again

- **Compiling is not verifying — including when the spec is the thing being
  written.** cov.340 and cov.342 failed their first sweep because I had probed
  that the program *compiled* and asserted an output I never ran. The program
  printed `$HOME` on a second line.
- **A filing is a claim, and claims get measured.** One of my four was simply
  false, and I found that only by probing the positions I had not probed the
  first time.
- **An audit is worth doing even when the suite is green.** 6961 green rows
  did not contain a name introduced only inside a `$( )` body.

## State

Sweep **6975** — 3590 native, 3372 c, 13 skipped — the predicted total.
`mk test` green, all gates green.

Left open: `bug-unset-whole-var-in-cmdsub-unvalidated`, the one filing that
survived contact. It needs `type_check` to open a child scope per body, and
its blast radius wants measuring before it is attempted.
