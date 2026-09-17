# 355 — The loop that ran once

Three bugs, all in loop lowering, all found by the same question: *what can
reach this cold branch?*

The thread started at `type_check.rs::check_len_operand`, four cold branches
adjacent to the `len`/`strlen` area that produced the last two bugs. Line 1058
is `env.is_slot(&name)` — a silent bail-out. `env.loop_vars` is populated only
by `check_for`, so the only way in is a **loop variable that is also a slot
name**. I wrote the program to see whether that was constructible:

```sh
fn f {
  for A in @B { echo "$A -> $(len A)" }
}
array a=(p q r)
array b=(xx yyy)
f =a =b
```

`xx -> 3`. One name with two live meanings — `$A` the loop variable, `len A`
the slot. That is bug one.

## The two I found by testing the fix

Bug one needed a nested-loop test case to check the slot/loop-var interaction,
so I wrote one that nested two splat loops. It printed two lines instead of
six. Different variables, different arrays, nothing to do with slots:

```sh
array b=(1 2 3); array c=(p q)
for x in @b { for y in @c { echo "$x$y" } }
```

`--emit-c` answered it in one read. A single `__ai_ptr`/`__ai_len` pair was
declared for the whole function and written by both loops:
`lower_for_array` named its induction variable with the hardcoded string
`"__ai"`. One name per compiler, not per loop. The inner loop reset it, ran it
to the inner array's length, and the outer header then compared *that* against
its own bound. `lower_for_argv_slice` did the same with `"__fi"`/`"__argc"`.

The fix is `format!("__ai{}", header)` — the header block id, already unique.
It is the convention the file next door already uses (`__args{counter}`,
`__bodycap_{blocks.len()}`), which is a small embarrassment: the pattern for
doing this correctly was three files away.

Then, checking `break` inside the now-working nested loops, bug three. `break`
printed `1 2 3`. Not a nesting artifact — `break` was inert in *every* splat
loop, including a single unnested one. `lower_for_array` and
`lower_for_argv_slice` never pushed `state.loop_stack`, and
`handle_break_stmt` guards on `depth > 0` and otherwise returns the block
unchanged. No diagnostic. The statement is discarded.

Fixing that one required moving the index increment into a **latch** block.
`continue` jumps to the continue-target, and with the increment at the end of
the body a `continue` would skip it and spin forever. The latch holds the
increment, `continue` targets the latch, `break` targets the exit, and the
latch becomes the back-edge predecessor for phi insertion.

## What the audit changed

DG chose the general form for bug one: extend no-shadowing to loop variables
versus any declared name, plus audit every construct that introduces a name
without a declaration. The audit inverted the answer.

`DD86-variable-scope.md` says, in the model section: **"A `for` variable is not
a declaration and is untouched."** So a loop variable reusing a global's name,
a local's name, or an enclosing loop variable's name is deliberate — and in
each of those cases the name has *one* meaning, one variable overwritten,
byte-identical to bash. The general rule would have rejected working,
bash-compatible programs and contradicted an accepted DD.

What is genuinely broken is narrower and has its own source of truth. DD#100
Part B reserves `A`–`E` "on the `args` precedent: rejected as user variables at
top level and inside functions". A for variable *is* a user variable; it just
is not a *declaration*, so it fell between DD#86's exemption and SB5052's
write gate. SB5080 enforces the reservation. That is the rule that was already
decided; nothing new was invented.

I reported this back rather than implementing the general form as instructed,
because the instruction was conditioned on an audit whose result reversed it.

## The gate that could not fail

`break` outside any loop hit the same empty-stack path — accepted, discarded,
silent. DG: reject. SB5081, with a function body starting a fresh context.

## Why all three survived a 5404-test suite

```
$ grep '^\s*break\b' spec/*.sh
$
```

Nothing. Not one spec in the suite executed a `break`. Nested splat loops were
equally unrepresented. The suite was green the entire time and stayed green
through all three fixes.

This keeps being the lesson. The coverage push is not producing better
coverage numbers so much as it is producing a list of things nobody ever ran.
Four of the last six bugs came from pointing at a cold line and asking what
reaches it — and two of those, today, came from *writing the test case for the
first one*. The fix for bug one needed a nested loop; the nested loop was
broken; testing the nested loop's `break` found the third. None of that is
visible from reading call sites.

18 specs, `cf.n1`–`cf.n18`. Predicted 5404 + 36; got 5440.
