# 257 — the pending ICEs (and the ones nobody had filed)

DG: *"let's fix the other ICEs still pending."*

One task was filed. Probing for it found four more, plus two cases that were
worse than an ICE.

## Don't fix the report, fix the class

The filed P1 was `$(f)` as a `[[ ]]` operand panicking at `values.rs:81`.
Fixing just that would have been a mistake, because the panic sites are
enumerable: 13 `panic!("ICE: …")` arms in `codegen/`. Probing each took one
battery of thirteen scripts and found:

| trigger | site | filed? |
|---|---|---|
| `[[ $(f) = x ]]` (string fn) | `values.rs:81` | yes |
| `[[ $(f) = 1 ]]` (`-> int` fn) | `values.rs:85` | yes |
| `[[ $(echo a; echo b) = x ]]` | `values.rs:90` | **no** |
| `(( y = ${x:~} ))` | `values.rs:103` | **no** |
| `int n=512; echo ${n:1:2}` | `types.rs:29` | **no** |
| `int n=5; echo hi > $n` | `types.rs:29` | **no** |

Four of six were unknown, and every one is something a person would plausibly
write. The report was the tip.

## Worse than an ICE

The probe also turned up two cases that *don't* panic:

```sh
array a=(x y);   [[ @a = x ]]        # compiles — and is ALWAYS false
array a=(o1 o2); echo hi > @a        # compiles — writes to neither file
```

`v=@a` is correctly SB5025. The assignment path was the only one that checked;
the test operand and the redirect target accepted a splat and then quietly did
something indefensible. `[[ @a = x ]]` returning false when `a`'s first element
*is* `x` is a wrong answer with no signal at all.

I'd rather have the panic. A crash is a bug report; a false comparison is a
wrong result someone builds on. So these went in the same batch — same root
cause (*a value kind reaching a position whose lowering doesn't handle it*),
opposite failure mode.

## The one real decision

Everything except one case was settled by a rule sash already had: elide in
arithmetic → SB5046 (as on an assignment RHS), splat in a scalar position →
SB5025 (as on an assignment RHS), string op on an int → SB4030 (as `n =~ s///`
already is). Applying an existing rule to a position that was missed isn't a
design decision, it's finishing one.

The capture was different. `[[ $(f) = x ]]` could reasonably *work* (the
assignment form does) or be *rejected* ("assign it first"). The filed task had
deliberately left both open — "compiles **or** is rejected — never panics" — so
it went to DG. He chose make it work.

That turned out to be nearly free: `lift_int_func_calls` already handles
`FuncCall`, `IntFuncCall` and `CmdSubstBody`, and already recurses through
`Concat`/`ArithBinOp`/`Ternary`. All the test path needed was to call it. The
invariant `values.rs:77` asserts had always been true of the assignment and
argument paths; the test path simply never upheld it. `lift_test_captures` is
30 lines of structural recursion over `SsaTest` and no new machinery.

## Comments that assert, tests that don't exist

Two of the six had a comment at the panic site explaining why it couldn't
happen:

> `types.rs:19`: "The Int arm is unreachable from those sites; a hit means a
> codegen path built a slot of the wrong shape (a compiler bug, **not
> user-reachable input**)."

`int n=512; echo ${n:1:2}` is user-reachable input. Same story at
`values.rs:100`: "Reaching a scalar `lower_value` means validate's SB5046 guard
was bypassed" — which is exactly what `(( y = ${x:~} ))` did, because the elide
walker never descended into an `ArithExpr`.

This is journal 252's lesson for the third session running, and it now has a
sharper form. These comments aren't documentation, they're **claims about a
global property** — and a claim about a global property is only as good as the
walker that enforces it. Both were true of every position the author had in
mind and false of one they hadn't. The comment is what stops the next person
from checking.

## The gap under the gap

Wiring the elide check into arithmetic exposed a second-order hole:
`walk_arith_expansions` — the shared walker that `walk_expr_tree` and
`walk_all_exprs` both rely on to descend into `$(( … ))` — had no arms for
`ArithExpr::Subscript` or `SubscriptAssign`. Their children are arithmetic
subtrees, so **every** expression-level check silently skipped anything inside
`a[…]`. Not just elide: the DD#65 pruned-operator reject too.

That walker was itself added as a fix, with a comment recording the bug it
closed: *"Without this, `$(( ${s:?…} ))` bypassed the reject and ICE'd in
codegen."* The fix was right and incomplete, in the same way and for the same
reason as the thing it fixed. `(( y = a[${x:~}] ))` is now spec'd (ice.10).

## Two probe errors worth recording

**I nearly mis-diagnosed two cases as fixed.** My probe scripts assigned to a
variable without reading it, so DD#86's dead-variable check (SB5050) fired
first and masked the real behavior — `v=${n:1:2}` reported "reject SB5050" when
it was really an ICE. The harness was lying to me and agreeing with what I
wanted to believe. Printing the actual message instead of just the code caught
it.

**And one spec was wrong, not the compiler.** `ice.6` used `[[ $i -lt 2 ]]`;
sash rejects `-lt` inside `[[ ]]` (SB5019 — numeric comparison belongs in
`(( ))`). I wrote bash out of habit in a language that deliberately isn't bash.

## Also verified: the advice

Each new diagnostic tells the user what to do instead. I ran all four
suggestions rather than assuming: `s="${n}"` then `${s:1:2}` → `12`;
`> "${n}"` → writes the file; `$(join a)` and `${a[0]}` in a test → both work.
A diagnostic that prescribes something broken is worse than one that says
nothing.

## Sweep

4225 passing / 39 failing / 394 skipped. 0 regressions, +43 passing. The 5 new
failures are all janet, from a **pre-existing** defect the new specs exposed:
janet renders a user-function capture as `(os/spawn ["f"] …)` and crashes, and
truncates a multi-statement `$(…)` to its first statement — in the *assignment*
form too, so unrelated to this work. Filed as
`janet-command-substitution-capture-broken` and left failing rather than
skipped.

Also closed stale: `cmdsub-followups-taint-intrinsics-c` gate 1 (the
unblessed-external ICE) no longer reproduces — it yields a clean SB5020, fixed
by `tighten-cmdsub-taint-execve`.
