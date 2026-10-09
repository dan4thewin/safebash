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

## Amendment 2026-10-09: what reservation bought, and what it broke

Reservation was justified in this DD as the thing that makes a bare
arithmetic word unambiguous. That was a promise, not a behaviour: on the day
DD#125 landed, `$(( getpid ))` still reported SB5031 and `$(( wait ))`
reported SB5031, exactly as before. Two changes collect the payment and fix
the hole the rule itself opened.

### Bare words in arithmetic are nullary calls

`$(( f ))` already called a nullary `-> int` user function — it had for as
long as int functions existed. The *intrinsics* did not, and that asymmetry
was the whole of the gap. DG's rule: consistent bare words.

`parse::arith::bare_word_atom` desugars a bare compiler-owned word into the
very `ArithExpr::FuncCall { args: [] }` node the DD#58 sexpr form builds. The
choice to do this in the parser rather than in the three consumers is the
load-bearing one: validate, `type_check` and the transform each then see
**one** shape, so

- a bare `echo` reports the same SB4030 as `(echo)`,
- a bare `len` reports the same SB4045 arity error as `(len)`,
- `$(( (argc) - argc ))` is one node evaluated twice, not two code paths that
  happen to agree today.

Teaching three passes to special-case `ArithExpr::Var` would have been three
lists for one question — the exact shape that let the sexpr denotation rule
rot into nineteen silent zeros one commit earlier (journal 482).

The lookahead in `bare_word_atom` is load-bearing too. An assignment target
and a `++`/`--` operand are not reads, and `try_assignment`/`try_postfix`
match on `Var`; leaving those as `Var` keeps `(( getpid = 5 ))` on SB5092
rather than a parse error about a stray `=`.

This is why reservation was required. Without it, `getpid` could be both a
variable and an intrinsic, and the parser would have no basis to choose.

### SB5093: reading a reserved name

DD#125 created a dead end the day it landed, and nothing noticed because
every spec about it was about *binding*.

```
$ echo $getpid
error [SB5031] '$getpid' is assigned nowhere — assign it first, …
$ int getpid=1
error [SB5092] `getpid` is a name the compiler already understands …
```

A rule satisfiable only by breaking another rule is not advice. Before
DD#125, `int getpid=5` was legal and SB5031's advice worked; reservation
removed the remedy without touching the diagnostic that recommends it.

SB5093 answers the read side, with advice that differs by what the name
denotes (`resolve::reserved_read_advice`, keyed on `ArithDenotation`):

| denotation  | example   | advice                                       |
|-------------|-----------|----------------------------------------------|
| `Int`       | `getpid`  | write `$(getpid)`, or a bare `getpid` in `$(( … ))` |
| `Str`       | `getcwd`  | capture it with `$(getcwd …)`                |
| `Statement` | `echo`    | a command, not a variable — it denotes no value |
| keyword     | `do`      | a sash keyword, not a variable               |

It is raised ahead of the `${x:-…}` opt-out. `${getpid:-5}` is not an
intentional default; it is a read of a name that can never be bound, and it
quietly produced `5`.

The scalar sink (`$x`) and the splat sink (`@a`) had the same dead end and
near-identical guard lists, and both had to grow the same branch. They now
share `classify_read` → `ReadVerdict`, so a third copy cannot drift.

### The binding gate moved to the front

"Diagnostic placement" above records why SB5092 sits in the second batch: run
early, its *command out-param* extractor reads a name out of a shape an
earlier check may already have rejected, so `read -a arr -d :` reported
"rename the variable `:`" and suppressed SB5057.

The bare-word desugar made that placement wrong for the other half. Bind
`status` and `$(( status ))` now reads the intrinsic, whose SB5012
availability error fires in `validate()` — an earlier batch — and masked
SB5092 entirely. The author was told the status was unavailable, with no hint
that their own declaration caused it.

`check_reserved_names` now takes a `ReservedScope`, and the two halves run in
different phases:

- **`Bindings`** — assignment LHS, declarations, `for` variables, `fn` names.
  Reads a name straight off a well-formed node, so nothing can mis-extract
  it. Runs as the compiler's **first** gate, on its own, exiting immediately:
  a naming violation is logically prior to every semantic check, and once a
  program binds `status` no other diagnostic about `status` is trustworthy.
  Reporting the cause alone is the same judgement the SB5002 retain makes for
  a heredoc's cascade.
- **`CommandTargets`** — `read`/`argv`/`match`/`keys` out-params and the
  DD#93 attest-convert target. Stays in the second batch, for the reason
  above.

Pinned by `125.22`, which asserts SB5092 **and** the absence of SB5012.

### Coverage

`58.54`-`58.67` (bare words, in `spec/arithmetic.sh`) and `125.16`-`125.22`
(SB5093 and the gate split, in `spec/scope.sh`).
