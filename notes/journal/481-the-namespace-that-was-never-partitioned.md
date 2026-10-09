# 481 — The namespace that was never partitioned

DD#125. DG: *"it should be an error to declare any function, variable, or
collection with the same name as anything the compiler already understands."*

The rule is one line of predicate. Everything interesting happened in the
fallout.

## What the rule fixes

sash resolved a bare name differently in three positions, and nobody had
written down the precedence:

```sh
fn getpid -> int { return 42 }
echo $(( (getpid) ))   # 3444042 — the intrinsic won
echo $(getpid)         # 42      — the user's function won
```

One program, one name, two answers. `int getpid=42` was worse still: the
intrinsic in a sexpr, and SB5050 *dead variable* in the word form, because the
declaration's only reader had quietly resolved somewhere else.

Reservation deletes the question rather than answering it. It is also the
enabling half of the `wait` work: `$(( wait ))` can name the intrinsic only
because no variable may be called `wait`.

## The blast radius I measured was wrong

I measured one collision before implementing — `fn sub` in `arithmetic.sh` —
and predicted one broken spec. The sweep broke **six**. The gap is the whole
lesson of the session: I had grepped for *declarations of reserved names*, which
is the rule's subject, and missed every program that binds a reserved name
through some other shape.

## Fallout 1 — the diagnostic that masked a better one

`22.27` is `read -a arr -d :`. It expects SB5057. It got SB5092 telling the
user to rename the variable `:`.

Two faults stacked. The read-target extractor filters `-`-prefixed arguments —
a bash flag surface **DD#99 retired** ("Two commands. No flags."). With no
flags, `read -a arr -d :` is a four-argument `read`, SB5057 owns it, and the
extractor's notion of "the target is the last argument" hands back the `-d`
flag's operand.

That alone was harmless for twenty-odd commits, because the only consumer was
`collect_scope_names`, where an extra name in a set merely suppresses an
"undeclared" complaint. Pointing a *diagnostic* at the same extractor turned a
harmless over-approximation into a false positive — and, because I had wired
the check into `validate()`'s first batch, it **suppressed SB5057 entirely**.

Fixed twice over: the stale flag filter is gone, and SB5092 now runs in the
second batch beside its sibling SB5080. A check that reads a name out of a
shape an earlier pass may already have rejected has to report *after* it.

## Fallout 2 — DG's three corrections

I took the question to DG and got three facts back, each of which was a defect
in the code I had been reading from:

> *very confused by this; read doesn't take switches and only splits on NUL,
> sash doesn't have subshells, and `__` is not a sigil.*

All three held:

1. **`read` has no switches** — DD#99. The filter above.
2. **`SubshellStart` is not a subshell.** It emits `sash_fork()` for `cmd &`.
   DD#38 is titled *"Reject subshells, & is the fork mechanism"*, so the
   opcode was named after the construct the DD removed. Renamed
   `ForkBackground` / `__fork_background`.
3. **`__` is not a sigil.** DD#100's sigil is `=`. `sigil_str` →
   `internal_name`, `SsaInst::sigil` → `SsaInst::internal`.

Worth recording that I propagated all three misnomers into a journal, four
specs and a commit message (`b4f2a01`, "the sigil namespace is a user namespace
too") without once checking them against the DDs that define the terms. The
code's vocabulary was wrong and I inherited it rather than reading it.

## Fallout 3 — the question that dissolved the problem

The nine `__`-prefixed names were going to be reserved. DG asked instead:

> *Explain why the 9 `__` names exist at all and why a compiler internal leaks
> to/from the user/script.*

They exist because `SsaInst::Command` carries a redundant pair: a
`name: String` **and** a `kind: BuiltinCmd`. A command the compiler synthesises
— the `&` fork, a table delete, a trap install — has no source text, but the
struct demands a name, so `SsaInst::internal` invented one by spelling the enum
out through `internal_name`.

They leaked because `from_name` classifies names that came from *source*. Once
the invented spellings lived in the same field as source names, `from_name` was
given arms for them too. One function, two populations — and the compiler's
private vocabulary became reachable by typing it.

Nothing ever needed that direction. Every backend dispatches on `kind`, which
is precisely why journal 379 found all nine arms cold. **The string is
write-only.**

So the nine arms came out of `from_name` instead of being fenced off with a
diagnostic. A `__`-prefixed name typed by a user is now `BuiltinCmd::Other`, an
ordinary SB5011, exactly like `__anything_else`. `cov.327`-`cov.330` pass
unchanged, because the behaviour they pin — the user's definition wins, a bare
one is unknown — is now true *by construction* rather than by precedence.

Journal 379 asked "can I reach the line" and then "what happens when someone
does". It never asked **why the line is there**. That third question is the one
that deleted the problem; the first two only characterised it.

## Fallout 4 — a spec that had stopped testing its subject

`25.6` claims to test the runtime SUB-overflow trap:

```sh
fn sub { int a=$(($1)); int b=$(($2)); (( c = a - b )); echo $c; }
sub -9000000000000000000 1000000000000000000
```

`sub` is a value intrinsic, so the *call* is SB5073 at compile time. The
assertion was `rc != 0 && -z $out` — which a compile error satisfies. DD#25's
SUB trap had no coverage, and had not had any since the intrinsic was added.

This predates DD#125 entirely; reservation only surfaced it by rejecting the
definition as well. Renamed `subtract`, assertion tightened with `-z $err`, and
the trap fires for the first time: `arithmetic overflow`, rc=2.

**The pattern worth keeping:** an assertion of `rc != 0` with no claim about
*which* error is a spec that will quietly accept any future error. Three of the
six specs this session broke were of that shape.

## The audit that found it

Grepping for reserved names produced noise (`int array a=` looks like a
declaration of `array`). What worked was instrumenting `chk` to log any spec
whose stderr contained SB5092, then running every file — three distinct
programs, all genuine. The `fn sub` case did **not** appear, because it fails
earlier with SB5073; that absence is what exposed fallout 4.

Measure the suite by running it, not by pattern-matching its source.

## Numbers

Sweep **7847** (7834 passing + 13 skipped), predicted 7847 before running,
0 failing. 12 flips, all `failing → passing` — the six specs broken mid-session
and then fixed. 30 new ids, predicted 30. Gate self-test 248. Units 85/27/20.
