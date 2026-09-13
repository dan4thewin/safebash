# 290 — Six characters past the end

`a{1,17}` crashed the compiler.

```
sash: malloc.c:2379: sysmalloc: Assertion `(old_top == initial_top (av) && old_size == 0) || ...' failed.
```

Not a diagnostic. Not an ICE with a message. A glibc assertion, which
means the heap metadata was already corrupt by the time anything noticed.
`a{1,16}` was fine. The threshold was exactly there.

I was not looking for this. The task was `docs-regex-dialect-writeup` —
write a `.SH REGULAR EXPRESSIONS` section for `sash.1`, with the gate
"each claim verified against the binary, not against DD text". So I was
walking the dialect construct by construct, typing bounds at it to find
out what the real cap was. That is the whole story of how it was found:
the documentation task's gate is an adversarial-input generator wearing
a modest hat.

## The arithmetic

`regcomp1` sizes its instruction arena like this:

```c
pp = malloc(sizeof(Reprog) + 6*sizeof(Reinst)*strlen(s));
```

Six instructions per character of pattern. That is Pike's budget and it
is a good one — for Pike's operators. Every construct he wrote consumes
at least one character and emits a bounded number of instructions, so the
per-character budget is an invariant maintained by construction. You can
read the whole compiler and never see it checked, because it never needs
checking.

`{n,m}` is ours. DD#39 added it. It does not consume characters in
proportion to what it emits — it consumes six (`a{1,17}`) and emits
seventeen copies of the preceding atom. `parsecounted` caps both bounds
at 32, so the worst case per `{` is 32 copies, each costing an
`operand()` plus an `operator()` that may emit two more. The budget was
never re-derived when the feature was added. There is no version of
reading `regcomp.c` that would show you this, because the invariant it
breaks isn't written down anywhere in the file; it's an emergent property
of the operator set, and we changed the operator set.

## The part that made it serious

A compiler crash is bad. I assumed that was the extent of it — `sash`
links `regcomp.c` via `build.rs` to validate patterns at compile time, so
a bad pattern in your source kills the build, loudly, and you fix your
source.

Then I checked whether a pattern has to be a literal.

```bash
p=$(getenv RE)
attest p [[ -n $p ]]
[[ $s =~ $p ]]
```

That compiles. `sash-rt` ships the same `regcomp.c`, and it compiles the
pattern at runtime, from the environment.

So I ran it. `RE='a{1,17}'` against thirty-four `a`s printed `N`.

Not a crash. An answer. A wrong one, with no indication anywhere that
anything had gone wrong. `sash-rt` allocates from its own `sash_malloc`
heap rather than glibc's, so the overflow ran off the end of the
allocation and into whatever was adjacent, and the matcher went on
matching against partly-overwritten instructions. Larger bounds —
`a{2,32}`, `a{32}{32}` — killed the process outright with no output at
all.

Environment-controlled input, silently wrong answers or process death.
That is the whole shape of a memory-safety bug in a security-relevant
position, and it was reachable from a documentation task.

## Three fixes

**The sizing.** `nslots = 6*strlen(s) + 8`, plus 128 slots for every `{`
in the pattern. Generous, and deliberately so.

**The guard.** `newinst` now checks `freep >= freeend` and calls
`rcerror`, which `longjmp`s out. This is the fix that actually matters.
The sizing arithmetic is a performance decision now, not a correctness
one: if I got the 128 wrong, or if someone adds another expanding
construct in two years and doesn't re-derive the budget either, the
failure is a diagnostic that says "expression too complex" instead of a
corrupted heap. The invariant that was implicit is now checked. That is
the difference between "we fixed this bug" and "we fixed this class".

**The build.** While testing the fix, the first rebuild still crashed,
identically. I had changed `regcomp.c` and nothing had noticed.
`sash/build.rs` compiles `../sash-rt/csrc/regcomp.c` — out of package, and
`cc-rs`'s automatic `rerun-if-changed` did not cover it. It was linking a
stale object. `sash-rt/build.rs` already had the explicit loop, with a
comment recording that a DD#63 `Reprog.nsub` incident had cost someone the
same debug cycle. The comment was right there in the sibling file. The
fix is four lines, now in both.

A stale-object bug costs you a debug cycle, which is annoying. What it
actually costs is worse than that: for ten minutes I believed a correct
fix was wrong and started looking for a second bug.

## The one I found on the way

Fixing the sizing let `(ab){2}` compile instead of crash. It returned `N`.
Bash returns `Y`.

In the main loop, `prev_atom_start` is only assigned when the token isn't
an operator. `(` and `)` are both operators. So after `(ab)`, the "previous
atom" is `b`, and the expansion loop — which re-lexes exactly one token per
copy — built `(ab)b`.

This one had been shipping. Not crashing, not warning: answering, plausibly,
`N`. I rejected it with a diagnostic rather than fixing it, because making
it work means replaying a token *span*, which changes what the language
accepts, and that is DG's call. `notes/tasks/regex-counted-group.md` lays
out both options. `(ab)(ab)` works today.

## Twelve specs

`39.20`–`39.31`. The bounds that used to crash the compiler, the bounds
that used to corrupt the runtime heap, the runtime-pattern path itself
(`getenv` → `=~`), the group rejection, and — importantly — the
constructs that were *never* affected (`[ab]{4}`, `a*{2}`), so that a
future implementation of counted groups can't quietly change them.

`39.27` is the runtime one, and it passed on the first try for the wrong
reason: `RE` was still exported in my probe shell. `env -u RE bash
regex.sh` failed it. The spec now exports `RE` itself and unsets it after.
A test that passes because of your shell's state is not a test, and the
only reason I checked is that a runtime-environment test is exactly the
kind that would do that.

## What the tally looks like

Running total of things found this session by the same mechanism — run
the thing, look at the answer, don't trust that it reads correctly: a pid
of `0`; `$(( 1 + ))` folding to `1`; `$(argc)` reading like a call and
printing `-1`; `$(argv)` reading like "the arguments" and iterating one
too many; `echo "$a$a"` losing a space; and now `a{1,17}` answering `N`.

Every one of them looked fine. That is not a coincidence, it's a
selection effect — the ones that looked wrong were fixed years ago. What's
left in a mature codebase is exactly the population that survives reading.
Which is the argument for the documentation gate that started this: "verify
each claim against the binary" is not pedantry about docs. It is the only
procedure that doesn't share the bug's blind spot.

Sweep: 4376 passing, 9 skipped, 0 failing, 0 flips.
