# 292 — Writing it down found four bugs

`docs-regex-dialect-writeup` was a documentation task. Write a section
for the man page describing sash's regex dialect. The source material was
already listed in the filing: two DDs and a diff against upstream Plan 9.
An hour's transcription, on the face of it.

It had one unusual gate:

> each claim is verified against the binary, not against DD text

That gate is the whole story. Transcribing the DDs would have produced a
section that was wrong in at least four places, and — much worse — would
have left four defects in the compiler, two of them memory-safety or
data-corruption bugs, sitting exactly where the documentation said
everything was fine.

## The four

**The `{n,m}` heap overflow** (journal 290). Typing bounds at the
compiler to find the real cap crashed it at `a{1,17}`. The same C source
ships in `sash-rt`, and a pattern can come from `$(getenv …)`, so the
overflow was reachable from the environment and produced silently wrong
answers instead of a crash.

**`(ab){2}` matching the wrong thing** (journal 290). Found only because
fixing the overflow let the pattern compile far enough to answer.

**Every non-ASCII literal double-encoded** (journal 291). Found because
`café =~ ^caf.$` matched neither the byte model nor the character model.
`echo café` had been printing `cafÃ©`.

**`[[ $s =~ ^((a))$ ]]` rejected** as "expected `]]`" — a lexer collision
with the arithmetic opener, filed rather than fixed because whether a
bare word may contain `((` is a language-surface question.

None of these is a documentation problem. All four were found by a
documentation task, because the gate turned it into a systematic sweep of
the feature surface by someone with no stake in the answer.

## Why documentation finds bugs that tests don't

A test suite asks the questions its author thought to ask. It grows by
accretion around the features people implemented and the bugs people
already hit. Its shape is the history of the project's attention.

Documentation has to answer a different question: *what does this do, in
general?* You cannot write "`.` matches any character" without asking
what happens to a character that is two bytes. You cannot write "bounds
up to 32" without trying 33, and then 17, and then finding out that the
number in the DD was never the number in the code. The obligation to make
a **complete, general** statement forces you across the whole surface,
including the parts nobody had a reason to visit.

Four of the section's claims came out different from what the DDs say.
DD#39's top section still denies that `{n,m}` exists. The task's own
source-material note lists two modified functions in `regcomp.c` and
omits `lib9.h` entirely — and `lib9.h`'s eight-line `chartorune` stub is
*the* reason the dialect is byte-oriented, which is the single most
user-visible divergence in the finished section.

## What the section says

Plan 9's engine, chosen deliberately, Thompson NFA, POSIX compliance
explicitly not a goal. The additions (`{n,m}`, `\d \w \s` and
complements, `\xNN`). Then the divergences, which is the part that earns
its keep: byte-oriented matching, no POSIX classes, no backreferences, no
lookaround, no `\b`, `\-` for a literal hyphen in a bracket, no counted
group repetition, no empty alternative, no case folding.

One of those divergences gets a sentence the others don't. Every
unsupported construct in that list is *rejected* — you write it, you get
SB5023, you stop. Except `[[:alpha:]]`, which is read as an ordinary
bracket expression followed by the literal text `:]`, and therefore
quietly fails to match. It is the only item on the list that will not
tell you. That sentence is in the man page because writing the list made
the asymmetry impossible not to notice.

## The gate is the artifact

The section is maybe eighty lines of roff. The gate that produced it is
one line, and it is the reusable part. "Verify each claim against the
binary, not against the design notes" converts writing prose into an
audit, performed by someone reading the feature as a stranger would,
which is a perspective no amount of familiarity with the code supplies.

DG wrote that gate into the filing. It found four bugs.
