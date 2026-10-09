# 483 — The rule that promised, and the remedy it removed

2026-10-09. Continuing DG's three-part design. Part 1 (DD#125 reserved
names) landed in `2d44023`; part 2 of the arithmetic work — the nineteen
silent zeros — landed in `4ac831f`. This session took the next piece: bare
words in arithmetic.

## What DD#125 had actually bought

DD#125's own justification is that reservation "makes a bare word in
arithmetic unambiguous, which is what lets `$(( wait ))` / `$(( getpid ))`
name the intrinsic". I went to write the specs for that and found it was a
promise, not a behaviour. Measured against the binary at `4ac831f`:

```
$ echo $(( getpid ))
error [SB5031] '$getpid' is assigned nowhere (possible typo) — assign it first
```

All ten of `getpid getppid argc random signum epochseconds status getcpid len
strlen`, identically. The rule had been paid for and never collected.

The asymmetry that made it a gap rather than a feature request: `$(( f ))`
**already** called a nullary `-> int` user function, and had for as long as
int functions existed. So two spellings of "call a nullary int thing in
arithmetic" disagreed purely on whether the thing was the compiler's.

## One node, not three special cases

The implementation fork was where to resolve the bare word. Three consumers
care — `validate::check_arith_reads`'s `Var` arm, `type_check`'s bare-operand
rule, and the transform's `arith_leaf_to_ssa`. Teaching each of them to
recognise a compiler-owned `ArithExpr::Var` is three lists answering one
question, which is precisely the shape that rotted into nineteen silent zeros
the commit before (journal 482). I was not going to re-create it one session
later.

So the parser desugars instead: a bare compiler-owned word becomes the very
`FuncCall { args: [] }` node the DD#58 sexpr form builds. There is precedent
— DD#111 already has the parser desugar `$?`/`$!` into `(status)`/`(getcpid)`
— and the payoff is that the two spellings are structurally incapable of
disagreeing. A bare `echo` reports the same SB4030 as `(echo)`; a bare `len`
the same SB4045; `$(( (argc) - argc ))` is one node evaluated twice. Nothing
downstream changed at all.

The lookahead is the subtle part. `try_assignment` and `try_postfix` match on
`ArithExpr::Var`, so desugaring unconditionally would send `(( getpid = 5 ))`
into a parse error about a stray `=` instead of SB5092. An assignment target
is not a read; `bare_word_atom` checks the next token and leaves those alone.
Specs `58.65`/`58.66` pin it.

## The remedy the rule removed

Then the dead end, which is the real finding.

```
$ echo $getpid
error [SB5031] '$getpid' is assigned nowhere — assign it first, or write
               ${getpid:-DEFAULT} for an intentional default
$ int getpid=1
error [SB5092] `getpid` is a name the compiler already understands, so it
               cannot also be a variable (DD#125)
```

SB5031's advice is advice SB5092 forbids the author from taking. Before
DD#125, `int getpid=5` was legal and the advice worked — the reservation
removed the remedy without touching the diagnostic that recommends it, one
commit earlier, and nothing noticed because every spec about DD#125 was about
*binding*. A rule satisfiable only by breaking another rule is not a
diagnostic, it is a maze.

SB5093 answers the read side, keyed on the `ArithDenotation` table from
journal 482 so the advice differs by what the name denotes: an `Int`
intrinsic is pointed at `$(getpid)` or the new bare form, a `Str` one at its
capture, a `Statement` one told it denotes no value, and a keyword told it is
a keyword.

It is raised **ahead of** the `${x:-…}` opt-out. That one is worth stating
plainly: `${getpid:-5}` was not an intentional default. It was a read of a
name that can never be bound, and it silently produced `5`.

The scalar sink and the splat sink had the same dead end, the same guard
list, and both needed the same new branch — so they now share
`classify_read` → `ReadVerdict` rather than growing a second copy. That also
brought both back under the complexity gate, which had rejected them at
cog 8 and cog 6.

## The gate that had to move

The desugar broke one spec, `125.4`, and the breakage was the interesting
kind.

```
int status=5
echo $(( status ))
```

Before: `$(( status ))` was `Var("status")`, `status` was in `known`, no
diagnostic, and SB5092 reported the declaration. After: it is `(status)`,
which trips SB5012 ("no preceding command") — and SB5012 lives in
`validate()`, an *earlier* batch than SB5092, which exits first. The author
was told their status was unavailable, with no hint that their own
declaration had caused it.

The fix is a split, not a move. DD#125's own "Diagnostic placement" section
records why SB5092 sits late: its command out-param extractor reads a name
out of a shape an earlier check may already have rejected, so `read -a arr -d
:` reported "rename the variable `:`" and suppressed SB5057. That reasoning
is about the *extractor*, and it does not apply to the four sites that read a
name straight off a well-formed node.

So `check_reserved_names` now takes a `ReservedScope`. `Bindings` —
assignment LHS, declarations, `for` variables, `fn` names — runs as the
compiler's first gate, alone, exiting immediately. `CommandTargets` stays
where it was. The justification for running the binding half first is not
convenience: a naming violation is logically prior to every semantic check,
and once a program binds `status`, no other diagnostic about `status` is
trustworthy. The SB5002 heredoc retain two lines below makes the same
judgement about the same kind of cascade.

`125.22` pins it by asserting SB5092 **and** `$err != *SB5012*` — the
absence is the claim, because the presence is what the bug looked like.

## Two bugs found and not fixed

Both predate this work; both are recorded for DG rather than quietly patched,
because each is a design question rather than an oversight.

**`$!` before any `&` is a silent empty.** `$(getcpid)`, `$!` and
`$(( (getcpid) ))` all yield `""` when no job has been backgrounded —
verified against the binary at `4ac831f`, so the bare-word form inherits it
rather than introducing it. `$(status)` has a flow-sensitive availability
state machine (SB5012) for exactly this; `$!` has nothing. An arithmetic
expression yielding the empty string is a value-model violation whichever way
it is resolved, but *which* way — an SB5012-style `$!` state machine, or a
`-1` sentinel like the one DD#126 gives `wait` on ECHILD — is DG's call.

**Bare `wait` and the C backend disagree.** With no children, native calls
`sash_wait(0)` → `waitpid(0, …)` → ECHILD → `$?` = 127; the C backend emits
no call at all and sets `$?` = 0. A differential-oracle divergence that no
spec catches, because no spec reads `$?` after a childless `wait`. DD#126
rewrites both lowerings, so it will be fixed there rather than twice.

## Gates

Sweep **7925** (3847 c + 4065 native + 13 skipped), predicted before running;
0 failing, 0 flips, 42 new ids (predicted 42), gate self-test 248. Units
86/27/20. fmt, clippy, totality, complexity all clean — `record_param` 7/3
and `record_splat` 7/5 after the shared classifier, against a 3× gate of
11.19/5.25.

## Method note

The sweep that found `125.4` was run *before* any spec was written. That
ordering is worth keeping: a change whose only spec evidence is the specs it
ships with cannot tell you what it broke.
