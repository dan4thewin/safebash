---
title: The registry stops being permanent
date: 2026-09-18
---

# The registry stops being permanent

DG picked option (c) from journal 361: when a sweep no longer produces
an id, drop it from the registry. The gate's own total is accurate
again — 5583 reported, 5583 run, matching the sum of every file's
`plan=` exactly.

The change is small. The care is in what it must not do.

## Three properties, two of them guards

A purge that deletes "every registered id not in this sweep's TSV" is
correct only for a complete, all-modes sweep. Two ways that goes wrong,
both of which end with an empty registry and a green-looking report:

**Mode scoping.** `SPEC_MODES="native"` is the documented fast-iteration
path, and its TSV contains no `c:` ids at all. Read naively, that is a
retirement of all 2697 of them. So the purge only considers ids whose
mode prefix actually appears in this sweep.

**The cap.** A partial or aborted sweep produces a short TSV, and
nothing in the TSV distinguishes "the suite stopped early" from "these
specs were deleted." There is no signal to read, so the purge refuses
past a threshold — `SPEC_PURGE_MAX`, default 25 — reporting the cohort
instead of deleting it. The number is deliberate: real retirements are
one or two specs, and the largest cohort in the project's history was
a deliberate event that should have to say so.

**History is kept.** Only the `tests` row goes. The `runs` rows stay,
because they are the cross-commit history the regression query reads,
and dropping the `tests` row is by itself enough to clear the id from
`test_status`. A re-added spec simply re-registers on its next sweep.

I dry-ran all three against a copy of the database before letting any
DELETE near the real one: the full sweep retires exactly the two
ghosts; the native-only sweep retires one and leaves 2697 `c:` rows
alone; the partial sweep wants to retire 2788 and deletes zero.

## The heredoc ate the comment

The first live run worked and printed this four times:

```
run.sh: line 51: tests: command not found
```

I had written the explanatory comment in the project's prose style,
with backticks around `tests`, `runs` and `75.99`. That comment lives
inside an *unquoted* heredoc, where a backtick is command substitution,
not quoting. Bash ran `tests`, `runs` and `75.99` as commands and
spliced the empty results into the SQL on its way to sqlite3.

It was harmless here — the damage was confined to comment text — but
the shape is not harmless. Everything in that heredoc is rewritten by
bash before sqlite3 sees it, including, if someone is unlucky, a
clause. A note to that effect now sits in the file where the next
person will write a comment.

Worth naming the pattern: the prose convention that is correct
everywhere else in this repo is a code defect in exactly this one
context. Style rules do not survive a change of interpreter.

## What the gate says now

The purge announces itself:

```
--- retired 2 id(s) the suite no longer produces ---
    c:75.99
    native:75.99
```

Which is the point. A retirement you did not expect now means a spec
vanished, and you will see it the sweep it happens rather than
discovering it as an unaccountable two in a total months later.

**Sweep 5583, 0 failing. Units 77, clippy clean.**
