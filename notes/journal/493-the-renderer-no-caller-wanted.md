---
title: The renderer no caller wanted
date: 2026-10-11
kind: journal
---

# The renderer no caller wanted

Two items, both DG's rulings on questions that had been waiting since the
previous session. They turned out to be the same question asked at two ends
of the compiler: *what is allowed to sit in a name position?*

## SB4074 — an empty command name is a syntax error

`''` as a command head reached the validator as an empty name and came back
with **SB5011**, "command '' not found — use absolute path or define as
function". In capture-condition position it came back with SB4025 as well,
and SB4025's text asserts a reason — "captures a sash intrinsic/builtin … or
concatenates" — that is untrue of `''`. The only shape in the language that
drew two diagnostics, and one of them lied about why.

DG's ruling: *"an empty name can neither name an intrinsic nor an external —
strikes me as more of a syntax error."*

That is the right frame, and it moves the check a whole pass earlier. SB5011
is a *resolution* answer: I looked for this name and did not find it. For the
empty string there is nothing to look for. The parser can say so.

Before adding the rule I enumerated the producer, because the question "can
anything else build an empty head?" is not one the diagnostic site can
answer. There are seven `Command` construction sites in the parser. Six use
fixed names (`"status"`, `"getcpid"`, `"[["`, `"attest"`) or names looked up
from an intrinsic table (`intrinsic_expansion`, `build_one_sed_sub`). Exactly
one — `parse_command` — takes user text, and it is the sole caller of
`normalize_command_head`. So `''` is the only route in, and one check in one
place closes it.

`""` is a different path and keeps SB5011: it is rejected for being
double-quoted, not for being empty, and that message is accurate.

The interesting part was what *not* to do. `resolve::is_external_looking` had
an `is_empty` arm, now dead. The reflex is to convert it to an `unreachable!`,
which is what the last three cold arms got. But DG's orthogonality rule says
an unreachable line earns its place only if it catches a regression by some
*other* route than the gate already does. An ICE there would ask "is this
string empty?" — the parser's own question, of the same string, three passes
later. It can only fire when the parser's check has already failed, and it
learns nothing the parser did not. So the arm is simply gone, with a comment
saying why no ICE replaced it.

First time the rule has been applied negatively. It is a sharper tool than I
had been treating it as: it rules ICEs *out*, not just in.

## Narrowing `expr_to_string`

The other ruling. `expr_to_string` was a renderer with an arm per `ExprKind`,
reproducing something close to the source text. Coverage said it was entered
21338 times and took the `Literal` arm every single time.

A cold arm is a hypothesis. The classification rubric offers four answers —
spec gap, validate-shadowed, totality, ICE — and coverage cannot distinguish
them. So: enumerate all 25 call sites from the producer side.

Every one is a name position an earlier pass constrains:

- **ten are command heads.** `validate::check_cmd_at` rejects a non-`Literal`
  head and returns. The thing that made this one settlement rather than ten
  was `ir::structural_bodies`' `Pipeline(p) => vec![&p.commands]` — that one
  line carries the traversal into pipeline stages, so a stage head is covered
  by the same guarantee as a top-level one;
- **two are `signal`'s arguments**, which SB4059-SB4062 require to be exactly
  two integer literals (`check_signum_arg`: "must be an integer literal, not
  an expansion");
- **thirteen** are collection names, read/match fill targets, DD#102
  intrinsic operands, and DD#65's disposition operand — all name positions
  with their own rules.

So no caller wanted a renderer. They all wanted one thing: *the literal name
here*. DG ruled: narrow it.

It is now `literal_name_of(&Expr) -> &str`, which ICEs on anything else. The
two private helpers it routed to, `joined_form_to_string` and
`opaque_form_to_string`, are gone with it. Returning a borrow instead of a
cloned String falls out of the narrowing for free — the old signature had to
own a String because some arms built one.

The justification for the ICE is not that the arms were uncovered. It is that
they were **answers**. `ExprKind::CommandSubstitution` rendered as the text
`"$(...)"`; a `Test` as `"[[ ... ]]"`. Hand either of those back to a caller
asking for a name and it goes looking for a variable called `$(...)`, finds
nothing, and returns a silent zero or an empty string. That is not
hypothetical: it is precisely the shape that once let a `join` separator
become the five characters `"$sep"`, and it is what SB5062's rule and the
`argv` arity rule exist to prevent upstream. Both of those rules' doc
comments name this renderer as the mechanism of the bug they fix. I left the
old name in those comments — they describe what it *did* — and noted the
narrowing beside each.

So the pattern across the two items is the same, in opposite directions: a
complete, plausible implementation that nothing executes is not neutral. The
empty-name arm in `is_external_looking` was neutral, and was deleted. The
renderer's arms were *wrong*, waiting, and became an ICE.

## The complexity gate, doing its job

The rename touched `lower_if`, which the grandfather-on-modify gate then
asked for: cc=6, cog=9. Pure rename on my part — the function was already
there — but that is exactly the mechanism working. cc 6 is unremarkable; cog
9 is the tell, and the shape is hidden nesting: three nested `if`s detecting
DD#24's if-attest form.

Extracted as `if_attest_condition`. The thing worth recording is what it
delegates to. `statements::is_attest_left` already held the same predicate
(`name == "attest" && args.len() >= 2`) for `&&`/`||` operands. Respelling it
in control_flow would have made two copies of one rule, which is the exact
shape that has bitten this repo repeatedly — a fix lands on one copy and the
other silently keeps the bug. So the predicate moved into
`statements::attest_command`, returning `Option<&Command>`, and
`is_attest_left` became `attest_command(stmt).is_some()`. Both callers now
ask one function.

`lower_if` is cc=3/cog=3.

## Gates

Sweep 8155 both times (3962 c + 4180 native + 13 skipped), predicted exactly,
zero flips, zero new ids. `mk test` 149 units, 0 failures. Commits `88a1b3f`
and `1d88eff`.
