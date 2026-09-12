---
title: DD Candidates
modified: 2026-05-24
---

# DD Candidates

Design decisions made during the parser migration (2026-05-24).
Cross-referenced against existing DDs below.

---

## NEEDS FORMAL DD: Compound-command postfix operators

No existing DD covers this. New restriction.

**Decision**: All postfix operators on compound commands rejected
(SB4021-4024): redirects, pipes, &&, ||, &.

**Sash idiom**: Wrap in a function for composition.

**Converter task**: `notes/tasks/converter-compound-postfix.md`

---

## CONTRADICTS DD#01-09: Heredocs removed entirely

DD#01-09 item 5 says "Reject $(cmd) in heredoc body" — implying
heredocs exist but can't contain command substitution.

**Our decision**: Heredocs (`<<` / `<<-`) removed from the language
entirely (SB5002). Stricter than the original DD.

**Rationale**: Heredocs complicate parsing, encourage inline
untrusted data, and the safe alternative (herestrings, variables)
is sufficient.

**Action**: Update DD#01-09 item 5 to reflect full heredoc removal.

---

## NEEDS FORMAL DD: Backslash literal in unquoted context

No existing DD covers this.

**Decision**: `\` in unquoted words is literal, not escape. Escape
sequences only in `"..."` and `$'...'`.

**Rationale**: Sash eliminates word splitting and restricts globbing.
Unquoted backslash escaping has no use case. Making `\` literal
allows regex patterns to pass through naturally.

---

## CLARIFIES DD#38: Braces at statement boundaries

DD#38 rejects subshells. Groups (`{ ... }`) remain for error
handler blocks (`cmd || { ... }`).

**Open question**: Are groups needed, or should all multi-statement
handlers use named functions?

---

## REFINES DD#24: Pipeline taint (exec boundary)

DD#24 says "read from any pipe" is tainted. Our implementation
refines: pipe from a sash builtin (no exec) is CLEAN. Only pipe
from a command that execs is tainted.

**Rationale**: `echo "hello" | read x` compiles to an internal
buffer (no real pipe, no exec). The data never leaves the process.
Only exec boundaries introduce external data.

**Action**: Add clarification to DD#24 that pipe taint depends on
whether the source execs, not merely that a pipe syntax is used.

---

## ALREADY IN DD#55: $? state machine

DD#55 already documents: "$? is only valid as the first expression
evaluated in the handling branch." Our implementation matches.

No action needed — DD#55 is authoritative.

---

## ALREADY IN DD#24: Bless LHS rule

DD#24 already says: "$VAR must appear on the LHS of the test operator."
Our implementation matches.

No action needed — DD#24 is authoritative.

---

## NEEDS DOCUMENTATION: Builtin list

No existing DD enumerates what's a builtin vs what execs.

**Builtins** (no exec, inline): echo, true, false, :, read, mapfile,
bless, getenv, argv, argc, shift, unset, [[, trap, signal, wait,
kill, signum, cd, status, getpid, getppid, getcpid, getcwd,
epochseconds.

**NOT builtins** (resolve via PATH, exec): printf, cat, grep, sed,
awk, wc, sort, head, tail, tee, date, hostname, etc.

**Reserved words** (parsed as syntax, not commands): if, then, else,
elif, fi, while, until, do, done, for, in, case, esac, fn, function,
local, int, const, array, map, bless, signal, end, return, break,
continue.

---

## ALREADY IN DD#31: Comma expressions in conditions

DD#31 fully documents this. Our implementation matches.

No action needed.

---

## REFINES DD#39/DD#55: Runtime regex abort

DD#39 says "regcomp9 failure prints error to stderr."
DD#55 says "runtime calls that can fail on data" are checked.

**Our refinement**: regcomp9 failure now exits with code 2 (hard
abort), consistent with DD#55's error model. Previously it printed
the error but returned "no match" and continued.

**Action**: Note in DD#39 that runtime regex failure is a hard abort.
