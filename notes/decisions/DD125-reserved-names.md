---
title: DD#125 — a name the compiler already understands is not the user's to bind
status: accepted
---

Settled with DG (2026-10-09), out of a bug found while probing a `wait` idiom.

> I'm thinking it should be an error to declare any function, variable, or
> collection with the same name as anything the compiler already understands —
> no naming a function as `fn` or `if` or `status`, likewise no variable named
> `cd`.
>
> — DG

## The rule

A declaration, assignment, `for` variable, read target, or `fn` definition may
not take a name the compiler already understands. Violations are **SB5092**.

The reserved set is the union of three groups, assembled in
`resolve::is_reserved_name`:

| group | source | examples |
|---|---|---|
| keywords | `parse::lexer::classify_word_token` | `if` `fn` `for` `in` `do` `str` `int` `attest` `END` |
| statement and value builtins | `BuiltinCmd::from_name` | `echo` `cd` `wait` `exit` `read` `unset` `len` `sub` `match` |
| value intrinsics | `resolve::INTRINSICS` | `status` `getpid` `argc` `random` `epochseconds` `getcwd` |

## Why

Not tidiness. sash resolves a bare name differently in three positions, and
without reservation the same name in one program meant two different things:

```sh
fn getpid -> int { return 42 }
echo $(( (getpid) ))   # 3444042 — the intrinsic
echo $(getpid)         # 42      — the user's function
```

An arithmetic sexpr `(name)` gave the intrinsic priority; a `$(name)` word
gave the user's function priority; a bare word in arithmetic consulted only
variables. Three positions, three precedence orders, no rule anyone had
written down. `int getpid=42` was worse: the intrinsic in a sexpr, and SB5050
*dead variable* in the word form, because the declaration's only reader had
silently resolved elsewhere.

Reservation deletes the question instead of answering it. There is no
precedence order to learn because there is no collision to order.

It is also what makes a **bare word in arithmetic** safe to define. `$(( wait
))` can name the intrinsic only because no variable may be called `wait`.
That is the enabling half of DD#126.

### Contextual keywords are included

`in` and `do` lex as ordinary words everywhere except their one syntactic
position, so `int in=5` and `for in in a b` used to compile. They are
reserved too: "anything the compiler already understands" does not have an
exception for words the lexer happens to tokenize late. The parse still
succeeds — `keyword_to_str` renders the name — and validation rejects it, so
the diagnostic still names the keyword by its source spelling (spec `31.88`).

### What is *not* reserved

The nine `__`-prefixed internal opcode spellings (`__table_delete`,
`__let_exit`, the `&`-fork one) are **not** reserved, because they were never
the user's problem. See the next section — they were a leak, and the leak is
closed rather than fenced.

## The leak this exposed, and why reservation was the wrong fix for it

`BuiltinCmd::from_name` used to map nine `__`-prefixed names to internal
opcodes, so `is_reserved_name` covered them and `fn __table_delete` became
SB5092. DG asked the right question: *why do those names exist at all, and
why does a compiler internal leak to and from a script?*

They exist because `SsaInst::Command` carries a redundant pair — a
`name: String` **and** a `kind: BuiltinCmd`. A command the compiler
synthesises (the `&` fork, `unset a[k]`'s table delete, a trap install) has no
source text, but the struct demands a name, so `SsaInst::internal` invented
one by spelling the enum out through `internal_name`.

They leaked because `from_name` classifies names that came from *source*.
Once the invented spellings lived in the same field as source names,
`from_name` was given arms for them too — one function answering for two
populations, and the compiler's private vocabulary became reachable by typing
it.

Nothing ever needed that direction. Every backend dispatches on `kind`, which
is exactly why all nine arms were cold ([journal 379](../journal/379-the-compilers-own-names-written-by-a-user.md)
measured them so). The string is write-only.

So the nine arms were **deleted from `from_name`** rather than reserved.
A `__`-prefixed name typed by a user is now `BuiltinCmd::Other`, an ordinary
unknown command (SB5011), exactly like `__anything_else`. Reserving would have
kept the leak and papered over it with a diagnostic; deleting removes the
namespace. `cov.327`-`cov.330` pass unchanged.

## Diagnostic placement

SB5092 runs in `lib.rs`'s second validation batch, beside its sibling
`check_reserved_loop_var` (SB5080), **not** in `validate()`. From the earlier
batch it masked better diagnostics: it reads a name out of a shape an earlier
check may already have rejected, so `read -a arr -d :` — a four-argument
`read`, SB5057 — reported "rename the variable `:`" and suppressed the arity
error entirely.

## Precedent

`args` (DD#92) and the collection slots `A`..`E` (DD#100) were already
reserved, by `check_reserved_loop_var`/SB5080 and SB5052. DD#125 generalises
what those two did by hand.

## Cost

Measured across `spec/`, `contrib/` and `suites/`: **one** genuine collision,
`fn sub` in `spec/arithmetic.sh`. It turned out to be a bug of its own — see
below.

The forward cost is real and accepted: adding an intrinsic becomes a breaking
change, because a script may already bind that name. DG took that trade for
the guarantee that one name means one thing.

## A spec that had stopped testing its subject

`25.6` claims to test the runtime SUB-overflow trap. It defined `fn sub` and
called `sub a b` — but `sub` is a value intrinsic, so the *call* was SB5073 at
compile time, and the assertion (`rc != 0 && -z $out`) was satisfied by the
compile error. DD#25's SUB trap had no coverage at all, and had not had any
since the intrinsic was added.

This predates DD#125; reservation only made it visible by rejecting the
definition too. The function is now `subtract`, the assertion also requires
`-z $err`, and the trap fires for the first time.
