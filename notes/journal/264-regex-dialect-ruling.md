# 264 — A bug filing with a wrong premise

`bug-regex-literal-dash-in-bracket` was filed P1 with a measured table, a
located cause, and a three-way fork for DG. It was wrong at the root, and DG
dissolved it in four sentences:

> the regex library is chosen on faith in the author, Rob Pike. It represents
> his view of a sane pre-feature-creep implementation. POSIX compliance is not
> a goal. dash can be used in a character class with a `\` (I think). I have
> made small changes for convenience, not compliance. It deserves a full
> write-up in the future docs/man page

The filing's reasoning was: POSIX ERE says a first-or-last `-` in a bracket
expression is a literal; bash agrees; sash rejects it; therefore sash has a
bug, and the only question is where to fix it. Every step follows from the
premise that **POSIX and bash define what sash's regex should accept**. They
don't. The engine is a deliberate choice of a particular author's judgement
about what a regex implementation should be, and its restraint is the point,
not an approximation to be corrected.

## What I should have checked

The filing offered option 3 as "document the divergence and reject with a
message that teaches the workaround (`[\-…]`)" — so the escape was *suspected*
and never tested. Testing it takes one command, and it works in all three
positions, agreeing with bash's result on the same subject:

    ^[A-Za-z\-]+$   matches a-b     (bash: same)
    ^[\-A-Za-z]+$   matches -ab     (bash: same)
    ^[a-z_\-]+$     matches a_b-c   (bash: same)

Which falsifies the filing's most serious claim — that sash "accepts a
strictly smaller regex language than bash while claiming byte-identical
behaviour." It doesn't. The same character classes are expressible; they are
spelled with an escape, exactly as `{` and `[` already are under DD#39's
2026-07-08 ruling. A P1 with a three-way fork was really one backslash.

The lesson is narrow and mechanical: **when a filing proposes "document the
workaround" as an option, test the workaround first.** If it works, the filing
is a documentation task, not a bug, and the fork evaporates.

## Two stale claims, found by confirming

DD#39 says, in its opening section, that the engine does not support `{n,m}`
or `\d`/`\w`. The 2026-07-08 clarification corrected the `{n,m}` half and
explicitly restated the other: "`\d`/`\w`/lookaround remain unsupported."

Both halves were stale. The vendored `regcomp.c` has a `bldshortclass`
addition providing `\d` `\w` `\s` and the negated `\D` `\W` `\S` — verified
against the binary, not read off the source. `{n,m}` works and is capped at 32
by `parsecounted`. Only lookaround is genuinely absent.

So DD#39's prose had drifted from the implementation twice, in the same
paragraph, across two separate corrections. That is what happens to a claim
nothing executes. The dialect now has specs — `wordsplit.sh` 39.12–39.16 pin
the escape, the deliberate rejection, all six shorthand classes, and the
repetition cap — so the next drift fails a test instead of misleading a
reader.

## What is actually vendored

Worth knowing for the write-up DG asked for: `sash-rt/csrc/regexec.c` and
`regaux.c` are **byte-identical** to `contrib/plan9port-regexp/`. Every local
change is in `regcomp.c`, about 223 lines, and both additions are to the
parser. The matcher — the part where a subtle change would produce silently
wrong answers — is untouched upstream code. "Small changes for convenience"
is precisely accurate.

## Result

No code change. The posture is a clarification in `DD39-glob-regex.md`, which
also corrects the two stale claims. `bug-regex-literal-dash-in-bracket` is
closed as not-a-bug with DG's ruling quoted in it. The write-up DG asked for
is filed as `docs-regex-dialect-writeup`, with a caution not to transcribe the
DDs — they were wrong twice — but to verify each claim against the compiler.

Sweep 4833 / 39 / 482, zero non-janet failures, zero regressions.
