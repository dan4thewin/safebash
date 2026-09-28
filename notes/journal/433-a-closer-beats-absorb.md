# 433 — a closer beats absorb, and what was hiding behind it

Three filings went into this session and four came out. That is the honest
summary: the queue said one thing, the binary said another, and the thing
worth finding was not on the list at all.

## The queue had drifted

`plan.md` still described the native backlog review, and its "queued next"
list opened with `bug-getenv-status-unvalidated` (P1). That was fixed on
2026-09-12 and the filing says so. Of the tasks it named, several were already
done. So the first real work was reading `notes/tasks/` directly for anything
not `status: done`, which turned up one open **P1 bug** the plan never
mentioned: `bug-function-call-redirect-dropped`.

That one is fixed too — `ad6be39`, 2026-09-19, twelve specs, and the stale doc
comment the filing complained about rewritten. Nobody closed the ticket.
Re-measured every gate against the live binary (`>`, `>>`, `2>`, `<`,
`> f 2>&1`, args, `if`/`while` bodies, external bodies, a nested call, a
second unredirected call) and closed it with the evidence.

Three stale filings in one sitting is a pattern, not an accident. The native
backlog review said it out loud — "a filing is a snapshot of a moving tree and
nothing ages it" — and this is the same finding arriving again. **Reproduce
before you fix.** It cost about ten minutes and saved reimplementing two fixes
that already existed.

## The bug that was real

`$(int x=1)` was reported as:

> error [SB4057] unterminated `$(` — it is never closed before the end of the
> source, so everything after it was read as part of it

with the `)` sitting right there. The filing blamed `parse_statement`
over-consuming. It was half right and half wrong, and both halves mattered.

**Wrong half:** the filing opens "a declaration inside a command substitution
is rejected — correctly". It is not rejected. `$(int x=1; echo $x)` compiles
and prints `1`. The fault needs the declaration to be the *last* statement,
and a plain assignment (`$(x=2)`) has it too, which the filing never mentions.

**Right half, but for a different reason:** the `)` really was being consumed —
by **DD#71 absorb**. `absorb_text` maps `Tok::RParen` to a literal `")"` in
assignment-value mode, so `int x=1)` built the value word `1)` and swallowed
the substitution's own terminator. SB4057 was telling the truth about what the
parser had seen.

The tell was visible at top level the whole time:

```
$ int y=1)
error [SB4030] cannot assign string value to int variable `y`
```

`1)` is a *string*. That message has been describing the absorb accurately for
as long as it has existed.

## The rule was already written down

This looked at first like a fork worth putting to DG — DD#71 deliberately makes
`)` a literal in a value position, so stopping at it is a change to a documented
surface. It is not a fork, because DD#71 answers it:

> an operand-word lexer mode (absorb to whitespace / **closer**, quote- and
> `$`-aware)

and the `[[ ]]` side already implements exactly that:

```sh
if [[ $x == a]] { echo yes }      # parses — `]]` beats absorb
if [[ $x == a]b ]] { echo yes }   # parses — a lone `]` is absorbed
```

Inside a `$( )` body, `)` *is* the closer. So this is the decision tree's step
1, settled by a source of truth, not step 4. Worth noting because my first
instinct was to write DG a three-option message, and that would have been the
over-asking failure mode the SOP warns about — the mirror of journal 432, where
I under-asked by not reading the DD first. Same root: read the DD, then decide
whether there is a question.

`Scanner` gains `cmd_sub_depth`, mirroring `arith_depth` field-for-field. Two
lines of behaviour: `absorb_one_part` declines `Tok::RParen` at depth, and
`Tok::RParen` joins the declaration break set — which already had `Tok::RBrace`
for the identical reason, with a comment describing the identical failure for
`fn f { str x="a" }`. The second site was a separate bug wearing a separate
message (SB4014 "expected variable name in declaration" for the spaced
spelling `$(int x=1 )`), and only the neuter matrix separated them.

## The thing that was not on the list

Making those bodies parse made them worth probing. Statements inside a `$( )`
body had, in effect, never been exercised — so what checks them?

```sh
/bin/echo $(str h=$(getenv TARGET); echo pwned > $h)
```

Compiles clean. No SB5020, no backstop, no warning.

```
$ TARGET=/tmp/attacker_chose_this ./esc
$ cat /tmp/attacker_chose_this
pwned
```

An environment variable chose a filename and the program wrote to it. The same
two lines at top level are SB5020.

The cause is four lines, `check_cmdsub_inner_reads`:

```rust
for s in stmts {
    if let StmtKind::Command(c) = &s.kind {
```

Not total over `StmtKind` — a declaration, an `if`, a `Redirected` are all
skipped — and the `tainted` set it consults is the **outer** one, never
extended by definitions made inside the body. The real walker, `taint_stmt`,
is never applied to a body at all.

**The fourth non-total walk in four sessions** (430 the taint walker pair, 431
the `Trusted` wrap, 432 the capture lift, now this). The other three were in
lowering passes. This one is in the primary validator.

Most of it is masked by the SSA backstop, which is why it survived — the
backstop fires with an internal-sounding message and no SB code, which reads
like a scary-but-handled case rather than a hole. The two positions it does
*not* mask are the two journal 400 already recorded as escaping it: `[[ ]]`
read-back and redirect targets. Those are the two that compile.

Filed as `bug-taint-escape-in-cmdsub-body`, P1, not fixed here. The fix is not
mechanical: `check_expr_taint` has no `SlotCx`, so either that threads through
the whole expression-taint layer, or the body walk moves up to the statement
layer where `taint_stmt` already holds one. That is an architecture choice with
real blast radius — every spec doing work inside a `$( )` body has never had
its statements taint-checked — so it goes to DG.

## Bookkeeping

`csc.1`-`csc.10` (errors.sh) and `av.f11` (argv.sh). Two disjoint neuters cover
csc.1/3/4/7 and csc.1/2/3/4; the rest are regression guards, deliberately — 
csc.8 pins that `str s=a)b` is *still* the literal `a)b`, csc.9 that a `[[ ]]`
regex operand still absorbs its parens, and csc.10 that a genuinely
unterminated `$(` is still SB4057. A fix that makes the real diagnostic
unreachable is the obvious way to get this wrong.

Sweep 6683, predicted exactly. Zero failing, zero flips.

The SB5025 wording went in alongside: the advice said "snapshot into an array
with `argv [N] a`", where `[N]` is a placeholder rendered as code. Taking it
literally is `av.f3`, which the compiler rejects — so the message was advising
a program it would refuse. It now leads with `argv a`, which compiles as
written. The ICE half of that filing had already been fixed months ago, and
found to span eleven intrinsics rather than the two it named.
