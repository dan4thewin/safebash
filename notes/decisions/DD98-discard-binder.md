---
title: DD#98 — `_` is the don't-care binder
status: accepted
---

Settled with DG (2026-09-08), from his question: *"how did we address discarding
unwanted fields when doing read? was it only the `: $x` mechanism? I'm thinking
a 'don't care' variable like `_` might have value."*

## The problem it solves

Discarding a value required satisfying **two independent rules that collide**:

- **DD#86** dead-variable (SB5050) — every variable must be read.
- **DD#24** taint — a `read` target is tainted, so any *use* needs `attest`.

`: $v`, the documented DD#86 escape hatch, is itself a use. On a **standalone
`read`** that use is a tainted read, so it hit the SSA taint backstop:

```sh
read a b
attest b p?
: $a        # tainted variable a_1 used as argument to command ':'
            # without attest (SSA taint backstop)
```

The only thing that compiled was `attest a p?` — **asserting a property of a
value you are explicitly throwing away**. That is backwards: attest exists to
state what you have checked, and you have checked nothing, because you do not
care.

(Measured caveat: `: $v` *does* work after a **pipeline** read
`cmd | read a b`. The two read shapes disagree — filed separately as
`read-taint-backstop-no-sb-code`.)

## The rule

`_` is the **don't-care binder**, legal **everywhere a binder appears**:
assignment LHS, `int`/`array`/`map`/`const` declarations, `for` variables,
`read`/`mapfile`/`readarray`/`argv` targets, DD#93 attest-convert targets, and
attest-all binders.

It is **write-only**:

| property | |
|---|---|
| exempt from SB5050 | being unread is the point |
| taint-safe | its value can never be observed, so there is nothing to attest |
| repeatable | `read _ b _` binds three fields, keeps one |
| **every read is SB5056** | `$_`, `${_}`, `${_[0]}`, `@_`, and a bare `_` in `(( … ))` |
| still binds | `read _ b` consumes field 1, so `b` gets field 2 |

```sh
/usr/bin/printf "xx yy\n" | read _ b
attest b p?
echo "b=$b"                    # b=yy — no acknowledgment, no attest

map m=([A]=1 [B]=2)
attest _ v in m [[ -n $v ]]    # discard the key; DD#61 normally demands both
```

## Why write-only is not negotiable

The taint exemption is only sound because nothing can observe `_`. If `_` were
both taint-exempt **and** readable, then

```sh
read _ x
y=$_          # ← would move a tainted value into a clean one
```

would be a validator bypass — laundering by punctuation. Rejecting every read
closes it, and costs nothing: a value you named `_` is a value you said you did
not want.

This is why the rule is "reading `_` is an error" rather than "`_` is just a
variable that is exempt from SB5050".

## Not a new sigil

`_` is an ordinary identifier the language now reserves a meaning for. No
lexer or parser change was needed — `is_plain_identifier` already accepted it,
and `read _ b _` (a repeated target) was already tolerated. The whole feature is
three checks: an exemption in `report_dead`, an exemption in the attest-all
both-binders rule, and the new SB5056 read reject.

In the shipped language, names *prefixed* with `_` (`_n`, `_args`) remain
ordinary variables, still subject to SB5050. Only the bare name `_` has
this discard meaning. Nothing in `spec/` or `contrib/` used `_` as a
variable in the original census, so that migration cost was zero.

DD131's 2026-11-09 **provisional** branch trial proposes `_batch`-style
scratch collection fill targets. They remain readable and subject to
normal taint and unused-variable rules; none inherits bare `_`'s
exemptions. Bare `_` stays repeatable, write-only discard, not a fresh
readable scratch binding. The proposed prefix rule is not shipped.

## The arithmetic hole this exposed

A bare `_` inside `(( … ))` is an `ArithExpr::Var`, not an expansion, so
neither `walk_expr_tree` nor `walk_all_exprs` surfaces it — the first
implementation let `(( y = _ ))` through to the *taint backstop*, which
rejected it with no SB code and no explanation. Correct outcome, useless
diagnostic. Closed by also walking `walk_arith_var_reads`, which is the exact
complement (it visits bare operands and skips `Expansion`).

Third instance this week of the same shape: a check that stops at the
`ArithExpr` boundary. See DD#97's `walk_arith_expansions` subscript gap and the
DD#65 pruned-operator reject before it.

## Alternatives considered

- **Fix `: $v` to work on tainted values.** Rejected: `:` taking a tainted
  argument and being *fine with it* is a special case in the taint rules, and
  it still reads as "I am doing something with this value" when the point is
  that you are not.
- **`_` scoped to `read` targets only.** DG chose everywhere a binder appears —
  one uniform rule beats a rule with a position list, and the attest-all binder
  case (`attest _ v in m`) turned out to be a real win on its own.

## Specs

`spec/scope.sh` dc.1–dc.15: read discard, binding position preserved, repeats,
every read spelling rejected (including both arithmetic forms), `for` variable,
tainted-capture assignment, attest-all binder, the SB5056 message, and two
controls proving `_`-prefixed names are unaffected.
