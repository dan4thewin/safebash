# 367 — The redirect scope, and a loop that would not end

**Sweep 5629, 0 failing** (5605 + 24, predicted exactly). Units green.
Clippy clean. Complexity gate clear on every touched function.

## SB4021 pointed at the broken path

Journal 366 filed the P1: `f > out.txt` compiles, prints to stdout, and
never creates the file. DG chose option A — **support it** — and the
strongest argument for that arrived while probing SB4021, whose message
reads:

> a redirect on a compound command isn't allowed — wrap the compound in
> a function

So `{ …; } > f` is rejected *and the compiler directs the user to the
form that silently fails*. Rejecting `f > file` would have made SB4021 a
dead end: both spellings refused, the second one only after the user did
what the first one told them to. Its sibling promises hold up — `f | cat`
works, and `f && x` needs only DD#37's return-on-every-path — so the
redirect was the one broken referral.

## One scope, not one redirect per command

The measurement that killed every shortcut:

```sh
fn f { echo a; echo b }
f > out.txt
```

bash writes **both** lines. Attaching the redirect to each command in the
body would open `out.txt` twice, and the second `>` truncates the first
line away. A function body is a *compound* body, so the redirect has to
be a scope: open once, restore once, around the whole inlined body.

That is the same problem `$(f)` already solved. `CaptureStart`/
`CaptureEnd` bracket an inlined body across basic blocks with per-site
slots, because captures nest and one shared slot lets an inner capture
clobber the enclosing one. `RedirectPush`/`RedirectPop` is that shape
copied deliberately, down to the per-site slot width — `fr.8` pins the
nesting case that a shared slot would break.

Two decisions inside it, both mine and both reversible:

- **The skip branch is always emitted**, even when the scope is checked.
  `mark_unchecked_in_conditions` and the END-block pass rewrite `checked`
  *after* lowering, so any shape conditioned on `checked` would be wrong
  by the time those passes ran. The extra edge is unreachable at runtime
  in the checked case.
- **`checked` is computed in the transform** from `state.in_condition`,
  not from `ctx.in_condition`. Because the scope's block now always ends
  in a Branch, `condition_inst_index` would otherwise report every scope
  as sitting in a condition.

## `--emit-c` localised the bug in one command

The first end-to-end run still ignored the redirect. The C backend's
output was *exactly right* — save, open, guard, branch, body, restore —
which proved the transform correct and put the fault in native codegen
alone. One command, one bisection.

The fault: `codegen/setup.rs`'s literal `Interner` never scanned
`RedirectPush`'s targets, so `"out.txt"` was not interned, and
`emit_open_redirect` **silently skips a redirect whose literal is missing
from the map** (`literals.get(s)?`). A missing intern degrades to a
dropped redirect with no diagnostic — the same failure mode as the P1
being fixed, one layer down. Worth remembering that the map lookup is a
`?`.

## Then END would not stop

Probing END-block parity turned up a **separate pre-existing bug**: a
redirect-open failure on a *plain command* inside `END { }` never
terminates.

```sh
echo main
END { echo hi > /nonexistent/x; echo tail }
```

rc=124 under `timeout`, 380,000 identical error lines. The uncaught path
jumps to the END cleanup block — and from inside END that is a jump to
itself.

`mark_unchecked_in_end_blocks` exists to prevent precisely this; its own
comment says a failed cleanup step "must not abort the program". It
cannot reach this path, because it clears `checked` while the
redirect-open abort keys on positional `in_condition`. The two signals
never meet.

It is redirect-specific. Measured in END: command failure continues,
exec failure prints and continues, division by zero exits cleanly at
rc=2. Only the open failure loops.

## The comparison that was wrong

The obvious next inference was that `-` should suppress an open failure
too, since an *exec* failure — the sibling setup failure, where the
command equally never runs — is suppressed by `-` and does print in a
condition. Four positions, and the open path disagreed with the exec
path in three of them. That looks like one inconsistency with one fix.

It is not. `errors.sh` 55.26 pins the suppressed case deliberately:

> `- echo > /bad` still ABORTS: `-` suppresses echo's write verdict, not
> the prior `exec N>file` open, which runs first

and 55.25/55.50/55.63/55.64 pin "no misroute, **no diag**" in condition
position. Three of the four "inconsistencies" are recorded design, with
a rationale: the open runs *before* the command, so suppression scoped
to the command does not reach it.

The general lesson is the specific one AGENTS.md already states — check
whether a behaviour is deliberate before calling it a bug — but the trap
here had a sharper edge. The exec path is a genuinely close analogue,
close enough to make a four-row table that looked like proof. A
consistency argument is only as good as the assumption that the two
things compared are the same kind of thing, and that assumption is
exactly what the spec comment denies.

Which left one real bug, and a fork the specs do not settle: what
*should* a failed open in END do? DG chose **A1** — diagnose, restore,
continue the rest of the cleanup — matching END's stated purpose and the
exec-failure precedent that does apply here. Landing separately.

## Gate work

Extracting the shared abort tail did double duty. `emit_redirect_push`
and `emit_redirect_guarded_body` carried the same "print the diagnostic,
then route through END or exit" block; `emit_open_failure_abort` is now
the single copy, which took `emit_redirect_guarded_body` from 4/6 to 2/2
and gives the END fix exactly one place to land.

`emit_c.rs::emit_inst` needed the established treatment: the `||` chain
of sub-dispatchers was itself costing three cyclomatic points in the
dispatcher it was meant to keep lean. Folding the chain into
`emit_family_inst` and adding `emit_redirect_inst` took it 14/3 → 10/2.

## Spec ids

`fr.1`–`fr.12` in `functions.sh` (dual-mode, plan 102 → 114, +24).
`fr.2` is the truncate-once differential, `fr.8` the nesting one, and
`fr.10`–`fr.12` mirror 55.24/55.25/55.50 for the scope.
