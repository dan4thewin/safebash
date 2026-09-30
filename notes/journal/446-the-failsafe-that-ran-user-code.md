# 446 — The failsafe that ran user code

DG asked a flat question: "how did you change `${x:?MESSAGE}`?"

I showed him the two-line diff from `3506dba` and cited DD#78 line 73 as the
authority. He did not accept it:

> `:?` is an abort. The code is exiting. Treating MESSAGE as an interpolating
> word means more code can run. Naming it a disposition default doesn't equate
> it to `:-` but rather groups it by purpose.

## The authority error

DD#78 does name `${x:?MESSAGE}` in its Scope section. What it says about it is
a **purity** rule — the operand must be clean and non-`random`. It says nothing
about how the operand is *spelled*. That belongs to DD#65, and DD#65 defines
the interpolating-word parse for `:-` alone.

So I took a DD that *groups two forms by purpose* and read it as settling a
question about the form it merely mentions. The grouping is real; the
conclusion is not in it.

Worth stating because it generalises: when a DD appears to settle a question
about a form it only names in passing, find the DD that owns that question.

## What it cost, measured

I conceded the argument and then measured what I had admitted, because a
concession is not a finding. Several probes were confounded first — `str y=`
is SB4014, unused variables trip SB5050, and a `$( )` capture *swallows stdout
side effects*, so a discarded default looks exactly like one that never ran.
A `touch mark` in the callee finally gave clean data:

| form | `y` | abort taken? | did `g` run? |
|---|---|---|---|
| `${y:-$(g)}` | `val` | fallback not taken | no — lazy |
| `${y:-$(g)}` | `""` | taken | ran (correct) |
| `${y:?$(g)}` | `""` | abort taken | ran |
| **`${y:?$(g)}`** | **`val`** | **NOT taken** | **RAN** |

The last row is the one that matters. `str y=val; str z=${y:?$(g)}` exits 0,
prints nothing, and still forks and execs — because the message is materialized
eagerly, whether or not the abort fires. Arbitrary user code on the success
path of a failsafe.

My own commit message had claimed the non-abort path was unaffected. It was
not, and `vm.req7` — the spec I wrote to cover it — did not test it.

DG then added the observation that closes off the obvious repair: "(NB. `$(( ))`
allows user functions to be called)". Verified — DD#58's arith sexpr resolves
`(g …)` to a user function, given `fn g -> int`. So "allow the word but ban
`$( )`" is not a rule that can be stated, never mind checked.

## The revert was half a fix, twice over

First half: reverting the scalar parse left `@{a:?WORD}` interpolating. It had
*always* been the more permissive spelling — `optional_splat_word` called
`parse_default_word` — so `@{a:?$(g)}` ran `g` and printed its return value as
the abort message, and kept doing so after the scalar form was fixed. Two
spellings of one rule, one of them never looked at.

Second half: a literal message degrades **silently**. `${y:?$m}` is valid bash,
reads as an interpolation, and quietly emits two characters. That is precisely
the failure SB4047 exists to prevent. DG:

> I think `${y:?MESSAGE}` should reject any `$` unless wrapped in single
> quotes, `${y:?'costs $5'}` in which case the message is the quoted string
> with the quotes removed.

**SB4070**, at both spellings, through one shared `parse_abort_message`.

## The archaeology DG asked for

He suspected "the eager business came as a way to avoid a taint bypass". Close,
with one correction worth keeping:

- **`14a3b70`** (DD#78 Stage 1) closed a real hole — `${x:-$(getenv HOME)}`
  compiled *and* left the assigned variable clean. It states plainly that
  "`:?`/pattern operators carry a literal operand (DD#65), nothing to check."
  So `:?`-as-literal was the intent from the start, and DD#78's own Scope
  section contradicts its own Stage 1.
- **Eagerness was never a safety choice.** It was pre-existing codegen — a
  branchless select. The `$(random)` ban (SB5035) is a *consequence* of it.
- **`62bd809`** made `:-` lazy, but only when the operand contains a capture,
  and recorded that DD#78's premise was false all along: it "deliberately chose
  eager, arguing it is safe because every legal default is side-effect-free —
  then lists user functions among the allowed defaults."
- That same commit **filed the `:?` divergence**, with the line "Not a taint
  leak — nothing is expanded, so nothing escapes." That filing is what I picked
  up. I converted a documented-safe literal into a call site.

A filing is a snapshot, and its framing is an argument, not a fact. This one
said "unimplemented DD#78 clause" when the honest reading was "deliberate, and
the DD disagrees with itself".

## Left open

`SB5035` — the `$(random)` ban in a `:-` default — justifies itself by
eagerness ("it would advance the PRNG whether or not the fallback is taken").
But `$(random)` is a capture, and since `62bd809` capture-bearing defaults take
the lazy path. The stated premise no longer holds. Filed as
`q-sb5035-random-ban-vestigial`, not chased — DG raised "perhaps `:-` should be
simplified" and this is the concrete lead, but it needs measuring before it is
a proposal.

I also flagged a backtick as having "the identical silent-degrade property",
left unasked because DG had specified `$`. DG: "backtick? we don't use
backtick, yes?" — and measuring says the flag was wrong. DD#67 **dropped**
backticks; SB5076 rejects one in any position where bash would have
substituted, and everywhere a literal is expected — inside `"…"`, inside
`'…'`, and now in a `:?` message — a backtick already is just a character.

There is nothing to degrade *from*. `$` degrades because interpolation is a
real feature everywhere else, so `${y:?$m}` reads as something the language
does. A backtick reads as nothing, anywhere, so the message accepting it is
consistent with the quoted-string forms rather than an exception to them.

Filed here because reaching for symmetry ("`$` and backtick are both command
substitution in bash") is what produced the flag, and it is the same move as
the DD#78 error at the top of this entry: a grouping that holds in bash does
not carry into a language that dropped one of the two.

## Gates

Sweep **7037** (3621 native, 3416 c, 13 skipped) — predicted exactly, from
`+3` glob_splat and the `+2` var_modifiers already in the tree. Zero failures.
`mk test`, fmt, clippy, totality green. `parse_disposition_operator` gained an
arm and sits at cc 10 / cog 4 against an 11.41 / 5.48 bar;
`parse_abort_message` 5/4.

One spec caught itself: `gs.58` passed in c mode for the wrong reason. A splat
into the `echo` *builtin* is SB5030 regardless of the message, and the
native-only `$rerr` guard left just `rc != 0` behind in c. Switched to
`/bin/echo`, so SB4070 is provably what rejects it.
